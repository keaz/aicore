use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{decode_internal_const, decode_internal_type_alias, Visibility};
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
    pub module_imports: BTreeMap<String, BTreeSet<String>>,
    pub entry_module: Option<String>,
    pub function_modules: BTreeMap<String, BTreeSet<String>>,
    pub module_functions: BTreeMap<String, BTreeSet<String>>,
    pub module_exported_functions: BTreeMap<String, BTreeSet<String>>,
    pub visible_functions: BTreeSet<String>,
    pub import_aliases: BTreeMap<String, String>,
    pub ambiguous_import_aliases: BTreeSet<String>,
    pub module_import_aliases: BTreeMap<String, BTreeMap<String, String>>,
    pub module_ambiguous_import_aliases: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub symbol: ir::SymbolId,
    pub visibility: Visibility,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub is_extern: bool,
    pub extern_abi: Option<String>,
    pub generics: Vec<String>,
    pub generic_bounds: BTreeMap<String, Vec<String>>,
    pub param_names: Vec<String>,
    pub param_types: Vec<ir::TypeId>,
    pub ret_type: ir::TypeId,
    pub effects: BTreeSet<String>,
    pub capabilities: BTreeSet<String>,
    pub span: crate::span::Span,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub symbol: ir::SymbolId,
    pub module: String,
    pub visibility: Visibility,
    pub generics: Vec<String>,
    pub fields: BTreeMap<String, ir::TypeId>,
    pub field_visibility: BTreeMap<String, Visibility>,
    pub default_fields: BTreeSet<String>,
    pub invariant: Option<ir::Expr>,
    pub span: crate::span::Span,
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub symbol: ir::SymbolId,
    pub module: String,
    pub visibility: Visibility,
    pub generics: Vec<String>,
    pub variants: BTreeMap<String, Option<ir::TypeId>>,
    pub span: crate::span::Span,
}

#[derive(Debug, Clone)]
pub struct TraitInfo {
    pub symbol: ir::SymbolId,
    pub module: String,
    pub visibility: Visibility,
    pub generics: Vec<String>,
    pub methods: BTreeMap<String, FunctionInfo>,
    pub span: crate::span::Span,
}

fn function_info_for(f: &ir::Function) -> FunctionInfo {
    FunctionInfo {
        symbol: f.symbol,
        visibility: f.visibility,
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
        param_names: f.params.iter().map(|p| p.name.clone()).collect(),
        param_types: f.params.iter().map(|p| p.ty).collect(),
        ret_type: f.ret_type,
        effects: f.effects.iter().cloned().collect(),
        capabilities: f.capabilities.iter().cloned().collect(),
        span: f.span,
    }
}

fn effective_visibility(module_name: &str, declared: Visibility, symbol_name: &str) -> Visibility {
    if declared != Visibility::Private {
        return declared;
    }
    if module_name.starts_with("std.") && !symbol_name.starts_with("aic_") {
        return Visibility::Public;
    }
    Visibility::Private
}

fn register_function_item(
    module_name: &str,
    f: &ir::Function,
    file: &str,
    value_decl_kind_by_module_name: &mut BTreeMap<(String, String), &'static str>,
    type_decl_kind_by_module_name: &mut BTreeMap<(String, String), &'static str>,
    diagnostics: &mut Vec<Diagnostic>,
    module_functions: &mut BTreeMap<String, BTreeSet<String>>,
    module_exported_functions: &mut BTreeMap<String, BTreeSet<String>>,
    function_modules: &mut BTreeMap<String, BTreeSet<String>>,
    module_function_infos: &mut BTreeMap<(String, String), FunctionInfo>,
    functions: &mut BTreeMap<String, FunctionInfo>,
) {
    if let Some(alias_name) = decode_internal_type_alias(&f.name) {
        if let Some(existing_kind) = type_decl_kind_by_module_name
            .insert((module_name.to_string(), alias_name.to_string()), "type")
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
                .with_help("rename one declaration to keep symbol names unique per module"),
            );
        }
        return;
    }

    if let Some(const_name) = decode_internal_const(&f.name) {
        if let Some(existing_kind) = value_decl_kind_by_module_name
            .insert((module_name.to_string(), const_name.to_string()), "const")
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
                .with_help("rename one declaration to keep symbol names unique per module"),
            );
        }
        return;
    }

    if let Some(existing_kind) =
        value_decl_kind_by_module_name.insert((module_name.to_string(), f.name.clone()), "fn")
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
        return;
    }

    module_functions
        .entry(module_name.to_string())
        .or_default()
        .insert(f.name.clone());
    function_modules
        .entry(f.name.clone())
        .or_default()
        .insert(module_name.to_string());

    let mut info = function_info_for(f);
    info.visibility = effective_visibility(module_name, info.visibility, &f.name);
    if info.visibility != Visibility::Private {
        module_exported_functions
            .entry(module_name.to_string())
            .or_default()
            .insert(f.name.clone());
    }

    module_function_infos.insert((module_name.to_string(), f.name.clone()), info.clone());
    functions.entry(f.name.clone()).or_insert_with(|| info);
}

pub fn resolve(program: &ir::Program, file: &str) -> (Resolution, Vec<Diagnostic>) {
    resolve_with_item_modules(program, file, None)
}

pub fn resolve_with_item_modules(
    program: &ir::Program,
    file: &str,
    item_modules: Option<&[Option<Vec<String>>]>,
) -> (Resolution, Vec<Diagnostic>) {
    resolve_with_item_modules_and_imports(program, file, item_modules, None)
}

pub fn resolve_with_item_modules_and_imports(
    program: &ir::Program,
    file: &str,
    item_modules: Option<&[Option<Vec<String>>]>,
    module_imports: Option<&BTreeMap<String, BTreeSet<String>>>,
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

    let mut imports_from_program = BTreeSet::new();
    for path in &program.imports {
        imports_from_program.insert(path.join("."));
    }

    let entry_module = program.module.as_ref().map(|m| m.join("."));
    let mut module_imports_map = module_imports.cloned().unwrap_or_default();
    let entry_module_name = entry_module
        .clone()
        .unwrap_or_else(|| ROOT_MODULE.to_string());
    let imports = module_imports_map
        .get(&entry_module_name)
        .cloned()
        .unwrap_or(imports_from_program);
    module_imports_map
        .entry(entry_module_name)
        .or_insert_with(|| imports.clone());

    let (import_aliases, ambiguous_import_aliases) = alias_maps_for_imports(&imports);
    let mut module_import_aliases = BTreeMap::new();
    let mut module_ambiguous_import_aliases = BTreeMap::new();
    for (module_name, module_import_set) in &module_imports_map {
        let (aliases, ambiguous) = alias_maps_for_imports(module_import_set);
        module_import_aliases.insert(module_name.clone(), aliases);
        module_ambiguous_import_aliases.insert(module_name.clone(), ambiguous);
    }

    let mut function_modules: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut module_functions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut module_exported_functions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

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
            ir::Item::Function(f) => register_function_item(
                &module_name,
                f,
                file,
                &mut value_decl_kind_by_module_name,
                &mut type_decl_kind_by_module_name,
                &mut diagnostics,
                &mut module_functions,
                &mut module_exported_functions,
                &mut function_modules,
                &mut module_function_infos,
                &mut functions,
            ),
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
                let mut field_visibility = BTreeMap::new();
                let mut default_fields = BTreeSet::new();
                for field in &s.fields {
                    if fields.insert(field.name.clone(), field.ty).is_some() {
                        diagnostics.push(Diagnostic::error(
                            "E1101",
                            format!("duplicate struct field '{}.{}'", s.name, field.name),
                            file,
                            field.span,
                        ));
                    }
                    let effective_field_visibility =
                        effective_visibility(&module_name, field.visibility, &field.name);
                    field_visibility.insert(field.name.clone(), effective_field_visibility);
                    if field.default_value.is_some() {
                        default_fields.insert(field.name.clone());
                    }
                }

                let struct_visibility = effective_visibility(&module_name, s.visibility, &s.name);
                structs.entry(s.name.clone()).or_insert_with(|| StructInfo {
                    symbol: s.symbol,
                    module: module_name.clone(),
                    visibility: struct_visibility,
                    generics: s.generics.iter().map(|g| g.name.clone()).collect(),
                    fields,
                    field_visibility,
                    default_fields,
                    invariant: s.invariant.clone(),
                    span: s.span,
                });
            }
            ir::Item::Enum(e) => {
                if let Some(existing_kind) = type_decl_kind_by_module_name
                    .insert((module_name.clone(), e.name.clone()), "enum")
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

                let enum_visibility = effective_visibility(&module_name, e.visibility, &e.name);
                enums.entry(e.name.clone()).or_insert_with(|| EnumInfo {
                    symbol: e.symbol,
                    module: module_name.clone(),
                    visibility: enum_visibility,
                    generics: e.generics.iter().map(|g| g.name.clone()).collect(),
                    variants,
                    span: e.span,
                });
            }
            ir::Item::Trait(t) => {
                if let Some(existing_kind) = type_decl_kind_by_module_name
                    .insert((module_name.clone(), t.name.clone()), "trait")
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
                let mut methods = BTreeMap::new();
                for method in &t.methods {
                    let method_key = method_name_key(&method.name).to_string();
                    let mut info = function_info_for(method);
                    info.visibility =
                        effective_visibility(&module_name, info.visibility, &method.name);
                    if methods.insert(method_key.clone(), info).is_some() {
                        diagnostics.push(
                            Diagnostic::error(
                                "E1107",
                                format!("duplicate trait method '{}.{}'", t.name, method_key),
                                file,
                                method.span,
                            )
                            .with_help("keep exactly one signature per trait method name"),
                        );
                    }
                }
                let trait_visibility = effective_visibility(&module_name, t.visibility, &t.name);
                traits.entry(t.name.clone()).or_insert_with(|| TraitInfo {
                    symbol: t.symbol,
                    module: module_name.clone(),
                    visibility: trait_visibility,
                    generics: t.generics.iter().map(|g| g.name.clone()).collect(),
                    methods,
                    span: t.span,
                });
            }
            ir::Item::Impl(impl_def) => {
                if impl_def.is_inherent {
                    let target_repr = impl_def
                        .target
                        .and_then(|id| type_repr_by_id.get(&id).cloned())
                        .unwrap_or_else(|| impl_def.trait_name.clone());
                    let target_name = base_type_name(&target_repr).to_string();
                    let known_target = structs.contains_key(&target_name)
                        || enums.contains_key(&target_name)
                        || program.items.iter().any(|item| {
                            matches!(item, ir::Item::Struct(def) if def.name == target_name)
                                || matches!(item, ir::Item::Enum(def) if def.name == target_name)
                        });
                    if !known_target {
                        diagnostics.push(Diagnostic::error(
                            "E1106",
                            format!("unknown type '{}' in inherent impl", target_name),
                            file,
                            impl_def.span,
                        ));
                    }
                    for method in &impl_def.methods {
                        register_function_item(
                            &module_name,
                            method,
                            file,
                            &mut value_decl_kind_by_module_name,
                            &mut type_decl_kind_by_module_name,
                            &mut diagnostics,
                            &mut module_functions,
                            &mut module_exported_functions,
                            &mut function_modules,
                            &mut module_function_infos,
                            &mut functions,
                        );
                    }
                    continue;
                }

                let trait_info = traits.get(&impl_def.trait_name).cloned().or_else(|| {
                    program.items.iter().find_map(|item| match item {
                        ir::Item::Trait(t) if t.name == impl_def.trait_name => Some(TraitInfo {
                            symbol: t.symbol,
                            module: ROOT_MODULE.to_string(),
                            visibility: effective_visibility(ROOT_MODULE, t.visibility, &t.name),
                            generics: t.generics.iter().map(|g| g.name.clone()).collect(),
                            methods: t
                                .methods
                                .iter()
                                .map(|method| {
                                    let mut info = function_info_for(method);
                                    info.visibility = effective_visibility(
                                        ROOT_MODULE,
                                        info.visibility,
                                        &method.name,
                                    );
                                    (method_name_key(&method.name).to_string(), info)
                                })
                                .collect(),
                            span: t.span,
                        }),
                        _ => None,
                    })
                });

                let Some(trait_info) = trait_info else {
                    diagnostics.push(Diagnostic::error(
                        "E1103",
                        format!("unknown trait '{}' in impl", impl_def.trait_name),
                        file,
                        impl_def.span,
                    ));
                    continue;
                };
                let trait_arity = trait_info.generics.len();

                let impl_arg_count = impl_def.trait_args.len();
                let allow_non_generic_trait_target = trait_arity == 0 && impl_arg_count == 1;
                if impl_arg_count != trait_arity && !allow_non_generic_trait_target {
                    diagnostics.push(Diagnostic::error(
                        "E1104",
                        format!(
                            "impl for trait '{}' expects {} type arguments, found {}",
                            impl_def.trait_name, trait_arity, impl_arg_count
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

                let mut type_bindings = BTreeMap::new();
                if let Some(first_arg) = impl_def.trait_args.first() {
                    type_bindings.insert(
                        "Self".to_string(),
                        type_repr_by_id
                            .get(first_arg)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string()),
                    );
                }
                for (idx, generic_name) in trait_info.generics.iter().enumerate() {
                    let Some(arg_id) = impl_def.trait_args.get(idx) else {
                        continue;
                    };
                    let repr = type_repr_by_id
                        .get(arg_id)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string());
                    type_bindings.insert(generic_name.clone(), repr);
                }

                let mut impl_methods = BTreeMap::new();
                for method in &impl_def.methods {
                    let method_key = method_name_key(&method.name).to_string();
                    if impl_methods
                        .insert(method_key.clone(), method as &ir::Function)
                        .is_some()
                    {
                        diagnostics.push(
                            Diagnostic::error(
                                "E1108",
                                format!(
                                    "duplicate method '{}.{}' in trait impl",
                                    impl_def.trait_name, method_key
                                ),
                                file,
                                method.span,
                            )
                            .with_help("keep one implementation per method in each trait impl"),
                        );
                    }
                }

                let mut missing = trait_info
                    .methods
                    .keys()
                    .filter(|name| !impl_methods.contains_key(*name))
                    .cloned()
                    .collect::<Vec<_>>();
                missing.sort();
                if !missing.is_empty() {
                    diagnostics.push(
                        Diagnostic::error(
                            "E1107",
                            format!(
                                "impl for trait '{}' is missing method(s): {}",
                                impl_def.trait_name,
                                missing.join(", ")
                            ),
                            file,
                            impl_def.span,
                        )
                        .with_help("implement every method declared in the trait"),
                    );
                }

                for (method_name, method) in &impl_methods {
                    let Some(expected) = trait_info.methods.get(method_name) else {
                        diagnostics.push(
                            Diagnostic::error(
                                "E1109",
                                format!(
                                    "method '{}' is not declared by trait '{}'",
                                    method_name, impl_def.trait_name
                                ),
                                file,
                                method.span,
                            )
                            .with_help("remove this method or add it to the trait declaration"),
                        );
                        continue;
                    };
                    if let Some(reason) = trait_method_signature_mismatch(
                        expected,
                        method,
                        &type_repr_by_id,
                        &type_bindings,
                    ) {
                        diagnostics.push(
                            Diagnostic::error(
                                "E1108",
                                format!(
                                    "method '{}.{}' does not match trait signature: {}",
                                    impl_def.trait_name, method_name, reason
                                ),
                                file,
                                method.span,
                            )
                            .with_help(
                                "match the trait method's parameters, return type, and modifiers",
                            ),
                        );
                    }
                }

                for method in &impl_def.methods {
                    register_function_item(
                        &module_name,
                        method,
                        file,
                        &mut value_decl_kind_by_module_name,
                        &mut type_decl_kind_by_module_name,
                        &mut diagnostics,
                        &mut module_functions,
                        &mut module_exported_functions,
                        &mut function_modules,
                        &mut module_function_infos,
                        &mut functions,
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

    let own_module = entry_module
        .clone()
        .unwrap_or_else(|| ROOT_MODULE.to_string());
    let mut visible_functions = BTreeSet::new();
    for module in &visible_modules {
        let source = if module == &own_module {
            module_functions.get(module)
        } else {
            module_exported_functions.get(module)
        };
        if let Some(names) = source {
            visible_functions.extend(names.iter().cloned());
        }
    }
    // Compiler-generated desugar paths may reference these intrinsics directly.
    // Keep them visible without requiring user-authored imports.
    visible_functions.insert("aic_vec_new_intrinsic".to_string());
    visible_functions.insert("aic_vec_push_intrinsic".to_string());
    visible_functions.insert("aic_string_format_intrinsic".to_string());

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

    for module_name in module_functions.keys() {
        module_imports_map.entry(module_name.clone()).or_default();
        module_import_aliases
            .entry(module_name.clone())
            .or_default();
        module_ambiguous_import_aliases
            .entry(module_name.clone())
            .or_default();
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
            module_imports: module_imports_map,
            entry_module,
            function_modules,
            module_functions,
            module_exported_functions,
            visible_functions,
            import_aliases,
            ambiguous_import_aliases,
            module_import_aliases,
            module_ambiguous_import_aliases,
        },
        diagnostics,
    )
}

fn alias_maps_for_imports(
    imports: &BTreeSet<String>,
) -> (BTreeMap<String, String>, BTreeSet<String>) {
    let mut import_aliases = BTreeMap::new();
    let mut ambiguous_import_aliases = BTreeSet::new();
    for import in imports {
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
    (import_aliases, ambiguous_import_aliases)
}

fn trait_method_signature_mismatch(
    expected: &FunctionInfo,
    found: &ir::Function,
    type_repr_by_id: &BTreeMap<ir::TypeId, String>,
    type_bindings: &BTreeMap<String, String>,
) -> Option<String> {
    if expected.is_async != found.is_async {
        return Some("async modifier does not match".to_string());
    }
    if expected.is_unsafe != found.is_unsafe {
        return Some("unsafe modifier does not match".to_string());
    }

    if expected.generics.len() != found.generics.len() {
        return Some(format!(
            "expected {} generic parameter(s), found {}",
            expected.generics.len(),
            found.generics.len()
        ));
    }
    for (idx, expected_generic) in expected.generics.iter().enumerate() {
        let expected_bounds = expected
            .generic_bounds
            .get(expected_generic)
            .cloned()
            .unwrap_or_default();
        let found_bounds = found
            .generics
            .get(idx)
            .map(|g| g.bounds.clone())
            .unwrap_or_default();
        if expected_bounds != found_bounds {
            return Some(format!(
                "generic parameter {} bounds differ: expected '{}', found '{}'",
                idx + 1,
                expected_bounds.join(" + "),
                found_bounds.join(" + ")
            ));
        }
    }

    let expected_params = expected
        .param_types
        .iter()
        .map(|ty| {
            let raw = type_repr_by_id
                .get(ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            substitute_type_vars(&raw, type_bindings)
        })
        .collect::<Vec<_>>();
    let found_params = found
        .params
        .iter()
        .map(|param| {
            type_repr_by_id
                .get(&param.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string())
        })
        .collect::<Vec<_>>();
    if expected_params.len() != found_params.len() {
        return Some(format!(
            "expected {} parameter(s), found {}",
            expected_params.len(),
            found_params.len()
        ));
    }
    for (idx, (expected_ty, found_ty)) in
        expected_params.iter().zip(found_params.iter()).enumerate()
    {
        if expected_ty != found_ty {
            return Some(format!(
                "parameter {} type mismatch: expected '{}', found '{}'",
                idx + 1,
                expected_ty,
                found_ty
            ));
        }
    }

    let expected_ret = type_repr_by_id
        .get(&expected.ret_type)
        .map(|raw| substitute_type_vars(raw, type_bindings))
        .unwrap_or_else(|| "<?>".to_string());
    let found_ret = type_repr_by_id
        .get(&found.ret_type)
        .cloned()
        .unwrap_or_else(|| "<?>".to_string());
    if expected_ret != found_ret {
        return Some(format!(
            "return type mismatch: expected '{}', found '{}'",
            expected_ret, found_ret
        ));
    }

    let found_effects = found.effects.iter().cloned().collect::<BTreeSet<_>>();
    if expected.effects != found_effects {
        return Some(format!(
            "effects mismatch: expected '{{ {} }}', found '{{ {} }}'",
            expected
                .effects
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            found_effects.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }

    None
}

fn substitute_type_vars(ty: &str, bindings: &BTreeMap<String, String>) -> String {
    if let Some(bound) = bindings.get(ty) {
        return bound.clone();
    }

    let Some(args) = extract_generic_args(ty) else {
        return ty.to_string();
    };

    let substituted = args
        .iter()
        .map(|arg| substitute_type_vars(arg, bindings))
        .collect::<Vec<_>>();
    format!("{}[{}]", base_type_name(ty), substituted.join(", "))
}

fn extract_generic_args(ty: &str) -> Option<Vec<String>> {
    let start = ty.find('[')?;
    if !ty.ends_with(']') || start + 1 > ty.len() - 1 {
        return None;
    }
    let inner = &ty[start + 1..ty.len() - 1];
    if inner.is_empty() {
        return Some(Vec::new());
    }
    Some(split_top_level(inner))
}

fn split_top_level(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in input.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(input[start..idx].trim().to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = input[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
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

fn base_type_name(ty: &str) -> &str {
    ty.split('[').next().unwrap_or(ty)
}

fn method_name_key(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
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
