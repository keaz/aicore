use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostics::Diagnostic;
use crate::ir;

const ROOT_MODULE: &str = "<root>";

#[derive(Debug, Clone)]
pub struct Resolution {
    pub functions: BTreeMap<String, FunctionInfo>,
    pub structs: BTreeMap<String, StructInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
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
    pub generics: Vec<String>,
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
    let mut structs = BTreeMap::new();
    let mut enums = BTreeMap::new();

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
                    .insert(module_name);

                functions
                    .entry(f.name.clone())
                    .or_insert_with(|| FunctionInfo {
                        symbol: f.symbol,
                        is_async: f.is_async,
                        generics: f.generics.iter().map(|g| g.name.clone()).collect(),
                        param_types: f.params.iter().map(|p| p.ty).collect(),
                        ret_type: f.ret_type,
                        effects: f.effects.iter().cloned().collect(),
                        span: f.span,
                    });
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

    let mut imported_name_sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for module in &imports {
        if let Some(names) = module_functions.get(module) {
            for name in names {
                imported_name_sources
                    .entry(name.clone())
                    .or_default()
                    .push(module.clone());
            }
        }
    }

    for (name, mut modules) in imported_name_sources {
        modules.sort();
        modules.dedup();
        if modules.len() > 1 {
            diagnostics.push(
                Diagnostic::error(
                    "E2104",
                    format!(
                        "ambiguous imported symbol '{}' exported by modules: {}",
                        name,
                        modules.join(", ")
                    ),
                    file,
                    program.span,
                )
                .with_help(format!(
                    "use a qualified call (for example `{alias}.{name}(...)`) or import fewer colliding modules",
                    alias = modules[0].rsplit('.').next().unwrap_or(&modules[0])
                )),
            );
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
            structs,
            enums,
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
}
