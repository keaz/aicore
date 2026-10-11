use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast;
use crate::diagnostics::Diagnostic;
use crate::machine_paths;
use crate::package_workflow::{resolve_dependency_context, PackageOptions};
use crate::parser;
use crate::span::Span;
use crate::toolchain;

const ROOT_MODULE: &str = "<root>";

#[derive(Debug, Clone)]
pub struct PackageLoadResult {
    pub program: Option<ast::Program>,
    pub diagnostics: Vec<Diagnostic>,
    pub module_order: Vec<Vec<String>>,
    pub parsed_module_count: usize,
    pub item_modules: Vec<Option<Vec<String>>>,
    pub module_imports: BTreeMap<String, BTreeSet<String>>,
    pub module_files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LoadOptions {
    pub offline: bool,
}

pub fn load_entry(input: &Path) -> anyhow::Result<PackageLoadResult> {
    load_entry_with_options(input, LoadOptions::default())
}

pub fn load_entry_with_options(
    input: &Path,
    options: LoadOptions,
) -> anyhow::Result<PackageLoadResult> {
    let entry = resolve_entry_path(input)?;
    let project_root = find_project_root(&entry);
    let dependency_context = resolve_dependency_context(
        &project_root,
        PackageOptions {
            offline: options.offline,
        },
    )?;
    let mut loader = Loader::new(
        project_root,
        entry,
        dependency_context.roots,
        dependency_context.source_roots,
    );
    loader.diagnostics.extend(dependency_context.diagnostics);
    loader.index_dependency_roots();
    loader.visit_entry()?;
    Ok(loader.finish())
}

#[derive(Debug, Clone)]
struct ParsedModule {
    program: Option<ast::Program>,
    diagnostics: Vec<Diagnostic>,
}

struct Loader {
    project_root: PathBuf,
    entry_path: PathBuf,
    module_roots: Vec<PathBuf>,
    project_excluded_roots: Vec<PathBuf>,
    parse_cache: BTreeMap<PathBuf, ParsedModule>,
    module_index: BTreeMap<String, PathBuf>,
    module_path_by_file: BTreeMap<PathBuf, Vec<String>>,
    // Later declarations of an already indexed module name. Reported as E2105
    // only when that module is actually imported or visited.
    duplicate_modules: BTreeMap<String, Vec<(PathBuf, Span)>>,
    reported_duplicates: BTreeSet<String>,
    // The project root is scanned lazily, the first time an import cannot be
    // resolved from dependencies, std, or the conventional module path.
    project_indexed: bool,
    diagnostics: Vec<Diagnostic>,
    visiting: Vec<PathBuf>,
    visited: BTreeSet<PathBuf>,
    ordered: Vec<PathBuf>,
    reported_cycles: BTreeSet<String>,
}

impl Loader {
    fn new(
        project_root: PathBuf,
        entry_path: PathBuf,
        module_roots: Vec<PathBuf>,
        project_excluded_roots: Vec<PathBuf>,
    ) -> Self {
        let mut project_excluded_roots = project_excluded_roots
            .into_iter()
            .map(canonical_or_self)
            .collect::<Vec<_>>();
        project_excluded_roots.sort();
        project_excluded_roots.dedup();

        Self {
            project_root,
            entry_path: canonical_or_self(entry_path),
            module_roots: module_roots.into_iter().map(canonical_or_self).collect(),
            project_excluded_roots,
            parse_cache: BTreeMap::new(),
            module_index: BTreeMap::new(),
            module_path_by_file: BTreeMap::new(),
            duplicate_modules: BTreeMap::new(),
            reported_duplicates: BTreeSet::new(),
            project_indexed: false,
            diagnostics: Vec::new(),
            visiting: Vec::new(),
            visited: BTreeSet::new(),
            ordered: Vec::new(),
            reported_cycles: BTreeSet::new(),
        }
    }

    fn finish(self) -> PackageLoadResult {
        let module_order = self
            .ordered
            .iter()
            .filter_map(|path| self.module_path_by_file.get(path).cloned())
            .collect::<Vec<_>>();

        let (program, item_modules, module_imports, module_files) = self.merge_program();
        let parsed_module_count = self.parse_cache.len();

        PackageLoadResult {
            program,
            diagnostics: self.diagnostics,
            module_order,
            parsed_module_count,
            item_modules,
            module_imports,
            module_files,
        }
    }

    /// Indexes the source roots of resolved package dependencies. These are
    /// bounded package trees, so they are indexed up front.
    fn index_dependency_roots(&mut self) {
        let mut roots = self.module_roots.clone();
        roots.sort();
        roots.dedup();
        for root in roots {
            self.index_root(&root, &[]);
        }
    }

    /// Indexes every module declared under the project root. Runs at most once,
    /// and only when an import needs it: checking a standalone file that imports
    /// nothing outside std never walks the directory it lives in.
    fn ensure_project_indexed(&mut self) {
        if self.project_indexed {
            return;
        }
        self.project_indexed = true;
        let root = self.project_root.clone();
        let exclusions = self.project_excluded_roots.clone();
        self.index_root(&root, &exclusions);
    }

    fn index_root(&mut self, root: &Path, exclusions: &[PathBuf]) {
        let mut files = Vec::new();
        collect_aic_files(root, &mut files);
        let files = files
            .into_iter()
            .map(canonical_or_self)
            .filter(|file| exclusions.is_empty() || !path_matches_any_root(file, exclusions))
            .collect::<BTreeSet<_>>();

        for file in files {
            // Unreadable files cannot be imported; skip them instead of failing
            // the whole load because of an unrelated file.
            if self.ensure_parsed(&file).is_err() {
                continue;
            }
            let declared = self
                .parse_cache
                .get(&file)
                .and_then(|parsed| parsed.program.as_ref())
                .and_then(|program| program.module.as_ref())
                .map(|module| (module.path.clone(), module.span));
            if let Some((module_path, span)) = declared {
                self.register_module(&module_path, &file, span);
            }
        }
    }

    fn register_module(&mut self, module_path: &[String], file: &Path, span: Span) {
        let key = module_path.join(".");
        self.module_path_by_file
            .entry(file.to_path_buf())
            .or_insert_with(|| module_path.to_vec());
        match self.module_index.get(&key) {
            None => {
                self.module_index.insert(key, file.to_path_buf());
            }
            Some(existing) if existing == file => {}
            Some(_) => {
                let duplicates = self.duplicate_modules.entry(key).or_default();
                if !duplicates.iter().any(|(path, _)| path == file) {
                    duplicates.push((file.to_path_buf(), span));
                }
            }
        }
    }

    /// Looks up an indexed module and reports E2105 once if the name is declared
    /// by more than one file.
    fn lookup_module(&mut self, key: &str) -> Option<PathBuf> {
        let path = self.module_index.get(key)?.clone();
        if let Some(duplicates) = self.duplicate_modules.get(key) {
            if self.reported_duplicates.insert(key.to_string()) {
                for (duplicate, span) in duplicates {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E2105",
                            format!("duplicate module declaration '{}'", key),
                            &machine_paths::normalize_separators_path(duplicate),
                            *span,
                        )
                        .with_help(format!(
                            "module '{}' is already declared in {}",
                            key,
                            path.display()
                        )),
                    );
                }
            }
        }
        Some(path)
    }

    fn visit_entry(&mut self) -> anyhow::Result<()> {
        self.visit_path(self.entry_path.clone(), true)
    }

    fn merge_program(
        &self,
    ) -> (
        Option<ast::Program>,
        Vec<Option<Vec<String>>>,
        BTreeMap<String, BTreeSet<String>>,
        BTreeMap<String, String>,
    ) {
        let Some(entry) = self.parse_cache.get(&self.entry_path) else {
            return (None, Vec::new(), BTreeMap::new(), BTreeMap::new());
        };
        let Some(entry_program) = entry.program.as_ref() else {
            return (None, Vec::new(), BTreeMap::new(), BTreeMap::new());
        };

        let mut merged_items = Vec::new();
        let mut item_modules = Vec::new();
        let mut merged_imports: BTreeMap<String, ast::ImportDecl> = BTreeMap::new();
        let mut module_imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut module_files: BTreeMap<String, String> = BTreeMap::new();
        for path in &self.ordered {
            if let Some(parsed) = self.parse_cache.get(path) {
                if let Some(program) = &parsed.program {
                    let module_name = program
                        .module
                        .as_ref()
                        .map(|m| m.path.join("."))
                        .unwrap_or_else(|| ROOT_MODULE.to_string());
                    module_files
                        .entry(module_name.clone())
                        .or_insert_with(|| machine_paths::normalize_separators_path(path));
                    let imports_for_module = module_imports.entry(module_name).or_default();
                    for import in &program.imports {
                        merged_imports
                            .entry(import.path.join("."))
                            .or_insert_with(|| import.clone());
                        imports_for_module.insert(import.path.join("."));
                    }
                    merged_items.extend(program.items.clone());
                    for _ in &program.items {
                        item_modules.push(program.module.as_ref().map(|m| m.path.clone()));
                    }
                }
            }
        }

        (
            Some(ast::Program {
                module: entry_program.module.clone(),
                imports: merged_imports.into_values().collect(),
                items: merged_items,
                span: entry_program.span,
            }),
            item_modules,
            module_imports,
            module_files,
        )
    }

    fn visit_path(&mut self, path: PathBuf, is_entry: bool) -> anyhow::Result<()> {
        let canonical = canonical_or_self(path);

        if self.visited.contains(&canonical) {
            return Ok(());
        }

        if let Some(pos) = self.visiting.iter().position(|p| p == &canonical) {
            let cycle = self.visiting[pos..]
                .iter()
                .chain(std::iter::once(&canonical))
                .map(|p| module_label(self.module_path_by_file.get(p), p))
                .collect::<Vec<_>>();
            let (cycle_key, cycle_text) = canonicalize_cycle(&cycle);
            if self.reported_cycles.insert(cycle_key) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E2103",
                        format!("import cycle detected: {cycle_text}"),
                        &machine_paths::normalize_separators_path(&canonical),
                        Span::new(0, 0),
                    )
                    .with_help("break the cycle by extracting shared code into a separate module"),
                );
            }
            return Ok(());
        }

        self.ensure_parsed(&canonical)?;
        let Some(parsed) = self.parse_cache.get(&canonical).cloned() else {
            return Ok(());
        };
        self.diagnostics.extend(parsed.diagnostics.clone());

        let Some(program) = parsed.program else {
            self.visited.insert(canonical);
            return Ok(());
        };

        if let Some(module) = &program.module {
            // Loaded files never count as duplicates: the entry may be a copy of
            // an indexed module (for example a test harness copy) and shadows it.
            self.module_path_by_file
                .entry(canonical.clone())
                .or_insert_with(|| module.path.clone());
            self.module_index
                .entry(module.path.join("."))
                .or_insert_with(|| canonical.clone());
        }

        if !is_entry && program.module.is_none() {
            self.diagnostics.push(
                Diagnostic::error(
                    "E2101",
                    "non-entry module must declare `module ...;`",
                    &machine_paths::normalize_separators_path(&canonical),
                    program.span,
                )
                .with_help("add a module declaration matching the import path"),
            );
        }

        self.visiting.push(canonical.clone());

        let mut imports = program
            .imports
            .iter()
            .map(|i| (i.path.join("."), i.clone()))
            .collect::<Vec<_>>();
        let module_is_std = program
            .module
            .as_ref()
            .and_then(|module| module.path.first())
            .map(|root| root == "std")
            .unwrap_or(false);
        if !module_is_std
            && program_uses_compiler_iterator_helpers(&program)
            && !imports.iter().any(|(key, _)| key == "std.iterator")
        {
            imports.push((
                "std.iterator".to_string(),
                ast::ImportDecl {
                    path: vec!["std".to_string(), "iterator".to_string()],
                    span: program.span,
                },
            ));
        }
        imports.sort_by(|a, b| a.0.cmp(&b.0));

        for (_, import) in imports {
            if let Some(target) = self.resolve_import(&canonical, &import)? {
                self.visit_path(target, false)?;
            }
        }

        self.visiting.pop();
        self.visited.insert(canonical.clone());
        self.ordered.push(canonical);

        Ok(())
    }

    fn resolve_import(
        &mut self,
        current_file: &Path,
        import: &ast::ImportDecl,
    ) -> anyhow::Result<Option<PathBuf>> {
        let key = import.path.join(".");

        if let Some(path) = self.lookup_module(&key) {
            return Ok(Some(path));
        }

        let is_std = import.path.first().map(String::as_str) == Some("std");
        if is_std {
            if let Some(path) = self.std_fallback(&import.path) {
                return Ok(Some(path));
            }
        } else if let Some(path) = self.conventional_module_file(&import.path) {
            return Ok(Some(path));
        }

        self.ensure_project_indexed();
        if let Some(path) = self.lookup_module(&key) {
            return Ok(Some(path));
        }

        if let Some(path) = self.fallback_by_path(&import.path) {
            return Ok(Some(path));
        }

        self.diagnostics.push(
            Diagnostic::error(
                "E2100",
                format!("cannot resolve import '{}'", key),
                &machine_paths::normalize_separators_path(current_file),
                import.span,
            )
            .with_help("create the module file and declaration, or fix the import path"),
        );
        Ok(None)
    }

    fn fallback_by_path(&self, path: &[String]) -> Option<PathBuf> {
        if path.is_empty() {
            return None;
        }

        if let Some(candidate) = self.std_fallback(path) {
            return Some(candidate);
        }

        self.conventional_candidates(path)
            .into_iter()
            .find(|candidate| candidate.exists())
    }

    fn std_fallback(&self, path: &[String]) -> Option<PathBuf> {
        if path.len() < 2 || path[0] != "std" {
            return None;
        }
        let rest = path[1..].join("/");
        self.std_roots()
            .into_iter()
            .map(|std_root| std_root.join(format!("{rest}.aic")))
            .find(|candidate| candidate.exists())
    }

    fn conventional_candidates(&self, path: &[String]) -> Vec<PathBuf> {
        let rel = format!("{}.aic", path.join("/"));
        vec![
            self.project_root.join(&rel),
            self.project_root.join("src").join(&rel),
        ]
    }

    /// Resolves `import a.b;` to `a/b.aic` or `src/a/b.aic` under the project
    /// root without scanning the project, when that file declares `module a.b;`.
    fn conventional_module_file(&mut self, path: &[String]) -> Option<PathBuf> {
        for candidate in self.conventional_candidates(path) {
            if !candidate.is_file() {
                continue;
            }
            let canonical = canonical_or_self(candidate);
            if self.ensure_parsed(&canonical).is_err() {
                continue;
            }
            let declared = self
                .parse_cache
                .get(&canonical)
                .and_then(|parsed| parsed.program.as_ref())
                .and_then(|program| program.module.as_ref())
                .filter(|module| module.path.as_slice() == path)
                .map(|module| module.span);
            if let Some(span) = declared {
                self.register_module(path, &canonical, span);
                return Some(canonical);
            }
        }
        None
    }

    fn std_roots(&self) -> Vec<PathBuf> {
        toolchain::std_import_roots(&self.project_root)
    }

    fn ensure_parsed(&mut self, file: &Path) -> anyhow::Result<()> {
        let canonical = canonical_or_self(file.to_path_buf());
        if self.parse_cache.contains_key(&canonical) {
            return Ok(());
        }
        let source = fs::read_to_string(&canonical)?;
        let parse_label = machine_paths::normalize_separators_path(&canonical);
        let (program, diagnostics) = parser::parse(&source, &parse_label);
        self.parse_cache.insert(
            canonical,
            ParsedModule {
                program,
                diagnostics,
            },
        );
        Ok(())
    }
}

fn resolve_entry_path(input: &Path) -> anyhow::Result<PathBuf> {
    if input.is_dir() {
        if let Some(manifest) = crate::package_workflow::read_manifest(input)? {
            return Ok(input.join(manifest.main));
        }
        return Ok(input.join("src/main.aic"));
    }
    Ok(input.to_path_buf())
}

fn find_project_root(entry: &Path) -> PathBuf {
    let mut dir = entry
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    loop {
        if dir.join("aic.toml").exists() {
            return dir;
        }
        let Some(parent) = dir.parent() else {
            return entry
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
        };
        dir = parent.to_path_buf();
    }
}

/// Recursively collects `.aic` files under `root` for module indexing.
///
/// Hidden directories (`.git`, `.aic-cache`, ...), `target` and `node_modules`
/// are skipped. Unreadable directories are skipped instead of failing the load,
/// and symlinked directories are visited at most once so link cycles terminate.
fn collect_aic_files(root: &Path, out: &mut Vec<PathBuf>) {
    let mut visited_dirs = BTreeSet::new();
    collect_aic_files_inner(root, out, &mut visited_dirs);
}

fn collect_aic_files_inner(
    root: &Path,
    out: &mut Vec<PathBuf>,
    visited_dirs: &mut BTreeSet<PathBuf>,
) {
    if !visited_dirs.insert(canonical_or_self(root.to_path_buf())) {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if name.starts_with('.') || matches!(name.as_str(), "target" | "node_modules") {
                continue;
            }
            collect_aic_files_inner(&path, out, visited_dirs);
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) == Some("aic") {
            out.push(path);
        }
    }
}

fn path_matches_any_root(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| path == root || path.starts_with(root))
}

fn canonical_or_self(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn module_label(module: Option<&Vec<String>>, path: &Path) -> String {
    module
        .map(|m| m.join("."))
        .unwrap_or_else(|| machine_paths::normalize_separators_path(path))
}

fn canonicalize_cycle(cycle: &[String]) -> (String, String) {
    let mut nodes = cycle.to_vec();
    if nodes.len() > 1 && nodes.first() == nodes.last() {
        nodes.pop();
    }
    if nodes.is_empty() {
        return (String::new(), String::new());
    }
    if nodes.len() == 1 {
        let node = nodes[0].clone();
        return (node.clone(), format!("{node} -> {node}"));
    }

    let mut best: Option<Vec<String>> = None;
    for candidate_seq in [nodes.clone(), {
        let mut rev = nodes.clone();
        rev.reverse();
        rev
    }] {
        for start in 0..candidate_seq.len() {
            let mut rotated = candidate_seq[start..].to_vec();
            rotated.extend_from_slice(&candidate_seq[..start]);
            let replace = best
                .as_ref()
                .map(|current| rotated.join("|") < current.join("|"))
                .unwrap_or(true);
            if replace {
                best = Some(rotated);
            }
        }
    }

    let mut best = best.unwrap_or(nodes);
    let key = best.join("|");
    best.push(best[0].clone());
    let display = best.join(" -> ");
    (key, display)
}

fn program_uses_compiler_iterator_helpers(program: &ast::Program) -> bool {
    program
        .items
        .iter()
        .any(item_uses_compiler_iterator_helpers)
}

fn item_uses_compiler_iterator_helpers(item: &ast::Item) -> bool {
    match item {
        ast::Item::Function(func) => function_uses_compiler_iterator_helpers(func),
        ast::Item::Impl(impl_def) => impl_def
            .methods
            .iter()
            .any(function_uses_compiler_iterator_helpers),
        ast::Item::Trait(trait_def) => trait_def
            .methods
            .iter()
            .any(function_uses_compiler_iterator_helpers),
        ast::Item::Struct(def) => {
            def.fields.iter().any(|field| {
                field
                    .default_value
                    .as_ref()
                    .map(expr_uses_compiler_iterator_helpers)
                    .unwrap_or(false)
            }) || def
                .invariant
                .as_ref()
                .map(expr_uses_compiler_iterator_helpers)
                .unwrap_or(false)
        }
        ast::Item::Enum(_) => false,
    }
}

fn function_uses_compiler_iterator_helpers(func: &ast::Function) -> bool {
    func.requires
        .as_ref()
        .map(expr_uses_compiler_iterator_helpers)
        .unwrap_or(false)
        || func
            .ensures
            .as_ref()
            .map(expr_uses_compiler_iterator_helpers)
            .unwrap_or(false)
        || block_uses_compiler_iterator_helpers(&func.body)
}

fn block_uses_compiler_iterator_helpers(block: &ast::Block) -> bool {
    block.stmts.iter().any(stmt_uses_compiler_iterator_helpers)
        || block
            .tail
            .as_ref()
            .map(|expr| expr_uses_compiler_iterator_helpers(expr))
            .unwrap_or(false)
}

fn stmt_uses_compiler_iterator_helpers(stmt: &ast::Stmt) -> bool {
    match stmt {
        ast::Stmt::Let { expr, .. }
        | ast::Stmt::Assign { expr, .. }
        | ast::Stmt::Assert { expr, .. } => expr_uses_compiler_iterator_helpers(expr),
        ast::Stmt::Expr { expr, .. } => expr_uses_compiler_iterator_helpers(expr),
        ast::Stmt::Return { expr, .. } => expr
            .as_ref()
            .map(expr_uses_compiler_iterator_helpers)
            .unwrap_or(false),
    }
}

fn expr_uses_compiler_iterator_helpers(expr: &ast::Expr) -> bool {
    match &expr.kind {
        ast::ExprKind::Call { callee, args, .. } => {
            matches!(
                &callee.kind,
                ast::ExprKind::Var(name)
                    if name == "aic_for_into_iter" || name == "aic_for_next_iter"
            ) || expr_uses_compiler_iterator_helpers(callee)
                || args.iter().any(expr_uses_compiler_iterator_helpers)
        }
        ast::ExprKind::TemplateLiteral { args, .. } => {
            args.iter().any(expr_uses_compiler_iterator_helpers)
        }
        ast::ExprKind::Closure { body, .. } => block_uses_compiler_iterator_helpers(body),
        ast::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            expr_uses_compiler_iterator_helpers(cond)
                || block_uses_compiler_iterator_helpers(then_block)
                || block_uses_compiler_iterator_helpers(else_block)
        }
        ast::ExprKind::While { cond, body } => {
            expr_uses_compiler_iterator_helpers(cond) || block_uses_compiler_iterator_helpers(body)
        }
        ast::ExprKind::Loop { body } => block_uses_compiler_iterator_helpers(body),
        ast::ExprKind::Break { expr } | ast::ExprKind::Return { expr } => expr
            .as_ref()
            .map(|value| expr_uses_compiler_iterator_helpers(value))
            .unwrap_or(false),
        ast::ExprKind::Match { expr, arms } => {
            expr_uses_compiler_iterator_helpers(expr)
                || arms.iter().any(|arm| {
                    arm.guard
                        .as_ref()
                        .map(expr_uses_compiler_iterator_helpers)
                        .unwrap_or(false)
                        || expr_uses_compiler_iterator_helpers(&arm.body)
                })
        }
        ast::ExprKind::Binary { lhs, rhs, .. } => {
            expr_uses_compiler_iterator_helpers(lhs) || expr_uses_compiler_iterator_helpers(rhs)
        }
        ast::ExprKind::Unary { expr, .. }
        | ast::ExprKind::Borrow { expr, .. }
        | ast::ExprKind::Await { expr }
        | ast::ExprKind::Try { expr } => expr_uses_compiler_iterator_helpers(expr),
        ast::ExprKind::UnsafeBlock { block } => block_uses_compiler_iterator_helpers(block),
        ast::ExprKind::StructInit { fields, .. } => fields
            .iter()
            .any(|(_, value, _)| expr_uses_compiler_iterator_helpers(value)),
        ast::ExprKind::FieldAccess { base, .. } => expr_uses_compiler_iterator_helpers(base),
        ast::ExprKind::Int(_)
        | ast::ExprKind::Float(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::Char(_)
        | ast::ExprKind::String(_)
        | ast::ExprKind::Unit
        | ast::ExprKind::Var(_)
        | ast::ExprKind::Continue => false,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::package_workflow::generate_and_write_lockfile;

    use super::{load_entry, load_entry_with_options, LoadOptions};

    #[test]
    fn loads_multi_file_package_deterministically() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(
            root.join("aic.toml"),
            "[package]\nname = \"demo\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write manifest");

        fs::write(
            root.join("src/main.aic"),
            "module app.main;\nimport app.math;\nfn main() -> Int { add(1, 2) }\n",
        )
        .expect("write main");

        fs::write(
            root.join("src/math.aic"),
            "module app.math;\nfn add(x: Int, y: Int) -> Int { x + y }\n",
        )
        .expect("write math");

        let loaded = load_entry(&root.join("src/main.aic")).expect("load package");
        assert!(
            loaded.diagnostics.is_empty(),
            "diags={:#?}",
            loaded.diagnostics
        );
        let program = loaded.program.expect("program");
        assert!(program.items.len() >= 2);
        assert_eq!(
            loaded.module_order,
            vec![
                vec!["app".to_string(), "math".to_string()],
                vec!["app".to_string(), "main".to_string()]
            ]
        );
        assert!(loaded.parsed_module_count >= 2);
    }

    #[test]
    fn reports_missing_module_with_actionable_help() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(
            root.join("src/main.aic"),
            "module app.main;\nimport app.missing;\nfn main() -> Int { 0 }\n",
        )
        .expect("write main");

        let loaded = load_entry(&root.join("src/main.aic")).expect("load package");
        assert!(loaded.diagnostics.iter().any(|d| d.code == "E2100"));
        assert!(loaded
            .diagnostics
            .iter()
            .any(|d| d.help.iter().any(|h| h.contains("create the module file"))));
    }

    #[test]
    fn reports_single_canonical_cycle_diagnostic() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(
            root.join("src/main.aic"),
            r#"module app.main;
import app.a;
import app.b;

fn main() -> Int { 0 }
"#,
        )
        .expect("write main");

        fs::write(
            root.join("src/a.aic"),
            r#"module app.a;
import app.b;

fn a() -> Int { 1 }
"#,
        )
        .expect("write a");

        fs::write(
            root.join("src/b.aic"),
            r#"module app.b;
import app.a;

fn b() -> Int { 2 }
"#,
        )
        .expect("write b");

        let loaded = load_entry(&root.join("src/main.aic")).expect("load package");
        let cycle_diags = loaded
            .diagnostics
            .iter()
            .filter(|d| d.code == "E2103")
            .collect::<Vec<_>>();
        assert_eq!(cycle_diags.len(), 1, "diags={:#?}", loaded.diagnostics);
        assert!(cycle_diags[0].message.contains("app.a -> app.b -> app.a"));
    }

    #[test]
    fn offline_load_uses_cache_without_duplicate_modules() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::create_dir_all(root.join("deps/util/src")).expect("mkdir dep src");

        fs::write(
            root.join("aic.toml"),
            r#"[package]
name = "app"
main = "src/main.aic"

[dependencies]
util = { path = "deps/util" }
"#,
        )
        .expect("write app manifest");

        fs::write(
            root.join("src/main.aic"),
            "module app.main;\nimport util.math;\nfn main() -> Int { util_answer() }\n",
        )
        .expect("write app main");

        fs::write(
            root.join("deps/util/aic.toml"),
            r#"[package]
name = "util"
main = "src/math.aic"
"#,
        )
        .expect("write dep manifest");

        fs::write(
            root.join("deps/util/src/math.aic"),
            "module util.math;\nfn util_answer() -> Int { 42 }\n",
        )
        .expect("write dep source");

        generate_and_write_lockfile(root).expect("write lockfile");

        let loaded = load_entry_with_options(root, LoadOptions { offline: true }).expect("load");
        assert!(
            loaded.diagnostics.is_empty(),
            "unexpected diagnostics: {:#?}",
            loaded.diagnostics
        );
    }

    fn codes(loaded: &super::PackageLoadResult) -> Vec<String> {
        loaded.diagnostics.iter().map(|d| d.code.clone()).collect()
    }

    /// Siblings that would fail the load if they were parsed and indexed.
    fn write_hostile_siblings(root: &std::path::Path, count: usize) {
        fs::create_dir_all(root.join("scratch/nested")).expect("mkdir scratch");
        fs::write(root.join("broken.aic"), "trait T { fn f( ").expect("write broken");
        fs::write(
            root.join("dup_a.aic"),
            "module app.dup;\nfn a() -> Int { 1 }\n",
        )
        .expect("write dup a");
        fs::write(
            root.join("dup_b.aic"),
            "module app.dup;\nfn b() -> Int { 2 }\n",
        )
        .expect("write dup b");
        fs::write(root.join("not_utf8.aic"), [0xff_u8, 0xfe, 0x00, 0x41]).expect("write bytes");
        for idx in 0..count {
            fs::write(
                root.join(format!("scratch/nested/p{idx}.aic")),
                format!("module scratch.p{idx};\nfn f{idx}() -> Int {{ {idx} }}\n"),
            )
            .expect("write sibling");
        }
    }

    #[test]
    fn standalone_file_does_not_parse_unrelated_siblings() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        write_hostile_siblings(root, 200);
        fs::write(root.join("one.aic"), "fn main() -> Int { 0 }\n").expect("write entry");

        let loaded = load_entry(&root.join("one.aic")).expect("load");
        assert!(
            loaded.diagnostics.is_empty(),
            "diags={:#?}",
            loaded.diagnostics
        );
        assert_eq!(loaded.parsed_module_count, 1);
    }

    #[test]
    fn standalone_file_with_std_import_does_not_index_project() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        write_hostile_siblings(root, 50);
        fs::write(
            root.join("io.aic"),
            "import std.io;\nfn main() -> Int effects { io } capabilities { io } { print_int(1); 0 }\n",
        )
        .expect("write entry");

        let loaded = load_entry(&root.join("io.aic")).expect("load");
        assert!(
            loaded.diagnostics.is_empty(),
            "diags={:#?}",
            loaded.diagnostics
        );
        let sibling_parsed = loaded
            .module_files
            .values()
            .any(|file| file.contains("/scratch/") || file.contains("dup_"));
        assert!(!sibling_parsed, "files={:#?}", loaded.module_files);
        assert!(
            loaded.parsed_module_count < 50,
            "parsed {} modules",
            loaded.parsed_module_count
        );
    }

    #[test]
    fn conventional_module_path_resolves_without_project_scan() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        write_hostile_siblings(root, 50);
        fs::create_dir_all(root.join("app")).expect("mkdir app");
        fs::write(
            root.join("app/util.aic"),
            "module app.util;\nfn answer() -> Int { 42 }\n",
        )
        .expect("write util");
        fs::write(
            root.join("main.aic"),
            "module app.main;\nimport app.util;\nfn main() -> Int { answer() }\n",
        )
        .expect("write main");

        let loaded = load_entry(&root.join("main.aic")).expect("load");
        assert!(
            loaded.diagnostics.is_empty(),
            "diags={:#?}",
            loaded.diagnostics
        );
        assert_eq!(loaded.parsed_module_count, 2);
        assert_eq!(
            loaded.module_order,
            vec![
                vec!["app".to_string(), "util".to_string()],
                vec!["app".to_string(), "main".to_string()]
            ]
        );
    }

    #[test]
    fn non_conventional_layout_resolves_through_project_index() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        write_hostile_siblings(root, 10);
        fs::create_dir_all(root.join("lib/helpers")).expect("mkdir lib");
        fs::write(
            root.join("lib/helpers/numbers.aic"),
            "module app.util;\nfn answer() -> Int { 42 }\n",
        )
        .expect("write util");
        fs::write(
            root.join("main.aic"),
            "module app.main;\nimport app.util;\nfn main() -> Int { answer() }\n",
        )
        .expect("write main");

        let loaded = load_entry(&root.join("main.aic")).expect("load");
        // The scan parses siblings, but their errors (syntax, duplicate
        // `app.dup`, invalid UTF-8) belong to modules nobody imports.
        assert!(
            loaded.diagnostics.is_empty(),
            "diags={:#?}",
            loaded.diagnostics
        );
        assert_eq!(
            loaded.module_order,
            vec![
                vec!["app".to_string(), "util".to_string()],
                vec!["app".to_string(), "main".to_string()]
            ]
        );
    }

    #[test]
    fn duplicate_module_is_reported_once_when_imported() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        write_hostile_siblings(root, 0);
        fs::write(
            root.join("main.aic"),
            "module app.main;\nimport app.dup;\nfn main() -> Int { a() }\n",
        )
        .expect("write main");
        fs::write(
            root.join("other.aic"),
            "module app.other;\nimport app.dup;\nfn other() -> Int { 0 }\n",
        )
        .expect("write other");

        let loaded = load_entry(&root.join("main.aic")).expect("load");
        assert_eq!(
            codes(&loaded),
            vec!["E2105"],
            "diags={:#?}",
            loaded.diagnostics
        );
        let diag = &loaded.diagnostics[0];
        assert!(diag.message.contains("app.dup"), "diag={diag:#?}");
        assert!(
            diag.spans[0].file.ends_with("dup_b.aic"),
            "duplicate should point at the later declaration: {diag:#?}"
        );
    }

    #[test]
    fn entry_module_shadows_other_declarations_of_its_name() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        // `aic test` checks a copy of a test file that declares the same module
        // as the original; the entry must not be reported as a duplicate.
        fs::create_dir_all(root.join(".aic-cache/harness")).expect("mkdir cache");
        fs::write(
            root.join("aic.toml"),
            "[package]\nname = \"demo\"\nmain = \"copy.aic\"\n",
        )
        .expect("write manifest");
        fs::write(
            root.join("copy.aic"),
            "module app.main;\nfn copy() -> Int { 0 }\n",
        )
        .expect("write original");
        fs::write(
            root.join("util.aic"),
            "module helpers.util;\nfn one() -> Int { 1 }\n",
        )
        .expect("write util");
        let entry = root.join(".aic-cache/harness/main.aic");
        fs::write(
            &entry,
            "module app.main;\nimport helpers.util;\nfn main() -> Int { one() }\n",
        )
        .expect("write entry");

        let loaded = load_entry(&entry).expect("load");
        assert!(
            loaded.diagnostics.is_empty(),
            "diags={:#?}",
            loaded.diagnostics
        );
        assert_eq!(
            loaded.module_order,
            vec![
                vec!["helpers".to_string(), "util".to_string()],
                vec!["app".to_string(), "main".to_string()]
            ]
        );
    }

    #[test]
    fn unresolved_import_still_reports_e2100_after_project_scan() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        write_hostile_siblings(root, 5);
        fs::write(
            root.join("main.aic"),
            "module app.main;\nimport app.nowhere;\nfn main() -> Int { 0 }\n",
        )
        .expect("write main");

        let loaded = load_entry(&root.join("main.aic")).expect("load");
        assert_eq!(
            codes(&loaded),
            vec!["E2100"],
            "diags={:#?}",
            loaded.diagnostics
        );
    }

    #[cfg(unix)]
    #[test]
    fn project_scan_skips_unreadable_directories_and_symlink_cycles() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("locked")).expect("mkdir locked");
        fs::write(root.join("locked/x.aic"), "module locked.x;\n").expect("write locked");
        fs::create_dir_all(root.join("lib")).expect("mkdir lib");
        std::os::unix::fs::symlink(root, root.join("lib/loop")).expect("symlink loop");
        fs::write(
            root.join("lib/numbers.aic"),
            "module app.util;\nfn answer() -> Int { 42 }\n",
        )
        .expect("write util");
        fs::write(
            root.join("main.aic"),
            "module app.main;\nimport app.util;\nfn main() -> Int { answer() }\n",
        )
        .expect("write main");
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000))
            .expect("lock dir");

        let loaded = load_entry(&root.join("main.aic"));
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o755))
            .expect("unlock dir");
        let loaded = loaded.expect("load must not fail on an unreadable sibling directory");
        assert!(
            loaded.diagnostics.is_empty(),
            "diags={:#?}",
            loaded.diagnostics
        );
        assert_eq!(loaded.module_order.len(), 2);
    }
}
