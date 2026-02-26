use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use serde::Serialize;
use serde_json::{json, Value};

use crate::ast::{self, BinOp, Expr, ExprKind, Pattern, PatternKind, Stmt, TypeExpr, TypeKind};
use crate::parser;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SemanticDiffReport {
    pub changes: Vec<SemanticChange>,
    pub summary: SemanticDiffSummary,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct SemanticDiffSummary {
    pub breaking: usize,
    pub non_breaking: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SemanticChange {
    pub kind: String,
    pub module: String,
    pub function: String,
    pub breaking: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FunctionKey {
    module: String,
    function: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSignature {
    generics: Vec<String>,
    params: Vec<String>,
    return_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSemantic {
    signature: FunctionSignature,
    effects: Vec<String>,
    requires: Option<String>,
    ensures: Option<String>,
}

pub fn diff_files(old_file: &Path, new_file: &Path) -> anyhow::Result<SemanticDiffReport> {
    let old_functions = collect_functions(old_file)?;
    let new_functions = collect_functions(new_file)?;
    Ok(compare_function_sets(&old_functions, &new_functions))
}

fn collect_functions(entry_file: &Path) -> anyhow::Result<BTreeMap<FunctionKey, FunctionSemantic>> {
    let mut visited = BTreeSet::new();
    let mut functions = BTreeMap::new();
    collect_functions_recursive(entry_file, &mut visited, &mut functions)?;
    Ok(functions)
}

fn collect_functions_recursive(
    path: &Path,
    visited: &mut BTreeSet<PathBuf>,
    functions: &mut BTreeMap<FunctionKey, FunctionSemantic>,
) -> anyhow::Result<()> {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        return Ok(());
    }

    let source = fs::read_to_string(&canonical)
        .with_context(|| format!("failed to read {}", canonical.display()))?;
    let file_label = canonical.to_string_lossy().to_string();
    let (program, diagnostics) = parser::parse(&source, &file_label);
    if diagnostics.iter().any(|diag| diag.is_error()) {
        let details = diagnostics
            .iter()
            .map(|diag| format!("{}:{}", diag.code, diag.message))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(anyhow!(
            "failed to parse {} for semantic diff: {}",
            canonical.display(),
            details
        ));
    }

    let Some(program) = program else {
        return Err(anyhow!(
            "failed to build AST for semantic diff from {}",
            canonical.display()
        ));
    };

    let module = program
        .module
        .as_ref()
        .map(|module| module.path.join("."))
        .unwrap_or_else(|| "<root>".to_string());

    for item in program.items {
        if let ast::Item::Function(function) = item {
            if is_internal_generated_function(&function.name) {
                continue;
            }
            let key = FunctionKey {
                module: module.clone(),
                function: function.name.clone(),
            };
            functions.insert(key, build_function_semantic(&function));
        }
    }

    let base_dir = canonical.parent().unwrap_or_else(|| Path::new("."));
    for import in program.imports {
        if let Some(import_path) = resolve_import_path(base_dir, &import.path) {
            collect_functions_recursive(&import_path, visited, functions)?;
        }
    }

    Ok(())
}

fn resolve_import_path(base_dir: &Path, import_path: &[String]) -> Option<PathBuf> {
    if import_path.is_empty() {
        return None;
    }

    let nested = import_path.iter().fold(PathBuf::new(), |mut acc, segment| {
        acc.push(segment);
        acc
    });

    let mut cursor = Some(base_dir);
    while let Some(dir) = cursor {
        let candidate = dir.join(&nested).with_extension("aic");
        if candidate.exists() {
            return Some(candidate);
        }
        let sibling = dir.join(format!("{}.aic", import_path.last()?));
        if sibling.exists() {
            return Some(sibling);
        }
        cursor = dir.parent();
    }

    None
}

fn is_internal_generated_function(name: &str) -> bool {
    ast::decode_internal_type_alias(name).is_some() || ast::decode_internal_const(name).is_some()
}

fn build_function_semantic(function: &ast::Function) -> FunctionSemantic {
    let mut effects = function.effects.clone();
    effects.sort();
    effects.dedup();

    FunctionSemantic {
        signature: FunctionSignature {
            generics: function.generics.iter().map(render_generic_param).collect(),
            params: function
                .params
                .iter()
                .map(|param| format!("{}: {}", param.name, render_type(&param.ty)))
                .collect(),
            return_type: render_type(&function.ret_type),
        },
        effects,
        requires: function.requires.as_ref().map(render_expr),
        ensures: function.ensures.as_ref().map(render_expr),
    }
}

fn compare_function_sets(
    old_functions: &BTreeMap<FunctionKey, FunctionSemantic>,
    new_functions: &BTreeMap<FunctionKey, FunctionSemantic>,
) -> SemanticDiffReport {
    let mut changes = Vec::new();
    let keys = old_functions
        .keys()
        .chain(new_functions.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    for key in keys {
        match (old_functions.get(&key), new_functions.get(&key)) {
            (Some(old_function), None) => push_change(
                &mut changes,
                "function_removed",
                &key,
                true,
                Some(function_payload(old_function)),
                None,
                None,
            ),
            (None, Some(new_function)) => push_change(
                &mut changes,
                "function_added",
                &key,
                false,
                None,
                Some(function_payload(new_function)),
                None,
            ),
            (Some(old_function), Some(new_function)) => {
                if old_function.signature.generics != new_function.signature.generics {
                    push_change(
                        &mut changes,
                        "generics_changed",
                        &key,
                        true,
                        Some(json!(&old_function.signature.generics)),
                        Some(json!(&new_function.signature.generics)),
                        None,
                    );
                }

                if old_function.signature.params != new_function.signature.params {
                    push_change(
                        &mut changes,
                        "params_changed",
                        &key,
                        true,
                        Some(json!(&old_function.signature.params)),
                        Some(json!(&new_function.signature.params)),
                        None,
                    );
                }

                if old_function.signature.return_type != new_function.signature.return_type {
                    push_change(
                        &mut changes,
                        "return_changed",
                        &key,
                        true,
                        Some(json!(&old_function.signature.return_type)),
                        Some(json!(&new_function.signature.return_type)),
                        None,
                    );
                }

                let old_effects = old_function
                    .effects
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let new_effects = new_function
                    .effects
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let added_effects = new_effects
                    .difference(&old_effects)
                    .cloned()
                    .collect::<Vec<_>>();
                let removed_effects = old_effects
                    .difference(&new_effects)
                    .cloned()
                    .collect::<Vec<_>>();
                if !added_effects.is_empty() || !removed_effects.is_empty() {
                    push_change(
                        &mut changes,
                        "effects_changed",
                        &key,
                        !added_effects.is_empty(),
                        Some(json!(&old_function.effects)),
                        Some(json!(&new_function.effects)),
                        Some(json!({
                            "added": added_effects,
                            "removed": removed_effects
                        })),
                    );
                }

                if old_function.requires != new_function.requires {
                    let (breaking, classification) =
                        classify_requires_change(&old_function.requires, &new_function.requires);
                    push_change(
                        &mut changes,
                        "requires_changed",
                        &key,
                        breaking,
                        Some(json!(&old_function.requires)),
                        Some(json!(&new_function.requires)),
                        Some(json!({ "classification": classification })),
                    );
                }

                if old_function.ensures != new_function.ensures {
                    let (breaking, classification) =
                        classify_ensures_change(&old_function.ensures, &new_function.ensures);
                    push_change(
                        &mut changes,
                        "ensures_changed",
                        &key,
                        breaking,
                        Some(json!(&old_function.ensures)),
                        Some(json!(&new_function.ensures)),
                        Some(json!({ "classification": classification })),
                    );
                }
            }
            (None, None) => {}
        }
    }

    let breaking = changes.iter().filter(|change| change.breaking).count();
    let non_breaking = changes.len().saturating_sub(breaking);

    SemanticDiffReport {
        changes,
        summary: SemanticDiffSummary {
            breaking,
            non_breaking,
        },
    }
}

fn function_payload(function: &FunctionSemantic) -> Value {
    json!({
        "generics": &function.signature.generics,
        "params": &function.signature.params,
        "return": &function.signature.return_type,
        "effects": &function.effects,
        "requires": &function.requires,
        "ensures": &function.ensures
    })
}

fn push_change(
    changes: &mut Vec<SemanticChange>,
    kind: &str,
    key: &FunctionKey,
    breaking: bool,
    old: Option<Value>,
    new: Option<Value>,
    detail: Option<Value>,
) {
    changes.push(SemanticChange {
        kind: kind.to_string(),
        module: key.module.clone(),
        function: key.function.clone(),
        breaking,
        old,
        new,
        detail,
    });
}

fn classify_requires_change(old: &Option<String>, new: &Option<String>) -> (bool, &'static str) {
    match (old, new) {
        (None, Some(_)) => (true, "new_precondition"),
        (Some(_), None) => (false, "removed_precondition"),
        (Some(_), Some(_)) => (true, "precondition_changed"),
        (None, None) => (false, "unchanged"),
    }
}

fn classify_ensures_change(old: &Option<String>, new: &Option<String>) -> (bool, &'static str) {
    match (old, new) {
        (None, Some(_)) => (false, "new_postcondition"),
        (Some(_), None) => (true, "removed_postcondition"),
        (Some(_), Some(_)) => (true, "postcondition_changed"),
        (None, None) => (false, "unchanged"),
    }
}

fn render_generic_param(param: &ast::GenericParam) -> String {
    if param.bounds.is_empty() {
        param.name.clone()
    } else {
        format!("{}: {}", param.name, param.bounds.join(" + "))
    }
}

fn render_type(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeKind::Unit => "()".to_string(),
        TypeKind::Named { name, args } => {
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
        TypeKind::Hole => "_".to_string(),
    }
}

fn render_expr(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Int(value) => value.to_string(),
        ExprKind::Float(value) => render_float_literal(*value),
        ExprKind::Bool(value) => value.to_string(),
        ExprKind::Char(value) => format!("{:?}", value),
        ExprKind::String(value) => format!("{value:?}"),
        ExprKind::Unit => "()".to_string(),
        ExprKind::Var(name) => name.clone(),
        ExprKind::Call {
            callee,
            args,
            arg_names,
        } => {
            let rendered_args = args
                .iter()
                .enumerate()
                .map(|(idx, arg)| {
                    if let Some(name) = arg_names.get(idx).and_then(|name| name.as_deref()) {
                        format!("{}: {}", name, render_expr(arg))
                    } else {
                        render_expr(arg)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({rendered_args})", render_expr(callee))
        }
        ExprKind::Closure {
            params,
            ret_type,
            body,
        } => {
            let rendered_params = params
                .iter()
                .map(|param| match &param.ty {
                    Some(ty) => format!("{}: {}", param.name, render_type(ty)),
                    None => param.name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "|{rendered_params}| -> {} {}",
                render_type(ret_type),
                render_block(body)
            )
        }
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => format!(
            "if {} {} else {}",
            render_expr(cond),
            render_block(then_block),
            render_block(else_block)
        ),
        ExprKind::While { cond, body } => {
            format!("while {} {}", render_expr(cond), render_block(body))
        }
        ExprKind::Loop { body } => format!("loop {}", render_block(body)),
        ExprKind::Break { expr } => match expr {
            Some(expr) => format!("break {}", render_expr(expr)),
            None => "break".to_string(),
        },
        ExprKind::Continue => "continue".to_string(),
        ExprKind::Match { expr, arms } => {
            let rendered_arms = arms
                .iter()
                .map(|arm| {
                    let guard = arm
                        .guard
                        .as_ref()
                        .map(|guard| format!(" if {}", render_expr(guard)))
                        .unwrap_or_default();
                    format!(
                        "{}{} => {}",
                        render_pattern(&arm.pattern),
                        guard,
                        render_expr(&arm.body)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("match {} {{ {rendered_arms} }}", render_expr(expr))
        }
        ExprKind::Binary { op, lhs, rhs } => {
            format!(
                "({} {} {})",
                render_expr(lhs),
                render_binop(*op),
                render_expr(rhs)
            )
        }
        ExprKind::Unary { op, expr } => format!("{}{}", render_unary_op(*op), render_expr(expr)),
        ExprKind::Borrow { mutable, expr } => {
            if *mutable {
                format!("&mut {}", render_expr(expr))
            } else {
                format!("&{}", render_expr(expr))
            }
        }
        ExprKind::Await { expr } => format!("await {}", render_expr(expr)),
        ExprKind::Try { expr } => format!("{}?", render_expr(expr)),
        ExprKind::UnsafeBlock { block } => format!("unsafe {}", render_block(block)),
        ExprKind::StructInit { name, fields } => {
            let rendered_fields = fields
                .iter()
                .map(|(field, value, _)| format!("{field}: {}", render_expr(value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {{ {rendered_fields} }}")
        }
        ExprKind::FieldAccess { base, field } => format!("{}.{}", render_expr(base), field),
    }
}

fn render_block(block: &ast::Block) -> String {
    let mut parts = block.stmts.iter().map(render_stmt).collect::<Vec<_>>();
    if let Some(tail) = &block.tail {
        parts.push(render_expr(tail));
    }
    if parts.is_empty() {
        "{ }".to_string()
    } else {
        format!("{{ {} }}", parts.join("; "))
    }
}

fn render_stmt(stmt: &Stmt) -> String {
    match stmt {
        Stmt::Let {
            name,
            mutable,
            ty,
            expr,
            ..
        } => {
            let mut_prefix = if *mutable { "mut " } else { "" };
            let rendered_ty = ty
                .as_ref()
                .map(|ty| format!(": {}", render_type(ty)))
                .unwrap_or_default();
            format!(
                "let {mut_prefix}{name}{rendered_ty} = {}",
                render_expr(expr)
            )
        }
        Stmt::Assign { target, expr, .. } => format!("{target} = {}", render_expr(expr)),
        Stmt::Expr { expr, .. } => render_expr(expr),
        Stmt::Return { expr, .. } => match expr {
            Some(expr) => format!("return {}", render_expr(expr)),
            None => "return".to_string(),
        },
        Stmt::Assert { expr, message, .. } => {
            format!("assert({}, {message:?})", render_expr(expr))
        }
    }
}

fn render_pattern(pattern: &Pattern) -> String {
    match &pattern.kind {
        PatternKind::Wildcard => "_".to_string(),
        PatternKind::Var(name) => name.clone(),
        PatternKind::Int(value) => value.to_string(),
        PatternKind::Bool(value) => value.to_string(),
        PatternKind::Unit => "()".to_string(),
        PatternKind::Or { patterns } => patterns
            .iter()
            .map(render_pattern)
            .collect::<Vec<_>>()
            .join(" | "),
        PatternKind::Variant { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}({})",
                    name,
                    args.iter()
                        .map(render_pattern)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

fn render_float_literal(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f64::INFINITY {
        "inf".to_string()
    } else if value == f64::NEG_INFINITY {
        "-inf".to_string()
    } else {
        let mut text = format!("{value}");
        if !text.contains('.') && !text.contains('e') && !text.contains('E') {
            text.push_str(".0");
        }
        text
    }
}

fn render_binop(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Ushr => ">>>",
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

fn render_unary_op(op: ast::UnaryOp) -> &'static str {
    match op {
        ast::UnaryOp::Neg => "-",
        ast::UnaryOp::Not => "!",
        ast::UnaryOp::BitNot => "~",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::diff_files;

    #[test]
    fn diff_reports_signature_effect_and_contract_changes_deterministically() {
        let dir = tempdir().expect("tempdir");
        let old_path = dir.path().join("v1.aic");
        let new_path = dir.path().join("v2.aic");

        fs::write(
            &old_path,
            r#"module demo.main;
fn compute[T](x: T) -> Int effects { io } requires x == x ensures result >= 0 {
    1
}
fn removed() -> Int { 0 }
"#,
        )
        .expect("write old file");

        fs::write(
            &new_path,
            r#"module demo.main;
fn compute[T, U](x: T, y: U) -> Float effects { io, fs } requires x == x ensures result >= 0.0 {
    1.0
}
fn added() -> Int { 0 }
"#,
        )
        .expect("write new file");

        let first = diff_files(&old_path, &new_path).expect("first diff");
        let second = diff_files(&old_path, &new_path).expect("second diff");
        assert_eq!(
            serde_json::to_string(&first).expect("serialize first"),
            serde_json::to_string(&second).expect("serialize second")
        );

        assert_eq!(first.summary.breaking, 6);
        assert_eq!(first.summary.non_breaking, 1);

        let kinds = first
            .changes
            .iter()
            .map(|change| change.kind.as_str())
            .collect::<Vec<_>>();
        assert!(kinds.contains(&"function_removed"));
        assert!(kinds.contains(&"function_added"));
        assert!(kinds.contains(&"generics_changed"));
        assert!(kinds.contains(&"params_changed"));
        assert!(kinds.contains(&"return_changed"));
        assert!(kinds.contains(&"effects_changed"));
        assert!(kinds.contains(&"ensures_changed"));
    }

    #[test]
    fn diff_includes_imported_modules_for_cross_module_changes() {
        let dir = tempdir().expect("tempdir");
        let old_root = dir.path().join("old");
        let new_root = dir.path().join("new");
        fs::create_dir_all(old_root.join("demo")).expect("mkdir old demo");
        fs::create_dir_all(new_root.join("demo")).expect("mkdir new demo");

        fs::write(
            old_root.join("main.aic"),
            "module demo.main;\nimport demo.util;\nfn main() -> Int { 0 }\n",
        )
        .expect("write old main");
        fs::write(
            new_root.join("main.aic"),
            "module demo.main;\nimport demo.util;\nfn main() -> Int { 0 }\n",
        )
        .expect("write new main");

        fs::write(
            old_root.join("demo/util.aic"),
            r#"module demo.util;
fn helper(x: Int) -> Int effects { io } requires x > 0 ensures result > 0 { x }
"#,
        )
        .expect("write old util");
        fs::write(
            new_root.join("demo/util.aic"),
            r#"module demo.util;
fn helper(x: Int) -> Int effects { io, fs } requires x >= 0 ensures result > 0 { x }
"#,
        )
        .expect("write new util");

        let report =
            diff_files(&old_root.join("main.aic"), &new_root.join("main.aic")).expect("report");

        assert!(report.changes.iter().any(|change| {
            change.module == "demo.util"
                && change.function == "helper"
                && change.kind == "effects_changed"
        }));
        assert!(report.changes.iter().any(|change| {
            change.module == "demo.util"
                && change.function == "helper"
                && change.kind == "requires_changed"
        }));
    }
}
