use crate::ast::{BinOp, UnaryOp, Visibility};
use crate::span::Span;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CURRENT_IR_SCHEMA_VERSION: u32 = 1;

fn default_schema_version() -> u32 {
    CURRENT_IR_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TypeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

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
    pub requires: Option<Expr>,
    pub ensures: Option<Expr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub symbol: SymbolId,
    pub name: String,
    pub ty: TypeId,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expr {
    pub node: NodeId,
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
        ret_type: TypeId,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternKind {
    Wildcard,
    Var(String),
    Int(i64),
    Bool(bool),
    Unit,
    Or { patterns: Vec<Pattern> },
    Variant { name: String, args: Vec<Pattern> },
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
