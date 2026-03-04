use crate::span::Span;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;

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
    pub ty: TypeExpr,
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
    #[serde(default)]
    pub visibility: Visibility,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<Field>,
    pub invariant: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(default)]
    pub visibility: Visibility,
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
    pub generics: Vec<GenericParam>,
    pub variants: Vec<VariantDef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitDef {
    pub name: String,
    #[serde(default)]
    pub visibility: Visibility,
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
    pub payload: Option<TypeExpr>,
    pub span: Span,
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

type IntLiteralKey = (usize, usize, i64);

thread_local! {
    static INT_LITERAL_METADATA_STORE: RefCell<BTreeMap<IntLiteralKey, IntLiteralMetadata>> =
        RefCell::new(BTreeMap::new());
}

pub fn clear_int_literal_metadata() {
    INT_LITERAL_METADATA_STORE.with(|store| {
        let mut guard = store.borrow_mut();
        guard.clear();
    });
}

pub fn record_int_literal_metadata(span: Span, value: i64, metadata: IntLiteralMetadata) {
    INT_LITERAL_METADATA_STORE.with(|store| {
        let mut guard = store.borrow_mut();
        guard.insert((span.start, span.end, value), metadata);
    });
}

pub fn lookup_int_literal_metadata(span: Span, value: i64) -> Option<IntLiteralMetadata> {
    INT_LITERAL_METADATA_STORE
        .with(|store| store.borrow().get(&(span.start, span.end, value)).cloned())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
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
    Assign {
        target: String,
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
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    String(String),
    Unit,
    Var(String),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        arg_names: Vec<Option<String>>,
    },
    Closure {
        params: Vec<ClosureParam>,
        ret_type: TypeExpr,
        body: Block,
    },
    If {
        cond: Box<Expr>,
        then_block: Block,
        else_block: Block,
    },
    While {
        cond: Box<Expr>,
        body: Block,
    },
    Loop {
        body: Block,
    },
    Break {
        expr: Option<Box<Expr>>,
    },
    Continue,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternKind {
    Wildcard,
    Var(String),
    Int(i64),
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
        match self.kind {
            ExprKind::Int(value) => lookup_int_literal_metadata(self.span, value),
            _ => None,
        }
    }
}

impl Pattern {
    pub fn int_literal_metadata(&self) -> Option<IntLiteralMetadata> {
        match self.kind {
            PatternKind::Int(value) => lookup_int_literal_metadata(self.span, value),
            _ => None,
        }
    }
}
