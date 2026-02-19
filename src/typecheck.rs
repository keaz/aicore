use std::collections::{BTreeMap, BTreeSet};

use crate::ast::BinOp;
use crate::diagnostics::Diagnostic;
use crate::ir;
use crate::resolver::{EnumInfo, Resolution, StructInfo};

#[derive(Debug, Clone, Default)]
pub struct TypecheckOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub function_effect_usage: BTreeMap<String, BTreeSet<String>>,
}

pub fn check(program: &ir::Program, resolution: &Resolution, file: &str) -> TypecheckOutput {
    let mut checker = Checker::new(program, resolution, file);
    checker.run();
    checker.finish()
}

#[derive(Debug, Clone)]
struct FnSig {
    params: Vec<String>,
    ret: String,
    effects: BTreeSet<String>,
    generics: usize,
}

struct Checker<'a> {
    program: &'a ir::Program,
    resolution: &'a Resolution,
    file: &'a str,
    diagnostics: Vec<Diagnostic>,
    types: BTreeMap<ir::TypeId, String>,
    functions: BTreeMap<String, FnSig>,
    effect_usage: BTreeMap<String, BTreeSet<String>>,
    enforce_import_visibility: bool,
}

#[derive(Default)]
struct ExprContext {
    effects_used: BTreeSet<String>,
}

impl<'a> Checker<'a> {
    fn new(program: &'a ir::Program, resolution: &'a Resolution, file: &'a str) -> Self {
        let mut types = BTreeMap::new();
        for ty in &program.types {
            types.insert(ty.id, ty.repr.clone());
        }

        let mut functions = BTreeMap::new();

        for (name, info) in &resolution.functions {
            let mut params = Vec::new();
            for ty in &info.param_types {
                params.push(types.get(ty).cloned().unwrap_or_else(|| "<?>".to_string()));
            }
            functions.insert(
                name.clone(),
                FnSig {
                    params,
                    ret: types
                        .get(&info.ret_type)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string()),
                    effects: info.effects.clone(),
                    generics: program
                        .items
                        .iter()
                        .find_map(|item| match item {
                            ir::Item::Function(f) if f.name == *name => Some(f.generics.len()),
                            _ => None,
                        })
                        .unwrap_or(0),
                },
            );
        }

        // Minimal std signatures.
        functions.insert(
            "print_int".to_string(),
            FnSig {
                params: vec!["Int".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                generics: 0,
            },
        );
        functions.insert(
            "print_str".to_string(),
            FnSig {
                params: vec!["String".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                generics: 0,
            },
        );
        functions.insert(
            "len".to_string(),
            FnSig {
                params: vec!["String".to_string()],
                ret: "Int".to_string(),
                effects: BTreeSet::new(),
                generics: 0,
            },
        );
        functions.insert(
            "panic".to_string(),
            FnSig {
                params: vec!["String".to_string()],
                ret: "()".to_string(),
                effects: BTreeSet::from(["io".to_string()]),
                generics: 0,
            },
        );

        Self {
            program,
            resolution,
            file,
            diagnostics: Vec::new(),
            types,
            functions,
            effect_usage: BTreeMap::new(),
            enforce_import_visibility: false,
        }
    }

    fn run(&mut self) {
        for item in &self.program.items {
            match item {
                ir::Item::Function(func) => self.check_function(func),
                ir::Item::Struct(strukt) => self.check_struct_invariant(strukt),
                ir::Item::Enum(_) => {}
            }
        }
    }

    fn finish(self) -> TypecheckOutput {
        TypecheckOutput {
            diagnostics: self.diagnostics,
            function_effect_usage: self.effect_usage,
        }
    }

    fn check_function(&mut self, func: &ir::Function) {
        let previous_enforce = self.enforce_import_visibility;
        self.enforce_import_visibility = self.should_enforce_import_visibility(&func.name);

        let declared_effects: BTreeSet<String> = func.effects.iter().cloned().collect();
        let mut locals = BTreeMap::new();
        for param in &func.params {
            locals.insert(
                param.name.clone(),
                self.types
                    .get(&param.ty)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string()),
            );
        }

        let ret_type = self
            .types
            .get(&func.ret_type)
            .cloned()
            .unwrap_or_else(|| "<?>".to_string());

        if let Some(requires) = &func.requires {
            let mut contract_ctx = ExprContext::default();
            let ty = self.check_expr(
                requires,
                &mut locals.clone(),
                &declared_effects,
                &mut contract_ctx,
                true,
            );
            if ty != "Bool" {
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
            if ty != "Bool" {
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

        if body_ty != ret_type {
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
            self.diagnostics.push(
                Diagnostic::error(
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
                )),
            );
        }

        self.effect_usage
            .insert(func.name.clone(), body_ctx.effects_used);

        self.enforce_import_visibility = previous_enforce;
    }

    fn should_enforce_import_visibility(&self, function_name: &str) -> bool {
        let Some(entry_module) = self.resolution.entry_module.as_ref() else {
            return true;
        };
        let Some(modules) = self.resolution.function_modules.get(function_name) else {
            return true;
        };
        modules.len() == 1 && modules.contains(entry_module)
    }

    fn check_struct_invariant(&mut self, strukt: &ir::StructDef) {
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
            if ty != "Bool" {
                self.diagnostics.push(Diagnostic::error(
                    "E1203",
                    format!("invariant for struct '{}' must be Bool", strukt.name),
                    self.file,
                    inv.span,
                ));
            }
        }
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

        for stmt in &block.stmts {
            match stmt {
                ir::Stmt::Let {
                    name,
                    ty,
                    expr,
                    span,
                    ..
                } => {
                    let expr_ty =
                        self.check_expr(expr, &mut scope, allowed_effects, ctx, contract_mode);
                    if let Some(ann) = ty {
                        let ann_ty = self
                            .types
                            .get(ann)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string());
                        if ann_ty != expr_ty {
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
                        scope.insert(name.clone(), ann_ty);
                    } else {
                        if contains_unresolved_type(&expr_ty) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1204",
                                    format!(
                                        "cannot infer concrete type for let binding '{}' (inferred '{}')",
                                        name, expr_ty
                                    ),
                                    self.file,
                                    *span,
                                )
                                .with_help("add an explicit type annotation on the binding"),
                            );
                        }
                        scope.insert(name.clone(), expr_ty);
                    }
                }
                ir::Stmt::Expr { expr, .. } => {
                    self.check_expr(expr, &mut scope, allowed_effects, ctx, contract_mode);
                }
                ir::Stmt::Return { expr, span } => {
                    let ty = if let Some(expr) = expr {
                        self.check_expr(expr, &mut scope, allowed_effects, ctx, contract_mode)
                    } else {
                        "()".to_string()
                    };
                    if ty != ret_type {
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
                    if ty != "Bool" {
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

        if let Some(tail) = &block.tail {
            self.check_expr(tail, &mut scope, allowed_effects, ctx, contract_mode)
        } else {
            "()".to_string()
        }
    }

    fn check_expr(
        &mut self,
        expr: &ir::Expr,
        locals: &mut BTreeMap<String, String>,
        allowed_effects: &BTreeSet<String>,
        ctx: &mut ExprContext,
        contract_mode: bool,
    ) -> String {
        match &expr.kind {
            ir::ExprKind::Int(_) => "Int".to_string(),
            ir::ExprKind::Bool(_) => "Bool".to_string(),
            ir::ExprKind::String(_) => "String".to_string(),
            ir::ExprKind::Unit => "()".to_string(),
            ir::ExprKind::Var(name) => {
                if let Some(ty) = locals.get(name) {
                    return ty.clone();
                }
                if self.functions.contains_key(name) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1207",
                            format!("function '{}' cannot be used as a value", name),
                            self.file,
                            expr.span,
                        )
                        .with_help("call the function with parentheses"),
                    );
                    return "<?>".to_string();
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
                let Some(call_path) = self.extract_callee_path(callee) else {
                    self.diagnostics.push(Diagnostic::error(
                        "E1209",
                        "callee must be a function or constructor path",
                        self.file,
                        callee.span,
                    ));
                    return "<?>".to_string();
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
                    let err =
                        self.check_expr(&args[0], locals, allowed_effects, ctx, contract_mode);
                    return format!("Result[<?>, {}]", err);
                }

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
                                format!("ambiguous callable '{}' exported by multiple modules", name),
                                self.file,
                                callee.span,
                            )
                            .with_help("rename colliding functions or import a single module exporting that name"),
                        );
                        return "<?>".to_string();
                    }

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

                    name.clone()
                };

                if let Some(sig) = self.functions.get(&resolved_name).cloned() {
                    if sig.generics > 0 {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1212",
                                format!(
                                    "generic function '{}' requires explicit specialization in MVP",
                                    resolved_name
                                ),
                                self.file,
                                expr.span,
                            )
                            .with_help("for MVP, use non-generic functions at call sites"),
                        );
                    }

                    if resolved_name == "print_int"
                        || resolved_name == "print_str"
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
                    for (idx, arg) in args.iter().enumerate() {
                        let arg_ty =
                            self.check_expr(arg, locals, allowed_effects, ctx, contract_mode);
                        if let Some(expected) = sig.params.get(idx) {
                            if !type_compatible(expected, &arg_ty) {
                                self.diagnostics.push(Diagnostic::error(
                                    "E1214",
                                    format!(
                                        "argument {} to '{}' expected '{}', found '{}'",
                                        idx + 1,
                                        rendered_path,
                                        expected,
                                        arg_ty
                                    ),
                                    self.file,
                                    arg.span,
                                ));
                            }
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
                    return sig.ret;
                }

                if !qualified {
                    if let Some((enum_name, payload)) = self.find_variant(&name) {
                        if let Some(payload_ty) = payload {
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
                                if !type_compatible(&payload_ty, &arg_ty) {
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
                        return enum_name;
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
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let cond_ty = self.check_expr(cond, locals, allowed_effects, ctx, contract_mode);
                if cond_ty != "Bool" {
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
                if !type_compatible(&then_ty, &else_ty) {
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
                    merge_types(&then_ty, &else_ty)
                }
            }
            ir::ExprKind::Match {
                expr: scrutinee,
                arms,
            } => {
                let scrutinee_ty =
                    self.check_expr(scrutinee, locals, allowed_effects, ctx, contract_mode);
                let mut arm_types = Vec::new();
                let mut seen = BTreeSet::new();
                let mut wildcard_seen = false;

                for arm in arms {
                    let mut arm_scope = locals.clone();
                    self.check_pattern(
                        &arm.pattern,
                        &scrutinee_ty,
                        &mut arm_scope,
                        &mut seen,
                        &mut wildcard_seen,
                    );
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
                        if !type_compatible(&first, ty) {
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
                        .reduce(|a, b| merge_types(&a, &b))
                        .unwrap_or_else(|| "()".to_string())
                }
            }
            ir::ExprKind::Binary { op, lhs, rhs } => {
                let left_ty = self.check_expr(lhs, locals, allowed_effects, ctx, contract_mode);
                let right_ty = self.check_expr(rhs, locals, allowed_effects, ctx, contract_mode);
                self.check_binary(*op, &left_ty, &right_ty, expr.span)
            }
            ir::ExprKind::Unary { op, expr: inner } => {
                let ty = self.check_expr(inner, locals, allowed_effects, ctx, contract_mode);
                match op {
                    crate::ast::UnaryOp::Neg => {
                        if ty != "Int" {
                            self.diagnostics.push(Diagnostic::error(
                                "E1222",
                                "unary '-' expects Int",
                                self.file,
                                inner.span,
                            ));
                        }
                        "Int".to_string()
                    }
                    crate::ast::UnaryOp::Not => {
                        if ty != "Bool" {
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
            ir::ExprKind::StructInit { name, fields } => {
                let Some(info) = self.resolution.structs.get(name).cloned() else {
                    self.diagnostics.push(Diagnostic::error(
                        "E1224",
                        format!("unknown struct '{}'", name),
                        self.file,
                        expr.span,
                    ));
                    return "<?>".to_string();
                };

                for (field_name, value, span) in fields {
                    let Some(expected) = info.fields.get(field_name) else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1225",
                            format!("struct '{}' has no field '{}'", name, field_name),
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
                    if !type_compatible(&expected_ty, &found_ty) {
                        self.diagnostics.push(Diagnostic::error(
                            "E1226",
                            format!(
                                "field '{}.{}' expects '{}', found '{}'",
                                name, field_name, expected_ty, found_ty
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

                name.clone()
            }
            ir::ExprKind::FieldAccess { base, field } => {
                let base_ty = self.check_expr(base, locals, allowed_effects, ctx, contract_mode);
                if let Some(info) = self.find_struct(&base_ty) {
                    if let Some(field_ty_id) = info.fields.get(field) {
                        return self
                            .types
                            .get(field_ty_id)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string());
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

    fn check_binary(&mut self, op: BinOp, lhs: &str, rhs: &str, span: crate::span::Span) -> String {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                if lhs != "Int" || rhs != "Int" {
                    self.diagnostics.push(Diagnostic::error(
                        "E1230",
                        format!(
                            "arithmetic operators require Int operands, found '{}' and '{}'",
                            lhs, rhs
                        ),
                        self.file,
                        span,
                    ));
                }
                "Int".to_string()
            }
            BinOp::Eq | BinOp::Ne => {
                if !type_compatible(lhs, rhs) {
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
                if lhs != "Int" || rhs != "Int" {
                    self.diagnostics.push(Diagnostic::error(
                        "E1232",
                        "comparison operators require Int operands",
                        self.file,
                        span,
                    ));
                }
                "Bool".to_string()
            }
            BinOp::And | BinOp::Or => {
                if lhs != "Bool" || rhs != "Bool" {
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
        seen: &mut BTreeSet<String>,
        wildcard_seen: &mut bool,
    ) {
        match &pattern.kind {
            ir::PatternKind::Wildcard => {
                *wildcard_seen = true;
            }
            ir::PatternKind::Var(name) => {
                locals.insert(name.clone(), scrutinee_ty.to_string());
                *wildcard_seen = true;
            }
            ir::PatternKind::Int(_) => {
                if scrutinee_ty != "Int" {
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
                seen.insert("_int_literal".to_string());
            }
            ir::PatternKind::Bool(v) => {
                if scrutinee_ty != "Bool" {
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
                seen.insert(if *v { "true" } else { "false" }.to_string());
            }
            ir::PatternKind::Unit => {
                if scrutinee_ty != "()" {
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
                seen.insert("()".to_string());
            }
            ir::PatternKind::Variant { name, args } => {
                if scrutinee_ty.starts_with("Option[") {
                    match name.as_str() {
                        "None" => {
                            if !args.is_empty() {
                                self.diagnostics.push(Diagnostic::error(
                                    "E1237",
                                    "None pattern takes no payload",
                                    self.file,
                                    pattern.span,
                                ));
                            }
                            seen.insert("None".to_string());
                        }
                        "Some" => {
                            if args.len() != 1 {
                                self.diagnostics.push(Diagnostic::error(
                                    "E1238",
                                    "Some pattern takes one payload pattern",
                                    self.file,
                                    pattern.span,
                                ));
                            } else {
                                let inner = extract_generic_args(scrutinee_ty)
                                    .and_then(|mut v| {
                                        if v.len() == 1 {
                                            Some(v.remove(0))
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or_else(|| "<?>".to_string());
                                self.check_pattern(
                                    &args[0],
                                    &inner,
                                    locals,
                                    &mut BTreeSet::new(),
                                    &mut false,
                                );
                            }
                            seen.insert("Some".to_string());
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

                if scrutinee_ty.starts_with("Result[") {
                    match name.as_str() {
                        "Ok" | "Err" => {
                            if args.len() != 1 {
                                self.diagnostics.push(Diagnostic::error(
                                    "E1240",
                                    format!("{} pattern takes one payload pattern", name),
                                    self.file,
                                    pattern.span,
                                ));
                            } else {
                                let vars = extract_generic_args(scrutinee_ty).unwrap_or_default();
                                let payload_ty = if name == "Ok" {
                                    vars.get(0).cloned().unwrap_or_else(|| "<?>".to_string())
                                } else {
                                    vars.get(1).cloned().unwrap_or_else(|| "<?>".to_string())
                                };
                                self.check_pattern(
                                    &args[0],
                                    &payload_ty,
                                    locals,
                                    &mut BTreeSet::new(),
                                    &mut false,
                                );
                            }
                            seen.insert(name.clone());
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

                if let Some(enum_info) = self.find_enum(scrutinee_ty) {
                    if let Some(payload_ty_id) = enum_info.variants.get(name) {
                        if let Some(payload_ty_id) = payload_ty_id {
                            let payload = self
                                .types
                                .get(payload_ty_id)
                                .cloned()
                                .unwrap_or_else(|| "<?>".to_string());
                            if args.len() != 1 {
                                self.diagnostics.push(Diagnostic::error(
                                    "E1242",
                                    format!("variant '{}' takes one payload pattern", name),
                                    self.file,
                                    pattern.span,
                                ));
                            } else {
                                self.check_pattern(
                                    &args[0],
                                    &payload,
                                    locals,
                                    &mut BTreeSet::new(),
                                    &mut false,
                                );
                            }
                        } else if !args.is_empty() {
                            self.diagnostics.push(Diagnostic::error(
                                "E1243",
                                format!("variant '{}' takes no payload", name),
                                self.file,
                                pattern.span,
                            ));
                        }
                        seen.insert(name.clone());
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

    fn check_exhaustive(
        &mut self,
        span: crate::span::Span,
        scrutinee_ty: &str,
        seen: &BTreeSet<String>,
        wildcard_seen: bool,
    ) {
        if wildcard_seen {
            return;
        }

        if scrutinee_ty == "Bool" {
            if !(seen.contains("true") && seen.contains("false")) {
                self.diagnostics.push(
                    Diagnostic::error("E1246", "non-exhaustive bool match", self.file, span)
                        .with_help("add missing `true` or `false` arm, or `_` wildcard"),
                );
            }
            return;
        }

        if scrutinee_ty.starts_with("Option[") {
            if !(seen.contains("None") && seen.contains("Some")) {
                self.diagnostics.push(
                    Diagnostic::error("E1247", "non-exhaustive Option match", self.file, span)
                        .with_help("add both `None` and `Some(...)` arms, or `_` wildcard"),
                );
            }
            return;
        }

        if scrutinee_ty.starts_with("Result[") {
            if !(seen.contains("Ok") && seen.contains("Err")) {
                self.diagnostics.push(
                    Diagnostic::error("E1248", "non-exhaustive Result match", self.file, span)
                        .with_help("add both `Ok(...)` and `Err(...)` arms, or `_` wildcard"),
                );
            }
            return;
        }

        if let Some(enum_info) = self.find_enum(scrutinee_ty) {
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

    fn find_variant(&self, name: &str) -> Option<(String, Option<String>)> {
        for (enum_name, info) in &self.resolution.enums {
            if let Some(payload) = info.variants.get(name) {
                let payload_ty = payload.and_then(|id| self.types.get(&id).cloned());
                return Some((enum_name.clone(), payload_ty));
            }
        }
        None
    }

    fn find_enum(&self, ty: &str) -> Option<&EnumInfo> {
        let base = base_type_name(ty);
        self.resolution.enums.get(base)
    }

    fn find_struct(&self, ty: &str) -> Option<&StructInfo> {
        let base = base_type_name(ty);
        self.resolution.structs.get(base)
    }
}

fn type_compatible(expected: &str, found: &str) -> bool {
    expected == found
        || expected == "<?>"
        || found == "<?>"
        || (expected.starts_with("Option[")
            && found.starts_with("Option[")
            && (expected.contains("<?>") || found.contains("<?>")))
        || (expected.starts_with("Result[")
            && found.starts_with("Result[")
            && (expected.contains("<?>") || found.contains("<?>")))
}

fn contains_unresolved_type(ty: &str) -> bool {
    ty.contains("<?>")
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

    "<?>".to_string()
}

fn base_type_name(ty: &str) -> &str {
    ty.split('[').next().unwrap_or(ty)
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
}
