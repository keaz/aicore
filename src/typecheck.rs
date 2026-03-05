use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::ast::{decode_internal_const, decode_internal_type_alias, BinOp, Visibility};
use crate::diagnostics::{Diagnostic, DiagnosticSpan, SuggestedFix};
use crate::ir;
use crate::resolver::{EnumInfo, FunctionInfo, Resolution, StructInfo, TraitInfo};
use crate::std_policy::find_deprecated_api;

const TUPLE_INTERNAL_NAME: &str = "Tuple";
const FIXED_WIDTH_INTEGER_PRIMITIVES: [&str; 13] = [
    "Int8", "Int16", "Int32", "Int64", "Int128", "UInt8", "UInt16", "UInt32", "UInt64", "UInt128",
    "ISize", "USize", "UInt",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntegerKind {
    Int,
    ISize,
    USize,
    Fixed { signed: bool, bits: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FloatKind {
    F32,
    F64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntegerLiteralValue {
    NonNegative(u128),
    NegativeMagnitude(u128),
}

#[derive(Debug, Clone, Default)]
pub struct TypecheckOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub function_effect_usage: BTreeMap<String, BTreeSet<String>>,
    pub function_effect_reasons: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub generic_instantiations: Vec<ir::GenericInstantiation>,
    pub call_graph: BTreeMap<String, Vec<String>>,
    pub holes: Vec<TypedHole>,
    pub call_arg_orders: BTreeMap<ir::NodeId, Vec<usize>>,
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
    param_names: Vec<String>,
    ret: String,
    effects: BTreeSet<String>,
    capabilities: BTreeSet<String>,
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
    current_module: Option<String>,
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
    call_arg_orders: BTreeMap<ir::NodeId, Vec<usize>>,
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
    Sender,
    Receiver,
    IntChannel,
    IntMutex,
    IntRwLock,
    FileHandle,
    TcpHandle,
    UdpHandle,
    TlsStream,
    ProcessHandle,
    AsyncIntOp,
    AsyncStringOp,
}

impl ResourceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Task => "Task",
            Self::Sender => "Sender",
            Self::Receiver => "Receiver",
            Self::IntChannel => "IntChannel",
            Self::IntMutex => "IntMutex",
            Self::IntRwLock => "IntRwLock",
            Self::FileHandle => "FileHandle",
            Self::TcpHandle => "TcpHandle",
            Self::UdpHandle => "UdpHandle",
            Self::TlsStream => "TlsStream",
            Self::ProcessHandle => "ProcessHandle",
            Self::AsyncIntOp => "AsyncIntOp",
            Self::AsyncStringOp => "AsyncStringOp",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ResourceState {
    closed_at: crate::span::Span,
    closed_by: &'static str,
}

type ResourceStateMap = BTreeMap<(String, ResourceKind), ResourceState>;
type ResourceBindingTypeMap = BTreeMap<String, String>;

#[derive(Debug, Clone, Copy)]
struct ResourceProtocolOp {
    kind: ResourceKind,
    terminal: bool,
    api: &'static str,
    first_param_base_type: &'static str,
    required_effect: &'static str,
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
                    param_names: info.param_names.clone(),
                    ret: types
                        .get(&info.ret_type)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string()),
                    effects: info.effects.clone(),
                    capabilities: info.capabilities.clone(),
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
                    param_names: info.param_names.clone(),
                    ret: types
                        .get(&info.ret_type)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string()),
                    effects: info.effects.clone(),
                    capabilities: info.capabilities.clone(),
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
                param_names: vec!["value".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                capabilities: BTreeSet::from(["io".to_string()]),
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
                param_names: vec!["value".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                capabilities: BTreeSet::from(["io".to_string()]),
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
                param_names: vec!["value".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                capabilities: BTreeSet::from(["io".to_string()]),
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
                param_names: vec!["value".to_string()],
                ret: "Int".to_string(),
                effects: BTreeSet::new(),
                capabilities: BTreeSet::new(),
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
                param_names: vec!["message".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                capabilities: BTreeSet::from(["io".to_string()]),
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
            current_module: None,
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
            call_arg_orders: BTreeMap::new(),
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
        self.check_capability_authority();
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
            call_arg_orders: self.call_arg_orders,
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
        let context = format!("const '{}' initializer", const_name);
        self.validate_compile_time_expr(&context, expr);
    }

    fn validate_compile_time_expr(&mut self, context: &str, expr: &ir::Expr) {
        match &expr.kind {
            ir::ExprKind::Int(_)
            | ir::ExprKind::Float(_)
            | ir::ExprKind::Bool(_)
            | ir::ExprKind::Char(_)
            | ir::ExprKind::String(_)
            | ir::ExprKind::Unit => {}
            ir::ExprKind::Var(name) => {
                if !self.const_types.contains_key(name) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1287",
                            format!(
                                "{} can only reference other constants, found '{}'",
                                context, name
                            ),
                            self.file,
                            expr.span,
                        )
                        .with_help("reference a previously declared const symbol"),
                    );
                }
            }
            ir::ExprKind::Unary { op, expr: inner } => match op {
                crate::ast::UnaryOp::Neg
                | crate::ast::UnaryOp::Not
                | crate::ast::UnaryOp::BitNot => {
                    self.validate_compile_time_expr(context, inner);
                }
            },
            ir::ExprKind::Binary { lhs, rhs, .. } => {
                self.validate_compile_time_expr(context, lhs);
                self.validate_compile_time_expr(context, rhs);
            }
            ir::ExprKind::Call { .. } => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1287",
                        format!("{} cannot call functions at compile time", context),
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
                            "{} only supports literals, unary/binary operators, and const references",
                            context
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
        let previous_function = self.current_function.clone();
        let previous_module = self.current_module.clone();
        let previous_async = self.current_function_is_async;
        let previous_unsafe = self.current_function_is_unsafe;
        let previous_ret = self.current_function_ret_type.clone();
        let previous_unsafe_depth = self.unsafe_depth;
        self.enforce_import_visibility = self.should_enforce_import_visibility(func.symbol);
        self.current_module = self
            .function_module_by_symbol
            .get(&func.symbol)
            .cloned()
            .or_else(|| self.resolution.entry_module.clone())
            .or_else(|| Some("<root>".to_string()));
        let current_function_key = self.current_function_key_for(&func.name);
        self.current_function = Some(current_function_key.clone());
        self.current_function_is_async = func.is_async;
        self.current_function_is_unsafe = func.is_unsafe;
        self.unsafe_depth = 0;
        self.call_graph
            .entry(current_function_key.clone())
            .or_default();
        self.function_spans
            .insert(current_function_key.clone(), (func.span, func.body.span));
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
            self.effect_usage
                .insert(current_function_key.clone(), BTreeSet::new());
            self.current_param_positions.clear();
            self.enforce_import_visibility = previous_enforce;
            self.current_function = previous_function;
            self.current_module = previous_module;
            self.current_function_is_async = previous_async;
            self.current_function_is_unsafe = previous_unsafe;
            self.current_function_ret_type = previous_ret;
            self.unsafe_depth = previous_unsafe_depth;
            return;
        }

        if func.is_intrinsic {
            self.check_intrinsic_function_signature(func);
            self.effect_usage
                .insert(current_function_key.clone(), BTreeSet::new());
            self.current_param_positions.clear();
            self.enforce_import_visibility = previous_enforce;
            self.current_function = previous_function;
            self.current_module = previous_module;
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
            .insert(current_function_key.clone(), body_ctx.effects_used);

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
        self.current_module = previous_module;
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

    fn module_matches_current(&self, module: &str) -> bool {
        self.current_module.as_deref() == Some(module)
    }

    fn current_module_name(&self) -> String {
        self.current_module
            .clone()
            .or_else(|| self.resolution.entry_module.clone())
            .unwrap_or_else(|| "<root>".to_string())
    }

    fn imports_for_current_module(&self) -> &BTreeSet<String> {
        let module = self.current_module_name();
        self.resolution
            .module_imports
            .get(&module)
            .unwrap_or(&self.resolution.imports)
    }

    fn module_aliases_for_current_module(&self) -> Option<&BTreeMap<String, String>> {
        let module = self.current_module_name();
        self.resolution.module_import_aliases.get(&module)
    }

    fn ambiguous_aliases_for_current_module(&self) -> Option<&BTreeSet<String>> {
        let module = self.current_module_name();
        self.resolution.module_ambiguous_import_aliases.get(&module)
    }

    fn module_is_imported_in_current_scope(&self, module: &str) -> bool {
        self.module_matches_current(module) || self.imports_for_current_module().contains(module)
    }

    fn qualified_function_key(module: &str, name: &str) -> String {
        if module == "<root>" {
            name.to_string()
        } else {
            format!("{module}::{name}")
        }
    }

    fn current_function_key_for(&self, name: &str) -> String {
        let module = self.current_module.as_deref().unwrap_or("<root>");
        Self::qualified_function_key(module, name)
    }

    fn fn_sig_for_key(&self, key: &str) -> Option<&FnSig> {
        if let Some((module, name)) = key.rsplit_once("::") {
            return self
                .module_functions
                .get(&(module.to_string(), name.to_string()))
                .or_else(|| self.functions.get(name));
        }
        self.functions.get(key)
    }

    fn resolution_has_function_key(&self, key: &str) -> bool {
        if let Some((module, name)) = key.rsplit_once("::") {
            return self
                .resolution
                .module_function_infos
                .contains_key(&(module.to_string(), name.to_string()));
        }
        self.resolution.functions.contains_key(key)
    }

    fn visibility_allows_cross_module_access(&self, visibility: Visibility) -> bool {
        !matches!(visibility, Visibility::Private)
    }

    fn function_is_accessible_from_current(&self, module: &str, info: &FunctionInfo) -> bool {
        self.module_matches_current(module)
            || self.visibility_allows_cross_module_access(info.visibility)
    }

    fn field_is_accessible_from_current(&self, owner_module: &str, visibility: Visibility) -> bool {
        self.module_matches_current(owner_module)
            || self.visibility_allows_cross_module_access(visibility)
    }

    fn is_user_written_intrinsic_use(&self, name: &str, span: crate::span::Span) -> bool {
        if !name.starts_with("aic_") {
            return false;
        }
        if self
            .current_module
            .as_deref()
            .map(|module| module == "std" || module.starts_with("std."))
            .unwrap_or(false)
        {
            return false;
        }
        let Some(source) = self.source.as_ref() else {
            return true;
        };
        if span.end > source.len() || span.start >= span.end {
            return true;
        }
        source[span.start..span.end].contains(name)
    }

    fn is_compiler_internal_intrinsic_use(&self, name: &str, span: crate::span::Span) -> bool {
        name.starts_with("aic_") && !self.is_user_written_intrinsic_use(name, span)
    }

    fn has_named_call_args(arg_names: &[Option<String>]) -> bool {
        !arg_names.is_empty() && arg_names.iter().any(|name| name.is_some())
    }

    fn call_arg_name<'b>(arg_names: &'b [Option<String>], idx: usize) -> Option<&'b str> {
        if idx < arg_names.len() {
            return arg_names.get(idx).and_then(|name| name.as_deref());
        }
        None
    }

    fn levenshtein_distance(a: &str, b: &str) -> usize {
        let a_chars = a.chars().collect::<Vec<_>>();
        let b_chars = b.chars().collect::<Vec<_>>();
        let mut prev = (0..=b_chars.len()).collect::<Vec<_>>();
        let mut curr = vec![0usize; b_chars.len() + 1];

        for (i, ca) in a_chars.iter().enumerate() {
            curr[0] = i + 1;
            for (j, cb) in b_chars.iter().enumerate() {
                let cost = if ca == cb { 0 } else { 1 };
                curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
            }
            std::mem::swap(&mut prev, &mut curr);
        }
        prev[b_chars.len()]
    }

    fn closest_parameter_name<'b>(
        &self,
        candidate: &str,
        param_names: &'b [String],
    ) -> Option<&'b str> {
        param_names
            .iter()
            .map(|name| (name.as_str(), Self::levenshtein_distance(candidate, name)))
            .min_by_key(|(_, distance)| *distance)
            .and_then(|(name, distance)| if distance <= 3 { Some(name) } else { None })
    }

    fn build_call_arg_plan(
        &mut self,
        rendered_path: &str,
        param_names: &[String],
        args: &[ir::Expr],
        arg_names: &[Option<String>],
        span: crate::span::Span,
    ) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
        let mut param_to_arg = vec![None; param_names.len()];
        let mut arg_to_param = vec![None; args.len()];

        if !Self::has_named_call_args(arg_names) {
            for idx in 0..args.len().min(param_names.len()) {
                param_to_arg[idx] = Some(idx);
                arg_to_param[idx] = Some(idx);
            }
            return (param_to_arg, arg_to_param);
        }

        let mut saw_named = false;
        for arg_idx in 0..args.len() {
            let named = Self::call_arg_name(arg_names, arg_idx);
            match named {
                Some(name) => {
                    saw_named = true;
                    let Some(param_idx) = param_names.iter().position(|param| param == name) else {
                        let mut diag = Diagnostic::error(
                            "E1213",
                            format!(
                                "unknown named argument '{}' in call to '{}'",
                                name, rendered_path
                            ),
                            self.file,
                            args[arg_idx].span,
                        )
                        .with_help(format!("valid parameter names: {}", param_names.join(", ")));
                        if let Some(suggested) = self.closest_parameter_name(name, param_names) {
                            diag = diag.with_help(format!("did you mean '{}' ?", suggested));
                        }
                        self.diagnostics.push(diag);
                        continue;
                    };

                    if let Some(previous_arg) = param_to_arg[param_idx] {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1213",
                                format!(
                                    "parameter '{}' is provided more than once in call to '{}'",
                                    name, rendered_path
                                ),
                                self.file,
                                args[arg_idx].span,
                            )
                            .with_help(format!(
                                "remove duplicate assignment for parameter '{}' (first provided at argument {})",
                                name,
                                previous_arg + 1
                            )),
                        );
                        continue;
                    }

                    param_to_arg[param_idx] = Some(arg_idx);
                    arg_to_param[arg_idx] = Some(param_idx);
                }
                None => {
                    if saw_named {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1092",
                                "positional arguments cannot follow named arguments",
                                self.file,
                                args[arg_idx].span,
                            )
                            .with_help("place positional arguments first, then named arguments"),
                        );
                        continue;
                    }
                    if arg_idx < param_names.len() {
                        param_to_arg[arg_idx] = Some(arg_idx);
                        arg_to_param[arg_idx] = Some(arg_idx);
                    }
                }
            }
        }

        for (param_idx, mapped) in param_to_arg.iter().enumerate() {
            if mapped.is_none() && args.len() <= param_names.len() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1213",
                        format!(
                            "missing argument for parameter '{}' in call to '{}'",
                            param_names[param_idx], rendered_path
                        ),
                        self.file,
                        span,
                    )
                    .with_help(format!("provide a value for '{}'", param_names[param_idx])),
                );
            }
        }

        (param_to_arg, arg_to_param)
    }

    fn maybe_record_named_call_order(
        &mut self,
        node: ir::NodeId,
        args_len: usize,
        arg_names: &[Option<String>],
        param_to_arg: &[Option<usize>],
    ) {
        if !Self::has_named_call_args(arg_names) {
            return;
        }
        if args_len != param_to_arg.len() {
            return;
        }
        let mut order = Vec::with_capacity(param_to_arg.len());
        for mapped in param_to_arg {
            let Some(arg_idx) = mapped else {
                return;
            };
            order.push(*arg_idx);
        }
        self.call_arg_orders.insert(node, order);
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
            || !func.capabilities.is_empty()
            || func.requires.is_some()
            || func.ensures.is_some()
        {
            self.diagnostics.push(
                Diagnostic::error(
                    "E2121",
                    format!(
                        "extern function '{}' must be a plain signature without async/generics/effects/capabilities/contracts",
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

    fn check_intrinsic_function_signature(&mut self, func: &ir::Function) {
        match func.intrinsic_abi.as_deref() {
            Some("runtime") => {}
            Some(other) => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E2121",
                        format!(
                            "unsupported intrinsic ABI '{}' on function '{}'",
                            other, func.name
                        ),
                        self.file,
                        func.span,
                    )
                    .with_help("use `intrinsic fn ...;` declarations with the default runtime ABI"),
                );
            }
            None => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E2121",
                        format!(
                            "intrinsic function '{}' is missing runtime ABI metadata",
                            func.name
                        ),
                        self.file,
                        func.span,
                    )
                    .with_help("rebuild IR from source so intrinsic ABI metadata is encoded"),
                );
            }
        }

        if func.is_async
            || !func.capabilities.is_empty()
            || func.requires.is_some()
            || func.ensures.is_some()
        {
            self.diagnostics.push(
                Diagnostic::error(
                    "E2121",
                    format!(
                        "intrinsic function '{}' must be a plain declaration without async/capabilities/contracts",
                        func.name
                    ),
                    self.file,
                    func.span,
                )
                .with_help(
                    "declare intrinsic bindings as `intrinsic fn name(...) -> Ret effects { ... };`",
                ),
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

    fn user_function_entries(&self) -> Vec<(String, String)> {
        self.program
            .items
            .iter()
            .filter_map(|item| match item {
                ir::Item::Function(func)
                    if decode_internal_type_alias(&func.name).is_none()
                        && decode_internal_const(&func.name).is_none() =>
                {
                    let module = self
                        .function_module_by_symbol
                        .get(&func.symbol)
                        .cloned()
                        .or_else(|| self.resolution.entry_module.clone())
                        .unwrap_or_else(|| "<root>".to_string());
                    Some((
                        func.name.clone(),
                        Self::qualified_function_key(&module, &func.name),
                    ))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    }

    fn resolved_function_key(&self, resolved_name: &str, resolved_module: Option<&str>) -> String {
        if let Some(module_name) = resolved_module {
            return Self::qualified_function_key(module_name, resolved_name);
        }

        if let Some(current_module) = self.current_module.as_ref() {
            if self
                .module_functions
                .contains_key(&(current_module.clone(), resolved_name.to_string()))
            {
                return Self::qualified_function_key(current_module, resolved_name);
            }
        }

        if let Some(modules) = self.resolution.function_modules.get(resolved_name) {
            if modules.len() == 1 {
                if let Some(module_name) = modules.iter().next() {
                    return Self::qualified_function_key(module_name, resolved_name);
                }
            }
        }

        resolved_name.to_string()
    }

    fn check_transitive_effects(&mut self) {
        let user_functions = self.user_function_entries();

        let mut memo = BTreeMap::new();
        for (_, function_key) in &user_functions {
            let mut visiting = BTreeSet::new();
            let closure = self.compute_effect_closure(function_key, &mut visiting, &mut memo);
            self.effect_usage.insert(function_key.clone(), closure);
        }

        for (function_name, function_key) in &user_functions {
            let declared = self
                .fn_sig_for_key(function_key)
                .map(|sig| sig.effects.clone())
                .unwrap_or_default();
            let closure = self
                .effect_usage
                .get(function_key)
                .cloned()
                .unwrap_or_default();
            let mut reasons = BTreeMap::new();
            for effect in &closure {
                let nodes = self
                    .find_effect_path(function_key, effect)
                    .map(|path| path.nodes)
                    .unwrap_or_else(|| vec![function_name.clone()]);
                reasons.insert(effect.clone(), nodes);
            }
            self.effect_reasons.insert(function_key.clone(), reasons);

            let missing = closure.difference(&declared).cloned().collect::<Vec<_>>();
            for effect in missing {
                let Some(path) = self.find_effect_path(function_key, &effect) else {
                    continue;
                };
                if path.nodes.len() < 3 {
                    continue;
                }
                let mut diagnostic = Diagnostic::error(
                    "E2005",
                    format!(
                        "function '{}' requires transitive effect '{}' via call path {}",
                        function_name,
                        effect,
                        path.nodes.join(" -> ")
                    ),
                    self.file,
                    path.span,
                )
                .with_help(format!(
                    "declare `effects {{ {} }}` on '{}' or refactor the call chain",
                    effect, function_name
                ));
                if let Some((function_span, body_span)) =
                    self.function_spans.get(function_key).copied()
                {
                    if let Some(fix) = self.effect_declaration_fix(
                        function_name,
                        function_span,
                        body_span,
                        &closure,
                    ) {
                        diagnostic = diagnostic.with_fix(fix);
                    }
                }
                self.diagnostics.push(diagnostic);
            }
        }
    }

    fn check_capability_authority(&mut self) {
        let user_functions = self.user_function_entries();

        for (function_name, function_key) in &user_functions {
            if self.is_std_function(function_key) {
                continue;
            }
            let declared_capabilities = self
                .fn_sig_for_key(function_key)
                .map(|sig| sig.capabilities.clone())
                .unwrap_or_default();
            let required_effects = self
                .effect_usage
                .get(function_key)
                .cloned()
                .unwrap_or_default();
            let missing = required_effects
                .difference(&declared_capabilities)
                .cloned()
                .collect::<Vec<_>>();

            for capability in missing {
                let mut diagnostic = if let Some(path) =
                    self.find_effect_path(function_key, &capability)
                {
                    Diagnostic::error(
                        "E2009",
                        format!(
                            "function '{}' requires capability '{}' via call path {}",
                            function_name,
                            capability,
                            path.nodes.join(" -> ")
                        ),
                        self.file,
                        path.span,
                    )
                    .with_help(format!(
                        "declare `capabilities {{ {} }}` on '{}' and thread authority through callers",
                        capability, function_name
                    ))
                } else {
                    Diagnostic::error(
                        "E2009",
                        format!(
                            "function '{}' declares effect '{}' but is missing capability '{}'",
                            function_name, capability, capability
                        ),
                        self.file,
                        self.function_spans
                            .get(function_key)
                            .map(|(span, _)| *span)
                            .unwrap_or(crate::span::Span::new(0, 0)),
                    )
                    .with_help(format!(
                        "declare `capabilities {{ {} }}` on '{}'",
                        capability, function_name
                    ))
                };

                if let Some((function_span, body_span)) =
                    self.function_spans.get(function_key).copied()
                {
                    if let Some(fix) = self.capability_declaration_fix(
                        function_name,
                        function_span,
                        body_span,
                        &required_effects,
                    ) {
                        diagnostic = diagnostic.with_fix(fix);
                    }
                }

                self.diagnostics.push(diagnostic);
            }
        }
    }

    fn is_std_function(&self, function: &str) -> bool {
        if let Some((module, _)) = function.rsplit_once("::") {
            return module == "std" || module.starts_with("std.");
        }
        self.resolution
            .functions
            .get(function)
            .and_then(|info| self.function_module_by_symbol.get(&info.symbol))
            .is_some_and(|module| module == "std" || module.starts_with("std."))
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
                .fn_sig_for_key(function)
                .map(|sig| sig.effects.clone())
                .unwrap_or_default();
        }

        let mut required = self
            .fn_sig_for_key(function)
            .map(|sig| sig.effects.clone())
            .unwrap_or_default();

        if let Some(edges) = self.call_graph.get(function) {
            for edge in edges {
                if let Some(sig) = self.fn_sig_for_key(&edge.callee) {
                    required.extend(sig.effects.iter().cloned());
                }
                if self.resolution_has_function_key(&edge.callee) {
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
                    .fn_sig_for_key(&edge.callee)
                    .map(|sig| sig.effects.contains(effect))
                    .unwrap_or(false)
                {
                    return Some(EffectPath {
                        nodes: next_path,
                        span,
                    });
                }

                if !self.resolution_has_function_key(&edge.callee) {
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

    fn capability_declaration_fix(
        &self,
        function_name: &str,
        function_span: crate::span::Span,
        body_span: crate::span::Span,
        required_capabilities: &BTreeSet<String>,
    ) -> Option<SuggestedFix> {
        let source = self.source.as_ref()?;
        if required_capabilities.is_empty()
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
        let mut capabilities = self
            .functions
            .get(function_name)
            .map(|sig| sig.capabilities.clone())
            .unwrap_or_default();
        capabilities.extend(required_capabilities.iter().cloned());
        let capabilities = capabilities.into_iter().collect::<Vec<_>>();
        if capabilities.is_empty() {
            return None;
        }
        let capabilities_text = capabilities.join(", ");

        if let Some((clause_rel_start, clause_rel_end)) = Self::find_capability_clause(signature) {
            let start = function_span.start + clause_rel_start;
            let end = function_span.start + clause_rel_end;
            return Some(SuggestedFix {
                message: format!(
                    "update capability declaration on '{}' to include required capabilities",
                    function_name
                ),
                replacement: Some(format!("capabilities {{ {} }}", capabilities_text)),
                start: Some(start),
                end: Some(end),
            });
        }

        if let Some((_, effects_clause_end)) = Self::find_effect_clause(signature) {
            let insert = function_span.start + effects_clause_end;
            return Some(SuggestedFix {
                message: format!(
                    "add missing capabilities declaration to function '{}'",
                    function_name
                ),
                replacement: Some(format!(" capabilities {{ {} }}", capabilities_text)),
                start: Some(insert),
                end: Some(insert),
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
            (rel, format!("capabilities {{ {} }} ", capabilities_text))
        } else {
            (
                signature.len(),
                format!(" capabilities {{ {} }}", capabilities_text),
            )
        };
        let insert = function_span.start + insert_rel;
        Some(SuggestedFix {
            message: format!(
                "add missing capabilities declaration to function '{}'",
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

    fn find_capability_clause(signature: &str) -> Option<(usize, usize)> {
        let bytes = signature.as_bytes();
        let mut start = 0usize;

        while start < bytes.len() {
            let found = signature[start..].find("capabilities")?;
            let idx = start + found;
            let end_keyword = idx + "capabilities".len();
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
        let mut binding_types = ResourceBindingTypeMap::new();
        for param in &func.params {
            let ty = self
                .types
                .get(&param.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            binding_types.insert(param.name.clone(), self.normalize_type(&ty));
        }
        self.check_resource_protocol_block(&func.body, &mut state, &mut binding_types);
    }

    fn check_resource_protocol_block(
        &mut self,
        block: &ir::Block,
        state: &mut ResourceStateMap,
        binding_types: &mut ResourceBindingTypeMap,
    ) {
        for stmt in &block.stmts {
            match stmt {
                ir::Stmt::Let { name, ty, expr, .. } => {
                    self.check_resource_protocol_expr(expr, state, binding_types);
                    clear_resource_state_for_var(name, state);
                    if let Some(binding_ty) =
                        self.infer_resource_binding_type(expr, *ty, binding_types)
                    {
                        binding_types.insert(name.clone(), binding_ty);
                    } else {
                        binding_types.remove(name);
                    }
                }
                ir::Stmt::Assign { target, expr, .. } => {
                    self.check_resource_protocol_expr(expr, state, binding_types);
                    clear_resource_state_for_var(target, state);
                    if let Some(binding_ty) =
                        self.infer_resource_binding_type(expr, None, binding_types)
                    {
                        binding_types.insert(target.clone(), binding_ty);
                    } else {
                        binding_types.remove(target);
                    }
                }
                ir::Stmt::Expr { expr, .. } => {
                    self.check_resource_protocol_expr(expr, state, binding_types)
                }
                ir::Stmt::Return {
                    expr: Some(expr), ..
                }
                | ir::Stmt::Assert { expr, .. } => {
                    self.check_resource_protocol_expr(expr, state, binding_types)
                }
                ir::Stmt::Return { expr: None, .. } => {}
            }
        }

        if let Some(tail) = &block.tail {
            self.check_resource_protocol_expr(tail, state, binding_types);
        }
    }

    fn check_resource_protocol_expr(
        &mut self,
        expr: &ir::Expr,
        state: &mut ResourceStateMap,
        binding_types: &mut ResourceBindingTypeMap,
    ) {
        self.check_resource_protocol_expr_mode(expr, state, binding_types, false);
    }

    fn check_resource_protocol_expr_mode(
        &mut self,
        expr: &ir::Expr,
        state: &mut ResourceStateMap,
        binding_types: &mut ResourceBindingTypeMap,
        allow_closed_use: bool,
    ) {
        match &expr.kind {
            ir::ExprKind::Call { callee, args, .. } => {
                self.check_resource_protocol_expr_mode(callee, state, binding_types, false);
                for arg in args {
                    self.check_resource_protocol_expr_mode(arg, state, binding_types, false);
                }
                self.check_resource_protocol_call(
                    callee,
                    args,
                    expr.span,
                    state,
                    binding_types,
                    allow_closed_use,
                );
            }
            ir::ExprKind::Closure { body, .. } => {
                let mut closure_state = state.clone();
                let mut closure_binding_types = binding_types.clone();
                self.check_resource_protocol_block(
                    body,
                    &mut closure_state,
                    &mut closure_binding_types,
                );
            }
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                self.check_resource_protocol_expr_mode(cond, state, binding_types, false);
                let mut then_state = state.clone();
                let mut then_binding_types = binding_types.clone();
                self.check_resource_protocol_block(
                    then_block,
                    &mut then_state,
                    &mut then_binding_types,
                );
                let mut else_state = state.clone();
                let mut else_binding_types = binding_types.clone();
                self.check_resource_protocol_block(
                    else_block,
                    &mut else_state,
                    &mut else_binding_types,
                );
            }
            ir::ExprKind::While { cond, body } => {
                self.check_resource_protocol_expr_mode(cond, state, binding_types, false);
                let mut loop_state = state.clone();
                let mut loop_binding_types = binding_types.clone();
                self.check_resource_protocol_block(body, &mut loop_state, &mut loop_binding_types);
            }
            ir::ExprKind::Loop { body } => {
                let mut loop_state = state.clone();
                let mut loop_binding_types = binding_types.clone();
                self.check_resource_protocol_block(body, &mut loop_state, &mut loop_binding_types);
            }
            ir::ExprKind::Break { expr } => {
                if let Some(expr) = expr {
                    self.check_resource_protocol_expr_mode(expr, state, binding_types, false);
                }
            }
            ir::ExprKind::Continue => {}
            ir::ExprKind::Match {
                expr: scrutinee,
                arms,
            } => {
                // `match call(...) { ... }` explicitly handles `Result` branches, including
                // expected runtime closed/cancelled outcomes.
                self.check_resource_protocol_expr_mode(scrutinee, state, binding_types, true);
                for arm in arms {
                    let mut arm_state = state.clone();
                    let mut arm_binding_types = binding_types.clone();
                    if let Some(guard) = &arm.guard {
                        self.check_resource_protocol_expr_mode(
                            guard,
                            &mut arm_state,
                            &mut arm_binding_types,
                            false,
                        );
                    }
                    self.check_resource_protocol_expr_mode(
                        &arm.body,
                        &mut arm_state,
                        &mut arm_binding_types,
                        false,
                    );
                }
            }
            ir::ExprKind::UnsafeBlock { block } => {
                let mut block_state = state.clone();
                let mut block_binding_types = binding_types.clone();
                self.check_resource_protocol_block(
                    block,
                    &mut block_state,
                    &mut block_binding_types,
                );
            }
            ir::ExprKind::Binary { lhs, rhs, .. } => {
                self.check_resource_protocol_expr_mode(lhs, state, binding_types, false);
                self.check_resource_protocol_expr_mode(rhs, state, binding_types, false);
            }
            ir::ExprKind::Unary { expr, .. }
            | ir::ExprKind::Borrow { expr, .. }
            | ir::ExprKind::Await { expr }
            | ir::ExprKind::Try { expr } => {
                self.check_resource_protocol_expr_mode(expr, state, binding_types, false);
            }
            ir::ExprKind::StructInit { fields, .. } => {
                for (_, value, _) in fields {
                    self.check_resource_protocol_expr_mode(value, state, binding_types, false);
                }
            }
            ir::ExprKind::FieldAccess { base, .. } => {
                self.check_resource_protocol_expr_mode(base, state, binding_types, false);
            }
            ir::ExprKind::Int(_)
            | ir::ExprKind::Float(_)
            | ir::ExprKind::Bool(_)
            | ir::ExprKind::Char(_)
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
        binding_types: &ResourceBindingTypeMap,
        allow_closed_use: bool,
    ) {
        let Some(op) = self.resolve_resource_protocol_call(callee) else {
            return;
        };
        let Some(first_arg) = args.first() else {
            return;
        };
        let ir::ExprKind::Var(var_name) = &first_arg.kind else {
            return;
        };
        let key = (var_name.clone(), op.kind);
        let binding_label = self.resource_protocol_binding_label(var_name, op, binding_types);
        if let Some(previous) = state.get(&key).copied() {
            if !allow_closed_use {
                let mut diag = Diagnostic::error(
                    "E2006",
                    format!(
                        "resource protocol violation: '{}' called after terminal '{}' on closed {} '{}'",
                        op.api,
                        previous.closed_by,
                        binding_label,
                        var_name
                    ),
                    self.file,
                    span,
                )
                .with_help(format!(
                    "create a new {} before calling '{}' again",
                    binding_label,
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
            state.insert(
                key,
                ResourceState {
                    closed_at: span,
                    closed_by: op.api,
                },
            );
        }
    }

    fn resolve_resource_protocol_call(&self, callee: &ir::Expr) -> Option<ResourceProtocolOp> {
        let call_path = self.extract_callee_path(callee)?;
        let name = call_path.last()?;
        let op = resource_protocol_op(name)?;
        let sig = self.functions.get(name)?;
        let first_param = sig.params.first()?;
        let first_param_normalized = self.normalize_type(first_param);
        if base_type_name(&first_param_normalized) == op.first_param_base_type
            && sig.effects.contains(op.required_effect)
        {
            Some(op)
        } else {
            None
        }
    }

    fn infer_resource_binding_type(
        &self,
        expr: &ir::Expr,
        declared_ty: Option<ir::TypeId>,
        binding_types: &ResourceBindingTypeMap,
    ) -> Option<String> {
        if let Some(ty_id) = declared_ty {
            let ty = self.types.get(&ty_id)?.clone();
            return Some(self.normalize_type(&ty));
        }
        match &expr.kind {
            ir::ExprKind::Var(name) => binding_types.get(name).cloned(),
            ir::ExprKind::FieldAccess { base, field } => {
                let base_ty = self.infer_resource_binding_type(base, None, binding_types)?;
                let index = field.parse::<usize>().ok()?;
                let elem_ty = extract_tuple_field_type(&base_ty, index)?;
                Some(self.normalize_type(&elem_ty))
            }
            _ => None,
        }
    }

    fn resource_protocol_binding_label(
        &self,
        var_name: &str,
        op: ResourceProtocolOp,
        binding_types: &ResourceBindingTypeMap,
    ) -> String {
        let Some(binding_ty) = binding_types.get(var_name) else {
            return op.kind.as_str().to_string();
        };
        let normalized = self.normalize_type(binding_ty);
        if base_type_name(&normalized) == op.first_param_base_type {
            normalized
        } else {
            op.kind.as_str().to_string()
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
            ir::ExprKind::Call { callee, args, .. } => {
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
            | ir::ExprKind::Char(_)
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
            ir::ExprKind::Float(_) => Some("Float64".to_string()),
            ir::ExprKind::Bool(_) => Some("Bool".to_string()),
            ir::ExprKind::Char(_) => Some("Char".to_string()),
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

            if let Some(default_expr) = &field.default_value {
                let context = format!("default value for field '{}.{}'", strukt.name, field.name);
                self.validate_compile_time_expr(&context, default_expr);

                let mut locals = BTreeMap::new();
                let mut ctx = ExprContext::default();
                let default_ty = self.check_expr_with_expected(
                    default_expr,
                    &mut locals,
                    &BTreeSet::new(),
                    &mut ctx,
                    true,
                    Some(&ty),
                );
                if contains_unresolved_type(&ty) {
                    self.observe_struct_field_hole(&strukt.name, &field.name, &default_ty);
                }
                if !self.types_compatible(&ty, &default_ty) {
                    self.diagnostics.push(Diagnostic::error(
                        "E1226",
                        format!(
                            "default value for field '{}.{}' expects '{}', found '{}'",
                            strukt.name, field.name, ty, default_ty
                        ),
                        self.file,
                        default_expr.span,
                    ));
                }
            }
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
                    let Some(target_ty) = scope.get(target).cloned() else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1208",
                            format!("unknown symbol '{}'", target),
                            self.file,
                            *span,
                        ));
                        continue;
                    };
                    let expr_ty = self.check_expr_with_expected(
                        expr,
                        &mut scope,
                        allowed_effects,
                        ctx,
                        contract_mode,
                        Some(&target_ty),
                    );
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

    fn integer_expected_type_hint(&self, expected_ty: Option<&str>) -> Option<String> {
        let expected = expected_ty?;
        let normalized = self.normalize_type(expected);
        if parse_integer_kind(&normalized).is_some() {
            Some(normalized)
        } else {
            None
        }
    }

    fn float_expected_type_hint(&self, expected_ty: Option<&str>) -> Option<String> {
        let expected = expected_ty?;
        let normalized = self.normalize_type(expected);
        parse_float_kind(&normalized).map(|kind| float_kind_type_name(kind).to_string())
    }

    fn parse_raw_int_literal_magnitude(text: &str) -> Option<u128> {
        let trimmed = text.trim();
        if let Some(hex) = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
        {
            u128::from_str_radix(hex, 16).ok()
        } else {
            trimmed.parse::<u128>().ok()
        }
    }

    fn non_negative_integer_literal_value(
        &self,
        value: i64,
        metadata: Option<&ir::IntLiteralMetadata>,
    ) -> Option<IntegerLiteralValue> {
        if let Some(meta) = metadata {
            return Self::parse_raw_int_literal_magnitude(&meta.raw_literal_text)
                .map(IntegerLiteralValue::NonNegative);
        }
        if value < 0 {
            return None;
        }
        Some(IntegerLiteralValue::NonNegative(value as u128))
    }

    fn negative_integer_literal_value(
        &self,
        value: i64,
        metadata: Option<&ir::IntLiteralMetadata>,
    ) -> Option<IntegerLiteralValue> {
        if let Some(meta) = metadata {
            return Self::parse_raw_int_literal_magnitude(&meta.raw_literal_text)
                .map(IntegerLiteralValue::NegativeMagnitude);
        }
        if value < 0 {
            return None;
        }
        Some(IntegerLiteralValue::NegativeMagnitude(value as u128))
    }

    fn expr_int_literal_value(&self, expr: &ir::Expr) -> Option<IntegerLiteralValue> {
        match &expr.kind {
            ir::ExprKind::Int(value) => {
                let metadata = expr.int_literal_metadata();
                self.non_negative_integer_literal_value(*value, metadata.as_ref())
            }
            ir::ExprKind::Unary {
                op: crate::ast::UnaryOp::Neg,
                expr: inner,
            } => match inner.kind {
                ir::ExprKind::Int(value) => {
                    let metadata = inner.int_literal_metadata();
                    self.negative_integer_literal_value(value, metadata.as_ref())
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn integer_literal_fits_expected_type(&self, expr: &ir::Expr, expected_ty: &str) -> bool {
        let Some(value) = self.expr_int_literal_value(expr) else {
            return false;
        };
        let expected_norm = self.normalize_type(expected_ty);
        let Some(kind) = parse_integer_kind(&expected_norm) else {
            return false;
        };
        integer_literal_fits_kind(value, kind)
    }

    fn argument_type_compatible(&self, expected: &str, found: &str, arg: &ir::Expr) -> bool {
        self.types_compatible(expected, found)
            || self.integer_literal_fits_expected_type(arg, expected)
    }

    fn validate_integer_literal_range(
        &mut self,
        value: IntegerLiteralValue,
        target_ty: &str,
        span: crate::span::Span,
        code: &str,
        subject: &str,
    ) -> bool {
        let Some(kind) = parse_integer_kind(target_ty) else {
            return true;
        };
        if integer_literal_fits_kind(value, kind) {
            return true;
        }
        let (min, max) = integer_kind_range_text(kind);
        let found = integer_literal_value_text(value);
        self.diagnostics.push(
            Diagnostic::error(
                code,
                format!(
                    "{subject} is out of range for type '{target_ty}' (expected {min}..={max}, found {found})"
                ),
                self.file,
                span,
            )
            .with_help("use a value within range or change the target integer type"),
        );
        false
    }

    fn check_int_literal_expr(
        &mut self,
        expr: &ir::Expr,
        value: i64,
        expected_ty: Option<&str>,
    ) -> String {
        let expected_int = self.integer_expected_type_hint(expected_ty);
        let metadata = expr.int_literal_metadata();
        if let Some(meta) = metadata.as_ref() {
            let literal_ty = integer_kind_type_name(integer_kind_from_literal_kind(meta.kind));
            let literal_value = self
                .non_negative_integer_literal_value(value, Some(meta))
                .unwrap_or(IntegerLiteralValue::NonNegative(value.max(0) as u128));
            self.validate_integer_literal_range(
                literal_value,
                literal_ty,
                expr.span,
                "E1204",
                &format!(
                    "integer literal '{}' with suffix '{}'",
                    meta.raw_literal_text, meta.suffix_text
                ),
            );
            literal_ty.to_string()
        } else if let Some(expected) = expected_int {
            let literal_value = self
                .non_negative_integer_literal_value(value, None)
                .unwrap_or(IntegerLiteralValue::NonNegative(value.max(0) as u128));
            self.validate_integer_literal_range(
                literal_value,
                &expected,
                expr.span,
                "E1204",
                &format!("integer literal '{}'", value),
            );
            expected
        } else {
            "Int".to_string()
        }
    }

    fn check_negated_int_literal_expr(
        &mut self,
        literal_expr: &ir::Expr,
        value: i64,
        expected_ty: Option<&str>,
    ) -> String {
        let expected_int = self.integer_expected_type_hint(expected_ty);
        let metadata = literal_expr.int_literal_metadata();
        if let Some(meta) = metadata.as_ref() {
            let literal_ty = integer_kind_type_name(integer_kind_from_literal_kind(meta.kind));
            let literal_value = self
                .negative_integer_literal_value(value, Some(meta))
                .unwrap_or(IntegerLiteralValue::NegativeMagnitude(value.max(0) as u128));
            self.validate_integer_literal_range(
                literal_value,
                literal_ty,
                literal_expr.span,
                "E1204",
                &format!(
                    "integer literal '-{}' with suffix '{}'",
                    meta.raw_literal_text, meta.suffix_text
                ),
            );
            literal_ty.to_string()
        } else if let Some(expected) = expected_int {
            let literal_value = self
                .negative_integer_literal_value(value, None)
                .unwrap_or(IntegerLiteralValue::NegativeMagnitude(value.max(0) as u128));
            self.validate_integer_literal_range(
                literal_value,
                &expected,
                literal_expr.span,
                "E1204",
                &format!("integer literal '-{}'", value),
            );
            expected
        } else {
            "Int".to_string()
        }
    }

    fn check_float_literal_expr(&self, expected_ty: Option<&str>) -> String {
        self.float_expected_type_hint(expected_ty)
            .unwrap_or_else(|| "Float64".to_string())
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
            ir::ExprKind::Int(value) => self.check_int_literal_expr(expr, *value, expected_ty),
            ir::ExprKind::Float(_) => self.check_float_literal_expr(expected_ty),
            ir::ExprKind::Bool(_) => "Bool".to_string(),
            ir::ExprKind::Char(_) => "Char".to_string(),
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
                if self.is_user_written_intrinsic_use(name, expr.span) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E2102",
                            format!(
                                "intrinsic symbol '{}' is private runtime implementation detail",
                                name
                            ),
                            self.file,
                            expr.span,
                        )
                        .with_help(
                            "call the corresponding public std API instead of intrinsic symbols",
                        ),
                    );
                    return "<?>".to_string();
                }
                let internal_intrinsic_use =
                    self.is_compiler_internal_intrinsic_use(name, expr.span);
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

                    if let Some(modules) = self.resolution.function_modules.get(name) {
                        if let Some(module_name) = modules.iter().next() {
                            if let Some(info) = self
                                .resolution
                                .module_function_infos
                                .get(&(module_name.clone(), name.clone()))
                            {
                                if !internal_intrinsic_use
                                    && !self.function_is_accessible_from_current(module_name, info)
                                {
                                    self.diagnostics.push(
                                        Diagnostic::error(
                                            "E2102",
                                            format!(
                                                "symbol '{}.{}' is private and not accessible from this module",
                                                module_name, name
                                            ),
                                            self.file,
                                            expr.span,
                                        )
                                        .with_help(format!(
                                            "mark '{}.{}' as `pub fn` or call it within module '{}'.",
                                            module_name, name, module_name
                                        )),
                                    );
                                    return "<?>".to_string();
                                }
                            }
                        }
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
            ir::ExprKind::Call {
                callee,
                args,
                arg_names,
            } => {
                if let ir::ExprKind::FieldAccess { base, field } = &callee.kind {
                    if !self.is_module_qualified_callee(callee, locals) {
                        return self.check_method_call(
                            base,
                            field,
                            args,
                            arg_names,
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
                        arg_names,
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

                if self.is_user_written_intrinsic_use(&name, callee.span) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E2102",
                            format!(
                                "intrinsic symbol '{}' is private runtime implementation detail",
                                name
                            ),
                            self.file,
                            callee.span,
                        )
                        .with_help(
                            "call the corresponding public std API instead of intrinsic symbols",
                        ),
                    );
                    return "<?>".to_string();
                }
                let internal_intrinsic_use =
                    self.is_compiler_internal_intrinsic_use(&name, callee.span);

                if !qualified {
                    if let Some(local_ty) = locals.get(&name).cloned() {
                        if parse_fn_type(&local_ty).is_some() {
                            return self.check_fn_value_call(
                                &local_ty,
                                &rendered_path,
                                args,
                                arg_names,
                                expr.span,
                                locals,
                                allowed_effects,
                                ctx,
                                contract_mode,
                            );
                        }
                    }
                }

                if !qualified && name == "aic_for_into_iter" {
                    return self.check_for_into_iter_call(
                        args,
                        arg_names,
                        expr,
                        locals,
                        allowed_effects,
                        ctx,
                        contract_mode,
                        expected_ty,
                    );
                }
                if !qualified && name == "aic_for_next_iter" {
                    return self.check_for_next_iter_call(
                        args,
                        arg_names,
                        expr,
                        locals,
                        allowed_effects,
                        ctx,
                        contract_mode,
                        expected_ty,
                    );
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

                if !qualified && !self.functions.contains_key(&name) {
                    if let Some(default_ty) =
                        self.check_struct_default_call(&name, args, expr.span, expected_ty)
                    {
                        return default_ty;
                    }
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

                    if !self.module_is_imported_in_current_scope(&module) {
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

                    let has_symbol = self
                        .resolution
                        .module_functions
                        .get(&module)
                        .map(|s| s.contains(&name))
                        .unwrap_or(false);
                    if !has_symbol {
                        self.diagnostics.push(Diagnostic::error(
                            "E1218",
                            format!("unknown callable '{}'", rendered_path),
                            self.file,
                            callee.span,
                        ));
                        return "<?>".to_string();
                    }

                    if let Some(info) = self
                        .resolution
                        .module_function_infos
                        .get(&(module.clone(), name.clone()))
                    {
                        if !internal_intrinsic_use
                            && !self.function_is_accessible_from_current(&module, info)
                        {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E2102",
                                    format!("symbol '{}.{}' is private and not accessible from this module", module, name),
                                    self.file,
                                    callee.span,
                                )
                                .with_help(format!("mark '{}.{}' as `pub fn` or call it within module '{}'.", module, name, module)),
                            );
                            return "<?>".to_string();
                        }
                    }

                    resolved_module = Some(module.clone());
                    name.clone()
                } else {
                    if self.enforce_import_visibility
                        && !internal_intrinsic_use
                        && !name.contains("::")
                        && !self.resolution.visible_functions.contains(&name)
                    {
                        if let Some(modules) = self.resolution.function_modules.get(&name) {
                            let mut modules = modules.iter().cloned().collect::<Vec<_>>();
                            modules.sort();
                            if let Some(private_module) = modules
                                .iter()
                                .find(|module| {
                                    self.resolution
                                        .module_function_infos
                                        .get(&((*module).clone(), name.clone()))
                                        .map(|info| {
                                            !self.function_is_accessible_from_current(module, info)
                                        })
                                        .unwrap_or(false)
                                })
                                .cloned()
                            {
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        "E2102",
                                        format!(
                                            "symbol '{}.{}' is private and not accessible from this module",
                                            private_module, name
                                        ),
                                        self.file,
                                        callee.span,
                                    )
                                    .with_help(format!(
                                        "mark '{}.{}' as `pub fn` or call it within module '{}'.",
                                        private_module, name, private_module
                                    )),
                                );
                                return "<?>".to_string();
                            }

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

                    let visible_candidate_modules = self
                        .resolution
                        .function_modules
                        .get(&name)
                        .map(|modules| {
                            modules
                                .iter()
                                .filter(|module| {
                                    if !self.module_is_imported_in_current_scope(module) {
                                        return false;
                                    }
                                    self.resolution
                                        .module_function_infos
                                        .get(&((*module).clone(), name.clone()))
                                        .map(|info| {
                                            internal_intrinsic_use
                                                || self.function_is_accessible_from_current(
                                                    module, info,
                                                )
                                        })
                                        .unwrap_or(true)
                                })
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    if visible_candidate_modules.len() > 1 {
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

                    if visible_candidate_modules.len() == 1 {
                        resolved_module = visible_candidate_modules.into_iter().next();
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
                    if let Some(module_name) = resolved_module.as_deref() {
                        if let Some(info) = self
                            .resolution
                            .module_function_infos
                            .get(&(module_name.to_string(), resolved_name.clone()))
                        {
                            if !internal_intrinsic_use
                                && !self.function_is_accessible_from_current(module_name, info)
                            {
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        "E2102",
                                        format!(
                                            "symbol '{}.{}' is private and not accessible from this module",
                                            module_name, resolved_name
                                        ),
                                        self.file,
                                        callee.span,
                                    )
                                    .with_help(format!(
                                        "mark '{}.{}' as `pub fn` or call it within module '{}'.",
                                        module_name, resolved_name, module_name
                                    )),
                                );
                                return "<?>".to_string();
                            }
                        }
                    }

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
                        if !self.imports_for_current_module().contains("std.io") {
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
                    if resolved_name == "len"
                        && !self.imports_for_current_module().contains("std.string")
                    {
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

                    let (param_to_arg, arg_to_param) = self.build_call_arg_plan(
                        &rendered_path,
                        &sig.param_names,
                        args,
                        arg_names,
                        expr.span,
                    );
                    self.maybe_record_named_call_order(
                        expr.node,
                        args.len(),
                        arg_names,
                        &param_to_arg,
                    );

                    let mut arg_types = Vec::new();
                    for (arg_idx, arg) in args.iter().enumerate() {
                        let expected_hint = arg_to_param.get(arg_idx).and_then(|mapped| {
                            mapped.and_then(|param_idx| sig.params.get(param_idx))
                        });
                        let arg_ty = if let Some(expected_hint) = expected_hint {
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

                    for (param_idx, maybe_arg_idx) in param_to_arg.iter().enumerate() {
                        let Some(arg_idx) = maybe_arg_idx else {
                            continue;
                        };
                        let Some(expected_raw) = sig.params.get(param_idx) else {
                            continue;
                        };
                        let arg_ty = arg_types
                            .get(*arg_idx)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string());
                        let expected_norm = self.normalize_type(expected_raw);
                        let arg_norm = self.normalize_type(&arg_ty);
                        let _ = infer_generic_bindings(
                            &expected_norm,
                            &arg_norm,
                            &generic_set,
                            &mut generic_bindings,
                        );
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
                        for generic_name in &sig.generic_params {
                            if generic_bindings.contains_key(generic_name) {
                                continue;
                            }
                            let used_in_signature =
                                sig.params.iter().any(|param_ty| {
                                    type_uses_generic_param(param_ty, generic_name)
                                }) || type_uses_generic_param(&sig.ret, generic_name);
                            if !used_in_signature {
                                generic_bindings.insert(generic_name.clone(), "Int".to_string());
                            }
                        }

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
                            let marker_failure =
                                self.marker_trait_failure_reason(bound_trait, bound_ty);
                            let implemented = if Self::is_auto_marker_trait(bound_trait) {
                                marker_failure.is_none()
                            } else {
                                self.trait_is_implemented_for(bound_trait, bound_ty)
                            };
                            if !implemented {
                                let mut diag = Diagnostic::error(
                                    "E1258",
                                    format!(
                                        "type '{}' does not satisfy trait bound '{}: {}'",
                                        bound_ty, generic_name, bound_trait
                                    ),
                                    self.file,
                                    expr.span,
                                );
                                if Self::is_auto_marker_trait(bound_trait) {
                                    let reason = marker_failure.unwrap_or_else(|| {
                                        format!("type '{}' is not {}", bound_ty, bound_trait)
                                    });
                                    diag = diag.with_help(format!(
                                        "{}; use a {}-safe type or move only {} values across concurrency boundaries",
                                        reason, bound_trait, bound_trait
                                    ));
                                } else {
                                    diag = diag.with_help(format!(
                                        "add `impl {}[{}];` or use a type that implements '{}'",
                                        bound_trait, bound_ty, bound_trait
                                    ));
                                }
                                self.diagnostics.push(diag);
                            }
                        }
                    }
                    self.enforce_std_concurrency_send_bounds(
                        resolved_module.as_deref(),
                        &resolved_name,
                        &sig,
                        &generic_bindings,
                        expr.span,
                    );

                    let instantiated_params = sig
                        .params
                        .iter()
                        .map(|param| substitute_type_vars(param, &generic_bindings, &generic_set))
                        .collect::<Vec<_>>();

                    for (param_idx, maybe_arg_idx) in param_to_arg.iter().enumerate() {
                        let Some(arg_idx) = maybe_arg_idx else {
                            continue;
                        };
                        let Some(expected) = instantiated_params.get(param_idx) else {
                            continue;
                        };
                        let mut observed_ty = arg_types
                            .get(*arg_idx)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string());
                        if let ir::ExprKind::Var(name) = &args[*arg_idx].kind {
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
                        if !self.argument_type_compatible(expected, &observed_ty, &args[*arg_idx]) {
                            self.diagnostics.push(Diagnostic::error(
                                "E1214",
                                format!(
                                    "argument {} to '{}' expected '{}', found '{}'",
                                    param_idx + 1,
                                    rendered_path,
                                    expected,
                                    observed_ty
                                ),
                                self.file,
                                args[*arg_idx].span,
                            ));
                        }
                        if contains_unresolved_type(expected) {
                            self.observe_fn_param_hole(&resolved_name, param_idx, &observed_ty);
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
                        let callee_key =
                            self.resolved_function_key(&resolved_name, resolved_module.as_deref());
                        self.record_call_edge(&callee_key, expr.span);
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

                        if let Some(expected) = expected_ty {
                            let expected_norm = self.normalize_type(expected);
                            if base_type_name(&expected_norm) == candidate.enum_name {
                                let enum_template = format!(
                                    "{}[{}]",
                                    candidate.enum_name,
                                    candidate.generic_params.join(", ")
                                );
                                let template_norm = self.normalize_type(&enum_template);
                                infer_generic_bindings(
                                    &template_norm,
                                    &expected_norm,
                                    &generic_set,
                                    &mut generic_bindings,
                                );
                            }
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
                let branch_expected = expected_ty.unwrap_or("()");
                let then_ty = self.check_block(
                    then_block,
                    locals,
                    branch_expected,
                    allowed_effects,
                    ctx,
                    contract_mode,
                );
                let else_ty = self.check_block(
                    else_block,
                    locals,
                    branch_expected,
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
                    let body_ty = self.check_expr_with_expected(
                        &arm.body,
                        &mut arm_scope,
                        allowed_effects,
                        ctx,
                        contract_mode,
                        expected_ty,
                    );
                    arm_types.push(body_ty);
                }

                self.check_exhaustive(expr.span, &scrutinee_ty, &seen, wildcard_seen);

                if arm_types.is_empty() {
                    "()".to_string()
                } else {
                    let first = arm_types[0].clone();
                    for ty in arm_types.iter().skip(1) {
                        if !self.types_compatible(&first, ty) && !self.types_compatible(ty, &first)
                        {
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
                let expected_numeric = self
                    .integer_expected_type_hint(expected_ty)
                    .or_else(|| self.float_expected_type_hint(expected_ty));
                let mut left_ty = self.check_expr_with_expected(
                    lhs,
                    locals,
                    allowed_effects,
                    ctx,
                    contract_mode,
                    expected_numeric.as_deref(),
                );
                let mut rhs_expected = expected_numeric;
                if rhs_expected.is_none()
                    && matches!(
                        op,
                        BinOp::Add
                            | BinOp::Sub
                            | BinOp::Mul
                            | BinOp::Div
                            | BinOp::Mod
                            | BinOp::BitAnd
                            | BinOp::BitOr
                            | BinOp::BitXor
                            | BinOp::Shl
                            | BinOp::Shr
                            | BinOp::Ushr
                            | BinOp::Eq
                            | BinOp::Ne
                            | BinOp::Lt
                            | BinOp::Le
                            | BinOp::Gt
                            | BinOp::Ge
                    )
                {
                    let left_norm = self.normalize_type(&left_ty);
                    if parse_integer_kind(&left_norm).is_some() {
                        rhs_expected = Some(left_norm);
                    } else if let Some(kind) = parse_float_kind(&left_norm) {
                        rhs_expected = Some(float_kind_type_name(kind).to_string());
                    }
                }
                let mut right_ty = self.check_expr_with_expected(
                    rhs,
                    locals,
                    allowed_effects,
                    ctx,
                    contract_mode,
                    rhs_expected.as_deref(),
                );
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
                if matches!(op, crate::ast::UnaryOp::Neg) {
                    if let ir::ExprKind::Int(value) = inner.kind {
                        return self.check_negated_int_literal_expr(inner, value, expected_ty);
                    }
                }
                let ty = self.check_expr(inner, locals, allowed_effects, ctx, contract_mode);
                let ty_norm = self.normalize_type(&ty);
                match op {
                    crate::ast::UnaryOp::Neg => {
                        if let Some(kind) = parse_float_kind(&ty_norm) {
                            return float_kind_type_name(kind).to_string();
                        }
                        if let Some(kind) = parse_integer_kind(&ty_norm) {
                            if matches!(kind, IntegerKind::Fixed { signed: false, .. }) {
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        "E1222",
                                        format!(
                                            "unary '-' cannot be applied to unsigned integer type '{}'",
                                            ty
                                        ),
                                        self.file,
                                        inner.span,
                                    )
                                    .with_help("use a signed integer type for negation"),
                                );
                            }
                            return ty_norm;
                        }
                        self.diagnostics.push(Diagnostic::error(
                            "E1222",
                            "unary '-' expects signed integer or Float32/Float64",
                            self.file,
                            inner.span,
                        ));
                        "<?>".to_string()
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
                    crate::ast::UnaryOp::BitNot => {
                        if parse_integer_kind(&ty_norm).is_none() {
                            self.diagnostics.push(Diagnostic::error(
                                "E1222",
                                "unary '~' expects integer operand",
                                self.file,
                                inner.span,
                            ));
                            return "<?>".to_string();
                        }
                        ty_norm
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

                if let Some(bridged_ty) = await_bridge_submit_type(&ty_norm) {
                    return bridged_ty;
                }

                if base_type_name(&ty_norm) != "Async" {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1257",
                            format!("await expects Async[T], found '{}'", ty),
                            self.file,
                            inner.span,
                        )
                        .with_help(
                            "await values returned from async functions or std.net/std.tls async submit calls",
                        ),
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

                if !self.field_is_accessible_from_current(&info.module, info.visibility) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E2102",
                            format!(
                                "type '{}.{}' is private and not accessible from this module",
                                info.module, name
                            ),
                            self.file,
                            expr.span,
                        )
                        .with_help(format!(
                            "mark '{}.{}' as `pub struct` or construct it within module '{}'.",
                            info.module, name, info.module
                        )),
                    );
                    return "<?>".to_string();
                }

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
                    if let Some(field_visibility) = info.field_visibility.get(field_name).copied() {
                        if !self.field_is_accessible_from_current(&info.module, field_visibility) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E2102",
                                    format!(
                                        "field '{}.{}' is private and not accessible from this module",
                                        name, field_name
                                    ),
                                    self.file,
                                    *span,
                                )
                                .with_help(format!(
                                    "mark '{}.{}' as `pub` or access it within module '{}'.",
                                    name, field_name, info.module
                                )),
                            );
                            continue;
                        }
                    }
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
                    if !fields.iter().any(|(name, _, _)| name == field)
                        && !info.default_fields.contains(field)
                    {
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
                        if let Some(field_visibility) = info.field_visibility.get(field).copied() {
                            if !self
                                .field_is_accessible_from_current(&info.module, field_visibility)
                            {
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        "E2102",
                                        format!(
                                            "field '{}.{}' is private and not accessible from this module",
                                            struct_name, field
                                        ),
                                        self.file,
                                        expr.span,
                                    )
                                    .with_help(format!(
                                        "mark '{}.{}' as `pub` or access it within module '{}'.",
                                        struct_name, field, info.module
                                    )),
                                );
                                return "<?>".to_string();
                            }
                        }
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
        arg_names: &[Option<String>],
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

        if Self::has_named_call_args(arg_names) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1213",
                    format!(
                        "named arguments are not supported for callable {} because parameter names are unavailable",
                        rendered_callee
                    ),
                    self.file,
                    span,
                )
                .with_help("call function values with positional arguments"),
            );
        }

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
            if !self.argument_type_compatible(expected, &found, arg) {
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
            let alias_is_ambiguous = self
                .ambiguous_aliases_for_current_module()
                .map(|set| set.contains(alias))
                .unwrap_or_else(|| self.resolution.ambiguous_import_aliases.contains(alias));
            if alias_is_ambiguous {
                return None;
            }
            if let Some(module) = self
                .module_aliases_for_current_module()
                .and_then(|aliases| aliases.get(alias))
                .or_else(|| self.resolution.import_aliases.get(alias))
            {
                return Some(module.clone());
            }
        }

        let full = qualifier.join(".");
        if self.module_is_imported_in_current_scope(&full) {
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
        arg_names: &[Option<String>],
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

        if let Some(dyn_trait) = parse_dyn_trait_name(&receiver_ty) {
            return self.check_dyn_trait_method_call(
                &dyn_trait,
                field,
                args,
                arg_names,
                call_expr,
                locals,
                allowed_effects,
                ctx,
                contract_mode,
                expected_ty,
            );
        }
        let assoc_name = format!("{}::{}", base_type_name(&receiver_ty), field);

        if Self::has_named_call_args(arg_names) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1213",
                    format!(
                        "named arguments are not currently supported for method call {}.{}",
                        base_type_name(&receiver_ty),
                        field
                    ),
                    self.file,
                    call_expr.span,
                )
                .with_help("use positional arguments for method calls"),
            );
        }

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
                    arg_names: Vec::new(),
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
                    if !self.argument_type_compatible(expected, &found, arg) {
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

    fn lookup_trait_info(&self, trait_name: &str) -> Option<(String, TraitInfo)> {
        if let Some(info) = self.resolution.traits.get(trait_name) {
            return Some((trait_name.to_string(), info.clone()));
        }
        let short = Self::unqualified_name(trait_name);
        if let Some(info) = self.resolution.traits.get(short) {
            return Some((short.to_string(), info.clone()));
        }
        if short != trait_name {
            let matches = self
                .resolution
                .traits
                .iter()
                .filter(|(name, _)| Self::unqualified_name(name) == short)
                .collect::<Vec<_>>();
            if matches.len() == 1 {
                let (name, info) = matches[0];
                return Some((name.clone(), info.clone()));
            }
        }
        None
    }

    fn validate_dyn_trait_object_safety(
        &mut self,
        trait_name: &str,
        span: crate::span::Span,
    ) -> bool {
        let Some((resolved_trait_name, trait_info)) = self.lookup_trait_info(trait_name) else {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1212",
                    format!("unknown trait '{}' in dyn type", trait_name),
                    self.file,
                    span,
                )
                .with_help("declare the trait or import the correct module"),
            );
            return false;
        };

        let mut ok = true;
        if !trait_info.generics.is_empty() {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1214",
                    format!(
                        "trait '{}' is not object-safe for dyn usage: trait generics are not supported",
                        resolved_trait_name
                    ),
                    self.file,
                    span,
                )
                .with_help("use a non-generic trait for dyn dispatch"),
            );
            ok = false;
        }

        for (method_name, method_sig) in &trait_info.methods {
            if !method_sig.generics.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1214",
                        format!(
                            "trait '{}.{}' is not object-safe: generic methods are not supported for dyn dispatch",
                            resolved_trait_name, method_name
                        ),
                        self.file,
                        span,
                    )
                    .with_help("remove method generics for dyn-compatible trait methods"),
                );
                ok = false;
            }

            let param_types = method_sig
                .param_types
                .iter()
                .map(|ty| {
                    self.types
                        .get(ty)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string())
                })
                .collect::<Vec<_>>();
            if param_types.is_empty() || self.normalize_type(&param_types[0]) != "Self" {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1214",
                        format!(
                            "trait '{}.{}' is not object-safe: first parameter must be receiver `Self`",
                            resolved_trait_name, method_name
                        ),
                        self.file,
                        span,
                    )
                    .with_help("use `fn method(self: Self, ...)` in dyn-compatible traits"),
                );
                ok = false;
            }

            for param_ty in param_types.iter().skip(1) {
                if type_uses_self(param_ty) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1214",
                            format!(
                                "trait '{}.{}' is not object-safe: `Self` may only appear in receiver position",
                                resolved_trait_name, method_name
                            ),
                            self.file,
                            span,
                        )
                        .with_help("remove `Self` from non-receiver parameters"),
                    );
                    ok = false;
                }
            }

            let ret_ty = self
                .types
                .get(&method_sig.ret_type)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            if type_uses_self(&ret_ty) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1214",
                        format!(
                            "trait '{}.{}' is not object-safe: `Self` may not appear in return type",
                            resolved_trait_name, method_name
                        ),
                        self.file,
                        span,
                    )
                    .with_help("return concrete or generic-independent types for dyn dispatch"),
                );
                ok = false;
            }
        }

        ok
    }

    #[allow(clippy::too_many_arguments)]
    fn check_dyn_trait_method_call(
        &mut self,
        trait_name: &str,
        field: &str,
        args: &[ir::Expr],
        arg_names: &[Option<String>],
        call_expr: &ir::Expr,
        locals: &mut BTreeMap<String, String>,
        allowed_effects: &BTreeSet<String>,
        ctx: &mut ExprContext,
        contract_mode: bool,
        _expected_ty: Option<&str>,
    ) -> String {
        if Self::has_named_call_args(arg_names) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1213",
                    format!(
                        "named arguments are not currently supported for method call dyn {}.{}",
                        trait_name, field
                    ),
                    self.file,
                    call_expr.span,
                )
                .with_help("use positional arguments for method calls"),
            );
        }

        let Some((resolved_trait_name, trait_info)) = self.lookup_trait_info(trait_name) else {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1228",
                    format!(
                        "unknown dyn trait '{}' for receiver method '{}'",
                        trait_name, field
                    ),
                    self.file,
                    call_expr.span,
                )
                .with_help("declare the trait or import the correct module"),
            );
            return "<?>".to_string();
        };
        if !self.validate_dyn_trait_object_safety(&resolved_trait_name, call_expr.span) {
            return "<?>".to_string();
        }

        let method_name = method_name_key(field);
        let Some(method_sig) = trait_info.methods.get(method_name).cloned() else {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1228",
                    format!(
                        "unknown method 'dyn {}.{}'",
                        resolved_trait_name, method_name
                    ),
                    self.file,
                    call_expr.span,
                )
                .with_help("call a method declared on this trait"),
            );
            return "<?>".to_string();
        };

        let method_params = method_sig
            .param_types
            .iter()
            .map(|ty| {
                self.types
                    .get(ty)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string())
            })
            .collect::<Vec<_>>();
        if method_params.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E1213",
                format!(
                    "method 'dyn {}.{}' has invalid trait signature: receiver parameter missing",
                    resolved_trait_name, method_name
                ),
                self.file,
                call_expr.span,
            ));
            return "<?>".to_string();
        }

        if args.len() + 1 != method_params.len() {
            self.diagnostics.push(Diagnostic::error(
                "E1213",
                format!(
                    "method 'dyn {}.{}' expects {} args, got {}",
                    resolved_trait_name,
                    method_name,
                    method_params.len() - 1,
                    args.len()
                ),
                self.file,
                call_expr.span,
            ));
        }

        for (idx, arg) in args.iter().enumerate() {
            let expected = method_params.get(idx + 1).map(String::as_str);
            let found = self.check_expr_with_expected(
                arg,
                locals,
                allowed_effects,
                ctx,
                contract_mode,
                expected,
            );
            if let Some(expected) = expected {
                if !self.argument_type_compatible(expected, &found, arg) {
                    self.diagnostics.push(Diagnostic::error(
                        "E1214",
                        format!(
                            "argument {} to 'dyn {}.{}' expected '{}', found '{}'",
                            idx + 1,
                            resolved_trait_name,
                            method_name,
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
                    .with_help("remove IO/time/rand/net/fs calls from requires/ensures/invariant"),
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
                            "calling 'dyn {}.{}' requires undeclared effects: {}",
                            resolved_trait_name,
                            method_name,
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
                        "call to unsafe method 'dyn {}.{}' requires an explicit unsafe boundary",
                        resolved_trait_name, method_name
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
        let mut bindings = BTreeMap::new();
        bindings.insert(
            "Self".to_string(),
            format!("dyn {}", resolved_trait_name.clone()),
        );
        let generic_params = method_sig.generics.iter().cloned().collect::<BTreeSet<_>>();
        let mut ret = substitute_type_vars(&ret_raw, &bindings, &generic_params);
        if method_sig.is_async {
            ret = format!("Async[{ret}]");
        }
        ret
    }

    #[allow(clippy::too_many_arguments)]
    fn check_for_into_iter_call(
        &mut self,
        args: &[ir::Expr],
        arg_names: &[Option<String>],
        call_expr: &ir::Expr,
        locals: &mut BTreeMap<String, String>,
        allowed_effects: &BTreeSet<String>,
        ctx: &mut ExprContext,
        contract_mode: bool,
        expected_ty: Option<&str>,
    ) -> String {
        if Self::has_named_call_args(arg_names) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1213",
                    "named arguments are not supported for compiler-generated iterator helpers",
                    self.file,
                    call_expr.span,
                )
                .with_help("use positional arguments"),
            );
        }
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E1213",
                format!("'aic_for_into_iter' expects 1 arg, got {}", args.len()),
                self.file,
                call_expr.span,
            ));
            return "<?>".to_string();
        }

        let source = &args[0];
        let source_ty = self.check_expr(source, locals, allowed_effects, ctx, contract_mode);
        let normalized = self.normalize_type(&source_ty);
        let receiver_ty = match base_type_name(&normalized) {
            "Ref" | "RefMut" => extract_generic_args(&normalized)
                .and_then(|vals| vals.first().cloned())
                .unwrap_or(normalized),
            _ => normalized,
        };
        let receiver_name = base_type_name(&receiver_ty).to_string();
        let iter_assoc = format!("{receiver_name}::iter");
        if self.functions.contains_key(&iter_assoc) {
            return self.check_method_call(
                source,
                "iter",
                &[],
                &[],
                call_expr,
                locals,
                allowed_effects,
                ctx,
                contract_mode,
                expected_ty,
            );
        }

        let next_assoc = format!("{receiver_name}::next");
        if self.functions.contains_key(&next_assoc) {
            return source_ty;
        }

        self.diagnostics.push(
            Diagnostic::error(
                "E1228",
                format!(
                    "for-in source of type '{}' is not iterable: missing '{}.iter()' or '{}.next()'",
                    receiver_ty, receiver_name, receiver_name
                ),
                self.file,
                call_expr.span,
            )
            .with_help("implement `iter` or `next` for this type"),
        );
        "<?>".to_string()
    }

    #[allow(clippy::too_many_arguments)]
    fn check_for_next_iter_call(
        &mut self,
        args: &[ir::Expr],
        arg_names: &[Option<String>],
        call_expr: &ir::Expr,
        locals: &mut BTreeMap<String, String>,
        allowed_effects: &BTreeSet<String>,
        ctx: &mut ExprContext,
        contract_mode: bool,
        expected_ty: Option<&str>,
    ) -> String {
        if Self::has_named_call_args(arg_names) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1213",
                    "named arguments are not supported for compiler-generated iterator helpers",
                    self.file,
                    call_expr.span,
                )
                .with_help("use positional arguments"),
            );
        }
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E1213",
                format!("'aic_for_next_iter' expects 1 arg, got {}", args.len()),
                self.file,
                call_expr.span,
            ));
            return "<?>".to_string();
        }
        self.check_method_call(
            &args[0],
            "next",
            &[],
            &[],
            call_expr,
            locals,
            allowed_effects,
            ctx,
            contract_mode,
            expected_ty,
        )
    }

    fn check_struct_default_call(
        &mut self,
        callable_name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        expected_ty: Option<&str>,
    ) -> Option<String> {
        let (struct_name, method_name) = callable_name.rsplit_once("::")?;
        if method_name != "default" {
            return None;
        }

        let Some(info) = self.resolution.structs.get(struct_name).cloned() else {
            return None;
        };

        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E1213",
                format!(
                    "function '{}' expects 0 args, got {}",
                    callable_name,
                    args.len()
                ),
                self.file,
                span,
            ));
            return Some("<?>".to_string());
        }

        if info.default_fields.len() != info.fields.len() {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1218",
                    format!(
                        "auto-generated '{}' is unavailable because not all fields have defaults",
                        callable_name
                    ),
                    self.file,
                    span,
                )
                .with_help("add default values for every struct field"),
            );
            return Some("<?>".to_string());
        }

        if info.generics.is_empty() {
            return Some(struct_name.to_string());
        }

        if let Some(expected) = expected_ty {
            let expected_norm = self.normalize_type(expected);
            if base_type_name(&expected_norm) == struct_name {
                let applied = extract_generic_args(&expected_norm).unwrap_or_default();
                if applied.len() == info.generics.len() {
                    if applied.iter().all(|ty| !contains_unresolved_type(ty)) {
                        self.record_instantiation(
                            ir::GenericInstantiationKind::Struct,
                            struct_name,
                            Some(info.symbol),
                            &applied,
                            span,
                        );
                    }
                    return Some(expected_norm);
                }
            }
        }

        self.diagnostics.push(
            Diagnostic::error(
                "E1212",
                format!(
                    "cannot infer generic parameters for auto-generated '{}'",
                    callable_name
                ),
                self.file,
                span,
            )
            .with_help("add a type annotation such as `let x: Struct[T] = Struct::default();`"),
        );

        Some(format!(
            "{}[{}]",
            struct_name,
            vec!["<?>"; info.generics.len()].join(", ")
        ))
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
        if let Some(dyn_trait) = parse_dyn_trait_name(ty) {
            let _ = self.validate_dyn_trait_object_safety(&dyn_trait, span);
            return;
        }
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
        let lhs_int = parse_integer_kind(&lhs_norm);
        let rhs_int = parse_integer_kind(&rhs_norm);
        let lhs_float = parse_float_kind(&lhs_norm);
        let rhs_float = parse_float_kind(&rhs_norm);
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let symbol = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    _ => unreachable!(),
                };
                if let (Some(lhs_float), Some(rhs_float)) = (lhs_float, rhs_float) {
                    if lhs_float == rhs_float {
                        float_kind_type_name(lhs_float).to_string()
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1230",
                            format!(
                                "arithmetic operator '{}' requires matching float widths, found '{}' and '{}'",
                                symbol, lhs, rhs
                            ),
                            self.file,
                            span,
                        ));
                        "<?>".to_string()
                    }
                } else if let (Some(lhs_int), Some(rhs_int)) = (lhs_int, rhs_int) {
                    if integer_kinds_match_exact(lhs_int, rhs_int) {
                        lhs_norm
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1230",
                            format!(
                                "arithmetic operator '{}' requires matching integer signedness/width, found '{}' and '{}'",
                                symbol, lhs, rhs
                            ),
                            self.file,
                            span,
                        ));
                        "<?>".to_string()
                    }
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        "E1230",
                        format!(
                            "arithmetic operator '{}' requires matching integer or float operands, found '{}' and '{}'",
                            symbol, lhs, rhs
                        ),
                        self.file,
                        span,
                    ));
                    "<?>".to_string()
                }
            }
            BinOp::Mod => {
                if let (Some(lhs_int), Some(rhs_int)) = (lhs_int, rhs_int) {
                    if !integer_kinds_match_exact(lhs_int, rhs_int) {
                        self.diagnostics.push(Diagnostic::error(
                            "E1230",
                            format!(
                                "operator '%' requires matching integer signedness/width, found '{}' and '{}'",
                                lhs, rhs
                            ),
                            self.file,
                            span,
                        ));
                        return "<?>".to_string();
                    }
                    lhs_norm
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        "E1230",
                        format!(
                            "operator '%' requires integer operands, found '{}' and '{}'",
                            lhs, rhs
                        ),
                        self.file,
                        span,
                    ));
                    "<?>".to_string()
                }
            }
            BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::BitXor
            | BinOp::Shl
            | BinOp::Shr
            | BinOp::Ushr => {
                let symbol = match op {
                    BinOp::BitAnd => "&",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Shl => "<<",
                    BinOp::Shr => ">>",
                    BinOp::Ushr => ">>>",
                    _ => unreachable!(),
                };
                if let (Some(lhs_int), Some(rhs_int)) = (lhs_int, rhs_int) {
                    if integer_kinds_match_exact(lhs_int, rhs_int) {
                        lhs_norm
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1230",
                            format!(
                                "bitwise/shift operator '{}' requires matching integer signedness/width, found '{}' and '{}'",
                                symbol, lhs, rhs
                            ),
                            self.file,
                            span,
                        ));
                        "<?>".to_string()
                    }
                } else {
                    let mut diag = Diagnostic::error(
                        "E1230",
                        format!(
                            "bitwise/shift operator '{}' requires integer operands, found '{}' and '{}'",
                            symbol, lhs, rhs
                        ),
                        self.file,
                        span,
                    );
                    if matches!(op, BinOp::BitAnd | BinOp::BitOr)
                        && lhs_norm == "Bool"
                        && rhs_norm == "Bool"
                    {
                        diag = diag.with_help("for logical operations on Bool, use '&&' or '||'");
                    }
                    self.diagnostics.push(diag);
                    "<?>".to_string()
                }
            }
            BinOp::Eq | BinOp::Ne => {
                if let (Some(lhs_int), Some(rhs_int)) = (lhs_int, rhs_int) {
                    if !integer_kinds_match_exact(lhs_int, rhs_int) {
                        self.diagnostics.push(Diagnostic::error(
                            "E1231",
                            format!(
                                "equality operands for fixed-width integers must match signedness/width, found '{}' and '{}'",
                                lhs, rhs
                            ),
                            self.file,
                            span,
                        ));
                    }
                } else if let (Some(lhs_float), Some(rhs_float)) = (lhs_float, rhs_float) {
                    if lhs_float != rhs_float {
                        self.diagnostics.push(Diagnostic::error(
                            "E1231",
                            format!(
                                "equality operands for floats must match width, found '{}' and '{}'",
                                lhs, rhs
                            ),
                            self.file,
                            span,
                        ));
                    }
                } else if !self.types_compatible(lhs, rhs) {
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
                if let (Some(lhs_int), Some(rhs_int)) = (lhs_int, rhs_int) {
                    if !integer_kinds_match_exact(lhs_int, rhs_int) {
                        self.diagnostics.push(Diagnostic::error(
                            "E1232",
                            format!(
                                "comparison operators for fixed-width integers require matching signedness/width, found '{}' and '{}'",
                                lhs, rhs
                            ),
                            self.file,
                            span,
                        ));
                    }
                } else if let (Some(lhs_float), Some(rhs_float)) = (lhs_float, rhs_float) {
                    if lhs_float != rhs_float {
                        self.diagnostics.push(Diagnostic::error(
                            "E1232",
                            format!(
                                "comparison operators for floats require matching width, found '{}' and '{}'",
                                lhs, rhs
                            ),
                            self.file,
                            span,
                        ));
                    }
                } else if !(lhs_norm == rhs_norm && lhs_norm == "Char") {
                    self.diagnostics.push(Diagnostic::error(
                        "E1232",
                        "comparison operators require matching integer, float, or Char operands",
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
            ir::PatternKind::Int(v) => {
                if parse_integer_kind(&normalized_scrutinee_ty).is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        "E1234",
                        format!(
                            "int pattern requires integer scrutinee, found '{}'",
                            scrutinee_ty
                        ),
                        self.file,
                        pattern.span,
                    ));
                    return;
                }

                let pattern_meta = pattern.int_literal_metadata();
                if let Some(meta) = pattern_meta.as_ref() {
                    let suffix_ty =
                        integer_kind_type_name(integer_kind_from_literal_kind(meta.kind));
                    let literal_value = self
                        .non_negative_integer_literal_value(*v, Some(meta))
                        .unwrap_or(IntegerLiteralValue::NonNegative((*v).max(0) as u128));
                    self.validate_integer_literal_range(
                        literal_value,
                        suffix_ty,
                        pattern.span,
                        "E1234",
                        &format!(
                            "pattern integer literal '{}' with suffix '{}'",
                            meta.raw_literal_text, meta.suffix_text
                        ),
                    );
                }

                let pattern_value = self
                    .non_negative_integer_literal_value(*v, pattern_meta.as_ref())
                    .unwrap_or(IntegerLiteralValue::NonNegative((*v).max(0) as u128));
                self.validate_integer_literal_range(
                    pattern_value,
                    &normalized_scrutinee_ty,
                    pattern.span,
                    "E1234",
                    &format!("pattern integer literal '{}'", v),
                );
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
            ir::PatternKind::Char(_v) => {
                if normalized_scrutinee_ty != "Char" {
                    self.diagnostics.push(Diagnostic::error(
                        "E1245",
                        format!(
                            "char pattern requires Char scrutinee, found '{}'",
                            scrutinee_ty
                        ),
                        self.file,
                        pattern.span,
                    ));
                }
            }
            ir::PatternKind::String(_v) => {
                if normalized_scrutinee_ty != "String" {
                    self.diagnostics.push(Diagnostic::error(
                        "E1245",
                        format!(
                            "string pattern requires String scrutinee, found '{}'",
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
            ir::PatternKind::Struct {
                name,
                fields,
                has_rest,
            } => {
                let Some(struct_info) = self.find_struct(&normalized_scrutinee_ty).cloned() else {
                    self.diagnostics.push(Diagnostic::error(
                        "E1245",
                        format!(
                            "struct pattern '{}' not valid for type '{}'",
                            name, scrutinee_ty
                        ),
                        self.file,
                        pattern.span,
                    ));
                    for field in fields {
                        self.check_pattern(&field.pattern, "<?>", locals, bound_names);
                    }
                    return;
                };

                let scrutinee_name = base_type_name(&normalized_scrutinee_ty);
                if name != scrutinee_name
                    && Self::unqualified_name(name) != Self::unqualified_name(scrutinee_name)
                {
                    self.diagnostics.push(Diagnostic::error(
                        "E1245",
                        format!(
                            "struct pattern '{}' does not match scrutinee type '{}'",
                            name, scrutinee_ty
                        ),
                        self.file,
                        pattern.span,
                    ));
                }

                let struct_bindings =
                    bindings_from_applied_type(&normalized_scrutinee_ty, &struct_info.generics);
                if struct_bindings.is_none() && !struct_info.generics.is_empty() {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1250",
                            format!(
                                "generic arity mismatch for struct '{}': expected {} arguments",
                                base_type_name(scrutinee_ty),
                                struct_info.generics.len()
                            ),
                            self.file,
                            pattern.span,
                        )
                        .with_help("fix the generic arguments on the scrutinee type"),
                    );
                }
                let struct_bindings = struct_bindings.unwrap_or_default();
                let struct_generic_set = struct_info
                    .generics
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>();

                let mut seen_fields = BTreeSet::new();
                for field in fields {
                    if !seen_fields.insert(field.name.clone()) {
                        self.diagnostics.push(Diagnostic::error(
                            "E1245",
                            format!(
                                "duplicate field '{}' in struct pattern '{}'",
                                field.name, name
                            ),
                            self.file,
                            field.pattern.span,
                        ));
                        continue;
                    }

                    let Some(field_ty_id) = struct_info.fields.get(&field.name) else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1245",
                            format!("unknown field '{}.{}' in struct pattern", name, field.name),
                            self.file,
                            field.pattern.span,
                        ));
                        self.check_pattern(&field.pattern, "<?>", locals, bound_names);
                        continue;
                    };

                    let field_visibility = struct_info
                        .field_visibility
                        .get(&field.name)
                        .copied()
                        .unwrap_or(Visibility::Public);
                    if !self.field_is_accessible_from_current(&struct_info.module, field_visibility)
                    {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1245",
                                format!(
                                    "field '{}.{}' is not visible in this module",
                                    name, field.name
                                ),
                                self.file,
                                field.pattern.span,
                            )
                            .with_help("destructure only fields visible from this module"),
                        );
                        continue;
                    }

                    let field_raw = self
                        .types
                        .get(field_ty_id)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string());
                    let field_ty =
                        substitute_type_vars(&field_raw, &struct_bindings, &struct_generic_set);
                    self.check_pattern(&field.pattern, &field_ty, locals, bound_names);
                }

                if !*has_rest {
                    let missing = struct_info
                        .fields
                        .keys()
                        .filter(|field_name| !seen_fields.contains(*field_name))
                        .filter(|field_name| {
                            let visibility = struct_info
                                .field_visibility
                                .get(*field_name)
                                .copied()
                                .unwrap_or(Visibility::Public);
                            self.field_is_accessible_from_current(&struct_info.module, visibility)
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    if !missing.is_empty() {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1245",
                                format!(
                                    "non-exhaustive struct pattern '{}'; missing fields: {}",
                                    name,
                                    missing.join(", ")
                                ),
                                self.file,
                                pattern.span,
                            )
                            .with_help("add missing fields or use `..` to ignore remaining fields"),
                        );
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
                if parse_integer_kind(&normalized_scrutinee_ty).is_some() {
                    seen.insert(format!("int:{v}"));
                }
            }
            ir::PatternKind::Char(v) => {
                if normalized_scrutinee_ty == "Char" {
                    seen.insert(format!("char:{}", *v as u32));
                }
            }
            ir::PatternKind::String(v) => {
                if normalized_scrutinee_ty == "String" {
                    seen.insert(format!("string:{v:?}"));
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
            ir::PatternKind::Struct { .. } => {}
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
                missing.push("None".to_string());
            }
            if !seen.contains("Some") {
                missing.push("Some".to_string());
            }
            if !missing.is_empty() {
                let mut diagnostic = Diagnostic::error(
                    "E1247",
                    Self::format_missing_variant_message(&missing),
                    self.file,
                    span,
                )
                .with_help("add missing variant arms or `_` wildcard");
                if let Some(fix) = self.missing_variants_fix(&normalized_scrutinee_ty, &missing) {
                    diagnostic = diagnostic.with_fix(fix);
                }
                self.diagnostics.push(diagnostic);
            }
            return;
        }

        if normalized_scrutinee_ty.starts_with("Result[") {
            let mut missing = Vec::new();
            if !seen.contains("Ok") {
                missing.push("Ok".to_string());
            }
            if !seen.contains("Err") {
                missing.push("Err".to_string());
            }
            if !missing.is_empty() {
                let mut diagnostic = Diagnostic::error(
                    "E1248",
                    Self::format_missing_variant_message(&missing),
                    self.file,
                    span,
                )
                .with_help("add missing variant arms or `_` wildcard");
                if let Some(fix) = self.missing_variants_fix(&normalized_scrutinee_ty, &missing) {
                    diagnostic = diagnostic.with_fix(fix);
                }
                self.diagnostics.push(diagnostic);
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
                let mut diagnostic = Diagnostic::error(
                    "E1249",
                    Self::format_missing_variant_message(&missing),
                    self.file,
                    span,
                )
                .with_help("add missing variant arms or `_` wildcard");
                if let Some(fix) = self.missing_variants_fix(&normalized_scrutinee_ty, &missing) {
                    diagnostic = diagnostic.with_fix(fix);
                }
                self.diagnostics.push(diagnostic);
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
            ir::PatternKind::Char(v) => seen.contains(&format!("char:{}", *v as u32)),
            ir::PatternKind::String(v) => seen.contains(&format!("string:{v:?}")),
            ir::PatternKind::Bool(v) => seen.contains(if *v { "true" } else { "false" }),
            ir::PatternKind::Unit => seen.contains("()"),
            ir::PatternKind::Or { patterns } => patterns
                .iter()
                .all(|p| self.arm_is_redundant(p, scrutinee_ty, seen, wildcard_seen)),
            ir::PatternKind::Struct { .. } => false,
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

    fn format_missing_variant_message(missing: &[String]) -> String {
        if missing.len() == 1 {
            return format!("non-exhaustive match: missing variant `{}`", missing[0]);
        }
        let rendered = missing
            .iter()
            .map(|variant| format!("`{variant}`"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("non-exhaustive match: missing variants {rendered}")
    }

    fn missing_variants_fix(&self, scrutinee_ty: &str, missing: &[String]) -> Option<SuggestedFix> {
        let arms = missing
            .iter()
            .filter_map(|variant| self.missing_variant_arm(scrutinee_ty, variant))
            .collect::<Vec<_>>();
        if arms.is_empty() {
            return None;
        }
        let message = if arms.len() == 1 {
            "insert missing match arm".to_string()
        } else {
            "insert missing match arms".to_string()
        };
        Some(SuggestedFix {
            message,
            replacement: Some(arms.join("\n")),
            start: None,
            end: None,
        })
    }

    fn missing_variant_arm(&self, scrutinee_ty: &str, variant: &str) -> Option<String> {
        if scrutinee_ty.starts_with("Option[") {
            return Some(match variant {
                "None" => "None => todo(),".to_string(),
                "Some" => "Some(_) => todo(),".to_string(),
                _ => return None,
            });
        }
        if scrutinee_ty.starts_with("Result[") {
            return Some(match variant {
                "Ok" => "Ok(_) => todo(),".to_string(),
                "Err" => "Err(_) => todo(),".to_string(),
                _ => return None,
            });
        }
        let enum_info = self.find_enum(scrutinee_ty)?;
        let payload = enum_info.variants.get(variant)?;
        if payload.is_some() {
            Some(format!("{variant}(_) => todo(),"))
        } else {
            Some(format!("{variant} => todo(),"))
        }
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
            if !self.field_is_accessible_from_current(&info.module, info.visibility) {
                continue;
            }
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
        if let Some(info) = self.resolution.enums.get(base) {
            return Some(info);
        }
        self.resolution.enums.get(Self::unqualified_name(base))
    }

    fn find_struct(&self, ty: &str) -> Option<&StructInfo> {
        let normalized = self.normalize_type(ty);
        let base = base_type_name(&normalized);
        if let Some(info) = self.resolution.structs.get(base) {
            return Some(info);
        }
        self.resolution.structs.get(Self::unqualified_name(base))
    }

    fn is_auto_marker_trait(bound_trait: &str) -> bool {
        matches!(Self::unqualified_name(bound_trait), "Send" | "Sync")
    }

    fn unqualified_name(name: &str) -> &str {
        name.rsplit('.').next().unwrap_or(name)
    }

    fn canonical_trait_impl_type(ty: &str) -> String {
        let trimmed = ty.trim();
        if let Some(trait_name) = parse_dyn_trait_name(trimmed) {
            return format!("dyn {}", Self::unqualified_name(&trait_name));
        }
        if let Some(args) = extract_generic_args(trimmed) {
            let base = Self::unqualified_name(base_type_name(trimmed));
            let canonical_args = args
                .iter()
                .map(|arg| Self::canonical_trait_impl_type(arg))
                .collect::<Vec<_>>();
            return format!("{base}[{}]", canonical_args.join(", "));
        }
        Self::unqualified_name(trimmed).to_string()
    }

    fn trait_is_implemented_for(&self, bound_trait: &str, bound_ty: &str) -> bool {
        let target_trait = Self::unqualified_name(bound_trait);
        let bound_ty_norm = self.normalize_type(bound_ty);
        let bound_ty_key = Self::canonical_trait_impl_type(&bound_ty_norm);
        self.resolution
            .trait_impls
            .iter()
            .any(|(trait_name, impls)| {
                if Self::unqualified_name(trait_name) != target_trait {
                    return false;
                }
                impls.iter().any(|implemented_ty| {
                    let implemented_norm = self.normalize_type(implemented_ty);
                    let implemented_key = Self::canonical_trait_impl_type(&implemented_norm);
                    implemented_key == bound_ty_key
                })
            })
    }

    fn enforce_std_concurrency_send_bounds(
        &mut self,
        resolved_module: Option<&str>,
        resolved_name: &str,
        sig: &FnSig,
        generic_bindings: &BTreeMap<String, String>,
        span: crate::span::Span,
    ) {
        if resolved_module != Some("std.concurrent") {
            return;
        }
        if !matches!(
            resolved_name,
            "send" | "try_send" | "spawn" | "spawn_named" | "scope_spawn"
        ) {
            return;
        }

        let Some(generic_name) = sig.generic_params.first() else {
            return;
        };
        let Some(bound_ty) = generic_bindings.get(generic_name) else {
            return;
        };
        if contains_unresolved_type(bound_ty) || contains_symbolic_generic_type(bound_ty) {
            return;
        }
        if sig
            .generic_bounds
            .get(generic_name)
            .map(|bounds| {
                bounds.iter().any(|bound_trait| {
                    Self::is_auto_marker_trait(bound_trait)
                        && Self::unqualified_name(bound_trait) == "Send"
                })
            })
            .unwrap_or(false)
        {
            return;
        }

        let Some(reason) = self.marker_trait_failure_reason("Send", bound_ty) else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::error(
                "E1258",
                format!(
                    "type '{}' does not satisfy trait bound '{}: Send'",
                    bound_ty, generic_name
                ),
                self.file,
                span,
            )
            .with_help(format!(
                "{}; use a Send-safe type or move only Send values across concurrency boundaries",
                reason
            )),
        );
    }

    fn marker_trait_failure_reason(&self, bound_trait: &str, ty: &str) -> Option<String> {
        if !Self::is_auto_marker_trait(bound_trait) {
            return None;
        }
        let mut visiting = BTreeSet::new();
        self.marker_trait_failure_reason_inner(bound_trait, ty, &mut visiting)
    }

    fn marker_trait_failure_reason_inner(
        &self,
        bound_trait: &str,
        ty: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<String> {
        let normalized = self.normalize_type(ty);
        if contains_unresolved_type(&normalized) || contains_symbolic_generic_type(&normalized) {
            return None;
        }

        let visit_key = format!("{bound_trait}:{normalized}");
        if !visiting.insert(visit_key.clone()) {
            return None;
        }

        let outcome = (|| {
            let base = base_type_name(&normalized);
            let base_name = Self::unqualified_name(base);
            let marker_name = Self::unqualified_name(bound_trait);

            if matches!(
                base_name,
                "Int" | "Float32" | "Float64" | "Bool" | "Char" | "String" | "()"
            ) {
                return None;
            }

            if matches!(
                base_name,
                "Mutex"
                    | "RwLock"
                    | "Arc"
                    | "AtomicInt"
                    | "AtomicBool"
                    | "ThreadLocal"
                    | "Sender"
                    | "Receiver"
                    | "Task"
                    | "Scope"
                    | "IntChannel"
                    | "IntMutex"
                    | "IntRwLock"
            ) {
                // Synchronization primitives are explicitly thread-safe wrappers.
                return None;
            }

            if matches!(base_name, "Fn" | "Async") {
                return Some(format!("type '{}' is not {}", normalized, marker_name));
            }

            if matches!(
                base_name,
                "FileHandle"
                    | "TcpReader"
                    | "TcpListener"
                    | "TcpConnection"
                    | "ProcessHandle"
                    | "AsyncIntOp"
                    | "AsyncStringOp"
                    | "ByteBuffer"
            ) {
                return Some(format!(
                    "type '{}' is a runtime handle and is not {}",
                    normalized, marker_name
                ));
            }

            if let Some(args) = extract_generic_args(&normalized) {
                if !matches!(base_name, "Mutex" | "RwLock" | "Arc" | "ThreadLocal") {
                    for (idx, arg) in args.iter().enumerate() {
                        if let Some(reason) =
                            self.marker_trait_failure_reason_inner(bound_trait, arg, visiting)
                        {
                            return Some(format!(
                                "type argument {} ('{}') of '{}' is not {}: {}",
                                idx + 1,
                                arg,
                                normalized,
                                marker_name,
                                reason
                            ));
                        }
                    }
                }
            }

            if let Some(info) = self.find_struct(&normalized) {
                let bindings =
                    bindings_from_applied_type(&normalized, &info.generics).unwrap_or_default();
                let generic_set = info.generics.iter().cloned().collect::<BTreeSet<_>>();
                for (field_name, field_ty_id) in &info.fields {
                    let raw_field_ty = self
                        .types
                        .get(field_ty_id)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string());
                    let field_ty = substitute_type_vars(&raw_field_ty, &bindings, &generic_set);
                    if let Some(reason) =
                        self.marker_trait_failure_reason_inner(bound_trait, &field_ty, visiting)
                    {
                        return Some(format!(
                            "field '{}.{}' has non-{} type '{}': {}",
                            base_name, field_name, marker_name, field_ty, reason
                        ));
                    }
                }
                return None;
            }

            if let Some(info) = self.find_enum(&normalized) {
                let bindings =
                    bindings_from_applied_type(&normalized, &info.generics).unwrap_or_default();
                let generic_set = info.generics.iter().cloned().collect::<BTreeSet<_>>();
                for (variant_name, payload_id) in &info.variants {
                    let Some(payload_id) = payload_id else {
                        continue;
                    };
                    let raw_payload_ty = self
                        .types
                        .get(payload_id)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string());
                    let payload_ty = substitute_type_vars(&raw_payload_ty, &bindings, &generic_set);
                    if let Some(reason) =
                        self.marker_trait_failure_reason_inner(bound_trait, &payload_ty, visiting)
                    {
                        return Some(format!(
                            "variant '{}.{}' has non-{} payload type '{}': {}",
                            base_name, variant_name, marker_name, payload_ty, reason
                        ));
                    }
                }
                return None;
            }

            None
        })();

        visiting.remove(&visit_key);
        outcome
    }

    fn normalize_type(&self, ty: &str) -> String {
        self.expand_aliases(ty, &mut BTreeSet::new())
    }

    fn expand_aliases(&self, ty: &str, visiting: &mut BTreeSet<String>) -> String {
        let base = base_type_name(ty);
        let canonical_base = canonical_builtin_type_name(base);
        let normalized_args = extract_generic_args(ty).map(|args| {
            args.iter()
                .map(|arg| self.expand_aliases(arg, visiting))
                .collect::<Vec<_>>()
        });

        let Some(alias) = self.type_aliases.get(base) else {
            return if let Some(args) = normalized_args {
                format!("{canonical_base}[{}]", args.join(", "))
            } else if canonical_base != base {
                canonical_base.to_string()
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
        self.types_compatible_with_dyn(&expected_norm, &found_norm)
    }

    fn types_compatible_with_dyn(&self, expected: &str, found: &str) -> bool {
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

        if let (Some(expected_kind), Some(found_kind)) =
            (parse_integer_kind(expected), parse_integer_kind(found))
        {
            return integer_conversion_is_lossless(expected_kind, found_kind);
        }
        if let (Some(expected_kind), Some(found_kind)) =
            (parse_float_kind(expected), parse_float_kind(found))
        {
            return expected_kind == found_kind;
        }

        if let Some(expected_dyn_trait) = parse_dyn_trait_name(expected) {
            if let Some(found_dyn_trait) = parse_dyn_trait_name(found) {
                let expected_name = Self::unqualified_name(&expected_dyn_trait);
                let found_name = Self::unqualified_name(&found_dyn_trait);
                return expected_name == found_name;
            }
            return self.trait_is_implemented_for(&expected_dyn_trait, found);
        }

        let expected_args = extract_generic_args(expected).unwrap_or_default();
        let found_args = extract_generic_args(found).unwrap_or_default();
        if expected_args.is_empty() || found_args.is_empty() {
            return false;
        }
        if base_type_name(expected) != base_type_name(found)
            || expected_args.len() != found_args.len()
        {
            return false;
        }
        expected_args
            .iter()
            .zip(found_args.iter())
            .all(|(exp, got)| self.types_compatible_with_dyn(exp, got))
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

    if let (Some(expected_kind), Some(found_kind)) =
        (parse_integer_kind(expected), parse_integer_kind(found))
    {
        return integer_conversion_is_lossless(expected_kind, found_kind);
    }
    if let (Some(expected_kind), Some(found_kind)) =
        (parse_float_kind(expected), parse_float_kind(found))
    {
        return expected_kind == found_kind;
    }

    if let Some(expected_dyn_trait) = parse_dyn_trait_name(expected) {
        if let Some(found_dyn_trait) = parse_dyn_trait_name(found) {
            let expected_name = expected_dyn_trait
                .rsplit('.')
                .next()
                .unwrap_or(&expected_dyn_trait);
            let found_name = found_dyn_trait
                .rsplit('.')
                .next()
                .unwrap_or(&found_dyn_trait);
            return expected_name == found_name;
        }
        // Generic inference uses this helper before full trait-impl checks.
        // Accept concrete candidates here and let semantic compatibility checks
        // validate actual trait implementation later.
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

fn resource_protocol_op(name: &str) -> Option<ResourceProtocolOp> {
    match name {
        "send_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntChannel,
            terminal: false,
            api: "send_int",
            first_param_base_type: "IntChannel",
            required_effect: "concurrency",
        }),
        "recv_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntChannel,
            terminal: false,
            api: "recv_int",
            first_param_base_type: "IntChannel",
            required_effect: "concurrency",
        }),
        "close_channel" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntChannel,
            terminal: true,
            api: "close_channel",
            first_param_base_type: "IntChannel",
            required_effect: "concurrency",
        }),
        "send" => Some(ResourceProtocolOp {
            kind: ResourceKind::Sender,
            terminal: false,
            api: "send",
            first_param_base_type: "Sender",
            required_effect: "concurrency",
        }),
        "try_send" => Some(ResourceProtocolOp {
            kind: ResourceKind::Sender,
            terminal: false,
            api: "try_send",
            first_param_base_type: "Sender",
            required_effect: "concurrency",
        }),
        "close_sender" => Some(ResourceProtocolOp {
            kind: ResourceKind::Sender,
            terminal: true,
            api: "close_sender",
            first_param_base_type: "Sender",
            required_effect: "concurrency",
        }),
        "recv" => Some(ResourceProtocolOp {
            kind: ResourceKind::Receiver,
            terminal: false,
            api: "recv",
            first_param_base_type: "Receiver",
            required_effect: "concurrency",
        }),
        "try_recv" => Some(ResourceProtocolOp {
            kind: ResourceKind::Receiver,
            terminal: false,
            api: "try_recv",
            first_param_base_type: "Receiver",
            required_effect: "concurrency",
        }),
        "recv_timeout" => Some(ResourceProtocolOp {
            kind: ResourceKind::Receiver,
            terminal: false,
            api: "recv_timeout",
            first_param_base_type: "Receiver",
            required_effect: "concurrency",
        }),
        "close_receiver" => Some(ResourceProtocolOp {
            kind: ResourceKind::Receiver,
            terminal: true,
            api: "close_receiver",
            first_param_base_type: "Receiver",
            required_effect: "concurrency",
        }),
        "lock_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntMutex,
            terminal: false,
            api: "lock_int",
            first_param_base_type: "IntMutex",
            required_effect: "concurrency",
        }),
        "unlock_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntMutex,
            terminal: false,
            api: "unlock_int",
            first_param_base_type: "IntMutex",
            required_effect: "concurrency",
        }),
        "close_mutex" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntMutex,
            terminal: true,
            api: "close_mutex",
            first_param_base_type: "IntMutex",
            required_effect: "concurrency",
        }),
        "read_lock_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntRwLock,
            terminal: false,
            api: "read_lock_int",
            first_param_base_type: "IntRwLock",
            required_effect: "concurrency",
        }),
        "write_lock_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntRwLock,
            terminal: false,
            api: "write_lock_int",
            first_param_base_type: "IntRwLock",
            required_effect: "concurrency",
        }),
        "write_unlock_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntRwLock,
            terminal: false,
            api: "write_unlock_int",
            first_param_base_type: "IntRwLock",
            required_effect: "concurrency",
        }),
        "close_rwlock" => Some(ResourceProtocolOp {
            kind: ResourceKind::IntRwLock,
            terminal: true,
            api: "close_rwlock",
            first_param_base_type: "IntRwLock",
            required_effect: "concurrency",
        }),
        "join_task" => Some(ResourceProtocolOp {
            kind: ResourceKind::Task,
            terminal: true,
            api: "join_task",
            first_param_base_type: "Task",
            required_effect: "concurrency",
        }),
        "cancel_task" => Some(ResourceProtocolOp {
            kind: ResourceKind::Task,
            terminal: true,
            api: "cancel_task",
            first_param_base_type: "Task",
            required_effect: "concurrency",
        }),
        "file_read_line" => Some(ResourceProtocolOp {
            kind: ResourceKind::FileHandle,
            terminal: false,
            api: "file_read_line",
            first_param_base_type: "FileHandle",
            required_effect: "fs",
        }),
        "file_write_str" => Some(ResourceProtocolOp {
            kind: ResourceKind::FileHandle,
            terminal: false,
            api: "file_write_str",
            first_param_base_type: "FileHandle",
            required_effect: "fs",
        }),
        "file_close" => Some(ResourceProtocolOp {
            kind: ResourceKind::FileHandle,
            terminal: true,
            api: "file_close",
            first_param_base_type: "FileHandle",
            required_effect: "fs",
        }),
        "tcp_send" => Some(ResourceProtocolOp {
            kind: ResourceKind::TcpHandle,
            terminal: false,
            api: "tcp_send",
            first_param_base_type: "Int",
            required_effect: "net",
        }),
        "tcp_recv" => Some(ResourceProtocolOp {
            kind: ResourceKind::TcpHandle,
            terminal: false,
            api: "tcp_recv",
            first_param_base_type: "Int",
            required_effect: "net",
        }),
        "tcp_close" => Some(ResourceProtocolOp {
            kind: ResourceKind::TcpHandle,
            terminal: true,
            api: "tcp_close",
            first_param_base_type: "Int",
            required_effect: "net",
        }),
        "tls_send" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: false,
            api: "tls_send",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "tls_send_bytes" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: false,
            api: "tls_send_bytes",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "tls_recv" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: false,
            api: "tls_recv",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "tls_recv_bytes" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: false,
            api: "tls_recv_bytes",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "tls_version" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: false,
            api: "tls_version",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "tls_peer_subject" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: false,
            api: "tls_peer_subject",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "tls_peer_issuer" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: false,
            api: "tls_peer_issuer",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "tls_peer_fingerprint_sha256" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: false,
            api: "tls_peer_fingerprint_sha256",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "tls_peer_san_entries" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: false,
            api: "tls_peer_san_entries",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "tls_peer_cn" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: false,
            api: "tls_peer_cn",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "tls_close" => Some(ResourceProtocolOp {
            kind: ResourceKind::TlsStream,
            terminal: true,
            api: "tls_close",
            first_param_base_type: "TlsStream",
            required_effect: "net",
        }),
        "udp_send_to" => Some(ResourceProtocolOp {
            kind: ResourceKind::UdpHandle,
            terminal: false,
            api: "udp_send_to",
            first_param_base_type: "Int",
            required_effect: "net",
        }),
        "udp_recv_from" => Some(ResourceProtocolOp {
            kind: ResourceKind::UdpHandle,
            terminal: false,
            api: "udp_recv_from",
            first_param_base_type: "Int",
            required_effect: "net",
        }),
        "udp_close" => Some(ResourceProtocolOp {
            kind: ResourceKind::UdpHandle,
            terminal: true,
            api: "udp_close",
            first_param_base_type: "Int",
            required_effect: "net",
        }),
        "async_wait_int" => Some(ResourceProtocolOp {
            kind: ResourceKind::AsyncIntOp,
            terminal: true,
            api: "async_wait_int",
            first_param_base_type: "AsyncIntOp",
            required_effect: "net",
        }),
        "async_wait_string" => Some(ResourceProtocolOp {
            kind: ResourceKind::AsyncStringOp,
            terminal: true,
            api: "async_wait_string",
            first_param_base_type: "AsyncStringOp",
            required_effect: "net",
        }),
        "is_running" => Some(ResourceProtocolOp {
            kind: ResourceKind::ProcessHandle,
            terminal: false,
            api: "is_running",
            first_param_base_type: "Int",
            required_effect: "proc",
        }),
        "wait" => Some(ResourceProtocolOp {
            kind: ResourceKind::ProcessHandle,
            terminal: true,
            api: "wait",
            first_param_base_type: "Int",
            required_effect: "proc",
        }),
        "kill" => Some(ResourceProtocolOp {
            kind: ResourceKind::ProcessHandle,
            terminal: true,
            api: "kill",
            first_param_base_type: "Int",
            required_effect: "proc",
        }),
        _ => None,
    }
}

fn clear_resource_state_for_var(name: &str, state: &mut ResourceStateMap) {
    state.retain(|(var, _), _| var != name);
}

fn extract_tuple_field_type(ty: &str, index: usize) -> Option<String> {
    let trimmed = ty.trim();
    if !(trimmed.starts_with('(') && trimmed.ends_with(')')) {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    split_top_level(inner).get(index).cloned()
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
            if let Some(bound_dyn_trait) = parse_dyn_trait_name(&bound) {
                if let Some(found_dyn_trait) = parse_dyn_trait_name(found) {
                    let bound_name = bound_dyn_trait
                        .rsplit('.')
                        .next()
                        .unwrap_or(&bound_dyn_trait);
                    let found_name = found_dyn_trait
                        .rsplit('.')
                        .next()
                        .unwrap_or(&found_dyn_trait);
                    return bound_name == found_name;
                }
                return true;
            }
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

fn type_uses_generic_param(ty: &str, generic: &str) -> bool {
    if ty == generic {
        return true;
    }
    extract_generic_args(ty)
        .map(|args| {
            args.iter()
                .any(|arg| type_uses_generic_param(arg.as_str(), generic))
        })
        .unwrap_or(false)
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

fn await_bridge_submit_type(ty: &str) -> Option<String> {
    if base_type_name(ty) != "Result" {
        return None;
    }
    let args = extract_generic_args(ty)?;
    if args.len() != 2 {
        return None;
    }
    let ok = args[0].trim();
    let err = args[1].trim();
    match (ok, err) {
        ("AsyncIntOp", "NetError") => Some("Result[Int, NetError]".to_string()),
        ("AsyncStringOp", "NetError") => Some("Result[Bytes, NetError]".to_string()),
        ("AsyncIntOp", "TlsError") => Some("Result[Int, TlsError]".to_string()),
        ("AsyncStringOp", "TlsError") => Some("Result[Bytes, TlsError]".to_string()),
        _ => None,
    }
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

fn parse_integer_kind(ty: &str) -> Option<IntegerKind> {
    match base_type_name(ty) {
        "Int" => Some(IntegerKind::Int),
        "ISize" => Some(IntegerKind::ISize),
        "USize" | "UInt" => Some(IntegerKind::USize),
        "Int8" => Some(IntegerKind::Fixed {
            signed: true,
            bits: 8,
        }),
        "Int16" => Some(IntegerKind::Fixed {
            signed: true,
            bits: 16,
        }),
        "Int32" => Some(IntegerKind::Fixed {
            signed: true,
            bits: 32,
        }),
        "Int64" => Some(IntegerKind::Fixed {
            signed: true,
            bits: 64,
        }),
        "Int128" => Some(IntegerKind::Fixed {
            signed: true,
            bits: 128,
        }),
        "UInt8" => Some(IntegerKind::Fixed {
            signed: false,
            bits: 8,
        }),
        "UInt16" => Some(IntegerKind::Fixed {
            signed: false,
            bits: 16,
        }),
        "UInt32" => Some(IntegerKind::Fixed {
            signed: false,
            bits: 32,
        }),
        "UInt64" => Some(IntegerKind::Fixed {
            signed: false,
            bits: 64,
        }),
        "UInt128" => Some(IntegerKind::Fixed {
            signed: false,
            bits: 128,
        }),
        _ => None,
    }
}

fn integer_kind_from_literal_kind(kind: ir::IntLiteralKind) -> IntegerKind {
    match (kind.signedness, kind.width) {
        (ir::IntLiteralSignedness::Signed, ir::IntLiteralWidth::W8) => IntegerKind::Fixed {
            signed: true,
            bits: 8,
        },
        (ir::IntLiteralSignedness::Signed, ir::IntLiteralWidth::W16) => IntegerKind::Fixed {
            signed: true,
            bits: 16,
        },
        (ir::IntLiteralSignedness::Signed, ir::IntLiteralWidth::W32) => IntegerKind::Fixed {
            signed: true,
            bits: 32,
        },
        (ir::IntLiteralSignedness::Signed, ir::IntLiteralWidth::W64) => IntegerKind::Fixed {
            signed: true,
            bits: 64,
        },
        (ir::IntLiteralSignedness::Signed, ir::IntLiteralWidth::W128) => IntegerKind::Fixed {
            signed: true,
            bits: 128,
        },
        (ir::IntLiteralSignedness::Unsigned, ir::IntLiteralWidth::W8) => IntegerKind::Fixed {
            signed: false,
            bits: 8,
        },
        (ir::IntLiteralSignedness::Unsigned, ir::IntLiteralWidth::W16) => IntegerKind::Fixed {
            signed: false,
            bits: 16,
        },
        (ir::IntLiteralSignedness::Unsigned, ir::IntLiteralWidth::W32) => IntegerKind::Fixed {
            signed: false,
            bits: 32,
        },
        (ir::IntLiteralSignedness::Unsigned, ir::IntLiteralWidth::W64) => IntegerKind::Fixed {
            signed: false,
            bits: 64,
        },
        (ir::IntLiteralSignedness::Unsigned, ir::IntLiteralWidth::W128) => IntegerKind::Fixed {
            signed: false,
            bits: 128,
        },
    }
}

fn integer_kind_type_name(kind: IntegerKind) -> &'static str {
    match kind {
        IntegerKind::Int => "Int",
        IntegerKind::ISize => "ISize",
        IntegerKind::USize => "USize",
        IntegerKind::Fixed {
            signed: true,
            bits: 8,
        } => "Int8",
        IntegerKind::Fixed {
            signed: true,
            bits: 16,
        } => "Int16",
        IntegerKind::Fixed {
            signed: true,
            bits: 32,
        } => "Int32",
        IntegerKind::Fixed {
            signed: true,
            bits: 64,
        } => "Int64",
        IntegerKind::Fixed {
            signed: true,
            bits: 128,
        } => "Int128",
        IntegerKind::Fixed {
            signed: false,
            bits: 8,
        } => "UInt8",
        IntegerKind::Fixed {
            signed: false,
            bits: 16,
        } => "UInt16",
        IntegerKind::Fixed {
            signed: false,
            bits: 32,
        } => "UInt32",
        IntegerKind::Fixed {
            signed: false,
            bits: 64,
        } => "UInt64",
        IntegerKind::Fixed {
            signed: false,
            bits: 128,
        } => "UInt128",
        IntegerKind::Fixed { .. } => "<?>",
    }
}

fn integer_unsigned_max(bits: u8) -> u128 {
    if bits == 128 {
        u128::MAX
    } else {
        (1_u128 << bits) - 1
    }
}

fn integer_signed_max(bits: u8) -> u128 {
    (1_u128 << (bits - 1)) - 1
}

fn integer_signed_min_magnitude(bits: u8) -> u128 {
    1_u128 << (bits - 1)
}

fn integer_kind_range_text(kind: IntegerKind) -> (String, String) {
    match kind {
        IntegerKind::Int => (i64::MIN.to_string(), i64::MAX.to_string()),
        IntegerKind::ISize => (i64::MIN.to_string(), i64::MAX.to_string()),
        IntegerKind::USize => ("0".to_string(), u64::MAX.to_string()),
        IntegerKind::Fixed { signed: true, bits } => (
            format!("-{}", integer_signed_min_magnitude(bits)),
            integer_signed_max(bits).to_string(),
        ),
        IntegerKind::Fixed {
            signed: false,
            bits,
        } => ("0".to_string(), integer_unsigned_max(bits).to_string()),
    }
}

fn integer_literal_value_text(value: IntegerLiteralValue) -> String {
    match value {
        IntegerLiteralValue::NonNegative(v) => v.to_string(),
        IntegerLiteralValue::NegativeMagnitude(v) => format!("-{v}"),
    }
}

fn integer_literal_fits_kind(value: IntegerLiteralValue, kind: IntegerKind) -> bool {
    match kind {
        IntegerKind::Int => match value {
            IntegerLiteralValue::NonNegative(v) => v <= i64::MAX as u128,
            IntegerLiteralValue::NegativeMagnitude(v) => v <= (i64::MAX as u128) + 1,
        },
        IntegerKind::ISize => match value {
            IntegerLiteralValue::NonNegative(v) => v <= i64::MAX as u128,
            IntegerLiteralValue::NegativeMagnitude(v) => v <= (i64::MAX as u128) + 1,
        },
        IntegerKind::USize => match value {
            IntegerLiteralValue::NonNegative(v) => v <= u64::MAX as u128,
            IntegerLiteralValue::NegativeMagnitude(_) => false,
        },
        IntegerKind::Fixed { signed: true, bits } => match value {
            IntegerLiteralValue::NonNegative(v) => v <= integer_signed_max(bits),
            IntegerLiteralValue::NegativeMagnitude(v) => v <= integer_signed_min_magnitude(bits),
        },
        IntegerKind::Fixed {
            signed: false,
            bits,
        } => match value {
            IntegerLiteralValue::NonNegative(v) => v <= integer_unsigned_max(bits),
            IntegerLiteralValue::NegativeMagnitude(_) => false,
        },
    }
}

fn integer_kind_props(kind: IntegerKind) -> (bool, u8) {
    match kind {
        IntegerKind::Int => (true, 64),
        IntegerKind::ISize => (true, 64),
        IntegerKind::USize => (false, 64),
        IntegerKind::Fixed { signed, bits } => (signed, bits),
    }
}

fn integer_conversion_is_lossless(expected: IntegerKind, found: IntegerKind) -> bool {
    let (expected_signed, expected_bits) = integer_kind_props(expected);
    let (found_signed, found_bits) = integer_kind_props(found);

    match (expected_signed, found_signed) {
        (true, true) => found_bits <= expected_bits,
        (true, false) => expected_bits > found_bits,
        (false, false) => found_bits <= expected_bits,
        (false, true) => false,
    }
}

fn integer_kinds_match_exact(left: IntegerKind, right: IntegerKind) -> bool {
    left == right
}

fn parse_float_kind(ty: &str) -> Option<FloatKind> {
    match base_type_name(ty) {
        "Float32" => Some(FloatKind::F32),
        "Float64" | "Float" => Some(FloatKind::F64),
        _ => None,
    }
}

fn float_kind_type_name(kind: FloatKind) -> &'static str {
    match kind {
        FloatKind::F32 => "Float32",
        FloatKind::F64 => "Float64",
    }
}

fn canonical_builtin_type_name(base: &str) -> &str {
    match base {
        "Float" => "Float64",
        _ => base,
    }
}

fn base_type_name(ty: &str) -> &str {
    ty.split('[').next().unwrap_or(ty)
}

fn method_name_key(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

fn parse_dyn_trait_name(ty: &str) -> Option<String> {
    let trimmed = ty.trim();
    let rest = trimmed.strip_prefix("dyn ")?;
    let trait_name = rest.trim();
    if trait_name.is_empty() {
        None
    } else {
        Some(trait_name.to_string())
    }
}

fn type_uses_self(ty: &str) -> bool {
    if ty == "Self" {
        return true;
    }
    extract_generic_args(ty)
        .map(|args| args.iter().any(|arg| type_uses_self(arg)))
        .unwrap_or(false)
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
    let base = canonical_builtin_type_name(base_type_name(ty));
    (matches!(base, "Int" | "Bool" | "Float32" | "Float64" | "Char")
        || FIXED_WIDTH_INTEGER_PRIMITIVES
            .iter()
            .any(|name| name == &base))
        && extract_generic_args(ty).is_none()
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

    use super::{await_bridge_submit_type, check, extract_generic_args, merge_types, split_top_level};

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
    fn string_and_struct_patterns_typecheck() {
        let src = r#"
struct User {
    name: String,
    age: Int,
}

fn f(method: String, user: User) -> Int {
    let status = match method {
        "GET" => 1,
        "POST" => 2,
        _ => 0,
    };
    let age = match user {
        User { age: years, .. } => years,
    };
    status + age
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
                .any(|d| matches!(d.severity, Severity::Error)),
            "typecheck diagnostics={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn result_match_reports_missing_variant_with_fix() {
        let src = r#"
fn f(x: Result[Int, Int]) -> Int {
    match x {
        Ok(v) => v,
    }
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        let diag = out
            .diagnostics
            .iter()
            .find(|d| d.code == "E1248")
            .expect("missing non-exhaustive Result diagnostic");
        assert!(
            diag.message.contains("missing variant `Err`"),
            "message={}",
            diag.message
        );
        let replacement = diag
            .suggested_fixes
            .first()
            .and_then(|fix| fix.replacement.as_ref())
            .cloned()
            .unwrap_or_default();
        assert!(
            replacement.contains("Err(_) => todo(),"),
            "replacement={replacement}"
        );
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
            diag.message.contains("top -> middle"),
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
    fn resource_protocol_accepts_valid_generic_channel_sequence() {
        let src = r#"
enum ChannelError { Closed }
enum ConcurrencyError { Closed }
struct Sender[T] { handle: Int }
struct Receiver[T] { handle: Int }
fn send[T](tx: Sender[T], value: T) -> Result[Bool, ChannelError] effects { concurrency } { Ok(true) }
fn recv[T](rx: Receiver[T]) -> Result[T, ChannelError] effects { concurrency } { Err(Closed()) }
fn close_sender[T](tx: Sender[T]) -> Result[Bool, ConcurrencyError] effects { concurrency } { Ok(true) }
fn close_receiver[T](rx: Receiver[T]) -> Result[Bool, ConcurrencyError] effects { concurrency } { Ok(true) }

fn main() -> Int effects { concurrency } {
    let tx: Sender[String] = Sender { handle: 1 };
    let rx: Receiver[String] = Receiver { handle: 2 };
    let _sent = send(tx, "hello");
    let _recv = recv(rx);
    let _close_tx = close_sender(tx);
    let _close_rx = close_receiver(rx);
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
    fn resource_protocol_reports_generic_terminal_reuse_matrix() {
        let src = r#"
enum ChannelError { Closed, Timeout }
enum ConcurrencyError { Closed }
struct Sender[T] { handle: Int }
struct Receiver[T] { handle: Int }
fn send[T](tx: Sender[T], value: T) -> Result[Bool, ChannelError] effects { concurrency } { Ok(true) }
fn recv_timeout[T](rx: Receiver[T], timeout_ms: Int) -> Result[T, ChannelError] effects { concurrency } { Err(Timeout()) }
fn close_sender[T](tx: Sender[T]) -> Result[Bool, ConcurrencyError] effects { concurrency } { Ok(true) }
fn close_receiver[T](rx: Receiver[T]) -> Result[Bool, ConcurrencyError] effects { concurrency } { Ok(true) }

fn main() -> Int effects { concurrency } {
    let tx: Sender[String] = Sender { handle: 1 };
    let rx: Receiver[String] = Receiver { handle: 2 };
    let _close_tx = close_sender(tx);
    let _send_again = send(tx, "x");
    let _close_tx_again = close_sender(tx);
    let _close_rx = close_receiver(rx);
    let _recv_again = recv_timeout(rx, 10);
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={:#?}", d1);
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={:#?}", d2);
        let out = check(&ir, &res, "test.aic");
        let e2006 = out
            .diagnostics
            .iter()
            .filter(|d| d.code == "E2006")
            .collect::<Vec<_>>();
        assert_eq!(e2006.len(), 3, "diagnostics={:#?}", out.diagnostics);
        assert!(
            e2006.iter().any(|diag| {
                diag.message.contains("send")
                    && diag.message.contains("close_sender")
                    && diag.message.contains("Sender[String]")
            }),
            "diagnostics={:#?}",
            out.diagnostics
        );
        assert!(
            e2006.iter().any(|diag| {
                diag.message.contains("close_sender")
                    && diag.message.contains("Sender[String]")
                    && diag.message.contains("terminal 'close_sender'")
            }),
            "diagnostics={:#?}",
            out.diagnostics
        );
        assert!(
            e2006.iter().any(|diag| {
                diag.message.contains("recv_timeout")
                    && diag.message.contains("close_receiver")
                    && diag.message.contains("Receiver[String]")
            }),
            "diagnostics={:#?}",
            out.diagnostics
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
    fn reports_missing_capability_with_transitive_path_and_fix() {
        let src = r#"
import std.io;
fn leaf() -> () effects { io } capabilities { io } {
    print_int(1)
}
fn middle() -> () effects { io } {
    leaf()
}
fn top() -> () effects { io } {
    middle()
}
"#;
        let file = std::env::temp_dir().join(format!(
            "aic_capability_fix_{}_{}.aic",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        ));
        std::fs::write(&file, src).expect("write temp source");
        let file = file.to_string_lossy().to_string();

        let (program, d1) = parse(src, &file);
        assert!(d1.is_empty(), "parse diagnostics={:#?}", d1);
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, &file);
        assert!(d2.is_empty(), "resolve diagnostics={:#?}", d2);
        let out = check(&ir, &res, &file);
        let _ = std::fs::remove_file(&file);
        let diag = out
            .diagnostics
            .iter()
            .find(|d| d.code == "E2009" && d.message.contains("top"))
            .expect("missing E2009 capability diagnostic");
        assert!(
            diag.message.contains("top -> middle"),
            "message={}",
            diag.message
        );
        assert!(
            diag.suggested_fixes.iter().any(|fix| {
                fix.replacement
                    .as_deref()
                    .unwrap_or_default()
                    .contains("capabilities { io }")
            }),
            "fixes={:#?}",
            diag.suggested_fixes
        );
    }

    #[test]
    fn resource_protocol_accepts_valid_fs_sequence() {
        let src = r#"
enum FsError { Closed }
struct FileHandle { handle: Int }
fn file_read_line(file: FileHandle) -> Result[Int, FsError] effects { fs } capabilities { fs } { Ok(0) }
fn file_close(file: FileHandle) -> Result[Bool, FsError] effects { fs } capabilities { fs } { Ok(true) }

fn main() -> Int effects { fs } capabilities { fs } {
    let file = FileHandle { handle: 1 };
    let _line = file_read_line(file);
    let _closed = file_close(file);
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
    fn resource_protocol_reports_fs_use_after_close() {
        let src = r#"
enum FsError { Closed }
struct FileHandle { handle: Int }
fn file_read_line(file: FileHandle) -> Result[Int, FsError] effects { fs } capabilities { fs } { Ok(0) }
fn file_close(file: FileHandle) -> Result[Bool, FsError] effects { fs } capabilities { fs } { Ok(true) }

fn main() -> Int effects { fs } capabilities { fs } {
    let file = FileHandle { handle: 1 };
    let _closed = file_close(file);
    let _line = file_read_line(file);
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
            diag.message.contains("file_read_line") && diag.message.contains("closed FileHandle"),
            "message={}",
            diag.message
        );
    }

    #[test]
    fn resource_protocol_reports_net_and_proc_terminal_reuse() {
        let src = r#"
enum NetError { Closed }
enum TlsError { Closed }
enum ProcError { Done }
struct TlsStream { handle: Int }
fn tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[Int, NetError] effects { net } capabilities { net } { Ok(0) }
fn tcp_close(handle: Int) -> Result[Bool, NetError] effects { net } capabilities { net } { Ok(true) }
fn tls_send(stream: TlsStream, payload: String) -> Result[Int, TlsError] effects { net } capabilities { net } { Ok(0) }
fn tls_close(stream: TlsStream) -> Result[Bool, TlsError] effects { net } capabilities { net } { Ok(true) }
fn wait(handle: Int) -> Result[Int, ProcError] effects { proc } capabilities { proc } { Ok(0) }
fn is_running(handle: Int) -> Result[Bool, ProcError] effects { proc } capabilities { proc } { Ok(true) }

fn main() -> Int effects { net, proc } capabilities { net, proc } {
    let sock = 7;
    let _closed = tcp_close(sock);
    let _recv = tcp_recv(sock, 1, 1);
    let tls = TlsStream { handle: 5 };
    let _tls_closed = tls_close(tls);
    let _tls_send = tls_send(tls, "x");

    let child = 9;
    let _waited = wait(child);
    let _alive = is_running(child);
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={:#?}", d1);
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={:#?}", d2);
        let out = check(&ir, &res, "test.aic");
        let e2006 = out
            .diagnostics
            .iter()
            .filter(|d| d.code == "E2006")
            .collect::<Vec<_>>();
        assert!(
            e2006.iter().any(|diag| diag.message.contains("tcp_recv")),
            "diagnostics={:#?}",
            out.diagnostics
        );
        assert!(
            e2006.iter().any(|diag| diag.message.contains("is_running")),
            "diagnostics={:#?}",
            out.diagnostics
        );
        assert!(
            e2006.iter().any(|diag| diag.message.contains("tls_send")),
            "diagnostics={:#?}",
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
    fn await_bridge_submit_type_supports_net_and_tls_results() {
        assert_eq!(
            await_bridge_submit_type("Result[AsyncIntOp, NetError]"),
            Some("Result[Int, NetError]".to_string())
        );
        assert_eq!(
            await_bridge_submit_type("Result[AsyncStringOp, NetError]"),
            Some("Result[Bytes, NetError]".to_string())
        );
        assert_eq!(
            await_bridge_submit_type("Result[AsyncIntOp, TlsError]"),
            Some("Result[Int, TlsError]".to_string())
        );
        assert_eq!(
            await_bridge_submit_type("Result[AsyncStringOp, TlsError]"),
            Some("Result[Bytes, TlsError]".to_string())
        );
        assert_eq!(
            await_bridge_submit_type("Result[AsyncStringOp, FsError]"),
            None
        );
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
    fn infers_multi_generic_enum_variant_from_expected_type() {
        let src = r#"
enum SelectResult[A, B] {
    First(A),
    Second(B),
    Timeout,
}

fn pick_first[A, B](value: A) -> SelectResult[A, B] {
    First(value)
}

fn pick_second[A, B](value: B) -> SelectResult[A, B] {
    Second(value)
}

fn pick_timeout[A, B]() -> SelectResult[A, B] {
    Timeout()
}

fn main() -> Int {
    let first: SelectResult[Int, String] = pick_first(7);
    let second: SelectResult[Int, String] = pick_second("quit");
    let timeout: SelectResult[Int, String] = pick_timeout();
    let first_ok = match first {
        First(v) => if v == 7 { 1 } else { 0 },
        Second(_) => 0,
        Timeout => 0,
    };
    let second_ok = match second {
        First(_) => 0,
        Second(v) => if v == "quit" { 1 } else { 0 },
        Timeout => 0,
    };
    let timeout_ok = match timeout {
        Timeout => 1,
        _ => 0,
    };
    if first_ok + second_ok + timeout_ok == 3 { 1 } else { 0 }
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(
            !out.diagnostics.iter().any(|diag| diag.code == "E1212"),
            "typecheck diagnostics={:#?}",
            out.diagnostics
        );
        assert!(
            !out.diagnostics.iter().any(|diag| diag.code == "E2104"),
            "typecheck diagnostics={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn propagates_expected_type_into_if_and_match_branches() {
        let src = r#"
enum SelectResult[A, B] {
    First(A),
    Second(B),
    Timeout,
}

fn from_if[A, B](ok: Bool, a: A, b: B) -> SelectResult[A, B] {
    if ok { First(a) } else { Second(b) }
}

fn from_match[A, B](code: Int, a: A, b: B) -> SelectResult[A, B] {
    match code {
        0 => First(a),
        1 => Second(b),
        _ => Timeout(),
    }
}

fn main() -> Int {
    let a = from_if(true, 7, "quit");
    let b = from_match(2, 7, "quit");
    let a_ok = match a {
        First(v) => if v == 7 { 1 } else { 0 },
        _ => 0,
    };
    let b_ok = match b {
        Timeout => 1,
        _ => 0,
    };
    if a_ok + b_ok == 2 { 1 } else { 0 }
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(
            !out.diagnostics.iter().any(|diag| diag.code == "E1212"),
            "typecheck diagnostics={:#?}",
            out.diagnostics
        );
        assert!(
            !out.diagnostics.iter().any(|diag| diag.code == "E2104"),
            "typecheck diagnostics={:#?}",
            out.diagnostics
        );
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

    #[test]
    fn struct_defaults_allow_omitted_fields_and_auto_default_call() {
        let src = r#"
struct Config {
    port: Int = 40 + 2,
    enabled: Bool = true,
}

fn main() -> Int {
    let c = Config { enabled: false };
    let d = Config::default();
    if c.port == 42 && d.enabled { 1 } else { 0 }
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
            "typecheck diagnostics={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn struct_literal_still_requires_non_default_fields() {
        let src = r#"
struct Config {
    port: Int = 1,
    retries: Int,
}

fn main() -> Int {
    let c = Config { };
    c.retries
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(out.diagnostics.iter().any(|diag| diag.code == "E1227"
            && diag.message.contains("missing field")
            && diag.message.contains("Config.retries")));
    }

    #[test]
    fn struct_default_call_requires_all_fields_to_have_defaults() {
        let src = r#"
struct Config {
    port: Int = 1,
    retries: Int,
}

fn main() -> Int {
    let c = Config::default();
    c.port
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(out.diagnostics.iter().any(|diag| {
            diag.code == "E1218"
                && diag.message.contains("auto-generated")
                && diag.message.contains("Config::default")
        }));
    }

    #[test]
    fn struct_default_expression_must_be_compile_time_evaluable() {
        let src = r#"
fn runtime() -> Int { 1 }

struct Bad {
    value: Int = runtime(),
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(out.diagnostics.iter().any(|diag| {
            diag.code == "E1287"
                && diag.message.contains("default value for field")
                && diag.message.contains("Bad.value")
                && diag.message.contains("cannot call functions")
        }));
    }

    #[test]
    fn struct_default_expression_type_must_match_field() {
        let src = r#"
struct Bad {
    value: Bool = 1,
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(out.diagnostics.iter().any(|diag| {
            diag.code == "E1226"
                && diag.message.contains("default value for field")
                && diag.message.contains("Bad.value")
                && diag.message.contains("expects")
                && diag.message.contains("Bool")
                && diag.message.contains("found")
                && diag.message.contains("Int")
        }));
    }

    #[test]
    fn bitwise_and_shift_int_operands_typecheck() {
        let src = r#"
fn main() -> Int {
    let a = 0xFF & 0x0F;
    let b = a | 0xF0;
    let c = b ^ 0xAA;
    let d = c << 2;
    let e = d >> 1;
    let f = e >>> 1;
    ~f
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
    }

    #[test]
    fn bitwise_bool_operands_report_helpful_diagnostic() {
        let src = r#"
fn main(x: Bool, y: Bool) -> Int {
    let v = x & y;
    if v { 1 } else { 0 }
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        let diag = out
            .diagnostics
            .iter()
            .find(|d| d.code == "E1230" && d.message.contains("'&'"))
            .expect("missing bitwise type diagnostic");
        assert!(
            diag.help
                .iter()
                .any(|hint| hint.contains("&&") || hint.contains("||")),
            "help={:?}, diag={diag:#?}",
            diag.help
        );
    }

    #[test]
    fn spawn_requires_send_bound_for_payload_type() {
        let src = r#"
trait Send[T];

struct Task[T] {
    handle: Int,
}

struct FileHandle {
    handle: Int,
}

struct Payload {
    file: FileHandle,
}

fn spawn[T: Send](f: Fn() -> T) -> Task[T] {
    let _unused = f;
    Task { handle: 0 }
}

fn main() -> Int {
    let payload = Payload { file: FileHandle { handle: 7 } };
    let _task: Task[Payload] = spawn(|| -> Payload { payload });
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        let diag = out
            .diagnostics
            .iter()
            .find(|d| d.code == "E1258" && d.message.contains("Send"))
            .expect("missing Send-bound diagnostic");
        assert!(
            diag.help.iter().any(|hint| {
                hint.contains("Payload.file")
                    || hint.contains("runtime handle")
                    || hint.contains("not Send")
            }),
            "help={:?}, diag={diag:#?}",
            diag.help
        );
    }

    #[test]
    fn channel_send_requires_send_bound_for_payload_type() {
        let src = r#"
trait Send[T];

struct Sender[T] {
    handle: Int,
}

struct FileHandle {
    handle: Int,
}

fn send[T: Send](tx: Sender[T], value: T) -> Int {
    let _tx = tx;
    let _value = value;
    1
}

fn main() -> Int {
    let tx: Sender[FileHandle] = Sender { handle: 1 };
    let file = FileHandle { handle: 2 };
    let _sent = send(tx, file);
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == "E1258" && d.message.contains("Send")),
            "diags={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn fixed_width_integer_ops_require_matching_signedness_and_width() {
        let src = r#"
fn main(a: Int8, b: UInt16) -> Int {
    let _bad_add = a + b;
    let _bad_cmp = a < b;
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(
            out.diagnostics.iter().any(|d| {
                d.code == "E1230"
                    && d.message.contains("matching integer signedness/width")
                    && d.message.contains("Int8")
                    && d.message.contains("UInt16")
            }),
            "diags={:#?}",
            out.diagnostics
        );
        assert!(
            out.diagnostics.iter().any(|d| {
                d.code == "E1232"
                    && d.message.contains("fixed-width integers")
                    && d.message.contains("Int8")
                    && d.message.contains("UInt16")
            }),
            "diags={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn fixed_width_assignments_allow_only_lossless_conversions() {
        let src = r#"
fn main(a: Int16, b: UInt16) -> Int {
    let _widen_ok: Int32 = a;
    let _bad_narrow: Int8 = a;
    let _bad_sign: UInt16 = a;
    let _bad_unsigned_to_signed: Int8 = b;
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        let mismatches = out.diagnostics.iter().filter(|d| d.code == "E1204").count();
        assert!(mismatches >= 3, "diags={:#?}", out.diagnostics);
    }

    #[test]
    fn fixed_width_integer_literals_narrow_and_report_range_errors() {
        let src = r#"
fn main() -> Int {
    let ok_u8: UInt8 = 255;
    let bad_u8: UInt8 = 256;
    let ok_i8: Int8 = -128;
    let bad_i8: Int8 = -129;
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == "E1204" && d.message.contains("UInt8")),
            "diags={:#?}",
            out.diagnostics
        );
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == "E1204" && d.message.contains("Int8")),
            "diags={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn fixed_width_integer_patterns_report_out_of_range_literals() {
        let src = r#"
fn main(x: UInt8, y: Int8) -> Int {
    let a = match x {
        255u8 => 1,
        256 => 2,
        _ => 0,
    };
    let b = match y {
        127 => 1,
        128 => 2,
        _ => 0,
    };
    a + b
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == "E1234" && d.message.contains("UInt8")),
            "diags={:#?}",
            out.diagnostics
        );
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == "E1234" && d.message.contains("Int8")),
            "diags={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn fixed_width_128_literals_validate_boundaries_and_failures() {
        let src = r#"
fn main() -> Int {
    let ok_i128_min: Int128 = -170141183460469231731687303715884105728i128;
    let ok_u128_max: UInt128 = 340282366920938463463374607431768211455u128;
    let bad_i128: Int128 = 170141183460469231731687303715884105728i128;
    let bad_u128_neg: UInt128 = -1u128;
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == "E1204" && d.message.contains("Int128")),
            "diags={:#?}",
            out.diagnostics
        );
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == "E1204" && d.message.contains("UInt128")),
            "diags={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn fixed_width_128_assignments_enforce_signedness_and_lossless_rules() {
        let src = r#"
fn main(a: Int64, b: UInt64, c: Int128, d: UInt128) -> Int {
    let _ok_signed_widen: Int128 = a;
    let _ok_unsigned_widen: UInt128 = b;
    let _bad_signed_to_unsigned: UInt128 = c;
    let _bad_unsigned_to_signed: Int128 = d;
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        let mismatches = out.diagnostics.iter().filter(|d| d.code == "E1204").count();
        assert!(mismatches >= 2, "diags={:#?}", out.diagnostics);
    }

    #[test]
    fn size_integer_assignments_enforce_lossless_policy_with_uint_alias() {
        let src = r#"
fn main(a: Int, b: ISize, c: USize, d: UInt32, e: UInt) -> Int {
    let _ok_int_to_isize: ISize = a;
    let _ok_isize_to_int: Int = b;
    let _ok_u32_to_usize: USize = d;
    let _ok_usize_to_uint: UInt = c;
    let _ok_uint_to_usize: USize = e;
    let _bad_usize_to_int: Int = c;
    let _bad_int_to_usize: USize = a;
    let _bad_isize_to_usize: USize = b;
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        let mismatches = out.diagnostics.iter().filter(|d| d.code == "E1204").count();
        assert!(mismatches >= 3, "diags={:#?}", out.diagnostics);
    }

    #[test]
    fn size_integer_ops_require_exact_kind_except_uint_usize_alias_match() {
        let src = r#"
fn main(a: ISize, b: Int, c: USize, d: UInt) -> Int {
    let _ok_alias_add = c + d;
    let _bad_arith = a + b;
    let _bad_cmp = c < b;
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(
            out.diagnostics.iter().any(|d| d.code == "E1230"
                && d.message.contains("ISize")
                && d.message.contains("Int")),
            "diags={:#?}",
            out.diagnostics
        );
        assert!(
            out.diagnostics.iter().any(|d| d.code == "E1232"
                && d.message.contains("USize")
                && d.message.contains("Int")),
            "diags={:#?}",
            out.diagnostics
        );
    }

    #[test]
    fn usize_and_uint_literals_follow_unsigned_range_rules() {
        let src = r#"
fn main() -> Int {
    let _ok_usize_from_u64: USize = 42u64;
    let _ok_uint_alias: UInt = 7;
    let _bad_negative_uint: UInt = -1;
    0
}
"#;
        let (program, d1) = parse(src, "test.aic");
        assert!(d1.is_empty(), "parse diagnostics={d1:#?}");
        let ir = build(&program.expect("program"));
        let (res, d2) = resolve(&ir, "test.aic");
        assert!(d2.is_empty(), "resolve diagnostics={d2:#?}");
        let out = check(&ir, &res, "test.aic");
        assert!(
            out.diagnostics.iter().any(|d| d.code == "E1204"
                && (d.message.contains("USize") || d.message.contains("UInt"))),
            "diags={:#?}",
            out.diagnostics
        );
    }
}
