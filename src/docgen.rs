use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::ast::BinOp;
use crate::driver::FrontendOutput;
use crate::ir;
use crate::std_policy::find_deprecated_api;

#[derive(Debug, Clone)]
pub struct DocOutput {
    pub output_dir: PathBuf,
    pub index_path: PathBuf,
    pub api_json_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct DocIndex {
    schema_version: u32,
    modules: Vec<DocModule>,
}

#[derive(Debug, Clone, Serialize)]
struct DocModule {
    module: String,
    items: Vec<DocItem>,
}

#[derive(Debug, Clone, Serialize)]
struct DocItem {
    kind: String,
    name: String,
    signature: String,
    effects: Vec<String>,
    requires: Option<String>,
    ensures: Option<String>,
    invariant: Option<String>,
    deprecated: Option<DocDeprecated>,
}

#[derive(Debug, Clone, Serialize)]
struct DocDeprecated {
    replacement: String,
    since: String,
    note: String,
}

pub fn generate_docs(front: &FrontendOutput, output_dir: &Path) -> anyhow::Result<DocOutput> {
    fs::create_dir_all(output_dir)?;

    let mut type_map = BTreeMap::new();
    for ty in &front.ir.types {
        type_map.insert(ty.id, ty.repr.clone());
    }

    let mut modules = BTreeMap::<String, Vec<DocItem>>::new();

    for (idx, item) in front.ir.items.iter().enumerate() {
        let module = front
            .item_modules
            .get(idx)
            .and_then(|value| value.clone())
            .map(|parts| parts.join("."))
            .or_else(|| front.ir.module.as_ref().map(|module| module.join(".")))
            .unwrap_or_else(|| "<entry>".to_string());

        let doc_item = match item {
            ir::Item::Function(func) => {
                let effects = func.effects.clone();
                let requires = func.requires.as_ref().map(render_expr);
                let ensures = func.ensures.as_ref().map(render_expr);
                let deprecated =
                    find_deprecated_api(&module, &func.name).map(|entry| DocDeprecated {
                        replacement: entry.replacement.to_string(),
                        since: entry.since.to_string(),
                        note: entry.note.to_string(),
                    });
                DocItem {
                    kind: "fn".to_string(),
                    name: func.name.clone(),
                    signature: render_function_signature(func, &type_map),
                    effects,
                    requires,
                    ensures,
                    invariant: None,
                    deprecated,
                }
            }
            ir::Item::Struct(strukt) => DocItem {
                kind: "struct".to_string(),
                name: strukt.name.clone(),
                signature: render_struct_signature(strukt, &type_map),
                effects: Vec::new(),
                requires: None,
                ensures: None,
                invariant: strukt.invariant.as_ref().map(render_expr),
                deprecated: None,
            },
            ir::Item::Enum(enm) => DocItem {
                kind: "enum".to_string(),
                name: enm.name.clone(),
                signature: render_enum_signature(enm, &type_map),
                effects: Vec::new(),
                requires: None,
                ensures: None,
                invariant: None,
                deprecated: None,
            },
            ir::Item::Trait(trait_def) => DocItem {
                kind: "trait".to_string(),
                name: trait_def.name.clone(),
                signature: render_trait_signature(trait_def),
                effects: Vec::new(),
                requires: None,
                ensures: None,
                invariant: None,
                deprecated: None,
            },
            ir::Item::Impl(impl_def) => DocItem {
                kind: "impl".to_string(),
                name: impl_def.trait_name.clone(),
                signature: render_impl_signature(impl_def, &type_map),
                effects: Vec::new(),
                requires: None,
                ensures: None,
                invariant: None,
                deprecated: None,
            },
        };

        modules.entry(module).or_default().push(doc_item);
    }

    for items in modules.values_mut() {
        items.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    }

    let mut module_docs = modules
        .into_iter()
        .map(|(module, items)| DocModule { module, items })
        .collect::<Vec<_>>();
    module_docs.sort_by(|a, b| a.module.cmp(&b.module));

    let doc_index = DocIndex {
        schema_version: 1,
        modules: module_docs,
    };

    let index_md = render_markdown(&doc_index);
    let index_path = output_dir.join("index.md");
    fs::write(&index_path, index_md)?;

    let api_json_path = output_dir.join("api.json");
    fs::write(
        &api_json_path,
        format!("{}\n", serde_json::to_string_pretty(&doc_index)?),
    )?;

    Ok(DocOutput {
        output_dir: output_dir.to_path_buf(),
        index_path,
        api_json_path,
    })
}

fn render_markdown(index: &DocIndex) -> String {
    let mut out = String::new();
    out.push_str("# AIC API Documentation\n\n");
    for module in &index.modules {
        out.push_str(&format!("## {}\n\n", module.module));
        for item in &module.items {
            out.push_str(&format!("### {} {}\n\n", item.kind, item.name));
            out.push_str("```aic\n");
            out.push_str(&item.signature);
            out.push_str("\n```\n\n");
            if !item.effects.is_empty() {
                out.push_str(&format!("- Effects: `{}`\n", item.effects.join(", ")));
            }
            if let Some(requires) = &item.requires {
                out.push_str(&format!("- Requires: `{}`\n", requires));
            }
            if let Some(ensures) = &item.ensures {
                out.push_str(&format!("- Ensures: `{}`\n", ensures));
            }
            if let Some(invariant) = &item.invariant {
                out.push_str(&format!("- Invariant: `{}`\n", invariant));
            }
            if let Some(deprecated) = &item.deprecated {
                out.push_str(&format!(
                    "- Deprecated: since `{}`; use `{}` ({})\n",
                    deprecated.since, deprecated.replacement, deprecated.note
                ));
            }
            out.push('\n');
        }
    }
    out
}

fn render_function_signature(func: &ir::Function, types: &BTreeMap<ir::TypeId, String>) -> String {
    let generics = render_generic_params(&func.generics);

    let params = func
        .params
        .iter()
        .map(|param| {
            let ty = types
                .get(&param.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            format!("{}: {}", param.name, ty)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let ret = types
        .get(&func.ret_type)
        .cloned()
        .unwrap_or_else(|| "<?>".to_string());

    let effects = if func.effects.is_empty() {
        String::new()
    } else {
        format!(" effects {{ {} }}", func.effects.join(", "))
    };

    let async_prefix = if func.is_async { "async " } else { "" };
    format!(
        "{}fn {}{}({}) -> {}{}",
        async_prefix, func.name, generics, params, ret, effects
    )
}

fn render_struct_signature(strukt: &ir::StructDef, types: &BTreeMap<ir::TypeId, String>) -> String {
    let generics = render_generic_params(&strukt.generics);

    let fields = strukt
        .fields
        .iter()
        .map(|field| {
            let ty = types
                .get(&field.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            format!("{}: {}", field.name, ty)
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("struct {}{} {{ {} }}", strukt.name, generics, fields)
}

fn render_enum_signature(enm: &ir::EnumDef, types: &BTreeMap<ir::TypeId, String>) -> String {
    let generics = render_generic_params(&enm.generics);

    let variants = enm
        .variants
        .iter()
        .map(|variant| {
            if let Some(payload) = variant.payload {
                let ty = types
                    .get(&payload)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string());
                format!("{}({})", variant.name, ty)
            } else {
                variant.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("enum {}{} {{ {} }}", enm.name, generics, variants)
}

fn render_trait_signature(trait_def: &ir::TraitDef) -> String {
    let generics = render_generic_params(&trait_def.generics);
    format!("trait {}{};", trait_def.name, generics)
}

fn render_impl_signature(impl_def: &ir::ImplDef, types: &BTreeMap<ir::TypeId, String>) -> String {
    let args = impl_def
        .trait_args
        .iter()
        .map(|ty| types.get(ty).cloned().unwrap_or_else(|| "<?>".to_string()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("impl {}[{}];", impl_def.trait_name, args)
}

fn render_generic_params(generics: &[ir::GenericParam]) -> String {
    if generics.is_empty() {
        return String::new();
    }
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

fn render_expr(expr: &ir::Expr) -> String {
    match &expr.kind {
        ir::ExprKind::Int(v) => v.to_string(),
        ir::ExprKind::Bool(v) => v.to_string(),
        ir::ExprKind::String(s) => format!("\"{}\"", s),
        ir::ExprKind::Unit => "()".to_string(),
        ir::ExprKind::Var(name) => name.clone(),
        ir::ExprKind::Call { callee, args } => {
            let args = args.iter().map(render_expr).collect::<Vec<_>>().join(", ");
            format!("{}({})", render_expr(callee), args)
        }
        ir::ExprKind::If { cond, .. } => {
            format!("if {} {{ ... }} else {{ ... }}", render_expr(cond))
        }
        ir::ExprKind::While { cond, .. } => format!("while {} {{ ... }}", render_expr(cond)),
        ir::ExprKind::Loop { .. } => "loop { ... }".to_string(),
        ir::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                format!("break {}", render_expr(expr))
            } else {
                "break".to_string()
            }
        }
        ir::ExprKind::Continue => "continue".to_string(),
        ir::ExprKind::Match { expr, .. } => format!("match {} {{ ... }}", render_expr(expr)),
        ir::ExprKind::Binary { op, lhs, rhs } => {
            format!(
                "({} {} {})",
                render_expr(lhs),
                render_binop(*op),
                render_expr(rhs)
            )
        }
        ir::ExprKind::Unary { op, expr } => {
            let op = match op {
                crate::ast::UnaryOp::Neg => "-",
                crate::ast::UnaryOp::Not => "!",
            };
            format!("{}{}", op, render_expr(expr))
        }
        ir::ExprKind::Borrow { mutable, expr } => {
            if *mutable {
                format!("&mut {}", render_expr(expr))
            } else {
                format!("&{}", render_expr(expr))
            }
        }
        ir::ExprKind::Await { expr } => format!("await {}", render_expr(expr)),
        ir::ExprKind::Try { expr } => format!("{}?", render_expr(expr)),
        ir::ExprKind::StructInit { name, fields } => {
            let rendered = fields
                .iter()
                .map(|(field, value, _)| format!("{}: {}", field, render_expr(value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {{ {} }}", name, rendered)
        }
        ir::ExprKind::FieldAccess { base, field } => format!("{}.{}", render_expr(base), field),
        ir::ExprKind::UnsafeBlock { .. } => "unsafe { ... }".to_string(),
    }
}

fn render_binop(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::driver::{has_errors, run_frontend};

    use super::generate_docs;

    #[test]
    fn docgen_emits_signatures_effects_and_contracts() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("main.aic");
        fs::write(
            &src,
            r#"module app.main;
import std.time;

fn abs(x: Int) -> Int effects { time } requires x >= 0 ensures result >= 0 {
    x
}

fn main() -> Int effects { time } {
    now();
    abs(1)
}
"#,
        )
        .expect("write source");

        let front = run_frontend(&src).expect("frontend");
        assert!(
            !has_errors(&front.diagnostics),
            "diagnostics={:#?}",
            front.diagnostics
        );

        let out = generate_docs(&front, &dir.path().join("docs/api")).expect("docgen");
        let index = fs::read_to_string(out.index_path).expect("read index");
        assert!(index.contains("fn abs(x: Int) -> Int effects { time }"));
        assert!(index.contains("Requires: `(x >= 0)`"));
        assert!(index.contains("Ensures: `(result >= 0)`"));
    }
}
