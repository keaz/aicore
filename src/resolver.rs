use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostics::Diagnostic;
use crate::ir;

#[derive(Debug, Clone)]
pub struct Resolution {
    pub functions: BTreeMap<String, FunctionInfo>,
    pub structs: BTreeMap<String, StructInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
    pub imports: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub symbol: ir::SymbolId,
    pub param_types: Vec<ir::TypeId>,
    pub ret_type: ir::TypeId,
    pub effects: BTreeSet<String>,
    pub span: crate::span::Span,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub symbol: ir::SymbolId,
    pub fields: BTreeMap<String, ir::TypeId>,
    pub invariant: Option<ir::Expr>,
    pub span: crate::span::Span,
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub symbol: ir::SymbolId,
    pub variants: BTreeMap<String, Option<ir::TypeId>>,
    pub span: crate::span::Span,
}

pub fn resolve(program: &ir::Program, file: &str) -> (Resolution, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();

    let mut functions = BTreeMap::new();
    let mut structs = BTreeMap::new();
    let mut enums = BTreeMap::new();

    let mut imports = BTreeSet::new();
    for path in &program.imports {
        imports.insert(path.join("."));
    }

    for item in &program.items {
        match item {
            ir::Item::Function(f) => {
                if functions.contains_key(&f.name)
                    || structs.contains_key(&f.name)
                    || enums.contains_key(&f.name)
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "E1100",
                            format!("duplicate symbol '{}'", f.name),
                            file,
                            f.span,
                        )
                        .with_help("rename one declaration to keep symbol names unique"),
                    );
                } else {
                    functions.insert(
                        f.name.clone(),
                        FunctionInfo {
                            symbol: f.symbol,
                            param_types: f.params.iter().map(|p| p.ty).collect(),
                            ret_type: f.ret_type,
                            effects: f.effects.iter().cloned().collect(),
                            span: f.span,
                        },
                    );
                }
            }
            ir::Item::Struct(s) => {
                if functions.contains_key(&s.name)
                    || structs.contains_key(&s.name)
                    || enums.contains_key(&s.name)
                {
                    diagnostics.push(Diagnostic::error(
                        "E1100",
                        format!("duplicate symbol '{}'", s.name),
                        file,
                        s.span,
                    ));
                } else {
                    let mut fields = BTreeMap::new();
                    for field in &s.fields {
                        if fields.insert(field.name.clone(), field.ty).is_some() {
                            diagnostics.push(Diagnostic::error(
                                "E1101",
                                format!("duplicate struct field '{}.{}'", s.name, field.name),
                                file,
                                field.span,
                            ));
                        }
                    }
                    structs.insert(
                        s.name.clone(),
                        StructInfo {
                            symbol: s.symbol,
                            fields,
                            invariant: s.invariant.clone(),
                            span: s.span,
                        },
                    );
                }
            }
            ir::Item::Enum(e) => {
                if functions.contains_key(&e.name)
                    || structs.contains_key(&e.name)
                    || enums.contains_key(&e.name)
                {
                    diagnostics.push(Diagnostic::error(
                        "E1100",
                        format!("duplicate symbol '{}'", e.name),
                        file,
                        e.span,
                    ));
                } else {
                    let mut variants = BTreeMap::new();
                    for variant in &e.variants {
                        if variants
                            .insert(variant.name.clone(), variant.payload)
                            .is_some()
                        {
                            diagnostics.push(Diagnostic::error(
                                "E1102",
                                format!("duplicate enum variant '{}.{}'", e.name, variant.name),
                                file,
                                variant.span,
                            ));
                        }
                    }
                    enums.insert(
                        e.name.clone(),
                        EnumInfo {
                            symbol: e.symbol,
                            variants,
                            span: e.span,
                        },
                    );
                }
            }
        }
    }

    (
        Resolution {
            functions,
            structs,
            enums,
            imports,
        },
        diagnostics,
    )
}

#[cfg(test)]
mod tests {
    use crate::{ir_builder::build, parser::parse};

    use super::resolve;

    #[test]
    fn resolves_top_level_symbols() {
        let src = "fn a() -> Int { 1 }\nstruct S { x: Int }\nenum E { A, B }";
        let (program, diags) = parse(src, "test.aic");
        assert!(diags.is_empty());
        let ir = build(&program.expect("program"));
        let (res, diags) = resolve(&ir, "test.aic");
        assert!(diags.is_empty());
        assert!(res.functions.contains_key("a"));
        assert!(res.structs.contains_key("S"));
        assert!(res.enums.contains_key("E"));
    }

    #[test]
    fn duplicate_symbol_is_diagnostic() {
        let src = "fn a() -> Int { 1 }\nfn a() -> Int { 2 }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let (_res, diags) = resolve(&ir, "test.aic");
        assert!(diags.iter().any(|d| d.code == "E1100"));
    }
}
