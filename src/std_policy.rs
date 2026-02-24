use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ast;
use crate::parser;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeprecatedApi {
    pub module: &'static str,
    pub symbol: &'static str,
    pub replacement: &'static str,
    pub since: &'static str,
    pub note: &'static str,
}

pub static DEPRECATED_APIS: &[DeprecatedApi] = &[DeprecatedApi {
    module: "std.time",
    symbol: "now",
    replacement: "std.time.now_ms",
    since: "0.1.0",
    note: "use millisecond precision API",
}];

pub fn find_deprecated_api(module: &str, symbol: &str) -> Option<&'static DeprecatedApi> {
    DEPRECATED_APIS
        .iter()
        .find(|entry| entry.module == module && entry.symbol == symbol)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StdApiSnapshot {
    pub schema_version: u32,
    pub symbols: Vec<StdApiSymbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct StdApiSymbol {
    pub module: String,
    pub kind: String,
    pub signature: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompatReport {
    pub breaking: Vec<StdApiSymbol>,
    pub additions: Vec<StdApiSymbol>,
}

pub fn collect_std_api_snapshot(std_root: &Path) -> anyhow::Result<StdApiSnapshot> {
    let mut files = Vec::new();
    collect_std_files(std_root, &mut files)?;
    files.sort();

    let mut symbols = Vec::new();
    for file in files {
        let source = fs::read_to_string(&file)?;
        let (program, diags) = parser::parse(&source, &file.to_string_lossy());
        if diags.iter().any(|d| d.is_error()) {
            let codes = diags
                .iter()
                .map(|d| format!("{}:{}", d.code, d.message))
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!("failed to parse std API file {}: {}", file.display(), codes);
        }
        let Some(program) = program else {
            continue;
        };
        let module = program
            .module
            .as_ref()
            .map(|m| m.path.join("."))
            .unwrap_or_else(|| file.to_string_lossy().to_string());

        for item in program.items {
            match item {
                ast::Item::Function(func) => symbols.push(StdApiSymbol {
                    module: module.clone(),
                    kind: "fn".to_string(),
                    signature: render_function_signature(&func),
                }),
                ast::Item::Struct(strukt) => symbols.push(StdApiSymbol {
                    module: module.clone(),
                    kind: "struct".to_string(),
                    signature: render_struct_signature(&strukt),
                }),
                ast::Item::Enum(enm) => symbols.push(StdApiSymbol {
                    module: module.clone(),
                    kind: "enum".to_string(),
                    signature: render_enum_signature(&enm),
                }),
                ast::Item::Trait(trait_def) => symbols.push(StdApiSymbol {
                    module: module.clone(),
                    kind: "trait".to_string(),
                    signature: render_trait_signature(&trait_def),
                }),
                ast::Item::Impl(impl_def) => symbols.push(StdApiSymbol {
                    module: module.clone(),
                    kind: "impl".to_string(),
                    signature: render_impl_signature(&impl_def),
                }),
            }
        }
    }

    symbols.sort();
    symbols.dedup();

    Ok(StdApiSnapshot {
        schema_version: 1,
        symbols,
    })
}

pub fn compare_snapshots(current: &StdApiSnapshot, baseline: &StdApiSnapshot) -> CompatReport {
    let current_set = current.symbols.iter().cloned().collect::<BTreeSet<_>>();
    let baseline_set = baseline.symbols.iter().cloned().collect::<BTreeSet<_>>();

    let breaking = baseline_set
        .difference(&current_set)
        .cloned()
        .collect::<Vec<_>>();
    let additions = current_set
        .difference(&baseline_set)
        .cloned()
        .collect::<Vec<_>>();

    CompatReport {
        breaking,
        additions,
    }
}

pub fn default_std_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("std")
}

fn collect_std_files(root: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_std_files(&path, out)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("aic") {
            out.push(path);
        }
    }

    Ok(())
}

fn render_function_signature(func: &ast::Function) -> String {
    let generics = render_generics(&func.generics);
    let params = func
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name, render_type(&param.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let effects = if func.effects.is_empty() {
        String::new()
    } else {
        format!(" effects {{ {} }}", func.effects.join(", "))
    };
    format!(
        "{}{}({}) -> {}{}",
        func.name,
        generics,
        params,
        render_type(&func.ret_type),
        effects
    )
}

fn render_struct_signature(strukt: &ast::StructDef) -> String {
    let generics = render_generics(&strukt.generics);
    let fields = strukt
        .fields
        .iter()
        .map(|field| format!("{}: {}", field.name, render_type(&field.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}{} {{ {} }}", strukt.name, generics, fields)
}

fn render_enum_signature(enm: &ast::EnumDef) -> String {
    let generics = render_generics(&enm.generics);
    let variants = enm
        .variants
        .iter()
        .map(|variant| match &variant.payload {
            Some(payload) => format!("{}({})", variant.name, render_type(payload)),
            None => variant.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}{} {{ {} }}", enm.name, generics, variants)
}

fn render_trait_signature(trait_def: &ast::TraitDef) -> String {
    let generics = render_generics(&trait_def.generics);
    format!("{}{};", trait_def.name, generics)
}

fn render_impl_signature(impl_def: &ast::ImplDef) -> String {
    let args = impl_def
        .trait_args
        .iter()
        .map(render_type)
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}[{}];", impl_def.trait_name, args)
}

fn render_generics(generics: &[ast::GenericParam]) -> String {
    if generics.is_empty() {
        String::new()
    } else {
        format!(
            "[{}]",
            generics
                .iter()
                .map(|g| {
                    if g.bounds.is_empty() {
                        g.name.clone()
                    } else {
                        format!("{}: {}", g.name, g.bounds.join(" + "))
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn render_type(ty: &ast::TypeExpr) -> String {
    match &ty.kind {
        ast::TypeKind::Unit => "()".to_string(),
        ast::TypeKind::Named { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}[{}]",
                    name,
                    args.iter().map(render_type).collect::<Vec<_>>().join(", ")
                )
            }
        }
        ast::TypeKind::Hole => "_".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{
        collect_std_api_snapshot, compare_snapshots, find_deprecated_api, StdApiSnapshot,
        StdApiSymbol,
    };

    #[test]
    fn finds_deprecated_api_entry() {
        let deprecated = find_deprecated_api("std.time", "now").expect("deprecated entry");
        assert_eq!(deprecated.replacement, "std.time.now_ms");
    }

    #[test]
    fn compare_snapshots_reports_breaking_and_additions() {
        let baseline = StdApiSnapshot {
            schema_version: 1,
            symbols: vec![StdApiSymbol {
                module: "std.time".to_string(),
                kind: "fn".to_string(),
                signature: "now() -> Int effects { time }".to_string(),
            }],
        };
        let current = StdApiSnapshot {
            schema_version: 1,
            symbols: vec![StdApiSymbol {
                module: "std.time".to_string(),
                kind: "fn".to_string(),
                signature: "now_ms() -> Int effects { time }".to_string(),
            }],
        };
        let report = compare_snapshots(&current, &baseline);
        assert_eq!(report.breaking.len(), 1);
        assert_eq!(report.additions.len(), 1);
    }

    #[test]
    fn collects_snapshot_from_std_dir() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("time.aic"),
            r#"module std.time;
fn now_ms() -> Int effects { time } { 0 }
"#,
        )
        .expect("write std file");

        let snapshot = collect_std_api_snapshot(dir.path()).expect("snapshot");
        assert_eq!(snapshot.schema_version, 1);
        assert_eq!(snapshot.symbols.len(), 1);
    }
}
