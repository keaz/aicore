//! Compile-time evaluation of `const` initializers and struct field defaults.
//!
//! The type checker and the LLVM backend share this evaluator so that every
//! initializer `aic check` accepts also evaluates in `aic build`. Integer
//! values are exact (`i128`) and carry the integer kind they were typed with;
//! a result that does not fit its kind is an overflow error instead of a
//! silently wrapped value.

use crate::ast::{BinOp, UnaryOp};
use crate::ir;
use crate::span::Span;

/// Width and signedness of a compile-time integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntKind {
    pub signed: bool,
    pub bits: u8,
}

impl IntKind {
    pub const INT: IntKind = IntKind {
        signed: true,
        bits: 64,
    };

    /// Kind for a canonical integer type name (`Int`, `UInt8`, `USize`, ...).
    pub fn from_type_name(name: &str) -> Option<IntKind> {
        let (signed, bits) = match name.trim() {
            "Int" | "Int64" | "ISize" => (true, 64),
            "Int8" => (true, 8),
            "Int16" => (true, 16),
            "Int32" => (true, 32),
            "Int128" => (true, 128),
            "UInt" | "USize" | "UInt64" => (false, 64),
            "UInt8" => (false, 8),
            "UInt16" => (false, 16),
            "UInt32" => (false, 32),
            "UInt128" => (false, 128),
            _ => return None,
        };
        Some(IntKind { signed, bits })
    }

    pub fn min(self) -> i128 {
        if !self.signed {
            0
        } else if self.bits == 128 {
            i128::MIN
        } else {
            -(1_i128 << (self.bits - 1))
        }
    }

    /// Largest value of the kind that `i128` can hold (`UInt128` is capped
    /// at `i128::MAX`).
    pub fn max(self) -> i128 {
        match (self.signed, self.bits) {
            (_, 128) => i128::MAX,
            (true, bits) => (1_i128 << (bits - 1)) - 1,
            (false, bits) => (1_i128 << bits) - 1,
        }
    }

    /// Largest value of the kind as an unsigned number, exact for `UInt128`.
    fn max_unsigned(self) -> u128 {
        if !self.signed && self.bits == 128 {
            u128::MAX
        } else {
            self.max() as u128
        }
    }

    /// The kind's range as `min..=max` text, with `UInt128`'s true maximum.
    pub fn range_text(self) -> String {
        if self.signed {
            format!("{}..={}", self.min(), self.max())
        } else {
            format!("0..={}", self.max_unsigned())
        }
    }

    pub fn contains(self, value: i128) -> bool {
        value >= self.min() && value <= self.max()
    }

    /// Truncates `value` to the kind's width with two's-complement wrapping.
    fn wrap(self, value: i128) -> i128 {
        if self.bits == 128 {
            return value;
        }
        let mask = (1_i128 << self.bits) - 1;
        let low = value & mask;
        if self.signed && low > self.max() {
            low - (1_i128 << self.bits)
        } else {
            low
        }
    }

    pub fn type_name(self) -> String {
        match (self.signed, self.bits) {
            (true, 64) => "Int".to_string(),
            (signed, bits) => format!("{}{}", if signed { "Int" } else { "UInt" }, bits),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    /// An integer; `kind` is `None` for an unsuffixed literal that has not met
    /// a typed operand yet.
    Int {
        value: i128,
        kind: Option<IntKind>,
    },
    Float(f64),
    Bool(bool),
    Char(char),
    Unit,
    String(String),
}

impl ConstValue {
    pub fn kind_name(&self) -> &'static str {
        match self {
            ConstValue::Int { .. } => "Int",
            ConstValue::Float(_) => "Float",
            ConstValue::Bool(_) => "Bool",
            ConstValue::Char(_) => "Char",
            ConstValue::Unit => "()",
            ConstValue::String(_) => "String",
        }
    }

    /// Bit pattern of an integer as a signed value of `bits` width, which is
    /// how LLVM spells constants of that width.
    pub fn int_repr_for_width(value: i128, bits: u8) -> i128 {
        IntKind { signed: true, bits }.wrap(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstEvalErrorKind {
    DivisionByZero,
    ModuloByZero,
    /// The result does not fit its integer kind.
    Overflow {
        kind: IntKind,
    },
    ShiftOutOfRange {
        amount: i128,
        kind: IntKind,
    },
    UnsupportedUnary {
        op: UnaryOp,
        operand: &'static str,
    },
    UnsupportedBinary {
        op: BinOp,
        lhs: &'static str,
        rhs: &'static str,
    },
    /// The expression form is not a compile-time expression.
    UnsupportedExpr(&'static str),
    UnknownConst(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstEvalError {
    pub kind: ConstEvalErrorKind,
    pub span: Span,
}

impl ConstEvalError {
    pub fn message(&self) -> String {
        match &self.kind {
            ConstEvalErrorKind::DivisionByZero => "divides by zero".to_string(),
            ConstEvalErrorKind::ModuloByZero => "computes a modulo by zero".to_string(),
            ConstEvalErrorKind::Overflow { kind } => format!(
                "overflows '{}' (range {})",
                kind.type_name(),
                kind.range_text()
            ),
            ConstEvalErrorKind::ShiftOutOfRange { amount, kind } => format!(
                "shifts a value of type '{}' by {} bits (allowed 0..{})",
                kind.type_name(),
                amount,
                kind.bits
            ),
            ConstEvalErrorKind::UnsupportedUnary { op, operand } => format!(
                "applies unary '{}' to a {} constant",
                unary_op_symbol(*op),
                operand
            ),
            ConstEvalErrorKind::UnsupportedBinary { op, lhs, rhs } => format!(
                "applies '{}' to {} and {} constants",
                binary_op_symbol(*op),
                lhs,
                rhs
            ),
            ConstEvalErrorKind::UnsupportedExpr(what) => format!("uses unsupported {what}"),
            ConstEvalErrorKind::UnknownConst(name) => {
                format!("references unknown constant '{name}'")
            }
        }
    }
}

/// Result of looking up a referenced constant.
pub enum ConstLookup {
    Value(ConstValue),
    /// The constant failed to evaluate and that failure was already reported.
    Failed,
    Unknown,
}

/// Evaluates `expr`. `expected` is the integer kind of the slot the value is
/// stored into; unsuffixed literals take it. `lookup` resolves references to
/// other constants. `Ok(None)` means a referenced constant already failed.
pub fn eval_expr(
    expr: &ir::Expr,
    expected: Option<IntKind>,
    lookup: &mut dyn FnMut(&str) -> ConstLookup,
) -> Result<Option<ConstValue>, ConstEvalError> {
    let value = match &expr.kind {
        ir::ExprKind::Int(value, meta) => {
            let exact = match meta {
                Some(meta) => crate::lexer::parse_int_literal_text(&meta.raw_literal_text)
                    .and_then(|v| i128::try_from(v).ok()),
                None => Some(*value as i128),
            };
            let kind = meta.as_deref().map(|meta| literal_suffix_kind(meta.kind));
            let Some(exact) = exact else {
                return Err(ConstEvalError {
                    kind: ConstEvalErrorKind::Overflow {
                        kind: kind.unwrap_or(IntKind {
                            signed: true,
                            bits: 128,
                        }),
                    },
                    span: expr.span,
                });
            };
            ConstValue::Int { value: exact, kind }
        }
        ir::ExprKind::Float(value, ..) => ConstValue::Float(*value),
        ir::ExprKind::Bool(value) => ConstValue::Bool(*value),
        ir::ExprKind::Char(value) => ConstValue::Char(*value),
        ir::ExprKind::String(value) => ConstValue::String(value.clone()),
        ir::ExprKind::Unit => ConstValue::Unit,
        ir::ExprKind::Var(name) => match lookup(name) {
            ConstLookup::Value(value) => value,
            ConstLookup::Failed => return Ok(None),
            ConstLookup::Unknown => {
                return Err(ConstEvalError {
                    kind: ConstEvalErrorKind::UnknownConst(name.clone()),
                    span: expr.span,
                })
            }
        },
        ir::ExprKind::Unary { op, expr: inner } => {
            let Some(value) = eval_expr(inner, expected, lookup)? else {
                return Ok(None);
            };
            eval_unary(*op, value, expected, expr.span)?
        }
        ir::ExprKind::Binary { op, lhs, rhs } => {
            let operand_expected = if is_comparison(*op) { None } else { expected };
            let Some(lhs) = eval_expr(lhs, operand_expected, lookup)? else {
                return Ok(None);
            };
            let Some(rhs) = eval_expr(rhs, operand_expected, lookup)? else {
                return Ok(None);
            };
            eval_binary(*op, lhs, rhs, expected, expr.span)?
        }
        other => {
            return Err(ConstEvalError {
                kind: ConstEvalErrorKind::UnsupportedExpr(expr_form_name(other)),
                span: expr.span,
            })
        }
    };
    Ok(Some(value))
}

/// Outcome of [`literal_binary_overflow`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralArith {
    /// The exact result fits the kind.
    Fits,
    /// The operation overflows the kind on every run.
    Overflows,
    /// An operand is not integer literal arithmetic, is a literal that does
    /// not fit the kind on its own, overflows on its own, or divides by zero.
    /// Those are reported at the operand or by other checks.
    NotConstant,
}

/// Whether `lhs op rhs` (`+`, `-`, `*` or `/`), where both operands are
/// integer literal arithmetic of `kind`, overflows `kind`. Every nested
/// operation is checked against `kind`, as it is at run time, and a negated
/// literal is a constant (`-128i8` is `Int8`'s minimum). The evaluation is
/// exact on every width: unsigned kinds use `u128`, so `UInt128` values above
/// `i128::MAX` are not mistaken for overflow.
pub fn literal_binary_overflow(
    op: BinOp,
    lhs: &ir::Expr,
    rhs: &ir::Expr,
    kind: IntKind,
) -> LiteralArith {
    let (Ok(lhs), Ok(rhs)) = (literal_value(lhs, kind), literal_value(rhs, kind)) else {
        return LiteralArith::NotConstant;
    };
    match literal_apply(op, lhs, rhs, kind) {
        Ok(_) => LiteralArith::Fits,
        Err(LiteralFail::Overflow) => LiteralArith::Overflows,
        Err(LiteralFail::NotConstant) => LiteralArith::NotConstant,
    }
}

/// An exact integer of a known kind: `i128` holds every signed value and
/// `u128` every unsigned one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exact {
    Signed(i128),
    Unsigned(u128),
}

enum LiteralFail {
    Overflow,
    NotConstant,
}

fn literal_magnitude(expr: &ir::Expr) -> Option<u128> {
    match &expr.kind {
        ir::ExprKind::Int(value, meta) => match meta {
            Some(meta) => crate::lexer::parse_int_literal_text(&meta.raw_literal_text),
            None => u128::try_from(*value).ok(),
        },
        _ => None,
    }
}

fn literal_value(expr: &ir::Expr, kind: IntKind) -> Result<Exact, LiteralFail> {
    match &expr.kind {
        ir::ExprKind::Int(..) => {
            let magnitude = literal_magnitude(expr).ok_or(LiteralFail::NotConstant)?;
            if kind.signed {
                i128::try_from(magnitude)
                    .ok()
                    .filter(|value| kind.contains(*value))
                    .map(Exact::Signed)
                    .ok_or(LiteralFail::NotConstant)
            } else if magnitude <= kind.max_unsigned() {
                Ok(Exact::Unsigned(magnitude))
            } else {
                Err(LiteralFail::NotConstant)
            }
        }
        ir::ExprKind::Unary {
            op: UnaryOp::Neg,
            expr: inner,
        } if kind.signed => {
            if let Some(magnitude) = literal_magnitude(inner) {
                // `-literal` is a constant, whose magnitude may be one past
                // the kind's maximum.
                let min_magnitude = kind.min().unsigned_abs();
                return if magnitude <= min_magnitude {
                    Ok(Exact::Signed(0i128.wrapping_sub_unsigned(magnitude)))
                } else {
                    Err(LiteralFail::NotConstant)
                };
            }
            match literal_value(inner, kind)? {
                Exact::Signed(value) => value
                    .checked_neg()
                    .filter(|negated| kind.contains(*negated))
                    .map(Exact::Signed)
                    .ok_or(LiteralFail::Overflow),
                Exact::Unsigned(_) => Err(LiteralFail::NotConstant),
            }
        }
        ir::ExprKind::Binary { op, lhs, rhs } => {
            let lhs = literal_value(lhs, kind)?;
            let rhs = literal_value(rhs, kind)?;
            literal_apply(*op, lhs, rhs, kind)
        }
        _ => Err(LiteralFail::NotConstant),
    }
}

fn literal_apply(op: BinOp, lhs: Exact, rhs: Exact, kind: IntKind) -> Result<Exact, LiteralFail> {
    match (lhs, rhs) {
        (Exact::Signed(lhs), Exact::Signed(rhs)) => {
            let result = match op {
                BinOp::Add => lhs.checked_add(rhs),
                BinOp::Sub => lhs.checked_sub(rhs),
                BinOp::Mul => lhs.checked_mul(rhs),
                BinOp::Div if rhs == 0 => return Err(LiteralFail::NotConstant),
                BinOp::Div => lhs.checked_div(rhs),
                _ => return Err(LiteralFail::NotConstant),
            };
            result
                .filter(|value| kind.contains(*value))
                .map(Exact::Signed)
                .ok_or(LiteralFail::Overflow)
        }
        (Exact::Unsigned(lhs), Exact::Unsigned(rhs)) => {
            let result = match op {
                BinOp::Add => lhs.checked_add(rhs),
                BinOp::Sub => lhs.checked_sub(rhs),
                BinOp::Mul => lhs.checked_mul(rhs),
                BinOp::Div if rhs == 0 => return Err(LiteralFail::NotConstant),
                BinOp::Div => lhs.checked_div(rhs),
                _ => return Err(LiteralFail::NotConstant),
            };
            result
                .filter(|value| *value <= kind.max_unsigned())
                .map(Exact::Unsigned)
                .ok_or(LiteralFail::Overflow)
        }
        _ => Err(LiteralFail::NotConstant),
    }
}

/// Gives an untyped integer the kind of its slot and checks that it fits.
/// Used for the final value of a constant or field default.
pub fn fit_to_slot(
    value: ConstValue,
    slot: Option<IntKind>,
    span: Span,
) -> Result<ConstValue, ConstEvalError> {
    match (value, slot) {
        (ConstValue::Int { value, .. }, Some(kind)) => {
            if kind.contains(value) {
                Ok(ConstValue::Int {
                    value,
                    kind: Some(kind),
                })
            } else {
                Err(ConstEvalError {
                    kind: ConstEvalErrorKind::Overflow { kind },
                    span,
                })
            }
        }
        (value, _) => Ok(value),
    }
}

fn eval_unary(
    op: UnaryOp,
    value: ConstValue,
    expected: Option<IntKind>,
    span: Span,
) -> Result<ConstValue, ConstEvalError> {
    match (op, value) {
        (UnaryOp::Neg, ConstValue::Int { value, kind }) => {
            let kind_for_check = kind.or(expected).unwrap_or(IntKind::INT);
            let negated = -value;
            if !kind_for_check.contains(negated) {
                return Err(ConstEvalError {
                    kind: ConstEvalErrorKind::Overflow {
                        kind: kind_for_check,
                    },
                    span,
                });
            }
            Ok(ConstValue::Int {
                value: negated,
                kind,
            })
        }
        (UnaryOp::Neg, ConstValue::Float(v)) => Ok(ConstValue::Float(-v)),
        (UnaryOp::Not, ConstValue::Bool(v)) => Ok(ConstValue::Bool(!v)),
        (UnaryOp::BitNot, ConstValue::Int { value, kind }) => {
            let kind_for_op = kind.or(expected).unwrap_or(IntKind::INT);
            let inverted = if kind_for_op.signed {
                !value
            } else {
                kind_for_op.max() ^ value
            };
            Ok(ConstValue::Int {
                value: inverted,
                kind: Some(kind_for_op),
            })
        }
        (op, value) => Err(ConstEvalError {
            kind: ConstEvalErrorKind::UnsupportedUnary {
                op,
                operand: value.kind_name(),
            },
            span,
        }),
    }
}

fn eval_binary(
    op: BinOp,
    lhs: ConstValue,
    rhs: ConstValue,
    expected: Option<IntKind>,
    span: Span,
) -> Result<ConstValue, ConstEvalError> {
    let unsupported = |lhs: &ConstValue, rhs: &ConstValue| ConstEvalError {
        kind: ConstEvalErrorKind::UnsupportedBinary {
            op,
            lhs: lhs.kind_name(),
            rhs: rhs.kind_name(),
        },
        span,
    };
    let err = |kind| ConstEvalError { kind, span };

    if is_comparison(op) {
        let ordering = match (&lhs, &rhs) {
            (ConstValue::Int { value: a, .. }, ConstValue::Int { value: b, .. }) => {
                a.partial_cmp(b)
            }
            (ConstValue::Float(a), ConstValue::Float(b)) => a.partial_cmp(b),
            (ConstValue::Char(a), ConstValue::Char(b)) => a.partial_cmp(b),
            (ConstValue::String(a), ConstValue::String(b)) => {
                a.as_bytes().partial_cmp(b.as_bytes())
            }
            (ConstValue::Bool(a), ConstValue::Bool(b)) if matches!(op, BinOp::Eq | BinOp::Ne) => {
                a.partial_cmp(b)
            }
            _ => return Err(unsupported(&lhs, &rhs)),
        };
        let result = match ordering {
            Some(ordering) => match op {
                BinOp::Eq => ordering.is_eq(),
                BinOp::Ne => ordering.is_ne(),
                BinOp::Lt => ordering.is_lt(),
                BinOp::Le => ordering.is_le(),
                BinOp::Gt => ordering.is_gt(),
                BinOp::Ge => ordering.is_ge(),
                _ => unreachable!("comparison operator"),
            },
            // NaN compares unequal to everything.
            None => matches!(op, BinOp::Ne),
        };
        return Ok(ConstValue::Bool(result));
    }

    match op {
        BinOp::And | BinOp::Or => match (&lhs, &rhs) {
            (ConstValue::Bool(a), ConstValue::Bool(b)) => {
                Ok(ConstValue::Bool(if op == BinOp::And {
                    *a && *b
                } else {
                    *a || *b
                }))
            }
            _ => Err(unsupported(&lhs, &rhs)),
        },
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => match (&lhs, &rhs) {
            (ConstValue::Int { value: a, kind: ka }, ConstValue::Int { value: b, kind: kb }) => {
                let kind = ka.or(*kb).or(expected).unwrap_or(IntKind::INT);
                let result = match op {
                    BinOp::Add => a.checked_add(*b),
                    BinOp::Sub => a.checked_sub(*b),
                    BinOp::Mul => a.checked_mul(*b),
                    BinOp::Div => {
                        if *b == 0 {
                            return Err(err(ConstEvalErrorKind::DivisionByZero));
                        }
                        a.checked_div(*b)
                    }
                    BinOp::Mod => {
                        if *b == 0 {
                            return Err(err(ConstEvalErrorKind::ModuloByZero));
                        }
                        a.checked_rem(*b)
                    }
                    _ => unreachable!("arithmetic operator"),
                };
                match result {
                    Some(value) if kind.contains(value) => Ok(ConstValue::Int {
                        value,
                        kind: ka.or(*kb),
                    }),
                    _ => Err(err(ConstEvalErrorKind::Overflow { kind })),
                }
            }
            (ConstValue::Float(a), ConstValue::Float(b)) if op != BinOp::Mod => {
                Ok(ConstValue::Float(match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div => a / b,
                    _ => unreachable!("float arithmetic operator"),
                }))
            }
            (ConstValue::String(a), ConstValue::String(b)) if op == BinOp::Add => {
                Ok(ConstValue::String(format!("{a}{b}")))
            }
            _ => Err(unsupported(&lhs, &rhs)),
        },
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr | BinOp::Ushr => {
            let (ConstValue::Int { value: a, kind: ka }, ConstValue::Int { value: b, kind: kb }) =
                (&lhs, &rhs)
            else {
                return Err(unsupported(&lhs, &rhs));
            };
            let is_shift = matches!(op, BinOp::Shl | BinOp::Shr | BinOp::Ushr);
            // A shift keeps the left operand's kind; the amount may differ.
            let kind = if is_shift {
                ka.or(expected).unwrap_or(IntKind::INT)
            } else {
                ka.or(*kb).or(expected).unwrap_or(IntKind::INT)
            };
            if is_shift && (*b < 0 || *b >= kind.bits as i128) {
                return Err(err(ConstEvalErrorKind::ShiftOutOfRange {
                    amount: *b,
                    kind,
                }));
            }
            let result = match op {
                BinOp::BitAnd => a & b,
                BinOp::BitOr => a | b,
                BinOp::BitXor => a ^ b,
                BinOp::Shl => kind.wrap(a.wrapping_shl(*b as u32)),
                BinOp::Shr if kind.signed => a >> b,
                BinOp::Shr | BinOp::Ushr => {
                    let unsigned = IntKind {
                        signed: false,
                        bits: kind.bits,
                    };
                    let bits = if kind.bits == 128 {
                        (*a as u128) >> b
                    } else {
                        (unsigned.wrap(*a) as u128) >> b
                    };
                    kind.wrap(bits as i128)
                }
                _ => unreachable!("bitwise operator"),
            };
            Ok(ConstValue::Int {
                value: kind.wrap(result),
                kind: if is_shift { *ka } else { ka.or(*kb) },
            })
        }
        _ => unreachable!("comparisons handled above"),
    }
}

fn is_comparison(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    )
}

fn literal_suffix_kind(kind: ir::IntLiteralKind) -> IntKind {
    let signed = matches!(kind.signedness, ir::IntLiteralSignedness::Signed);
    let bits = match kind.width {
        ir::IntLiteralWidth::W8 => 8,
        ir::IntLiteralWidth::W16 => 16,
        ir::IntLiteralWidth::W32 => 32,
        ir::IntLiteralWidth::W64 => 64,
        ir::IntLiteralWidth::W128 => 128,
    };
    IntKind { signed, bits }
}

fn expr_form_name(kind: &ir::ExprKind) -> &'static str {
    match kind {
        ir::ExprKind::Call { .. } => "function call",
        ir::ExprKind::TemplateLiteral { .. } => "template literal",
        ir::ExprKind::Closure { .. } => "closure expression",
        ir::ExprKind::If { .. } => "`if` expression",
        ir::ExprKind::While { .. } => "`while` expression",
        ir::ExprKind::Loop { .. } => "`loop` expression",
        ir::ExprKind::Break { .. } => "`break` expression",
        ir::ExprKind::Return { .. } => "`return` expression",
        ir::ExprKind::Continue { .. } => "`continue` expression",
        ir::ExprKind::Match { .. } => "`match` expression",
        ir::ExprKind::Borrow { .. } => "borrow expression",
        ir::ExprKind::Await { .. } => "`await` expression",
        ir::ExprKind::Try { .. } => "`?` expression",
        ir::ExprKind::UnsafeBlock { .. } => "`unsafe` block",
        ir::ExprKind::Block { .. } => "block expression",
        ir::ExprKind::StructInit { .. } => "struct construction",
        ir::ExprKind::FieldAccess { .. } => "field access",
        _ => "expression",
    }
}

pub fn unary_op_symbol(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
        UnaryOp::BitNot => "~",
    }
}

pub fn binary_op_symbol(op: BinOp) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Expr, ExprKind, NodeId};

    fn expr(kind: ExprKind) -> Expr {
        Expr {
            node: NodeId(0),
            kind,
            span: Span::new(0, 0),
        }
    }

    fn int(v: i64) -> Expr {
        expr(ExprKind::Int(v, None))
    }

    fn bin(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        expr(ExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        })
    }

    fn eval(e: &Expr, slot: Option<IntKind>) -> Result<ConstValue, ConstEvalErrorKind> {
        let mut lookup = |_: &str| ConstLookup::Unknown;
        eval_expr(e, slot, &mut lookup)
            .and_then(|value| fit_to_slot(value.expect("value"), slot, e.span))
            .map_err(|err| err.kind)
    }

    const I8: IntKind = IntKind {
        signed: true,
        bits: 8,
    };
    const U8: IntKind = IntKind {
        signed: false,
        bits: 8,
    };

    fn int_value(value: Result<ConstValue, ConstEvalErrorKind>) -> i128 {
        match value {
            Ok(ConstValue::Int { value, .. }) => value,
            other => panic!("expected an integer, got {other:?}"),
        }
    }

    #[test]
    fn kinds_have_exact_ranges() {
        assert_eq!((I8.min(), I8.max()), (-128, 127));
        assert_eq!((U8.min(), U8.max()), (0, 255));
        let u64k = IntKind::from_type_name("UInt64").unwrap();
        assert_eq!(u64k.max(), u64::MAX as i128);
        assert_eq!(IntKind::from_type_name("UInt"), Some(u64k));
        assert_eq!(IntKind::from_type_name("Int"), Some(IntKind::INT));
        assert_eq!(IntKind::from_type_name("String"), None);
        assert_eq!(ConstValue::int_repr_for_width(u64::MAX as i128, 64), -1);
        assert_eq!(ConstValue::int_repr_for_width(255, 8), -1);
        assert_eq!(ConstValue::int_repr_for_width(127, 8), 127);
        let u128k = IntKind::from_type_name("UInt128").unwrap();
        assert_eq!(u128k.range_text(), format!("0..={}", u128::MAX));
        let i128k = IntKind::from_type_name("Int128").unwrap();
        assert_eq!(i128k.range_text(), format!("{}..={}", i128::MIN, i128::MAX));
        assert_eq!(U8.range_text(), "0..=255");
        assert_eq!(I8.range_text(), "-128..=127");
    }

    #[test]
    fn arithmetic_is_checked_against_the_slot_kind() {
        assert_eq!(
            int_value(eval(&bin(BinOp::Add, int(100), int(27)), Some(I8))),
            127
        );
        assert_eq!(
            eval(&bin(BinOp::Add, int(100), int(28)), Some(I8)),
            Err(ConstEvalErrorKind::Overflow { kind: I8 })
        );
        assert_eq!(
            eval(&bin(BinOp::Sub, int(3), int(4)), Some(U8)),
            Err(ConstEvalErrorKind::Overflow { kind: U8 })
        );
        assert_eq!(
            eval(&bin(BinOp::Div, int(i64::MIN), int(-1)), None),
            Err(ConstEvalErrorKind::Overflow { kind: IntKind::INT })
        );
        assert_eq!(
            eval(&bin(BinOp::Div, int(1), int(0)), None),
            Err(ConstEvalErrorKind::DivisionByZero)
        );
        assert_eq!(
            eval(&bin(BinOp::Mod, int(1), int(0)), None),
            Err(ConstEvalErrorKind::ModuloByZero)
        );
        assert_eq!(int_value(eval(&bin(BinOp::Mod, int(-7), int(2)), None)), -1);
        assert_eq!(int_value(eval(&bin(BinOp::Div, int(-7), int(2)), None)), -3);
    }

    #[test]
    fn shifts_follow_the_left_operand_width() {
        assert_eq!(
            int_value(eval(&bin(BinOp::Shl, int(1), int(7)), Some(I8))),
            -128
        );
        assert_eq!(
            eval(&bin(BinOp::Shl, int(1), int(8)), Some(I8)),
            Err(ConstEvalErrorKind::ShiftOutOfRange {
                amount: 8,
                kind: I8
            })
        );
        assert_eq!(int_value(eval(&bin(BinOp::Shr, int(-8), int(1)), None)), -4);
        assert_eq!(
            int_value(eval(&bin(BinOp::Ushr, int(-1), int(60)), None)),
            15
        );
        assert_eq!(
            int_value(eval(&bin(BinOp::Shr, int(255), int(4)), Some(U8))),
            15
        );
    }

    #[test]
    fn bit_not_respects_signedness() {
        let not = |v| {
            expr(ExprKind::Unary {
                op: UnaryOp::BitNot,
                expr: Box::new(int(v)),
            })
        };
        assert_eq!(int_value(eval(&not(0), Some(U8))), 255);
        assert_eq!(int_value(eval(&not(0), Some(I8))), -1);
        let neg = expr(ExprKind::Unary {
            op: UnaryOp::Neg,
            expr: Box::new(int(1)),
        });
        assert_eq!(
            eval(&neg, Some(U8)),
            Err(ConstEvalErrorKind::Overflow { kind: U8 })
        );
    }

    #[test]
    fn strings_compare_and_concatenate() {
        let s = |v: &str| expr(ExprKind::String(v.to_string()));
        assert_eq!(
            eval(&bin(BinOp::Add, s("a"), s("b")), None),
            Ok(ConstValue::String("ab".to_string()))
        );
        assert_eq!(
            eval(&bin(BinOp::Lt, s("a"), s("b")), None),
            Ok(ConstValue::Bool(true))
        );
        assert_eq!(
            eval(&bin(BinOp::Eq, s("a"), s("b")), None),
            Ok(ConstValue::Bool(false))
        );
        assert_eq!(
            eval(&bin(BinOp::Sub, s("a"), s("b")), None),
            Err(ConstEvalErrorKind::UnsupportedBinary {
                op: BinOp::Sub,
                lhs: "String",
                rhs: "String"
            })
        );
    }

    #[test]
    fn nan_is_unequal_to_itself() {
        let nan = || expr(ExprKind::Float(f64::NAN, None));
        assert_eq!(
            eval(&bin(BinOp::Eq, nan(), nan()), None),
            Ok(ConstValue::Bool(false))
        );
        assert_eq!(
            eval(&bin(BinOp::Ne, nan(), nan()), None),
            Ok(ConstValue::Bool(true))
        );
    }

    #[test]
    fn references_use_the_lookup() {
        let var = expr(ExprKind::Var("K".to_string()));
        let mut lookup = |name: &str| match name {
            "K" => ConstLookup::Value(ConstValue::Int {
                value: 5,
                kind: Some(I8),
            }),
            _ => ConstLookup::Unknown,
        };
        let sum = bin(BinOp::Mul, var.clone(), int(30));
        assert_eq!(
            eval_expr(&sum, None, &mut lookup).map_err(|e| e.kind),
            Err(ConstEvalErrorKind::Overflow { kind: I8 })
        );
        let mut failed = |_: &str| ConstLookup::Failed;
        assert_eq!(eval_expr(&var, None, &mut failed), Ok(None));
        let mut unknown = |_: &str| ConstLookup::Unknown;
        assert_eq!(
            eval_expr(&var, None, &mut unknown).map_err(|e| e.kind),
            Err(ConstEvalErrorKind::UnknownConst("K".to_string()))
        );
    }
}
