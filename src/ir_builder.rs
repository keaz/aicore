use std::collections::BTreeMap;

use crate::ast;
use crate::ir;

pub fn build(program: &ast::Program) -> ir::Program {
    let mut builder = Builder {
        next_symbol: 1,
        next_node: 1,
        next_type: 1,
        symbols: Vec::new(),
        types: Vec::new(),
        type_map: BTreeMap::new(),
    };

    let items = program
        .items
        .iter()
        .map(|item| builder.lower_item(item))
        .collect::<Vec<_>>();

    ir::Program {
        schema_version: ir::CURRENT_IR_SCHEMA_VERSION,
        module: program.module.as_ref().map(|m| m.path.clone()),
        imports: program.imports.iter().map(|i| i.path.clone()).collect(),
        items,
        symbols: builder.symbols,
        types: builder.types,
        generic_instantiations: Vec::new(),
        span: program.span,
    }
}

struct Builder {
    next_symbol: u32,
    next_node: u32,
    next_type: u32,
    symbols: Vec<ir::Symbol>,
    types: Vec<ir::TypeDef>,
    type_map: BTreeMap<String, ir::TypeId>,
}

impl Builder {
    fn lower_item(&mut self, item: &ast::Item) -> ir::Item {
        match item {
            ast::Item::Function(func) => ir::Item::Function(self.lower_function(func)),
            ast::Item::Struct(def) => ir::Item::Struct(self.lower_struct(def)),
            ast::Item::Enum(def) => ir::Item::Enum(self.lower_enum(def)),
            ast::Item::Trait(def) => ir::Item::Trait(self.lower_trait(def)),
            ast::Item::Impl(def) => ir::Item::Impl(self.lower_impl(def)),
        }
    }

    fn lower_function(&mut self, func: &ast::Function) -> ir::Function {
        let symbol = self.push_symbol(&func.name, ir::SymbolKind::Function, func.span);
        let params = func
            .params
            .iter()
            .map(|param| {
                let sym = self.push_symbol(&param.name, ir::SymbolKind::Parameter, param.span);
                ir::Param {
                    symbol: sym,
                    name: param.name.clone(),
                    ty: self.lower_type(&param.ty),
                    span: param.span,
                }
            })
            .collect();
        ir::Function {
            symbol,
            name: func.name.clone(),
            is_async: func.is_async,
            generics: func
                .generics
                .iter()
                .map(|g| ir::GenericParam {
                    name: g.name.clone(),
                    bounds: g.bounds.clone(),
                })
                .collect(),
            params,
            ret_type: self.lower_type(&func.ret_type),
            effects: func.effects.clone(),
            requires: func.requires.as_ref().map(|e| self.lower_expr(e)),
            ensures: func.ensures.as_ref().map(|e| self.lower_expr(e)),
            body: self.lower_block(&func.body),
            span: func.span,
        }
    }

    fn lower_struct(&mut self, def: &ast::StructDef) -> ir::StructDef {
        let symbol = self.push_symbol(&def.name, ir::SymbolKind::Struct, def.span);
        let fields = def
            .fields
            .iter()
            .map(|field| {
                let sym = self.push_symbol(&field.name, ir::SymbolKind::Field, field.span);
                ir::Field {
                    symbol: sym,
                    name: field.name.clone(),
                    ty: self.lower_type(&field.ty),
                    span: field.span,
                }
            })
            .collect();
        ir::StructDef {
            symbol,
            name: def.name.clone(),
            generics: def
                .generics
                .iter()
                .map(|g| ir::GenericParam {
                    name: g.name.clone(),
                    bounds: g.bounds.clone(),
                })
                .collect(),
            fields,
            invariant: def.invariant.as_ref().map(|e| self.lower_expr(e)),
            span: def.span,
        }
    }

    fn lower_enum(&mut self, def: &ast::EnumDef) -> ir::EnumDef {
        let symbol = self.push_symbol(&def.name, ir::SymbolKind::Enum, def.span);
        let variants = def
            .variants
            .iter()
            .map(|variant| {
                let sym = self.push_symbol(&variant.name, ir::SymbolKind::Variant, variant.span);
                ir::VariantDef {
                    symbol: sym,
                    name: variant.name.clone(),
                    payload: variant.payload.as_ref().map(|t| self.lower_type(t)),
                    span: variant.span,
                }
            })
            .collect();
        ir::EnumDef {
            symbol,
            name: def.name.clone(),
            generics: def
                .generics
                .iter()
                .map(|g| ir::GenericParam {
                    name: g.name.clone(),
                    bounds: g.bounds.clone(),
                })
                .collect(),
            variants,
            span: def.span,
        }
    }

    fn lower_trait(&mut self, def: &ast::TraitDef) -> ir::TraitDef {
        let symbol = self.push_symbol(&def.name, ir::SymbolKind::Trait, def.span);
        ir::TraitDef {
            symbol,
            name: def.name.clone(),
            generics: def
                .generics
                .iter()
                .map(|g| ir::GenericParam {
                    name: g.name.clone(),
                    bounds: g.bounds.clone(),
                })
                .collect(),
            span: def.span,
        }
    }

    fn lower_impl(&mut self, def: &ast::ImplDef) -> ir::ImplDef {
        let symbol = self.push_symbol(&def.trait_name, ir::SymbolKind::Impl, def.span);
        ir::ImplDef {
            symbol,
            trait_name: def.trait_name.clone(),
            trait_args: def
                .trait_args
                .iter()
                .map(|arg| self.lower_type(arg))
                .collect(),
            span: def.span,
        }
    }

    fn lower_type(&mut self, ty: &ast::TypeExpr) -> ir::TypeId {
        let repr = type_repr(ty);
        if let Some(id) = self.type_map.get(&repr) {
            return *id;
        }
        let id = ir::TypeId(self.next_type);
        self.next_type += 1;
        self.type_map.insert(repr.clone(), id);
        self.types.push(ir::TypeDef { id, repr });
        id
    }

    fn lower_block(&mut self, block: &ast::Block) -> ir::Block {
        ir::Block {
            node: self.next_node_id(),
            stmts: block
                .stmts
                .iter()
                .map(|stmt| self.lower_stmt(stmt))
                .collect(),
            tail: block.tail.as_ref().map(|e| Box::new(self.lower_expr(e))),
            span: block.span,
        }
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt) -> ir::Stmt {
        match stmt {
            ast::Stmt::Let {
                name,
                ty,
                expr,
                span,
            } => {
                let symbol = self.push_symbol(name, ir::SymbolKind::Local, *span);
                ir::Stmt::Let {
                    symbol,
                    name: name.clone(),
                    ty: ty.as_ref().map(|t| self.lower_type(t)),
                    expr: self.lower_expr(expr),
                    span: *span,
                }
            }
            ast::Stmt::Expr { expr, span } => ir::Stmt::Expr {
                expr: self.lower_expr(expr),
                span: *span,
            },
            ast::Stmt::Return { expr, span } => ir::Stmt::Return {
                expr: expr.as_ref().map(|e| self.lower_expr(e)),
                span: *span,
            },
            ast::Stmt::Assert {
                expr,
                message,
                span,
            } => ir::Stmt::Assert {
                expr: self.lower_expr(expr),
                message: message.clone(),
                span: *span,
            },
        }
    }

    fn lower_expr(&mut self, expr: &ast::Expr) -> ir::Expr {
        let kind = match &expr.kind {
            ast::ExprKind::Int(v) => ir::ExprKind::Int(*v),
            ast::ExprKind::Bool(v) => ir::ExprKind::Bool(*v),
            ast::ExprKind::String(v) => ir::ExprKind::String(v.clone()),
            ast::ExprKind::Unit => ir::ExprKind::Unit,
            ast::ExprKind::Var(v) => ir::ExprKind::Var(v.clone()),
            ast::ExprKind::Call { callee, args } => ir::ExprKind::Call {
                callee: Box::new(self.lower_expr(callee)),
                args: args.iter().map(|a| self.lower_expr(a)).collect(),
            },
            ast::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => ir::ExprKind::If {
                cond: Box::new(self.lower_expr(cond)),
                then_block: self.lower_block(then_block),
                else_block: self.lower_block(else_block),
            },
            ast::ExprKind::Match { expr, arms } => ir::ExprKind::Match {
                expr: Box::new(self.lower_expr(expr)),
                arms: arms
                    .iter()
                    .map(|arm| ir::MatchArm {
                        pattern: self.lower_pattern(&arm.pattern),
                        body: self.lower_expr(&arm.body),
                        span: arm.span,
                    })
                    .collect(),
            },
            ast::ExprKind::Binary { op, lhs, rhs } => ir::ExprKind::Binary {
                op: *op,
                lhs: Box::new(self.lower_expr(lhs)),
                rhs: Box::new(self.lower_expr(rhs)),
            },
            ast::ExprKind::Unary { op, expr } => ir::ExprKind::Unary {
                op: *op,
                expr: Box::new(self.lower_expr(expr)),
            },
            ast::ExprKind::Await { expr } => ir::ExprKind::Await {
                expr: Box::new(self.lower_expr(expr)),
            },
            ast::ExprKind::Try { expr } => ir::ExprKind::Try {
                expr: Box::new(self.lower_expr(expr)),
            },
            ast::ExprKind::StructInit { name, fields } => ir::ExprKind::StructInit {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(name, expr, span)| (name.clone(), self.lower_expr(expr), *span))
                    .collect(),
            },
            ast::ExprKind::FieldAccess { base, field } => ir::ExprKind::FieldAccess {
                base: Box::new(self.lower_expr(base)),
                field: field.clone(),
            },
        };

        ir::Expr {
            node: self.next_node_id(),
            kind,
            span: expr.span,
        }
    }

    fn lower_pattern(&mut self, pattern: &ast::Pattern) -> ir::Pattern {
        let kind = match &pattern.kind {
            ast::PatternKind::Wildcard => ir::PatternKind::Wildcard,
            ast::PatternKind::Var(v) => ir::PatternKind::Var(v.clone()),
            ast::PatternKind::Int(v) => ir::PatternKind::Int(*v),
            ast::PatternKind::Bool(v) => ir::PatternKind::Bool(*v),
            ast::PatternKind::Unit => ir::PatternKind::Unit,
            ast::PatternKind::Variant { name, args } => ir::PatternKind::Variant {
                name: name.clone(),
                args: args.iter().map(|a| self.lower_pattern(a)).collect(),
            },
        };
        ir::Pattern {
            node: self.next_node_id(),
            kind,
            span: pattern.span,
        }
    }

    fn push_symbol(
        &mut self,
        name: &str,
        kind: ir::SymbolKind,
        span: crate::span::Span,
    ) -> ir::SymbolId {
        let id = ir::SymbolId(self.next_symbol);
        self.next_symbol += 1;
        self.symbols.push(ir::Symbol {
            id,
            name: name.to_string(),
            kind,
            span,
        });
        id
    }

    fn next_node_id(&mut self) -> ir::NodeId {
        let id = ir::NodeId(self.next_node);
        self.next_node += 1;
        id
    }
}

fn type_repr(ty: &ast::TypeExpr) -> String {
    match &ty.kind {
        ast::TypeKind::Unit => "()".to_string(),
        ast::TypeKind::Named { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                let args = args.iter().map(type_repr).collect::<Vec<_>>().join(", ");
                format!("{name}[{args}]")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse;

    use super::build;

    #[test]
    fn builds_ir_with_stable_ids() {
        let src = "fn main() -> Int { let x = 1; x }";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty());
        let ir = build(&program.expect("program"));
        assert!(!ir.symbols.is_empty());
        assert_eq!(ir.symbols[0].id.0, 1);
    }

    #[test]
    fn interns_types() {
        let src = "fn f(x: Int) -> Int { x }\nfn g(y: Int) -> Int { y }";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty());
        let ir = build(&program.expect("program"));
        let int_count = ir.types.iter().filter(|t| t.repr == "Int").count();
        assert_eq!(int_count, 1);
    }

    #[test]
    fn preserves_generic_metadata_in_ir() {
        let src = r#"
fn id[T](x: T) -> T { x }
struct Box[T] { value: T }
enum Pair[T, U] { Mk(Result[T, U]) }
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty());
        let ir = build(&program.expect("program"));

        let mut saw_fn = false;
        let mut saw_struct = false;
        let mut saw_enum = false;
        for item in &ir.items {
            match item {
                crate::ir::Item::Function(f) if f.name == "id" => {
                    saw_fn = true;
                    assert_eq!(f.generics.len(), 1);
                    assert_eq!(f.generics[0].name, "T");
                }
                crate::ir::Item::Struct(s) if s.name == "Box" => {
                    saw_struct = true;
                    assert_eq!(s.generics.len(), 1);
                    assert_eq!(s.generics[0].name, "T");
                }
                crate::ir::Item::Enum(e) if e.name == "Pair" => {
                    saw_enum = true;
                    assert_eq!(e.generics.len(), 2);
                    assert_eq!(e.generics[0].name, "T");
                    assert_eq!(e.generics[1].name, "U");
                }
                _ => {}
            }
        }
        assert!(saw_fn && saw_struct && saw_enum);
    }
}
