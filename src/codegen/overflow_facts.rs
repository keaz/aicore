//! Value ranges that let integer `+`, `-`, `*` and unary `-` skip their
//! overflow check.
//!
//! Every integer `+`, `-`, `*` and unary `-` traps when its result does not
//! fit the operand type. The check is left out only when interval
//! arithmetic proves that the exact result fits. Ranges come from:
//!
//! - integer literals;
//! - the declared integer type of a local or parameter (`UInt8` is
//!   `0..=255`), narrowed for a parameter by its function's enforced
//!   `requires` clause (`n >= 0 && n <= 1000`) when the parameter is never
//!   rebound or assigned, and for an immutable `let` by the range of its
//!   initializer (`let x = byte as Int` is `0..=255`);
//! - nested `+`, `-`, `*`, unary `-`, `/` and `%` by a literal, `&` with a
//!   non-negative operand, `>>` / `>>>` of a non-negative value by a literal,
//!   and lossless `as` casts of a local.
//!
//! A nested operation whose range does not fit its type traps when it
//! overflows, so its value is still within the type: its range is clamped
//! to the type range instead of being discarded.
//!
//! 128-bit operations are never proven (their ranges do not fit `i128`
//! arithmetic for `UInt128`).

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{BinOp, UnaryOp};
use crate::ir;

use super::divisor_facts::{
    collect_bound_names_in_block, enforced_requires, flip_comparison, plain_int_literal,
};

/// An inclusive range of integer values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Range {
    pub lo: i128,
    pub hi: i128,
}

impl Range {
    pub(super) fn new(lo: i128, hi: i128) -> Self {
        Range { lo, hi }
    }

    fn exact(value: i128) -> Self {
        Range::new(value, value)
    }

    fn contains(self, other: Range) -> bool {
        self.lo <= other.lo && other.hi <= self.hi
    }

    /// `self` narrowed to `bounds`, or `bounds` when they do not overlap.
    fn clamp(self, bounds: Range) -> Range {
        let lo = self.lo.max(bounds.lo);
        let hi = self.hi.min(bounds.hi);
        if lo <= hi {
            Range::new(lo, hi)
        } else {
            bounds
        }
    }
}

/// The range of an integer type of `bits` width, or `None` for 128-bit types.
pub(super) fn type_range(signed: bool, bits: u32) -> Option<Range> {
    if bits >= 128 {
        return None;
    }
    Some(if signed {
        Range::new(-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        Range::new(0, (1i128 << bits) - 1)
    })
}

/// Ranges for the parameters of `func` implied by its enforced `requires`
/// clause. Parameters that are rebound or assigned in the body are left out.
pub(super) fn requires_range_facts(func: &ir::Function) -> BTreeMap<String, Range> {
    let mut bounds: BTreeMap<String, (Option<i128>, Option<i128>)> = BTreeMap::new();
    let Some(requires) = enforced_requires(func) else {
        return BTreeMap::new();
    };
    let params = func
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<BTreeSet<_>>();
    collect_requires_bounds(requires, &params, &mut bounds);
    if bounds.is_empty() {
        return BTreeMap::new();
    }
    let mut rebound = BTreeSet::new();
    collect_bound_names_in_block(&func.body, &mut rebound);
    bounds
        .into_iter()
        .filter(|(name, _)| !rebound.contains(name))
        .filter_map(|(name, bounds)| match bounds {
            (None, None) => None,
            (lo, hi) => Some((
                name,
                Range::new(lo.unwrap_or(i128::MIN), hi.unwrap_or(i128::MAX)),
            )),
        })
        .collect()
}

fn collect_requires_bounds(
    expr: &ir::Expr,
    params: &BTreeSet<&str>,
    bounds: &mut BTreeMap<String, (Option<i128>, Option<i128>)>,
) {
    let ir::ExprKind::Binary { op, lhs, rhs } = &expr.kind else {
        return;
    };
    if *op == BinOp::And {
        collect_requires_bounds(lhs, params, bounds);
        collect_requires_bounds(rhs, params, bounds);
        return;
    }
    let (name, op, k) = match (&lhs.kind, plain_int_literal(rhs), plain_int_literal(lhs)) {
        (ir::ExprKind::Var(name), Some(k), _) => (name, *op, k),
        (_, _, Some(k)) => match (&rhs.kind, flip_comparison(*op)) {
            (ir::ExprKind::Var(name), Some(flipped)) => (name, flipped, k),
            _ => return,
        },
        _ => return,
    };
    if !params.contains(name.as_str()) {
        return;
    }
    let (lo, hi) = match op {
        BinOp::Eq => (Some(k), Some(k)),
        BinOp::Gt => (Some(k + 1), None),
        BinOp::Ge => (Some(k), None),
        BinOp::Lt => (None, Some(k - 1)),
        BinOp::Le => (None, Some(k)),
        _ => return,
    };
    let entry = bounds.entry(name.clone()).or_insert((None, None));
    if let Some(lo) = lo {
        entry.0 = Some(entry.0.map_or(lo, |old| old.max(lo)));
    }
    if let Some(hi) = hi {
        entry.1 = Some(entry.1.map_or(hi, |old| old.min(hi)));
    }
}

/// The exact range of `lhs op rhs` for `+`, `-` and `*`.
pub(super) fn arith_range(op: BinOp, lhs: Range, rhs: Range) -> Option<Range> {
    let corners = match op {
        BinOp::Add => vec![lhs.lo.checked_add(rhs.lo)?, lhs.hi.checked_add(rhs.hi)?],
        BinOp::Sub => vec![lhs.lo.checked_sub(rhs.hi)?, lhs.hi.checked_sub(rhs.lo)?],
        BinOp::Mul => vec![
            lhs.lo.checked_mul(rhs.lo)?,
            lhs.lo.checked_mul(rhs.hi)?,
            lhs.hi.checked_mul(rhs.lo)?,
            lhs.hi.checked_mul(rhs.hi)?,
        ],
        _ => return None,
    };
    let lo = corners.iter().copied().min()?;
    let hi = corners.iter().copied().max()?;
    Some(Range::new(lo, hi))
}

/// Whether `lhs op rhs` provably fits `ty`.
pub(super) fn proves_binary(op: BinOp, lhs: Range, rhs: Range, ty: Range) -> bool {
    arith_range(op, lhs, rhs).is_some_and(|result| ty.contains(result))
}

/// Whether `-x` provably fits `ty` (a signed type).
pub(super) fn proves_negation(operand: Range, ty: Range) -> bool {
    operand.lo > ty.lo && ty.contains(Range::new(-operand.hi, -operand.lo))
}

/// The range of the integer expression `expr` whose type has range `ty`.
/// `var_range` gives the range of a named local (its declared type,
/// narrowed by `requires` facts).
pub(super) fn expr_range(
    expr: &ir::Expr,
    ty: Range,
    var_range: &dyn Fn(&str) -> Option<Range>,
) -> Range {
    if let Some(value) = plain_int_literal(expr) {
        return Range::exact(value).clamp(ty);
    }
    let range = match &expr.kind {
        ir::ExprKind::Var(name) => var_range(name),
        ir::ExprKind::Unary {
            op: UnaryOp::Neg,
            expr: inner,
        } => {
            let inner = expr_range(inner, ty, var_range);
            Some(Range::new(-inner.hi, -inner.lo))
        }
        ir::ExprKind::Binary { op, lhs, rhs } => binary_range(*op, lhs, rhs, ty, var_range),
        ir::ExprKind::Call { callee, args, .. } => match (&callee.kind, args.as_slice()) {
            (
                ir::ExprKind::Var(name),
                [ir::Expr {
                    kind: ir::ExprKind::Var(arg),
                    ..
                }],
            ) if name.starts_with(crate::ast::CAST_BUILTIN_PREFIX) => var_range(arg),
            _ => None,
        },
        _ => None,
    };
    range.map_or(ty, |range| range.clamp(ty))
}

fn binary_range(
    op: BinOp,
    lhs: &ir::Expr,
    rhs: &ir::Expr,
    ty: Range,
    var_range: &dyn Fn(&str) -> Option<Range>,
) -> Option<Range> {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul => arith_range(
            op,
            expr_range(lhs, ty, var_range),
            expr_range(rhs, ty, var_range),
        ),
        BinOp::Div => {
            let k = plain_int_literal(rhs).filter(|k| *k > 0)?;
            let lhs = expr_range(lhs, ty, var_range);
            Some(Range::new(lhs.lo / k, lhs.hi / k))
        }
        BinOp::Mod => {
            let k = plain_int_literal(rhs).filter(|k| *k != 0)?;
            let max = k.abs() - 1;
            let lhs = expr_range(lhs, ty, var_range);
            Some(if lhs.lo >= 0 {
                Range::new(0, max.min(lhs.hi))
            } else if lhs.hi <= 0 {
                Range::new((-max).max(lhs.lo), 0)
            } else {
                Range::new(-max, max)
            })
        }
        BinOp::BitAnd => {
            // A non-negative operand clears the sign bit and bounds the result.
            let lhs = expr_range(lhs, ty, var_range);
            let rhs = expr_range(rhs, ty, var_range);
            match (lhs.lo >= 0, rhs.lo >= 0) {
                (true, true) => Some(Range::new(0, lhs.hi.min(rhs.hi))),
                (true, false) => Some(Range::new(0, lhs.hi)),
                (false, true) => Some(Range::new(0, rhs.hi)),
                (false, false) => None,
            }
        }
        BinOp::Shr | BinOp::Ushr => {
            let k = plain_int_literal(rhs).filter(|k| (0..127).contains(k))?;
            let lhs = expr_range(lhs, ty, var_range);
            (lhs.lo >= 0).then(|| Range::new(lhs.lo >> k, lhs.hi >> k))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{contracts::lower_runtime_asserts, ir_builder::build, parser::parse};

    fn function(src: &str) -> ir::Function {
        let (program, diags) = parse(src, "test.aic");
        assert!(diags.is_empty(), "{diags:#?}");
        lower_runtime_asserts(&build(&program.expect("program")))
            .items
            .into_iter()
            .find_map(|item| match item {
                ir::Item::Function(func) if func.name == "f" => Some(func),
                _ => None,
            })
            .expect("function")
    }

    const I64: Range = Range {
        lo: i64::MIN as i128,
        hi: i64::MAX as i128,
    };
    const U8: Range = Range { lo: 0, hi: 255 };

    #[test]
    fn type_ranges_cover_each_width() {
        assert_eq!(type_range(true, 8), Some(Range::new(-128, 127)));
        assert_eq!(type_range(false, 8), Some(U8));
        assert_eq!(type_range(true, 64), Some(I64));
        assert_eq!(type_range(false, 64), Some(Range::new(0, u64::MAX as i128)));
        assert_eq!(type_range(true, 128), None);
        assert_eq!(type_range(false, 128), None);
    }

    #[test]
    fn requires_clauses_bound_parameters() {
        let func =
            function("fn f(a: Int, b: Int) -> Int requires a >= 0 && a < 100 && 5 >= b { a }");
        let facts = requires_range_facts(&func);
        assert_eq!(facts.get("a"), Some(&Range::new(0, 99)));
        assert_eq!(facts.get("b"), Some(&Range::new(i128::MIN, 5)));
    }

    #[test]
    fn rebound_parameters_and_disjunctions_give_no_range() {
        let rebound = function("fn f(a: Int) -> Int requires a >= 0 && a < 9 { a = 100; a }");
        assert!(requires_range_facts(&rebound).is_empty());
        let either = function("fn f(a: Int) -> Int requires a >= 0 || a < 9 { a }");
        assert!(requires_range_facts(&either).is_empty());
        let unlowered = {
            let (program, _) = parse("fn f(a: Int) -> Int requires a == 1 { a }", "t.aic");
            build(&program.expect("program"))
                .items
                .into_iter()
                .find_map(|item| match item {
                    ir::Item::Function(func) => Some(func),
                    _ => None,
                })
                .expect("function")
        };
        assert!(requires_range_facts(&unlowered).is_empty());
    }

    #[test]
    fn binary_proofs_follow_interval_arithmetic() {
        let small = Range::new(0, 1000);
        assert!(proves_binary(BinOp::Mul, small, small, I64));
        assert!(proves_binary(BinOp::Add, U8, U8, I64));
        assert!(!proves_binary(BinOp::Add, U8, U8, U8));
        assert!(!proves_binary(BinOp::Sub, U8, Range::new(1, 1), U8));
        assert!(proves_binary(
            BinOp::Sub,
            Range::new(1, 255),
            Range::new(1, 1),
            U8
        ));
        assert!(!proves_binary(BinOp::Add, I64, Range::new(1, 1), I64));
        assert!(!proves_binary(BinOp::Mul, I64, I64, I64));
        assert!(proves_negation(Range::new(-5, 5), I64));
        assert!(!proves_negation(I64, I64));
    }

    #[test]
    fn expression_ranges_use_literals_locals_and_operators() {
        let tail = |src: &str| *function(src).body.tail.expect("tail");
        let locals = |name: &str| match name {
            "x" => Some(Range::new(0, 1_000_000)),
            "y" => Some(I64),
            _ => None,
        };
        let sum = tail("fn f(x: Int, y: Int) -> Int { x % 10 + (y & 255) * 3 - x / 1000 + 7 }");
        assert_eq!(expr_range(&sum, I64, &locals), Range::new(-993, 781));
        let shifted = tail("fn f(x: Int, y: Int) -> Int { ((y & 255) >> 4) + (x >>> 10) }");
        assert_eq!(
            expr_range(&shifted, I64, &locals),
            Range::new(0, 15 + (1_000_000 >> 10))
        );
        let cast = tail("fn f(x: Int, y: Int) -> Int { -(x as Int) }");
        assert_eq!(expr_range(&cast, I64, &locals), Range::new(-1_000_000, 0));
        // A possibly negative shift operand, and names without a known
        // range, fall back to the type range.
        let unknown = tail("fn f(x: Int, y: Int) -> Int { y >> 60 }");
        assert_eq!(expr_range(&unknown, I64, &locals), I64);
        let no_facts = BTreeMap::<String, Range>::new();
        let lookup = |name: &str| no_facts.get(name).copied();
        let wide = (i64::MAX / 1000) as i128;
        assert_eq!(
            expr_range(&sum, I64, &lookup),
            Range::new(-9 - wide + 7, 9 + 765 + wide + 7)
        );
    }
}
