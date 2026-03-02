use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast;
use crate::diagnostics::Diagnostic;
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
    loader.build_module_index()?;
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

        let (program, item_modules, module_imports) = self.merge_program();
        let parsed_module_count = self.parse_cache.len();

        PackageLoadResult {
            program,
            diagnostics: self.diagnostics,
            module_order,
            parsed_module_count,
            item_modules,
            module_imports,
        }
    }

    fn build_module_index(&mut self) -> anyhow::Result<()> {
        let mut search_roots = Vec::new();
        search_roots.push(self.project_root.clone());
        search_roots.extend(self.module_roots.clone());
        search_roots.sort();
        search_roots.dedup();

        let mut files = Vec::new();
        for root in search_roots {
            let exclusions = if root == self.project_root {
                self.project_excluded_roots.as_slice()
            } else {
                &[]
            };
            let mut root_files = Vec::new();
            collect_aic_files(&root, &mut root_files)?;
            if !exclusions.is_empty() {
                root_files.retain(|file| {
                    let canonical = canonical_or_self(file.clone());
                    !path_matches_any_root(&canonical, exclusions)
                });
            }
            files.extend(root_files);
        }
        let files = files
            .into_iter()
            .map(canonical_or_self)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        for file in files {
            let canonical = file;
            self.ensure_parsed(&canonical)?;
            let Some(parsed) = self.parse_cache.get(&canonical) else {
                continue;
            };
            let Some(program) = &parsed.program else {
                continue;
            };
            let Some(module) = &program.module else {
                continue;
            };

            let key = module.path.join(".");
            if let Some(previous) = self.module_index.get(&key) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E2105",
                        format!("duplicate module declaration '{}'", key),
                        &canonical.to_string_lossy(),
                        module.span,
                    )
                    .with_help(format!(
                        "module '{}' is already declared in {}",
                        key,
                        previous.display()
                    )),
                );
                continue;
            }
            self.module_index.insert(key, canonical.clone());
            self.module_path_by_file
                .insert(canonical, module.path.clone());
        }

        Ok(())
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
    ) {
        let Some(entry) = self.parse_cache.get(&self.entry_path) else {
            return (None, Vec::new(), BTreeMap::new());
        };
        let Some(entry_program) = entry.program.as_ref() else {
            return (None, Vec::new(), BTreeMap::new());
        };

        let mut merged_items = Vec::new();
        let mut item_modules = Vec::new();
        let mut merged_imports: BTreeMap<String, ast::ImportDecl> = BTreeMap::new();
        let mut module_imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for path in &self.ordered {
            if let Some(parsed) = self.parse_cache.get(path) {
                if let Some(program) = &parsed.program {
                    let module_name = program
                        .module
                        .as_ref()
                        .map(|m| m.path.join("."))
                        .unwrap_or_else(|| ROOT_MODULE.to_string());
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
                        &canonical.to_string_lossy(),
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

        if !is_entry && program.module.is_none() {
            self.diagnostics.push(
                Diagnostic::error(
                    "E2101",
                    "non-entry module must declare `module ...;`",
                    &canonical.to_string_lossy(),
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

        if let Some(path) = self.module_index.get(&key) {
            return Ok(Some(path.clone()));
        }

        if let Some(path) = self.fallback_by_path(&import.path) {
            return Ok(Some(path));
        }

        self.diagnostics.push(
            Diagnostic::error(
                "E2100",
                format!("cannot resolve import '{}'", key),
                &current_file.to_string_lossy(),
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

        if path[0] == "std" {
            let rest = if path.len() > 1 {
                path[1..].join("/")
            } else {
                String::new()
            };
            if !rest.is_empty() {
                for std_root in self.std_roots() {
                    let candidate = std_root.join(format!("{rest}.aic"));
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
        }

        let rel = format!("{}.aic", path.join("/"));
        let direct = self.project_root.join(&rel);
        if direct.exists() {
            return Some(direct);
        }

        let under_src = self.project_root.join("src").join(&rel);
        if under_src.exists() {
            return Some(under_src);
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
        let (program, diagnostics) = parser::parse(&source, &canonical.to_string_lossy());
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

fn collect_aic_files(root: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        if path.is_dir() {
            if name == ".git" || name == "target" || name == ".aic-cache" {
                continue;
            }
            collect_aic_files(&path, out)?;
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) == Some("aic") {
            out.push(path);
        }
    }
    Ok(())
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
        .unwrap_or_else(|| path.to_string_lossy().to_string())
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
        ast::ExprKind::Break { expr } => expr
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
}
