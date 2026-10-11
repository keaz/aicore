//! Facts that let integer division and remainder skip their runtime guards.
//!
//! Every integer `/` and `%` is guarded against a zero divisor, and signed
//! division is guarded against `MIN / -1`. A guard is skipped only when the
//! divisor is known to be safe:
//!
//! - a plain integer literal (not zero, and not `-1` for the overflow guard);
//! - a function parameter that the function's `requires` clause proves
//!   non-zero (`b != 0`, `b > 0`, `b >= 1`, `b < 0`, ...) and that is never
//!   rebound or assigned in the body, so the fact holds at every use.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{BinOp, UnaryOp};
use crate::ir;

/// What is known about a divisor before the division runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum DivisorFact {
    /// Nothing is known; emit both guards.
    Unknown,
    /// The divisor is never zero but may be `-1`.
    NonZero,
    /// The divisor is never zero and never `-1`.
    NonZeroNotMinusOne,
}

impl DivisorFact {
    pub(super) fn excludes_zero(self) -> bool {
        self != DivisorFact::Unknown
    }

    pub(super) fn excludes_minus_one(self) -> bool {
        self == DivisorFact::NonZeroNotMinusOne
    }

    fn from_value(value: i128) -> Self {
        match value {
            0 => DivisorFact::Unknown,
            -1 => DivisorFact::NonZero,
            _ => DivisorFact::NonZeroNotMinusOne,
        }
    }
}

/// Divisor facts for the parameters of `func`, derived from its `requires`
/// clause. Parameters that are rebound or assigned anywhere in the body are
/// left out because the fact would not hold at every use.
pub(super) fn requires_divisor_facts(func: &ir::Function) -> BTreeMap<String, DivisorFact> {
    let mut facts = BTreeMap::new();
    let Some(requires) = &func.requires else {
        return facts;
    };
    // Only trust `requires` when contract lowering enforces it at entry. A
    // requires clause that is satisfiable but not proven is lowered to an
    // entry assert; without that assert (IR emitted without
    // `lower_runtime_asserts`) the clause proves nothing at runtime.
    let enforced = matches!(
        func.body.stmts.first(),
        Some(ir::Stmt::Assert { message, .. })
            if message.starts_with("contract_violation{kind:requires,")
    );
    if !enforced {
        return facts;
    }
    let params = func
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<BTreeSet<_>>();
    collect_requires_facts(requires, &params, &mut facts);
    if facts.is_empty() {
        return facts;
    }
    let mut rebound = BTreeSet::new();
    collect_bound_names_in_block(&func.body, &mut rebound);
    facts.retain(|name, _| !rebound.contains(name));
    facts
}

/// The fact for a divisor expression, given the facts of the current function.
pub(super) fn divisor_fact(expr: &ir::Expr, params: &BTreeMap<String, DivisorFact>) -> DivisorFact {
    if let Some(value) = plain_int_literal(expr) {
        return DivisorFact::from_value(value);
    }
    match &expr.kind {
        ir::ExprKind::Var(name) => params.get(name).copied().unwrap_or(DivisorFact::Unknown),
        _ => DivisorFact::Unknown,
    }
}

/// The value of an integer literal or a negated integer literal. Literals with
/// wide-literal metadata are not folded, so their guards stay in place.
fn plain_int_literal(expr: &ir::Expr) -> Option<i128> {
    match &expr.kind {
        ir::ExprKind::Int(value) if expr.int_literal_metadata().is_none() => Some(*value as i128),
        ir::ExprKind::Unary {
            op: UnaryOp::Neg,
            expr: inner,
        } => plain_int_literal(inner).map(|value| -value),
        _ => None,
    }
}

fn collect_requires_facts(
    expr: &ir::Expr,
    params: &BTreeSet<&str>,
    facts: &mut BTreeMap<String, DivisorFact>,
) {
    let ir::ExprKind::Binary { op, lhs, rhs } = &expr.kind else {
        return;
    };
    if *op == BinOp::And {
        collect_requires_facts(lhs, params, facts);
        collect_requires_facts(rhs, params, facts);
        return;
    }
    let (name, op, bound) = match (&lhs.kind, plain_int_literal(rhs), plain_int_literal(lhs)) {
        (ir::ExprKind::Var(name), Some(bound), _) => (name, *op, bound),
        (_, _, Some(bound)) => match &rhs.kind {
            // `k op x` is `x flipped(op) k`.
            ir::ExprKind::Var(name) => match flip_comparison(*op) {
                Some(flipped) => (name, flipped, bound),
                None => return,
            },
            _ => return,
        },
        _ => return,
    };
    if !params.contains(name.as_str()) {
        return;
    }
    let fact = match comparison_fact(op, bound) {
        Some(fact) if fact != DivisorFact::Unknown => fact,
        _ => return,
    };
    let entry = facts.entry(name.clone()).or_insert(DivisorFact::Unknown);
    *entry = (*entry).max(fact);
}

fn flip_comparison(op: BinOp) -> Option<BinOp> {
    match op {
        BinOp::Eq => Some(BinOp::Eq),
        BinOp::Ne => Some(BinOp::Ne),
        BinOp::Lt => Some(BinOp::Gt),
        BinOp::Le => Some(BinOp::Ge),
        BinOp::Gt => Some(BinOp::Lt),
        BinOp::Ge => Some(BinOp::Le),
        _ => None,
    }
}

/// The fact implied by `x op bound`.
fn comparison_fact(op: BinOp, bound: i128) -> Option<DivisorFact> {
    match op {
        BinOp::Ne if bound == 0 => Some(DivisorFact::NonZero),
        BinOp::Eq => Some(DivisorFact::from_value(bound)),
        // x > k with k >= 0, or x >= k with k >= 1: x is positive.
        BinOp::Gt if bound >= 0 => Some(DivisorFact::NonZeroNotMinusOne),
        BinOp::Ge if bound >= 1 => Some(DivisorFact::NonZeroNotMinusOne),
        // x < k with k <= 0, or x <= k with k <= -1: x is negative.
        BinOp::Lt if bound <= -1 => Some(DivisorFact::NonZeroNotMinusOne),
        BinOp::Lt if bound == 0 => Some(DivisorFact::NonZero),
        BinOp::Le if bound <= -2 => Some(DivisorFact::NonZeroNotMinusOne),
        BinOp::Le if bound == -1 => Some(DivisorFact::NonZero),
        _ => None,
    }
}

fn collect_bound_names_in_block(block: &ir::Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            ir::Stmt::Let { name, expr, .. } => {
                out.insert(name.clone());
                collect_bound_names_in_expr(expr, out);
            }
            ir::Stmt::Assign { target, expr, .. } => {
                out.insert(target.clone());
                collect_bound_names_in_expr(expr, out);
            }
            ir::Stmt::Expr { expr, .. } | ir::Stmt::Assert { expr, .. } => {
                collect_bound_names_in_expr(expr, out);
            }
            ir::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    collect_bound_names_in_expr(expr, out);
                }
            }
        }
    }
    if let Some(tail) = &block.tail {
        collect_bound_names_in_expr(tail, out);
    }
}

fn collect_bound_names_in_expr(expr: &ir::Expr, out: &mut BTreeSet<String>) {
    match &expr.kind {
        ir::ExprKind::Closure { params, body, .. } => {
            out.extend(params.iter().map(|param| param.name.clone()));
            collect_bound_names_in_block(body, out);
        }
        ir::ExprKind::Match { expr, arms } => {
            collect_bound_names_in_expr(expr, out);
            for arm in arms {
                collect_bound_names_in_pattern(&arm.pattern, out);
                if let Some(guard) = &arm.guard {
                    collect_bound_names_in_expr(guard, out);
                }
                collect_bound_names_in_expr(&arm.body, out);
            }
        }
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_bound_names_in_expr(cond, out);
            collect_bound_names_in_block(then_block, out);
            collect_bound_names_in_block(else_block, out);
        }
        ir::ExprKind::While { cond, body } => {
            collect_bound_names_in_expr(cond, out);
            collect_bound_names_in_block(body, out);
        }
        ir::ExprKind::Loop { body } | ir::ExprKind::UnsafeBlock { block: body } => {
            collect_bound_names_in_block(body, out);
        }
        ir::ExprKind::Break { expr } | ir::ExprKind::Return { expr } => {
            if let Some(expr) = expr {
                collect_bound_names_in_expr(expr, out);
            }
        }
        ir::ExprKind::Call { callee, args, .. } => {
            collect_bound_names_in_expr(callee, out);
            for arg in args {
                collect_bound_names_in_expr(arg, out);
            }
        }
        ir::ExprKind::TemplateLiteral { args, .. } => {
            for arg in args {
                collect_bound_names_in_expr(arg, out);
            }
        }
        ir::ExprKind::Binary { lhs, rhs, .. } => {
            collect_bound_names_in_expr(lhs, out);
            collect_bound_names_in_expr(rhs, out);
        }
        ir::ExprKind::Unary { expr, .. }
        | ir::ExprKind::Borrow { expr, .. }
        | ir::ExprKind::Await { expr }
        | ir::ExprKind::Try { expr }
        | ir::ExprKind::FieldAccess { base: expr, .. } => collect_bound_names_in_expr(expr, out),
        ir::ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                collect_bound_names_in_expr(value, out);
            }
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
}

fn collect_bound_names_in_pattern(pattern: &ir::Pattern, out: &mut BTreeSet<String>) {
    match &pattern.kind {
        ir::PatternKind::Var(name) => {
            out.insert(name.clone());
        }
        ir::PatternKind::Or { patterns } | ir::PatternKind::Variant { args: patterns, .. } => {
            for pattern in patterns {
                collect_bound_names_in_pattern(pattern, out);
            }
        }
        ir::PatternKind::Struct { fields, .. } => {
            for field in fields {
                collect_bound_names_in_pattern(&field.pattern, out);
            }
        }
        ir::PatternKind::Wildcard
        | ir::PatternKind::Int(_)
        | ir::PatternKind::Bool(_)
        | ir::PatternKind::Char(_)
        | ir::PatternKind::String(_)
        | ir::PatternKind::Unit => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{divisor_fact, requires_divisor_facts, DivisorFact};
    use crate::{contracts::lower_runtime_asserts, ir, ir_builder::build, parser::parse};

    fn function(src: &str, name: &str) -> ir::Function {
        let (program, diags) = parse(src, "test.aic");
        assert!(diags.is_empty(), "{diags:#?}");
        let program = lower_runtime_asserts(&build(&program.expect("program")));
        program
            .items
            .into_iter()
            .find_map(|item| match item {
                ir::Item::Function(func) if func.name == name => Some(func),
                _ => None,
            })
            .expect("function")
    }

    fn facts(requires: &str) -> BTreeMap<String, DivisorFact> {
        let src = format!("fn f(a: Int, b: Int) -> Int requires {requires} {{ a / b }}");
        requires_divisor_facts(&function(&src, "f"))
    }

    fn unlowered_function(src: &str, name: &str) -> ir::Function {
        let (program, diags) = parse(src, "test.aic");
        assert!(diags.is_empty(), "{diags:#?}");
        build(&program.expect("program"))
            .items
            .into_iter()
            .find_map(|item| match item {
                ir::Item::Function(func) if func.name == name => Some(func),
                _ => None,
            })
            .expect("function")
    }

    #[test]
    fn requires_without_lowered_entry_assert_is_not_trusted() {
        let func = unlowered_function("fn f(a: Int, b: Int) -> Int requires b > 0 { a / b }", "f");
        assert!(requires_divisor_facts(&func).is_empty());
    }

    #[test]
    fn requires_comparisons_produce_divisor_facts() {
        use DivisorFact::*;
        let cases = [
            ("b != 0", Some(NonZero)),
            ("0 != b", Some(NonZero)),
            ("b > 0", Some(NonZeroNotMinusOne)),
            ("0 < b", Some(NonZeroNotMinusOne)),
            ("b >= 1", Some(NonZeroNotMinusOne)),
            ("b >= 0", None),
            ("b > -1", None),
            ("b > -2", None),
            ("b < 0", Some(NonZero)),
            ("b < -1", Some(NonZeroNotMinusOne)),
            ("b <= -1", Some(NonZero)),
            ("b <= -2", Some(NonZeroNotMinusOne)),
            ("b == 5", Some(NonZeroNotMinusOne)),
            ("b == -1", Some(NonZero)),
            ("b == 0", None),
            ("b != 1", None),
            ("a > 0 && b != 0", Some(NonZero)),
            ("b != 0 && b > 0", Some(NonZeroNotMinusOne)),
            ("b != 0 || a > 0", None),
            ("true", None),
        ];
        for (requires, expected) in cases {
            assert_eq!(
                facts(requires).get("b").copied(),
                expected,
                "requires {requires}"
            );
        }
        assert_eq!(
            facts("a > 0 && b != 0").get("a").copied(),
            Some(NonZeroNotMinusOne)
        );
    }

    #[test]
    fn rebound_or_assigned_parameters_lose_their_facts() {
        let shadowed = function(
            "fn f(a: Int, b: Int) -> Int requires b != 0 { let b = 0; a / b }",
            "f",
        );
        assert!(requires_divisor_facts(&shadowed).is_empty());

        let matched = function(
            "fn f(a: Int, b: Int) -> Int requires b != 0 { match a { b => a / b } }",
            "f",
        );
        assert!(requires_divisor_facts(&matched).is_empty());

        let closure = function(
            "fn f(a: Int, b: Int) -> Int requires b > 0 { let g = |b: Int| -> Int { a / b }; g(0) }",
            "f",
        );
        assert!(requires_divisor_facts(&closure).is_empty());

        let untouched = function(
            "fn f(a: Int, b: Int) -> Int requires b > 0 { let c = a; c / b }",
            "f",
        );
        assert_eq!(
            requires_divisor_facts(&untouched).get("b").copied(),
            Some(DivisorFact::NonZeroNotMinusOne)
        );
    }

    #[test]
    fn non_parameters_and_missing_requires_produce_no_facts() {
        let no_requires = function("fn f(a: Int, b: Int) -> Int { a / b }", "f");
        assert!(requires_divisor_facts(&no_requires).is_empty());
    }

    #[test]
    fn literal_divisors_are_classified_by_value() {
        let func = function(
            "fn f(a: Int) -> Int { a / 4 + a / 0 + a / -1 + a / -7 + a / a }",
            "f",
        );
        let mut divisors = Vec::new();
        collect_divisors(func.body.tail.as_ref().expect("tail"), &mut divisors);
        let params = BTreeMap::new();
        let found: Vec<DivisorFact> = divisors
            .iter()
            .map(|expr| divisor_fact(expr, &params))
            .collect();
        assert_eq!(
            found,
            vec![
                DivisorFact::NonZeroNotMinusOne,
                DivisorFact::Unknown,
                DivisorFact::NonZero,
                DivisorFact::NonZeroNotMinusOne,
                DivisorFact::Unknown,
            ]
        );
    }

    fn collect_divisors<'a>(expr: &'a ir::Expr, out: &mut Vec<&'a ir::Expr>) {
        if let ir::ExprKind::Binary { op, lhs, rhs } = &expr.kind {
            collect_divisors(lhs, out);
            if *op == crate::ast::BinOp::Div {
                out.push(rhs);
            }
            collect_divisors(rhs, out);
        }
    }
}
