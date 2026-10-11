use crate::span::Span;
use serde::{Deserialize, Serialize};

pub const INTERNAL_TYPE_ALIAS_PREFIX: &str = "__aic_type_alias__";
pub const INTERNAL_CONST_PREFIX: &str = "__aic_const__";

pub fn encode_internal_type_alias(name: &str) -> String {
    format!("{INTERNAL_TYPE_ALIAS_PREFIX}{name}")
}

pub fn decode_internal_type_alias(name: &str) -> Option<&str> {
    name.strip_prefix(INTERNAL_TYPE_ALIAS_PREFIX)
}

pub fn encode_internal_const(name: &str) -> String {
    format!("{INTERNAL_CONST_PREFIX}{name}")
}

pub fn decode_internal_const(name: &str) -> Option<&str> {
    name.strip_prefix(INTERNAL_CONST_PREFIX)
}

pub fn canonical_primitive_type_name(name: &str) -> &str {
    match name {
        "UInt" => "USize",
        "Float" => "Float64",
        _ => name,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Visibility {
    #[default]
    Private,
    Public,
    Crate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub module: Option<ModuleDecl>,
    pub imports: Vec<ImportDecl>,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDecl {
    pub path: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportDecl {
    pub path: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Item {
    Function(Function),
    Struct(StructDef),
    Enum(EnumDef),
    Trait(TraitDef),
    Impl(ImplDef),
}

impl Item {
    pub fn name(&self) -> &str {
        match self {
            Item::Function(f) => &f.name,
            Item::Struct(s) => &s.name,
            Item::Enum(e) => &e.name,
            Item::Trait(t) => &t.name,
            Item::Impl(i) => &i.trait_name,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Item::Function(f) => f.span,
            Item::Struct(s) => s.span,
            Item::Enum(e) => e.span,
            Item::Trait(t) => t.span,
            Item::Impl(i) => i.span,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: String,
    #[serde(default)]
    pub bounds: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    #[serde(default)]
    pub is_async: bool,
    #[serde(default)]
    pub is_unsafe: bool,
    #[serde(default)]
    pub is_extern: bool,
    #[serde(default)]
    pub extern_abi: Option<String>,
    #[serde(default)]
    pub is_intrinsic: bool,
    #[serde(default)]
    pub intrinsic_abi: Option<String>,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub ret_type: TypeExpr,
    pub effects: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    pub requires: Option<Expr>,
    pub ensures: Option<Expr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    pub ty: TypeExpr,
    /// `true` for a `&mut self` receiver: the method updates the caller's
    /// value in place.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mutable_receiver: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosureParam {
    pub name: String,
    #[serde(default)]
    pub ty: Option<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructDef {
    pub name: String,
    /// `struct S { .. }`, `struct S(Int, Bool);` or `struct S;`.
    #[serde(default, skip_serializing_if = "StructShape::is_named")]
    pub shape: StructShape,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<Field>,
    pub invariant: Option<Expr>,
    pub span: Span,
}

/// How a struct is declared. Tuple struct fields are named `0`, `1`, ...;
/// a unit struct has none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructShape {
    #[default]
    Named,
    Tuple,
    Unit,
}

impl StructShape {
    pub fn is_named(&self) -> bool {
        *self == StructShape::Named
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    pub ty: TypeExpr,
    #[serde(default)]
    pub default_value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDef {
    pub name: String,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<VariantDef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitDef {
    pub name: String,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    pub generics: Vec<GenericParam>,
    #[serde(default)]
    pub methods: Vec<Function>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplDef {
    pub trait_name: String,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    #[serde(default)]
    pub trait_args: Vec<TypeExpr>,
    #[serde(default)]
    pub target: Option<TypeExpr>,
    #[serde(default)]
    pub methods: Vec<Function>,
    #[serde(default)]
    pub is_inherent: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    pub payload: Option<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<AttributeArg>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttributeArg {
    Positional(AttributeValue),
    Named {
        name: String,
        value: AttributeValue,
        span: Span,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeValue {
    pub kind: AttributeValueKind,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttributeValueKind {
    String(String),
    Int(i64),
    Bool(BoolLiteral),
    Ident(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoolLiteral {
    True,
    False,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeExpr {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeKind {
    Unit,
    Named { name: String, args: Vec<TypeExpr> },
    DynTrait { trait_name: String },
    Hole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntLiteralSignedness {
    Signed,
    Unsigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntLiteralWidth {
    W8,
    W16,
    W32,
    W64,
    W128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntLiteralSuffix {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl IntLiteralSuffix {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
        }
    }

    pub fn kind(self) -> IntLiteralKind {
        match self {
            Self::I8 => IntLiteralKind {
                signedness: IntLiteralSignedness::Signed,
                width: IntLiteralWidth::W8,
            },
            Self::I16 => IntLiteralKind {
                signedness: IntLiteralSignedness::Signed,
                width: IntLiteralWidth::W16,
            },
            Self::I32 => IntLiteralKind {
                signedness: IntLiteralSignedness::Signed,
                width: IntLiteralWidth::W32,
            },
            Self::I64 => IntLiteralKind {
                signedness: IntLiteralSignedness::Signed,
                width: IntLiteralWidth::W64,
            },
            Self::U8 => IntLiteralKind {
                signedness: IntLiteralSignedness::Unsigned,
                width: IntLiteralWidth::W8,
            },
            Self::U16 => IntLiteralKind {
                signedness: IntLiteralSignedness::Unsigned,
                width: IntLiteralWidth::W16,
            },
            Self::U32 => IntLiteralKind {
                signedness: IntLiteralSignedness::Unsigned,
                width: IntLiteralWidth::W32,
            },
            Self::U64 => IntLiteralKind {
                signedness: IntLiteralSignedness::Unsigned,
                width: IntLiteralWidth::W64,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntLiteralKind {
    pub signedness: IntLiteralSignedness,
    pub width: IntLiteralWidth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntLiteralMetadata {
    pub suffix: IntLiteralSuffix,
    pub suffix_text: String,
    pub kind: IntLiteralKind,
    pub raw_value_span: Span,
    pub raw_literal_text: String,
}

impl IntLiteralMetadata {
    pub fn new(
        suffix: IntLiteralSuffix,
        raw_value_span: Span,
        raw_literal_text: impl Into<String>,
    ) -> Self {
        Self {
            kind: suffix.kind(),
            suffix,
            suffix_text: suffix.as_str().to_string(),
            raw_value_span,
            raw_literal_text: raw_literal_text.into(),
        }
    }

    pub fn with_kind_and_suffix_text(
        suffix: IntLiteralSuffix,
        kind: IntLiteralKind,
        suffix_text: impl Into<String>,
        raw_value_span: Span,
        raw_literal_text: impl Into<String>,
    ) -> Self {
        Self {
            suffix,
            suffix_text: suffix_text.into(),
            kind,
            raw_value_span,
            raw_literal_text: raw_literal_text.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloatLiteralWidth {
    W32,
    W64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloatLiteralSuffix {
    F32,
    F64,
}

impl FloatLiteralSuffix {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    pub fn kind(self) -> FloatLiteralKind {
        match self {
            Self::F32 => FloatLiteralKind {
                width: FloatLiteralWidth::W32,
            },
            Self::F64 => FloatLiteralKind {
                width: FloatLiteralWidth::W64,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloatLiteralKind {
    pub width: FloatLiteralWidth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloatLiteralMetadata {
    pub suffix: FloatLiteralSuffix,
    pub suffix_text: String,
    pub kind: FloatLiteralKind,
    pub raw_value_span: Span,
    pub raw_literal_text: String,
}

impl FloatLiteralMetadata {
    pub fn new(
        suffix: FloatLiteralSuffix,
        raw_value_span: Span,
        raw_literal_text: impl Into<String>,
    ) -> Self {
        Self {
            kind: suffix.kind(),
            suffix,
            suffix_text: suffix.as_str().to_string(),
            raw_value_span,
            raw_literal_text: raw_literal_text.into(),
        }
    }

    pub fn with_kind_and_suffix_text(
        suffix: FloatLiteralSuffix,
        kind: FloatLiteralKind,
        suffix_text: impl Into<String>,
        raw_value_span: Span,
        raw_literal_text: impl Into<String>,
    ) -> Self {
        Self {
            suffix,
            suffix_text: suffix_text.into(),
            kind,
            raw_value_span,
            raw_literal_text: raw_literal_text.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

/// Prefix of the builtin callee of a cast: `x as UInt8` parses as
/// `$as:UInt8(x)`.
pub const CAST_BUILTIN_PREFIX: &str = "$as:";
/// Prefix of the builtin callee of a numeric conversion method
/// (`x.checked_to_u8()` is checked as `$convert:checked:UInt8(x)`).
pub const CONVERT_BUILTIN_PREFIX: &str = "$convert:";

/// Builtin callees of range expressions: `a..b` is `$range(a, b)`, `a..=b`
/// is `$range_inclusive(a, b)`, `a..` is `$range_from(a)`, `..b` is
/// `$range_to(b)`, `..=b` is `$range_to_inclusive(b)` and `..` is
/// `$range_full()`. The open forms are slice indexes only.
pub const RANGE_BUILTIN: &str = "$range";
pub const RANGE_INCLUSIVE_BUILTIN: &str = "$range_inclusive";
pub const RANGE_FROM_BUILTIN: &str = "$range_from";
pub const RANGE_TO_BUILTIN: &str = "$range_to";
pub const RANGE_TO_INCLUSIVE_BUILTIN: &str = "$range_to_inclusive";
pub const RANGE_FULL_BUILTIN: &str = "$range_full";

/// Builtin callee of an index expression: `base[index]` parses as
/// `$index(base, index)`.
pub const INDEX_BUILTIN: &str = "$index";
/// Builtin callee of a list literal: `[a, b]` parses as `$vec_literal(a, b)`.
pub const VEC_LITERAL_BUILTIN: &str = "$vec_literal";
/// Builtin the checker uses on the failing arm of a rewritten index
/// expression; it panics with `index_out_of_bounds`.
pub const INDEX_PANIC_BUILTIN: &str = "$index_panic";

/// Name of the hidden payload struct of the struct-like variant
/// `Enum::Variant { .. }`.
pub fn struct_variant_payload_name(enum_name: &str, variant_name: &str) -> String {
    format!("{enum_name}__{variant_name}")
}

/// Hidden catch-all binding that marks the `else` arm of a desugared
/// `if let P = e { .. } else { .. }` (`match e { P => .., _$if_let_else => .. }`).
/// Names starting with `_$` cannot be written in source.
pub const IF_LET_ELSE_BINDING: &str = "_$if_let_else";
/// Hidden catch-all binding of the `break` arm in a desugared
/// `while let P = e { .. }` (`loop { match e { P => .., _$while_let_end => break } }`).
pub const WHILE_LET_END_BINDING: &str = "_$while_let_end";
/// Hidden catch-all binding of the diverging arm in a desugared
/// `let P = e else { .. };`.
pub const LET_ELSE_BINDING: &str = "_$let_else";
/// Prefix of the temporary that holds the bindings of a `let ... else`
/// with more than one binding.
pub const LET_ELSE_TMP_PREFIX: &str = "_$let_else_value_";

/// The index expressions of an assignment place, in source order.
pub fn place_index_exprs(path: &[PlaceProjection]) -> impl Iterator<Item = &Expr> {
    path.iter().filter_map(|projection| match projection {
        PlaceProjection::Index { index, .. } => Some(index),
        PlaceProjection::Field { .. } => None,
    })
}

/// One step from a binding to the place an assignment writes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlaceProjection {
    /// A struct field, or a tuple element when the name is a decimal index.
    Field { name: String, span: Span },
    /// A `Vec` element.
    Index { index: Expr, span: Span },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stmt {
    Let {
        name: String,
        #[serde(default)]
        mutable: bool,
        ty: Option<TypeExpr>,
        expr: Expr,
        span: Span,
    },
    /// `target = expr`, or a place assignment such as `target.a.0 = expr`
    /// or `target[i].b = expr` when `path` is not empty, or a compound
    /// assignment (`target.n += expr`) when `op` is set. `target` is the
    /// root binding.
    Assign {
        target: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        path: Vec<PlaceProjection>,
        /// The operator of a compound assignment (`place += expr`); `expr`
        /// is then the right-hand operand.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        op: Option<BinOp>,
        expr: Expr,
        span: Span,
    },
    Expr {
        expr: Expr,
        span: Span,
    },
    Return {
        expr: Option<Expr>,
        span: Span,
    },
    Assert {
        expr: Expr,
        message: String,
        span: Span,
    },
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. }
            | Stmt::Assign { span, .. }
            | Stmt::Expr { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Assert { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExprKind {
    /// Integer literal; the metadata is present when the literal has a
    /// type suffix such as `65u8`.
    Int(i64, Option<Box<IntLiteralMetadata>>),
    /// Float literal; the metadata is present for `f32`/`f64` suffixes.
    Float(f64, Option<Box<FloatLiteralMetadata>>),
    Bool(bool),
    Char(char),
    String(String),
    TemplateLiteral {
        template: String,
        args: Vec<Expr>,
    },
    Unit,
    Var(String),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        arg_names: Vec<Option<String>>,
        /// Explicit generic arguments: `f::[Int](x)`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<TypeExpr>,
    },
    /// `|x: Int| -> Int { .. }`, `|x| { .. }` or `|x| x + 1`; without
    /// `-> T` the checker infers the return type from the body.
    Closure {
        params: Vec<ClosureParam>,
        ret_type: Option<TypeExpr>,
        body: Block,
    },
    If {
        cond: Box<Expr>,
        then_block: Block,
        else_block: Block,
    },
    /// `'label: while cond { .. }`; `label` is the name without the quote.
    While {
        cond: Box<Expr>,
        body: Block,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    Loop {
        body: Block,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// `break 'label value`: without a label it leaves the innermost loop.
    Break {
        expr: Option<Box<Expr>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    Continue {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// `return` in expression position (match arms, branch tails). It never
    /// produces a value: control leaves the enclosing function.
    Return {
        expr: Option<Box<Expr>>,
    },
    Match {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Borrow {
        #[serde(default)]
        mutable: bool,
        expr: Box<Expr>,
    },
    Await {
        expr: Box<Expr>,
    },
    Try {
        expr: Box<Expr>,
    },
    UnsafeBlock {
        block: Block,
    },
    /// `{ stmts; tail }` used as an expression: a new scope whose value is
    /// the tail (or `()` without one).
    Block {
        block: Block,
    },
    StructInit {
        name: String,
        fields: Vec<(String, Expr, Span)>,
    },
    FieldAccess {
        base: Box<Expr>,
        field: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Ushr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    #[serde(default)]
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructPatternField {
    pub name: String,
    pub pattern: Pattern,
}

/// A bound of a range pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RangeBound {
    Int(i64),
    Char(char),
}

impl RangeBound {
    /// Source form: `-3`, `'a'`.
    pub fn render(&self) -> String {
        match self {
            RangeBound::Int(value) => value.to_string(),
            RangeBound::Char(value) => format!("{value:?}"),
        }
    }
}

/// Source form of a range pattern: `1..=9`, `0..10`, `5..`, `..=-1`.
pub fn render_range_pattern(
    start: Option<&RangeBound>,
    end: Option<&RangeBound>,
    inclusive: bool,
) -> String {
    format!(
        "{}{}{}",
        start.map(RangeBound::render).unwrap_or_default(),
        if inclusive { "..=" } else { ".." },
        end.map(RangeBound::render).unwrap_or_default()
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternKind {
    Wildcard,
    Var(String),
    Int(i64, Option<Box<IntLiteralMetadata>>),
    Bool(bool),
    Char(char),
    String(String),
    Unit,
    Or {
        patterns: Vec<Pattern>,
    },
    Variant {
        name: String,
        args: Vec<Pattern>,
    },
    Struct {
        name: String,
        fields: Vec<StructPatternField>,
        has_rest: bool,
    },
    /// A float literal pattern (`1.5`, `-0.25`); it matches by `==`.
    Float(f64),
    /// `lo..=hi`, `lo..hi`, `lo..` or `..=hi` over integers or chars.
    Range {
        start: Option<RangeBound>,
        end: Option<RangeBound>,
        inclusive: bool,
    },
    /// `name @ pattern`: binds the whole value and matches `pattern`.
    Binding {
        name: String,
        pattern: Box<Pattern>,
    },
}

impl Expr {
    pub fn var(name: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ExprKind::Var(name.into()),
            span,
        }
    }

    pub fn bool(value: bool, span: Span) -> Self {
        Self {
            kind: ExprKind::Bool(value),
            span,
        }
    }

    pub fn int_literal_metadata(&self) -> Option<IntLiteralMetadata> {
        match &self.kind {
            ExprKind::Int(_, meta) => meta.as_deref().cloned(),
            _ => None,
        }
    }

    pub fn float_literal_metadata(&self) -> Option<FloatLiteralMetadata> {
        match &self.kind {
            ExprKind::Float(_, meta) => meta.as_deref().cloned(),
            _ => None,
        }
    }
}

impl Pattern {
    pub fn int_literal_metadata(&self) -> Option<IntLiteralMetadata> {
        match &self.kind {
            PatternKind::Int(_, meta) => meta.as_deref().cloned(),
            _ => None,
        }
    }
}
