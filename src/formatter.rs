use std::collections::BTreeMap;

use crate::ast::{decode_internal_const, decode_internal_type_alias, BinOp, UnaryOp};
use crate::ir;

const TUPLE_INTERNAL_NAME: &str = "Tuple";

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
            ir::Item::Trait(t) => format_trait(&mut out, t, &type_map),
            ir::Item::Impl(i) => format_impl(&mut out, i, &type_map),
        }
    }

    out
}

fn display_type(type_map: &BTreeMap<ir::TypeId, String>, ty: &ir::TypeId) -> String {
    type_map
        .get(ty)
        .map(|repr| format_type_repr(repr))
        .unwrap_or_else(|| "<?>".to_string())
}

fn format_type_repr(repr: &str) -> String {
    let repr = repr.trim();
    let Some(args) = extract_generic_args(repr) else {
        return ir::canonical_primitive_type_name(repr).to_string();
    };
    let rendered = args
        .iter()
        .map(|arg| format_type_repr(arg))
        .collect::<Vec<_>>();
    let base = ir::canonical_primitive_type_name(base_type_name(repr));
    if base == TUPLE_INTERNAL_NAME {
        if rendered.len() == 1 {
            format!("({},)", rendered[0])
        } else {
            format!("({})", rendered.join(", "))
        }
    } else {
        format!("{base}[{}]", rendered.join(", "))
    }
}

fn base_type_name(ty: &str) -> &str {
    ty.split_once('[').map(|(base, _)| base).unwrap_or(ty)
}

fn extract_generic_args(ty: &str) -> Option<Vec<String>> {
    let start = ty.find('[')?;
    if !ty.ends_with(']') || start + 1 >= ty.len() {
        return None;
    }
    let inner = &ty[start + 1..ty.len() - 1];
    Some(split_top_level(inner))
}

fn split_top_level(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0_i32;
    let mut start = 0;
    for (idx, ch) in input.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(input[start..idx].trim().to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let tail = input[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
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
                .map(|p| format!("{}: {}", p.name, display_type(type_map, &p.ty)))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str(") -> ");
        out.push_str(&display_type(type_map, &f.ret_type));
        out.push_str(";\n");
        return;
    }

    if f.is_intrinsic {
        out.push_str("intrinsic fn ");
        out.push_str(&f.name);
        format_generic_params(out, &f.generics);
        out.push('(');
        out.push_str(
            &f.params
                .iter()
                .map(|p| format!("{}: {}", p.name, display_type(type_map, &p.ty)))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str(") -> ");
        out.push_str(&display_type(type_map, &f.ret_type));
        if !f.effects.is_empty() {
            let mut effects = f.effects.clone();
            effects.sort();
            effects.dedup();
            out.push_str(" effects { ");
            out.push_str(&effects.join(", "));
            out.push_str(" }");
        }
        if !f.capabilities.is_empty() {
            let mut capabilities = f.capabilities.clone();
            capabilities.sort();
            capabilities.dedup();
            out.push_str(" capabilities { ");
            out.push_str(&capabilities.join(", "));
            out.push_str(" }");
        }
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
            .map(|p| format!("{}: {}", p.name, display_type(type_map, &p.ty)))
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str(") -> ");
    out.push_str(&display_type(type_map, &f.ret_type));

    if !f.effects.is_empty() {
        let mut effects = f.effects.clone();
        effects.sort();
        effects.dedup();
        out.push_str(" effects { ");
        out.push_str(&effects.join(", "));
        out.push_str(" }");
    }
    if !f.capabilities.is_empty() {
        let mut capabilities = f.capabilities.clone();
        capabilities.sort();
        capabilities.dedup();
        out.push_str(" capabilities { ");
        out.push_str(&capabilities.join(", "));
        out.push_str(" }");
    }

    if let Some(req) = &f.requires {
        out.push_str(" requires ");
        format_expr(out, req, 0, type_map, 0);
    }

    if let Some(ens) = &f.ensures {
        out.push_str(" ensures ");
        format_expr(out, ens, 0, type_map, 0);
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
    out.push_str(&display_type(type_map, &f.ret_type));
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
    out.push_str(&display_type(type_map, &f.ret_type));
    out.push_str(" = ");
    if let Some(expr) = &f.body.tail {
        format_expr(out, expr, 0, type_map, 0);
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
        out.push_str(&display_type(type_map, &field.ty));
        out.push_str(",\n");
    }
    out.push('}');
    if let Some(inv) = &s.invariant {
        out.push_str(" invariant ");
        format_expr(out, inv, 0, type_map, 0);
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
            out.push_str(&display_type(type_map, &ty));
            out.push(')');
        }
        out.push_str(",\n");
    }
    out.push_str("}\n");
}

fn format_trait(out: &mut String, t: &ir::TraitDef, type_map: &BTreeMap<ir::TypeId, String>) {
    out.push_str("trait ");
    out.push_str(&t.name);
    format_generic_params(out, &t.generics);
    if t.methods.is_empty() {
        out.push_str(";\n");
        return;
    }
    out.push_str(" {\n");
    for method in &t.methods {
        out.push_str("    ");
        format_method_signature(out, method, type_map);
        out.push_str(";\n");
    }
    out.push_str("}\n");
}

fn format_impl(out: &mut String, i: &ir::ImplDef, type_map: &BTreeMap<ir::TypeId, String>) {
    if i.is_inherent {
        out.push_str("impl ");
        if let Some(target) = i.target.as_ref() {
            out.push_str(&display_type(type_map, target));
        } else {
            out.push_str(&i.trait_name);
        }
        out.push_str(" {\n");
        for method in &i.methods {
            out.push_str("    ");
            format_method_signature(out, method, type_map);
            out.push(' ');
            format_block(out, &method.body, type_map, 4);
            out.push('\n');
        }
        out.push_str("}\n");
        return;
    }

    out.push_str("impl ");
    out.push_str(&i.trait_name);
    if !i.trait_args.is_empty() {
        out.push('[');
        out.push_str(
            &i.trait_args
                .iter()
                .map(|ty| display_type(type_map, ty))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push(']');
    }
    if i.methods.is_empty() {
        out.push_str(";\n");
        return;
    }
    out.push_str(" {\n");
    for method in &i.methods {
        out.push_str("    ");
        format_method_signature(out, method, type_map);
        out.push(' ');
        format_block(out, &method.body, type_map, 4);
        out.push('\n');
    }
    out.push_str("}\n");
}

fn format_method_signature(
    out: &mut String,
    method: &ir::Function,
    type_map: &BTreeMap<ir::TypeId, String>,
) {
    out.push_str("fn ");
    let method_name = method
        .name
        .rsplit("::")
        .next()
        .unwrap_or(method.name.as_str());
    out.push_str(method_name);
    format_generic_params(out, &method.generics);
    out.push('(');
    out.push_str(
        &method
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, display_type(type_map, &p.ty)))
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str(") -> ");
    out.push_str(&display_type(type_map, &method.ret_type));
    if !method.effects.is_empty() {
        let mut effects = method.effects.clone();
        effects.sort();
        effects.dedup();
        out.push_str(" effects { ");
        out.push_str(&effects.join(", "));
        out.push_str(" }");
    }
    if !method.capabilities.is_empty() {
        let mut capabilities = method.capabilities.clone();
        capabilities.sort();
        capabilities.dedup();
        out.push_str(" capabilities { ");
        out.push_str(&capabilities.join(", "));
        out.push_str(" }");
    }
    if let Some(req) = &method.requires {
        out.push_str(" requires ");
        format_expr(out, req, 0, type_map, 0);
    }
    if let Some(ens) = &method.ensures {
        out.push_str(" ensures ");
        format_expr(out, ens, 0, type_map, 0);
    }
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
                    out.push_str(&display_type(type_map, ty));
                }
                out.push_str(" = ");
                format_expr(out, expr, 0, type_map, indent + 4);
                out.push_str(";\n");
            }
            ir::Stmt::Assign { target, expr, .. } => {
                out.push_str(target);
                out.push_str(" = ");
                format_expr(out, expr, 0, type_map, indent + 4);
                out.push_str(";\n");
            }
            ir::Stmt::Expr { expr, .. } => {
                format_expr(out, expr, 0, type_map, indent + 4);
                out.push_str(";\n");
            }
            ir::Stmt::Return { expr, .. } => {
                out.push_str("return");
                if let Some(expr) = expr {
                    out.push(' ');
                    format_expr(out, expr, 0, type_map, indent + 4);
                }
                out.push_str(";\n");
            }
            ir::Stmt::Assert { expr, message, .. } => {
                out.push_str("assert ");
                format_expr(out, expr, 0, type_map, indent + 4);
                out.push_str("; // ");
                out.push_str(message);
                out.push('\n');
            }
        }
    }

    if let Some(tail) = &block.tail {
        out.push_str(&" ".repeat(indent + 4));
        format_expr(out, tail, 0, type_map, indent + 4);
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
    indent: usize,
) {
    if let Some(rendered_for) = extract_for_syntax(expr) {
        out.push_str("for ");
        out.push_str(&rendered_for.binding);
        out.push_str(" in ");
        match rendered_for.iterable {
            RenderedIterable::Expr(iterable) => format_expr(out, &iterable, 0, type_map, indent),
            RenderedIterable::Range { start, end } => {
                format_expr(out, &start, 0, type_map, indent);
                out.push_str("..");
                format_expr(out, &end, 0, type_map, indent);
            }
        }
        out.push(' ');
        format_block(out, &rendered_for.body, type_map, indent);
        return;
    }

    match &expr.kind {
        ir::ExprKind::Int(v) => {
            if let Some(meta) = expr.int_literal_metadata() {
                out.push_str(&meta.raw_literal_text);
                out.push_str(&meta.suffix_text);
            } else {
                out.push_str(&v.to_string());
            }
        }
        ir::ExprKind::Float(v) => out.push_str(&render_float_literal(*v)),
        ir::ExprKind::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        ir::ExprKind::Char(v) => out.push_str(&format!("{:?}", v)),
        ir::ExprKind::String(v) => {
            out.push('"');
            out.push_str(&v.replace('\\', "\\\\").replace('"', "\\\""));
            out.push('"');
        }
        ir::ExprKind::Unit => out.push_str("()"),
        ir::ExprKind::Var(v) => out.push_str(v),
        ir::ExprKind::Call {
            callee,
            args,
            arg_names,
        } => {
            format_expr(out, callee, 10, type_map, indent);
            out.push('(');
            for (idx, arg) in args.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                if let Some(name) = arg_names.get(idx).and_then(|name| name.as_deref()) {
                    out.push_str(name);
                    out.push_str(": ");
                }
                format_expr(out, arg, 0, type_map, indent);
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
                    out.push_str(&display_type(type_map, &ty));
                }
            }
            out.push('|');
            out.push_str(" -> ");
            out.push_str(&display_type(type_map, ret_type));
            out.push(' ');
            format_block(out, body, type_map, indent);
        }
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            out.push_str("if ");
            format_expr(out, cond, 0, type_map, indent);
            out.push(' ');
            format_block(out, then_block, type_map, indent);
            out.push_str(" else ");
            format_block(out, else_block, type_map, indent);
        }
        ir::ExprKind::While { cond, body } => {
            out.push_str("while ");
            format_expr(out, cond, 0, type_map, indent);
            out.push(' ');
            format_block(out, body, type_map, indent);
        }
        ir::ExprKind::Loop { body } => {
            out.push_str("loop ");
            format_block(out, body, type_map, indent);
        }
        ir::ExprKind::Break { expr } => {
            out.push_str("break");
            if let Some(expr) = expr {
                out.push(' ');
                format_expr(out, expr, 0, type_map, indent);
            }
        }
        ir::ExprKind::Continue => out.push_str("continue"),
        ir::ExprKind::Match { expr, arms } => {
            out.push_str("match ");
            format_expr(out, expr, 0, type_map, indent);
            out.push_str(" {\n");
            for arm in arms {
                out.push_str(&" ".repeat(indent + 4));
                format_pattern(out, &arm.pattern);
                if let Some(guard) = &arm.guard {
                    out.push_str(" if ");
                    format_expr(out, guard, 0, type_map, indent + 4);
                }
                out.push_str(" => ");
                format_expr(out, &arm.body, 0, type_map, indent + 4);
                out.push_str(",\n");
            }
            out.push_str(&" ".repeat(indent));
            out.push('}');
        }
        ir::ExprKind::Binary { op, lhs, rhs } => {
            let (prec, op_str) = binop_info(*op);
            let needs_paren = prec < parent_prec;
            if needs_paren {
                out.push('(');
            }
            format_expr(out, lhs, prec, type_map, indent);
            out.push(' ');
            out.push_str(op_str);
            out.push(' ');
            format_expr(out, rhs, prec + 1, type_map, indent);
            if needs_paren {
                out.push(')');
            }
        }
        ir::ExprKind::Unary { op, expr } => {
            let token = match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "!",
                UnaryOp::BitNot => "~",
            };
            out.push_str(token);
            format_expr(out, expr, 9, type_map, indent);
        }
        ir::ExprKind::Borrow { mutable, expr } => {
            out.push('&');
            if *mutable {
                out.push_str("mut ");
            }
            format_expr(out, expr, 9, type_map, indent);
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
            format_expr(out, expr, 9, type_map, indent);
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
            format_expr(out, expr, 10, type_map, indent);
            if needs_paren {
                out.push(')');
            }
            out.push('?');
        }
        ir::ExprKind::UnsafeBlock { block } => {
            out.push_str("unsafe ");
            format_block(out, block, type_map, indent);
        }
        ir::ExprKind::StructInit { name, fields } => {
            if name == TUPLE_INTERNAL_NAME {
                let mut indexed = fields
                    .iter()
                    .filter_map(|(field, expr, _)| {
                        field.parse::<usize>().ok().map(|idx| (idx, expr))
                    })
                    .collect::<Vec<_>>();
                indexed.sort_by_key(|(idx, _)| *idx);
                out.push('(');
                for (idx, (_field_idx, expr)) in indexed.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(", ");
                    }
                    format_expr(out, expr, 0, type_map, indent);
                }
                if indexed.len() == 1 {
                    out.push(',');
                }
                out.push(')');
            } else {
                out.push_str(name);
                out.push_str(" {");
                for (idx, (field, expr, _)) in fields.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(field);
                    out.push_str(": ");
                    format_expr(out, expr, 0, type_map, indent);
                }
                out.push('}');
            }
        }
        ir::ExprKind::FieldAccess { base, field } => {
            format_expr(out, base, 10, type_map, indent);
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
    let ir::Stmt::Expr {
        expr: match_expr, ..
    } = &loop_body.stmts[0]
    else {
        return None;
    };
    let ir::ExprKind::Match {
        expr: scrutinee,
        arms,
    } = &match_expr.kind
    else {
        return None;
    };
    if arms.len() != 2 {
        return None;
    }

    let ir::ExprKind::Call { callee, args, .. } = &scrutinee.kind else {
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
        || else_block
            .tail
            .as_ref()
            .is_none_or(|tail| !is_break_none_expr(tail))
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
        ir::PatternKind::Int(v) => {
            if let Some(meta) = pattern.int_literal_metadata() {
                out.push_str(&meta.raw_literal_text);
                out.push_str(&meta.suffix_text);
            } else {
                out.push_str(&v.to_string());
            }
        }
        ir::PatternKind::Char(v) => out.push_str(&format!("{:?}", v)),
        ir::PatternKind::String(v) => {
            out.push('"');
            out.push_str(&v.replace('\\', "\\\\").replace('"', "\\\""));
            out.push('"');
        }
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
            if name == TUPLE_INTERNAL_NAME {
                out.push('(');
                for (idx, arg) in args.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(", ");
                    }
                    format_pattern(out, arg);
                }
                if args.len() == 1 {
                    out.push(',');
                }
                out.push(')');
            } else {
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
        ir::PatternKind::Struct {
            name,
            fields,
            has_rest,
        } => {
            out.push_str(name);
            out.push_str(" { ");
            for (idx, field) in fields.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                if matches!(&field.pattern.kind, ir::PatternKind::Var(binding) if binding == &field.name)
                {
                    out.push_str(&field.name);
                } else {
                    out.push_str(&field.name);
                    out.push_str(": ");
                    format_pattern(out, &field.pattern);
                }
            }
            if *has_rest {
                if !fields.is_empty() {
                    out.push_str(", ");
                }
                out.push_str("..");
            }
            out.push_str(" }");
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
        BinOp::BitOr => (3, "|"),
        BinOp::BitXor => (4, "^"),
        BinOp::BitAnd => (5, "&"),
        BinOp::Eq => (6, "=="),
        BinOp::Ne => (6, "!="),
        BinOp::Lt => (7, "<"),
        BinOp::Le => (7, "<="),
        BinOp::Gt => (7, ">"),
        BinOp::Ge => (7, ">="),
        BinOp::Shl => (8, "<<"),
        BinOp::Shr => (8, ">>"),
        BinOp::Ushr => (8, ">>>"),
        BinOp::Add => (9, "+"),
        BinOp::Sub => (9, "-"),
        BinOp::Mul => (10, "*"),
        BinOp::Div => (10, "/"),
        BinOp::Mod => (10, "%"),
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

    #[test]
    fn canonicalizes_uint_alias_to_usize_in_formatted_types() {
        let src = r#"
type Counter = UInt;

fn bump(index: USize, delta: UInt, signed: ISize) -> UInt {
    let _index: USize = index;
    let _delta: UInt = delta;
    let _signed: ISize = signed;
    delta
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let ir = build(&program.expect("program"));
        let formatted = format_program(&ir);
        assert!(formatted.contains("ISize"), "formatted={formatted}");
        assert!(formatted.contains("USize"), "formatted={formatted}");
        assert!(!formatted.contains("UInt"), "formatted={formatted}");
    }

    #[test]
    fn formats_nested_if_and_match_with_block_indentation() {
        let src = r#"module sample.main;

import std.io;

fn maybe_even(x: Int) -> Option[Int] {
    if x % 2 == 0 {
    Some(x)
} else {
    None()
}
}

fn main() -> Int effects { io } capabilities { io } {
    let v = maybe_even(10);
    let out = match v {
    Some(n) => n,
    None => 0,
};
    print_int(out);
    0
}
"#;
        let expected = r#"module sample.main;

import std.io;

fn maybe_even(x: Int) -> Option[Int] {
    if x % 2 == 0 {
        Some(x)
    } else {
        None()
    }
}

fn main() -> Int effects { io } capabilities { io } {
    let v = maybe_even(10);
    let out = match v {
        Some(n) => n,
        None => 0,
    };
    print_int(out);
    0
}
"#;

        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let ir = build(&program.expect("program"));
        assert_eq!(format_program(&ir), expected);
    }
}
