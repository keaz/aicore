use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::ast::{decode_internal_const, decode_internal_type_alias, BinOp};
use crate::diagnostics::{Diagnostic, DiagnosticSpan, SuggestedFix};
use crate::ir;
use crate::resolver::{EnumInfo, FunctionInfo, Resolution, StructInfo};
use crate::std_policy::find_deprecated_api;

const TUPLE_INTERNAL_NAME: &str = "Tuple";

#[derive(Debug, Clone, Default)]
pub struct TypecheckOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub function_effect_usage: BTreeMap<String, BTreeSet<String>>,
    pub function_effect_reasons: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub generic_instantiations: Vec<ir::GenericInstantiation>,
    pub call_graph: BTreeMap<String, Vec<String>>,
    pub holes: Vec<TypedHole>,
}

#[derive(Debug, Clone)]
pub struct TypedHole {
    pub file: String,
    pub span: crate::span::Span,
    pub inferred: String,
    pub context: String,
}

#[derive(Debug, Clone)]
struct DeferredHole {
    span: crate::span::Span,
    context: String,
}

pub fn check(program: &ir::Program, resolution: &Resolution, file: &str) -> TypecheckOutput {
    let mut checker = Checker::new(program, resolution, file);
    checker.run();
    checker.finish()
}

#[derive(Debug, Clone)]
struct FnSig {
    is_async: bool,
    is_unsafe: bool,
    is_extern: bool,
    extern_abi: Option<String>,
    params: Vec<String>,
    ret: String,
    effects: BTreeSet<String>,
    generic_params: Vec<String>,
    generic_bounds: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
struct AliasDef {
    generics: Vec<String>,
    target: String,
}

struct Checker<'a> {
    program: &'a ir::Program,
    resolution: &'a Resolution,
    file: &'a str,
    source: Option<String>,
    diagnostics: Vec<Diagnostic>,
    types: BTreeMap<ir::TypeId, String>,
    functions: BTreeMap<String, FnSig>,
    module_functions: BTreeMap<(String, String), FnSig>,
    function_module_by_symbol: BTreeMap<ir::SymbolId, String>,
    generic_arity: BTreeMap<String, usize>,
    effect_usage: BTreeMap<String, BTreeSet<String>>,
    effect_reasons: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    call_graph: BTreeMap<String, Vec<CallEdge>>,
    function_spans: BTreeMap<String, (crate::span::Span, crate::span::Span)>,
    current_function: Option<String>,
    current_function_is_async: bool,
    current_function_is_unsafe: bool,
    current_function_ret_type: Option<String>,
    unsafe_depth: usize,
    instantiation_seen: BTreeMap<String, PendingInstantiation>,
    mangled_keys: BTreeMap<String, String>,
    enforce_import_visibility: bool,
    type_aliases: BTreeMap<String, AliasDef>,
    const_types: BTreeMap<String, String>,
    typed_holes: Vec<TypedHole>,
    fn_param_holes: BTreeMap<(String, usize), DeferredHole>,
    fn_param_inferred: BTreeMap<(String, usize), String>,
    fn_return_holes: BTreeMap<String, DeferredHole>,
    fn_return_inferred: BTreeMap<String, String>,
    struct_field_holes: BTreeMap<(String, String), DeferredHole>,
    struct_field_inferred: BTreeMap<(String, String), String>,
    current_param_positions: BTreeMap<String, usize>,
}

#[derive(Default)]
struct ExprContext {
    effects_used: BTreeSet<String>,
    loop_stack: Vec<LoopContext>,
}

#[derive(Debug, Clone, Default)]
struct LoopContext {
    break_ty: Option<String>,
}

#[derive(Debug, Clone)]
struct VariantMatch {
    enum_name: String,
    generic_params: Vec<String>,
    enum_symbol: ir::SymbolId,
    payload: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingInstantiation {
    kind: ir::GenericInstantiationKind,
    name: String,
    symbol: Option<ir::SymbolId>,
    type_args: Vec<String>,
    mangled: String,
}

#[derive(Debug, Clone)]
struct CallEdge {
    callee: String,
    span: crate::span::Span,
}

#[derive(Debug, Clone)]
struct EffectPath {
    nodes: Vec<String>,
    span: crate::span::Span,
}

#[derive(Debug, Clone, Default)]
struct BorrowState {
    active_by_target: BTreeMap<String, Vec<ActiveBorrow>>,
    persistent_by_owner: BTreeMap<ir::SymbolId, PersistentBorrow>,
    moved_by_binding: BTreeMap<String, crate::span::Span>,
}

#[derive(Debug, Clone)]
struct ActiveBorrow {
    mutable: bool,
    span: crate::span::Span,
    owner: Option<ir::SymbolId>,
}

#[derive(Debug, Clone)]
struct PersistentBorrow {
    target: String,
    mutable: bool,
    span: crate::span::Span,
}

#[derive(Debug, Clone)]
struct TempBorrow {
    target: String,
    mutable: bool,
    span: crate::span::Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ResourceKind {
    Task,
    IntChannel,
    IntMutex,
}

impl ResourceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Task => "Task",
            Self::IntChannel => "IntChannel",
            Self::IntMutex => "IntMutex",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ResourceState {
    closed_at: crate::span::Span,
}

type ResourceStateMap = BTreeMap<(String, ResourceKind), ResourceState>;

#[derive(Debug, Clone, Copy)]
struct ResourceProtocolOp {
    kind: ResourceKind,
    terminal: bool,
    api: &'static str,
}

impl<'a> Checker<'a> {
    fn new(program: &'a ir::Program, resolution: &'a Resolution, file: &'a str) -> Self {
        let mut types = BTreeMap::new();
        for ty in &program.types {
            types.insert(ty.id, ty.repr.clone());
        }

        let mut functions = BTreeMap::new();
        let mut module_functions = BTreeMap::new();
        let mut function_module_by_symbol = BTreeMap::new();
        let mut generic_arity = BTreeMap::new();
        generic_arity.insert("Option".to_string(), 1);
        generic_arity.insert("Result".to_string(), 2);
        generic_arity.insert("Async".to_string(), 1);
        generic_arity.insert("Ref".to_string(), 1);
        generic_arity.insert("RefMut".to_string(), 1);
        let mut type_aliases = BTreeMap::new();
        let mut const_types = BTreeMap::new();

        for item in &program.items {
            let ir::Item::Function(func) = item else {
                continue;
            };

            if let Some(alias_name) = decode_internal_type_alias(&func.name) {
                type_aliases
                    .entry(alias_name.to_string())
                    .or_insert_with(|| AliasDef {
                        generics: func.generics.iter().map(|g| g.name.clone()).collect(),
                        target: types
                            .get(&func.ret_type)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string()),
                    });
                continue;
            }

            if let Some(const_name) = decode_internal_const(&func.name) {
                let declared_ty = types
                    .get(&func.ret_type)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string());
                const_types
                    .entry(const_name.to_string())
                    .or_insert_with(|| declared_ty.clone());
            }
        }

        for (name, info) in &resolution.functions {
            let mut params = Vec::new();
            for ty in &info.param_types {
                params.push(types.get(ty).cloned().unwrap_or_else(|| "<?>".to_string()));
            }
            functions.insert(
                name.clone(),
                FnSig {
                    is_async: info.is_async,
                    is_unsafe: info.is_unsafe,
                    is_extern: info.is_extern,
                    extern_abi: info.extern_abi.clone(),
                    params,
                    ret: types
                        .get(&info.ret_type)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string()),
                    effects: info.effects.clone(),
                    generic_params: info.generics.clone(),
                    generic_bounds: info.generic_bounds.clone(),
                },
            );
        }

        for ((module, name), info) in &resolution.module_function_infos {
            function_module_by_symbol.insert(info.symbol, module.clone());
            let mut params = Vec::new();
            for ty in &info.param_types {
                params.push(types.get(ty).cloned().unwrap_or_else(|| "<?>".to_string()));
            }
            module_functions.insert(
                (module.clone(), name.clone()),
                FnSig {
                    is_async: info.is_async,
                    is_unsafe: info.is_unsafe,
                    is_extern: info.is_extern,
                    extern_abi: info.extern_abi.clone(),
                    params,
                    ret: types
                        .get(&info.ret_type)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string()),
                    effects: info.effects.clone(),
                    generic_params: info.generics.clone(),
                    generic_bounds: info.generic_bounds.clone(),
                },
            );
        }

        for (name, info) in &resolution.structs {
            generic_arity.insert(name.clone(), info.generics.len());
        }
        for (name, info) in &resolution.enums {
            generic_arity.insert(name.clone(), info.generics.len());
        }
        for (name, alias) in &type_aliases {
            generic_arity.insert(name.clone(), alias.generics.len());
        }

        // Minimal std signatures.
        functions.insert(
            "print_int".to_string(),
            FnSig {
                is_async: false,
                is_unsafe: false,
                is_extern: false,
                extern_abi: None,
                params: vec!["Int".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                generic_params: Vec::new(),
                generic_bounds: BTreeMap::new(),
            },
        );
        functions.insert(
            "print_str".to_string(),
            FnSig {
                is_async: false,
                is_unsafe: false,
                is_extern: false,
                extern_abi: None,
                params: vec!["String".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                generic_params: Vec::new(),
                generic_bounds: BTreeMap::new(),
            },
        );
        functions.insert(
            "print_float".to_string(),
            FnSig {
                is_async: false,
                is_unsafe: false,
                is_extern: false,
                extern_abi: None,
                params: vec!["Float".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                generic_params: Vec::new(),
                generic_bounds: BTreeMap::new(),
            },
        );
        functions.insert(
            "len".to_string(),
            FnSig {
                is_async: false,
                is_unsafe: false,
                is_extern: false,
                extern_abi: None,
                params: vec!["String".to_string()],
                ret: "Int".to_string(),
                effects: BTreeSet::new(),
                generic_params: Vec::new(),
                generic_bounds: BTreeMap::new(),
            },
        );
        functions.insert(
            "panic".to_string(),
            FnSig {
                is_async: false,
                is_unsafe: false,
                is_extern: false,
                extern_abi: None,
                params: vec!["String".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                generic_params: Vec::new(),
                generic_bounds: BTreeMap::new(),
            },
        );

        Self {
            program,
            resolution,
            file,
            source: std::fs::read_to_string(file).ok(),
            diagnostics: Vec::new(),
            types,
            functions,
            module_functions,
            function_module_by_symbol,
            generic_arity,
            effect_usage: BTreeMap::new(),
            effect_reasons: BTreeMap::new(),
            call_graph: BTreeMap::new(),
            function_spans: BTreeMap::new(),
            current_function: None,
            current_function_is_async: false,
            current_function_is_unsafe: false,
            current_function_ret_type: None,
            unsafe_depth: 0,
            instantiation_seen: BTreeMap::new(),
            mangled_keys: BTreeMap::new(),
            enforce_import_visibility: false,
            type_aliases,
            const_types,
            typed_holes: Vec::new(),
            fn_param_holes: BTreeMap::new(),
            fn_param_inferred: BTreeMap::new(),
            fn_return_holes: BTreeMap::new(),
            fn_return_inferred: BTreeMap::new(),
            struct_field_holes: BTreeMap::new(),
            struct_field_inferred: BTreeMap::new(),
            current_param_positions: BTreeMap::new(),
        }
    }

    fn run(&mut self) {
        self.check_no_null_boundary();
        for item in &self.program.items {
            match item {
                ir::Item::Function(func) => {
                    if decode_internal_type_alias(&func.name).is_some() {
                        self.check_type_alias_item(func);
                    } else if decode_internal_const(&func.name).is_some() {
                        self.check_const_item(func);
                    } else {
                        self.check_function(func);
                    }
                }
                ir::Item::Struct(strukt) => self.check_struct_invariant(strukt),
                ir::Item::Enum(enm) => self.check_enum_definition(enm),
                ir::Item::Trait(_) => {}
                ir::Item::Impl(impl_def) => {
                    for method in &impl_def.methods {
                        self.check_function(method);
                    }
                }
            }
        }
        self.check_transitive_effects();
    }

    fn finish(mut self) -> TypecheckOutput {
        self.flush_deferred_holes();
        let generic_instantiations = self
            .instantiation_seen
            .into_values()
            .enumerate()
            .map(|(idx, pending)| ir::GenericInstantiation {
                id: (idx + 1) as u32,
                kind: pending.kind,
                name: pending.name,
                symbol: pending.symbol,
                type_args: pending.type_args,
                mangled: pending.mangled,
            })
            .collect::<Vec<_>>();
        let call_graph = self
            .call_graph
            .into_iter()
            .map(|(caller, edges)| {
                let mut callees = edges
                    .into_iter()
                    .map(|edge| edge.callee)
                    .collect::<Vec<_>>();
                callees.sort();
                callees.dedup();
                (caller, callees)
            })
            .collect::<BTreeMap<_, _>>();
        TypecheckOutput {
            diagnostics: self.diagnostics,
            function_effect_usage: self.effect_usage,
            function_effect_reasons: self.effect_reasons,
            generic_instantiations,
            call_graph,
            holes: self.typed_holes,
        }
    }

    fn check_type_alias_item(&mut self, func: &ir::Function) {
        let target = self
            .types
            .get(&func.ret_type)
            .cloned()
            .unwrap_or_else(|| "<?>".to_string());
        self.check_generic_arity(&target, func.span);
    }

    fn check_const_item(&mut self, func: &ir::Function) {
        let Some(const_name) = decode_internal_const(&func.name) else {
            return;
        };
        let declared_ty = self
            .types
            .get(&func.ret_type)
            .cloned()
            .unwrap_or_else(|| "<?>".to_string());
        self.check_generic_arity(&declared_ty, func.span);
        let Some(expr) = &func.body.tail else {
            self.diagnostics.push(Diagnostic::error(
                "E1288",
                format!(
                    "const '{}' is missing an initializer expression",
                    const_name
                ),
                self.file,
                func.span,
            ));
            return;
        };

        self.validate_const_initializer(const_name, expr);

        let mut locals = BTreeMap::new();
        let mut ctx = ExprContext::default();
        let expr_ty = self.check_expr_with_expected(
            expr,
            &mut locals,
            &BTreeSet::new(),
            &mut ctx,
            true,
            Some(&declared_ty),
        );
        if !self.types_compatible(&declared_ty, &expr_ty) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1288",
                    format!(
                        "const '{}' expects type '{}', found '{}'",
                        const_name, declared_ty, expr_ty
                    ),
                    self.file,
                    func.span,
                )
                .with_help("make the const initializer type match its annotation"),
            );
        }
    }

    fn validate_const_initializer(&mut self, const_name: &str, expr: &ir::Expr) {
        match &expr.kind {
            ir::ExprKind::Int(_)
            | ir::ExprKind::Float(_)
            | ir::ExprKind::Bool(_)
            | ir::ExprKind::String(_)
            | ir::ExprKind::Unit => {}
            ir::ExprKind::Var(name) => {
                if !self.const_types.contains_key(name) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1287",
                            format!(
                                "const '{}' initializer can only reference other constants, found '{}'",
                                const_name, name
                            ),
                            self.file,
                            expr.span,
                        )
                        .with_help("reference a previously declared const symbol"),
                    );
                }
            }
            ir::ExprKind::Unary { op, expr: inner } => match op {
                crate::ast::UnaryOp::Neg | crate::ast::UnaryOp::Not => {
                    self.validate_const_initializer(const_name, inner);
                }
            },
            ir::ExprKind::Binary { lhs, rhs, .. } => {
                self.validate_const_initializer(const_name, lhs);
                self.validate_const_initializer(const_name, rhs);
            }
            ir::ExprKind::Call { .. } => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1287",
                        format!(
                            "const '{}' initializer cannot call functions at compile time",
                            const_name
                        ),
                        self.file,
                        expr.span,
                    )
                    .with_help("use literals, operators, and other constants only"),
                );
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1287",
                        format!(
                            "const '{}' initializer only supports literals, unary/binary operators, and const references",
                            const_name
                        ),
                        self.file,
                        expr.span,
                    )
                    .with_help("move non-constant logic into a function and assign its result at runtime"),
                );
            }
        }
    }

    fn check_function(&mut self, func: &ir::Function) {
        let previous_enforce = self.enforce_import_visibility;
        let previous_function = self.current_function.replace(func.name.clone());
        let previous_async = self.current_function_is_async;
        let previous_unsafe = self.current_function_is_unsafe;
        let previous_ret = self.current_function_ret_type.clone();
        let previous_unsafe_depth = self.unsafe_depth;
        self.enforce_import_visibility = self.should_enforce_import_visibility(func.symbol);
        self.current_function_is_async = func.is_async;
        self.current_function_is_unsafe = func.is_unsafe;
        self.unsafe_depth = 0;
        self.call_graph.entry(func.name.clone()).or_default();
        self.function_spans
            .insert(func.name.clone(), (func.span, func.body.span));
        self.current_param_positions.clear();

        let declared_effects: BTreeSet<String> = func.effects.iter().cloned().collect();
        let mut locals = BTreeMap::new();
        for (idx, param) in func.params.iter().enumerate() {
            let param_ty = self
                .types
                .get(&param.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            self.check_generic_arity(&param_ty, param.span);
            if contains_unresolved_type(&param_ty) {
                let key = (func.name.clone(), idx);
                self.fn_param_holes
                    .entry(key.clone())
                    .or_insert(DeferredHole {
                        span: param.span,
                        context: format!("parameter '{}' in function '{}'", param.name, func.name),
                    });
                self.fn_param_inferred
                    .entry(key)
                    .or_insert_with(|| param_ty.clone());
            }
            self.current_param_positions.insert(param.name.clone(), idx);
            locals.insert(param.name.clone(), param_ty);
        }

        let ret_type = self
            .types
            .get(&func.ret_type)
            .cloned()
            .unwrap_or_else(|| "<?>".to_string());
        if contains_unresolved_type(&ret_type) {
            self.fn_return_holes
                .entry(func.name.clone())
                .or_insert(DeferredHole {
                    span: func.span,
                    context: format!("return type in function '{}'", func.name),
                });
            self.fn_return_inferred
                .entry(func.name.clone())
                .or_insert_with(|| ret_type.clone());
        }
        self.current_function_ret_type = Some(ret_type.clone());
        self.check_generic_arity(&ret_type, func.span);

        if func.is_extern {
            self.check_extern_function_signature(func, &ret_type, &locals);
            self.effect_usage.insert(func.name.clone(), BTreeSet::new());
            self.current_param_positions.clear();
            self.enforce_import_visibility = previous_enforce;
            self.current_function = previous_function;
            self.current_function_is_async = previous_async;
            self.current_function_is_unsafe = previous_unsafe;
            self.current_function_ret_type = previous_ret;
            self.unsafe_depth = previous_unsafe_depth;
            return;
        }

        for generic in &func.generics {
            for bound in &generic.bounds {
                let Some(trait_info) = self.resolution.traits.get(bound) else {
                    self.diagnostics.push(Diagnostic::error(
                        "E1259",
                        format!(
                            "unknown trait bound '{}' on generic parameter '{}'",
                            bound, generic.name
                        ),
                        self.file,
                        func.span,
                    ));
                    continue;
                };
                if trait_info.generics.len() != 1 {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1259",
                            format!(
                                "trait bound '{}' on '{}' expects trait arity 1 for `T: Trait` syntax",
                                bound, generic.name
                            ),
                            self.file,
                            func.span,
                        )
                        .with_help(format!(
                            "trait '{}' currently declares {} generic parameter(s)",
                            bound,
                            trait_info.generics.len()
                        )),
                    );
                }
            }
        }

        if let Some(requires) = &func.requires {
            let mut contract_ctx = ExprContext::default();
            let ty = self.check_expr(
                requires,
                &mut locals.clone(),
                &declared_effects,
                &mut contract_ctx,
                true,
            );
            if self.normalize_type(&ty) != "Bool" {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1200",
                        "requires contract must have type Bool",
                        self.file,
                        requires.span,
                    )
                    .with_help(format!("found `{}`", ty)),
                );
            }
        }

        if let Some(ensures) = &func.ensures {
            let mut contract_locals = locals.clone();
            contract_locals.insert("result".to_string(), ret_type.clone());
            let mut contract_ctx = ExprContext::default();
            let ty = self.check_expr(
                ensures,
                &mut contract_locals,
                &declared_effects,
                &mut contract_ctx,
                true,
            );
            if self.normalize_type(&ty) != "Bool" {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1201",
                        "ensures contract must have type Bool",
                        self.file,
                        ensures.span,
                    )
                    .with_help(format!("found `{}`", ty)),
                );
            }
        }

        let mut body_ctx = ExprContext::default();
        let body_ty = self.check_block(
            &func.body,
            &mut locals,
            &ret_type,
            &declared_effects,
            &mut body_ctx,
            false,
        );
        if contains_unresolved_type(&ret_type) {
            self.observe_fn_return_hole(&func.name, &body_ty);
        }
        self.check_mutability_and_borrows(func);
        self.check_resource_protocols(func);

        if !self.types_compatible(&ret_type, &body_ty) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1202",
                    format!(
                        "function '{}' returns '{}', but body has type '{}'",
                        func.name, ret_type, body_ty
                    ),
                    self.file,
                    func.body.span,
                )
                .with_help("make the block tail expression match the return type"),
            );
        }

        if !body_ctx.effects_used.is_subset(&declared_effects) {
            let missing = body_ctx
                .effects_used
                .difference(&declared_effects)
                .cloned()
                .collect::<Vec<_>>();
            let mut required_effects = declared_effects.clone();
            required_effects.extend(body_ctx.effects_used.iter().cloned());

            let mut diagnostic = Diagnostic::error(
                "E2001",
                format!(
                    "function '{}' uses undeclared effects: {}",
                    func.name,
                    missing.join(", ")
                ),
                self.file,
                func.span,
            )
            .with_help(format!(
                "declare `effects {{ {} }}` on the function",
                missing.join(", ")
            ));

            if let Some(fix) = self.effect_declaration_fix(
                &func.name,
                func.span,
                func.body.span,
                &required_effects,
            ) {
                diagnostic = diagnostic.with_fix(fix);
            }

            self.diagnostics.push(diagnostic);
        }

        self.effect_usage
            .insert(func.name.clone(), body_ctx.effects_used);

        let param_inferred = (0..func.params.len())
            .map(|idx| {
                self.fn_param_inferred
                    .get(&(func.name.clone(), idx))
                    .cloned()
            })
            .collect::<Vec<_>>();
        let return_inferred = self.fn_return_inferred.get(&func.name).cloned();
        if let Some(sig) = self.functions.get_mut(&func.name) {
            for (idx, inferred) in param_inferred.into_iter().enumerate() {
                if let Some(inferred) = inferred {
                    sig.params[idx] = merge_types(&sig.params[idx], &inferred);
                }
            }
            if let Some(inferred_ret) = return_inferred {
                sig.ret = merge_types(&sig.ret, &inferred_ret);
            }
        }

        self.current_param_positions.clear();
        self.enforce_import_visibility = previous_enforce;
        self.current_function = previous_function;
        self.current_function_is_async = previous_async;
        self.current_function_is_unsafe = previous_unsafe;
        self.current_function_ret_type = previous_ret;
        self.unsafe_depth = previous_unsafe_depth;
    }

    fn should_enforce_import_visibility(&self, function_symbol: ir::SymbolId) -> bool {
        let Some(function_module) = self.function_module_by_symbol.get(&function_symbol) else {
            return true;
        };
        match self.resolution.entry_module.as_deref() {
            Some(entry_module) => function_module == entry_module,
            None => function_module == "<root>",
        }
    }

    fn check_extern_function_signature(
        &mut self,
        func: &ir::Function,
        ret_type: &str,
        locals: &BTreeMap<String, String>,
    ) {
        match func.extern_abi.as_deref() {
            Some("C") => {}
            Some(other) => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E2120",
                        format!(
                            "unsupported extern ABI '{}' on function '{}'",
                            other, func.name
                        ),
                        self.file,
                        func.span,
                    )
                    .with_help("use `extern \"C\" fn ...;` for currently supported native interop"),
                );
            }
            None => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E2120",
                        format!(
                            "extern function '{}' is missing an ABI declaration",
                            func.name
                        ),
                        self.file,
                        func.span,
                    )
                    .with_help("declare extern functions as `extern \"C\" fn ...;`"),
                );
            }
        }

        if func.is_async
            || !func.generics.is_empty()
            || !func.effects.is_empty()
            || func.requires.is_some()
            || func.ensures.is_some()
        {
            self.diagnostics.push(
                Diagnostic::error(
                    "E2121",
                    format!(
                        "extern function '{}' must be a plain signature without async/generics/effects/contracts",
                        func.name
                    ),
                    self.file,
                    func.span,
                )
                .with_help("declare the raw extern signature, then add a separate wrapper function"),
            );
        }

        for param in &func.params {
            let ty = locals
                .get(&param.name)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            if !is_c_abi_compatible_type(&ty) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E2123",
                        format!(
                            "extern function '{}' parameter '{}' has unsupported C ABI type '{}'",
                            func.name, param.name, ty
                        ),
                        self.file,
                        param.span,
                    )
                    .with_help("supported extern C types in MVP are Int, Bool, and ()"),
                );
            }
        }
        if !is_c_abi_compatible_type(ret_type) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E2123",
                    format!(
                        "extern function '{}' return type '{}' is not C ABI-compatible",
                        func.name, ret_type
                    ),
                    self.file,
                    func.span,
                )
                .with_help("use Int/Bool/() for raw extern signatures and convert in wrapper code"),
            );
        }
    }

    fn record_call_edge(&mut self, callee: &str, span: crate::span::Span) {
        let Some(caller) = self.current_function.as_ref() else {
            return;
        };
        self.call_graph
            .entry(caller.clone())
            .or_default()
            .push(CallEdge {
                callee: callee.to_string(),
                span,
            });
    }

    fn check_transitive_effects(&mut self) {
        let user_functions = self
            .program
            .items
            .iter()
            .filter_map(|item| match item {
                ir::Item::Function(func)
                    if decode_internal_type_alias(&func.name).is_none()
                        && decode_internal_const(&func.name).is_none() =>
                {
                    Some(func.name.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        let mut memo = BTreeMap::new();
        for function in &user_functions {
            let mut visiting = BTreeSet::new();
            let closure = self.compute_effect_closure(function, &mut visiting, &mut memo);
            self.effect_usage.insert(function.clone(), closure);
        }

        for function in &user_functions {
            let declared = self
                .functions
                .get(function)
                .map(|sig| sig.effects.clone())
                .unwrap_or_default();
            let closure = self.effect_usage.get(function).cloned().unwrap_or_default();
            let mut reasons = BTreeMap::new();
            for effect in &closure {
                let nodes = self
                    .find_effect_path(function, effect)
                    .map(|path| path.nodes)
                    .unwrap_or_else(|| vec![function.clone()]);
                reasons.insert(effect.clone(), nodes);
            }
            self.effect_reasons.insert(function.clone(), reasons);

            let missing = closure.difference(&declared).cloned().collect::<Vec<_>>();
            for effect in missing {
                let Some(path) = self.find_effect_path(function, &effect) else {
                    continue;
                };
                if path.nodes.len() < 3 {
                    continue;
                }
                let mut diagnostic = Diagnostic::error(
                    "E2005",
                    format!(
                        "function '{}' requires transitive effect '{}' via call path {}",
                        function,
                        effect,
                        path.nodes.join(" -> ")
                    ),
                    self.file,
                    path.span,
                )
                .with_help(format!(
                    "declare `effects {{ {} }}` on '{}' or refactor the call chain",
                    effect, function
                ));
                if let Some((function_span, body_span)) = self.function_spans.get(function).copied()
                {
                    if let Some(fix) =
                        self.effect_declaration_fix(function, function_span, body_span, &closure)
                    {
                        diagnostic = diagnostic.with_fix(fix);
                    }
                }
                self.diagnostics.push(diagnostic);
            }
        }
    }

    fn compute_effect_closure(
        &self,
        function: &str,
        visiting: &mut BTreeSet<String>,
        memo: &mut BTreeMap<String, BTreeSet<String>>,
    ) -> BTreeSet<String> {
        if let Some(cached) = memo.get(function) {
            return cached.clone();
        }

        if !visiting.insert(function.to_string()) {
            return self
                .functions
                .get(function)
                .map(|sig| sig.effects.clone())
                .unwrap_or_default();
        }

        let mut required = self
            .functions
            .get(function)
            .map(|sig| sig.effects.clone())
            .unwrap_or_default();

        if let Some(edges) = self.call_graph.get(function) {
            for edge in edges {
                if let Some(sig) = self.functions.get(&edge.callee) {
                    required.extend(sig.effects.iter().cloned());
                }
                if self.resolution.functions.contains_key(&edge.callee) {
                    required.extend(self.compute_effect_closure(&edge.callee, visiting, memo));
                }
            }
        }

        visiting.remove(function);
        memo.insert(function.to_string(), required.clone());
        required
    }

    fn find_effect_path(&self, start: &str, effect: &str) -> Option<EffectPath> {
        let mut queue = VecDeque::new();
        let mut visited = BTreeSet::new();
        visited.insert(start.to_string());
        queue.push_back((start.to_string(), vec![start.to_string()], None));

        while let Some((node, path, first_span)) = queue.pop_front() {
            let Some(edges) = self.call_graph.get(&node) else {
                continue;
            };
            for edge in edges {
                let mut next_path = path.clone();
                next_path.push(edge.callee.clone());
                let span = first_span.unwrap_or(edge.span);

                if self
                    .functions
                    .get(&edge.callee)
                    .map(|sig| sig.effects.contains(effect))
                    .unwrap_or(false)
                {
                    return Some(EffectPath {
                        nodes: next_path,
                        span,
                    });
                }

                if !self.resolution.functions.contains_key(&edge.callee) {
                    continue;
                }
                if visited.insert(edge.callee.clone()) {
                    queue.push_back((edge.callee.clone(), next_path, Some(span)));
                }
            }
        }

        None
    }

    fn effect_declaration_fix(
        &self,
        function_name: &str,
        function_span: crate::span::Span,
        body_span: crate::span::Span,
        required_effects: &BTreeSet<String>,
    ) -> Option<SuggestedFix> {
        let source = self.source.as_ref()?;
        if required_effects.is_empty()
            || function_span.start > function_span.end
            || body_span.start < function_span.start
            || body_span.start > source.len()
            || function_span.start > source.len()
            || function_span.end > source.len()
        {
            return None;
        }
        if !source.is_char_boundary(function_span.start)
            || !source.is_char_boundary(function_span.end)
            || !source.is_char_boundary(body_span.start)
        {
            return None;
        }

        let signature = &source[function_span.start..body_span.start];
        let effects = required_effects.iter().cloned().collect::<Vec<_>>();
        if effects.is_empty() {
            return None;
        }
        let effects_text = effects.join(", ");

        if let Some((clause_rel_start, clause_rel_end)) = Self::find_effect_clause(signature) {
            let start = function_span.start + clause_rel_start;
            let end = function_span.start + clause_rel_end;
            return Some(SuggestedFix {
                message: format!(
                    "update effect declaration on '{}' to include required effects",
                    function_name
                ),
                replacement: Some(format!("effects {{ {} }}", effects_text)),
                start: Some(start),
                end: Some(end),
            });
        }

        let insertion_before = [
            Self::find_keyword(signature, "requires"),
            Self::find_keyword(signature, "ensures"),
        ]
        .into_iter()
        .flatten()
        .min();
        let (insert_rel, replacement) = if let Some(rel) = insertion_before {
            (rel, format!("effects {{ {} }} ", effects_text))
        } else {
            (signature.len(), format!(" effects {{ {} }}", effects_text))
        };
        let insert = function_span.start + insert_rel;
        Some(SuggestedFix {
            message: format!(
                "add missing effects declaration to function '{}'",
                function_name
            ),
            replacement: Some(replacement),
            start: Some(insert),
            end: Some(insert),
        })
    }

    fn find_effect_clause(signature: &str) -> Option<(usize, usize)> {
        let bytes = signature.as_bytes();
        let mut start = 0usize;

        while start < bytes.len() {
            let found = signature[start..].find("effects")?;
            let idx = start + found;
            let end_keyword = idx + "effects".len();
            let before_ok = idx == 0 || !is_ident_byte(bytes[idx - 1]);
            let after_ok = end_keyword >= bytes.len() || !is_ident_byte(bytes[end_keyword]);
            if !before_ok || !after_ok {
                start = idx + 1;
                continue;
            }

            let mut cursor = end_keyword;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor >= bytes.len() || bytes[cursor] != b'{' {
                start = idx + 1;
                continue;
            }
            let mut close = cursor + 1;
            while close < bytes.len() && bytes[close] != b'}' {
                close += 1;
            }
            if close >= bytes.len() {
                return None;
            }
            return Some((idx, close + 1));
        }
        None
    }

    fn find_keyword(signature: &str, keyword: &str) -> Option<usize> {
        let bytes = signature.as_bytes();
        let keyword_len = keyword.len();
        let mut start = 0usize;

        while start < bytes.len() {
            let found = signature[start..].find(keyword)?;
            let idx = start + found;
            let end = idx + keyword_len;
            let before_ok = idx == 0 || !is_ident_byte(bytes[idx - 1]);
            let after_ok = end >= bytes.len() || !is_ident_byte(bytes[end]);
            if before_ok && after_ok {
                return Some(idx);
            }
            start = idx + 1;
        }
        None
    }

    fn check_mutability_and_borrows(&mut self, func: &ir::Function) {
        let mut mutability = BTreeMap::new();
        let mut binding_types = BTreeMap::new();
        for param in &func.params {
            mutability.insert(param.name.clone(), false);
            let ty = self
                .types
                .get(&param.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            binding_types.insert(param.name.clone(), ty);
        }
        let mut borrow_state = BorrowState::default();
        self.check_borrow_block(
            &func.body,
            &mut mutability,
            &mut borrow_state,
            &mut binding_types,
        );
    }

    fn check_resource_protocols(&mut self, func: &ir::Function) {
        let mut state = ResourceStateMap::new();
        self.check_resource_protocol_block(&func.body, &mut state);
    }

    fn check_resource_protocol_block(&mut self, block: &ir::Block, state: &mut ResourceStateMap) {
        for stmt in &block.stmts {
            match stmt {
                ir::Stmt::Let { name, expr, .. } => {
                    self.check_resource_protocol_expr(expr, state);
                    clear_resource_state_for_var(name, state);
                }
                ir::Stmt::Assign { target, expr, .. } => {
                    self.check_resource_protocol_expr(expr, state);
                    clear_resource_state_for_var(target, state);
                }
                ir::Stmt::Expr { expr, .. } => self.check_resource_protocol_expr(expr, state),
                ir::Stmt::Return {
                    expr: Some(expr), ..
                }
                | ir::Stmt::Assert { expr, .. } => self.check_resource_protocol_expr(expr, state),
                ir::Stmt::Return { expr: None, .. } => {}
            }
        }

        if let Some(tail) = &block.tail {
            self.check_resource_protocol_expr(tail, state);
        }
    }

    fn check_resource_protocol_expr(&mut self, expr: &ir::Expr, state: &mut ResourceStateMap) {
        self.check_resource_protocol_expr_mode(expr, state, false);
    }

    fn check_resource_protocol_expr_mode(
        &mut self,
        expr: &ir::Expr,
        state: &mut ResourceStateMap,
        allow_closed_use: bool,
    ) {
        match &expr.kind {
            ir::ExprKind::Call { callee, args } => {
                self.check_resource_protocol_expr_mode(callee, state, false);
                for arg in args {
                    self.check_resource_protocol_expr_mode(arg, state, false);
                }
                self.check_resource_protocol_call(callee, args, expr.span, state, allow_closed_use);
            }
            ir::ExprKind::Closure { body, .. } => {
                let mut closure_state = state.clone();
                self.check_resource_protocol_block(body, &mut closure_state);
            }
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                self.check_resource_protocol_expr_mode(cond, state, false);
                let mut then_state = state.clone();
                self.check_resource_protocol_block(then_block, &mut then_state);
                let mut else_state = state.clone();
                self.check_resource_protocol_block(else_block, &mut else_state);
            }
            ir::ExprKind::While { cond, body } => {
                self.check_resource_protocol_expr_mode(cond, state, false);
                let mut loop_state = state.clone();
                self.check_resource_protocol_block(body, &mut loop_state);
            }
            ir::ExprKind::Loop { body } => {
                let mut loop_state = state.clone();
                self.check_resource_protocol_block(body, &mut loop_state);
            }
            ir::ExprKind::Break { expr } => {
                if let Some(expr) = expr {
                    self.check_resource_protocol_expr_mode(expr, state, false);
                }
            }
            ir::ExprKind::Continue => {}
            ir::ExprKind::Match {
                expr: scrutinee,
                arms,
            } => {
                // `match call(...) { ... }` explicitly handles `Result` branches, including
                // expected runtime closed/cancelled outcomes.
                self.check_resource_protocol_expr_mode(scrutinee, state, true);
                for arm in arms {
                    let mut arm_state = state.clone();
                    if let Some(guard) = &arm.guard {
                        self.check_resource_protocol_expr_mode(guard, &mut arm_state, false);
                    }
                    self.check_resource_protocol_expr_mode(&arm.body, &mut arm_state, false);
                }
            }
            ir::ExprKind::UnsafeBlock { block } => {
                let mut block_state = state.clone();
                self.check_resource_protocol_block(block, &mut block_state);
            }
            ir::ExprKind::Binary { lhs, rhs, .. } => {
                self.check_resource_protocol_expr_mode(lhs, state, false);
                self.check_resource_protocol_expr_mode(rhs, state, false);
            }
            ir::ExprKind::Unary { expr, .. }
            | ir::ExprKind::Borrow { expr, .. }
            | ir::ExprKind::Await { expr }
            | ir::ExprKind::Try { expr } => {
                self.check_resource_protocol_expr_mode(expr, state, false);
            }
            ir::ExprKind::StructInit { fields, .. } => {
                for (_, value, _) in fields {
                    self.check_resource_protocol_expr_mode(value, state, false);
                }
            }
            ir::ExprKind::FieldAccess { base, .. } => {
                self.check_resource_protocol_expr_mode(base, state, false);
            }
            ir::ExprKind::Int(_)
            | ir::ExprKind::Float(_)
            | ir::ExprKind::Bool(_)
            | ir::ExprKind::String(_)
            | ir::ExprKind::Unit
            | ir::ExprKind::Var(_) => {}
        }
    }

    fn check_resource_protocol_call(
        &mut self,
        callee: &ir::Expr,
        args: &[ir::Expr],
        span: crate::span::Span,
        state: &mut ResourceStateMap,
        allow_closed_use: bool,
    ) {
        let Some(name) = self.resolve_concurrent_protocol_call(callee) else {
            return;
        };
        let Some(op) = concurrent_protocol_op(&name) else {
            return;
        };
        let Some(first_arg) = args.first() else {
            return;
        };
        let ir::ExprKind::Var(var_name) = &first_arg.kind else {
            return;
        };
        let key = (var_name.clone(), op.kind);
        if let Some(previous) = state.get(&key).copied() {
            if !allow_closed_use {
                let mut diag = Diagnostic::error(
                    "E2006",
                    format!(
                        "resource protocol violation: '{}' called on closed {} '{}'",
                        op.api,
                        op.kind.as_str(),
                        var_name
                    ),
                    self.file,
                    span,
                )
                .with_help(format!(
                    "create a new {} before calling '{}' again",
                    op.kind.as_str(),
                    op.api
                ));
                diag.spans.push(DiagnosticSpan {
                    file: self.file.to_string(),
                    start: previous.closed_at.start,
                    end: previous.closed_at.end,
                    label: Some("resource was closed here".to_string()),
                });
                self.diagnostics.push(diag);
            }
            return;
        }
        if op.terminal {
            state.insert(key, ResourceState { closed_at: span });
        }
    }

    fn resolve_concurrent_protocol_call(&self, callee: &ir::Expr) -> Option<String> {
        let call_path = self.extract_callee_path(callee)?;
        let name = call_path.last()?.clone();
        let op = concurrent_protocol_op(&name)?;
        let sig = self.functions.get(&name)?;
        if sig.params.first().map(String::as_str) == Some(op.kind.as_str()) {
            Some(name)
        } else {
            None
        }
    }

    fn check_borrow_block(
        &mut self,
        block: &ir::Block,
        mutability: &mut BTreeMap<String, bool>,
        state: &mut BorrowState,
        binding_types: &mut BTreeMap<String, String>,
    ) {
        let mut introduced_bindings: Vec<(String, Option<bool>)> = Vec::new();
        let mut introduced_move_states: Vec<(String, Option<crate::span::Span>)> = Vec::new();
        let mut introduced_type_states: Vec<(String, Option<String>)> = Vec::new();
        let mut introduced_persistent_borrows: Vec<ir::SymbolId> = Vec::new();

        for stmt in &block.stmts {
            match stmt {
                ir::Stmt::Let {
                    symbol,
                    name,
                    mutable,
                    ty,
                    expr,
                    ..
                } => {
                    if let Some((target, is_mutable, borrow_span)) =
                        self.extract_direct_borrow(expr)
                    {
                        if self.acquire_borrow(
                            &target,
                            is_mutable,
                            borrow_span,
                            Some(*symbol),
                            mutability,
                            state,
                        ) {
                            introduced_persistent_borrows.push(*symbol);
                        }
                    } else {
                        let temp_borrows =
                            self.check_borrow_expr(expr, mutability, state, binding_types);
                        self.release_temp_borrows(&temp_borrows, state);
                    }
                    self.track_move_in_let_binding(name, expr, state, mutability, binding_types);
                    let previous = mutability.insert(name.clone(), *mutable);
                    introduced_bindings.push((name.clone(), previous));
                    let inferred_type = ty
                        .and_then(|id| self.types.get(&id).cloned())
                        .or_else(|| self.infer_binding_type(expr, binding_types))
                        .unwrap_or_else(|| "<?>".to_string());
                    let previous_type = binding_types.insert(name.clone(), inferred_type);
                    introduced_type_states.push((name.clone(), previous_type));
                    let previous_move = state.moved_by_binding.remove(name);
                    introduced_move_states.push((name.clone(), previous_move));
                }
                ir::Stmt::Assign { target, expr, span } => {
                    let temp_borrows =
                        self.check_borrow_expr(expr, mutability, state, binding_types);
                    self.release_temp_borrows(&temp_borrows, state);

                    let Some(is_mutable_binding) = mutability.get(target).copied() else {
                        continue;
                    };
                    if !is_mutable_binding {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1266",
                                format!(
                                    "cannot assign to immutable binding '{}'; declare it as `let mut {}`",
                                    target, target
                                ),
                                self.file,
                                *span,
                            )
                            .with_help("use `let mut` for bindings that are reassigned"),
                        );
                    }
                    if let Some((conflict_target, conflict)) =
                        self.find_first_overlapping_borrow(target, state)
                    {
                        let mut diag = Diagnostic::error(
                            "E1265",
                            format!(
                                "cannot assign to '{}' while it is actively borrowed",
                                target
                            ),
                            self.file,
                            *span,
                        )
                        .with_label("assignment while borrowed");
                        diag.spans.push(DiagnosticSpan {
                            file: self.file.to_string(),
                            start: conflict.span.start,
                            end: conflict.span.end,
                            label: Some(format!(
                                "active borrow of '{}' starts here",
                                conflict_target
                            )),
                        });
                        diag.help.push(
                            "release or narrow the borrow scope before mutating this binding"
                                .to_string(),
                        );
                        self.diagnostics.push(diag);
                    }
                    state.moved_by_binding.remove(target);
                    if let Some(inferred) = self.infer_binding_type(expr, binding_types) {
                        binding_types.insert(target.clone(), inferred);
                    }
                }
                ir::Stmt::Expr { expr, .. } => {
                    let temp_borrows =
                        self.check_borrow_expr(expr, mutability, state, binding_types);
                    self.release_temp_borrows(&temp_borrows, state);
                }
                ir::Stmt::Return { expr, .. } => {
                    if let Some(expr) = expr {
                        let temp_borrows =
                            self.check_borrow_expr(expr, mutability, state, binding_types);
                        self.release_temp_borrows(&temp_borrows, state);
                    }
                }
                ir::Stmt::Assert { expr, .. } => {
                    let temp_borrows =
                        self.check_borrow_expr(expr, mutability, state, binding_types);
                    self.release_temp_borrows(&temp_borrows, state);
                }
            }
        }

        if let Some(tail) = &block.tail {
            let temp_borrows = self.check_borrow_expr(tail, mutability, state, binding_types);
            self.release_temp_borrows(&temp_borrows, state);
        }

        let drop_order = block.lexical_drop_order();
        let introduced = introduced_persistent_borrows
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let mut release_order = drop_order
            .iter()
            .copied()
            .filter(|symbol| introduced.contains(symbol))
            .collect::<Vec<_>>();
        if release_order.len() != introduced.len() {
            for symbol in &introduced_persistent_borrows {
                if !release_order.contains(symbol) {
                    release_order.push(*symbol);
                }
            }
        }
        for owner in release_order.drain(..).rev() {
            self.release_persistent_borrow(owner, state);
        }
        for (name, previous) in introduced_bindings.into_iter().rev() {
            if let Some(previous) = previous {
                mutability.insert(name, previous);
            } else {
                mutability.remove(&name);
            }
        }
        for (name, previous_move) in introduced_move_states.into_iter().rev() {
            if let Some(previous_move) = previous_move {
                state.moved_by_binding.insert(name, previous_move);
            } else {
                state.moved_by_binding.remove(&name);
            }
        }
        for (name, previous_type) in introduced_type_states.into_iter().rev() {
            if let Some(previous_type) = previous_type {
                binding_types.insert(name, previous_type);
            } else {
                binding_types.remove(&name);
            }
        }
    }

    fn check_borrow_expr(
        &mut self,
        expr: &ir::Expr,
        mutability: &mut BTreeMap<String, bool>,
        state: &mut BorrowState,
        binding_types: &mut BTreeMap<String, String>,
    ) -> Vec<TempBorrow> {
        match &expr.kind {
            ir::ExprKind::Borrow {
                mutable,
                expr: inner,
            } => {
                if let Some(target) = self.borrow_target_path(inner) {
                    if self.acquire_borrow(&target, *mutable, expr.span, None, mutability, state) {
                        return vec![TempBorrow {
                            target,
                            mutable: *mutable,
                            span: expr.span,
                        }];
                    }
                    return Vec::new();
                }
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1268",
                        "borrow target must be a local variable or field path",
                        self.file,
                        expr.span,
                    )
                    .with_help("use `&name`, `&mut name`, or `&name.field` on a local binding"),
                );
                self.check_borrow_expr(inner, mutability, state, binding_types)
            }
            ir::ExprKind::Call { callee, args } => {
                let mut borrows = self.check_borrow_expr(callee, mutability, state, binding_types);
                for arg in args {
                    borrows.extend(self.check_borrow_expr(arg, mutability, state, binding_types));
                }
                borrows
            }
            ir::ExprKind::Closure { body, .. } => {
                let mut closure_mutability = mutability.clone();
                let mut closure_state = state.clone();
                let mut closure_types = binding_types.clone();
                self.check_borrow_block(
                    body,
                    &mut closure_mutability,
                    &mut closure_state,
                    &mut closure_types,
                );
                Vec::new()
            }
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let cond_borrows = self.check_borrow_expr(cond, mutability, state, binding_types);
                self.release_temp_borrows(&cond_borrows, state);

                let mut then_mutability = mutability.clone();
                let mut then_state = state.clone();
                let mut then_types = binding_types.clone();
                self.check_borrow_block(
                    then_block,
                    &mut then_mutability,
                    &mut then_state,
                    &mut then_types,
                );

                let mut else_mutability = mutability.clone();
                let mut else_state = state.clone();
                let mut else_types = binding_types.clone();
                self.check_borrow_block(
                    else_block,
                    &mut else_mutability,
                    &mut else_state,
                    &mut else_types,
                );
                Vec::new()
            }
            ir::ExprKind::While { cond, body } => {
                let cond_borrows = self.check_borrow_expr(cond, mutability, state, binding_types);
                self.release_temp_borrows(&cond_borrows, state);

                let mut loop_mutability = mutability.clone();
                let mut loop_state = state.clone();
                let mut loop_types = binding_types.clone();
                self.check_borrow_block(
                    body,
                    &mut loop_mutability,
                    &mut loop_state,
                    &mut loop_types,
                );
                Vec::new()
            }
            ir::ExprKind::Loop { body } => {
                let mut loop_mutability = mutability.clone();
                let mut loop_state = state.clone();
                let mut loop_types = binding_types.clone();
                self.check_borrow_block(
                    body,
                    &mut loop_mutability,
                    &mut loop_state,
                    &mut loop_types,
                );
                Vec::new()
            }
            ir::ExprKind::Break { expr } => {
                if let Some(expr) = expr {
                    self.check_borrow_expr(expr, mutability, state, binding_types)
                } else {
                    Vec::new()
                }
            }
            ir::ExprKind::Continue => Vec::new(),
            ir::ExprKind::Match {
                expr: scrutinee,
                arms,
            } => {
                let scrutinee_borrows =
                    self.check_borrow_expr(scrutinee, mutability, state, binding_types);
                self.release_temp_borrows(&scrutinee_borrows, state);
                for arm in arms {
                    let mut arm_mutability = mutability.clone();
                    let mut arm_state = state.clone();
                    let mut arm_types = binding_types.clone();
                    let arm_borrows = self.check_borrow_expr(
                        &arm.body,
                        &mut arm_mutability,
                        &mut arm_state,
                        &mut arm_types,
                    );
                    self.release_temp_borrows(&arm_borrows, &mut arm_state);
                }
                Vec::new()
            }
            ir::ExprKind::UnsafeBlock { block } => {
                let mut block_mutability = mutability.clone();
                let mut block_state = state.clone();
                let mut block_types = binding_types.clone();
                self.check_borrow_block(
                    block,
                    &mut block_mutability,
                    &mut block_state,
                    &mut block_types,
                );
                Vec::new()
            }
            ir::ExprKind::Binary { lhs, rhs, .. } => {
                let mut borrows = self.check_borrow_expr(lhs, mutability, state, binding_types);
                borrows.extend(self.check_borrow_expr(rhs, mutability, state, binding_types));
                borrows
            }
            ir::ExprKind::Unary { expr, .. }
            | ir::ExprKind::Await { expr }
            | ir::ExprKind::Try { expr } => {
                self.check_borrow_expr(expr, mutability, state, binding_types)
            }
            ir::ExprKind::StructInit { fields, .. } => {
                let mut borrows = Vec::new();
                for (_, value, _) in fields {
                    borrows.extend(self.check_borrow_expr(value, mutability, state, binding_types));
                }
                borrows
            }
            ir::ExprKind::FieldAccess { base, .. } => {
                self.check_borrow_expr(base, mutability, state, binding_types)
            }
            ir::ExprKind::Int(_)
            | ir::ExprKind::Float(_)
            | ir::ExprKind::Bool(_)
            | ir::ExprKind::String(_)
            | ir::ExprKind::Unit => Vec::new(),
            ir::ExprKind::Var(name) => {
                self.check_use_after_move(name, expr.span, state);
                Vec::new()
            }
        }
    }

    fn release_temp_borrows(&self, borrows: &[TempBorrow], state: &mut BorrowState) {
        for borrow in borrows {
            Self::release_borrow(&borrow.target, borrow.mutable, borrow.span, None, state);
        }
    }

    fn extract_direct_borrow(&self, expr: &ir::Expr) -> Option<(String, bool, crate::span::Span)> {
        let ir::ExprKind::Borrow {
            mutable,
            expr: inner,
        } = &expr.kind
        else {
            return None;
        };
        let target = self.borrow_target_path(inner)?;
        Some((target, *mutable, expr.span))
    }

    fn acquire_borrow(
        &mut self,
        target: &str,
        mutable: bool,
        span: crate::span::Span,
        owner: Option<ir::SymbolId>,
        mutability: &BTreeMap<String, bool>,
        state: &mut BorrowState,
    ) -> bool {
        let root = binding_root(target);
        if state.moved_by_binding.contains_key(root) {
            self.check_use_after_move(root, span, state);
            return false;
        }
        if mutable && !mutability.get(root).copied().unwrap_or(false) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1267",
                    format!("cannot take mutable borrow of immutable binding '{}'", root),
                    self.file,
                    span,
                )
                .with_help(format!("declare `{}` as `let mut {}`", root, root)),
            );
            return false;
        }

        if mutable {
            if let Some((conflict_target, conflict)) =
                self.find_first_overlapping_borrow(target, state)
            {
                let mut diag = Diagnostic::error(
                    "E1263",
                    format!(
                        "cannot take mutable borrow of '{}' because it is already borrowed",
                        target
                    ),
                    self.file,
                    span,
                )
                .with_label("new mutable borrow");
                diag.spans.push(DiagnosticSpan {
                    file: self.file.to_string(),
                    start: conflict.span.start,
                    end: conflict.span.end,
                    label: Some(format!(
                        "conflicting borrow of '{}' starts here",
                        conflict_target
                    )),
                });
                diag.help
                    .push("release the previous borrow before taking a mutable borrow".to_string());
                self.diagnostics.push(diag);
                return false;
            }
        } else if let Some((conflict_target, conflict)) =
            self.find_first_overlapping_mutable_borrow(target, state)
        {
            let mut diag = Diagnostic::error(
                "E1264",
                format!(
                    "cannot take immutable borrow of '{}' while a mutable borrow is active",
                    target
                ),
                self.file,
                span,
            )
            .with_label("new immutable borrow");
            diag.spans.push(DiagnosticSpan {
                file: self.file.to_string(),
                start: conflict.span.start,
                end: conflict.span.end,
                label: Some(format!(
                    "active mutable borrow of '{}' starts here",
                    conflict_target
                )),
            });
            diag.help
                .push("end the mutable borrow before taking shared borrows".to_string());
            self.diagnostics.push(diag);
            return false;
        }

        state
            .active_by_target
            .entry(target.to_string())
            .or_default()
            .push(ActiveBorrow {
                mutable,
                span,
                owner,
            });
        if let Some(owner) = owner {
            state.persistent_by_owner.insert(
                owner,
                PersistentBorrow {
                    target: target.to_string(),
                    mutable,
                    span,
                },
            );
        }
        true
    }

    fn release_persistent_borrow(&self, owner: ir::SymbolId, state: &mut BorrowState) {
        let Some(binding) = state.persistent_by_owner.remove(&owner) else {
            return;
        };
        Self::release_borrow(
            &binding.target,
            binding.mutable,
            binding.span,
            Some(owner),
            state,
        );
    }

    fn release_borrow(
        target: &str,
        mutable: bool,
        span: crate::span::Span,
        owner: Option<ir::SymbolId>,
        state: &mut BorrowState,
    ) {
        let mut remove_target = false;
        if let Some(list) = state.active_by_target.get_mut(target) {
            if let Some(index) = list.iter().position(|borrow| {
                borrow.mutable == mutable && borrow.span == span && borrow.owner == owner
            }) {
                list.remove(index);
            }
            remove_target = list.is_empty();
        }
        if remove_target {
            state.active_by_target.remove(target);
        }
    }

    fn borrow_target_path(&self, expr: &ir::Expr) -> Option<String> {
        match &expr.kind {
            ir::ExprKind::Var(name) => Some(name.clone()),
            ir::ExprKind::FieldAccess { base, field } => {
                let base_path = self.borrow_target_path(base)?;
                Some(format!("{base_path}.{field}"))
            }
            _ => None,
        }
    }

    fn find_first_overlapping_borrow(
        &self,
        target: &str,
        state: &BorrowState,
    ) -> Option<(String, ActiveBorrow)> {
        for (borrow_target, borrows) in &state.active_by_target {
            if !targets_overlap(target, borrow_target) {
                continue;
            }
            if let Some(borrow) = borrows.first() {
                return Some((borrow_target.clone(), borrow.clone()));
            }
        }
        None
    }

    fn find_first_overlapping_mutable_borrow(
        &self,
        target: &str,
        state: &BorrowState,
    ) -> Option<(String, ActiveBorrow)> {
        for (borrow_target, borrows) in &state.active_by_target {
            if !targets_overlap(target, borrow_target) {
                continue;
            }
            if let Some(borrow) = borrows.iter().find(|borrow| borrow.mutable) {
                return Some((borrow_target.clone(), borrow.clone()));
            }
        }
        None
    }

    fn check_use_after_move(
        &mut self,
        target: &str,
        span: crate::span::Span,
        state: &BorrowState,
    ) -> bool {
        let root = binding_root(target);
        let Some(moved_at) = state.moved_by_binding.get(root) else {
            return true;
        };

        let mut diag = Diagnostic::error(
            "E1270",
            format!("use of moved value '{}'", root),
            self.file,
            span,
        )
        .with_help("reinitialize the binding before using it again");
        diag.spans.push(DiagnosticSpan {
            file: self.file.to_string(),
            start: moved_at.start,
            end: moved_at.end,
            label: Some("value moved here".to_string()),
        });
        self.diagnostics.push(diag);
        false
    }

    fn infer_binding_type(
        &self,
        expr: &ir::Expr,
        binding_types: &BTreeMap<String, String>,
    ) -> Option<String> {
        match &expr.kind {
            ir::ExprKind::Var(name) => binding_types.get(name).cloned(),
            ir::ExprKind::StructInit { name, .. } => Some(name.clone()),
            ir::ExprKind::Int(_) => Some("Int".to_string()),
            ir::ExprKind::Float(_) => Some("Float".to_string()),
            ir::ExprKind::Bool(_) => Some("Bool".to_string()),
            ir::ExprKind::String(_) => Some("String".to_string()),
            ir::ExprKind::Unit => Some("()".to_string()),
            _ => None,
        }
    }

    fn track_move_in_let_binding(
        &mut self,
        binding_name: &str,
        expr: &ir::Expr,
        state: &mut BorrowState,
        mutability: &BTreeMap<String, bool>,
        binding_types: &BTreeMap<String, String>,
    ) {
        let ir::ExprKind::Var(source) = &expr.kind else {
            return;
        };
        if source == binding_name || !mutability.contains_key(source) {
            return;
        }
        let Some(source_ty) = binding_types.get(source) else {
            return;
        };
        let source_base = base_type_name(source_ty);
        let movable = self.resolution.structs.contains_key(source_base)
            || self.resolution.enums.contains_key(source_base);
        if !movable {
            return;
        }

        if let Some((borrow_target, borrow)) = self.find_first_overlapping_borrow(source, state) {
            let mut diag = Diagnostic::error(
                "E1271",
                format!(
                    "cannot move '{}' while it is borrowed as '{}'",
                    source, borrow_target
                ),
                self.file,
                expr.span,
            )
            .with_help("end the borrow before moving this value");
            diag.spans.push(DiagnosticSpan {
                file: self.file.to_string(),
                start: borrow.span.start,
                end: borrow.span.end,
                label: Some("active borrow starts here".to_string()),
            });
            self.diagnostics.push(diag);
            return;
        }
        state.moved_by_binding.insert(source.clone(), expr.span);
    }

    fn check_struct_invariant(&mut self, strukt: &ir::StructDef) {
        for field in &strukt.fields {
            let ty = self
                .types
                .get(&field.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            if contains_unresolved_type(&ty) {
                let key = (strukt.name.clone(), field.name.clone());
                self.struct_field_holes
                    .entry(key.clone())
                    .or_insert(DeferredHole {
                        span: field.span,
                        context: format!("struct field '{}.{}'", strukt.name, field.name),
                    });
                self.struct_field_inferred
                    .entry(key)
                    .or_insert_with(|| ty.clone());
            }
            self.check_generic_arity(&ty, field.span);
        }

        if let Some(inv) = &strukt.invariant {
            let mut locals = BTreeMap::new();
            for field in &strukt.fields {
                locals.insert(
                    field.name.clone(),
                    self.types
                        .get(&field.ty)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string()),
                );
            }
            let mut ctx = ExprContext::default();
            let ty = self.check_expr(inv, &mut locals, &BTreeSet::new(), &mut ctx, true);
            if self.normalize_type(&ty) != "Bool" {
                self.diagnostics.push(Diagnostic::error(
                    "E1203",
                    format!("invariant for struct '{}' must be Bool", strukt.name),
                    self.file,
                    inv.span,
                ));
            }
        }
    }

    fn check_enum_definition(&mut self, enm: &ir::EnumDef) {
        for variant in &enm.variants {
            if let Some(payload) = variant.payload {
                let ty = self
                    .types
                    .get(&payload)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string());
                self.check_generic_arity(&ty, variant.span);
            }
        }
    }

    fn note_typed_hole(&mut self, span: crate::span::Span, inferred: &str, context: &str) {
        let inferred = if inferred.is_empty() {
            "<?>".to_string()
        } else {
            inferred.to_string()
        };
        self.typed_holes.push(TypedHole {
            file: self.file.to_string(),
            span,
            inferred: inferred.clone(),
            context: context.to_string(),
        });
        self.diagnostics.push(
            Diagnostic::warning(
                "E6003",
                format!("typed hole in {context} inferred as '{inferred}'"),
                self.file,
                span,
            )
            .with_help("replace `_` with the inferred type when ready"),
        );
    }

    fn observe_fn_param_hole(&mut self, function: &str, index: usize, observed: &str) {
        let key = (function.to_string(), index);
        let merged = self
            .fn_param_inferred
            .get(&key)
            .map(|prev| merge_types(prev, observed))
            .unwrap_or_else(|| observed.to_string());
        self.fn_param_inferred.insert(key, merged);
    }

    fn observe_fn_return_hole(&mut self, function: &str, observed: &str) {
        let key = function.to_string();
        let merged = self
            .fn_return_inferred
            .get(&key)
            .map(|prev| merge_types(prev, observed))
            .unwrap_or_else(|| observed.to_string());
        self.fn_return_inferred.insert(key, merged);
    }

    fn observe_struct_field_hole(&mut self, strukt: &str, field: &str, observed: &str) {
        let key = (strukt.to_string(), field.to_string());
        let merged = self
            .struct_field_inferred
            .get(&key)
            .map(|prev| merge_types(prev, observed))
            .unwrap_or_else(|| observed.to_string());
        self.struct_field_inferred.insert(key, merged);
    }

    fn flush_deferred_holes(&mut self) {
        let param_holes = self
            .fn_param_holes
            .iter()
            .map(|(key, hole)| (key.clone(), hole.clone()))
            .collect::<Vec<_>>();
        for (key, hole) in param_holes {
            let inferred = self
                .fn_param_inferred
                .get(&key)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            self.note_typed_hole(hole.span, &inferred, &hole.context);
        }

        let return_holes = self
            .fn_return_holes
            .iter()
            .map(|(name, hole)| (name.clone(), hole.clone()))
            .collect::<Vec<_>>();
        for (name, hole) in return_holes {
            let inferred = self
                .fn_return_inferred
                .get(&name)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            self.note_typed_hole(hole.span, &inferred, &hole.context);
        }

        let struct_field_holes = self
            .struct_field_holes
            .iter()
            .map(|(key, hole)| (key.clone(), hole.clone()))
            .collect::<Vec<_>>();
        for (key, hole) in struct_field_holes {
            let inferred = self
                .struct_field_inferred
                .get(&key)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            self.note_typed_hole(hole.span, &inferred, &hole.context);
        }
    }

    fn refine_variable_with_expected(
        &mut self,
        expr: &ir::Expr,
        expected_ty: &str,
        locals: &mut BTreeMap<String, String>,
    ) -> Option<String> {
        let ir::ExprKind::Var(name) = &expr.kind else {
            return None;
        };
        let local_ty = locals.get(name).cloned()?;
        let expected_norm = self.normalize_type(expected_ty);
        if !contains_unresolved_type(&local_ty)
            || contains_unresolved_type(&expected_norm)
            || contains_symbolic_generic_type(&expected_norm)
            || !self.types_compatible(&expected_norm, &local_ty)
        {
            return None;
        }
        let refined = self.merge_compatible_types(&local_ty, &expected_norm);
        if contains_unresolved_type(&refined) {
            return None;
        }
        locals.insert(name.clone(), refined.clone());
        if let Some(function_name) = self.current_function.clone() {
            if let Some(index) = self.current_param_positions.get(name).copied() {
                self.observe_fn_param_hole(&function_name, index, &refined);
            }
        }
        Some(refined)
    }

    fn check_block(
        &mut self,
        block: &ir::Block,
        locals: &mut BTreeMap<String, String>,
        ret_type: &str,
        allowed_effects: &BTreeSet<String>,
        ctx: &mut ExprContext,
        contract_mode: bool,
    ) -> String {
        let mut scope = locals.clone();
        let mut pending_unresolved_lets: Vec<(String, crate::span::Span)> = Vec::new();

        for stmt in &block.stmts {
            match stmt {
                ir::Stmt::Let {
                    name,
                    ty,
                    expr,
                    span,
                    ..
                } => {
                    if let Some(ann) = ty {
                        let ann_ty = self
                            .types
                            .get(ann)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string());
                        self.check_generic_arity(&ann_ty, *span);
                        let expr_ty = self.check_expr_with_expected(
                            expr,
                            &mut scope,
                            allowed_effects,
                            ctx,
                            contract_mode,
                            Some(&ann_ty),
                        );
                        if !self.types_compatible(&ann_ty, &expr_ty) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1204",
                                    format!(
                                        "let binding '{}' expected type '{}', found '{}'",
                                        name, ann_ty, expr_ty
                                    ),
                                    self.file,
                                    *span,
                                )
                                .with_help("make the initializer type match the annotation"),
                            );
                        }
                        let binding_ty = if contains_unresolved_type(&ann_ty) {
                            let inferred_ty = self.merge_compatible_types(&ann_ty, &expr_ty);
                            self.note_typed_hole(
                                *span,
                                &inferred_ty,
                                &format!("let binding '{}'", name),
                            );
                            inferred_ty
                        } else {
                            ann_ty.clone()
                        };
                        scope.insert(name.clone(), binding_ty);
                    } else {
                        let expr_ty =
                            self.check_expr(expr, &mut scope, allowed_effects, ctx, contract_mode);
                        if contains_unresolved_type(&expr_ty) {
                            pending_unresolved_lets.push((name.clone(), *span));
                        }
                        scope.insert(name.clone(), expr_ty);
                    }
                }
                ir::Stmt::Assign { target, expr, span } => {
                    let expr_ty =
                        self.check_expr(expr, &mut scope, allowed_effects, ctx, contract_mode);
                    let Some(target_ty) = scope.get(target).cloned() else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1208",
                            format!("unknown symbol '{}'", target),
                            self.file,
                            *span,
                        ));
                        continue;
                    };
                    if !self.types_compatible(&target_ty, &expr_ty) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1269",
                                format!(
                                    "assignment to '{}' expects '{}', found '{}'",
                                    target, target_ty, expr_ty
                                ),
                                self.file,
                                *span,
                            )
                            .with_help("ensure assignment value matches the binding type"),
                        );
                    }
                }
                ir::Stmt::Expr { expr, .. } => {
                    self.check_expr(expr, &mut scope, allowed_effects, ctx, contract_mode);
                }
                ir::Stmt::Return { expr, span } => {
                    let ty = if let Some(expr) = expr {
                        self.check_expr_with_expected(
                            expr,
                            &mut scope,
                            allowed_effects,
                            ctx,
                            contract_mode,
                            Some(ret_type),
                        )
                    } else {
                        "()".to_string()
                    };
                    if !self.types_compatible(ret_type, &ty) {
                        self.diagnostics.push(Diagnostic::error(
                            "E1205",
                            format!(
                                "return type '{}' does not match function return '{}'",
                                ty, ret_type
                            ),
                            self.file,
                            *span,
                        ));
                    }
                }
                ir::Stmt::Assert { expr, span, .. } => {
                    let ty = self.check_expr(expr, &mut scope, allowed_effects, ctx, contract_mode);
                    if self.normalize_type(&ty) != "Bool" {
                        self.diagnostics.push(Diagnostic::error(
                            "E1206",
                            "assert expression must be Bool",
                            self.file,
                            *span,
                        ));
                    }
                }
            }
        }

        let result_ty = if let Some(tail) = &block.tail {
            self.check_expr_with_expected(
                tail,
                &mut scope,
                allowed_effects,
                ctx,
                contract_mode,
                Some(ret_type),
            )
        } else {
            "()".to_string()
        };

        for (name, span) in pending_unresolved_lets {
            if let Some(ty) = scope.get(&name) {
                if contains_unresolved_type(ty) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1204",
                            format!(
                                "cannot infer concrete type for let binding '{}' (inferred '{}')",
                                name, ty
                            ),
                            self.file,
                            span,
                        )
                        .with_help("add an explicit type annotation on the binding"),
                    );
                }
            }
        }

        result_ty
    }

    fn check_expr(
        &mut self,
        expr: &ir::Expr,
        locals: &mut BTreeMap<String, String>,
        allowed_effects: &BTreeSet<String>,
        ctx: &mut ExprContext,
        contract_mode: bool,
    ) -> String {
        self.check_expr_with_expected(expr, locals, allowed_effects, ctx, contract_mode, None)
    }

    fn check_expr_with_expected(
        &mut self,
        expr: &ir::Expr,
        locals: &mut BTreeMap<String, String>,
        allowed_effects: &BTreeSet<String>,
        ctx: &mut ExprContext,
        contract_mode: bool,
        expected_ty: Option<&str>,
    ) -> String {
        match &expr.kind {
            ir::ExprKind::Int(_) => "Int".to_string(),
            ir::ExprKind::Float(_) => "Float".to_string(),
            ir::ExprKind::Bool(_) => "Bool".to_string(),
            ir::ExprKind::String(_) => "String".to_string(),
            ir::ExprKind::Unit => "()".to_string(),
            ir::ExprKind::Var(name) => {
                if let Some(local_ty) = locals.get(name).cloned() {
                    if let Some(expected) = expected_ty {
                        if let Some(refined) =
                            self.refine_variable_with_expected(expr, expected, locals)
                        {
                            return refined;
                        }
                    }
                    return local_ty;
                }
                if let Some(ty) = self.const_types.get(name) {
                    return ty.clone();
                }
                if let Some(sig) = self.functions.get(name) {
                    if self
                        .resolution
                        .function_modules
                        .get(name)
                        .map(|mods| mods.len() > 1)
                        .unwrap_or(false)
                    {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E2104",
                                format!("ambiguous callable '{}' from multiple modules", name),
                                self.file,
                                expr.span,
                            )
                            .with_help("qualify the function or add a type annotation"),
                        );
                        return "<?>".to_string();
                    }

                    if !sig.generic_params.is_empty() {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1282",
                                format!(
                                    "generic function '{}' cannot be used as a value without specialization",
                                    name
                                ),
                                self.file,
                                expr.span,
                            )
                            .with_help("wrap it in a closure with concrete argument/return types"),
                        );
                        return "<?>".to_string();
                    }
                    if !sig.effects.is_empty() {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1283",
                                format!(
                                    "effectful function '{}' cannot be used as a first-class value",
                                    name
                                ),
                                self.file,
                                expr.span,
                            )
                            .with_help(
                                "wrap the call in a pure closure or keep a direct call site with explicit effects",
                            ),
                        );
                        return "<?>".to_string();
                    }

                    let mut ret = sig.ret.clone();
                    if sig.is_async {
                        ret = format!("Async[{ret}]");
                    }
                    return render_fn_type(&sig.params, &ret);
                }
                self.diagnostics.push(Diagnostic::error(
                    "E1208",
                    format!("unknown symbol '{}'", name),
                    self.file,
                    expr.span,
                ));
                "<?>".to_string()
            }
            ir::ExprKind::Call { callee, args } => {
                if let ir::ExprKind::FieldAccess { base, field } = &callee.kind {
                    if !self.is_module_qualified_callee(callee, locals) {
                        return self.check_method_call(
                            base,
                            field,
                            args,
                            expr,
                            locals,
                            allowed_effects,
                            ctx,
                            contract_mode,
                            expected_ty,
                        );
                    }
                }

                let Some(call_path) = self.extract_callee_path(callee) else {
                    let callee_ty =
                        self.check_expr(callee, locals, allowed_effects, ctx, contract_mode);
                    return self.check_fn_value_call(
                        &callee_ty,
                        "<expr>",
                        args,
                        expr.span,
                        locals,
                        allowed_effects,
                        ctx,
                        contract_mode,
                    );
                };
                let Some(name) = call_path.last().cloned() else {
                    self.diagnostics.push(Diagnostic::error(
                        "E1209",
                        "callee path cannot be empty",
                        self.file,
                        callee.span,
                    ));
                    return "<?>".to_string();
                };
                let qualified = call_path.len() > 1;
                let rendered_path = call_path.join(".");

                if !qualified {
                    if let Some(local_ty) = locals.get(&name).cloned() {
                        if parse_fn_type(&local_ty).is_some() {
                            return self.check_fn_value_call(
                                &local_ty,
                                &rendered_path,
                                args,
                                expr.span,
                                locals,
                                allowed_effects,
                                ctx,
                                contract_mode,
                            );
                        }
                    }
                }

                // Option / Result constructors.
                if !qualified && name == "Some" {
                    if args.len() != 1 {
                        self.diagnostics.push(Diagnostic::error(
                            "E1210",
                            "Some constructor takes exactly one argument",
                            self.file,
                            expr.span,
                        ));
                        return "<?>".to_string();
                    }
                    let inner =
                        self.check_expr(&args[0], locals, allowed_effects, ctx, contract_mode);
                    if !contains_unresolved_type(&inner) {
                        self.record_instantiation(
                            ir::GenericInstantiationKind::Enum,
                            "Option",
                            None,
                            std::slice::from_ref(&inner),
                            expr.span,
                        );
                    }
                    return format!("Option[{}]", inner);
                }
                if !qualified && name == "None" {
                    if !args.is_empty() {
                        self.diagnostics.push(Diagnostic::error(
                            "E1211",
                            "None constructor takes no arguments",
                            self.file,
                            expr.span,
                        ));
                    }
                    return "Option[<?>]".to_string();
                }
                if !qualified && name == "Ok" {
                    if args.len() != 1 {
                        self.diagnostics.push(Diagnostic::error(
                            "E1210",
                            "Ok constructor takes exactly one argument",
                            self.file,
                            expr.span,
                        ));
                        return "<?>".to_string();
                    }
                    let inner =
                        self.check_expr(&args[0], locals, allowed_effects, ctx, contract_mode);
                    return format!("Result[{}, <?>]", inner);
                }
                if !qualified && name == "Err" {
                    if args.len() != 1 {
                        self.diagnostics.push(Diagnostic::error(
                            "E1210",
                            "Err constructor takes exactly one argument",
                            self.file,
                            expr.span,
                        ));
                        return "<?>".to_string();
                    }
                    let expected_err = expected_ty
                        .and_then(|expected| self.extract_result_error_expected(expected));
                    let err = self.check_expr_with_expected(
                        &args[0],
                        locals,
                        allowed_effects,
                        ctx,
                        contract_mode,
                        expected_err.as_deref(),
                    );
                    return format!("Result[<?>, {}]", err);
                }

                let mut resolved_module: Option<String> = None;
                let resolved_name = if qualified {
                    let qualifier = &call_path[..call_path.len() - 1];
                    let Some(module) = self.resolve_qualifier_module(qualifier) else {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E2102",
                                format!(
                                    "module qualifier '{}' is not imported",
                                    qualifier.join(".")
                                ),
                                self.file,
                                callee.span,
                            )
                            .with_help("add an explicit import for that module"),
                        );
                        return "<?>".to_string();
                    };

                    if !self.resolution.imports.contains(&module) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E2102",
                                format!("module '{}' is not directly imported", module),
                                self.file,
                                callee.span,
                            )
                            .with_help(format!("add `import {};`", module)),
                        );
                        return "<?>".to_string();
                    }

                    let exported = self
                        .resolution
                        .module_functions
                        .get(&module)
                        .map(|s| s.contains(&name))
                        .unwrap_or(false);
                    if !exported {
                        self.diagnostics.push(Diagnostic::error(
                            "E1218",
                            format!("unknown callable '{}'", rendered_path),
                            self.file,
                            callee.span,
                        ));
                        return "<?>".to_string();
                    }

                    resolved_module = Some(module.clone());
                    name.clone()
                } else {
                    if self.enforce_import_visibility
                        && !self.resolution.visible_functions.contains(&name)
                    {
                        if let Some(modules) = self.resolution.function_modules.get(&name) {
                            let mut modules = modules.iter().cloned().collect::<Vec<_>>();
                            modules.sort();
                            let import_hint = modules
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "module.path".to_string());
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E2102",
                                    format!(
                                        "symbol '{}' is not available without an explicit import",
                                        name
                                    ),
                                    self.file,
                                    callee.span,
                                )
                                .with_help(format!("add `import {};`", import_hint)),
                            );
                            return "<?>".to_string();
                        }
                    }

                    if self
                        .resolution
                        .function_modules
                        .get(&name)
                        .map(|mods| mods.len() > 1)
                        .unwrap_or(false)
                    {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E2104",
                                format!("ambiguous callable '{}' from multiple modules", name),
                                self.file,
                                callee.span,
                            )
                            .with_help("qualify the call (for example `module.symbol(...)`)"),
                        );
                        return "<?>".to_string();
                    }

                    if let Some(modules) = self.resolution.function_modules.get(&name) {
                        if modules.len() == 1 {
                            resolved_module = modules.iter().next().cloned();
                        }
                    }

                    name.clone()
                };

                let sig = if let Some(module_name) = resolved_module.as_ref() {
                    self.module_functions
                        .get(&(module_name.clone(), resolved_name.clone()))
                        .cloned()
                        .or_else(|| self.functions.get(&resolved_name).cloned())
                } else {
                    self.functions.get(&resolved_name).cloned()
                };

                if let Some(mut sig) = sig {
                    for (idx, param_ty) in sig.params.iter_mut().enumerate() {
                        if contains_unresolved_type(param_ty) {
                            let key = (resolved_name.clone(), idx);
                            if let Some(inferred) = self.fn_param_inferred.get(&key) {
                                *param_ty = merge_types(param_ty, inferred);
                            }
                        }
                    }
                    if contains_unresolved_type(&sig.ret) {
                        if let Some(inferred_ret) = self.fn_return_inferred.get(&resolved_name) {
                            sig.ret = merge_types(&sig.ret, inferred_ret);
                        }
                    }

                    if let Some(module_name) = resolved_module.as_deref() {
                        if let Some(entry) = find_deprecated_api(module_name, &resolved_name) {
                            self.diagnostics.push(
                                Diagnostic::warning(
                                    "E6001",
                                    format!(
                                        "deprecated API '{}.{}' is used",
                                        entry.module, entry.symbol
                                    ),
                                    self.file,
                                    expr.span,
                                )
                                .with_help(format!(
                                    "replace with '{}' (deprecated since {}, {})",
                                    entry.replacement, entry.since, entry.note
                                )),
                            );
                        }
                    }

                    if resolved_name == "print_int"
                        || resolved_name == "print_str"
                        || resolved_name == "print_float"
                        || resolved_name == "panic"
                    {
                        if !self.resolution.imports.contains("std.io") {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1300",
                                    "std.io import required to call IO functions",
                                    self.file,
                                    expr.span,
                                )
                                .with_help("add `import std.io;`"),
                            );
                        }
                    }
                    if resolved_name == "len" && !self.resolution.imports.contains("std.string") {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1301",
                                "std.string import required to call len",
                                self.file,
                                expr.span,
                            )
                            .with_help("add `import std.string;`"),
                        );
                    }

                    if (sig.is_extern || sig.is_unsafe)
                        && !(self.current_function_is_unsafe || self.unsafe_depth > 0)
                    {
                        let target = if sig.is_extern {
                            let abi = sig.extern_abi.as_deref().unwrap_or("<?>");
                            format!("extern \"{}\" function '{}'", abi, rendered_path)
                        } else {
                            format!("unsafe function '{}'", rendered_path)
                        };
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E2122",
                                format!("call to {} requires an explicit unsafe boundary", target),
                                self.file,
                                expr.span,
                            )
                            .with_help(
                                "wrap this call in `unsafe { ... }` or use an `unsafe fn` wrapper",
                            ),
                        );
                    }

                    let mut arg_types = Vec::new();
                    for (idx, arg) in args.iter().enumerate() {
                        let arg_ty = if let Some(expected_hint) = sig.params.get(idx) {
                            self.check_expr_with_expected(
                                arg,
                                locals,
                                allowed_effects,
                                ctx,
                                contract_mode,
                                Some(expected_hint),
                            )
                        } else {
                            self.check_expr(arg, locals, allowed_effects, ctx, contract_mode)
                        };
                        arg_types.push(arg_ty);
                    }

                    if args.len() != sig.params.len() {
                        self.diagnostics.push(Diagnostic::error(
                            "E1213",
                            format!(
                                "function '{}' expects {} args, got {}",
                                rendered_path,
                                sig.params.len(),
                                args.len()
                            ),
                            self.file,
                            expr.span,
                        ));
                    }

                    let generic_set = sig.generic_params.iter().cloned().collect::<BTreeSet<_>>();
                    let mut generic_bindings = BTreeMap::new();

                    for (idx, arg_ty) in arg_types.iter().enumerate() {
                        let Some(expected_raw) = sig.params.get(idx) else {
                            continue;
                        };
                        let expected_norm = self.normalize_type(expected_raw);
                        let arg_norm = self.normalize_type(arg_ty);
                        let inferred = infer_generic_bindings(
                            &expected_norm,
                            &arg_norm,
                            &generic_set,
                            &mut generic_bindings,
                        );
                        if !inferred {
                            self.diagnostics.push(Diagnostic::error(
                                "E1214",
                                format!(
                                    "argument {} to '{}' expected '{}', found '{}'",
                                    idx + 1,
                                    rendered_path,
                                    expected_raw,
                                    arg_ty
                                ),
                                self.file,
                                args[idx].span,
                            ));
                        }
                    }

                    if !sig.generic_params.is_empty() {
                        if let Some(expected) = expected_ty {
                            let expected_norm = self.normalize_type(expected);
                            let expected_ret_hint =
                                if sig.is_async && base_type_name(&expected_norm) == "Async" {
                                    extract_generic_args(&expected_norm)
                                        .and_then(|args| args.into_iter().next())
                                        .unwrap_or_else(|| expected.to_string())
                                } else {
                                    expected.to_string()
                                };
                            if !contains_unresolved_type(&expected_ret_hint) {
                                let ret_norm = self.normalize_type(&sig.ret);
                                let expected_ret_norm = self.normalize_type(&expected_ret_hint);
                                infer_generic_bindings(
                                    &ret_norm,
                                    &expected_ret_norm,
                                    &generic_set,
                                    &mut generic_bindings,
                                );
                            }
                        }
                    }

                    if !sig.generic_params.is_empty() {
                        let unresolved = sig
                            .generic_params
                            .iter()
                            .filter(|g| !generic_bindings.contains_key(*g))
                            .cloned()
                            .collect::<Vec<_>>();
                        if !unresolved.is_empty() && expected_ty.is_some() {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1212",
                                    format!(
                                        "cannot infer generic parameters for '{}': {}",
                                        rendered_path,
                                        unresolved.join(", ")
                                    ),
                                    self.file,
                                    expr.span,
                                )
                                .with_help(
                                    "provide argument values with concrete types or annotate intermediates",
                                ),
                            );
                        }
                    }

                    for (generic_name, bounds) in &sig.generic_bounds {
                        let Some(bound_ty) = generic_bindings.get(generic_name) else {
                            continue;
                        };
                        if contains_unresolved_type(bound_ty) {
                            continue;
                        }
                        for bound_trait in bounds {
                            let implemented = self
                                .resolution
                                .trait_impls
                                .get(bound_trait)
                                .map(|impls| impls.contains(bound_ty))
                                .unwrap_or(false);
                            if !implemented {
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        "E1258",
                                        format!(
                                            "type '{}' does not satisfy trait bound '{}: {}'",
                                            bound_ty, generic_name, bound_trait
                                        ),
                                        self.file,
                                        expr.span,
                                    )
                                    .with_help(format!(
                                        "add `impl {}[{}];` or use a type that implements '{}'",
                                        bound_trait, bound_ty, bound_trait
                                    )),
                                );
                            }
                        }
                    }

                    let instantiated_params = sig
                        .params
                        .iter()
                        .map(|param| substitute_type_vars(param, &generic_bindings, &generic_set))
                        .collect::<Vec<_>>();

                    for (idx, arg_ty) in arg_types.iter().enumerate() {
                        let Some(expected) = instantiated_params.get(idx) else {
                            continue;
                        };
                        let mut observed_ty = arg_ty.clone();
                        if let ir::ExprKind::Var(name) = &args[idx].kind {
                            if let Some(local_ty) = locals.get(name).cloned() {
                                let expected_norm = self.normalize_type(expected);
                                if contains_unresolved_type(&local_ty)
                                    && !contains_unresolved_type(&expected_norm)
                                    && !contains_symbolic_generic_type(&expected_norm)
                                    && self.types_compatible(&expected_norm, &local_ty)
                                {
                                    let refined =
                                        self.merge_compatible_types(&local_ty, &expected_norm);
                                    if !contains_unresolved_type(&refined) {
                                        locals.insert(name.clone(), refined.clone());
                                        observed_ty = refined;
                                    }
                                } else {
                                    observed_ty = local_ty;
                                }
                            }
                        }
                        if !self.types_compatible(expected, &observed_ty) {
                            self.diagnostics.push(Diagnostic::error(
                                "E1214",
                                format!(
                                    "argument {} to '{}' expected '{}', found '{}'",
                                    idx + 1,
                                    rendered_path,
                                    expected,
                                    observed_ty
                                ),
                                self.file,
                                args[idx].span,
                            ));
                        }
                        if contains_unresolved_type(expected) {
                            self.observe_fn_param_hole(&resolved_name, idx, &observed_ty);
                        }
                    }

                    if !sig.effects.is_empty() {
                        if contract_mode {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E2002",
                                    "contracts must be pure; effectful call found",
                                    self.file,
                                    expr.span,
                                )
                                .with_help("remove IO/time/rand/net/fs calls from requires/ensures/invariant"),
                            );
                        }
                        for effect in &sig.effects {
                            ctx.effects_used.insert(effect.clone());
                        }
                        if !sig.effects.is_subset(allowed_effects) {
                            let missing = sig
                                .effects
                                .difference(allowed_effects)
                                .cloned()
                                .collect::<Vec<_>>();
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E2001",
                                    format!(
                                        "calling '{}' requires undeclared effects: {}",
                                        rendered_path,
                                        missing.join(", ")
                                    ),
                                    self.file,
                                    expr.span,
                                )
                                .with_help(format!(
                                    "add `effects {{ {} }}` on the enclosing function",
                                    missing.join(", ")
                                )),
                            );
                        }
                    }

                    if !contract_mode {
                        self.record_call_edge(&resolved_name, expr.span);
                    }

                    let mut ret_ty =
                        substitute_type_vars(&sig.ret, &generic_bindings, &generic_set);
                    if sig.is_async {
                        ret_ty = format!("Async[{ret_ty}]");
                    }
                    if contains_unresolved_type(&ret_ty) {
                        if let Some(expected) = expected_ty {
                            let expected_norm = self.normalize_type(expected);
                            if self.types_compatible(&ret_ty, &expected_norm) {
                                let merged = self.merge_compatible_types(&ret_ty, &expected_norm);
                                self.observe_fn_return_hole(&resolved_name, &merged);
                                ret_ty = merged;
                            }
                        }
                    }
                    if contains_unresolved_type(&ret_ty) {
                        self.observe_fn_return_hole(&resolved_name, &ret_ty);
                    }
                    let param_inferred = (0..sig.params.len())
                        .map(|idx| {
                            self.fn_param_inferred
                                .get(&(resolved_name.clone(), idx))
                                .cloned()
                        })
                        .collect::<Vec<_>>();
                    let return_inferred = self.fn_return_inferred.get(&resolved_name).cloned();
                    if let Some(sig_mut) = self.functions.get_mut(&resolved_name) {
                        for (idx, inferred) in param_inferred.into_iter().enumerate() {
                            if let Some(inferred) = inferred {
                                sig_mut.params[idx] = merge_types(&sig_mut.params[idx], &inferred);
                            }
                        }
                        if let Some(inferred_ret) = return_inferred {
                            sig_mut.ret = merge_types(&sig_mut.ret, &inferred_ret);
                        }
                    }
                    if !sig.generic_params.is_empty() && contains_unresolved_type(&ret_ty) {
                        if expected_ty.is_some() {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1212",
                                    format!(
                                        "cannot fully resolve return type for generic call '{}': inferred '{}'",
                                        rendered_path, ret_ty
                                    ),
                                    self.file,
                                    expr.span,
                                )
                                .with_help("add explicit type annotations to constrain generic inference"),
                            );
                        }
                    }
                    if !sig.generic_params.is_empty() {
                        let applied = sig
                            .generic_params
                            .iter()
                            .map(|g| {
                                generic_bindings
                                    .get(g)
                                    .cloned()
                                    .unwrap_or_else(|| "<?>".to_string())
                            })
                            .collect::<Vec<_>>();
                        if applied.iter().all(|arg| !contains_unresolved_type(arg)) {
                            let symbol = resolved_module
                                .as_ref()
                                .and_then(|module| {
                                    self.resolution
                                        .module_function_infos
                                        .get(&(module.clone(), resolved_name.clone()))
                                })
                                .or_else(|| self.resolution.functions.get(&resolved_name))
                                .map(|f| f.symbol);
                            self.record_instantiation(
                                ir::GenericInstantiationKind::Function,
                                &resolved_name,
                                symbol,
                                &applied,
                                expr.span,
                            );
                        }
                    }
                    return ret_ty;
                }

                if !qualified {
                    let mut candidates = self.find_variants(&name);
                    if candidates.len() > 1 {
                        if let Some(expected_enum) = expected_ty
                            .and_then(|expected| self.extract_expected_enum_name(expected))
                        {
                            candidates.retain(|c| c.enum_name == expected_enum);
                        }
                    }
                    if candidates.len() > 1 {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E2104",
                                format!(
                                    "ambiguous variant '{}' found in enums: {}",
                                    name,
                                    candidates
                                        .iter()
                                        .map(|c| c.enum_name.clone())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                                self.file,
                                expr.span,
                            )
                            .with_help(
                                "use a typed context (annotation) to disambiguate the variant",
                            ),
                        );
                        return "<?>".to_string();
                    }

                    if let Some(candidate) = candidates.first() {
                        let generic_set = candidate
                            .generic_params
                            .iter()
                            .cloned()
                            .collect::<BTreeSet<_>>();
                        let mut generic_bindings = BTreeMap::new();

                        if let Some(payload_ty) = &candidate.payload {
                            if args.len() != 1 {
                                self.diagnostics.push(Diagnostic::error(
                                    "E1215",
                                    format!("variant '{}' expects one payload argument", name),
                                    self.file,
                                    expr.span,
                                ));
                            } else {
                                let arg_ty = self.check_expr(
                                    &args[0],
                                    locals,
                                    allowed_effects,
                                    ctx,
                                    contract_mode,
                                );
                                let payload_norm = self.normalize_type(payload_ty);
                                let arg_norm = self.normalize_type(&arg_ty);
                                let inferred = infer_generic_bindings(
                                    &payload_norm,
                                    &arg_norm,
                                    &generic_set,
                                    &mut generic_bindings,
                                );
                                if !inferred {
                                    self.diagnostics.push(Diagnostic::error(
                                        "E1216",
                                        format!(
                                            "variant '{}' payload type mismatch: expected '{}', found '{}'",
                                            name, payload_ty, arg_ty
                                        ),
                                        self.file,
                                        args[0].span,
                                    ));
                                }
                            }
                        } else if !args.is_empty() {
                            self.diagnostics.push(Diagnostic::error(
                                "E1217",
                                format!("variant '{}' takes no payload", name),
                                self.file,
                                expr.span,
                            ));
                        }

                        if candidate.generic_params.is_empty() {
                            return candidate.enum_name.clone();
                        }

                        let applied = candidate
                            .generic_params
                            .iter()
                            .map(|g| {
                                generic_bindings
                                    .get(g)
                                    .cloned()
                                    .unwrap_or_else(|| "<?>".to_string())
                            })
                            .collect::<Vec<_>>();
                        if applied.iter().any(|ty| contains_unresolved_type(ty)) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1212",
                                    format!(
                                        "cannot infer generic parameters for enum '{}'",
                                        candidate.enum_name
                                    ),
                                    self.file,
                                    expr.span,
                                )
                                .with_help("add a type annotation at the call site"),
                            );
                        } else {
                            self.record_instantiation(
                                ir::GenericInstantiationKind::Enum,
                                &candidate.enum_name,
                                Some(candidate.enum_symbol),
                                &applied,
                                expr.span,
                            );
                        }
                        return format!("{}[{}]", candidate.enum_name, applied.join(", "));
                    }
                }

                self.diagnostics.push(Diagnostic::error(
                    "E1218",
                    format!("unknown callable '{}'", rendered_path),
                    self.file,
                    callee.span,
                ));
                "<?>".to_string()
            }
            ir::ExprKind::Closure {
                params,
                ret_type,
                body,
            } => {
                let declared_ret = self
                    .types
                    .get(ret_type)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string());
                self.check_generic_arity(&declared_ret, expr.span);

                let expected_fn = expected_ty.and_then(parse_fn_type);
                if let Some((expected_params, _)) = &expected_fn {
                    if expected_params.len() != params.len() {
                        self.diagnostics.push(Diagnostic::error(
                            "E1284",
                            format!(
                                "closure expected {} parameter(s) from context, found {}",
                                expected_params.len(),
                                params.len()
                            ),
                            self.file,
                            expr.span,
                        ));
                    }
                }

                let mut closure_locals = locals.clone();
                let mut closure_param_tys = Vec::new();
                for (_idx, param) in params.iter().enumerate() {
                    let inferred_expected_param = expected_fn
                        .as_ref()
                        .and_then(|(params, _)| params.get(_idx))
                        .cloned();
                    let param_ty = if let Some(ty) = param.ty {
                        self.types
                            .get(&ty)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string())
                    } else if let Some(expected_param) = inferred_expected_param {
                        expected_param
                    } else {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1280",
                                format!("cannot infer type for closure parameter '{}'", param.name),
                                self.file,
                                param.span,
                            )
                            .with_help(
                                "add an explicit closure parameter type or pass the closure where an expected Fn type is known",
                            ),
                        );
                        "<?>".to_string()
                    };
                    self.check_generic_arity(&param_ty, param.span);
                    closure_locals.insert(param.name.clone(), param_ty.clone());
                    closure_param_tys.push(param_ty);
                }

                let body_ty = self.check_block(
                    body,
                    &mut closure_locals,
                    &declared_ret,
                    allowed_effects,
                    ctx,
                    contract_mode,
                );
                if !self.types_compatible(&declared_ret, &body_ty) {
                    self.diagnostics.push(Diagnostic::error(
                        "E1281",
                        format!(
                            "closure body type '{}' does not match declared return '{}'",
                            body_ty, declared_ret
                        ),
                        self.file,
                        body.span,
                    ));
                }

                if let Some((expected_params, expected_ret)) = &expected_fn {
                    for (idx, (expected_param, actual_param)) in expected_params
                        .iter()
                        .zip(closure_param_tys.iter())
                        .enumerate()
                    {
                        if is_symbolic_generic_name(expected_param) {
                            continue;
                        }
                        if !self.types_compatible(expected_param, actual_param) {
                            self.diagnostics.push(Diagnostic::error(
                                "E1285",
                                format!(
                                    "closure parameter {} expected '{}', found '{}'",
                                    idx + 1,
                                    expected_param,
                                    actual_param
                                ),
                                self.file,
                                expr.span,
                            ));
                        }
                    }
                    if !is_symbolic_generic_name(expected_ret)
                        && !self.types_compatible(expected_ret, &declared_ret)
                    {
                        self.diagnostics.push(Diagnostic::error(
                            "E1286",
                            format!(
                                "closure return '{}' does not match expected '{}'",
                                declared_ret, expected_ret
                            ),
                            self.file,
                            expr.span,
                        ));
                    }
                }

                render_fn_type(&closure_param_tys, &declared_ret)
            }
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let cond_ty = self.check_expr(cond, locals, allowed_effects, ctx, contract_mode);
                if self.normalize_type(&cond_ty) != "Bool" {
                    self.diagnostics.push(Diagnostic::error(
                        "E1219",
                        "if condition must be Bool",
                        self.file,
                        cond.span,
                    ));
                }
                let then_ty = self.check_block(
                    then_block,
                    locals,
                    "()",
                    allowed_effects,
                    ctx,
                    contract_mode,
                );
                let else_ty = self.check_block(
                    else_block,
                    locals,
                    "()",
                    allowed_effects,
                    ctx,
                    contract_mode,
                );
                if !self.types_compatible(&then_ty, &else_ty) {
                    self.diagnostics.push(Diagnostic::error(
                        "E1220",
                        format!(
                            "if branches must have same type (then '{}', else '{}')",
                            then_ty, else_ty
                        ),
                        self.file,
                        expr.span,
                    ));
                    "<?>".to_string()
                } else {
                    self.merge_compatible_types(&then_ty, &else_ty)
                }
            }
            ir::ExprKind::While { cond, body } => {
                let cond_ty = self.check_expr(cond, locals, allowed_effects, ctx, contract_mode);
                if self.normalize_type(&cond_ty) != "Bool" {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1273",
                            format!("while condition must be Bool, found '{}'", cond_ty),
                            self.file,
                            cond.span,
                        )
                        .with_help("use a Bool expression as the while condition"),
                    );
                }
                ctx.loop_stack.push(LoopContext {
                    break_ty: Some("()".to_string()),
                });
                let _ = self.check_block(body, locals, "()", allowed_effects, ctx, contract_mode);
                let loop_ctx = ctx.loop_stack.pop().unwrap_or_default();
                loop_ctx.break_ty.unwrap_or_else(|| "()".to_string())
            }
            ir::ExprKind::Loop { body } => {
                ctx.loop_stack.push(LoopContext { break_ty: None });
                let _ = self.check_block(body, locals, "()", allowed_effects, ctx, contract_mode);
                let loop_ctx = ctx.loop_stack.pop().unwrap_or_default();
                loop_ctx.break_ty.unwrap_or_else(|| "()".to_string())
            }
            ir::ExprKind::Break { expr: break_expr } => {
                if ctx.loop_stack.is_empty() {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1275",
                            "`break` may only be used inside a loop",
                            self.file,
                            expr.span,
                        )
                        .with_help("move `break` into a `while` or `loop` body"),
                    );
                    return "<?>".to_string();
                }

                let break_ty = if let Some(break_expr) = break_expr {
                    self.check_expr(break_expr, locals, allowed_effects, ctx, contract_mode)
                } else {
                    "()".to_string()
                };

                let loop_ctx = ctx.loop_stack.last_mut().expect("checked non-empty stack");
                match &loop_ctx.break_ty {
                    Some(expected) => {
                        if !self.types_compatible(expected, &break_ty) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1274",
                                    format!(
                                        "break type '{}' does not match loop break type '{}'",
                                        break_ty, expected
                                    ),
                                    self.file,
                                    expr.span,
                                )
                                .with_help("ensure every `break` in the loop has the same type"),
                            );
                        }
                        expected.clone()
                    }
                    None => {
                        loop_ctx.break_ty = Some(break_ty.clone());
                        break_ty
                    }
                }
            }
            ir::ExprKind::Continue => {
                if ctx.loop_stack.is_empty() {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1276",
                            "`continue` may only be used inside a loop",
                            self.file,
                            expr.span,
                        )
                        .with_help("move `continue` into a `while` or `loop` body"),
                    );
                    return "<?>".to_string();
                }
                "()".to_string()
            }
            ir::ExprKind::Match {
                expr: scrutinee,
                arms,
            } => {
                let scrutinee_ty =
                    self.check_expr(scrutinee, locals, allowed_effects, ctx, contract_mode);
                self.record_instantiation_from_applied_type(&scrutinee_ty, expr.span);
                let mut arm_types = Vec::new();
                let mut seen = BTreeSet::new();
                let mut wildcard_seen = false;

                for arm in arms {
                    let redundant = self.coverage_is_complete(&scrutinee_ty, &seen, wildcard_seen)
                        || self.arm_is_redundant(&arm.pattern, &scrutinee_ty, &seen, wildcard_seen);
                    if redundant {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1251",
                                "unreachable match arm",
                                self.file,
                                arm.span,
                            )
                            .with_help(
                                "remove this arm or place it before earlier overlapping patterns",
                            ),
                        );
                    }
                    let mut arm_scope = locals.clone();
                    let mut bound_names = BTreeSet::new();
                    self.check_pattern(
                        &arm.pattern,
                        &scrutinee_ty,
                        &mut arm_scope,
                        &mut bound_names,
                    );
                    if let Some(guard) = &arm.guard {
                        let guard_ty = self.check_expr(
                            guard,
                            &mut arm_scope,
                            allowed_effects,
                            ctx,
                            contract_mode,
                        );
                        if self.normalize_type(&guard_ty) != "Bool" {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1270",
                                    format!("match guard must be Bool, found '{}'", guard_ty),
                                    self.file,
                                    guard.span,
                                )
                                .with_help("make the guard expression evaluate to Bool"),
                            );
                        }
                    } else {
                        self.record_pattern_coverage(
                            &arm.pattern,
                            &scrutinee_ty,
                            &mut seen,
                            &mut wildcard_seen,
                        );
                    }
                    let body_ty = self.check_expr(
                        &arm.body,
                        &mut arm_scope,
                        allowed_effects,
                        ctx,
                        contract_mode,
                    );
                    arm_types.push(body_ty);
                }

                self.check_exhaustive(expr.span, &scrutinee_ty, &seen, wildcard_seen);

                if arm_types.is_empty() {
                    "()".to_string()
                } else {
                    let first = arm_types[0].clone();
                    for ty in arm_types.iter().skip(1) {
                        if !self.types_compatible(&first, ty) {
                            self.diagnostics.push(Diagnostic::error(
                                "E1221",
                                format!(
                                    "match arms must return same type (found '{}' and '{}')",
                                    first, ty
                                ),
                                self.file,
                                expr.span,
                            ));
                            return "<?>".to_string();
                        }
                    }
                    arm_types
                        .into_iter()
                        .reduce(|a, b| self.merge_compatible_types(&a, &b))
                        .unwrap_or_else(|| "()".to_string())
                }
            }
            ir::ExprKind::Binary { op, lhs, rhs } => {
                let mut left_ty = self.check_expr(lhs, locals, allowed_effects, ctx, contract_mode);
                let mut right_ty =
                    self.check_expr(rhs, locals, allowed_effects, ctx, contract_mode);
                if contains_unresolved_type(&left_ty) && !contains_unresolved_type(&right_ty) {
                    if let Some(refined) =
                        self.refine_variable_with_expected(lhs, &right_ty, locals)
                    {
                        left_ty = refined;
                    }
                }
                if contains_unresolved_type(&right_ty) && !contains_unresolved_type(&left_ty) {
                    if let Some(refined) = self.refine_variable_with_expected(rhs, &left_ty, locals)
                    {
                        right_ty = refined;
                    }
                }
                self.check_binary(*op, &left_ty, &right_ty, expr.span)
            }
            ir::ExprKind::Unary { op, expr: inner } => {
                let ty = self.check_expr(inner, locals, allowed_effects, ctx, contract_mode);
                let ty_norm = self.normalize_type(&ty);
                match op {
                    crate::ast::UnaryOp::Neg => {
                        if ty_norm != "Int" && ty_norm != "Float" {
                            self.diagnostics.push(Diagnostic::error(
                                "E1222",
                                "unary '-' expects Int or Float",
                                self.file,
                                inner.span,
                            ));
                        }
                        if ty_norm == "Float" {
                            "Float".to_string()
                        } else {
                            "Int".to_string()
                        }
                    }
                    crate::ast::UnaryOp::Not => {
                        if ty_norm != "Bool" {
                            self.diagnostics.push(Diagnostic::error(
                                "E1223",
                                "unary '!' expects Bool",
                                self.file,
                                inner.span,
                            ));
                        }
                        "Bool".to_string()
                    }
                }
            }
            ir::ExprKind::Borrow {
                mutable,
                expr: inner,
            } => {
                let ty = self.check_expr(inner, locals, allowed_effects, ctx, contract_mode);
                if *mutable {
                    format!("RefMut[{}]", ty)
                } else {
                    format!("Ref[{}]", ty)
                }
            }
            ir::ExprKind::Await { expr: inner } => {
                let ty = self.check_expr(inner, locals, allowed_effects, ctx, contract_mode);
                let ty_norm = self.normalize_type(&ty);
                if !self.current_function_is_async {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1256",
                            "await can only be used inside async functions",
                            self.file,
                            expr.span,
                        )
                        .with_help("mark the enclosing function as `async fn`"),
                    );
                }

                if base_type_name(&ty_norm) != "Async" {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1257",
                            format!("await expects Async[T], found '{}'", ty),
                            self.file,
                            inner.span,
                        )
                        .with_help("await values returned from async function calls"),
                    );
                    return "<?>".to_string();
                }

                let args = extract_generic_args(&ty_norm).unwrap_or_default();
                if args.len() != 1 {
                    self.diagnostics.push(Diagnostic::error(
                        "E1257",
                        format!("await expects Async[T], found '{}'", ty),
                        self.file,
                        inner.span,
                    ));
                    return "<?>".to_string();
                }
                args[0].clone()
            }
            ir::ExprKind::Try { expr: inner } => {
                let ty = self.check_expr(inner, locals, allowed_effects, ctx, contract_mode);
                let ty_norm = self.normalize_type(&ty);
                if base_type_name(&ty_norm) != "Result" {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1260",
                            format!("`?` expects Result[T, E], found '{}'", ty),
                            self.file,
                            inner.span,
                        )
                        .with_help("use `?` only on Result-returning expressions"),
                    );
                    return "<?>".to_string();
                }

                let args = extract_generic_args(&ty_norm).unwrap_or_default();
                if args.len() != 2 {
                    self.diagnostics.push(Diagnostic::error(
                        "E1260",
                        format!("`?` expects Result[T, E], found '{}'", ty),
                        self.file,
                        inner.span,
                    ));
                    return "<?>".to_string();
                }

                let Some(function_ret) = self.current_function_ret_type.clone() else {
                    self.diagnostics.push(Diagnostic::error(
                        "E1261",
                        "`?` cannot be used outside of a function body",
                        self.file,
                        expr.span,
                    ));
                    return "<?>".to_string();
                };

                let function_ret_norm = self.normalize_type(&function_ret);
                if base_type_name(&function_ret_norm) != "Result" {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1261",
                            format!(
                                "`?` requires enclosing function return type Result[_, E], found '{}'",
                                function_ret
                            ),
                            self.file,
                            expr.span,
                        )
                        .with_help("change the function return type to Result[T, E] or handle Err explicitly"),
                    );
                    return "<?>".to_string();
                }

                let fn_args = extract_generic_args(&function_ret_norm).unwrap_or_default();
                if fn_args.len() != 2 {
                    self.diagnostics.push(Diagnostic::error(
                        "E1261",
                        format!(
                            "`?` requires enclosing function return type Result[_, E], found '{}'",
                            function_ret
                        ),
                        self.file,
                        expr.span,
                    ));
                    return "<?>".to_string();
                }

                let expr_err = &args[1];
                let fn_err = &fn_args[1];
                if !self.types_compatible(fn_err, expr_err) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1262",
                            format!(
                                "`?` error type mismatch: expression has '{}', function expects '{}'",
                                expr_err, fn_err
                            ),
                            self.file,
                            expr.span,
                        )
                        .with_help(
                            "align Result error types explicitly; implicit error conversions are not allowed",
                        ),
                    );
                }

                args[0].clone()
            }
            ir::ExprKind::UnsafeBlock { block } => {
                let previous_depth = self.unsafe_depth;
                self.unsafe_depth += 1;
                let ty = self.check_block(block, locals, "()", allowed_effects, ctx, contract_mode);
                self.unsafe_depth = previous_depth;
                ty
            }
            ir::ExprKind::StructInit { name, fields } => {
                if name == TUPLE_INTERNAL_NAME {
                    let expected_tuple_items = expected_ty.and_then(|expected| {
                        let normalized = self.normalize_type(expected);
                        if base_type_name(&normalized) == TUPLE_INTERNAL_NAME {
                            extract_generic_args(&normalized)
                        } else {
                            None
                        }
                    });
                    let mut indexed: BTreeMap<usize, (String, crate::span::Span)> = BTreeMap::new();
                    for (field_name, value, span) in fields {
                        let Ok(index) = field_name.parse::<usize>() else {
                            self.diagnostics.push(Diagnostic::error(
                                "E1225",
                                format!(
                                    "tuple field '{}' is not a valid numeric index",
                                    field_name
                                ),
                                self.file,
                                *span,
                            ));
                            continue;
                        };
                        if indexed.contains_key(&index) {
                            self.diagnostics.push(Diagnostic::error(
                                "E1254",
                                format!("duplicate tuple element index '{}'", field_name),
                                self.file,
                                *span,
                            ));
                            continue;
                        }
                        let expected_item = expected_tuple_items
                            .as_ref()
                            .and_then(|items| items.get(index))
                            .map(String::as_str);
                        let value_ty = self.check_expr_with_expected(
                            value,
                            locals,
                            allowed_effects,
                            ctx,
                            contract_mode,
                            expected_item,
                        );
                        indexed.insert(index, (value_ty, *span));
                    }

                    if indexed.is_empty() {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1227",
                                "tuple literal must include at least one element",
                                self.file,
                                expr.span,
                            )
                            .with_help("write tuple literals as `(a, b, ...)`"),
                        );
                        return "Tuple[<?>]".to_string();
                    }

                    let max_index = indexed.keys().copied().max().unwrap_or(0);
                    let mut item_types = Vec::new();
                    for index in 0..=max_index {
                        if let Some((ty, _)) = indexed.remove(&index) {
                            item_types.push(ty);
                        } else {
                            self.diagnostics.push(Diagnostic::error(
                                "E1227",
                                format!("tuple literal is missing element index '{}'", index),
                                self.file,
                                expr.span,
                            ));
                            item_types.push("<?>".to_string());
                        }
                    }
                    return format!("{TUPLE_INTERNAL_NAME}[{}]", item_types.join(", "));
                }

                let Some(info) = self.resolution.structs.get(name).cloned() else {
                    self.diagnostics.push(Diagnostic::error(
                        "E1224",
                        format!("unknown struct '{}'", name),
                        self.file,
                        expr.span,
                    ));
                    return "<?>".to_string();
                };

                let generic_set = info.generics.iter().cloned().collect::<BTreeSet<_>>();
                let mut generic_bindings = BTreeMap::new();
                let mut seen_fields = BTreeSet::new();

                for (field_name, value, span) in fields {
                    if !seen_fields.insert(field_name.clone()) {
                        self.diagnostics.push(Diagnostic::error(
                            "E1254",
                            format!(
                                "duplicate field '{}.{}' in struct literal",
                                name, field_name
                            ),
                            self.file,
                            *span,
                        ));
                        continue;
                    }

                    let Some(expected) = info.fields.get(field_name) else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1225",
                            format!("struct member '{}.{}' does not exist", name, field_name),
                            self.file,
                            *span,
                        ));
                        continue;
                    };
                    let expected_ty = self
                        .types
                        .get(expected)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string());
                    let found_ty =
                        self.check_expr(value, locals, allowed_effects, ctx, contract_mode);
                    if contains_unresolved_type(&expected_ty) {
                        self.observe_struct_field_hole(name, field_name, &found_ty);
                    }
                    let expected_norm = self.normalize_type(&expected_ty);
                    let found_norm = self.normalize_type(&found_ty);

                    let inferred = infer_generic_bindings(
                        &expected_norm,
                        &found_norm,
                        &generic_set,
                        &mut generic_bindings,
                    );
                    if !inferred {
                        self.diagnostics.push(Diagnostic::error(
                            "E1226",
                            format!(
                                "field '{}.{}' expects '{}', found '{}'",
                                name, field_name, expected_ty, found_ty
                            ),
                            self.file,
                            value.span,
                        ));
                        continue;
                    }
                    let expected_inst =
                        substitute_type_vars(&expected_ty, &generic_bindings, &generic_set);

                    if !self.types_compatible(&expected_inst, &found_ty) {
                        self.diagnostics.push(Diagnostic::error(
                            "E1226",
                            format!(
                                "field '{}.{}' expects '{}', found '{}'",
                                name, field_name, expected_inst, found_ty
                            ),
                            self.file,
                            value.span,
                        ));
                    }
                }

                for field in info.fields.keys() {
                    if !fields.iter().any(|(name, _, _)| name == field) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1227",
                                format!("missing field '{}.{}' in struct literal", name, field),
                                self.file,
                                expr.span,
                            )
                            .with_help("provide values for all struct fields"),
                        );
                    }
                }

                if info.generics.is_empty() {
                    return name.clone();
                }

                let applied = info
                    .generics
                    .iter()
                    .map(|g| {
                        generic_bindings
                            .get(g)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string())
                    })
                    .collect::<Vec<_>>();
                if applied.iter().all(|ty| !contains_unresolved_type(ty)) {
                    self.record_instantiation(
                        ir::GenericInstantiationKind::Struct,
                        name,
                        Some(info.symbol),
                        &applied,
                        expr.span,
                    );
                }
                format!("{}[{}]", name, applied.join(", "))
            }
            ir::ExprKind::FieldAccess { base, field } => {
                let base_ty = self.check_expr(base, locals, allowed_effects, ctx, contract_mode);
                let base_ty_norm = self.normalize_type(&base_ty);
                if base_type_name(&base_ty_norm) == TUPLE_INTERNAL_NAME {
                    let Ok(index) = field.parse::<usize>() else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1228",
                            format!(
                                "tuple field access requires numeric index like `.0`, found '.{}'",
                                field
                            ),
                            self.file,
                            expr.span,
                        ));
                        return "<?>".to_string();
                    };
                    let elements = extract_generic_args(&base_ty_norm).unwrap_or_default();
                    if let Some(ty) = elements.get(index) {
                        return ty.clone();
                    }
                    self.diagnostics.push(Diagnostic::error(
                        "E1228",
                        format!(
                            "tuple index .{} is out of range for type '{}' with {} elements",
                            index,
                            base_ty,
                            elements.len()
                        ),
                        self.file,
                        expr.span,
                    ));
                    return "<?>".to_string();
                }
                if let Some(info) = self.find_struct(&base_ty) {
                    let struct_name = base_type_name(&base_ty_norm).to_string();
                    if let Some(field_ty_id) = info.fields.get(field) {
                        let mut field_ty = self
                            .types
                            .get(field_ty_id)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string());
                        if contains_unresolved_type(&field_ty) {
                            if let Some(inferred) = self
                                .struct_field_inferred
                                .get(&(struct_name.clone(), field.clone()))
                            {
                                field_ty = merge_types(&field_ty, inferred);
                            }
                        }
                        if info.generics.is_empty() {
                            return field_ty;
                        }

                        if let Some(bindings) =
                            bindings_from_applied_type(&base_ty_norm, &info.generics)
                        {
                            let generic_set =
                                info.generics.iter().cloned().collect::<BTreeSet<_>>();
                            return substitute_type_vars(&field_ty, &bindings, &generic_set);
                        }

                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1250",
                                format!(
                                    "generic arity mismatch for struct type '{}': expected {} arguments",
                                    base_type_name(&base_ty),
                                    info.generics.len()
                                ),
                                self.file,
                                expr.span,
                            )
                            .with_help("provide the correct number of generic arguments"),
                        );
                        return "<?>".to_string();
                    }
                    self.diagnostics.push(Diagnostic::error(
                        "E1228",
                        format!("struct '{}' has no field '{}'", base_ty, field),
                        self.file,
                        expr.span,
                    ));
                    return "<?>".to_string();
                }
                self.diagnostics.push(Diagnostic::error(
                    "E1229",
                    format!("field access requires struct type, found '{}'", base_ty),
                    self.file,
                    expr.span,
                ));
                "<?>".to_string()
            }
        }
    }

    fn check_fn_value_call(
        &mut self,
        callee_ty: &str,
        rendered_callee: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        locals: &mut BTreeMap<String, String>,
        allowed_effects: &BTreeSet<String>,
        ctx: &mut ExprContext,
        contract_mode: bool,
    ) -> String {
        let callee_ty_norm = self.normalize_type(callee_ty);
        let Some((param_tys, ret_ty)) = parse_fn_type(&callee_ty_norm) else {
            self.diagnostics.push(Diagnostic::error(
                "E1209",
                "callee must be a function value of type Fn(...) -> ...",
                self.file,
                span,
            ));
            return "<?>".to_string();
        };

        if args.len() != param_tys.len() {
            self.diagnostics.push(Diagnostic::error(
                "E1213",
                format!(
                    "callable '{}' expects {} args, got {}",
                    rendered_callee,
                    param_tys.len(),
                    args.len()
                ),
                self.file,
                span,
            ));
        }

        for (idx, arg) in args.iter().enumerate() {
            let expected = param_tys.get(idx).map(String::as_str);
            let found = self.check_expr_with_expected(
                arg,
                locals,
                allowed_effects,
                ctx,
                contract_mode,
                expected,
            );
            let Some(expected) = param_tys.get(idx) else {
                continue;
            };
            if !self.types_compatible(expected, &found) {
                self.diagnostics.push(Diagnostic::error(
                    "E1214",
                    format!(
                        "argument {} to '{}' expected '{}', found '{}'",
                        idx + 1,
                        rendered_callee,
                        expected,
                        found
                    ),
                    self.file,
                    arg.span,
                ));
            }
        }

        ret_ty
    }

    fn resolve_qualifier_module(&self, qualifier: &[String]) -> Option<String> {
        if qualifier.is_empty() {
            return None;
        }

        if qualifier.len() == 1 {
            let alias = &qualifier[0];
            if self.resolution.ambiguous_import_aliases.contains(alias) {
                return None;
            }
            if let Some(module) = self.resolution.import_aliases.get(alias) {
                return Some(module.clone());
            }
        }

        let full = qualifier.join(".");
        if self.resolution.imports.contains(&full) {
            return Some(full);
        }

        None
    }

    fn is_module_qualified_callee(
        &self,
        callee: &ir::Expr,
        locals: &BTreeMap<String, String>,
    ) -> bool {
        let Some(path) = self.extract_callee_path(callee) else {
            return false;
        };
        if path.len() < 2 {
            return false;
        }
        let qualifier = &path[..path.len() - 1];
        if qualifier.len() == 1 && locals.contains_key(&qualifier[0]) {
            return false;
        }
        self.resolve_qualifier_module(qualifier).is_some()
    }

    #[allow(clippy::too_many_arguments)]
    fn check_method_call(
        &mut self,
        base: &ir::Expr,
        field: &str,
        args: &[ir::Expr],
        call_expr: &ir::Expr,
        locals: &mut BTreeMap<String, String>,
        allowed_effects: &BTreeSet<String>,
        ctx: &mut ExprContext,
        contract_mode: bool,
        expected_ty: Option<&str>,
    ) -> String {
        let base_ty = self.check_expr(base, locals, allowed_effects, ctx, contract_mode);
        let normalized = self.normalize_type(&base_ty);
        let receiver_ty = match base_type_name(&normalized) {
            "Ref" | "RefMut" => extract_generic_args(&normalized)
                .and_then(|args| args.first().cloned())
                .unwrap_or(normalized),
            _ => normalized,
        };
        let assoc_name = format!("{}::{}", base_type_name(&receiver_ty), field);

        if self.functions.contains_key(&assoc_name) {
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(base.clone());
            call_args.extend(args.iter().cloned());
            let synthetic = ir::Expr {
                node: call_expr.node,
                kind: ir::ExprKind::Call {
                    callee: Box::new(ir::Expr {
                        node: call_expr.node,
                        kind: ir::ExprKind::Var(assoc_name),
                        span: base.span,
                    }),
                    args: call_args,
                },
                span: call_expr.span,
            };
            return self.check_expr_with_expected(
                &synthetic,
                locals,
                allowed_effects,
                ctx,
                contract_mode,
                expected_ty,
            );
        }

        if let Some((trait_name, method_sig, mut type_bindings)) =
            self.find_trait_bound_method(&receiver_ty, field)
        {
            type_bindings
                .entry("Self".to_string())
                .or_insert_with(|| receiver_ty.clone());
            let generic_params = method_sig.generics.iter().cloned().collect::<BTreeSet<_>>();
            let params = method_sig
                .param_types
                .iter()
                .map(|ty| {
                    let raw = self
                        .types
                        .get(ty)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string());
                    substitute_type_vars(&raw, &type_bindings, &generic_params)
                })
                .collect::<Vec<_>>();

            let rendered = format!("{trait_name}::{}", field);
            if params.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    "E1213",
                    format!(
                        "method '{}' has invalid trait signature: receiver parameter missing",
                        rendered
                    ),
                    self.file,
                    call_expr.span,
                ));
                return "<?>".to_string();
            }
            if args.len() + 1 != params.len() {
                self.diagnostics.push(Diagnostic::error(
                    "E1213",
                    format!(
                        "method '{}' expects {} args, got {}",
                        rendered,
                        params.len() - 1,
                        args.len()
                    ),
                    self.file,
                    call_expr.span,
                ));
            }

            if !self.types_compatible(&params[0], &receiver_ty) {
                self.diagnostics.push(Diagnostic::error(
                    "E1214",
                    format!(
                        "receiver for '{}' expected '{}', found '{}'",
                        rendered, params[0], receiver_ty
                    ),
                    self.file,
                    base.span,
                ));
            }

            for (idx, arg) in args.iter().enumerate() {
                let expected = params.get(idx + 1).map(String::as_str);
                let found = self.check_expr_with_expected(
                    arg,
                    locals,
                    allowed_effects,
                    ctx,
                    contract_mode,
                    expected,
                );
                if let Some(expected) = expected {
                    if !self.types_compatible(expected, &found) {
                        self.diagnostics.push(Diagnostic::error(
                            "E1214",
                            format!(
                                "argument {} to '{}' expected '{}', found '{}'",
                                idx + 1,
                                rendered,
                                expected,
                                found
                            ),
                            self.file,
                            arg.span,
                        ));
                    }
                }
            }

            if !method_sig.effects.is_empty() {
                if contract_mode {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E2002",
                            "contracts must be pure; effectful call found",
                            self.file,
                            call_expr.span,
                        )
                        .with_help(
                            "remove IO/time/rand/net/fs calls from requires/ensures/invariant",
                        ),
                    );
                }
                for effect in &method_sig.effects {
                    ctx.effects_used.insert(effect.clone());
                }
                if !method_sig.effects.is_subset(allowed_effects) {
                    let missing = method_sig
                        .effects
                        .difference(allowed_effects)
                        .cloned()
                        .collect::<Vec<_>>();
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E2001",
                            format!(
                                "calling '{}' requires undeclared effects: {}",
                                rendered,
                                missing.join(", ")
                            ),
                            self.file,
                            call_expr.span,
                        )
                        .with_help(format!(
                            "add `effects {{ {} }}` on the enclosing function",
                            missing.join(", ")
                        )),
                    );
                }
            }

            if method_sig.is_unsafe && !(self.current_function_is_unsafe || self.unsafe_depth > 0) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E2122",
                        format!(
                            "call to unsafe method '{}' requires an explicit unsafe boundary",
                            rendered
                        ),
                        self.file,
                        call_expr.span,
                    )
                    .with_help("wrap this call in `unsafe { ... }` or use an `unsafe fn` wrapper"),
                );
            }

            let ret_raw = self
                .types
                .get(&method_sig.ret_type)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            let mut ret = substitute_type_vars(&ret_raw, &type_bindings, &generic_params);
            if method_sig.is_async {
                ret = format!("Async[{ret}]");
            }
            return ret;
        }

        self.diagnostics.push(
            Diagnostic::error(
                "E1228",
                format!(
                    "unknown method '{}.{}' for receiver type '{}'",
                    base_type_name(&receiver_ty),
                    field,
                    receiver_ty
                ),
                self.file,
                call_expr.span,
            )
            .with_help("define an inherent method or add a trait bound that declares this method"),
        );
        "<?>".to_string()
    }

    fn find_trait_bound_method(
        &self,
        receiver_ty: &str,
        method_name: &str,
    ) -> Option<(String, FunctionInfo, BTreeMap<String, String>)> {
        let receiver_base = base_type_name(receiver_ty);
        let current = self.current_function.as_ref()?;
        let current_sig = self.functions.get(current)?;
        let bounds = current_sig.generic_bounds.get(receiver_base)?;
        for bound_trait in bounds {
            let Some(trait_info) = self.resolution.traits.get(bound_trait) else {
                continue;
            };
            let Some(method) = trait_info.methods.get(method_name) else {
                continue;
            };
            let mut bindings = BTreeMap::new();
            bindings.insert("Self".to_string(), receiver_ty.to_string());
            if trait_info.generics.len() == 1 {
                bindings.insert(trait_info.generics[0].clone(), receiver_ty.to_string());
            }
            return Some((bound_trait.clone(), method.clone(), bindings));
        }
        None
    }

    fn extract_callee_path(&self, callee: &ir::Expr) -> Option<Vec<String>> {
        fn walk(expr: &ir::Expr, out: &mut Vec<String>) -> bool {
            match &expr.kind {
                ir::ExprKind::Var(name) => {
                    out.push(name.clone());
                    true
                }
                ir::ExprKind::FieldAccess { base, field } => {
                    if !walk(base, out) {
                        return false;
                    }
                    out.push(field.clone());
                    true
                }
                _ => false,
            }
        }

        let mut out = Vec::new();
        if walk(callee, &mut out) {
            Some(out)
        } else {
            None
        }
    }

    fn check_generic_arity(&mut self, ty: &str, span: crate::span::Span) {
        let base = base_type_name(ty).to_string();
        if base == "Fn" {
            let provided = extract_generic_args(ty).map(|a| a.len()).unwrap_or(0);
            if provided == 0 {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1250",
                        "generic arity mismatch for 'Fn': expected at least 1 type argument",
                        self.file,
                        span,
                    )
                    .with_help("use `Fn(...) -> Ret` with at least a return type"),
                );
            }
            if let Some(args) = extract_generic_args(ty) {
                for arg in args {
                    self.check_generic_arity(&arg, span);
                }
            }
            return;
        }
        if let Some(expected) = self.generic_arity.get(&base).copied() {
            let provided = extract_generic_args(ty).map(|a| a.len()).unwrap_or(0);
            if provided != expected {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1250",
                        format!(
                            "generic arity mismatch for '{}': expected {}, found {}",
                            base, expected, provided
                        ),
                        self.file,
                        span,
                    )
                    .with_help("adjust the number of generic type arguments"),
                );
            }
        }

        if let Some(args) = extract_generic_args(ty) {
            for arg in args {
                self.check_generic_arity(&arg, span);
            }
        }
    }

    fn check_binary(&mut self, op: BinOp, lhs: &str, rhs: &str, span: crate::span::Span) -> String {
        let lhs_norm = self.normalize_type(lhs);
        let rhs_norm = self.normalize_type(rhs);
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                if lhs_norm == rhs_norm && (lhs_norm == "Int" || lhs_norm == "Float") {
                    lhs_norm
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        "E1230",
                        format!(
                            "arithmetic operators require matching Int or Float operands, found '{}' and '{}'",
                            lhs, rhs
                        ),
                        self.file,
                        span,
                    ));
                    "<?>".to_string()
                }
            }
            BinOp::Mod => {
                if lhs_norm != "Int" || rhs_norm != "Int" {
                    self.diagnostics.push(Diagnostic::error(
                        "E1230",
                        format!(
                            "operator '%' requires Int operands, found '{}' and '{}'",
                            lhs, rhs
                        ),
                        self.file,
                        span,
                    ));
                }
                "Int".to_string()
            }
            BinOp::Eq | BinOp::Ne => {
                if !self.types_compatible(lhs, rhs) {
                    self.diagnostics.push(Diagnostic::error(
                        "E1231",
                        format!(
                            "equality operands must match, found '{}' and '{}'",
                            lhs, rhs
                        ),
                        self.file,
                        span,
                    ));
                }
                "Bool".to_string()
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                if !(lhs_norm == rhs_norm && (lhs_norm == "Int" || lhs_norm == "Float")) {
                    self.diagnostics.push(Diagnostic::error(
                        "E1232",
                        "comparison operators require matching Int or Float operands",
                        self.file,
                        span,
                    ));
                }
                "Bool".to_string()
            }
            BinOp::And | BinOp::Or => {
                if lhs_norm != "Bool" || rhs_norm != "Bool" {
                    self.diagnostics.push(Diagnostic::error(
                        "E1233",
                        "logical operators require Bool operands",
                        self.file,
                        span,
                    ));
                }
                "Bool".to_string()
            }
        }
    }

    fn check_pattern(
        &mut self,
        pattern: &ir::Pattern,
        scrutinee_ty: &str,
        locals: &mut BTreeMap<String, String>,
        bound_names: &mut BTreeSet<String>,
    ) {
        let normalized_scrutinee_ty = self.normalize_type(scrutinee_ty);
        match &pattern.kind {
            ir::PatternKind::Wildcard => {}
            ir::PatternKind::Var(name) => {
                if !bound_names.insert(name.clone()) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1252",
                            format!("duplicate binding '{}' in pattern", name),
                            self.file,
                            pattern.span,
                        )
                        .with_help("each variable name may be bound at most once per pattern"),
                    );
                }
                locals.insert(name.clone(), scrutinee_ty.to_string());
            }
            ir::PatternKind::Int(_v) => {
                if normalized_scrutinee_ty != "Int" {
                    self.diagnostics.push(Diagnostic::error(
                        "E1234",
                        format!(
                            "int pattern requires Int scrutinee, found '{}'",
                            scrutinee_ty
                        ),
                        self.file,
                        pattern.span,
                    ));
                }
            }
            ir::PatternKind::Bool(_v) => {
                if normalized_scrutinee_ty != "Bool" {
                    self.diagnostics.push(Diagnostic::error(
                        "E1235",
                        format!(
                            "bool pattern requires Bool scrutinee, found '{}'",
                            scrutinee_ty
                        ),
                        self.file,
                        pattern.span,
                    ));
                }
            }
            ir::PatternKind::Unit => {
                if normalized_scrutinee_ty != "()" {
                    self.diagnostics.push(Diagnostic::error(
                        "E1236",
                        format!(
                            "unit pattern requires unit scrutinee, found '{}'",
                            scrutinee_ty
                        ),
                        self.file,
                        pattern.span,
                    ));
                }
            }
            ir::PatternKind::Or { patterns } => {
                if patterns.len() < 2 {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1271",
                            "or-pattern requires at least two alternatives",
                            self.file,
                            pattern.span,
                        )
                        .with_help("use `p1 | p2` with two or more alternatives"),
                    );
                }

                let mut alternatives = Vec::new();
                for alt in patterns {
                    let mut alt_locals = locals.clone();
                    let mut alt_bound_names = BTreeSet::new();
                    self.check_pattern(alt, scrutinee_ty, &mut alt_locals, &mut alt_bound_names);
                    let mut bindings = BTreeMap::new();
                    for name in alt_bound_names {
                        if let Some(ty) = alt_locals.get(&name) {
                            bindings.insert(name, ty.clone());
                        }
                    }
                    alternatives.push(bindings);
                }

                let Some(first_bindings) = alternatives.first().cloned() else {
                    return;
                };
                let first_names = first_bindings.keys().cloned().collect::<BTreeSet<_>>();

                let mut consistent = true;
                for (idx, bindings) in alternatives.iter().enumerate().skip(1) {
                    let names = bindings.keys().cloned().collect::<BTreeSet<_>>();
                    if names != first_names {
                        let expected = if first_names.is_empty() {
                            "<none>".to_string()
                        } else {
                            first_names.iter().cloned().collect::<Vec<_>>().join(", ")
                        };
                        let found = if names.is_empty() {
                            "<none>".to_string()
                        } else {
                            names.iter().cloned().collect::<Vec<_>>().join(", ")
                        };
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1271",
                                format!(
                                    "or-pattern alternative {} binds [{}], expected [{}]",
                                    idx + 1,
                                    found,
                                    expected
                                ),
                                self.file,
                                pattern.span,
                            )
                            .with_help(
                                "all alternatives in an or-pattern must bind the same variable names",
                            ),
                        );
                        consistent = false;
                        continue;
                    }
                    for name in &first_names {
                        let a = first_bindings.get(name).expect("first binding present");
                        let b = bindings.get(name).expect("alternative binding present");
                        if !self.types_compatible(a, b) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1272",
                                    format!(
                                        "or-pattern binding '{}' has incompatible types '{}' and '{}'",
                                        name, a, b
                                    ),
                                    self.file,
                                    pattern.span,
                                )
                                .with_help(
                                    "make every alternative bind this variable with the same type",
                                ),
                            );
                            consistent = false;
                        }
                    }
                }

                if consistent {
                    for (name, ty) in first_bindings {
                        if !bound_names.insert(name.clone()) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1252",
                                    format!("duplicate binding '{}' in pattern", name),
                                    self.file,
                                    pattern.span,
                                )
                                .with_help(
                                    "each variable name may be bound at most once per pattern",
                                ),
                            );
                        }
                        locals.insert(name, ty);
                    }
                }
            }
            ir::PatternKind::Variant { name, args } => {
                if name == TUPLE_INTERNAL_NAME {
                    if base_type_name(&normalized_scrutinee_ty) != TUPLE_INTERNAL_NAME {
                        self.diagnostics.push(Diagnostic::error(
                            "E1245",
                            format!(
                                "tuple pattern is not valid for scrutinee type '{}'",
                                scrutinee_ty
                            ),
                            self.file,
                            pattern.span,
                        ));
                        for arg in args {
                            self.check_pattern(arg, "<?>", locals, bound_names);
                        }
                        return;
                    }
                    let tuple_items =
                        extract_generic_args(&normalized_scrutinee_ty).unwrap_or_default();
                    if args.len() != tuple_items.len() {
                        self.diagnostics.push(Diagnostic::error(
                            "E1242",
                            format!(
                                "tuple pattern expects {} element(s), found {}",
                                tuple_items.len(),
                                args.len()
                            ),
                            self.file,
                            pattern.span,
                        ));
                    }
                    for (idx, arg) in args.iter().enumerate() {
                        let item_ty = tuple_items
                            .get(idx)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string());
                        self.check_pattern(arg, &item_ty, locals, bound_names);
                    }
                    return;
                }

                if normalized_scrutinee_ty.starts_with("Option[") {
                    match name.as_str() {
                        "None" => {
                            if !args.is_empty() {
                                self.diagnostics.push(Diagnostic::error(
                                    "E1237",
                                    "None pattern takes no payload",
                                    self.file,
                                    pattern.span,
                                ));
                                for arg in args {
                                    self.check_pattern(arg, "<?>", locals, bound_names);
                                }
                            }
                        }
                        "Some" => {
                            let inner = extract_generic_args(&normalized_scrutinee_ty)
                                .and_then(|mut v| {
                                    if v.len() == 1 {
                                        Some(v.remove(0))
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or_else(|| "<?>".to_string());
                            if args.len() != 1 {
                                self.diagnostics.push(Diagnostic::error(
                                    "E1238",
                                    "Some pattern takes one payload pattern",
                                    self.file,
                                    pattern.span,
                                ));
                            }
                            for arg in args {
                                self.check_pattern(arg, &inner, locals, bound_names);
                            }
                        }
                        _ => {
                            self.diagnostics.push(Diagnostic::error(
                                "E1239",
                                format!("unknown Option variant '{}'", name),
                                self.file,
                                pattern.span,
                            ));
                        }
                    }
                    return;
                }

                if normalized_scrutinee_ty.starts_with("Result[") {
                    match name.as_str() {
                        "Ok" | "Err" => {
                            let vars =
                                extract_generic_args(&normalized_scrutinee_ty).unwrap_or_default();
                            let payload_ty = if name == "Ok" {
                                vars.get(0).cloned().unwrap_or_else(|| "<?>".to_string())
                            } else {
                                vars.get(1).cloned().unwrap_or_else(|| "<?>".to_string())
                            };
                            if args.len() != 1 {
                                self.diagnostics.push(Diagnostic::error(
                                    "E1240",
                                    format!("{} pattern takes one payload pattern", name),
                                    self.file,
                                    pattern.span,
                                ));
                            }
                            for arg in args {
                                self.check_pattern(arg, &payload_ty, locals, bound_names);
                            }
                        }
                        _ => {
                            self.diagnostics.push(Diagnostic::error(
                                "E1241",
                                format!("unknown Result variant '{}'", name),
                                self.file,
                                pattern.span,
                            ));
                        }
                    }
                    return;
                }

                if let Some(enum_info) = self.find_enum(&normalized_scrutinee_ty).cloned() {
                    let enum_bindings =
                        bindings_from_applied_type(&normalized_scrutinee_ty, &enum_info.generics);
                    if enum_bindings.is_none() && !enum_info.generics.is_empty() {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1250",
                                format!(
                                    "generic arity mismatch for enum '{}': expected {} arguments",
                                    base_type_name(scrutinee_ty),
                                    enum_info.generics.len()
                                ),
                                self.file,
                                pattern.span,
                            )
                            .with_help("fix the generic arguments on the scrutinee type"),
                        );
                    }
                    let enum_bindings = enum_bindings.unwrap_or_default();
                    let enum_generic_set =
                        enum_info.generics.iter().cloned().collect::<BTreeSet<_>>();

                    if let Some(payload_ty_id) = enum_info.variants.get(name) {
                        if let Some(payload_ty_id) = payload_ty_id {
                            let payload_raw = self
                                .types
                                .get(payload_ty_id)
                                .cloned()
                                .unwrap_or_else(|| "<?>".to_string());
                            let payload = substitute_type_vars(
                                &payload_raw,
                                &enum_bindings,
                                &enum_generic_set,
                            );
                            if args.len() != 1 {
                                self.diagnostics.push(Diagnostic::error(
                                    "E1242",
                                    format!("variant '{}' takes one payload pattern", name),
                                    self.file,
                                    pattern.span,
                                ));
                            }
                            for arg in args {
                                self.check_pattern(arg, &payload, locals, bound_names);
                            }
                        } else if !args.is_empty() {
                            self.diagnostics.push(Diagnostic::error(
                                "E1243",
                                format!("variant '{}' takes no payload", name),
                                self.file,
                                pattern.span,
                            ));
                            for arg in args {
                                self.check_pattern(arg, "<?>", locals, bound_names);
                            }
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1244",
                            format!("unknown variant '{}' for enum '{}'", name, scrutinee_ty),
                            self.file,
                            pattern.span,
                        ));
                    }
                    return;
                }

                self.diagnostics.push(Diagnostic::error(
                    "E1245",
                    format!(
                        "variant pattern '{}' not valid for type '{}'",
                        name, scrutinee_ty
                    ),
                    self.file,
                    pattern.span,
                ));
            }
        }
    }

    fn record_pattern_coverage(
        &self,
        pattern: &ir::Pattern,
        scrutinee_ty: &str,
        seen: &mut BTreeSet<String>,
        wildcard_seen: &mut bool,
    ) {
        let normalized_scrutinee_ty = self.normalize_type(scrutinee_ty);
        match &pattern.kind {
            ir::PatternKind::Wildcard | ir::PatternKind::Var(_) => {
                *wildcard_seen = true;
            }
            ir::PatternKind::Int(v) => {
                if normalized_scrutinee_ty == "Int" {
                    seen.insert(format!("int:{v}"));
                }
            }
            ir::PatternKind::Bool(v) => {
                if normalized_scrutinee_ty == "Bool" {
                    seen.insert(if *v { "true" } else { "false" }.to_string());
                }
            }
            ir::PatternKind::Unit => {
                if normalized_scrutinee_ty == "()" {
                    seen.insert("()".to_string());
                }
            }
            ir::PatternKind::Or { patterns } => {
                for part in patterns {
                    self.record_pattern_coverage(part, scrutinee_ty, seen, wildcard_seen);
                }
            }
            ir::PatternKind::Variant { name, .. } => {
                if base_type_name(&normalized_scrutinee_ty) == TUPLE_INTERNAL_NAME
                    && name == TUPLE_INTERNAL_NAME
                {
                    return;
                }
                if normalized_scrutinee_ty.starts_with("Option[") {
                    if name == "None" || name == "Some" {
                        seen.insert(name.clone());
                    }
                    return;
                }
                if normalized_scrutinee_ty.starts_with("Result[") {
                    if name == "Ok" || name == "Err" {
                        seen.insert(name.clone());
                    }
                    return;
                }
                if let Some(enum_info) = self.find_enum(&normalized_scrutinee_ty) {
                    if enum_info.variants.contains_key(name) {
                        seen.insert(name.clone());
                    }
                }
            }
        }
    }

    fn check_exhaustive(
        &mut self,
        span: crate::span::Span,
        scrutinee_ty: &str,
        seen: &BTreeSet<String>,
        wildcard_seen: bool,
    ) {
        let normalized_scrutinee_ty = self.normalize_type(scrutinee_ty);
        if wildcard_seen {
            return;
        }

        if normalized_scrutinee_ty == "Bool" {
            let mut missing = Vec::new();
            if !seen.contains("true") {
                missing.push("true");
            }
            if !seen.contains("false") {
                missing.push("false");
            }
            if !missing.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1246",
                        format!("non-exhaustive bool match; missing: {}", missing.join(", ")),
                        self.file,
                        span,
                    )
                    .with_help("add missing `true` or `false` arm, or `_` wildcard"),
                );
            }
            return;
        }

        if normalized_scrutinee_ty.starts_with("Option[") {
            let mut missing = Vec::new();
            if !seen.contains("None") {
                missing.push("None");
            }
            if !seen.contains("Some") {
                missing.push("Some");
            }
            if !missing.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1247",
                        format!(
                            "non-exhaustive Option match; missing: {}",
                            missing.join(", ")
                        ),
                        self.file,
                        span,
                    )
                    .with_help("add both `None` and `Some(...)` arms, or `_` wildcard"),
                );
            }
            return;
        }

        if normalized_scrutinee_ty.starts_with("Result[") {
            let mut missing = Vec::new();
            if !seen.contains("Ok") {
                missing.push("Ok");
            }
            if !seen.contains("Err") {
                missing.push("Err");
            }
            if !missing.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1248",
                        format!(
                            "non-exhaustive Result match; missing: {}",
                            missing.join(", ")
                        ),
                        self.file,
                        span,
                    )
                    .with_help("add both `Ok(...)` and `Err(...)` arms, or `_` wildcard"),
                );
            }
            return;
        }

        if let Some(enum_info) = self.find_enum(&normalized_scrutinee_ty) {
            let missing = enum_info
                .variants
                .keys()
                .filter(|name| !seen.contains(*name))
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1249",
                        format!(
                            "non-exhaustive match for enum '{}'; missing: {}",
                            scrutinee_ty,
                            missing.join(", ")
                        ),
                        self.file,
                        span,
                    )
                    .with_help("add missing variant arms or `_` wildcard"),
                );
            }
        }
    }

    fn arm_is_redundant(
        &self,
        pattern: &ir::Pattern,
        scrutinee_ty: &str,
        seen: &BTreeSet<String>,
        wildcard_seen: bool,
    ) -> bool {
        let normalized_scrutinee_ty = self.normalize_type(scrutinee_ty);
        if wildcard_seen {
            return true;
        }

        match &pattern.kind {
            ir::PatternKind::Wildcard | ir::PatternKind::Var(_) => {
                self.coverage_is_complete(scrutinee_ty, seen, wildcard_seen)
            }
            ir::PatternKind::Int(v) => seen.contains(&format!("int:{v}")),
            ir::PatternKind::Bool(v) => seen.contains(if *v { "true" } else { "false" }),
            ir::PatternKind::Unit => seen.contains("()"),
            ir::PatternKind::Or { patterns } => patterns
                .iter()
                .all(|p| self.arm_is_redundant(p, scrutinee_ty, seen, wildcard_seen)),
            ir::PatternKind::Variant { name, .. } => {
                if base_type_name(&normalized_scrutinee_ty) == TUPLE_INTERNAL_NAME
                    && name == TUPLE_INTERNAL_NAME
                {
                    return false;
                }
                if normalized_scrutinee_ty.starts_with("Option[") {
                    return (name == "None" || name == "Some") && seen.contains(name);
                }
                if normalized_scrutinee_ty.starts_with("Result[") {
                    return (name == "Ok" || name == "Err") && seen.contains(name);
                }
                if self.find_enum(&normalized_scrutinee_ty).is_some() {
                    return seen.contains(name);
                }
                false
            }
        }
    }

    fn coverage_is_complete(
        &self,
        scrutinee_ty: &str,
        seen: &BTreeSet<String>,
        wildcard_seen: bool,
    ) -> bool {
        let normalized_scrutinee_ty = self.normalize_type(scrutinee_ty);
        if wildcard_seen {
            return true;
        }
        if normalized_scrutinee_ty == "Bool" {
            return seen.contains("true") && seen.contains("false");
        }
        if normalized_scrutinee_ty.starts_with("Option[") {
            return seen.contains("None") && seen.contains("Some");
        }
        if normalized_scrutinee_ty.starts_with("Result[") {
            return seen.contains("Ok") && seen.contains("Err");
        }
        if let Some(enum_info) = self.find_enum(&normalized_scrutinee_ty) {
            return enum_info.variants.keys().all(|name| seen.contains(name));
        }
        false
    }

    fn check_no_null_boundary(&mut self) {
        for symbol in &self.program.symbols {
            if symbol.name.eq_ignore_ascii_case("null") {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1253",
                        "null is not a valid language construct; use Option for absence",
                        self.file,
                        symbol.span,
                    )
                    .with_help("replace `null` with `None`/`Some(...)` modeling"),
                );
            }
        }
        for ty in &self.program.types {
            if contains_null_token(&ty.repr) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1253",
                        format!("type '{}' exposes forbidden null semantics", ty.repr),
                        self.file,
                        self.program.span,
                    )
                    .with_help("model absence with Option[T] only"),
                );
            }
        }
    }

    fn record_instantiation(
        &mut self,
        kind: ir::GenericInstantiationKind,
        name: &str,
        symbol: Option<ir::SymbolId>,
        type_args: &[String],
        span: crate::span::Span,
    ) {
        if type_args.is_empty() || type_args.iter().any(|arg| contains_unresolved_type(arg)) {
            return;
        }

        let kind_tag = instantiation_kind_tag(&kind);
        let key = format!("{}::{}[{}]", kind_tag, name, type_args.join(", "));
        let mangled = mangle_instantiation(kind_tag, name, type_args);

        if let Some(existing_key) = self.mangled_keys.get(&mangled) {
            if existing_key != &key {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1255",
                        format!(
                            "generic instantiation mangling collision: '{}' conflicts with '{}'",
                            key, existing_key
                        ),
                        self.file,
                        span,
                    )
                    .with_help("refine mangling strategy to keep instantiation keys unique"),
                );
                return;
            }
        } else {
            self.mangled_keys.insert(mangled.clone(), key.clone());
        }

        self.instantiation_seen
            .entry(key)
            .or_insert_with(|| PendingInstantiation {
                kind,
                name: name.to_string(),
                symbol,
                type_args: type_args.to_vec(),
                mangled,
            });
    }

    fn record_instantiation_from_applied_type(&mut self, ty: &str, span: crate::span::Span) {
        let Some(args) = extract_generic_args(ty) else {
            return;
        };
        if args.is_empty() || args.iter().any(|arg| contains_unresolved_type(arg)) {
            return;
        }

        for arg in &args {
            self.record_instantiation_from_applied_type(arg, span);
        }

        let base = base_type_name(ty);
        if base == "Option" || base == "Result" {
            self.record_instantiation(ir::GenericInstantiationKind::Enum, base, None, &args, span);
            return;
        }
        if let Some(info) = self.resolution.enums.get(base) {
            if info.generics.len() == args.len() {
                self.record_instantiation(
                    ir::GenericInstantiationKind::Enum,
                    base,
                    Some(info.symbol),
                    &args,
                    span,
                );
            }
            return;
        }
        if let Some(info) = self.resolution.structs.get(base) {
            if info.generics.len() == args.len() {
                self.record_instantiation(
                    ir::GenericInstantiationKind::Struct,
                    base,
                    Some(info.symbol),
                    &args,
                    span,
                );
            }
        }
    }

    fn extract_result_error_expected(&self, expected: &str) -> Option<String> {
        let expected_norm = self.normalize_type(expected);
        if base_type_name(&expected_norm) == "Result" {
            return extract_generic_args(&expected_norm).and_then(|args| args.get(1).cloned());
        }
        if base_type_name(&expected_norm) == "Async" {
            let inner =
                extract_generic_args(&expected_norm).and_then(|args| args.into_iter().next())?;
            if base_type_name(&inner) == "Result" {
                return extract_generic_args(&inner).and_then(|args| args.get(1).cloned());
            }
        }
        None
    }

    fn extract_expected_enum_name(&self, expected: &str) -> Option<String> {
        let normalized = self.normalize_type(expected);
        let base = base_type_name(&normalized);
        if self.resolution.enums.contains_key(base) {
            return Some(base.to_string());
        }
        None
    }

    fn find_variants(&self, name: &str) -> Vec<VariantMatch> {
        let mut out = Vec::new();
        for (enum_name, info) in &self.resolution.enums {
            if let Some(payload) = info.variants.get(name) {
                let payload_ty = payload.and_then(|id| self.types.get(&id).cloned());
                out.push(VariantMatch {
                    enum_name: enum_name.clone(),
                    generic_params: info.generics.clone(),
                    enum_symbol: info.symbol,
                    payload: payload_ty,
                });
            }
        }
        out
    }

    fn find_enum(&self, ty: &str) -> Option<&EnumInfo> {
        let normalized = self.normalize_type(ty);
        let base = base_type_name(&normalized);
        self.resolution.enums.get(base)
    }

    fn find_struct(&self, ty: &str) -> Option<&StructInfo> {
        let normalized = self.normalize_type(ty);
        let base = base_type_name(&normalized);
        self.resolution.structs.get(base)
    }

    fn normalize_type(&self, ty: &str) -> String {
        self.expand_aliases(ty, &mut BTreeSet::new())
    }

    fn expand_aliases(&self, ty: &str, visiting: &mut BTreeSet<String>) -> String {
        let base = base_type_name(ty);
        let normalized_args = extract_generic_args(ty).map(|args| {
            args.iter()
                .map(|arg| self.expand_aliases(arg, visiting))
                .collect::<Vec<_>>()
        });

        let Some(alias) = self.type_aliases.get(base) else {
            return if let Some(args) = normalized_args {
                format!("{base}[{}]", args.join(", "))
            } else {
                ty.to_string()
            };
        };

        if !visiting.insert(base.to_string()) {
            return ty.to_string();
        }

        let expanded = if alias.generics.is_empty() {
            self.expand_aliases(&alias.target, visiting)
        } else if let Some(args) = normalized_args {
            if args.len() != alias.generics.len() {
                format!("{base}[{}]", args.join(", "))
            } else {
                let mut bindings = BTreeMap::new();
                for (generic, arg) in alias.generics.iter().zip(args.iter()) {
                    bindings.insert(generic.clone(), arg.clone());
                }
                let generic_set = alias.generics.iter().cloned().collect::<BTreeSet<_>>();
                let substituted = substitute_type_vars(&alias.target, &bindings, &generic_set);
                self.expand_aliases(&substituted, visiting)
            }
        } else {
            ty.to_string()
        };

        visiting.remove(base);
        expanded
    }

    fn types_compatible(&self, expected: &str, found: &str) -> bool {
        let expected_norm = self.normalize_type(expected);
        let found_norm = self.normalize_type(found);
        type_compatible(&expected_norm, &found_norm)
    }

    fn merge_compatible_types(&self, left: &str, right: &str) -> String {
        if left == right {
            return left.to_string();
        }
        if !self.types_compatible(left, right) {
            return "<?>".to_string();
        }

        if !contains_unresolved_type(left) {
            return left.to_string();
        }
        if !contains_unresolved_type(right) {
            return right.to_string();
        }

        let left_norm = self.normalize_type(left);
        let right_norm = self.normalize_type(right);
        merge_types(&left_norm, &right_norm)
    }
}

fn type_compatible(expected: &str, found: &str) -> bool {
    if expected == found
        || expected == "<?>"
        || found == "<?>"
        || (expected.starts_with("Option[")
            && found.starts_with("Option[")
            && (expected.contains("<?>") || found.contains("<?>")))
        || (expected.starts_with("Result[")
            && found.starts_with("Result[")
            && (expected.contains("<?>") || found.contains("<?>")))
        || (expected.starts_with("Async[")
            && found.starts_with("Async[")
            && (expected.contains("<?>") || found.contains("<?>")))
    {
        return true;
    }

    let expected_args = extract_generic_args(expected).unwrap_or_default();
    let found_args = extract_generic_args(found).unwrap_or_default();
    if expected_args.is_empty() || found_args.is_empty() {
        return false;
    }
    if base_type_name(expected) != base_type_name(found) || expected_args.len() != found_args.len()
    {
        return false;
    }
    expected_args
        .iter()
        .zip(found_args.iter())
        .all(|(exp, got)| type_compatible(exp, got))
}

fn parse_fn_type(ty: &str) -> Option<(Vec<String>, String)> {
    if base_type_name(ty) != "Fn" {
        return None;
    }
    let args = extract_generic_args(ty)?;
    if args.is_empty() {
        return None;
    }
    let mut params = args;
    let ret = params.pop()?;
    Some((params, ret))
}

fn render_fn_type(params: &[String], ret: &str) -> String {
    let mut args = params.to_vec();
    args.push(ret.to_string());
    format!("Fn[{}]", args.join(", "))
}

fn instantiation_kind_tag(kind: &ir::GenericInstantiationKind) -> &'static str {
    match kind {
        ir::GenericInstantiationKind::Function => "fn",
        ir::GenericInstantiationKind::Struct => "struct",
        ir::GenericInstantiationKind::Enum => "enum",
    }
}

fn mangle_instantiation(kind_tag: &str, name: &str, type_args: &[String]) -> String {
    let mut out = String::new();
    out.push_str(kind_tag);
    out.push('_');
    out.push_str(&mangle_component(name));
    for arg in type_args {
        out.push('_');
        out.push_str(&mangle_component(arg));
    }
    out
}

fn mangle_component(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '_' => out.push(ch),
            '[' => out.push_str("_lb_"),
            ']' => out.push_str("_rb_"),
            ',' => out.push_str("_c_"),
            ' ' => {}
            other => out.push_str(&format!("_x{:02X}_", other as u32)),
        }
    }
    out
}

fn contains_null_token(input: &str) -> bool {
    input
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|segment| segment.eq_ignore_ascii_case("null"))
}

fn concurrent_protocol_op(name: &str) -> Option<ResourceProtocolOp> {
    match name {
        "send_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntChannel,
            terminal: false,
            api: "send_int",
        }),
        "recv_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntChannel,
            terminal: false,
            api: "recv_int",
        }),
        "close_channel" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntChannel,
            terminal: true,
            api: "close_channel",
        }),
        "lock_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntMutex,
            terminal: false,
            api: "lock_int",
        }),
        "unlock_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntMutex,
            terminal: false,
            api: "unlock_int",
        }),
        "close_mutex" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntMutex,
            terminal: true,
            api: "close_mutex",
        }),
        "join_task" => Some(ResourceProtocolOp {
            kind: ResourceKind::Task,
            terminal: true,
            api: "join_task",
        }),
        "cancel_task" => Some(ResourceProtocolOp {
            kind: ResourceKind::Task,
            terminal: true,
            api: "cancel_task",
        }),
        _ => None,
    }
}

fn clear_resource_state_for_var(name: &str, state: &mut ResourceStateMap) {
    state.retain(|(var, _), _| var != name);
}

fn contains_unresolved_type(ty: &str) -> bool {
    ty.contains("<?>")
}

fn is_symbolic_generic_name(ty: &str) -> bool {
    if ty.len() > 4 || ty.contains('[') || ty.contains(']') || ty.contains(',') || ty.contains('.')
    {
        return false;
    }
    ty.chars().all(|ch| ch.is_ascii_uppercase())
}

fn contains_symbolic_generic_type(ty: &str) -> bool {
    if is_symbolic_generic_name(ty) {
        return true;
    }
    extract_generic_args(ty)
        .map(|args| args.iter().any(|arg| contains_symbolic_generic_type(arg)))
        .unwrap_or(false)
}

fn infer_generic_bindings(
    expected: &str,
    found: &str,
    generic_params: &BTreeSet<String>,
    bindings: &mut BTreeMap<String, String>,
) -> bool {
    if generic_params.contains(expected) {
        if let Some(bound) = bindings.get(expected).cloned() {
            if contains_unresolved_type(&bound) && !contains_unresolved_type(found) {
                bindings.insert(expected.to_string(), found.to_string());
                return true;
            }
            if contains_unresolved_type(found) && !contains_unresolved_type(&bound) {
                return true;
            }
            if type_compatible(&bound, found) {
                bindings.insert(expected.to_string(), merge_types(&bound, found));
                return true;
            }
            return false;
        }
        bindings.insert(expected.to_string(), found.to_string());
        return true;
    }

    let expected_args = extract_generic_args(expected).unwrap_or_default();
    let found_args = extract_generic_args(found).unwrap_or_default();
    if expected_args.is_empty() || found_args.is_empty() {
        return type_compatible(expected, found);
    }

    if base_type_name(expected) != base_type_name(found) || expected_args.len() != found_args.len()
    {
        return false;
    }

    for (expected_arg, found_arg) in expected_args.iter().zip(found_args.iter()) {
        if !infer_generic_bindings(expected_arg, found_arg, generic_params, bindings) {
            return false;
        }
    }
    true
}

fn substitute_type_vars(
    ty: &str,
    bindings: &BTreeMap<String, String>,
    generic_params: &BTreeSet<String>,
) -> String {
    if generic_params.contains(ty) {
        return bindings
            .get(ty)
            .cloned()
            .unwrap_or_else(|| "<?>".to_string());
    }

    let Some(args) = extract_generic_args(ty) else {
        return ty.to_string();
    };

    let substituted = args
        .iter()
        .map(|arg| substitute_type_vars(arg, bindings, generic_params))
        .collect::<Vec<_>>();
    format!("{}[{}]", base_type_name(ty), substituted.join(", "))
}

fn bindings_from_applied_type(
    applied_ty: &str,
    generic_params: &[String],
) -> Option<BTreeMap<String, String>> {
    if generic_params.is_empty() {
        return Some(BTreeMap::new());
    }

    let args = extract_generic_args(applied_ty)?;
    if args.len() != generic_params.len() {
        return None;
    }

    let mut out = BTreeMap::new();
    for (param, arg) in generic_params.iter().zip(args.into_iter()) {
        out.insert(param.clone(), arg);
    }
    Some(out)
}

fn merge_types(a: &str, b: &str) -> String {
    if a == b {
        return a.to_string();
    }
    if a == "<?>" {
        return b.to_string();
    }
    if b == "<?>" {
        return a.to_string();
    }

    if a.starts_with("Option[") && b.starts_with("Option[") {
        let args_a = extract_generic_args(a).unwrap_or_default();
        let args_b = extract_generic_args(b).unwrap_or_default();
        if args_a.len() == 1 && args_b.len() == 1 {
            return format!("Option[{}]", merge_types(&args_a[0], &args_b[0]));
        }
    }

    if a.starts_with("Result[") && b.starts_with("Result[") {
        let args_a = extract_generic_args(a).unwrap_or_default();
        let args_b = extract_generic_args(b).unwrap_or_default();
        if args_a.len() == 2 && args_b.len() == 2 {
            return format!(
                "Result[{}, {}]",
                merge_types(&args_a[0], &args_b[0]),
                merge_types(&args_a[1], &args_b[1])
            );
        }
    }

    if a.starts_with("Async[") && b.starts_with("Async[") {
        let args_a = extract_generic_args(a).unwrap_or_default();
        let args_b = extract_generic_args(b).unwrap_or_default();
        if args_a.len() == 1 && args_b.len() == 1 {
            return format!("Async[{}]", merge_types(&args_a[0], &args_b[0]));
        }
    }

    let args_a = extract_generic_args(a).unwrap_or_default();
    let args_b = extract_generic_args(b).unwrap_or_default();
    if !args_a.is_empty()
        && !args_b.is_empty()
        && base_type_name(a) == base_type_name(b)
        && args_a.len() == args_b.len()
    {
        let merged = args_a
            .iter()
            .zip(args_b.iter())
            .map(|(left, right)| merge_types(left, right))
            .collect::<Vec<_>>();
        return format!("{}[{}]", base_type_name(a), merged.join(", "));
    }

    "<?>".to_string()
}

fn base_type_name(ty: &str) -> &str {
    ty.split('[').next().unwrap_or(ty)
}

fn binding_root(target: &str) -> &str {
    target.split('.').next().unwrap_or(target)
}

fn targets_overlap(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .map(|suffix| suffix.starts_with('.'))
            .unwrap_or(false)
        || right
            .strip_prefix(left)
            .map(|suffix| suffix.starts_with('.'))
            .unwrap_or(false)
}

fn extract_generic_args(ty: &str) -> Option<Vec<String>> {
    let start = ty.find('[')?;
    let end = ty.rfind(']')?;
    if end <= start {
        return None;
    }
    let inner = &ty[start + 1..end];
    Some(split_top_level(inner))
}

fn is_c_abi_compatible_type(ty: &str) -> bool {
    if ty == "()" {
        return true;
    }
    if contains_unresolved_type(ty) {
        return false;
    }
    matches!(base_type_name(ty), "Int" | "Bool" | "Float") && extract_generic_args(ty).is_none()
}

fn is_ident_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
}

fn split_top_level(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0_i32;
    let mut current = String::new();
    for c in input.chars() {
        match c {
            '[' => {
                depth += 1;
                current.push(c);
            }
            ']' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::diagnostics::Severity;
    use crate::{ir_builder::build, parser::parse, resolver::resolve};

    use super::{check, extract_generic_args, merge_types, split_top_level};

    #[test]
    fn option_match_exhaustive() {
        let src = r#"
fn f(x: Option[Int]) -> Int {
    match x {
        None => 0,
        Some(v) => v,
    }
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty());
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty());
        let out = check(&ir, &res, "test.aic");
        assert!(out.diagnostics.is_empty());
    }

    #[test]
    fn detects_missing_effect() {
        let src = r#"
import std.io;
fn io_fn() -> () effects { io } {
    print_int(1)
}
fn pure_fn() -> () {
    io_fn()
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty());
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty());
        let out = check(&ir, &res, "test.aic");
        assert!(out.diagnostics.iter().any(|d| d.code == "E2001"));
    }

    #[test]
    fn reports_transitive_effect_path() {
        let src = r#"
import std.io;
fn leaf() -> () effects { io } {
    print_int(1)
}
fn middle() -> () {
    leaf()
}
fn top() -> () {
    middle()
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty());
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty());
        let out = check(&ir, &res, "test.aic");
        let diag = out
            .diagnostics
            .iter()
            .find(|d| d.code == "E2005")
            .expect("missing transitive effect diagnostic");
        assert!(
            diag.message.contains("top -> middle -> leaf"),
            "message={}",
            diag.message
        );
        assert_eq!(
            out.function_effect_usage.get("top"),
            Some(&BTreeSet::from(["io".to_string()]))
        );
    }

    #[test]
    fn resource_protocol_accepts_valid_channel_sequence() {
        let src = r#"
enum ConcurrencyError { Closed }
struct IntChannel { handle: Int }
fn send_int(ch: IntChannel, value: Int, timeout_ms: Int) -> Result[Bool, ConcurrencyError] effects { concurrency } { Ok(true) }
fn recv_int(ch: IntChannel, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency } { Ok(0) }
fn close_channel(ch: IntChannel) -> Result[Bool, ConcurrencyError] effects { concurrency } { Ok(true) }

fn main() -> Int effects { concurrency } {
    let ch = IntChannel { handle: 1 };
    let _sent = send_int(ch, 1, 100);
    let _recv = recv_int(ch, 100);
    let _closed = close_channel(ch);
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={:#?}", d1);
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={:#?}", d2);
        let out = check(&ir, &res, "test.aic");
        assert!(
            !out.diagnostics.iter().any(|d| d.code == "E2006"),
            "diags={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn resource_protocol_reports_use_after_close() {
        let src = r#"
enum ConcurrencyError { Closed }
struct IntChannel { handle: Int }
fn send_int(ch: IntChannel, value: Int, timeout_ms: Int) -> Result[Bool, ConcurrencyError] effects { concurrency } { Ok(true) }
fn close_channel(ch: IntChannel) -> Result[Bool, ConcurrencyError] effects { concurrency } { Ok(true) }

fn main() -> Int effects { concurrency } {
    let ch = IntChannel { handle: 1 };
    let _closed = close_channel(ch);
    let _sent = send_int(ch, 7, 50);
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={:#?}", d1);
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={:#?}", d2);
        let out = check(&ir, &res, "test.aic");
        let diag = out
            .diagnostics
            .iter()
            .find(|d| d.code == "E2006")
            .expect("missing E2006 protocol diagnostic");
        assert!(
            diag.message.contains("send_int") && diag.message.contains("closed IntChannel"),
            "message={}",
            diag.message
        );
    }

    #[test]
    fn resource_protocol_checker_avoids_branch_false_positive() {
        let src = r#"
enum ConcurrencyError { Closed }
struct IntChannel { handle: Int }
fn send_int(ch: IntChannel, value: Int, timeout_ms: Int) -> Result[Bool, ConcurrencyError] effects { concurrency } { Ok(true) }
fn close_channel(ch: IntChannel) -> Result[Bool, ConcurrencyError] effects { concurrency } { Ok(true) }

fn maybe_close(ch: IntChannel, flag: Bool) -> Int effects { concurrency } {
    if flag {
        let _closed = close_channel(ch);
    } else {
        ()
    };
    let _sent = send_int(ch, 1, 25);
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={:#?}", d1);
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={:#?}", d2);
        let out = check(&ir, &res, "test.aic");
        assert!(
            !out.diagnostics.iter().any(|d| d.code == "E2006"),
            "diags={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn parse_generic_args_nested() {
        let args = extract_generic_args("Result[Option[Int], Result[Int, Bool]]").unwrap();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "Option[Int]");
    }

    #[test]
    fn merge_option_types() {
        assert_eq!(merge_types("Option[Int]", "Option[<?>]"), "Option[Int]");
    }

    #[test]
    fn split_top_level_works() {
        let parts = split_top_level("Int, Option[Int], Result[Int, Bool]");
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn infers_annotation_free_let_binding() {
        let src = r#"
fn f() -> Int {
    let x = 41;
    x + 1
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty());
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty());
        let out = check(&ir, &res, "test.aic");
        assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
    }

    #[test]
    fn ambiguous_none_binding_requires_annotation() {
        let src = r#"
fn f() -> Int {
    let x = None;
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty());
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty());
        let out = check(&ir, &res, "test.aic");
        assert!(out
            .diagnostics
            .iter()
            .any(|d| { d.code == "E1204" && d.message.contains("cannot infer concrete type") }));
    }

    #[test]
    fn propagates_block_tail_type_in_let() {
        let src = r#"
fn f(flag: Bool) -> Int {
    let x = if flag { 1 } else { 2 };
    x
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty());
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty());
        let out = check(&ir, &res, "test.aic");
        assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
    }

    #[test]
    fn typed_holes_warn_and_infer_supported_positions() {
        let src = r#"
struct Counter {
    value: _,
}

fn plus_one(x: _) -> _ {
    let y: _ = x + 1;
    y
}

fn main() -> Int {
    let counter = Counter { value: 41 };
    let out: _ = plus_one(counter.value);
    out
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");

        assert!(
            !out.diagnostics
                .iter()
                .any(|diag| matches!(diag.severity, Severity::Error)),
            "unexpected errors: {:#?}",
            out.diagnostics
        );
        assert!(
            out.diagnostics
                .iter()
                .any(|diag| diag.code == "E6003" && matches!(diag.severity, Severity::Warning)),
            "expected typed-hole warning E6003, got: {:#?}",
            out.diagnostics
        );
        assert_eq!(out.holes.len(), 5, "holes={:#?}", out.holes);
        assert!(out
            .holes
            .iter()
            .any(|hole| hole.context == "struct field 'Counter.value'" && hole.inferred == "Int"));
        assert!(out.holes.iter().any(|hole| {
            hole.context == "parameter 'x' in function 'plus_one'" && hole.inferred == "Int"
        }));
        assert!(out
            .holes
            .iter()
            .any(|hole| hole.context == "return type in function 'plus_one'"
                && hole.inferred == "Int"));
        assert!(out
            .holes
            .iter()
            .any(|hole| hole.context == "let binding 'y'" && hole.inferred == "Int"));
        assert!(out
            .holes
            .iter()
            .any(|hole| hole.context == "let binding 'out'" && hole.inferred == "Int"));
    }
}
