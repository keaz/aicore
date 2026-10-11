use crate::ast::{Attribute, BinOp, RangeBound, StructShape, UnaryOp, Visibility};
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
        /// Explicit generic arguments: `f::[Int](x)`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<TypeId>,
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

/// Calls `f` on every expression in `program` (function and method
/// bodies, contracts, struct defaults and invariants), parents before
/// children. `f` may replace an expression; the walk then descends into the
/// replacement.
/// Whether `block` returns from its function or closure: a `return`
/// statement or expression anywhere in it, except inside nested closures
/// (their `return` leaves the nested closure).
pub fn block_contains_return(block: &Block) -> bool {
    fn in_block(block: &Block) -> bool {
        block.stmts.iter().any(|stmt| match stmt {
            Stmt::Return { .. } => true,
            Stmt::Let { expr: value, .. }
            | Stmt::Assign { expr: value, .. }
            | Stmt::Expr { expr: value, .. }
            | Stmt::Assert { expr: value, .. } => in_expr(value),
        }) || block.tail.as_deref().is_some_and(in_expr)
    }
    fn in_expr(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Return { .. } => true,
            ExprKind::Closure { .. } => false,
            ExprKind::Call { callee, args, .. } => in_expr(callee) || args.iter().any(in_expr),
            ExprKind::TemplateLiteral { args, .. } => args.iter().any(in_expr),
            ExprKind::Loop { body, .. }
            | ExprKind::UnsafeBlock { block: body }
            | ExprKind::Block { block: body } => in_block(body),
            ExprKind::If {
                cond,
                then_block,
                else_block,
            } => in_expr(cond) || in_block(then_block) || in_block(else_block),
            ExprKind::While { cond, body, .. } => in_expr(cond) || in_block(body),
            ExprKind::Break { expr: value, .. } => value.as_deref().is_some_and(in_expr),
            ExprKind::Match {
                expr: scrutinee,
                arms,
            } => {
                in_expr(scrutinee)
                    || arms
                        .iter()
                        .any(|arm| arm.guard.as_ref().is_some_and(in_expr) || in_expr(&arm.body))
            }
            ExprKind::Binary { lhs, rhs, .. } => in_expr(lhs) || in_expr(rhs),
            ExprKind::Unary { expr: inner, .. }
            | ExprKind::Borrow { expr: inner, .. }
            | ExprKind::Await { expr: inner }
            | ExprKind::Try { expr: inner }
            | ExprKind::FieldAccess { base: inner, .. } => in_expr(inner),
            ExprKind::StructInit { fields, .. } => {
                fields.iter().any(|(_, value, _)| in_expr(value))
            }
            ExprKind::Int(..)
            | ExprKind::Float(..)
            | ExprKind::Bool(_)
            | ExprKind::Char(_)
            | ExprKind::String(_)
            | ExprKind::Unit
            | ExprKind::Var(_)
            | ExprKind::Continue { .. } => false,
        }
    }
    in_block(block)
}

pub fn visit_exprs_mut(program: &mut Program, f: &mut dyn FnMut(&mut Expr)) {
    fn function(func: &mut Function, f: &mut dyn FnMut(&mut Expr)) {
        if let Some(req) = func.requires.as_mut() {
            walk_expr_mut(req, f);
        }
        if let Some(ens) = func.ensures.as_mut() {
            walk_expr_mut(ens, f);
        }
        walk_block_mut(&mut func.body, f);
    }
    for item in &mut program.items {
        match item {
            Item::Function(func) => function(func, f),
            Item::Struct(strukt) => {
                for field in &mut strukt.fields {
                    if let Some(default) = field.default_value.as_mut() {
                        walk_expr_mut(default, f);
                    }
                }
                if let Some(invariant) = strukt.invariant.as_mut() {
                    walk_expr_mut(invariant, f);
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

fn walk_block_mut(block_ref: &mut Block, f: &mut dyn FnMut(&mut Expr)) {
    for stmt in &mut block_ref.stmts {
        match stmt {
            Stmt::Let { expr: value, .. }
            | Stmt::Assign { expr: value, .. }
            | Stmt::Expr { expr: value, .. }
            | Stmt::Assert { expr: value, .. } => walk_expr_mut(value, f),
            Stmt::Return { expr: value, .. } => {
                if let Some(value) = value {
                    walk_expr_mut(value, f);
                }
            }
        }
    }
    if let Some(tail) = block_ref.tail.as_mut() {
        walk_expr_mut(tail, f);
    }
}

fn walk_expr_mut(e: &mut Expr, f: &mut dyn FnMut(&mut Expr)) {
    f(e);
    match &mut e.kind {
        ExprKind::Call { callee, args, .. } => {
            walk_expr_mut(callee, f);
            for arg in args {
                walk_expr_mut(arg, f);
            }
        }
        ExprKind::TemplateLiteral { args, .. } => {
            for arg in args {
                walk_expr_mut(arg, f);
            }
        }
        ExprKind::Closure { body, .. }
        | ExprKind::Loop { body, .. }
        | ExprKind::UnsafeBlock { block: body }
        | ExprKind::Block { block: body } => walk_block_mut(body, f),
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            walk_expr_mut(cond, f);
            walk_block_mut(then_block, f);
            walk_block_mut(else_block, f);
        }
        ExprKind::While { cond, body, .. } => {
            walk_expr_mut(cond, f);
            walk_block_mut(body, f);
        }
        ExprKind::Break { expr: value, .. } | ExprKind::Return { expr: value } => {
            if let Some(value) = value {
                walk_expr_mut(value, f);
            }
        }
        ExprKind::Match {
            expr: scrutinee,
            arms,
        } => {
            walk_expr_mut(scrutinee, f);
            for arm in arms {
                if let Some(guard) = arm.guard.as_mut() {
                    walk_expr_mut(guard, f);
                }
                walk_expr_mut(&mut arm.body, f);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr_mut(lhs, f);
            walk_expr_mut(rhs, f);
        }
        ExprKind::Unary { expr: inner, .. }
        | ExprKind::Borrow { expr: inner, .. }
        | ExprKind::Await { expr: inner }
        | ExprKind::Try { expr: inner } => walk_expr_mut(inner, f),
        ExprKind::FieldAccess { base, .. } => walk_expr_mut(base, f),
        ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                walk_expr_mut(value, f);
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

/// In a struct invariant, `self.field` names the field like the bare
/// field name does (the only way to name tuple struct fields: `self.0`).
pub fn resolve_invariant_self(invariant: &mut Expr, fields: &[String]) {
    visit_expr_mut(invariant, &mut |e| {
        if let ExprKind::FieldAccess { base, field } = &e.kind {
            if matches!(&base.kind, ExprKind::Var(name) if name == "self") && fields.contains(field)
            {
                e.kind = ExprKind::Var(field.clone());
            }
        }
    });
}

/// Calls `f` on `e` and every expression inside it, parents before
/// children (the expression-level form of `visit_exprs_mut`).
pub fn visit_expr_mut(e: &mut Expr, f: &mut dyn FnMut(&mut Expr)) {
    walk_expr_mut(e, f);
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
