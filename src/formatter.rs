use std::collections::BTreeMap;

use crate::ast::{decode_internal_const, decode_internal_type_alias, BinOp, UnaryOp};
use crate::ir;

pub fn format_program(program: &ir::Program) -> String {
    let mut out = String::new();
    let type_map = type_map(program);

    if let Some(module) = &program.module {
        out.push_str("module ");
        out.push_str(&module.join("."));
        out.push_str(";\n\n");
    }

    let mut imports = program.imports.clone();
    imports.sort();
    for import in imports {
        out.push_str("import ");
        out.push_str(&import.join("."));
        out.push_str(";\n");
    }
    if !program.imports.is_empty() {
        out.push('\n');
    }

    for (idx, item) in program.items.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        match item {
            ir::Item::Function(f) => format_function(&mut out, f, &type_map),
            ir::Item::Struct(s) => format_struct(&mut out, s, &type_map),
            ir::Item::Enum(e) => format_enum(&mut out, e, &type_map),
            ir::Item::Trait(t) => format_trait(&mut out, t),
            ir::Item::Impl(i) => format_impl(&mut out, i, &type_map),
        }
    }

    out
}

fn format_function(out: &mut String, f: &ir::Function, type_map: &BTreeMap<ir::TypeId, String>) {
    if let Some(name) = decode_internal_type_alias(&f.name) {
        format_type_alias(out, name, f, type_map);
        return;
    }
    if let Some(name) = decode_internal_const(&f.name) {
        format_const(out, name, f, type_map);
        return;
    }

    if f.is_extern {
        out.push_str("extern \"");
        out.push_str(f.extern_abi.as_deref().unwrap_or("C"));
        out.push_str("\" fn ");
        out.push_str(&f.name);
        format_generic_params(out, &f.generics);
        out.push('(');
        out.push_str(
            &f.params
                .iter()
                .map(|p| {
                    format!(
                        "{}: {}",
                        p.name,
                        type_map
                            .get(&p.ty)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string())
                    )
                })
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str(") -> ");
        out.push_str(
            type_map
                .get(&f.ret_type)
                .map(|s| s.as_str())
                .unwrap_or("<?>"),
        );
        out.push_str(";\n");
        return;
    }

    if f.is_async {
        out.push_str("async ");
    }
    if f.is_unsafe {
        out.push_str("unsafe ");
    }
    out.push_str("fn ");
    out.push_str(&f.name);
    format_generic_params(out, &f.generics);
    out.push('(');
    out.push_str(
        &f.params
            .iter()
            .map(|p| {
                format!(
                    "{}: {}",
                    p.name,
                    type_map
                        .get(&p.ty)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string())
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str(") -> ");
    out.push_str(
        type_map
            .get(&f.ret_type)
            .map(|s| s.as_str())
            .unwrap_or("<?>"),
    );

    if !f.effects.is_empty() {
        let mut effects = f.effects.clone();
        effects.sort();
        effects.dedup();
        out.push_str(" effects { ");
        out.push_str(&effects.join(", "));
        out.push_str(" }");
    }

    if let Some(req) = &f.requires {
        out.push_str(" requires ");
        format_expr(out, req, 0, type_map);
    }

    if let Some(ens) = &f.ensures {
        out.push_str(" ensures ");
        format_expr(out, ens, 0, type_map);
    }

    out.push(' ');
    format_block(out, &f.body, type_map, 0);
    out.push('\n');
}

fn format_type_alias(
    out: &mut String,
    name: &str,
    f: &ir::Function,
    type_map: &BTreeMap<ir::TypeId, String>,
) {
    out.push_str("type ");
    out.push_str(name);
    format_generic_params(out, &f.generics);
    out.push_str(" = ");
    out.push_str(
        type_map
            .get(&f.ret_type)
            .map(|s| s.as_str())
            .unwrap_or("<?>"),
    );
    out.push_str(";\n");
}

fn format_const(
    out: &mut String,
    name: &str,
    f: &ir::Function,
    type_map: &BTreeMap<ir::TypeId, String>,
) {
    out.push_str("const ");
    out.push_str(name);
    out.push_str(": ");
    out.push_str(
        type_map
            .get(&f.ret_type)
            .map(|s| s.as_str())
            .unwrap_or("<?>"),
    );
    out.push_str(" = ");
    if let Some(expr) = &f.body.tail {
        format_expr(out, expr, 0, type_map);
    } else {
        out.push_str("()");
    }
    out.push_str(";\n");
}

fn format_struct(out: &mut String, s: &ir::StructDef, type_map: &BTreeMap<ir::TypeId, String>) {
    out.push_str("struct ");
    out.push_str(&s.name);
    format_generic_params(out, &s.generics);
    out.push_str(" {\n");
    for field in &s.fields {
        out.push_str("    ");
        out.push_str(&field.name);
        out.push_str(": ");
        out.push_str(type_map.get(&field.ty).map(|s| s.as_str()).unwrap_or("<?>"));
        out.push_str(",\n");
    }
    out.push('}');
    if let Some(inv) = &s.invariant {
        out.push_str(" invariant ");
        format_expr(out, inv, 0, type_map);
    }
    out.push('\n');
}

fn format_enum(out: &mut String, e: &ir::EnumDef, type_map: &BTreeMap<ir::TypeId, String>) {
    out.push_str("enum ");
    out.push_str(&e.name);
    format_generic_params(out, &e.generics);
    out.push_str(" {\n");
    for variant in &e.variants {
        out.push_str("    ");
        out.push_str(&variant.name);
        if let Some(ty) = variant.payload {
            out.push('(');
            out.push_str(type_map.get(&ty).map(|s| s.as_str()).unwrap_or("<?>"));
            out.push(')');
        }
        out.push_str(",\n");
    }
    out.push_str("}\n");
}

fn format_trait(out: &mut String, t: &ir::TraitDef) {
    out.push_str("trait ");
    out.push_str(&t.name);
    format_generic_params(out, &t.generics);
    out.push_str(";\n");
}

fn format_impl(out: &mut String, i: &ir::ImplDef, type_map: &BTreeMap<ir::TypeId, String>) {
    out.push_str("impl ");
    out.push_str(&i.trait_name);
    out.push('[');
    out.push_str(
        &i.trait_args
            .iter()
            .map(|ty| {
                type_map
                    .get(ty)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string())
            })
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str("];\n");
}

fn format_generic_params(out: &mut String, generics: &[ir::GenericParam]) {
    if generics.is_empty() {
        return;
    }
    out.push('[');
    out.push_str(
        &generics
            .iter()
            .map(|g| {
                if g.bounds.is_empty() {
                    g.name.clone()
                } else {
                    format!("{}: {}", g.name, g.bounds.join(" + "))
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push(']');
}

fn format_block(
    out: &mut String,
    block: &ir::Block,
    type_map: &BTreeMap<ir::TypeId, String>,
    indent: usize,
) {
    out.push_str("{\n");
    for stmt in &block.stmts {
        out.push_str(&" ".repeat(indent + 4));
        match stmt {
            ir::Stmt::Let {
                name,
                mutable,
                ty,
                expr,
                ..
            } => {
                out.push_str("let ");
                if *mutable {
                    out.push_str("mut ");
                }
                out.push_str(name);
                if let Some(ty) = ty {
                    out.push_str(": ");
                    out.push_str(type_map.get(ty).map(|s| s.as_str()).unwrap_or("<?>"));
                }
                out.push_str(" = ");
                format_expr(out, expr, 0, type_map);
                out.push_str(";\n");
            }
            ir::Stmt::Assign { target, expr, .. } => {
                out.push_str(target);
                out.push_str(" = ");
                format_expr(out, expr, 0, type_map);
                out.push_str(";\n");
            }
            ir::Stmt::Expr { expr, .. } => {
                format_expr(out, expr, 0, type_map);
                out.push_str(";\n");
            }
            ir::Stmt::Return { expr, .. } => {
                out.push_str("return");
                if let Some(expr) = expr {
                    out.push(' ');
                    format_expr(out, expr, 0, type_map);
                }
                out.push_str(";\n");
            }
            ir::Stmt::Assert { expr, message, .. } => {
                out.push_str("assert ");
                format_expr(out, expr, 0, type_map);
                out.push_str("; // ");
                out.push_str(message);
                out.push('\n');
            }
        }
    }

    if let Some(tail) = &block.tail {
        out.push_str(&" ".repeat(indent + 4));
        format_expr(out, tail, 0, type_map);
        out.push('\n');
    }
    out.push_str(&" ".repeat(indent));
    out.push('}');
}

fn format_expr(
    out: &mut String,
    expr: &ir::Expr,
    parent_prec: u8,
    type_map: &BTreeMap<ir::TypeId, String>,
) {
    if let Some(rendered_for) = extract_for_syntax(expr) {
        out.push_str("for ");
        out.push_str(&rendered_for.binding);
        out.push_str(" in ");
        match rendered_for.iterable {
            RenderedIterable::Expr(iterable) => format_expr(out, &iterable, 0, type_map),
            RenderedIterable::Range { start, end } => {
                format_expr(out, &start, 0, type_map);
                out.push_str("..");
                format_expr(out, &end, 0, type_map);
            }
        }
        out.push(' ');
        format_block(out, &rendered_for.body, type_map, 0);
        return;
    }

    match &expr.kind {
        ir::ExprKind::Int(v) => out.push_str(&v.to_string()),
        ir::ExprKind::Float(v) => out.push_str(&render_float_literal(*v)),
        ir::ExprKind::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        ir::ExprKind::String(v) => {
            out.push('"');
            out.push_str(&v.replace('\\', "\\\\").replace('"', "\\\""));
            out.push('"');
        }
        ir::ExprKind::Unit => out.push_str("()"),
        ir::ExprKind::Var(v) => out.push_str(v),
        ir::ExprKind::Call { callee, args } => {
            format_expr(out, callee, 10, type_map);
            out.push('(');
            for (idx, arg) in args.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                format_expr(out, arg, 0, type_map);
            }
            out.push(')');
        }
        ir::ExprKind::Closure {
            params,
            ret_type,
            body,
        } => {
            out.push('|');
            for (idx, param) in params.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(&param.name);
                if let Some(ty) = param.ty {
                    out.push_str(": ");
                    out.push_str(type_map.get(&ty).map(|s| s.as_str()).unwrap_or("<?>"));
                }
            }
            out.push('|');
            out.push_str(" -> ");
            out.push_str(type_map.get(ret_type).map(|s| s.as_str()).unwrap_or("<?>"));
            out.push(' ');
            format_block(out, body, type_map, 0);
        }
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            out.push_str("if ");
            format_expr(out, cond, 0, type_map);
            out.push(' ');
            format_block(out, then_block, type_map, 0);
            out.push_str(" else ");
            format_block(out, else_block, type_map, 0);
        }
        ir::ExprKind::While { cond, body } => {
            out.push_str("while ");
            format_expr(out, cond, 0, type_map);
            out.push(' ');
            format_block(out, body, type_map, 0);
        }
        ir::ExprKind::Loop { body } => {
            out.push_str("loop ");
            format_block(out, body, type_map, 0);
        }
        ir::ExprKind::Break { expr } => {
            out.push_str("break");
            if let Some(expr) = expr {
                out.push(' ');
                format_expr(out, expr, 0, type_map);
            }
        }
        ir::ExprKind::Continue => out.push_str("continue"),
        ir::ExprKind::Match { expr, arms } => {
            out.push_str("match ");
            format_expr(out, expr, 0, type_map);
            out.push_str(" {\n");
            for arm in arms {
                out.push_str("    ");
                format_pattern(out, &arm.pattern);
                if let Some(guard) = &arm.guard {
                    out.push_str(" if ");
                    format_expr(out, guard, 0, type_map);
                }
                out.push_str(" => ");
                format_expr(out, &arm.body, 0, type_map);
                out.push_str(",\n");
            }
            out.push('}');
        }
        ir::ExprKind::Binary { op, lhs, rhs } => {
            let (prec, op_str) = binop_info(*op);
            let needs_paren = prec < parent_prec;
            if needs_paren {
                out.push('(');
            }
            format_expr(out, lhs, prec, type_map);
            out.push(' ');
            out.push_str(op_str);
            out.push(' ');
            format_expr(out, rhs, prec + 1, type_map);
            if needs_paren {
                out.push(')');
            }
        }
        ir::ExprKind::Unary { op, expr } => {
            let token = match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "!",
            };
            out.push_str(token);
            format_expr(out, expr, 9, type_map);
        }
        ir::ExprKind::Borrow { mutable, expr } => {
            out.push('&');
            if *mutable {
                out.push_str("mut ");
            }
            format_expr(out, expr, 9, type_map);
        }
        ir::ExprKind::Await { expr } => {
            let needs_paren = matches!(
                expr.kind,
                ir::ExprKind::Binary { .. } | ir::ExprKind::If { .. } | ir::ExprKind::Match { .. }
            );
            out.push_str("await ");
            if needs_paren {
                out.push('(');
            }
            format_expr(out, expr, 9, type_map);
            if needs_paren {
                out.push(')');
            }
        }
        ir::ExprKind::Try { expr } => {
            let needs_paren = matches!(
                expr.kind,
                ir::ExprKind::Binary { .. } | ir::ExprKind::If { .. } | ir::ExprKind::Match { .. }
            );
            if needs_paren {
                out.push('(');
            }
            format_expr(out, expr, 10, type_map);
            if needs_paren {
                out.push(')');
            }
            out.push('?');
        }
        ir::ExprKind::UnsafeBlock { block } => {
            out.push_str("unsafe ");
            format_block(out, block, type_map, 0);
        }
        ir::ExprKind::StructInit { name, fields } => {
            out.push_str(name);
            out.push_str(" {");
            for (idx, (field, expr, _)) in fields.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(field);
                out.push_str(": ");
                format_expr(out, expr, 0, type_map);
            }
            out.push('}');
        }
        ir::ExprKind::FieldAccess { base, field } => {
            format_expr(out, base, 10, type_map);
            out.push('.');
            out.push_str(field);
        }
    }
}

#[derive(Debug, Clone)]
enum RenderedIterable {
    Expr(ir::Expr),
    Range { start: ir::Expr, end: ir::Expr },
}

#[derive(Debug, Clone)]
struct RenderedFor {
    binding: String,
    iterable: RenderedIterable,
    body: ir::Block,
}

fn extract_for_syntax(expr: &ir::Expr) -> Option<RenderedFor> {
    let ir::ExprKind::If {
        cond,
        then_block,
        else_block,
    } = &expr.kind
    else {
        return None;
    };
    if !matches!(cond.kind, ir::ExprKind::Bool(true)) || !is_unit_block(else_block) {
        return None;
    }
    if then_block.stmts.len() != 2 {
        return None;
    }
    let loop_expr = then_block.tail.as_ref()?;
    let ir::ExprKind::Loop { body: loop_body } = &loop_expr.kind else {
        return None;
    };

    extract_range_for(&then_block.stmts, loop_body)
        .or_else(|| extract_vec_for(&then_block.stmts, loop_body))
}

fn extract_vec_for(stmts: &[ir::Stmt], loop_body: &ir::Block) -> Option<RenderedFor> {
    let (iter_name, iterable_expr) = match &stmts[0] {
        ir::Stmt::Let {
            name,
            mutable,
            expr,
            ..
        } if !*mutable => (name.as_str(), expr.clone()),
        _ => return None,
    };
    let index_name = match &stmts[1] {
        ir::Stmt::Let {
            name,
            mutable,
            expr,
            ..
        } if *mutable && matches!(expr.kind, ir::ExprKind::Int(0)) => name.as_str(),
        _ => return None,
    };

    if loop_body.stmts.len() != 1 || loop_body.tail.is_some() {
        return None;
    }
    let ir::Stmt::Expr { expr: match_expr, .. } = &loop_body.stmts[0] else {
        return None;
    };
    let ir::ExprKind::Match { expr: scrutinee, arms } = &match_expr.kind else {
        return None;
    };
    if arms.len() != 2 {
        return None;
    }

    let ir::ExprKind::Call { callee, args } = &scrutinee.kind else {
        return None;
    };
    let ir::ExprKind::Var(callee_name) = &callee.kind else {
        return None;
    };
    if callee_name != "aic_vec_get_intrinsic" || args.len() != 2 {
        return None;
    }
    if !matches!(&args[0].kind, ir::ExprKind::Var(name) if name == iter_name) {
        return None;
    }
    if !matches!(&args[1].kind, ir::ExprKind::Var(name) if name == index_name) {
        return None;
    }

    let arm_some = &arms[0];
    let arm_none = &arms[1];

    let binding = match &arm_some.pattern.kind {
        ir::PatternKind::Variant { name, args } if name == "Some" && args.len() == 1 => {
            match &args[0].kind {
                ir::PatternKind::Var(binding) => binding.clone(),
                _ => return None,
            }
        }
        _ => return None,
    };
    if arm_some.guard.is_some() {
        return None;
    }
    if !matches!(
        arm_none.pattern.kind,
        ir::PatternKind::Variant { ref name, ref args }
            if name == "None" && args.is_empty()
    ) || arm_none.guard.is_some()
        || !is_break_none_expr(&arm_none.body)
    {
        return None;
    }

    let some_then = extract_if_true_then_block(&arm_some.body)?;
    if some_then.stmts.is_empty() || some_then.tail.is_some() {
        return None;
    }
    let ir::Stmt::Assign { target, expr, .. } = &some_then.stmts[0] else {
        return None;
    };
    if target != index_name || !is_increment_expr(expr, index_name) {
        return None;
    }

    let body = ir::Block {
        node: some_then.node,
        stmts: some_then.stmts[1..].to_vec(),
        tail: None,
        span: some_then.span,
    };

    Some(RenderedFor {
        binding,
        iterable: RenderedIterable::Expr(iterable_expr),
        body,
    })
}

fn extract_range_for(stmts: &[ir::Stmt], loop_body: &ir::Block) -> Option<RenderedFor> {
    let (cur_name, start_expr) = match &stmts[0] {
        ir::Stmt::Let {
            name,
            mutable,
            expr,
            ..
        } if *mutable => (name.as_str(), expr.clone()),
        _ => return None,
    };
    let (end_name, end_expr) = match &stmts[1] {
        ir::Stmt::Let {
            name,
            mutable,
            expr,
            ..
        } if !*mutable => (name.as_str(), expr.clone()),
        _ => return None,
    };

    if loop_body.stmts.len() != 1 || loop_body.tail.is_some() {
        return None;
    }
    let ir::Stmt::Expr { expr: if_expr, .. } = &loop_body.stmts[0] else {
        return None;
    };
    let ir::ExprKind::If {
        cond,
        then_block,
        else_block,
    } = &if_expr.kind
    else {
        return None;
    };

    let ir::ExprKind::Binary { op, lhs, rhs } = &cond.kind else {
        return None;
    };
    if *op != BinOp::Lt {
        return None;
    }
    if !matches!(&lhs.kind, ir::ExprKind::Var(name) if name == cur_name) {
        return None;
    }
    if !matches!(&rhs.kind, ir::ExprKind::Var(name) if name == end_name) {
        return None;
    }
    if else_block.stmts.len() != 0
        || else_block.tail.as_ref().is_none_or(|tail| !is_break_none_expr(tail))
    {
        return None;
    }
    if then_block.stmts.len() < 2 || then_block.tail.is_some() {
        return None;
    }

    let binding = match &then_block.stmts[0] {
        ir::Stmt::Let {
            name,
            mutable,
            expr,
            ..
        } if !*mutable && matches!(&expr.kind, ir::ExprKind::Var(source) if source == cur_name) => {
            name.clone()
        }
        _ => return None,
    };
    match &then_block.stmts[1] {
        ir::Stmt::Assign { target, expr, .. }
            if target == cur_name && is_increment_expr(expr, cur_name) => {}
        _ => return None,
    }

    let body = ir::Block {
        node: then_block.node,
        stmts: then_block.stmts[2..].to_vec(),
        tail: None,
        span: then_block.span,
    };

    Some(RenderedFor {
        binding,
        iterable: RenderedIterable::Range {
            start: start_expr,
            end: end_expr,
        },
        body,
    })
}

fn extract_if_true_then_block(expr: &ir::Expr) -> Option<&ir::Block> {
    let ir::ExprKind::If {
        cond,
        then_block,
        else_block,
    } = &expr.kind
    else {
        return None;
    };
    if !matches!(cond.kind, ir::ExprKind::Bool(true)) || !is_unit_block(else_block) {
        return None;
    }
    Some(then_block)
}

fn is_unit_block(block: &ir::Block) -> bool {
    block.stmts.is_empty()
        && block
            .tail
            .as_ref()
            .is_none_or(|tail| matches!(tail.kind, ir::ExprKind::Unit))
}

fn is_break_none_expr(expr: &ir::Expr) -> bool {
    matches!(expr.kind, ir::ExprKind::Break { expr: None })
}

fn is_increment_expr(expr: &ir::Expr, target: &str) -> bool {
    let ir::ExprKind::Binary { op, lhs, rhs } = &expr.kind else {
        return false;
    };
    *op == BinOp::Add
        && matches!(&lhs.kind, ir::ExprKind::Var(name) if name == target)
        && matches!(&rhs.kind, ir::ExprKind::Int(1))
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

fn format_pattern(out: &mut String, pattern: &ir::Pattern) {
    match &pattern.kind {
        ir::PatternKind::Wildcard => out.push('_'),
        ir::PatternKind::Var(v) => out.push_str(v),
        ir::PatternKind::Int(v) => out.push_str(&v.to_string()),
        ir::PatternKind::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        ir::PatternKind::Unit => out.push_str("()"),
        ir::PatternKind::Or { patterns } => {
            for (idx, part) in patterns.iter().enumerate() {
                if idx > 0 {
                    out.push_str(" | ");
                }
                format_pattern(out, part);
            }
        }
        ir::PatternKind::Variant { name, args } => {
            out.push_str(name);
            if !args.is_empty() {
                out.push('(');
                for (idx, arg) in args.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(", ");
                    }
                    format_pattern(out, arg);
                }
                out.push(')');
            }
        }
    }
}

fn type_map(program: &ir::Program) -> BTreeMap<ir::TypeId, String> {
    program
        .types
        .iter()
        .map(|ty| (ty.id, ty.repr.clone()))
        .collect()
}

fn binop_info(op: BinOp) -> (u8, &'static str) {
    match op {
        BinOp::Or => (1, "||"),
        BinOp::And => (2, "&&"),
        BinOp::Eq => (3, "=="),
        BinOp::Ne => (3, "!="),
        BinOp::Lt => (4, "<"),
        BinOp::Le => (4, "<="),
        BinOp::Gt => (4, ">"),
        BinOp::Ge => (4, ">="),
        BinOp::Add => (5, "+"),
        BinOp::Sub => (5, "-"),
        BinOp::Mul => (6, "*"),
        BinOp::Div => (6, "/"),
        BinOp::Mod => (6, "%"),
    }
}

#[cfg(test)]
mod tests {
    use crate::{ir_builder::build, parser::parse};

    use super::format_program;

    #[test]
    fn deterministic_formatting() {
        let src = "import std.io;\nfn main()->Int effects{io}{print_int(1);0}";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty());
        let ir = build(&program.expect("program"));
        let a = format_program(&ir);
        let b = format_program(&ir);
        assert_eq!(a, b);
    }
}
