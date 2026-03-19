use crate::ir;
use crate::span::Span;

pub fn lower_template_literals(program: &ir::Program) -> ir::Program {
    let mut lowerer = TemplateLowerer::new(program);
    lowerer.lower_program(program)
}

struct TemplateLowerer {
    next_node: u32,
}

impl TemplateLowerer {
    fn new(program: &ir::Program) -> Self {
        Self {
            next_node: max_node(program) + 1,
        }
    }

    fn fresh_node(&mut self) -> ir::NodeId {
        let out = ir::NodeId(self.next_node);
        self.next_node += 1;
        out
    }

    fn lower_program(&mut self, program: &ir::Program) -> ir::Program {
        let mut lowered = program.clone();
        lowered.items = program
            .items
            .iter()
            .map(|item| self.lower_item(item))
            .collect();
        lowered
    }

    fn lower_item(&mut self, item: &ir::Item) -> ir::Item {
        match item {
            ir::Item::Function(func) => ir::Item::Function(self.lower_function(func)),
            ir::Item::Struct(def) => {
                let mut lowered = def.clone();
                lowered.invariant = def.invariant.as_ref().map(|expr| self.lower_expr(expr));
                lowered.fields = def
                    .fields
                    .iter()
                    .map(|field| {
                        let mut lowered_field = field.clone();
                        lowered_field.default_value = field
                            .default_value
                            .as_ref()
                            .map(|expr| self.lower_expr(expr));
                        lowered_field
                    })
                    .collect();
                ir::Item::Struct(lowered)
            }
            ir::Item::Enum(def) => ir::Item::Enum(def.clone()),
            ir::Item::Trait(def) => {
                let mut lowered = def.clone();
                lowered.methods = def
                    .methods
                    .iter()
                    .map(|method| self.lower_function(method))
                    .collect();
                ir::Item::Trait(lowered)
            }
            ir::Item::Impl(def) => {
                let mut lowered = def.clone();
                lowered.methods = def
                    .methods
                    .iter()
                    .map(|method| self.lower_function(method))
                    .collect();
                ir::Item::Impl(lowered)
            }
        }
    }

    fn lower_function(&mut self, func: &ir::Function) -> ir::Function {
        let mut lowered = func.clone();
        lowered.requires = func.requires.as_ref().map(|expr| self.lower_expr(expr));
        lowered.ensures = func.ensures.as_ref().map(|expr| self.lower_expr(expr));
        lowered.body = self.lower_block(&func.body);
        lowered
    }

    fn lower_block(&mut self, block: &ir::Block) -> ir::Block {
        ir::Block {
            node: block.node,
            stmts: block
                .stmts
                .iter()
                .map(|stmt| self.lower_stmt(stmt))
                .collect(),
            tail: block
                .tail
                .as_ref()
                .map(|expr| Box::new(self.lower_expr(expr))),
            span: block.span,
        }
    }

    fn lower_stmt(&mut self, stmt: &ir::Stmt) -> ir::Stmt {
        match stmt {
            ir::Stmt::Let {
                symbol,
                name,
                mutable,
                ty,
                expr,
                span,
            } => ir::Stmt::Let {
                symbol: *symbol,
                name: name.clone(),
                mutable: *mutable,
                ty: *ty,
                expr: self.lower_expr(expr),
                span: *span,
            },
            ir::Stmt::Assign { target, expr, span } => ir::Stmt::Assign {
                target: target.clone(),
                expr: self.lower_expr(expr),
                span: *span,
            },
            ir::Stmt::Expr { expr, span } => ir::Stmt::Expr {
                expr: self.lower_expr(expr),
                span: *span,
            },
            ir::Stmt::Return { expr, span } => ir::Stmt::Return {
                expr: expr.as_ref().map(|value| self.lower_expr(value)),
                span: *span,
            },
            ir::Stmt::Assert {
                expr,
                message,
                span,
            } => ir::Stmt::Assert {
                expr: self.lower_expr(expr),
                message: message.clone(),
                span: *span,
            },
        }
    }

    fn lower_expr(&mut self, expr: &ir::Expr) -> ir::Expr {
        match &expr.kind {
            ir::ExprKind::TemplateLiteral { template, args } => {
                self.lower_template_literal(expr.node, expr.span, template, args)
            }
            ir::ExprKind::Call {
                callee,
                args,
                arg_names,
            } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::Call {
                    callee: Box::new(self.lower_expr(callee)),
                    args: args.iter().map(|arg| self.lower_expr(arg)).collect(),
                    arg_names: arg_names.clone(),
                },
                span: expr.span,
            },
            ir::ExprKind::Closure {
                params,
                ret_type,
                body,
            } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::Closure {
                    params: params.clone(),
                    ret_type: *ret_type,
                    body: self.lower_block(body),
                },
                span: expr.span,
            },
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::If {
                    cond: Box::new(self.lower_expr(cond)),
                    then_block: self.lower_block(then_block),
                    else_block: self.lower_block(else_block),
                },
                span: expr.span,
            },
            ir::ExprKind::While { cond, body } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::While {
                    cond: Box::new(self.lower_expr(cond)),
                    body: self.lower_block(body),
                },
                span: expr.span,
            },
            ir::ExprKind::Loop { body } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::Loop {
                    body: self.lower_block(body),
                },
                span: expr.span,
            },
            ir::ExprKind::Break { expr: inner } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::Break {
                    expr: inner.as_ref().map(|value| Box::new(self.lower_expr(value))),
                },
                span: expr.span,
            },
            ir::ExprKind::Match {
                expr: matched,
                arms,
            } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::Match {
                    expr: Box::new(self.lower_expr(matched)),
                    arms: arms
                        .iter()
                        .map(|arm| ir::MatchArm {
                            pattern: arm.pattern.clone(),
                            guard: arm.guard.as_ref().map(|guard| self.lower_expr(guard)),
                            body: self.lower_expr(&arm.body),
                            span: arm.span,
                        })
                        .collect(),
                },
                span: expr.span,
            },
            ir::ExprKind::Binary { op, lhs, rhs } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::Binary {
                    op: *op,
                    lhs: Box::new(self.lower_expr(lhs)),
                    rhs: Box::new(self.lower_expr(rhs)),
                },
                span: expr.span,
            },
            ir::ExprKind::Unary { op, expr: inner } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::Unary {
                    op: *op,
                    expr: Box::new(self.lower_expr(inner)),
                },
                span: expr.span,
            },
            ir::ExprKind::Borrow {
                mutable,
                expr: inner,
            } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::Borrow {
                    mutable: *mutable,
                    expr: Box::new(self.lower_expr(inner)),
                },
                span: expr.span,
            },
            ir::ExprKind::Await { expr: inner } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::Await {
                    expr: Box::new(self.lower_expr(inner)),
                },
                span: expr.span,
            },
            ir::ExprKind::Try { expr: inner } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::Try {
                    expr: Box::new(self.lower_expr(inner)),
                },
                span: expr.span,
            },
            ir::ExprKind::UnsafeBlock { block } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::UnsafeBlock {
                    block: self.lower_block(block),
                },
                span: expr.span,
            },
            ir::ExprKind::StructInit { name, fields } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::StructInit {
                    name: name.clone(),
                    fields: fields
                        .iter()
                        .map(|(field, value, span)| (field.clone(), self.lower_expr(value), *span))
                        .collect(),
                },
                span: expr.span,
            },
            ir::ExprKind::FieldAccess { base, field } => ir::Expr {
                node: expr.node,
                kind: ir::ExprKind::FieldAccess {
                    base: Box::new(self.lower_expr(base)),
                    field: field.clone(),
                },
                span: expr.span,
            },
            ir::ExprKind::Int(_)
            | ir::ExprKind::Float(_)
            | ir::ExprKind::Bool(_)
            | ir::ExprKind::Char(_)
            | ir::ExprKind::String(_)
            | ir::ExprKind::Unit
            | ir::ExprKind::Var(_)
            | ir::ExprKind::Continue => expr.clone(),
        }
    }

    fn lower_template_literal(
        &mut self,
        root_node: ir::NodeId,
        span: Span,
        template: &str,
        args: &[ir::Expr],
    ) -> ir::Expr {
        let lowered_args = args
            .iter()
            .map(|arg| self.lower_expr(arg))
            .collect::<Vec<_>>();
        if lowered_args.is_empty() {
            return ir::Expr {
                node: root_node,
                kind: ir::ExprKind::String(template.to_string()),
                span,
            };
        }

        let mut args_iter = lowered_args.into_iter();
        let first = args_iter.next().expect("non-empty interpolation args");
        let vec_of_node = self.fresh_node();
        let mut vec_expr = self.make_call(vec_of_node, "aic_vec_of_intrinsic", vec![first], span);
        for arg in args_iter {
            let push_node = self.fresh_node();
            vec_expr = self.make_call(
                push_node,
                "aic_vec_push_intrinsic",
                vec![vec_expr, arg],
                span,
            );
        }

        let template_arg = ir::Expr {
            node: self.fresh_node(),
            kind: ir::ExprKind::String(template.to_string()),
            span,
        };
        self.make_call(
            root_node,
            "aic_string_format_intrinsic",
            vec![template_arg, vec_expr],
            span,
        )
    }

    fn make_call(
        &mut self,
        node: ir::NodeId,
        callee_name: &str,
        args: Vec<ir::Expr>,
        span: Span,
    ) -> ir::Expr {
        ir::Expr {
            node,
            kind: ir::ExprKind::Call {
                callee: Box::new(self.make_var(callee_name, span)),
                args,
                arg_names: Vec::new(),
            },
            span,
        }
    }

    fn make_var(&mut self, name: &str, span: Span) -> ir::Expr {
        ir::Expr {
            node: self.fresh_node(),
            kind: ir::ExprKind::Var(name.to_string()),
            span,
        }
    }
}

fn max_node(program: &ir::Program) -> u32 {
    let mut max = 0;
    for item in &program.items {
        match item {
            ir::Item::Function(func) => {
                max = max.max(max_node_block(&func.body));
                if let Some(req) = &func.requires {
                    max = max.max(max_node_expr(req));
                }
                if let Some(ens) = &func.ensures {
                    max = max.max(max_node_expr(ens));
                }
            }
            ir::Item::Struct(def) => {
                if let Some(inv) = &def.invariant {
                    max = max.max(max_node_expr(inv));
                }
                for field in &def.fields {
                    if let Some(default_value) = &field.default_value {
                        max = max.max(max_node_expr(default_value));
                    }
                }
            }
            ir::Item::Enum(_) => {}
            ir::Item::Trait(def) => {
                for method in &def.methods {
                    max = max.max(max_node_block(&method.body));
                    if let Some(req) = &method.requires {
                        max = max.max(max_node_expr(req));
                    }
                    if let Some(ens) = &method.ensures {
                        max = max.max(max_node_expr(ens));
                    }
                }
            }
            ir::Item::Impl(def) => {
                for method in &def.methods {
                    max = max.max(max_node_block(&method.body));
                    if let Some(req) = &method.requires {
                        max = max.max(max_node_expr(req));
                    }
                    if let Some(ens) = &method.ensures {
                        max = max.max(max_node_expr(ens));
                    }
                }
            }
        }
    }
    max
}

fn max_node_block(block: &ir::Block) -> u32 {
    let mut max = block.node.0;
    for stmt in &block.stmts {
        match stmt {
            ir::Stmt::Let { expr, .. }
            | ir::Stmt::Expr { expr, .. }
            | ir::Stmt::Assert { expr, .. } => {
                max = max.max(max_node_expr(expr));
            }
            ir::Stmt::Assign { expr, .. } => {
                max = max.max(max_node_expr(expr));
            }
            ir::Stmt::Return { expr, .. } => {
                if let Some(value) = expr {
                    max = max.max(max_node_expr(value));
                }
            }
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
        ir::ExprKind::Call { callee, args, .. } => {
            max = max.max(max_node_expr(callee));
            for arg in args {
                max = max.max(max_node_expr(arg));
            }
        }
        ir::ExprKind::TemplateLiteral { args, .. } => {
            for arg in args {
                max = max.max(max_node_expr(arg));
            }
        }
        ir::ExprKind::Closure { body, .. } => {
            max = max.max(max_node_block(body));
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
        ir::ExprKind::While { cond, body } => {
            max = max.max(max_node_expr(cond));
            max = max.max(max_node_block(body));
        }
        ir::ExprKind::Loop { body } | ir::ExprKind::UnsafeBlock { block: body } => {
            max = max.max(max_node_block(body));
        }
        ir::ExprKind::Break { expr } => {
            if let Some(value) = expr {
                max = max.max(max_node_expr(value));
            }
        }
        ir::ExprKind::Match { expr, arms } => {
            max = max.max(max_node_expr(expr));
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    max = max.max(max_node_expr(guard));
                }
                max = max.max(max_node_expr(&arm.body));
            }
        }
        ir::ExprKind::Binary { lhs, rhs, .. } => {
            max = max.max(max_node_expr(lhs));
            max = max.max(max_node_expr(rhs));
        }
        ir::ExprKind::Unary { expr, .. }
        | ir::ExprKind::Borrow { expr, .. }
        | ir::ExprKind::Await { expr }
        | ir::ExprKind::Try { expr } => {
            max = max.max(max_node_expr(expr));
        }
        ir::ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                max = max.max(max_node_expr(value));
            }
        }
        ir::ExprKind::FieldAccess { base, .. } => {
            max = max.max(max_node_expr(base));
        }
        ir::ExprKind::Int(_)
        | ir::ExprKind::Float(_)
        | ir::ExprKind::Bool(_)
        | ir::ExprKind::Char(_)
        | ir::ExprKind::String(_)
        | ir::ExprKind::Unit
        | ir::ExprKind::Var(_)
        | ir::ExprKind::Continue => {}
    }
    max
}

#[cfg(test)]
mod tests {
    use super::lower_template_literals;
    use crate::ir;
    use crate::ir_builder::build;
    use crate::parser::parse;

    fn parse_ir(source: &str) -> ir::Program {
        let (program, diagnostics) = parse(source, "template_lowering_test.aic");
        assert!(
            diagnostics.is_empty(),
            "parse diagnostics: {diagnostics:#?}"
        );
        build(&program.expect("program"))
    }

    #[test]
    fn lowers_template_literal_to_intrinsic_call_chain() {
        let src = r#"
fn main(name: String) -> String {
    f"Hello, {name} {name}"
}
"#;
        let ir = parse_ir(src);
        let lowered = lower_template_literals(&ir);
        let ir::Item::Function(function) = &lowered.items[0] else {
            panic!("expected function item");
        };
        let tail = function.body.tail.as_ref().expect("tail expression");
        let ir::ExprKind::Call { callee, args, .. } = &tail.kind else {
            panic!("expected lowered format call");
        };
        assert!(matches!(
            callee.kind,
            ir::ExprKind::Var(ref name) if name == "aic_string_format_intrinsic"
        ));
        assert_eq!(args.len(), 2);
        assert!(matches!(
            args[0].kind,
            ir::ExprKind::String(ref template) if template == "Hello, {0} {1}"
        ));
        let ir::ExprKind::Call {
            callee: push_callee,
            args: push_args,
            ..
        } = &args[1].kind
        else {
            panic!("expected vector push chain");
        };
        assert!(matches!(
            push_callee.kind,
            ir::ExprKind::Var(ref name) if name == "aic_vec_push_intrinsic"
        ));
        assert_eq!(push_args.len(), 2);
        let ir::ExprKind::Call {
            callee: vec_of_callee,
            args: vec_of_args,
            ..
        } = &push_args[0].kind
        else {
            panic!("expected vec_of seed call");
        };
        assert!(matches!(
            vec_of_callee.kind,
            ir::ExprKind::Var(ref name) if name == "aic_vec_of_intrinsic"
        ));
        assert_eq!(vec_of_args.len(), 1);
    }

    #[test]
    fn lowering_removes_template_literal_nodes() {
        let src = r#"
fn main(name: String) -> String {
    f"hello {name}"
}
"#;
        let ir = parse_ir(src);
        let lowered = lower_template_literals(&ir);
        let ir::Item::Function(function) = &lowered.items[0] else {
            panic!("expected function item");
        };
        let tail = function.body.tail.as_ref().expect("tail expression");
        assert!(
            !contains_template_literal(tail),
            "lowered expression tree must not keep template literal nodes"
        );
    }

    fn contains_template_literal(expr: &ir::Expr) -> bool {
        match &expr.kind {
            ir::ExprKind::TemplateLiteral { .. } => true,
            ir::ExprKind::Call { callee, args, .. } => {
                contains_template_literal(callee) || args.iter().any(contains_template_literal)
            }
            ir::ExprKind::Closure { body, .. } => body
                .tail
                .as_ref()
                .map(|tail| contains_template_literal(tail))
                .unwrap_or(false),
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                contains_template_literal(cond)
                    || then_block
                        .tail
                        .as_ref()
                        .map(|tail| contains_template_literal(tail))
                        .unwrap_or(false)
                    || else_block
                        .tail
                        .as_ref()
                        .map(|tail| contains_template_literal(tail))
                        .unwrap_or(false)
            }
            ir::ExprKind::Match { expr, arms } => {
                contains_template_literal(expr)
                    || arms.iter().any(|arm| contains_template_literal(&arm.body))
            }
            ir::ExprKind::Binary { lhs, rhs, .. } => {
                contains_template_literal(lhs) || contains_template_literal(rhs)
            }
            ir::ExprKind::Unary { expr, .. }
            | ir::ExprKind::Borrow { expr, .. }
            | ir::ExprKind::Await { expr }
            | ir::ExprKind::Try { expr }
            | ir::ExprKind::FieldAccess { base: expr, .. } => contains_template_literal(expr),
            ir::ExprKind::StructInit { fields, .. } => fields
                .iter()
                .any(|(_, value, _)| contains_template_literal(value)),
            ir::ExprKind::Int(_)
            | ir::ExprKind::Float(_)
            | ir::ExprKind::Bool(_)
            | ir::ExprKind::Char(_)
            | ir::ExprKind::String(_)
            | ir::ExprKind::Unit
            | ir::ExprKind::Var(_)
            | ir::ExprKind::While { .. }
            | ir::ExprKind::Loop { .. }
            | ir::ExprKind::Break { .. }
            | ir::ExprKind::Continue
            | ir::ExprKind::UnsafeBlock { .. } => false,
        }
    }
}
