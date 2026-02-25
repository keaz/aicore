use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::ast::{
    decode_internal_const, decode_internal_type_alias, BinOp, Block, Expr, ExprKind, Function,
    Item, Program, Stmt, UnaryOp,
};
use crate::driver::FrontendOutput;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SuggestContractsResponse {
    pub suggestions: Vec<FunctionContractSuggestion>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FunctionContractSuggestion {
    pub function: String,
    pub suggested_requires: Vec<ContractClauseSuggestion>,
    pub suggested_ensures: Vec<ContractClauseSuggestion>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ContractClauseSuggestion {
    pub expr: String,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VarContext {
    Value,
    Callee,
}

#[derive(Default)]
struct ClauseAccumulator {
    by_expr: BTreeMap<String, ClauseEvidence>,
}

#[derive(Default)]
struct ClauseEvidence {
    confidence: f64,
    reasons: BTreeSet<String>,
}

struct FunctionEntry<'a> {
    name: String,
    function: &'a Function,
}

enum ReturnObservation<'a> {
    Expr(&'a Expr),
    Unit,
}

struct ReturnEvidence {
    expr: String,
    confidence: f64,
    reason: String,
}

pub fn analyze(front: &FrontendOutput) -> SuggestContractsResponse {
    let mut functions = collect_functions(&front.ast);
    functions.sort_by(|left, right| left.name.cmp(&right.name));

    let mut suggestions = Vec::with_capacity(functions.len());
    for entry in functions {
        suggestions.push(analyze_function(entry));
    }

    SuggestContractsResponse { suggestions }
}

pub fn format_text(response: &SuggestContractsResponse) -> String {
    let mut lines = Vec::new();
    let mut rendered = 0usize;

    for suggestion in &response.suggestions {
        if suggestion.suggested_requires.is_empty() && suggestion.suggested_ensures.is_empty() {
            continue;
        }
        rendered += 1;
        lines.push(format!("function {}", suggestion.function));
        lines.push("  requires:".to_string());
        append_clause_lines(&mut lines, &suggestion.suggested_requires);
        lines.push("  ensures:".to_string());
        append_clause_lines(&mut lines, &suggestion.suggested_ensures);
        lines.push(String::new());
    }

    if rendered == 0 {
        "suggest-contracts: no contract suggestions".to_string()
    } else {
        while matches!(lines.last(), Some(last) if last.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }
}

fn append_clause_lines(lines: &mut Vec<String>, clauses: &[ContractClauseSuggestion]) {
    if clauses.is_empty() {
        lines.push("    - (none)".to_string());
        return;
    }

    for clause in clauses {
        lines.push(format!(
            "    - {} [confidence={:.2}] ({})",
            clause.expr, clause.confidence, clause.reason
        ));
    }
}

fn collect_functions(program: &Program) -> Vec<FunctionEntry<'_>> {
    let mut functions = Vec::new();
    for item in &program.items {
        match item {
            Item::Function(function) => {
                if decode_internal_type_alias(&function.name).is_some()
                    || decode_internal_const(&function.name).is_some()
                {
                    continue;
                }
                functions.push(FunctionEntry {
                    name: function.name.clone(),
                    function,
                });
            }
            Item::Trait(trait_def) => {
                for method in &trait_def.methods {
                    functions.push(FunctionEntry {
                        name: format!("{}::{}", trait_def.name, method.name),
                        function: method,
                    });
                }
            }
            Item::Impl(impl_def) => {
                for method in &impl_def.methods {
                    functions.push(FunctionEntry {
                        name: format!("{}::{}", impl_def.trait_name, method.name),
                        function: method,
                    });
                }
            }
            Item::Struct(_) | Item::Enum(_) => {}
        }
    }
    functions
}

fn analyze_function(entry: FunctionEntry<'_>) -> FunctionContractSuggestion {
    let param_names = entry
        .function
        .params
        .iter()
        .map(|param| param.name.clone())
        .collect::<BTreeSet<_>>();

    let mut requires = ClauseAccumulator::default();
    collect_requires_from_block(&entry.function.body, &param_names, &mut requires);

    let mut ensures = ClauseAccumulator::default();
    infer_postconditions(entry.function, &param_names, &mut ensures);

    let existing_requires = entry.function.requires.as_ref().and_then(expr_to_string);
    let existing_ensures = entry.function.ensures.as_ref().and_then(expr_to_string);

    FunctionContractSuggestion {
        function: entry.name,
        suggested_requires: requires.into_suggestions(existing_requires.as_deref()),
        suggested_ensures: ensures.into_suggestions(existing_ensures.as_deref()),
    }
}

impl ClauseAccumulator {
    fn add(&mut self, expr: String, confidence: f64, reason: &str) {
        if expr.trim().is_empty() {
            return;
        }
        let confidence = normalize_confidence(confidence);
        let entry = self.by_expr.entry(expr).or_default();
        if confidence > entry.confidence {
            entry.confidence = confidence;
        }
        entry.reasons.insert(reason.to_string());
    }

    fn into_suggestions(self, skip: Option<&str>) -> Vec<ContractClauseSuggestion> {
        self.by_expr
            .into_iter()
            .filter_map(|(expr, evidence)| {
                if skip == Some(expr.as_str()) {
                    return None;
                }
                Some(ContractClauseSuggestion {
                    expr,
                    confidence: evidence.confidence,
                    reason: evidence.reasons.into_iter().collect::<Vec<_>>().join("; "),
                })
            })
            .collect()
    }
}

fn collect_requires_from_block(
    block: &Block,
    param_names: &BTreeSet<String>,
    requires: &mut ClauseAccumulator,
) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { expr, .. } | Stmt::Assign { expr, .. } | Stmt::Expr { expr, .. } => {
                collect_requires_from_expr(expr, param_names, requires);
            }
            Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    collect_requires_from_expr(expr, param_names, requires);
                }
            }
            Stmt::Assert { expr, .. } => {
                add_condition_candidates(
                    expr,
                    param_names,
                    0.96,
                    "assertion checks this condition",
                    requires,
                );
            }
        }
    }
    if let Some(tail) = block.tail.as_deref() {
        collect_requires_from_expr(tail, param_names, requires);
    }
}

fn collect_requires_from_expr(
    expr: &Expr,
    param_names: &BTreeSet<String>,
    requires: &mut ClauseAccumulator,
) {
    match &expr.kind {
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            add_condition_candidates(
                cond,
                param_names,
                0.78,
                "conditional guard checks this condition",
                requires,
            );
            collect_requires_from_block(then_block, param_names, requires);
            collect_requires_from_block(else_block, param_names, requires);
        }
        ExprKind::While { cond, body } => {
            add_condition_candidates(
                cond,
                param_names,
                0.66,
                "loop guard checks this condition",
                requires,
            );
            collect_requires_from_block(body, param_names, requires);
        }
        ExprKind::Loop { body } => collect_requires_from_block(body, param_names, requires),
        ExprKind::Match { expr, arms } => {
            collect_requires_from_expr(expr, param_names, requires);
            for arm in arms {
                if let Some(guard) = arm.guard.as_ref() {
                    add_condition_candidates(
                        guard,
                        param_names,
                        0.74,
                        "match guard checks this condition",
                        requires,
                    );
                }
                collect_requires_from_expr(&arm.body, param_names, requires);
            }
        }
        ExprKind::Call { callee, args } => {
            if let ExprKind::Var(name) = &callee.kind {
                if name == "assert" {
                    if let Some(first) = args.first() {
                        add_condition_candidates(
                            first,
                            param_names,
                            0.95,
                            "assertion-style call checks this condition",
                            requires,
                        );
                    }
                }
            }
            collect_requires_from_expr(callee, param_names, requires);
            for arg in args {
                collect_requires_from_expr(arg, param_names, requires);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_requires_from_expr(lhs, param_names, requires);
            collect_requires_from_expr(rhs, param_names, requires);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Borrow { expr, .. }
        | ExprKind::Await { expr }
        | ExprKind::Try { expr } => collect_requires_from_expr(expr, param_names, requires),
        ExprKind::FieldAccess { base, .. } => {
            collect_requires_from_expr(base, param_names, requires)
        }
        ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                collect_requires_from_expr(value, param_names, requires);
            }
        }
        ExprKind::UnsafeBlock { block } => {
            collect_requires_from_block(block, param_names, requires)
        }
        ExprKind::Break { expr } => {
            if let Some(expr) = expr.as_deref() {
                collect_requires_from_expr(expr, param_names, requires);
            }
        }
        ExprKind::Closure { .. }
        | ExprKind::Continue
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::Unit
        | ExprKind::Var(_) => {}
    }
}

fn add_condition_candidates(
    expr: &Expr,
    param_names: &BTreeSet<String>,
    confidence: f64,
    reason: &str,
    requires: &mut ClauseAccumulator,
) {
    match &expr.kind {
        ExprKind::Binary {
            op: BinOp::And,
            lhs,
            rhs,
        } => {
            add_condition_candidates(lhs, param_names, confidence, reason, requires);
            add_condition_candidates(rhs, param_names, confidence, reason, requires);
        }
        ExprKind::Binary { op: BinOp::Or, .. } => {
            add_single_condition(expr, param_names, confidence * 0.6, reason, requires)
        }
        _ => {
            if is_condition_shape(expr) {
                add_single_condition(expr, param_names, confidence, reason, requires);
            }
        }
    }
}

fn add_single_condition(
    expr: &Expr,
    param_names: &BTreeSet<String>,
    confidence: f64,
    reason: &str,
    requires: &mut ClauseAccumulator,
) {
    if !expr_references_params(expr, param_names)
        || !expr_uses_only_contract_visible_values(expr, param_names)
    {
        return;
    }
    let Some(rendered) = expr_to_string(expr) else {
        return;
    };
    requires.add(rendered, confidence, reason);
}

fn infer_postconditions(
    function: &Function,
    param_names: &BTreeSet<String>,
    ensures: &mut ClauseAccumulator,
) {
    let mut observations = Vec::new();
    collect_explicit_returns_from_block(&function.body, &mut observations);
    if let Some(tail) = function.body.tail.as_deref() {
        observations.push(ReturnObservation::Expr(tail));
    }

    if observations.is_empty() {
        return;
    }

    let total_observations = observations.len();
    let mut representable = 0usize;
    let mut by_expr = BTreeMap::<String, ReturnEvidence>::new();
    for observation in observations {
        let Some(evidence) = return_observation_evidence(observation, param_names) else {
            continue;
        };
        representable += 1;
        by_expr.entry(evidence.expr.clone()).or_insert(evidence);
    }

    if by_expr.len() != 1 || representable != total_observations {
        return;
    }

    let evidence = by_expr.into_values().next().expect("single evidence");
    ensures.add(
        format!("result == {}", evidence.expr),
        evidence.confidence,
        &evidence.reason,
    );
}

fn collect_explicit_returns_from_block<'a>(
    block: &'a Block,
    observations: &mut Vec<ReturnObservation<'a>>,
) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Return { expr, .. } => match expr {
                Some(expr) => observations.push(ReturnObservation::Expr(expr)),
                None => observations.push(ReturnObservation::Unit),
            },
            Stmt::Let { expr, .. } | Stmt::Assign { expr, .. } | Stmt::Expr { expr, .. } => {
                collect_explicit_returns_from_expr(expr, observations)
            }
            Stmt::Assert { expr, .. } => collect_explicit_returns_from_expr(expr, observations),
        }
    }
    if let Some(tail) = block.tail.as_deref() {
        collect_explicit_returns_from_expr(tail, observations);
    }
}

fn collect_explicit_returns_from_expr<'a>(
    expr: &'a Expr,
    observations: &mut Vec<ReturnObservation<'a>>,
) {
    match &expr.kind {
        ExprKind::If {
            then_block,
            else_block,
            ..
        } => {
            collect_explicit_returns_from_block(then_block, observations);
            collect_explicit_returns_from_block(else_block, observations);
        }
        ExprKind::While { body, .. } | ExprKind::Loop { body } => {
            collect_explicit_returns_from_block(body, observations);
        }
        ExprKind::Match { arms, .. } => {
            for arm in arms {
                collect_explicit_returns_from_expr(&arm.body, observations);
            }
        }
        ExprKind::UnsafeBlock { block } => {
            collect_explicit_returns_from_block(block, observations);
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_explicit_returns_from_expr(lhs, observations);
            collect_explicit_returns_from_expr(rhs, observations);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Borrow { expr, .. }
        | ExprKind::Await { expr }
        | ExprKind::Try { expr } => collect_explicit_returns_from_expr(expr, observations),
        ExprKind::Call { callee, args } => {
            collect_explicit_returns_from_expr(callee, observations);
            for arg in args {
                collect_explicit_returns_from_expr(arg, observations);
            }
        }
        ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                collect_explicit_returns_from_expr(value, observations);
            }
        }
        ExprKind::FieldAccess { base, .. } => {
            collect_explicit_returns_from_expr(base, observations)
        }
        ExprKind::Break { expr } => {
            if let Some(value) = expr.as_deref() {
                collect_explicit_returns_from_expr(value, observations);
            }
        }
        ExprKind::Closure { .. }
        | ExprKind::Continue
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::Unit
        | ExprKind::Var(_) => {}
    }
}

fn return_observation_evidence(
    observation: ReturnObservation<'_>,
    param_names: &BTreeSet<String>,
) -> Option<ReturnEvidence> {
    match observation {
        ReturnObservation::Unit => Some(ReturnEvidence {
            expr: "()".to_string(),
            confidence: 0.92,
            reason: "function returns unit on all observed paths".to_string(),
        }),
        ReturnObservation::Expr(expr) => {
            if !expr_uses_only_contract_visible_values(expr, param_names) {
                return None;
            }
            let rendered = expr_to_string(expr)?;
            let (confidence, reason) = match &expr.kind {
                ExprKind::Int(_)
                | ExprKind::Float(_)
                | ExprKind::Bool(_)
                | ExprKind::String(_)
                | ExprKind::Unit => (
                    0.92,
                    "function returns this literal on all observed paths".to_string(),
                ),
                ExprKind::Var(name) if param_names.contains(name) => (
                    0.88,
                    format!("function returns parameter `{name}` on all observed paths"),
                ),
                _ => (
                    0.72,
                    "function returns the same parameter-derived expression on all observed paths"
                        .to_string(),
                ),
            };
            Some(ReturnEvidence {
                expr: rendered,
                confidence,
                reason,
            })
        }
    }
}

fn expr_references_params(expr: &Expr, param_names: &BTreeSet<String>) -> bool {
    let mut vars = BTreeSet::new();
    collect_value_vars(expr, VarContext::Value, &mut vars);
    vars.iter().any(|name| param_names.contains(name))
}

fn expr_uses_only_contract_visible_values(expr: &Expr, param_names: &BTreeSet<String>) -> bool {
    let mut vars = BTreeSet::new();
    collect_value_vars(expr, VarContext::Value, &mut vars);
    vars.into_iter()
        .all(|name| param_names.contains(&name) || is_constructor_symbol(&name))
}

fn is_constructor_symbol(name: &str) -> bool {
    name.chars()
        .next()
        .map(|ch| ch.is_uppercase())
        .unwrap_or(false)
}

fn collect_value_vars(expr: &Expr, context: VarContext, out: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Var(name) => {
            if matches!(context, VarContext::Value) {
                out.insert(name.clone());
            }
        }
        ExprKind::Call { callee, args } => {
            collect_value_vars(callee, VarContext::Callee, out);
            for arg in args {
                collect_value_vars(arg, VarContext::Value, out);
            }
        }
        ExprKind::FieldAccess { base, .. } => collect_value_vars(base, VarContext::Value, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_value_vars(lhs, VarContext::Value, out);
            collect_value_vars(rhs, VarContext::Value, out);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Borrow { expr, .. }
        | ExprKind::Await { expr }
        | ExprKind::Try { expr } => collect_value_vars(expr, VarContext::Value, out),
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_value_vars(cond, VarContext::Value, out);
            collect_value_vars_block(then_block, out);
            collect_value_vars_block(else_block, out);
        }
        ExprKind::While { cond, body } => {
            collect_value_vars(cond, VarContext::Value, out);
            collect_value_vars_block(body, out);
        }
        ExprKind::Loop { body } | ExprKind::UnsafeBlock { block: body } => {
            collect_value_vars_block(body, out);
        }
        ExprKind::Match { expr, arms } => {
            collect_value_vars(expr, VarContext::Value, out);
            for arm in arms {
                if let Some(guard) = arm.guard.as_ref() {
                    collect_value_vars(guard, VarContext::Value, out);
                }
                collect_value_vars(&arm.body, VarContext::Value, out);
            }
        }
        ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                collect_value_vars(value, VarContext::Value, out);
            }
        }
        ExprKind::Break { expr } => {
            if let Some(value) = expr.as_deref() {
                collect_value_vars(value, VarContext::Value, out);
            }
        }
        ExprKind::Closure { .. }
        | ExprKind::Continue
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::Unit => {}
    }
}

fn collect_value_vars_block(block: &Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { expr, .. } | Stmt::Assign { expr, .. } | Stmt::Expr { expr, .. } => {
                collect_value_vars(expr, VarContext::Value, out);
            }
            Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    collect_value_vars(expr, VarContext::Value, out);
                }
            }
            Stmt::Assert { expr, .. } => collect_value_vars(expr, VarContext::Value, out),
        }
    }
    if let Some(tail) = block.tail.as_deref() {
        collect_value_vars(tail, VarContext::Value, out);
    }
}

fn is_condition_shape(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Binary { op, .. } => matches!(
            op,
            BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or
        ),
        ExprKind::Unary { op, .. } => matches!(op, UnaryOp::Not),
        ExprKind::Var(_) | ExprKind::Call { .. } | ExprKind::FieldAccess { .. } => true,
        _ => false,
    }
}

fn normalize_confidence(value: f64) -> f64 {
    (value.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

fn expr_to_string(expr: &Expr) -> Option<String> {
    render_expr(expr, 0)
}

fn render_expr(expr: &Expr, parent_prec: u8) -> Option<String> {
    const PREC_OR: u8 = 1;
    const PREC_AND: u8 = 2;
    const PREC_BIT_OR: u8 = 3;
    const PREC_BIT_XOR: u8 = 4;
    const PREC_BIT_AND: u8 = 5;
    const PREC_COMPARE: u8 = 6;
    const PREC_SHIFT: u8 = 7;
    const PREC_ADD: u8 = 8;
    const PREC_MUL: u8 = 9;
    const PREC_UNARY: u8 = 10;
    const PREC_POSTFIX: u8 = 11;

    match &expr.kind {
        ExprKind::Int(value) => Some(value.to_string()),
        ExprKind::Float(value) => Some(format_float_literal(*value)),
        ExprKind::Bool(value) => Some(value.to_string()),
        ExprKind::String(value) => serde_json::to_string(value).ok(),
        ExprKind::Unit => Some("()".to_string()),
        ExprKind::Var(name) => Some(name.clone()),
        ExprKind::Unary { op, expr } => {
            let symbol = match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "!",
                UnaryOp::BitNot => "~",
            };
            let rendered = format!("{symbol}{}", render_expr(expr, PREC_UNARY)?);
            parenthesize(rendered, PREC_UNARY, parent_prec)
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let (symbol, prec) = match op {
                BinOp::Or => ("||", PREC_OR),
                BinOp::And => ("&&", PREC_AND),
                BinOp::BitOr => ("|", PREC_BIT_OR),
                BinOp::BitXor => ("^", PREC_BIT_XOR),
                BinOp::BitAnd => ("&", PREC_BIT_AND),
                BinOp::Eq => ("==", PREC_COMPARE),
                BinOp::Ne => ("!=", PREC_COMPARE),
                BinOp::Lt => ("<", PREC_COMPARE),
                BinOp::Le => ("<=", PREC_COMPARE),
                BinOp::Gt => (">", PREC_COMPARE),
                BinOp::Ge => (">=", PREC_COMPARE),
                BinOp::Shl => ("<<", PREC_SHIFT),
                BinOp::Shr => (">>", PREC_SHIFT),
                BinOp::Ushr => (">>>", PREC_SHIFT),
                BinOp::Add => ("+", PREC_ADD),
                BinOp::Sub => ("-", PREC_ADD),
                BinOp::Mul => ("*", PREC_MUL),
                BinOp::Div => ("/", PREC_MUL),
                BinOp::Mod => ("%", PREC_MUL),
            };
            let left = render_expr(lhs, prec)?;
            let right = render_expr(rhs, prec + 1)?;
            parenthesize(format!("{left} {symbol} {right}"), prec, parent_prec)
        }
        ExprKind::Call { callee, args } => {
            let callee = render_expr(callee, PREC_POSTFIX)?;
            let mut rendered_args = Vec::with_capacity(args.len());
            for arg in args {
                rendered_args.push(render_expr(arg, 0)?);
            }
            parenthesize(
                format!("{callee}({})", rendered_args.join(", ")),
                PREC_POSTFIX,
                parent_prec,
            )
        }
        ExprKind::FieldAccess { base, field } => parenthesize(
            format!("{}.{}", render_expr(base, PREC_POSTFIX)?, field),
            PREC_POSTFIX,
            parent_prec,
        ),
        ExprKind::Borrow { mutable, expr } => {
            let prefix = if *mutable { "&mut " } else { "&" };
            parenthesize(
                format!("{prefix}{}", render_expr(expr, PREC_UNARY)?),
                PREC_UNARY,
                parent_prec,
            )
        }
        ExprKind::Await { expr } => parenthesize(
            format!("await {}", render_expr(expr, PREC_UNARY)?),
            PREC_UNARY,
            parent_prec,
        ),
        ExprKind::Try { expr } => parenthesize(
            format!("{}?", render_expr(expr, PREC_POSTFIX)?),
            PREC_POSTFIX,
            parent_prec,
        ),
        ExprKind::StructInit { name, fields } => {
            let mut rendered_fields = Vec::with_capacity(fields.len());
            for (field, value, _) in fields {
                rendered_fields.push(format!("{field}: {}", render_expr(value, 0)?));
            }
            Some(format!("{name} {{ {} }}", rendered_fields.join(", ")))
        }
        ExprKind::If { .. }
        | ExprKind::While { .. }
        | ExprKind::Loop { .. }
        | ExprKind::Match { .. }
        | ExprKind::Closure { .. }
        | ExprKind::Break { .. }
        | ExprKind::Continue
        | ExprKind::UnsafeBlock { .. } => None,
    }
}

fn parenthesize(expr: String, precedence: u8, parent_prec: u8) -> Option<String> {
    if precedence < parent_prec {
        Some(format!("({expr})"))
    } else {
        Some(expr)
    }
}

fn format_float_literal(value: f64) -> String {
    let mut rendered = value.to_string();
    if !rendered.contains('.') && !rendered.contains('e') && !rendered.contains('E') {
        rendered.push_str(".0");
    }
    rendered
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::driver::{run_frontend_with_options, FrontendOptions};
    use crate::suggest_contracts::{analyze, format_text};

    #[test]
    fn suggest_contracts_infers_guards_and_parameter_postconditions() {
        let project = tempdir().expect("temp project");
        let source_path = project.path().join("main.aic");
        fs::write(
            &source_path,
            concat!(
                "module suggest.contracts;\n",
                "import std.io;\n",
                "fn bounded(i: Int, n: Int) -> Int {\n",
                "    if i >= 0 && i < n {\n",
                "        i\n",
                "    } else {\n",
                "        0\n",
                "    }\n",
                "}\n",
                "fn passthrough[T](x: T) -> T effects { io } {\n",
                "    print_int(1);\n",
                "    x\n",
                "}\n",
            ),
        )
        .expect("write source");

        let front = run_frontend_with_options(&source_path, FrontendOptions { offline: false })
            .expect("frontend");
        let first = analyze(&front);
        let second = analyze(&front);
        assert_eq!(first, second, "suggestions must be deterministic");

        let bounded = first
            .suggestions
            .iter()
            .find(|entry| entry.function == "bounded")
            .expect("bounded suggestion");
        let requires = bounded
            .suggested_requires
            .iter()
            .map(|clause| clause.expr.as_str())
            .collect::<Vec<_>>();
        assert!(requires.contains(&"i >= 0"));
        assert!(requires.contains(&"i < n"));

        let passthrough = first
            .suggestions
            .iter()
            .find(|entry| entry.function == "passthrough")
            .expect("passthrough suggestion");
        assert_eq!(passthrough.suggested_ensures.len(), 1);
        assert_eq!(passthrough.suggested_ensures[0].expr, "result == x");

        for suggestion in &first.suggestions {
            for clause in suggestion
                .suggested_requires
                .iter()
                .chain(suggestion.suggested_ensures.iter())
            {
                assert!(
                    (0.0..=1.0).contains(&clause.confidence),
                    "confidence must be within [0,1]: {clause:?}"
                );
            }
        }
    }

    #[test]
    fn suggest_contracts_text_mode_is_human_readable() {
        let project = tempdir().expect("temp project");
        let source_path = project.path().join("main.aic");
        fs::write(
            &source_path,
            concat!(
                "fn pick(flag: Bool, x: Int, y: Int) -> Int {\n",
                "    if flag {\n",
                "        x\n",
                "    } else {\n",
                "        y\n",
                "    }\n",
                "}\n",
            ),
        )
        .expect("write source");

        let front = run_frontend_with_options(&source_path, FrontendOptions { offline: false })
            .expect("frontend");
        let report = analyze(&front);
        let text = format_text(&report);
        assert!(text.contains("function pick"));
        assert!(text.contains("requires:"));
        assert!(text.contains("confidence="));
    }
}
