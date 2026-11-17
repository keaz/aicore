//! Integer arithmetic with overflow checks, and the explicit
//! `wrapping_*`, `checked_*` and `saturating_*` methods.
//!
//! Semantics:
//! - `+`, `-`, `*` and unary `-` on any integer type trap when the exact
//!   result does not fit the type, with the panic message
//!   `integer_overflow{op:<op>,type:<T>}: <T> <operation> overflows`. They
//!   lower to `llvm.{s,u}{add,sub,mul}.with.overflow` and a branch to the
//!   panic; the check is skipped when `overflow_facts` proves the result
//!   fits. (`/` traps on `MIN / -1` in `emit_integer_division_guards`.)
//! - `wrapping_add/sub/mul/neg` keep the low bits (two's complement).
//! - `checked_add/sub/mul/div/rem/neg` return `None` on overflow (and on a
//!   zero divisor for `div` / `rem`).
//! - `saturating_add/sub/mul/neg` clamp to the type's bounds
//!   (`llvm.{s,u}{add,sub}.sat` for `+` and `-`).

use super::overflow_facts::{self, Range};
use super::*;

/// An integer operation that can overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
}

impl ArithOp {
    pub(super) fn from_bin_op(op: BinOp) -> Option<Self> {
        Some(match op {
            BinOp::Add => ArithOp::Add,
            BinOp::Sub => ArithOp::Sub,
            BinOp::Mul => ArithOp::Mul,
            _ => return None,
        })
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "add" => ArithOp::Add,
            "sub" => ArithOp::Sub,
            "mul" => ArithOp::Mul,
            "div" => ArithOp::Div,
            "rem" => ArithOp::Rem,
            "neg" => ArithOp::Neg,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            ArithOp::Add => "add",
            ArithOp::Sub => "sub",
            ArithOp::Mul => "mul",
            ArithOp::Div => "div",
            ArithOp::Rem => "rem",
            ArithOp::Neg => "neg",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            ArithOp::Add => "addition",
            ArithOp::Sub => "subtraction",
            ArithOp::Mul => "multiplication",
            ArithOp::Div => "division",
            ArithOp::Rem => "remainder",
            ArithOp::Neg => "negation",
        }
    }

    fn plain_inst(self) -> &'static str {
        match self {
            ArithOp::Add => "add",
            ArithOp::Sub | ArithOp::Neg => "sub",
            ArithOp::Mul => "mul",
            ArithOp::Div => "div",
            ArithOp::Rem => "rem",
        }
    }
}

/// Signedness and width of an integer type.
fn int_shape(ty: &LType) -> Option<(bool, u32)> {
    integer_width_bits(ty).map(|bits| (!is_unsigned_integer_type(ty), bits))
}

/// Smallest and largest value of an integer type as LLVM constants.
fn int_limits(signed: bool, bits: u32) -> (String, String) {
    if signed {
        let max = (1u128 << (bits - 1)) - 1;
        (format!("-{}", max + 1), max.to_string())
    } else {
        // `-1` is the all-ones bit pattern, the largest unsigned value.
        ("0".to_string(), "-1".to_string())
    }
}

impl<'a> Generator<'a> {
    /// The range of an integer type, or `None` for 128-bit and non-integer
    /// types.
    fn ltype_range(ty: &LType) -> Option<Range> {
        let (signed, bits) = int_shape(ty)?;
        overflow_facts::type_range(signed, bits)
    }

    /// The range of a named local: its declared type, narrowed by the
    /// function's `requires` facts.
    fn local_range(name: &str, fctx: &FnCtx) -> Option<Range> {
        let local = find_local(&fctx.vars, name)?;
        let mut range = Self::ltype_range(&local.ty)?;
        for fact in [
            fctx.range_facts.get(name),
            fctx.local_ranges.get(&local.ptr),
        ]
        .into_iter()
        .flatten()
        {
            range = Range::new(range.lo.max(fact.lo), range.hi.min(fact.hi));
        }
        Some(range)
    }

    /// Records the range of an immutable integer `let` whose initializer is
    /// narrower than its type (`let x = byte as Int` is `0..=255`). The slot
    /// `ptr` is unique to the binding, and the binding is never assigned.
    pub(super) fn record_let_range(
        &self,
        ptr: &str,
        init: &ir::Expr,
        ty: &LType,
        fctx: &mut FnCtx,
    ) {
        let Some(ty_range) = Self::ltype_range(ty) else {
            return;
        };
        let lookup = |name: &str| Self::local_range(name, fctx);
        let range = overflow_facts::expr_range(init, ty_range, &lookup);
        if range != ty_range {
            fctx.local_ranges.insert(ptr.to_string(), range);
        }
    }

    /// Whether `lhs op rhs` on `ty` provably cannot overflow. `lhs` is a
    /// place name for `place op= rhs`.
    pub(super) fn arith_overflow_proven(
        &self,
        op: BinOp,
        lhs: OverflowOperand<'_>,
        rhs: &ir::Expr,
        ty: &LType,
        fctx: &FnCtx,
    ) -> bool {
        if ArithOp::from_bin_op(op).is_none() {
            return false;
        }
        let Some(ty_range) = Self::ltype_range(ty) else {
            return false;
        };
        let lookup = |name: &str| Self::local_range(name, fctx);
        let lhs_range = match lhs {
            OverflowOperand::Expr(expr) => overflow_facts::expr_range(expr, ty_range, &lookup),
            OverflowOperand::Local(name) => lookup(name).unwrap_or(ty_range),
            OverflowOperand::Unknown => ty_range,
        };
        let rhs_range = overflow_facts::expr_range(rhs, ty_range, &lookup);
        overflow_facts::proves_binary(op, lhs_range, rhs_range, ty_range)
    }

    /// Whether `-operand` on `ty` provably cannot overflow.
    pub(super) fn negation_overflow_proven(
        &self,
        operand: &ir::Expr,
        ty: &LType,
        fctx: &FnCtx,
    ) -> bool {
        let Some(ty_range) = Self::ltype_range(ty) else {
            return false;
        };
        let lookup = |name: &str| Self::local_range(name, fctx);
        let range = overflow_facts::expr_range(operand, ty_range, &lookup);
        overflow_facts::proves_negation(range, ty_range)
    }

    /// Emits `llvm.<s|u><op>.with.overflow` and returns the wrapped result
    /// and the overflow flag.
    fn emit_overflowing(
        &mut self,
        op: ArithOp,
        ty: &LType,
        lhs: &str,
        rhs: &str,
        fctx: &mut FnCtx,
    ) -> (String, String) {
        let (signed, _) = int_shape(ty).unwrap_or((true, 64));
        let llvm_ty = llvm_type(ty);
        let name = format!(
            "llvm.{}{}.with.overflow.{llvm_ty}",
            if signed { "s" } else { "u" },
            op.plain_inst()
        );
        self.llvm_intrinsic_decls.insert(format!(
            "declare {{{llvm_ty}, i1}} @{name}({llvm_ty}, {llvm_ty})"
        ));
        let pair = self.new_temp();
        fctx.lines.push(format!(
            "  {pair} = call {{{llvm_ty}, i1}} @{name}({llvm_ty} {lhs}, {llvm_ty} {rhs})"
        ));
        let value = self.new_temp();
        fctx.lines.push(format!(
            "  {value} = extractvalue {{{llvm_ty}, i1}} {pair}, 0"
        ));
        let overflow = self.new_temp();
        fctx.lines.push(format!(
            "  {overflow} = extractvalue {{{llvm_ty}, i1}} {pair}, 1"
        ));
        (value, overflow)
    }

    /// `lhs op rhs` (or `0 - rhs` for `Neg`) that panics on overflow unless
    /// `proven`. Returns the result register.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_trapping_int_op(
        &mut self,
        op: ArithOp,
        ty: &LType,
        lhs: &str,
        rhs: &str,
        proven: bool,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> String {
        let llvm_ty = llvm_type(ty);
        if proven {
            let reg = self.new_temp();
            let flags = if is_unsigned_integer_type(ty) {
                "nuw"
            } else {
                "nsw"
            };
            fctx.lines.push(format!(
                "  {reg} = {} {flags} {llvm_ty} {lhs}, {rhs}",
                op.plain_inst()
            ));
            return reg;
        }
        let (value, overflow) = self.emit_overflowing(op, ty, lhs, rhs, fctx);
        let fail_label = self.new_label("int_overflow");
        let ok_label = self.new_label("int_in_range");
        fctx.lines.push(format!(
            "  br i1 {overflow}, label %{fail_label}, label %{ok_label}"
        ));
        fctx.lines.push(format!("{fail_label}:"));
        let type_name = render_type(ty);
        self.emit_static_panic(
            &format!(
                "integer_overflow{{op:{},type:{type_name}}}: {type_name} {} overflows",
                op.name(),
                op.noun()
            ),
            span,
            fctx,
        );
        fctx.lines.push(format!("{ok_label}:"));
        fctx.current_label = ok_label;
        value
    }

    /// `$arith:<mode>_<op>:<Type>(receiver[, operand])`: the explicit
    /// arithmetic methods.
    pub(super) fn gen_arith_method_call(
        &mut self,
        spec: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let parsed = spec.split_once(':').and_then(|(method, ty)| {
            let (mode, op) = method.split_once('_')?;
            Some((mode, ArithOp::from_name(op)?, ty))
        });
        let Some((mode, op, ty_repr)) = parsed else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("unknown arithmetic builtin '{spec}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let ty = self.parse_type_repr(ty_repr, span)?;
        let Some((signed, bits)) = int_shape(&ty) else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "arithmetic builtin '{spec}' expects an integer type, found '{}'",
                    render_type(&ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        let arity = if op == ArithOp::Neg { 1 } else { 2 };
        if args.len() != arity {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("arithmetic builtin '{spec}' expects {arity} argument(s)"),
                self.file,
                span,
            ));
            return None;
        }
        let mut operands = Vec::with_capacity(arity);
        for arg in args {
            let value = self.gen_expr_with_expected(arg, Some(&ty), fctx)?;
            let value = self.coerce_value_to_expected(value, &ty, arg.span, fctx)?;
            operands.push(value.repr.unwrap_or_else(|| "0".to_string()));
        }
        let (lhs, rhs) = if op == ArithOp::Neg {
            ("0".to_string(), operands[0].clone())
        } else {
            (operands[0].clone(), operands[1].clone())
        };
        let llvm_ty = llvm_type(&ty);
        let out = match (mode, op) {
            ("wrapping", ArithOp::Add | ArithOp::Sub | ArithOp::Mul | ArithOp::Neg) => {
                let reg = self.new_temp();
                fctx.lines.push(format!(
                    "  {reg} = {} {llvm_ty} {lhs}, {rhs}",
                    op.plain_inst()
                ));
                reg
            }
            ("checked", ArithOp::Add | ArithOp::Sub | ArithOp::Mul | ArithOp::Neg) => {
                let (value, overflow) = self.emit_overflowing(op, &ty, &lhs, &rhs, fctx);
                let fits = self.new_temp();
                fctx.lines
                    .push(format!("  {fits} = xor i1 {overflow}, true"));
                return self.wrap_checked_arith(value, &fits, &ty, span, fctx);
            }
            ("checked", ArithOp::Div | ArithOp::Rem) => {
                let (value, fits) =
                    self.emit_checked_division(op, signed, bits, &llvm_ty, &lhs, &rhs, fctx);
                return self.wrap_checked_arith(value, &fits, &ty, span, fctx);
            }
            ("saturating", ArithOp::Add | ArithOp::Sub) | ("saturating", ArithOp::Neg)
                if signed || op != ArithOp::Neg =>
            {
                let name = format!(
                    "llvm.{}{}.sat.{llvm_ty}",
                    if signed { "s" } else { "u" },
                    op.plain_inst()
                );
                self.llvm_intrinsic_decls
                    .insert(format!("declare {llvm_ty} @{name}({llvm_ty}, {llvm_ty})"));
                let reg = self.new_temp();
                fctx.lines.push(format!(
                    "  {reg} = call {llvm_ty} @{name}({llvm_ty} {lhs}, {llvm_ty} {rhs})"
                ));
                reg
            }
            ("saturating", ArithOp::Mul) => {
                let (value, overflow) = self.emit_overflowing(op, &ty, &lhs, &rhs, fctx);
                let (min, max) = int_limits(signed, bits);
                let bound = if signed {
                    // The exact product is negative when the signs differ.
                    let sign = self.new_temp();
                    fctx.lines
                        .push(format!("  {sign} = xor {llvm_ty} {lhs}, {rhs}"));
                    let negative = self.new_temp();
                    fctx.lines
                        .push(format!("  {negative} = icmp slt {llvm_ty} {sign}, 0"));
                    let bound = self.new_temp();
                    fctx.lines.push(format!(
                        "  {bound} = select i1 {negative}, {llvm_ty} {min}, {llvm_ty} {max}"
                    ));
                    bound
                } else {
                    max
                };
                let reg = self.new_temp();
                fctx.lines.push(format!(
                    "  {reg} = select i1 {overflow}, {llvm_ty} {bound}, {llvm_ty} {value}"
                ));
                reg
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    format!(
                        "arithmetic builtin '{spec}' is not supported for '{}'",
                        render_type(&ty)
                    ),
                    self.file,
                    span,
                ));
                return None;
            }
        };
        Some(Value {
            ty,
            repr: Some(out),
        })
    }

    /// `lhs / rhs` or `lhs % rhs` with a divisor that is never zero or, for a
    /// signed `MIN` dividend, `-1`. Returns the result and whether the
    /// operation is defined.
    #[allow(clippy::too_many_arguments)]
    fn emit_checked_division(
        &mut self,
        op: ArithOp,
        signed: bool,
        bits: u32,
        llvm_ty: &str,
        lhs: &str,
        rhs: &str,
        fctx: &mut FnCtx,
    ) -> (String, String) {
        let zero = self.new_temp();
        fctx.lines
            .push(format!("  {zero} = icmp eq {llvm_ty} {rhs}, 0"));
        let undefined = if signed {
            let (min, _) = int_limits(true, bits);
            let lhs_min = self.new_temp();
            fctx.lines
                .push(format!("  {lhs_min} = icmp eq {llvm_ty} {lhs}, {min}"));
            let rhs_minus_one = self.new_temp();
            fctx.lines
                .push(format!("  {rhs_minus_one} = icmp eq {llvm_ty} {rhs}, -1"));
            let overflow = self.new_temp();
            fctx.lines
                .push(format!("  {overflow} = and i1 {lhs_min}, {rhs_minus_one}"));
            let undefined = self.new_temp();
            fctx.lines
                .push(format!("  {undefined} = or i1 {zero}, {overflow}"));
            undefined
        } else {
            zero
        };
        let safe_rhs = self.new_temp();
        fctx.lines.push(format!(
            "  {safe_rhs} = select i1 {undefined}, {llvm_ty} 1, {llvm_ty} {rhs}"
        ));
        let inst = format!("{}{}", if signed { "s" } else { "u" }, op.plain_inst());
        let value = self.new_temp();
        fctx.lines
            .push(format!("  {value} = {inst} {llvm_ty} {lhs}, {safe_rhs}"));
        let fits = self.new_temp();
        fctx.lines
            .push(format!("  {fits} = xor i1 {undefined}, true"));
        (value, fits)
    }

    fn wrap_checked_arith(
        &mut self,
        value: String,
        fits: &str,
        ty: &LType,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let option_ty = self.parse_type_repr(&format!("Option[{}]", render_type(ty)), span)?;
        self.wrap_option_with_condition(
            &option_ty,
            Value {
                ty: ty.clone(),
                repr: Some(value),
            },
            fits,
            span,
            fctx,
        )
    }
}

/// The left operand of an arithmetic operation, for overflow proofs.
#[derive(Clone, Copy)]
pub(super) enum OverflowOperand<'e> {
    Expr(&'e ir::Expr),
    /// A local variable read by `local op= rhs`.
    Local(&'e str),
    /// A projected place (`p.x op= rhs`); only its type range is known.
    Unknown,
}
