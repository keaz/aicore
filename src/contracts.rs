use crate::ast::{BinOp, UnaryOp};
use crate::diagnostics::Diagnostic;
use crate::ir;

pub fn verify_static(program: &ir::Program, file: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for item in &program.items {
        match item {
            ir::Item::Function(f) => {
                if let Some(req) = &f.requires {
                    if let Some(value) = eval_const(req) {
                        if matches!(value, Value::Bool(false)) {
                            diagnostics.push(
                                Diagnostic::error(
                                    "E4001",
                                    format!("requires contract for '{}' is always false", f.name),
                                    file,
                                    req.span,
                                )
                                .with_help("fix the contract or function preconditions"),
                            );
                        }
                    }
                }
                if let Some(ens) = &f.ensures {
                    if let Some(value) = eval_const(ens) {
                        if matches!(value, Value::Bool(false)) {
                            diagnostics.push(
                                Diagnostic::error(
                                    "E4002",
                                    format!("ensures contract for '{}' is always false", f.name),
                                    file,
                                    ens.span,
                                )
                                .with_help("fix the postcondition expression"),
                            );
                        }
                    }
                    if block_contains_return(&f.body) {
                        diagnostics.push(
                            Diagnostic::error(
                                "E4003",
                                format!(
                                    "ensures in '{}' currently supports tail-return style, not explicit return statements",
                                    f.name
                                ),
                                file,
                                f.span,
                            )
                            .with_help("rewrite function to use tail expression instead of `return`"),
                        );
                    }
                }
            }
            ir::Item::Struct(s) => {
                if let Some(inv) = &s.invariant {
                    if let Some(value) = eval_const(inv) {
                        if matches!(value, Value::Bool(false)) {
                            diagnostics.push(
                                Diagnostic::error(
                                    "E4004",
                                    format!("invariant for struct '{}' is always false", s.name),
                                    file,
                                    inv.span,
                                )
                                .with_help("fix the invariant expression"),
                            );
                        }
                    }
                }
            }
            ir::Item::Enum(_) => {}
        }
    }

    diagnostics
}

pub fn lower_runtime_asserts(program: &ir::Program) -> ir::Program {
    let mut lowered = program.clone();
    let mut alloc = IdAlloc::from_program(&lowered);

    for item in &mut lowered.items {
        if let ir::Item::Function(func) = item {
            if let Some(req) = &func.requires {
                let req_clone = clone_expr(req, &mut alloc);
                let stmt = ir::Stmt::Assert {
                    expr: req_clone,
                    message: format!("requires failed in {}", func.name),
                    span: func.span,
                };
                let mut stmts = vec![stmt];
                stmts.extend(func.body.stmts.clone());
                func.body.stmts = stmts;
            }

            if let Some(ens) = &func.ensures {
                if let Some(tail) = func.body.tail.take() {
                    let result_sym = ir::SymbolId(alloc.next_symbol());
                    let result_name = "__aic_result".to_string();
                    let tail_expr = clone_expr(&tail, &mut alloc);
                    let let_stmt = ir::Stmt::Let {
                        symbol: result_sym,
                        name: result_name.clone(),
                        ty: Some(func.ret_type),
                        expr: tail_expr,
                        span: func.span,
                    };
                    func.body.stmts.push(let_stmt);

                    let ensures_expr = substitute_result_var(ens, &result_name, &mut alloc);
                    let assert_stmt = ir::Stmt::Assert {
                        expr: ensures_expr,
                        message: format!("ensures failed in {}", func.name),
                        span: func.span,
                    };
                    func.body.stmts.push(assert_stmt);

                    func.body.tail = Some(Box::new(ir::Expr {
                        node: ir::NodeId(alloc.next_node()),
                        kind: ir::ExprKind::Var(result_name),
                        span: func.span,
                    }));
                }
            }
        }
    }

    lowered
}

#[derive(Debug, Clone)]
enum Value {
    Int(i64),
    Bool(bool),
}

fn eval_const(expr: &ir::Expr) -> Option<Value> {
    match &expr.kind {
        ir::ExprKind::Int(v) => Some(Value::Int(*v)),
        ir::ExprKind::Bool(v) => Some(Value::Bool(*v)),
        ir::ExprKind::Unary { op, expr } => {
            let v = eval_const(expr)?;
            match (op, v) {
                (UnaryOp::Neg, Value::Int(i)) => Some(Value::Int(-i)),
                (UnaryOp::Not, Value::Bool(b)) => Some(Value::Bool(!b)),
                _ => None,
            }
        }
        ir::ExprKind::Binary { op, lhs, rhs } => {
            let l = eval_const(lhs)?;
            let r = eval_const(rhs)?;
            match (op, l, r) {
                (BinOp::Add, Value::Int(a), Value::Int(b)) => Some(Value::Int(a + b)),
                (BinOp::Sub, Value::Int(a), Value::Int(b)) => Some(Value::Int(a - b)),
                (BinOp::Mul, Value::Int(a), Value::Int(b)) => Some(Value::Int(a * b)),
                (BinOp::Div, Value::Int(a), Value::Int(b)) => Some(Value::Int(a / b)),
                (BinOp::Mod, Value::Int(a), Value::Int(b)) => Some(Value::Int(a % b)),
                (BinOp::Eq, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a == b)),
                (BinOp::Eq, Value::Bool(a), Value::Bool(b)) => Some(Value::Bool(a == b)),
                (BinOp::Ne, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a != b)),
                (BinOp::Ne, Value::Bool(a), Value::Bool(b)) => Some(Value::Bool(a != b)),
                (BinOp::Lt, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a < b)),
                (BinOp::Le, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a <= b)),
                (BinOp::Gt, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a > b)),
                (BinOp::Ge, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a >= b)),
                (BinOp::And, Value::Bool(a), Value::Bool(b)) => Some(Value::Bool(a && b)),
                (BinOp::Or, Value::Bool(a), Value::Bool(b)) => Some(Value::Bool(a || b)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn block_contains_return(block: &ir::Block) -> bool {
    for stmt in &block.stmts {
        match stmt {
            ir::Stmt::Return { .. } => return true,
            ir::Stmt::Expr { expr, .. } => {
                if expr_contains_return(expr) {
                    return true;
                }
            }
            _ => {}
        }
    }
    if let Some(tail) = &block.tail {
        expr_contains_return(tail)
    } else {
        false
    }
}

fn expr_contains_return(expr: &ir::Expr) -> bool {
    match &expr.kind {
        ir::ExprKind::If {
            then_block,
            else_block,
            ..
        } => block_contains_return(then_block) || block_contains_return(else_block),
        ir::ExprKind::Match { arms, .. } => arms.iter().any(|a| expr_contains_return(&a.body)),
        _ => false,
    }
}

struct IdAlloc {
    next_symbol: u32,
    next_node: u32,
}

impl IdAlloc {
    fn from_program(program: &ir::Program) -> Self {
        let next_symbol = program.symbols.iter().map(|s| s.id.0).max().unwrap_or(0) + 1;
        let next_node = max_node(program) + 1;
        Self {
            next_symbol,
            next_node,
        }
    }

    fn next_symbol(&mut self) -> u32 {
        let out = self.next_symbol;
        self.next_symbol += 1;
        out
    }

    fn next_node(&mut self) -> u32 {
        let out = self.next_node;
        self.next_node += 1;
        out
    }
}

fn max_node(program: &ir::Program) -> u32 {
    let mut max = 0;
    for item in &program.items {
        if let ir::Item::Function(func) = item {
            max = max.max(max_node_block(&func.body));
            if let Some(req) = &func.requires {
                max = max.max(max_node_expr(req));
            }
            if let Some(ens) = &func.ensures {
                max = max.max(max_node_expr(ens));
            }
        }
        if let ir::Item::Struct(s) = item {
            if let Some(inv) = &s.invariant {
                max = max.max(max_node_expr(inv));
            }
        }
    }
    max
}

fn max_node_block(block: &ir::Block) -> u32 {
    let mut max = block.node.0;
    for stmt in &block.stmts {
        match stmt {
            ir::Stmt::Let { expr, .. } => max = max.max(max_node_expr(expr)),
            ir::Stmt::Expr { expr, .. } => max = max.max(max_node_expr(expr)),
            ir::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    max = max.max(max_node_expr(expr));
                }
            }
            ir::Stmt::Assert { expr, .. } => max = max.max(max_node_expr(expr)),
        }
    }
    if let Some(tail) = &block.tail {
        max = max.max(max_node_expr(tail));
    }
    max
}

fn max_node_expr(expr: &ir::Expr) -> u32 {
    let mut max = expr.node.0;
    match &expr.kind {
        ir::ExprKind::Call { callee, args } => {
            max = max.max(max_node_expr(callee));
            for arg in args {
                max = max.max(max_node_expr(arg));
            }
        }
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            max = max.max(max_node_expr(cond));
            max = max.max(max_node_block(then_block));
            max = max.max(max_node_block(else_block));
        }
        ir::ExprKind::Match { expr, arms } => {
            max = max.max(max_node_expr(expr));
            for arm in arms {
                max = max.max(max_node_pattern(&arm.pattern));
                max = max.max(max_node_expr(&arm.body));
            }
        }
        ir::ExprKind::Binary { lhs, rhs, .. } => {
            max = max.max(max_node_expr(lhs));
            max = max.max(max_node_expr(rhs));
        }
        ir::ExprKind::Unary { expr, .. } => {
            max = max.max(max_node_expr(expr));
        }
        ir::ExprKind::StructInit { fields, .. } => {
            for (_, expr, _) in fields {
                max = max.max(max_node_expr(expr));
            }
        }
        ir::ExprKind::FieldAccess { base, .. } => {
            max = max.max(max_node_expr(base));
        }
        _ => {}
    }
    max
}

fn max_node_pattern(pattern: &ir::Pattern) -> u32 {
    let mut max = pattern.node.0;
    if let ir::PatternKind::Variant { args, .. } = &pattern.kind {
        for arg in args {
            max = max.max(max_node_pattern(arg));
        }
    }
    max
}

fn clone_expr(expr: &ir::Expr, alloc: &mut IdAlloc) -> ir::Expr {
    let kind = match &expr.kind {
        ir::ExprKind::Int(v) => ir::ExprKind::Int(*v),
        ir::ExprKind::Bool(v) => ir::ExprKind::Bool(*v),
        ir::ExprKind::String(v) => ir::ExprKind::String(v.clone()),
        ir::ExprKind::Unit => ir::ExprKind::Unit,
        ir::ExprKind::Var(v) => ir::ExprKind::Var(v.clone()),
        ir::ExprKind::Call { callee, args } => ir::ExprKind::Call {
            callee: Box::new(clone_expr(callee, alloc)),
            args: args.iter().map(|a| clone_expr(a, alloc)).collect(),
        },
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => ir::ExprKind::If {
            cond: Box::new(clone_expr(cond, alloc)),
            then_block: clone_block(then_block, alloc),
            else_block: clone_block(else_block, alloc),
        },
        ir::ExprKind::Match { expr, arms } => ir::ExprKind::Match {
            expr: Box::new(clone_expr(expr, alloc)),
            arms: arms
                .iter()
                .map(|arm| ir::MatchArm {
                    pattern: clone_pattern(&arm.pattern, alloc),
                    body: clone_expr(&arm.body, alloc),
                    span: arm.span,
                })
                .collect(),
        },
        ir::ExprKind::Binary { op, lhs, rhs } => ir::ExprKind::Binary {
            op: *op,
            lhs: Box::new(clone_expr(lhs, alloc)),
            rhs: Box::new(clone_expr(rhs, alloc)),
        },
        ir::ExprKind::Unary { op, expr } => ir::ExprKind::Unary {
            op: *op,
            expr: Box::new(clone_expr(expr, alloc)),
        },
        ir::ExprKind::StructInit { name, fields } => ir::ExprKind::StructInit {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, expr, span)| (name.clone(), clone_expr(expr, alloc), *span))
                .collect(),
        },
        ir::ExprKind::FieldAccess { base, field } => ir::ExprKind::FieldAccess {
            base: Box::new(clone_expr(base, alloc)),
            field: field.clone(),
        },
    };

    ir::Expr {
        node: ir::NodeId(alloc.next_node()),
        kind,
        span: expr.span,
    }
}

fn clone_block(block: &ir::Block, alloc: &mut IdAlloc) -> ir::Block {
    ir::Block {
        node: ir::NodeId(alloc.next_node()),
        stmts: block
            .stmts
            .iter()
            .map(|stmt| match stmt {
                ir::Stmt::Let {
                    symbol,
                    name,
                    ty,
                    expr,
                    span,
                } => ir::Stmt::Let {
                    symbol: *symbol,
                    name: name.clone(),
                    ty: *ty,
                    expr: clone_expr(expr, alloc),
                    span: *span,
                },
                ir::Stmt::Expr { expr, span } => ir::Stmt::Expr {
                    expr: clone_expr(expr, alloc),
                    span: *span,
                },
                ir::Stmt::Return { expr, span } => ir::Stmt::Return {
                    expr: expr.as_ref().map(|e| clone_expr(e, alloc)),
                    span: *span,
                },
                ir::Stmt::Assert {
                    expr,
                    message,
                    span,
                } => ir::Stmt::Assert {
                    expr: clone_expr(expr, alloc),
                    message: message.clone(),
                    span: *span,
                },
            })
            .collect(),
        tail: block.tail.as_ref().map(|e| Box::new(clone_expr(e, alloc))),
        span: block.span,
    }
}

fn clone_pattern(pattern: &ir::Pattern, alloc: &mut IdAlloc) -> ir::Pattern {
    let kind = match &pattern.kind {
        ir::PatternKind::Wildcard => ir::PatternKind::Wildcard,
        ir::PatternKind::Var(v) => ir::PatternKind::Var(v.clone()),
        ir::PatternKind::Int(v) => ir::PatternKind::Int(*v),
        ir::PatternKind::Bool(v) => ir::PatternKind::Bool(*v),
        ir::PatternKind::Unit => ir::PatternKind::Unit,
        ir::PatternKind::Variant { name, args } => ir::PatternKind::Variant {
            name: name.clone(),
            args: args.iter().map(|a| clone_pattern(a, alloc)).collect(),
        },
    };
    ir::Pattern {
        node: ir::NodeId(alloc.next_node()),
        kind,
        span: pattern.span,
    }
}

fn substitute_result_var(expr: &ir::Expr, result_name: &str, alloc: &mut IdAlloc) -> ir::Expr {
    let kind = match &expr.kind {
        ir::ExprKind::Var(name) if name == "result" => ir::ExprKind::Var(result_name.to_string()),
        ir::ExprKind::Int(v) => ir::ExprKind::Int(*v),
        ir::ExprKind::Bool(v) => ir::ExprKind::Bool(*v),
        ir::ExprKind::String(v) => ir::ExprKind::String(v.clone()),
        ir::ExprKind::Unit => ir::ExprKind::Unit,
        ir::ExprKind::Var(v) => ir::ExprKind::Var(v.clone()),
        ir::ExprKind::Call { callee, args } => ir::ExprKind::Call {
            callee: Box::new(substitute_result_var(callee, result_name, alloc)),
            args: args
                .iter()
                .map(|a| substitute_result_var(a, result_name, alloc))
                .collect(),
        },
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => ir::ExprKind::If {
            cond: Box::new(substitute_result_var(cond, result_name, alloc)),
            then_block: clone_block(then_block, alloc),
            else_block: clone_block(else_block, alloc),
        },
        ir::ExprKind::Match { expr, arms } => ir::ExprKind::Match {
            expr: Box::new(substitute_result_var(expr, result_name, alloc)),
            arms: arms
                .iter()
                .map(|arm| ir::MatchArm {
                    pattern: clone_pattern(&arm.pattern, alloc),
                    body: substitute_result_var(&arm.body, result_name, alloc),
                    span: arm.span,
                })
                .collect(),
        },
        ir::ExprKind::Binary { op, lhs, rhs } => ir::ExprKind::Binary {
            op: *op,
            lhs: Box::new(substitute_result_var(lhs, result_name, alloc)),
            rhs: Box::new(substitute_result_var(rhs, result_name, alloc)),
        },
        ir::ExprKind::Unary { op, expr } => ir::ExprKind::Unary {
            op: *op,
            expr: Box::new(substitute_result_var(expr, result_name, alloc)),
        },
        ir::ExprKind::StructInit { name, fields } => ir::ExprKind::StructInit {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, expr, span)| {
                    (
                        name.clone(),
                        substitute_result_var(expr, result_name, alloc),
                        *span,
                    )
                })
                .collect(),
        },
        ir::ExprKind::FieldAccess { base, field } => ir::ExprKind::FieldAccess {
            base: Box::new(substitute_result_var(base, result_name, alloc)),
            field: field.clone(),
        },
    };

    ir::Expr {
        node: ir::NodeId(alloc.next_node()),
        kind,
        span: expr.span,
    }
}

#[cfg(test)]
mod tests {
    use crate::ir;
    use crate::{ir_builder::build, parser::parse};

    use super::{lower_runtime_asserts, verify_static};

    #[test]
    fn static_contract_false_is_reported() {
        let src = "fn f() -> Int ensures false { 1 }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let diags = verify_static(&ir, "test.aic");
        assert!(diags.iter().any(|d| d.code == "E4002"));
    }

    #[test]
    fn lowering_inserts_asserts() {
        let src = "fn f(x: Int) -> Int requires x > 0 ensures result > 0 { x }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let lowered = lower_runtime_asserts(&ir);
        let func = match &lowered.items[0] {
            ir::Item::Function(f) => f,
            _ => panic!(),
        };
        assert!(func
            .body
            .stmts
            .iter()
            .any(|s| matches!(s, ir::Stmt::Assert { .. })));
    }
}
