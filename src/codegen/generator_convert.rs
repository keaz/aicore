//! Numeric conversions: `x as T` (lossless only, checked by the type
//! checker) and the conversion methods `checked_to_*`, `saturating_to_*`,
//! `wrapping_to_*` and `to_f32` / `to_f64`.
//!
//! Semantics:
//! - integer to integer: `exact` widens; `wrapping` keeps the low bits
//!   (two's complement); `saturating` clamps to the target range;
//!   `checked` returns `None` when the value does not fit.
//! - float to integer: values truncate toward zero; `checked` returns
//!   `None` for NaN, infinities and out-of-range values; `saturating`
//!   maps NaN to 0 and clamps (LLVM `fptosi.sat` / `fptoui.sat`).
//! - to a float (`round` / `exact`): rounds to nearest; float narrowing
//!   overflows to an infinity and keeps NaN.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Num {
    Int { signed: bool, bits: u32 },
    Float { bits: u32 },
}

fn num_kind(ty: &LType) -> Option<Num> {
    Some(match ty {
        LType::Int | LType::ISize | LType::Int64 => Num::Int {
            signed: true,
            bits: 64,
        },
        LType::USize | LType::UInt64 => Num::Int {
            signed: false,
            bits: 64,
        },
        LType::Int8 => Num::Int {
            signed: true,
            bits: 8,
        },
        LType::Int16 => Num::Int {
            signed: true,
            bits: 16,
        },
        LType::Int32 => Num::Int {
            signed: true,
            bits: 32,
        },
        LType::Int128 => Num::Int {
            signed: true,
            bits: 128,
        },
        LType::UInt8 => Num::Int {
            signed: false,
            bits: 8,
        },
        LType::UInt16 => Num::Int {
            signed: false,
            bits: 16,
        },
        LType::UInt32 => Num::Int {
            signed: false,
            bits: 32,
        },
        LType::UInt128 => Num::Int {
            signed: false,
            bits: 128,
        },
        LType::Float32 => Num::Float { bits: 32 },
        LType::Float64 | LType::Float => Num::Float { bits: 64 },
        _ => return None,
    })
}

/// Smallest and largest value of an integer type, as decimal text.
fn int_bounds(signed: bool, bits: u32) -> (String, String) {
    if signed {
        let max = (1u128 << (bits - 1)) - 1;
        (format!("-{}", max + 1), max.to_string())
    } else {
        let max = if bits == 128 {
            u128::MAX
        } else {
            (1u128 << bits) - 1
        };
        ("0".to_string(), max.to_string())
    }
}

/// An LLVM float constant (hex form, exact for both `float` and `double`
/// when the value is representable).
fn float_const(value: f64) -> String {
    format!("0x{:016X}", value.to_bits())
}

impl<'a> Generator<'a> {
    pub(super) fn gen_numeric_conversion_call(
        &mut self,
        mode: &str,
        target: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let [operand] = args else {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "numeric conversion expects one argument",
                self.file,
                span,
            ));
            return None;
        };
        let target_ty = self.parse_type_repr(target, span)?;
        let value = self.gen_expr(operand, fctx)?;
        self.gen_numeric_conversion(mode, value, target_ty, span, fctx)
    }

    fn gen_numeric_conversion(
        &mut self,
        mode: &str,
        value: Value,
        target: LType,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let (Some(from), Some(to)) = (num_kind(&value.ty), num_kind(&target)) else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "numeric conversion from '{}' to '{}' is not supported in codegen",
                    render_type(&value.ty),
                    render_type(&target)
                ),
                self.file,
                span,
            ));
            return None;
        };
        let repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let src = llvm_type(&value.ty);
        let dst = llvm_type(&target);
        let converted = |this: &mut Self, op: &str, fctx: &mut FnCtx| -> String {
            let reg = this.new_temp();
            fctx.lines
                .push(format!("  {reg} = {op} {src} {repr} to {dst}"));
            reg
        };
        let out = match (mode, from, to) {
            // Same type: unchanged (`checked` still wraps it in `Some`).
            (_, Num::Int { .. }, Num::Int { .. }) if value.ty == target && mode != "checked" => {
                repr.clone()
            }
            (_, Num::Float { bits: b1 }, Num::Float { bits: b2 }) if b1 == b2 => repr.clone(),
            // Widening, and the low bits for `wrapping`.
            ("exact" | "wrapping", Num::Int { signed, bits: b1 }, Num::Int { bits: b2, .. }) => {
                if b2 > b1 {
                    converted(self, if signed { "sext" } else { "zext" }, fctx)
                } else if b2 < b1 {
                    converted(self, "trunc", fctx)
                } else {
                    repr.clone()
                }
            }
            (
                "saturating",
                Num::Int {
                    signed: from_signed,
                    bits: from_bits,
                },
                Num::Int { signed, bits },
            ) => self.int_saturate(&repr, (from_signed, from_bits), (signed, bits), &dst, fctx),
            (
                "checked",
                Num::Int {
                    signed: from_signed,
                    bits: from_bits,
                },
                Num::Int { signed, bits },
            ) => {
                let (fits, narrowed) =
                    self.int_fits(&repr, (from_signed, from_bits), (signed, bits), &dst, fctx);
                return self.checked_result(narrowed, fits, &target, span, fctx);
            }
            ("exact" | "round", Num::Int { signed, .. }, Num::Float { .. }) => {
                converted(self, if signed { "sitofp" } else { "uitofp" }, fctx)
            }
            ("exact" | "round", Num::Float { bits: b1 }, Num::Float { bits: b2 }) => {
                converted(self, if b2 > b1 { "fpext" } else { "fptrunc" }, fctx)
            }
            ("saturating", Num::Float { .. }, Num::Int { signed, bits }) => {
                let name = format!(
                    "llvm.{}.sat.i{bits}.{src}",
                    if signed { "fptosi" } else { "fptoui" }
                );
                self.llvm_intrinsic_decls
                    .insert(format!("declare {dst} @{name}({src})"));
                let reg = self.new_temp();
                fctx.lines
                    .push(format!("  {reg} = call {dst} @{name}({src} {repr})"));
                reg
            }
            ("checked", Num::Float { bits: float_bits }, Num::Int { signed, bits }) => {
                let fits = self.float_fits_int(&repr, &src, float_bits, signed, bits, fctx);
                let reg = converted(self, if signed { "fptosi" } else { "fptoui" }, fctx);
                // The conversion result is unused when `fits` is false.
                let frozen = self.new_temp();
                fctx.lines.push(format!("  {frozen} = freeze {dst} {reg}"));
                return self.checked_result(frozen, fits, &target, span, fctx);
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    format!(
                        "numeric conversion '{mode}' from '{}' to '{}' is not supported",
                        render_type(&value.ty),
                        render_type(&target)
                    ),
                    self.file,
                    span,
                ));
                return None;
            }
        };
        Some(Value {
            ty: target,
            repr: Some(out),
        })
    }

    /// Extends an integer of `(signed, bits)` to `bits` (by its own
    /// signedness).
    fn int_widen_to(
        &mut self,
        repr: &str,
        from: (bool, u32),
        bits: u32,
        fctx: &mut FnCtx,
    ) -> String {
        let (signed, from_bits) = from;
        if bits == from_bits {
            return repr.to_string();
        }
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {reg} = {} i{from_bits} {repr} to i{bits}",
            if signed { "sext" } else { "zext" }
        ));
        reg
    }

    /// Whether the integer `repr` fits the target integer type, and the
    /// value truncated to it.
    fn int_fits(
        &mut self,
        repr: &str,
        from: (bool, u32),
        to: (bool, u32),
        dst: &str,
        fctx: &mut FnCtx,
    ) -> (String, String) {
        let ((_, b1), (signed, b2)) = (from, to);
        // Compare in a width that holds both ranges.
        let wide = b1.max(b2) + 1;
        let wide_value = self.int_widen_to(repr, from, wide, fctx);
        let (min, max) = int_bounds(signed, b2);
        let ge = self.new_temp();
        fctx.lines
            .push(format!("  {ge} = icmp sge i{wide} {wide_value}, {min}"));
        let le = self.new_temp();
        fctx.lines
            .push(format!("  {le} = icmp sle i{wide} {wide_value}, {max}"));
        let fits = self.new_temp();
        fctx.lines.push(format!("  {fits} = and i1 {ge}, {le}"));
        let narrowed = self.new_temp();
        fctx.lines.push(format!(
            "  {narrowed} = trunc i{wide} {wide_value} to {dst}"
        ));
        (fits, narrowed)
    }

    fn int_saturate(
        &mut self,
        repr: &str,
        from: (bool, u32),
        to: (bool, u32),
        dst: &str,
        fctx: &mut FnCtx,
    ) -> String {
        let ((_, b1), (signed, b2)) = (from, to);
        let wide = b1.max(b2) + 1;
        let wide_value = self.int_widen_to(repr, from, wide, fctx);
        let (min, max) = int_bounds(signed, b2);
        let below = self.new_temp();
        fctx.lines
            .push(format!("  {below} = icmp slt i{wide} {wide_value}, {min}"));
        let low = self.new_temp();
        fctx.lines.push(format!(
            "  {low} = select i1 {below}, i{wide} {min}, i{wide} {wide_value}"
        ));
        let above = self.new_temp();
        fctx.lines
            .push(format!("  {above} = icmp sgt i{wide} {low}, {max}"));
        let clamped = self.new_temp();
        fctx.lines.push(format!(
            "  {clamped} = select i1 {above}, i{wide} {max}, i{wide} {low}"
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {out} = trunc i{wide} {clamped} to {dst}"));
        out
    }

    /// Whether the float `repr` truncates (toward zero) to a value of the
    /// integer type; false for NaN and infinities.
    fn float_fits_int(
        &mut self,
        repr: &str,
        src: &str,
        float_bits: u32,
        signed: bool,
        bits: u32,
        fctx: &mut FnCtx,
    ) -> String {
        let mantissa = if float_bits == 32 { 24 } else { 53 };
        let (lower_op, lower) = if signed {
            let min = -(2f64.powi(bits as i32 - 1));
            // `min - 1` is exact only for narrow targets; otherwise values
            // between it and `min` are not representable anyway.
            if bits - 1 < mantissa {
                ("ogt", min - 1.0)
            } else {
                ("oge", min)
            }
        } else {
            ("ogt", -1.0)
        };
        let mut upper = if signed {
            2f64.powi(bits as i32 - 1)
        } else {
            2f64.powi(bits as i32)
        };
        if float_bits == 32 && upper > f64::from(f32::MAX) {
            // 2^128 is not a `float`; every finite `float` is below it.
            upper = f64::INFINITY;
        }
        let lower_ok = self.new_temp();
        fctx.lines.push(format!(
            "  {lower_ok} = fcmp {lower_op} {src} {repr}, {}",
            float_const(lower)
        ));
        let upper_ok = self.new_temp();
        fctx.lines.push(format!(
            "  {upper_ok} = fcmp olt {src} {repr}, {}",
            float_const(upper)
        ));
        let fits = self.new_temp();
        fctx.lines
            .push(format!("  {fits} = and i1 {lower_ok}, {upper_ok}"));
        fits
    }

    fn checked_result(
        &mut self,
        value: String,
        fits: String,
        target: &LType,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let option_ty = self.parse_type_repr(&format!("Option[{}]", render_type(target)), span)?;
        self.wrap_option_with_condition(
            &option_ty,
            Value {
                ty: target.clone(),
                repr: Some(value),
            },
            &fits,
            span,
            fctx,
        )
    }
}
