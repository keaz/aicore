use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{decode_internal_const, decode_internal_type_alias};
use crate::diagnostics::Diagnostic;
use crate::ir;

const ROOT_MODULE: &str = "<root>";

#[derive(Debug, Clone)]
pub struct Resolution {
    pub functions: BTreeMap<String, FunctionInfo>,
    pub module_function_infos: BTreeMap<(String, String), FunctionInfo>,
    pub structs: BTreeMap<String, StructInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
    pub traits: BTreeMap<String, TraitInfo>,
    pub trait_impls: BTreeMap<String, BTreeSet<String>>,
    pub imports: BTreeSet<String>,
    pub entry_module: Option<String>,
    pub function_modules: BTreeMap<String, BTreeSet<String>>,
    pub module_functions: BTreeMap<String, BTreeSet<String>>,
    pub visible_functions: BTreeSet<String>,
    pub import_aliases: BTreeMap<String, String>,
    pub ambiguous_import_aliases: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub symbol: ir::SymbolId,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub is_extern: bool,
    pub extern_abi: Option<String>,
    pub generics: Vec<String>,
    pub generic_bounds: BTreeMap<String, Vec<String>>,
    pub param_types: Vec<ir::TypeId>,
    pub ret_type: ir::TypeId,
    pub effects: BTreeSet<String>,
    pub span: crate::span::Span,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub symbol: ir::SymbolId,
    pub generics: Vec<String>,
    pub fields: BTreeMap<String, ir::TypeId>,
    pub invariant: Option<ir::Expr>,
    pub span: crate::span::Span,
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub symbol: ir::SymbolId,
    pub generics: Vec<String>,
    pub variants: BTreeMap<String, Option<ir::TypeId>>,
    pub span: crate::span::Span,
}

#[derive(Debug, Clone)]
pub struct TraitInfo {
    pub symbol: ir::SymbolId,
    pub generics: Vec<String>,
    pub span: crate::span::Span,
}

pub fn resolve(program: &ir::Program, file: &str) -> (Resolution, Vec<Diagnostic>) {
    resolve_with_item_modules(program, file, None)
}

pub fn resolve_with_item_modules(
    program: &ir::Program,
    file: &str,
    item_modules: Option<&[Option<Vec<String>>]>,
) -> (Resolution, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();

    let mut functions = BTreeMap::new();
    let mut module_function_infos = BTreeMap::new();
    let mut structs = BTreeMap::new();
    let mut enums = BTreeMap::new();
    let mut traits = BTreeMap::new();
    let mut trait_impls: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let type_repr_by_id = program
        .types
        .iter()
        .map(|ty| (ty.id, ty.repr.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut imports = BTreeSet::new();
    for path in &program.imports {
        imports.insert(path.join("."));
    }

    let entry_module = program.module.as_ref().map(|m| m.join("."));

    let mut import_aliases = BTreeMap::new();
    let mut ambiguous_import_aliases = BTreeSet::new();
    for import in &imports {
        let alias = import.rsplit('.').next().unwrap_or(import).to_string();
        if alias.is_empty() {
            continue;
        }
        if let Some(existing) = import_aliases.get(&alias) {
            if existing != import {
                ambiguous_import_aliases.insert(alias.clone());
                import_aliases.remove(&alias);
            }
        } else if !ambiguous_import_aliases.contains(&alias) {
            import_aliases.insert(alias, import.clone());
        }
    }

    let mut function_modules: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut module_functions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    // Namespace model:
    // - Value namespace: functions
    // - Type namespace: structs, enums
    // - Module namespace: import aliases (tracked separately)
    let mut value_decl_kind_by_module_name: BTreeMap<(String, String), &'static str> =
        BTreeMap::new();
    let mut type_decl_kind_by_module_name: BTreeMap<(String, String), &'static str> =
        BTreeMap::new();

    for (index, item) in program.items.iter().enumerate() {
        let module_name = module_for_item(program, item_modules, index);

        match item {
            ir::Item::Function(f) => {
                if let Some(alias_name) = decode_internal_type_alias(&f.name) {
                    if let Some(existing_kind) = type_decl_kind_by_module_name
                        .insert((module_name.clone(), alias_name.to_string()), "type")
                    {
                        diagnostics.push(
                            Diagnostic::error(
                                "E1100",
                                format!(
                                    "duplicate symbol '{}', kinds '{}' and 'type'",
                                    alias_name, existing_kind
                                ),
                                file,
                                f.span,
                            )
                            .with_help(
                                "rename one declaration to keep symbol names unique per module",
                            ),
                        );
                    }
                    continue;
                }

                if let Some(const_name) = decode_internal_const(&f.name) {
                    if let Some(existing_kind) = value_decl_kind_by_module_name
                        .insert((module_name, const_name.to_string()), "const")
                    {
                        diagnostics.push(
                            Diagnostic::error(
                                "E1100",
                                format!(
                                    "duplicate symbol '{}', kinds '{}' and 'const'",
                                    const_name, existing_kind
                                ),
                                file,
                                f.span,
                            )
                            .with_help(
                                "rename one declaration to keep symbol names unique per module",
                            ),
                        );
                    }
                    continue;
                }

                if let Some(existing_kind) = value_decl_kind_by_module_name
                    .insert((module_name.clone(), f.name.clone()), "fn")
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "E1100",
                            format!(
                                "duplicate symbol '{}', kinds '{}' and 'fn'",
                                f.name, existing_kind
                            ),
                            file,
                            f.span,
                        )
                        .with_help("rename one declaration to keep symbol names unique per module"),
                    );
                    continue;
                }

                module_functions
                    .entry(module_name.clone())
                    .or_default()
                    .insert(f.name.clone());
                function_modules
                    .entry(f.name.clone())
                    .or_default()
                    .insert(module_name.clone());

                let info = FunctionInfo {
                    symbol: f.symbol,
                    is_async: f.is_async,
                    is_unsafe: f.is_unsafe,
                    is_extern: f.is_extern,
                    extern_abi: f.extern_abi.clone(),
                    generics: f.generics.iter().map(|g| g.name.clone()).collect(),
                    generic_bounds: f
                        .generics
                        .iter()
                        .map(|g| (g.name.clone(), g.bounds.clone()))
                        .collect(),
                    param_types: f.params.iter().map(|p| p.ty).collect(),
                    ret_type: f.ret_type,
                    effects: f.effects.iter().cloned().collect(),
                    span: f.span,
                };
                module_function_infos.insert((module_name, f.name.clone()), info.clone());

                functions.entry(f.name.clone()).or_insert_with(|| info);
            }
            ir::Item::Struct(s) => {
                if let Some(existing_kind) = type_decl_kind_by_module_name
                    .insert((module_name.clone(), s.name.clone()), "struct")
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "E1100",
                            format!(
                                "duplicate symbol '{}', kinds '{}' and 'struct'",
                                s.name, existing_kind
                            ),
                            file,
                            s.span,
                        )
                        .with_help("rename one declaration to keep symbol names unique per module"),
                    );
                    continue;
                }

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

                structs.entry(s.name.clone()).or_insert_with(|| StructInfo {
                    symbol: s.symbol,
                    generics: s.generics.iter().map(|g| g.name.clone()).collect(),
                    fields,
                    invariant: s.invariant.clone(),
                    span: s.span,
                });
            }
            ir::Item::Enum(e) => {
                if let Some(existing_kind) =
                    type_decl_kind_by_module_name.insert((module_name, e.name.clone()), "enum")
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "E1100",
                            format!(
                                "duplicate symbol '{}', kinds '{}' and 'enum'",
                                e.name, existing_kind
                            ),
                            file,
                            e.span,
                        )
                        .with_help("rename one declaration to keep symbol names unique per module"),
                    );
                    continue;
                }

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

                enums.entry(e.name.clone()).or_insert_with(|| EnumInfo {
                    symbol: e.symbol,
                    generics: e.generics.iter().map(|g| g.name.clone()).collect(),
                    variants,
                    span: e.span,
                });
            }
            ir::Item::Trait(t) => {
                if let Some(existing_kind) =
                    type_decl_kind_by_module_name.insert((module_name, t.name.clone()), "trait")
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "E1100",
                            format!(
                                "duplicate symbol '{}', kinds '{}' and 'trait'",
                                t.name, existing_kind
                            ),
                            file,
                            t.span,
                        )
                        .with_help("rename one declaration to keep symbol names unique per module"),
                    );
                    continue;
                }
                traits.entry(t.name.clone()).or_insert_with(|| TraitInfo {
                    symbol: t.symbol,
                    generics: t.generics.iter().map(|g| g.name.clone()).collect(),
                    span: t.span,
                });
            }
            ir::Item::Impl(impl_def) => {
                let trait_arity = traits
                    .get(&impl_def.trait_name)
                    .map(|info| info.generics.len())
                    .or_else(|| {
                        program.items.iter().find_map(|item| match item {
                            ir::Item::Trait(t) if t.name == impl_def.trait_name => {
                                Some(t.generics.len())
                            }
                            _ => None,
                        })
                    });

                let Some(trait_arity) = trait_arity else {
                    diagnostics.push(Diagnostic::error(
                        "E1103",
                        format!("unknown trait '{}' in impl", impl_def.trait_name),
                        file,
                        impl_def.span,
                    ));
                    continue;
                };

                if impl_def.trait_args.len() != trait_arity {
                    diagnostics.push(Diagnostic::error(
                        "E1104",
                        format!(
                            "impl for trait '{}' expects {} type arguments, found {}",
                            impl_def.trait_name,
                            trait_arity,
                            impl_def.trait_args.len()
                        ),
                        file,
                        impl_def.span,
                    ));
                    continue;
                }

                let key = impl_def
                    .trait_args
                    .iter()
                    .map(|arg| {
                        type_repr_by_id
                            .get(arg)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                if !trait_impls
                    .entry(impl_def.trait_name.clone())
                    .or_default()
                    .insert(key.clone())
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "E1105",
                            format!(
                                "conflicting impl for trait '{}' with type arguments [{}]",
                                impl_def.trait_name, key
                            ),
                            file,
                            impl_def.span,
                        )
                        .with_help("remove duplicate impl or use a different concrete type"),
                    );
                }
            }
        }
    }

    let mut visible_modules = BTreeSet::new();
    match &entry_module {
        Some(module) => {
            visible_modules.insert(module.clone());
        }
        None => {
            visible_modules.insert(ROOT_MODULE.to_string());
        }
    }
    visible_modules.extend(imports.iter().cloned());

    let mut visible_functions = BTreeSet::new();
    for module in &visible_modules {
        if let Some(names) = module_functions.get(module) {
            visible_functions.extend(names.iter().cloned());
        }
    }

    for alias in &ambiguous_import_aliases {
        diagnostics.push(
            Diagnostic::error(
                "E2104",
                format!(
                    "ambiguous import alias '{}': multiple imports share this tail segment",
                    alias
                ),
                file,
                program.span,
            )
            .with_help("use fully-qualified module prefixes in call sites"),
        );
    }

    (
        Resolution {
            functions,
            module_function_infos,
            structs,
            enums,
            traits,
            trait_impls,
            imports,
            entry_module,
            function_modules,
            module_functions,
            visible_functions,
            import_aliases,
            ambiguous_import_aliases,
        },
        diagnostics,
    )
}

fn module_for_item(
    program: &ir::Program,
    item_modules: Option<&[Option<Vec<String>>]>,
    index: usize,
) -> String {
    if let Some(module) = item_modules
        .and_then(|m| m.get(index))
        .and_then(|m| m.as_ref())
        .map(|m| m.join("."))
    {
        return module;
    }

    if let Some(module) = &program.module {
        return module.join(".");
    }

    ROOT_MODULE.to_string()
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

    #[test]
    fn allows_type_and_value_name_shadowing() {
        let src = "struct Token { x: Int }\nfn Token(x: Int) -> Int { x }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let (_res, diags) = resolve(&ir, "test.aic");
        assert!(
            !diags.iter().any(|d| d.code == "E1100"),
            "diags={:#?}",
            diags
        );
    }

    #[test]
    fn duplicate_type_namespace_symbol_is_diagnostic() {
        let src = "struct Token { x: Int }\nenum Token { A }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let (_res, diags) = resolve(&ir, "test.aic");
        assert!(diags.iter().any(|d| d.code == "E1100"));
    }

    #[test]
    fn collects_trait_impl_capabilities() {
        let src = "trait Order[T]; impl Order[Int];";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let (res, diags) = resolve(&ir, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(res.traits.contains_key("Order"));
        assert!(res
            .trait_impls
            .get("Order")
            .map(|impls| impls.contains("Int"))
            .unwrap_or(false));
    }

    #[test]
    fn conflicting_trait_impl_is_diagnostic() {
        let src = "trait Order[T]; impl Order[Int]; impl Order[Int];";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let (_res, diags) = resolve(&ir, "test.aic");
        assert!(diags.iter().any(|d| d.code == "E1105"));
    }
}
