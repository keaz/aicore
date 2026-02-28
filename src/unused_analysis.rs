use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::ast::{self, decode_internal_const, decode_internal_type_alias};
use crate::diagnostics::{Diagnostic, SuggestedFix};
use crate::resolver::Resolution;
use crate::span::Span;

const ROOT_MODULE: &str = "<root>";
const UNUSED_IMPORT_CODE: &str = "E6004";
const UNUSED_FUNCTION_CODE: &str = "E6005";
const UNUSED_VARIABLE_CODE: &str = "E6006";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FunctionKey {
    module: String,
    name: String,
}

impl FunctionKey {
    fn display(&self) -> String {
        if self.module == ROOT_MODULE {
            self.name.clone()
        } else {
            format!("{}.{}", self.module, self.name)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FunctionDecl {
    span: Span,
    is_extern: bool,
    is_intrinsic: bool,
}

#[derive(Debug, Clone, Copy)]
struct ImportDeclInfo {
    span: Span,
}

pub fn analyze_unused_warnings(
    program: &ast::Program,
    resolution: &Resolution,
    item_modules: &[Option<Vec<String>>],
    file: &str,
    source: &str,
) -> Vec<Diagnostic> {
    Analyzer::new(program, resolution, item_modules, file, source).run()
}

struct Analyzer<'a> {
    program: &'a ast::Program,
    resolution: &'a Resolution,
    item_modules: &'a [Option<Vec<String>>],
    file: &'a str,
    source: &'a str,
    entry_module: String,
    imports: BTreeMap<String, Vec<ImportDeclInfo>>,
    imported_module_set: BTreeSet<String>,
    used_import_modules: BTreeSet<String>,
    function_decls: BTreeMap<FunctionKey, FunctionDecl>,
    function_edges: BTreeMap<FunctionKey, BTreeSet<FunctionKey>>,
    called_functions: BTreeSet<FunctionKey>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Analyzer<'a> {
    fn new(
        program: &'a ast::Program,
        resolution: &'a Resolution,
        item_modules: &'a [Option<Vec<String>>],
        file: &'a str,
        source: &'a str,
    ) -> Self {
        let entry_module = program
            .module
            .as_ref()
            .map(|module| module.path.join("."))
            .unwrap_or_else(|| ROOT_MODULE.to_string());

        let mut imports: BTreeMap<String, Vec<ImportDeclInfo>> = BTreeMap::new();
        let mut imported_module_set = BTreeSet::new();
        for import in &program.imports {
            let path = import.path.join(".");
            imported_module_set.insert(path.clone());
            imports
                .entry(path)
                .or_default()
                .push(ImportDeclInfo { span: import.span });
        }

        let mut function_decls = BTreeMap::new();
        for (index, item) in program.items.iter().enumerate() {
            let ast::Item::Function(func) = item else {
                continue;
            };
            if is_internal_function(func) {
                continue;
            }
            let module = module_for_item(program, item_modules, index);
            if module != entry_module {
                continue;
            }
            let key = FunctionKey {
                module,
                name: func.name.clone(),
            };
            function_decls.entry(key).or_insert(FunctionDecl {
                span: func.span,
                is_extern: func.is_extern,
                is_intrinsic: func.is_intrinsic,
            });
        }

        Self {
            program,
            resolution,
            item_modules,
            file,
            source,
            entry_module,
            imports,
            imported_module_set,
            used_import_modules: BTreeSet::new(),
            function_decls,
            function_edges: BTreeMap::new(),
            called_functions: BTreeSet::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<Diagnostic> {
        self.collect_usage();
        self.emit_unused_import_warnings();
        self.emit_unused_function_warnings();
        self.emit_unused_variable_warnings();
        self.diagnostics
    }

    fn collect_usage(&mut self) {
        for (index, item) in self.program.items.iter().enumerate() {
            let module_name = module_for_item(self.program, self.item_modules, index);
            if module_name != self.entry_module {
                continue;
            }
            match item {
                ast::Item::Function(func) => {
                    self.visit_function_signature(func);
                    let caller_key = if is_internal_function(func) {
                        None
                    } else {
                        Some(FunctionKey {
                            module: module_name.clone(),
                            name: func.name.clone(),
                        })
                    };
                    self.visit_function_expressions(func, &module_name, caller_key.as_ref());
                }
                ast::Item::Struct(strukt) => {
                    for generic in &strukt.generics {
                        self.visit_trait_bounds(&generic.bounds);
                    }
                    for field in &strukt.fields {
                        self.visit_type_expr(&field.ty);
                    }
                    if let Some(invariant) = &strukt.invariant {
                        self.visit_expr(invariant, &module_name, None);
                    }
                }
                ast::Item::Enum(enm) => {
                    for generic in &enm.generics {
                        self.visit_trait_bounds(&generic.bounds);
                    }
                    for variant in &enm.variants {
                        if let Some(payload) = &variant.payload {
                            self.visit_type_expr(payload);
                        }
                    }
                }
                ast::Item::Trait(trait_def) => {
                    for generic in &trait_def.generics {
                        self.visit_trait_bounds(&generic.bounds);
                    }
                    for method in &trait_def.methods {
                        self.visit_function_signature(method);
                        self.visit_function_expressions(method, &module_name, None);
                    }
                }
                ast::Item::Impl(impl_def) => {
                    if let Some(module_ref) = module_prefix(&impl_def.trait_name) {
                        self.record_module_reference(module_ref);
                    }
                    for arg in &impl_def.trait_args {
                        self.visit_type_expr(arg);
                    }
                    if let Some(target) = &impl_def.target {
                        self.visit_type_expr(target);
                    }
                    for method in &impl_def.methods {
                        self.visit_function_signature(method);
                        self.visit_function_expressions(method, &module_name, None);
                    }
                }
            }
        }
    }

    fn visit_function_signature(&mut self, func: &ast::Function) {
        for generic in &func.generics {
            self.visit_trait_bounds(&generic.bounds);
        }
        for param in &func.params {
            self.visit_type_expr(&param.ty);
        }
        self.visit_type_expr(&func.ret_type);
    }

    fn visit_function_expressions(
        &mut self,
        func: &ast::Function,
        module_name: &str,
        caller: Option<&FunctionKey>,
    ) {
        if let Some(requires) = &func.requires {
            self.visit_expr(requires, module_name, caller);
        }
        if let Some(ensures) = &func.ensures {
            self.visit_expr(ensures, module_name, caller);
        }
        self.visit_block(&func.body, module_name, caller);
    }

    fn visit_block(&mut self, block: &ast::Block, module_name: &str, caller: Option<&FunctionKey>) {
        for stmt in &block.stmts {
            match stmt {
                ast::Stmt::Let { ty, expr, .. } => {
                    if let Some(ty) = ty {
                        self.visit_type_expr(ty);
                    }
                    self.visit_expr(expr, module_name, caller);
                }
                ast::Stmt::Assign { expr, .. } => self.visit_expr(expr, module_name, caller),
                ast::Stmt::Expr { expr, .. } => self.visit_expr(expr, module_name, caller),
                ast::Stmt::Return { expr, .. } => {
                    if let Some(expr) = expr {
                        self.visit_expr(expr, module_name, caller);
                    }
                }
                ast::Stmt::Assert { expr, .. } => self.visit_expr(expr, module_name, caller),
            }
        }
        if let Some(tail) = &block.tail {
            self.visit_expr(tail, module_name, caller);
        }
    }

    fn visit_expr(&mut self, expr: &ast::Expr, module_name: &str, caller: Option<&FunctionKey>) {
        match &expr.kind {
            ast::ExprKind::Var(name) => {
                if let Some(module_ref) = module_prefix(name) {
                    self.record_module_reference(module_ref);
                }
            }
            ast::ExprKind::Call { callee, args, .. } => {
                if let Some(call_path) = call_path(callee) {
                    if call_path.len() >= 2 {
                        let qualifier = call_path[..call_path.len() - 1].join(".");
                        self.record_module_reference(&qualifier);
                    } else if let Some(module) =
                        self.resolve_unqualified_call_module(module_name, &call_path[0])
                    {
                        if self.imported_module_set.contains(&module) {
                            self.used_import_modules.insert(module);
                        }
                    }
                    if let Some(target) = self.resolve_call_target(module_name, &call_path) {
                        if let Some(caller_key) = caller {
                            self.function_edges
                                .entry(caller_key.clone())
                                .or_default()
                                .insert(target.clone());
                        }
                        if caller != Some(&target) {
                            self.called_functions.insert(target.clone());
                        }
                        if self.imported_module_set.contains(&target.module) {
                            self.used_import_modules.insert(target.module.clone());
                        }
                    }
                }
                self.visit_expr(callee, module_name, caller);
                for arg in args {
                    self.visit_expr(arg, module_name, caller);
                }
            }
            ast::ExprKind::Closure {
                params,
                ret_type,
                body,
            } => {
                for param in params {
                    if let Some(ty) = &param.ty {
                        self.visit_type_expr(ty);
                    }
                }
                self.visit_type_expr(ret_type);
                self.visit_block(body, module_name, caller);
            }
            ast::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                self.visit_expr(cond, module_name, caller);
                self.visit_block(then_block, module_name, caller);
                self.visit_block(else_block, module_name, caller);
            }
            ast::ExprKind::While { cond, body } => {
                self.visit_expr(cond, module_name, caller);
                self.visit_block(body, module_name, caller);
            }
            ast::ExprKind::Loop { body } => self.visit_block(body, module_name, caller),
            ast::ExprKind::Break { expr } => {
                if let Some(expr) = expr {
                    self.visit_expr(expr, module_name, caller);
                }
            }
            ast::ExprKind::Continue => {}
            ast::ExprKind::Match { expr, arms } => {
                self.visit_expr(expr, module_name, caller);
                for arm in arms {
                    self.visit_pattern(&arm.pattern);
                    if let Some(guard) = &arm.guard {
                        self.visit_expr(guard, module_name, caller);
                    }
                    self.visit_expr(&arm.body, module_name, caller);
                }
            }
            ast::ExprKind::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs, module_name, caller);
                self.visit_expr(rhs, module_name, caller);
            }
            ast::ExprKind::Unary { expr, .. }
            | ast::ExprKind::Borrow { expr, .. }
            | ast::ExprKind::Await { expr }
            | ast::ExprKind::Try { expr } => self.visit_expr(expr, module_name, caller),
            ast::ExprKind::UnsafeBlock { block } => self.visit_block(block, module_name, caller),
            ast::ExprKind::StructInit { name, fields } => {
                if let Some(module_ref) = module_prefix(name) {
                    self.record_module_reference(module_ref);
                }
                for (_, expr, _) in fields {
                    self.visit_expr(expr, module_name, caller);
                }
            }
            ast::ExprKind::FieldAccess { base, .. } => self.visit_expr(base, module_name, caller),
            ast::ExprKind::Int(_)
            | ast::ExprKind::Float(_)
            | ast::ExprKind::Bool(_)
            | ast::ExprKind::Char(_)
            | ast::ExprKind::String(_)
            | ast::ExprKind::Unit => {}
        }
    }

    fn visit_pattern(&mut self, pattern: &ast::Pattern) {
        match &pattern.kind {
            ast::PatternKind::Or { patterns } => {
                for nested in patterns {
                    self.visit_pattern(nested);
                }
            }
            ast::PatternKind::Variant { name, args } => {
                if let Some(module_ref) = module_prefix(name) {
                    self.record_module_reference(module_ref);
                }
                for arg in args {
                    self.visit_pattern(arg);
                }
            }
            ast::PatternKind::Struct { name, fields, .. } => {
                if let Some(module_ref) = module_prefix(name) {
                    self.record_module_reference(module_ref);
                }
                for field in fields {
                    self.visit_pattern(&field.pattern);
                }
            }
            ast::PatternKind::Wildcard
            | ast::PatternKind::Var(_)
            | ast::PatternKind::Int(_)
            | ast::PatternKind::Char(_)
            | ast::PatternKind::String(_)
            | ast::PatternKind::Bool(_)
            | ast::PatternKind::Unit => {}
        }
    }

    fn visit_type_expr(&mut self, ty: &ast::TypeExpr) {
        match &ty.kind {
            ast::TypeKind::DynTrait { trait_name } => {
                if let Some(module_ref) = module_prefix(trait_name) {
                    self.record_module_reference(module_ref);
                }
            }
            ast::TypeKind::Named { name, args } => {
                if let Some(module_ref) = module_prefix(name) {
                    self.record_module_reference(module_ref);
                }
                for arg in args {
                    self.visit_type_expr(arg);
                }
            }
            ast::TypeKind::Unit | ast::TypeKind::Hole => {}
        }
    }

    fn visit_trait_bounds(&mut self, bounds: &[String]) {
        for bound in bounds {
            if let Some(module_ref) = module_prefix(bound) {
                self.record_module_reference(module_ref);
            }
        }
    }

    fn resolve_call_target(
        &self,
        caller_module: &str,
        call_path: &[String],
    ) -> Option<FunctionKey> {
        if call_path.is_empty() {
            return None;
        }

        if call_path.len() == 1 {
            let name = call_path[0].clone();
            let module = self.resolve_unqualified_call_module(caller_module, &name)?;
            let key = FunctionKey { module, name };
            return self.function_decls.contains_key(&key).then_some(key);
        }

        let qualifier = call_path[..call_path.len() - 1].join(".");
        let module = self.normalize_module_reference(&qualifier);
        let name = call_path[call_path.len() - 1].clone();
        let key = FunctionKey { module, name };
        self.function_decls.contains_key(&key).then_some(key)
    }

    fn resolve_unqualified_call_module(&self, caller_module: &str, name: &str) -> Option<String> {
        if let Some(candidates) = self.resolution.function_modules.get(name) {
            if candidates.contains(caller_module) {
                return Some(caller_module.to_string());
            }
            if candidates.len() == 1 {
                return candidates.iter().next().cloned();
            }
            return None;
        }

        let key = FunctionKey {
            module: caller_module.to_string(),
            name: name.to_string(),
        };
        self.function_decls.contains_key(&key).then_some(key.module)
    }

    fn normalize_module_reference(&self, module_ref: &str) -> String {
        if !module_ref.contains('.') {
            if let Some(mapped) = self.resolution.import_aliases.get(module_ref) {
                return mapped.clone();
            }
        }
        module_ref.to_string()
    }

    fn record_module_reference(&mut self, module_ref: &str) {
        let module = self.normalize_module_reference(module_ref);
        if self.imported_module_set.contains(&module) {
            self.used_import_modules.insert(module);
        }
    }

    fn emit_unused_import_warnings(&mut self) {
        for (path, infos) in &self.imports {
            if self.used_import_modules.contains(path) {
                continue;
            }
            for info in infos {
                let mut diagnostic = Diagnostic::warning(
                    UNUSED_IMPORT_CODE,
                    format!("unused import '{}'", path),
                    self.file,
                    info.span,
                )
                .with_help("remove the import or use a symbol from that module");
                if let Some(fix_span) = unused_import_fix_span(self.source, info.span) {
                    diagnostic = diagnostic.with_fix(SuggestedFix {
                        message: format!("remove unused import '{}'", path),
                        replacement: Some(String::new()),
                        start: Some(fix_span.start),
                        end: Some(fix_span.end),
                    });
                }
                self.diagnostics.push(diagnostic);
            }
        }
    }

    fn emit_unused_function_warnings(&mut self) {
        let roots = self
            .function_decls
            .keys()
            .filter(|key| key.name == "main")
            .cloned()
            .collect::<Vec<_>>();
        let reachable = reachable_from_roots(&roots, &self.function_edges, &self.function_decls);

        for (key, decl) in &self.function_decls {
            if key.name == "main" || decl.is_extern || decl.is_intrinsic {
                continue;
            }

            if !roots.is_empty() && !reachable.contains(key) {
                self.diagnostics.push(
                    Diagnostic::warning(
                        UNUSED_FUNCTION_CODE,
                        format!(
                            "function '{}' is unreachable from entrypoint",
                            key.display()
                        ),
                        self.file,
                        decl.span,
                    )
                    .with_help("remove the function or invoke it from reachable code"),
                );
                continue;
            }

            if !self.called_functions.contains(key) {
                self.diagnostics.push(
                    Diagnostic::warning(
                        UNUSED_FUNCTION_CODE,
                        format!("function '{}' is never called", key.display()),
                        self.file,
                        decl.span,
                    )
                    .with_help("remove the function or invoke it from live code paths"),
                );
            }
        }
    }

    fn emit_unused_variable_warnings(&mut self) {
        for (index, item) in self.program.items.iter().enumerate() {
            let module_name = module_for_item(self.program, self.item_modules, index);
            if module_name != self.entry_module {
                continue;
            }
            match item {
                ast::Item::Function(func) => {
                    if is_internal_function(func) {
                        continue;
                    }
                    self.diagnostics.extend(collect_unused_variable_warnings(
                        func,
                        self.file,
                        self.source,
                    ));
                }
                ast::Item::Trait(trait_def) => {
                    for method in &trait_def.methods {
                        self.diagnostics.extend(collect_unused_variable_warnings(
                            method,
                            self.file,
                            self.source,
                        ));
                    }
                }
                ast::Item::Impl(impl_def) => {
                    for method in &impl_def.methods {
                        self.diagnostics.extend(collect_unused_variable_warnings(
                            method,
                            self.file,
                            self.source,
                        ));
                    }
                }
                ast::Item::Struct(_) | ast::Item::Enum(_) => {}
            }
        }
    }
}

fn collect_unused_variable_warnings(
    func: &ast::Function,
    file: &str,
    source: &str,
) -> Vec<Diagnostic> {
    let mut analyzer = VariableAnalyzer::new(file, source);
    analyzer.enter_scope();
    for param in &func.params {
        let fix_span = find_first_identifier_span(source, param.span, &param.name);
        analyzer.declare_binding(param.name.clone(), param.span, fix_span);
    }
    if let Some(requires) = &func.requires {
        analyzer.visit_expr(requires);
    }
    if let Some(ensures) = &func.ensures {
        analyzer.visit_expr(ensures);
    }
    analyzer.visit_block(&func.body);
    analyzer.exit_scope();
    analyzer.finish()
}

struct VariableAnalyzer<'a> {
    file: &'a str,
    source: &'a str,
    bindings: Vec<Binding>,
    scopes: Vec<Vec<usize>>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
struct Binding {
    name: String,
    span: Span,
    fix_span: Option<Span>,
    used: bool,
}

impl<'a> VariableAnalyzer<'a> {
    fn new(file: &'a str, source: &'a str) -> Self {
        Self {
            file,
            source,
            bindings: Vec::new(),
            scopes: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn finish(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    fn enter_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn exit_scope(&mut self) {
        let Some(indices) = self.scopes.pop() else {
            return;
        };
        for idx in indices {
            let Some(binding) = self.bindings.get(idx) else {
                continue;
            };
            if binding.used || binding.name.starts_with('_') {
                continue;
            }
            let mut diagnostic = Diagnostic::warning(
                UNUSED_VARIABLE_CODE,
                format!("unused variable '{}'", binding.name),
                self.file,
                binding.span,
            )
            .with_help("prefix with '_' to mark intentionally unused");

            if let Some(fix_span) = binding.fix_span {
                if span_text(self.source, fix_span) == Some(binding.name.as_str()) {
                    diagnostic = diagnostic.with_fix(SuggestedFix {
                        message: format!("prefix unused variable '{}' with '_'", binding.name),
                        replacement: Some(format!("_{}", binding.name)),
                        start: Some(fix_span.start),
                        end: Some(fix_span.end),
                    });
                }
            }
            self.diagnostics.push(diagnostic);
        }
    }

    fn declare_binding(&mut self, name: String, span: Span, fix_span: Option<Span>) {
        if self.scopes.is_empty() {
            self.enter_scope();
        }
        let idx = self.bindings.len();
        self.bindings.push(Binding {
            name,
            span,
            fix_span,
            used: false,
        });
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(idx);
        }
    }

    fn mark_used(&mut self, name: &str) {
        if name.contains('.') {
            return;
        }
        for scope in self.scopes.iter().rev() {
            for idx in scope.iter().rev() {
                if let Some(binding) = self.bindings.get_mut(*idx) {
                    if binding.name == name {
                        binding.used = true;
                        return;
                    }
                }
            }
        }
    }

    fn visit_block(&mut self, block: &ast::Block) {
        self.enter_scope();
        for stmt in &block.stmts {
            self.visit_stmt(stmt);
        }
        if let Some(tail) = &block.tail {
            self.visit_expr(tail);
        }
        self.exit_scope();
    }

    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::Let {
                name, expr, span, ..
            } => {
                self.visit_expr(expr);
                let fix_span = find_let_binding_name_span(self.source, *span, name);
                self.declare_binding(name.clone(), *span, fix_span);
            }
            ast::Stmt::Assign { target, expr, .. } => {
                self.mark_used(target);
                self.visit_expr(expr);
            }
            ast::Stmt::Expr { expr, .. } => self.visit_expr(expr),
            ast::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    self.visit_expr(expr);
                }
            }
            ast::Stmt::Assert { expr, .. } => self.visit_expr(expr),
        }
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        match &expr.kind {
            ast::ExprKind::Var(name) => self.mark_used(name),
            ast::ExprKind::Call { callee, args, .. } => {
                self.visit_expr(callee);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ast::ExprKind::Closure { params, body, .. } => {
                self.enter_scope();
                for param in params {
                    let fix_span = find_first_identifier_span(self.source, param.span, &param.name);
                    self.declare_binding(param.name.clone(), param.span, fix_span);
                }
                self.visit_block(body);
                self.exit_scope();
            }
            ast::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                self.visit_expr(cond);
                self.visit_block(then_block);
                self.visit_block(else_block);
            }
            ast::ExprKind::While { cond, body } => {
                self.visit_expr(cond);
                self.visit_block(body);
            }
            ast::ExprKind::Loop { body } => self.visit_block(body),
            ast::ExprKind::Break { expr } => {
                if let Some(expr) = expr {
                    self.visit_expr(expr);
                }
            }
            ast::ExprKind::Continue => {}
            ast::ExprKind::Match { expr, arms } => {
                self.visit_expr(expr);
                for arm in arms {
                    self.enter_scope();
                    self.bind_pattern_vars(&arm.pattern);
                    if let Some(guard) = &arm.guard {
                        self.visit_expr(guard);
                    }
                    self.visit_expr(&arm.body);
                    self.exit_scope();
                }
            }
            ast::ExprKind::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            ast::ExprKind::Unary { expr, .. }
            | ast::ExprKind::Borrow { expr, .. }
            | ast::ExprKind::Await { expr }
            | ast::ExprKind::Try { expr } => self.visit_expr(expr),
            ast::ExprKind::UnsafeBlock { block } => self.visit_block(block),
            ast::ExprKind::StructInit { fields, .. } => {
                for (_, field_expr, _) in fields {
                    self.visit_expr(field_expr);
                }
            }
            ast::ExprKind::FieldAccess { base, .. } => self.visit_expr(base),
            ast::ExprKind::Int(_)
            | ast::ExprKind::Float(_)
            | ast::ExprKind::Bool(_)
            | ast::ExprKind::Char(_)
            | ast::ExprKind::String(_)
            | ast::ExprKind::Unit => {}
        }
    }

    fn bind_pattern_vars(&mut self, pattern: &ast::Pattern) {
        let mut vars = Vec::new();
        collect_pattern_vars(pattern, &mut vars);
        let mut seen = BTreeSet::new();
        for (name, span) in vars {
            if !seen.insert(name.clone()) {
                continue;
            }
            let fix_span = if span_text(self.source, span) == Some(name.as_str()) {
                Some(span)
            } else {
                None
            };
            self.declare_binding(name, span, fix_span);
        }
    }
}

fn collect_pattern_vars(pattern: &ast::Pattern, out: &mut Vec<(String, Span)>) {
    match &pattern.kind {
        ast::PatternKind::Var(name) => out.push((name.clone(), pattern.span)),
        ast::PatternKind::Or { patterns } => {
            for nested in patterns {
                collect_pattern_vars(nested, out);
            }
        }
        ast::PatternKind::Variant { args, .. } => {
            for arg in args {
                collect_pattern_vars(arg, out);
            }
        }
        ast::PatternKind::Struct { fields, .. } => {
            for field in fields {
                collect_pattern_vars(&field.pattern, out);
            }
        }
        ast::PatternKind::Wildcard
        | ast::PatternKind::Int(_)
        | ast::PatternKind::Char(_)
        | ast::PatternKind::String(_)
        | ast::PatternKind::Bool(_)
        | ast::PatternKind::Unit => {}
    }
}

fn module_for_item(
    program: &ast::Program,
    item_modules: &[Option<Vec<String>>],
    index: usize,
) -> String {
    if let Some(module) = item_modules
        .get(index)
        .and_then(|module| module.as_ref())
        .map(|module| module.join("."))
    {
        return module;
    }
    if let Some(module) = &program.module {
        return module.path.join(".");
    }
    ROOT_MODULE.to_string()
}

fn is_internal_function(func: &ast::Function) -> bool {
    decode_internal_type_alias(&func.name).is_some() || decode_internal_const(&func.name).is_some()
}

fn call_path(expr: &ast::Expr) -> Option<Vec<String>> {
    let ast::ExprKind::Var(name) = &expr.kind else {
        return None;
    };
    let parts = name
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

fn module_prefix(path: &str) -> Option<&str> {
    let idx = path.rfind('.')?;
    if idx == 0 || idx + 1 >= path.len() {
        return None;
    }
    Some(&path[..idx])
}

fn reachable_from_roots(
    roots: &[FunctionKey],
    edges: &BTreeMap<FunctionKey, BTreeSet<FunctionKey>>,
    decls: &BTreeMap<FunctionKey, FunctionDecl>,
) -> BTreeSet<FunctionKey> {
    let mut reachable = BTreeSet::new();
    let mut queue = VecDeque::new();
    for root in roots {
        if decls.contains_key(root) && reachable.insert(root.clone()) {
            queue.push_back(root.clone());
        }
    }
    while let Some(current) = queue.pop_front() {
        if let Some(next) = edges.get(&current) {
            for target in next {
                if reachable.insert(target.clone()) {
                    queue.push_back(target.clone());
                }
            }
        }
    }
    reachable
}

fn unused_import_fix_span(source: &str, import_span: Span) -> Option<Span> {
    let len = source.len();
    if import_span.start > import_span.end || import_span.end > len {
        return None;
    }
    let bytes = source.as_bytes();
    let mut end = import_span.end;

    while end < len && matches!(bytes[end], b' ' | b'\t') {
        end += 1;
    }
    if end < len && bytes[end] == b';' {
        end += 1;
    }
    while end < len && matches!(bytes[end], b' ' | b'\t') {
        end += 1;
    }
    if end + 1 < len && bytes[end] == b'\r' && bytes[end + 1] == b'\n' {
        end += 2;
    } else if end < len && bytes[end] == b'\n' {
        end += 1;
    }

    Some(Span::new(import_span.start, end))
}

fn find_let_binding_name_span(source: &str, stmt_span: Span, expected: &str) -> Option<Span> {
    let len = source.len();
    if stmt_span.start > stmt_span.end || stmt_span.end > len {
        return None;
    }
    let bytes = source.as_bytes();
    let mut idx = stmt_span.start;
    skip_whitespace(bytes, &mut idx, stmt_span.end);

    if idx + 3 > stmt_span.end || &bytes[idx..idx + 3] != b"let" {
        return None;
    }
    idx += 3;
    if idx < stmt_span.end && is_ident_continue(bytes[idx]) {
        return None;
    }
    skip_whitespace(bytes, &mut idx, stmt_span.end);

    if idx + 3 <= stmt_span.end && &bytes[idx..idx + 3] == b"mut" {
        let after_mut = idx + 3;
        if after_mut == stmt_span.end || !is_ident_continue(bytes[after_mut]) {
            idx = after_mut;
            skip_whitespace(bytes, &mut idx, stmt_span.end);
        }
    }

    let ident_span = find_identifier_span(source, idx, stmt_span.end)?;
    if span_text(source, ident_span) == Some(expected) {
        Some(ident_span)
    } else {
        None
    }
}

fn find_first_identifier_span(source: &str, span: Span, expected: &str) -> Option<Span> {
    let ident_span = find_identifier_span(source, span.start, span.end)?;
    if span_text(source, ident_span) == Some(expected) {
        Some(ident_span)
    } else {
        None
    }
}

fn find_identifier_span(source: &str, start: usize, end: usize) -> Option<Span> {
    if start > end || end > source.len() {
        return None;
    }
    let bytes = source.as_bytes();
    let mut idx = start;
    skip_whitespace(bytes, &mut idx, end);
    if idx >= end || !is_ident_start(bytes[idx]) {
        return None;
    }
    let ident_start = idx;
    idx += 1;
    while idx < end && is_ident_continue(bytes[idx]) {
        idx += 1;
    }
    Some(Span::new(ident_start, idx))
}

fn skip_whitespace(bytes: &[u8], idx: &mut usize, end: usize) {
    while *idx < end && matches!(bytes[*idx], b' ' | b'\t' | b'\n' | b'\r') {
        *idx += 1;
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

fn span_text(source: &str, span: Span) -> Option<&str> {
    if span.start > span.end || span.end > source.len() {
        return None;
    }
    if !source.is_char_boundary(span.start) || !source.is_char_boundary(span.end) {
        return None;
    }
    Some(&source[span.start..span.end])
}

#[cfg(test)]
mod tests {
    use crate::{ir_builder::build, parser::parse, resolver::resolve_with_item_modules};

    use super::analyze_unused_warnings;

    fn analyze(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
        let file = "test.aic";
        let (program, parse_diags) = parse(source, file);
        assert!(parse_diags.is_empty(), "parse diagnostics={parse_diags:#?}");
        let program = program.expect("program");
        let item_modules = program
            .items
            .iter()
            .map(|_| program.module.as_ref().map(|module| module.path.clone()))
            .collect::<Vec<_>>();
        let ir = build(&program);
        let (resolution, resolve_diags) =
            resolve_with_item_modules(&ir, file, Some(item_modules.as_slice()));
        assert!(
            !resolve_diags
                .iter()
                .any(crate::diagnostics::Diagnostic::is_error),
            "resolve diagnostics={resolve_diags:#?}"
        );
        analyze_unused_warnings(&program, &resolution, item_modules.as_slice(), file, source)
    }

    #[test]
    fn detects_unused_import_function_and_variable_with_fixes() {
        let source = concat!(
            "module demo.main;\n",
            "import std.io;\n",
            "\n",
            "fn helper() -> Int {\n",
            "    1\n",
            "}\n",
            "\n",
            "fn dead() -> Int {\n",
            "    2\n",
            "}\n",
            "\n",
            "fn main() -> Int {\n",
            "    let scratch = helper();\n",
            "    0\n",
            "}\n",
        );

        let first = analyze(source);
        let second = analyze(source);
        assert_eq!(first, second);
        assert!(first.iter().any(|diag| diag.code == "E6004"));
        assert!(first.iter().any(|diag| diag.code == "E6005"));
        assert!(first.iter().any(|diag| diag.code == "E6006"));

        let import_diag = first
            .iter()
            .find(|diag| diag.code == "E6004")
            .expect("missing E6004");
        assert!(
            !import_diag.suggested_fixes.is_empty(),
            "missing import fix: {import_diag:#?}"
        );

        let variable_diag = first
            .iter()
            .find(|diag| diag.code == "E6006" && diag.message.contains("scratch"))
            .expect("missing E6006 for scratch");
        assert!(
            variable_diag
                .suggested_fixes
                .iter()
                .any(|fix| fix.replacement.as_deref() == Some("_scratch")),
            "expected _scratch fix, got {variable_diag:#?}"
        );
    }

    #[test]
    fn does_not_warn_when_symbols_are_used_or_intentionally_prefixed() {
        let source = concat!(
            "module demo.main;\n",
            "\n",
            "fn helper() -> Int {\n",
            "    1\n",
            "}\n",
            "\n",
            "fn main() -> Int {\n",
            "    let _scratch = helper();\n",
            "    _scratch\n",
            "}\n",
        );

        let diagnostics = analyze(source);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {diagnostics:#?}"
        );
    }
}
