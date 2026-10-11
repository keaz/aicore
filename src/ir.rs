use crate::ast::{Attribute, BinOp, UnaryOp, Visibility};
use crate::span::Span;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const CURRENT_IR_SCHEMA_VERSION: u32 = 1;
pub use crate::ast::{
    FloatLiteralKind, FloatLiteralMetadata, FloatLiteralSuffix, FloatLiteralWidth, IntLiteralKind,
    IntLiteralMetadata, IntLiteralSignedness, IntLiteralSuffix, IntLiteralWidth,
};

pub fn canonical_primitive_type_name(name: &str) -> &str {
    crate::ast::canonical_primitive_type_name(name)
}

fn default_schema_version() -> u32 {
    CURRENT_IR_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TypeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

impl NodeId {
    /// Ids at or above this value belong to nodes the type checker
    /// synthesizes (counting down from `u32::MAX`, e.g. for indexing and
    /// numeric conversion rewrites).
    pub const SYNTHETIC_FLOOR: u32 = 1 << 31;

    /// The id for computing where a later pass may allocate fresh ids:
    /// synthetic ids count as 0, so fresh ids start above the parsed ones
    /// and never overflow.
    pub fn allocation_id(self) -> u32 {
        if self.0 >= Self::SYNTHETIC_FLOOR {
            0
        } else {
            self.0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub module: Option<Vec<String>>,
    pub imports: Vec<Vec<String>>,
    pub items: Vec<Item>,
    pub symbols: Vec<Symbol>,
    pub types: Vec<TypeDef>,
    #[serde(default)]
    pub generic_instantiations: Vec<GenericInstantiation>,
    pub span: Span,
    /// Types the checker inferred for expressions whose type the code
    /// generator cannot derive locally, such as a generic constructor nested
    /// in another generic call (`vec.push(vec.new_vec(), 5)`). Filled by the
    /// driver after type checking; not part of the serialized IR.
    #[serde(skip)]
    pub expr_type_hints: BTreeMap<NodeId, String>,
}

pub fn migrate_json_to_current(input: &str) -> anyhow::Result<Program> {
    let mut value: Value = serde_json::from_str(input)?;
    migrate_value_to_current(&mut value)?;
    let program: Program = serde_json::from_value(value)?;
    if program.schema_version != CURRENT_IR_SCHEMA_VERSION {
        anyhow::bail!(
            "migrated schema_version {} does not match current {}",
            program.schema_version,
            CURRENT_IR_SCHEMA_VERSION
        );
    }
    Ok(program)
}

fn migrate_value_to_current(value: &mut Value) -> anyhow::Result<()> {
    let current = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;

    match current {
        0 => {
            let Value::Object(map) = value else {
                anyhow::bail!("IR JSON root must be an object");
            };
            map.insert(
                "schema_version".to_string(),
                Value::from(CURRENT_IR_SCHEMA_VERSION),
            );
            Ok(())
        }
        v if v == CURRENT_IR_SCHEMA_VERSION => Ok(()),
        other => anyhow::bail!(
            "unsupported IR schema_version {other}; current schema_version is {CURRENT_IR_SCHEMA_VERSION}"
        ),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Variant,
    Field,
    Parameter,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDef {
    pub id: TypeId,
    pub repr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenericInstantiationKind {
    Function,
    Struct,
    Enum,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenericInstantiation {
    pub id: u32,
    pub kind: GenericInstantiationKind,
    pub name: String,
    pub symbol: Option<SymbolId>,
    pub type_args: Vec<String>,
    pub mangled: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Item {
    Function(Function),
    Struct(StructDef),
    Enum(EnumDef),
    Trait(TraitDef),
    Impl(ImplDef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: String,
    #[serde(default)]
    pub bounds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub symbol: SymbolId,
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
    pub ret_type: TypeId,
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
    pub symbol: SymbolId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    pub ty: TypeId,
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
    pub ty: Option<TypeId>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructDef {
    pub symbol: SymbolId,
    pub name: String,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<Field>,
    pub invariant: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub symbol: SymbolId,
    pub name: String,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    pub ty: TypeId,
    #[serde(default)]
    pub default_value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDef {
    pub symbol: SymbolId,
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
    pub symbol: SymbolId,
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
    pub symbol: SymbolId,
    pub trait_name: String,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    #[serde(default)]
    pub trait_args: Vec<TypeId>,
    #[serde(default)]
    pub target: Option<TypeId>,
    #[serde(default)]
    pub methods: Vec<Function>,
    #[serde(default)]
    pub is_inherent: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantDef {
    pub symbol: SymbolId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    pub payload: Option<TypeId>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub node: NodeId,
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

impl Block {
    pub fn lexical_drop_order(&self) -> Vec<SymbolId> {
        self.stmts
            .iter()
            .filter_map(|stmt| match stmt {
                Stmt::Let { symbol, .. } => Some(*symbol),
                _ => None,
            })
            .collect()
    }
}

/// Renders an assignment place for diagnostics: `p.a.0`, `v[index].x`.
pub fn render_place(target: &str, path: &[PlaceProjection]) -> String {
    let mut out = target.to_string();
    for projection in path {
        match projection {
            PlaceProjection::Field { name, .. } => {
                out.push('.');
                out.push_str(name);
            }
            PlaceProjection::Index { .. } => out.push_str("[index]"),
        }
    }
    out
}

/// One step from a binding to the place an assignment writes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlaceProjection {
    /// A struct field, or a tuple element when the name is a decimal index.
    Field { name: String, span: Span },
    /// A `Vec` element. The AST index expression is evaluated into the
    /// hidden `Int` binding `binding` (declared by a `Let` emitted just
    /// before the assignment), so the index is evaluated once, left to
    /// right, before the right-hand side.
    Index { binding: String, span: Span },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stmt {
    Let {
        symbol: SymbolId,
        name: String,
        #[serde(default)]
        mutable: bool,
        ty: Option<TypeId>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expr {
    pub node: NodeId,
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        symbol: Option<SymbolId>,
    },
    Closure {
        params: Vec<ClosureParam>,
        ret_type: TypeId,
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
    pub node: NodeId,
    pub kind: PatternKind,
    pub span: Span,
}

impl Pattern {
    pub fn int_literal_metadata(&self) -> Option<IntLiteralMetadata> {
        match &self.kind {
            PatternKind::Int(_, meta) => meta.as_deref().cloned(),
            _ => None,
        }
    }
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
}

/// Calls `f` on every expression in `program` (function and method
/// bodies, contracts, struct defaults and invariants), parents before
/// children. `f` may replace an expression; the walk then descends into the
/// replacement.
pub fn visit_exprs_mut(program: &mut Program, f: &mut dyn FnMut(&mut Expr)) {
    fn function(func: &mut Function, f: &mut dyn FnMut(&mut Expr)) {
        if let Some(req) = func.requires.as_mut() {
            expr(req, f);
        }
        if let Some(ens) = func.ensures.as_mut() {
            expr(ens, f);
        }
        block(&mut func.body, f);
    }
    fn block(block_ref: &mut Block, f: &mut dyn FnMut(&mut Expr)) {
        for stmt in &mut block_ref.stmts {
            match stmt {
                Stmt::Let { expr: value, .. }
                | Stmt::Assign { expr: value, .. }
                | Stmt::Expr { expr: value, .. }
                | Stmt::Assert { expr: value, .. } => expr(value, f),
                Stmt::Return { expr: value, .. } => {
                    if let Some(value) = value {
                        expr(value, f);
                    }
                }
            }
        }
        if let Some(tail) = block_ref.tail.as_mut() {
            expr(tail, f);
        }
    }
    fn expr(e: &mut Expr, f: &mut dyn FnMut(&mut Expr)) {
        f(e);
        match &mut e.kind {
            ExprKind::Call { callee, args, .. } => {
                expr(callee, f);
                for arg in args {
                    expr(arg, f);
                }
            }
            ExprKind::TemplateLiteral { args, .. } => {
                for arg in args {
                    expr(arg, f);
                }
            }
            ExprKind::Closure { body, .. }
            | ExprKind::Loop { body, .. }
            | ExprKind::UnsafeBlock { block: body }
            | ExprKind::Block { block: body } => block(body, f),
            ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                expr(cond, f);
                block(then_block, f);
                block(else_block, f);
            }
            ExprKind::While { cond, body, .. } => {
                expr(cond, f);
                block(body, f);
            }
            ExprKind::Break { expr: value, .. } | ExprKind::Return { expr: value } => {
                if let Some(value) = value {
                    expr(value, f);
                }
            }
            ExprKind::Match {
                expr: scrutinee,
                arms,
            } => {
                expr(scrutinee, f);
                for arm in arms {
                    if let Some(guard) = arm.guard.as_mut() {
                        expr(guard, f);
                    }
                    expr(&mut arm.body, f);
                }
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, f);
                expr(rhs, f);
            }
            ExprKind::Unary { expr: inner, .. }
            | ExprKind::Borrow { expr: inner, .. }
            | ExprKind::Await { expr: inner }
            | ExprKind::Try { expr: inner } => expr(inner, f),
            ExprKind::FieldAccess { base, .. } => expr(base, f),
            ExprKind::StructInit { fields, .. } => {
                for (_, value, _) in fields {
                    expr(value, f);
                }
            }
            ExprKind::Int(..)
            | ExprKind::Float(..)
            | ExprKind::Bool(_)
            | ExprKind::Char(_)
            | ExprKind::String(_)
            | ExprKind::Unit
            | ExprKind::Var(_)
            | ExprKind::Continue { .. } => {}
        }
    }
    for item in &mut program.items {
        match item {
            Item::Function(func) => function(func, f),
            Item::Struct(strukt) => {
                for field in &mut strukt.fields {
                    if let Some(default) = field.default_value.as_mut() {
                        expr(default, f);
                    }
                }
                if let Some(invariant) = strukt.invariant.as_mut() {
                    expr(invariant, f);
                }
            }
            Item::Enum(_) => {}
            Item::Trait(trait_def) => {
                for method in &mut trait_def.methods {
                    function(method, f);
                }
            }
            Item::Impl(impl_def) => {
                for method in &mut impl_def.methods {
                    function(method, f);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{migrate_json_to_current, CURRENT_IR_SCHEMA_VERSION};

    #[test]
    fn migrate_legacy_v0_without_schema_version() {
        let legacy = r#"{
  "module": null,
  "imports": [],
  "items": [],
  "symbols": [],
  "types": [],
  "span": { "start": 0, "end": 0 }
}"#;
        let migrated = migrate_json_to_current(legacy).expect("migrate");
        assert_eq!(migrated.schema_version, CURRENT_IR_SCHEMA_VERSION);
    }

    #[test]
    fn reject_unknown_schema_version() {
        let unsupported = r#"{
  "schema_version": 99,
  "module": null,
  "imports": [],
  "items": [],
  "symbols": [],
  "types": [],
  "span": { "start": 0, "end": 0 }
}"#;
        let err = migrate_json_to_current(unsupported).expect_err("expected error");
        assert!(err.to_string().contains("unsupported IR schema_version"));
    }
}
