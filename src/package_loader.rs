use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast;
use crate::diagnostics::Diagnostic;
use crate::parser;
use crate::span::Span;

#[derive(Debug, Clone)]
pub struct PackageLoadResult {
    pub program: Option<ast::Program>,
    pub diagnostics: Vec<Diagnostic>,
    pub module_order: Vec<Vec<String>>,
    pub parsed_module_count: usize,
    pub item_modules: Vec<Option<Vec<String>>>,
}

pub fn load_entry(input: &Path) -> anyhow::Result<PackageLoadResult> {
    let entry = resolve_entry_path(input)?;
    let project_root = find_project_root(&entry);
    let mut loader = Loader::new(project_root, entry);
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
    fn new(project_root: PathBuf, entry_path: PathBuf) -> Self {
        Self {
            project_root,
            entry_path,
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

        let (program, item_modules) = self.merge_program();
        let parsed_module_count = self.parse_cache.len();

        PackageLoadResult {
            program,
            diagnostics: self.diagnostics,
            module_order,
            parsed_module_count,
            item_modules,
        }
    }

    fn build_module_index(&mut self) -> anyhow::Result<()> {
        let mut files = Vec::new();
        collect_aic_files(&self.project_root, &mut files)?;

        for file in files {
            self.ensure_parsed(&file)?;
            let Some(parsed) = self.parse_cache.get(&file) else {
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
                        &file.to_string_lossy(),
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
            self.module_index.insert(key, file.clone());
            self.module_path_by_file.insert(file, module.path.clone());
        }

        Ok(())
    }

    fn visit_entry(&mut self) -> anyhow::Result<()> {
        self.visit_path(self.entry_path.clone(), true)
    }

    fn merge_program(&self) -> (Option<ast::Program>, Vec<Option<Vec<String>>>) {
        let Some(entry) = self.parse_cache.get(&self.entry_path) else {
            return (None, Vec::new());
        };
        let Some(entry_program) = entry.program.as_ref() else {
            return (None, Vec::new());
        };

        let mut merged_items = Vec::new();
        let mut item_modules = Vec::new();
        for path in &self.ordered {
            if let Some(parsed) = self.parse_cache.get(path) {
                if let Some(program) = &parsed.program {
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
                imports: entry_program.imports.clone(),
                items: merged_items,
                span: entry_program.span,
            }),
            item_modules,
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
        let mut roots = vec![self.project_root.join("std")];
        let builtin_std = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("std");
        if !roots.iter().any(|r| r == &builtin_std) {
            roots.push(builtin_std);
        }
        roots
    }

    fn ensure_parsed(&mut self, file: &Path) -> anyhow::Result<()> {
        if self.parse_cache.contains_key(file) {
            return Ok(());
        }
        let source = fs::read_to_string(file)?;
        let (program, diagnostics) = parser::parse(&source, &file.to_string_lossy());
        self.parse_cache.insert(
            file.to_path_buf(),
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
        let manifest = input.join("aic.toml");
        if manifest.exists() {
            if let Some(main) = manifest_main(&manifest)? {
                return Ok(input.join(main));
            }
        }
        return Ok(input.join("src/main.aic"));
    }
    Ok(input.to_path_buf())
}

fn manifest_main(path: &Path) -> anyhow::Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("main") {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                let value = value.trim();
                if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    return Ok(Some(value[1..value.len() - 1].to_string()));
                }
            }
        }
    }
    Ok(None)
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
            if name == ".git" || name == "target" {
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::load_entry;

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
}
