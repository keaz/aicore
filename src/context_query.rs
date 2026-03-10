use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;

use anyhow::bail;
use serde::Serialize;

use crate::ast;
use crate::driver::{run_frontend_with_options, FrontendOptions};
use crate::parser;
use crate::span::Span;
use crate::symbol_query::{self, SymbolKind, SymbolRecord};

const CONTEXT_PROTOCOL_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextTarget {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextDependency {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub relation: String,
    pub distance: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub effects: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ensures: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invariant: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextCaller {
    pub name: String,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub distance: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextContracts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ensures: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invariant: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextReport {
    pub protocol_version: String,
    pub phase: String,
    pub depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    pub signature: String,
    pub target: ContextTarget,
    pub dependencies: Vec<ContextDependency>,
    pub callers: Vec<ContextCaller>,
    pub contracts: ContextContracts,
    pub related_tests: Vec<String>,
}

#[derive(Debug, Clone)]
struct TargetSelector {
    kind: Option<SymbolKind>,
    name: String,
    module: Option<String>,
}

pub fn build_context_report(
    project_root: &Path,
    target_tokens: &[String],
    depth: usize,
    limit: Option<usize>,
) -> anyhow::Result<ContextReport> {
    if depth == 0 {
        bail!("--depth must be greater than 0");
    }

    let selector = parse_target_selector(target_tokens)?;
    let symbols = symbol_query::list_symbols(project_root)?;
    let target_symbol = select_target_symbol(&symbols, &selector)?;

    let front = run_frontend_with_options(project_root, FrontendOptions::default())?;
    let call_graph = merge_call_graphs(
        normalize_call_graph(&front.typecheck.call_graph),
        collect_syntactic_call_graph(project_root)?,
    );

    let type_dependency_names = collect_type_dependency_names(&target_symbol)?;
    let mut dependencies = Vec::new();

    for ty_name in type_dependency_names {
        for symbol in symbols.iter().filter(|symbol| {
            symbol.name == ty_name
                && matches!(
                    symbol.kind,
                    SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Trait
                )
        }) {
            dependencies.push(ContextDependency {
                name: symbol.name.clone(),
                kind: symbol.kind,
                signature: symbol.signature.clone(),
                module: symbol.module.clone(),
                relation: "signature_type".to_string(),
                distance: 1,
                effects: symbol.effects.clone(),
                capabilities: symbol.capabilities.clone(),
                requires: symbol.requires.clone(),
                ensures: symbol.ensures.clone(),
                invariant: symbol.invariant.clone(),
            });
        }
    }

    if target_symbol.kind == SymbolKind::Function {
        let call_distances = collect_call_distances(&call_graph, &target_symbol.name, depth);
        for (name, distance) in call_distances {
            for symbol in symbols
                .iter()
                .filter(|symbol| symbol.kind == SymbolKind::Function && symbol.name == name)
            {
                dependencies.push(ContextDependency {
                    name: symbol.name.clone(),
                    kind: symbol.kind,
                    signature: symbol.signature.clone(),
                    module: symbol.module.clone(),
                    relation: "call".to_string(),
                    distance,
                    effects: symbol.effects.clone(),
                    capabilities: symbol.capabilities.clone(),
                    requires: symbol.requires.clone(),
                    ensures: symbol.ensures.clone(),
                    invariant: symbol.invariant.clone(),
                });
            }
        }
    }

    dependencies.sort_by(|lhs, rhs| {
        lhs.distance
            .cmp(&rhs.distance)
            .then(relation_rank(&lhs.relation).cmp(&relation_rank(&rhs.relation)))
            .then(lhs.kind.as_str().cmp(rhs.kind.as_str()))
            .then(lhs.name.cmp(&rhs.name))
            .then(lhs.module.cmp(&rhs.module))
    });
    dependencies.dedup_by(|lhs, rhs| {
        lhs.distance == rhs.distance
            && lhs.relation == rhs.relation
            && lhs.kind == rhs.kind
            && lhs.name == rhs.name
            && lhs.module == rhs.module
    });

    let callers = if target_symbol.kind == SymbolKind::Function {
        let caller_distances = collect_caller_distances(&call_graph, &target_symbol.name, depth);
        let mut rows = Vec::new();
        for (name, distance) in caller_distances {
            for symbol in symbols
                .iter()
                .filter(|symbol| symbol.kind == SymbolKind::Function && symbol.name == name)
            {
                rows.push(ContextCaller {
                    name: symbol.name.clone(),
                    signature: symbol.signature.clone(),
                    module: symbol.module.clone(),
                    distance,
                });
            }
        }
        rows.sort_by(|lhs, rhs| {
            lhs.distance
                .cmp(&rhs.distance)
                .then(lhs.name.cmp(&rhs.name))
                .then(lhs.module.cmp(&rhs.module))
        });
        rows.dedup_by(|lhs, rhs| {
            lhs.distance == rhs.distance && lhs.name == rhs.name && lhs.module == rhs.module
        });
        rows
    } else {
        Vec::new()
    };

    let mut related_tests = collect_related_tests(&symbols, &target_symbol.name, &callers);
    truncate_if_needed(&mut dependencies, limit);
    truncate_if_needed(&mut related_tests, limit);
    let mut callers = callers;
    truncate_if_needed(&mut callers, limit);

    Ok(ContextReport {
        protocol_version: CONTEXT_PROTOCOL_VERSION.to_string(),
        phase: "context".to_string(),
        depth,
        limit,
        signature: target_symbol.signature.clone(),
        target: ContextTarget {
            name: target_symbol.name.clone(),
            kind: target_symbol.kind,
            signature: target_symbol.signature.clone(),
            module: target_symbol.module.clone(),
        },
        dependencies,
        callers,
        contracts: ContextContracts {
            requires: target_symbol.requires.clone(),
            ensures: target_symbol.ensures.clone(),
            invariant: target_symbol.invariant.clone(),
        },
        related_tests,
    })
}

pub fn format_context_report_text(report: &ContextReport) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "context target: {} {}",
        report.target.kind.as_str(),
        qualified_name(&report.target.module, &report.target.name)
    ));
    lines.push(format!("signature: {}", report.signature));
    lines.push(format!("depth: {}", report.depth));
    if let Some(limit) = report.limit {
        lines.push(format!("limit: {limit}"));
    }

    let has_contracts = report.contracts.requires.is_some()
        || report.contracts.ensures.is_some()
        || report.contracts.invariant.is_some();
    if has_contracts {
        lines.push("contracts:".to_string());
        if let Some(requires) = &report.contracts.requires {
            lines.push(format!("  requires: {requires}"));
        }
        if let Some(ensures) = &report.contracts.ensures {
            lines.push(format!("  ensures: {ensures}"));
        }
        if let Some(invariant) = &report.contracts.invariant {
            lines.push(format!("  invariant: {invariant}"));
        }
    }

    if report.dependencies.is_empty() {
        lines.push("dependencies: none".to_string());
    } else {
        lines.push(format!("dependencies ({}):", report.dependencies.len()));
        for dependency in &report.dependencies {
            lines.push(format!(
                "  [d{} {}] {} {}",
                dependency.distance,
                dependency.relation,
                dependency.kind.as_str(),
                qualified_name(&dependency.module, &dependency.name)
            ));
        }
    }

    if report.callers.is_empty() {
        lines.push("callers: none".to_string());
    } else {
        lines.push(format!("callers ({}):", report.callers.len()));
        for caller in &report.callers {
            lines.push(format!(
                "  [d{}] {}",
                caller.distance,
                qualified_name(&caller.module, &caller.name)
            ));
        }
    }

    if report.related_tests.is_empty() {
        lines.push("related_tests: none".to_string());
    } else {
        lines.push(format!("related_tests ({}):", report.related_tests.len()));
        for test_name in &report.related_tests {
            lines.push(format!("  {test_name}"));
        }
    }

    lines.join("\n")
}

fn parse_target_selector(tokens: &[String]) -> anyhow::Result<TargetSelector> {
    if tokens.is_empty() {
        bail!("--for requires a target selector");
    }

    let flattened = tokens.join(" ");
    let parts = flattened
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        bail!("--for requires a non-empty target selector");
    }

    let (kind, raw_name) = if let Some(kind) = parse_symbol_kind_label(&parts[0]) {
        if parts.len() < 2 {
            bail!("--for requires a symbol name after `{}`", parts[0]);
        }
        (Some(kind), parts[1..].join(" "))
    } else {
        (None, parts.join(" "))
    };

    let raw_name = raw_name.trim();
    if raw_name.is_empty() {
        bail!("--for requires a non-empty symbol name");
    }

    let (module, name) = split_module_and_name(raw_name);
    Ok(TargetSelector { kind, name, module })
}

fn parse_symbol_kind_label(raw: &str) -> Option<SymbolKind> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "function" | "fn" => Some(SymbolKind::Function),
        "struct" => Some(SymbolKind::Struct),
        "enum" => Some(SymbolKind::Enum),
        "variant" => Some(SymbolKind::Variant),
        "trait" => Some(SymbolKind::Trait),
        "impl" => Some(SymbolKind::Impl),
        "module" => Some(SymbolKind::Module),
        _ => None,
    }
}

fn split_module_and_name(raw: &str) -> (Option<String>, String) {
    if let Some((module, name)) = raw.rsplit_once("::") {
        let module = module.trim().replace("::", ".");
        let name = name.trim().to_string();
        if !module.is_empty() && !name.is_empty() {
            return (Some(module), name);
        }
    }
    if let Some((module, name)) = raw.rsplit_once('.') {
        let module = module.trim().to_string();
        let name = name.trim().to_string();
        if !module.is_empty() && !name.is_empty() {
            return (Some(module), name);
        }
    }
    (None, raw.trim().to_string())
}

fn select_target_symbol(
    symbols: &[SymbolRecord],
    selector: &TargetSelector,
) -> anyhow::Result<SymbolRecord> {
    let mut candidates = symbols
        .iter()
        .filter(|symbol| symbol_matches_selector(symbol, selector))
        .cloned()
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        bail!(
            "unknown context target `{}`",
            qualified_name(&selector.module, &selector.name)
        );
    }

    if selector.kind.is_none() {
        let function_candidates = candidates
            .iter()
            .filter(|symbol| symbol.kind == SymbolKind::Function)
            .cloned()
            .collect::<Vec<_>>();
        if function_candidates.len() == 1 {
            return Ok(function_candidates[0].clone());
        }
        if !function_candidates.is_empty() {
            candidates = function_candidates;
        }
    }

    candidates.sort_by(|lhs, rhs| {
        lhs.kind
            .as_str()
            .cmp(rhs.kind.as_str())
            .then(lhs.module.cmp(&rhs.module))
            .then(lhs.location.file.cmp(&rhs.location.file))
            .then(lhs.location.span_start.cmp(&rhs.location.span_start))
    });
    candidates.dedup_by(|lhs, rhs| {
        lhs.kind == rhs.kind
            && lhs.module == rhs.module
            && lhs.location.file == rhs.location.file
            && lhs.location.span_start == rhs.location.span_start
            && lhs.location.span_end == rhs.location.span_end
    });

    if candidates.len() > 1 {
        let options = candidates
            .iter()
            .map(|symbol| {
                format!(
                    "{} {}",
                    symbol.kind.as_str(),
                    qualified_name(&symbol.module, &symbol.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "ambiguous context target `{}`; candidates: {}",
            qualified_name(&selector.module, &selector.name),
            options
        );
    }

    Ok(candidates.remove(0))
}

fn symbol_matches_selector(symbol: &SymbolRecord, selector: &TargetSelector) -> bool {
    if symbol.name != selector.name {
        return false;
    }
    if let Some(kind) = selector.kind {
        if symbol.kind != kind {
            return false;
        }
    }
    match &selector.module {
        Some(module) => symbol.module.as_deref() == Some(module.as_str()),
        None => true,
    }
}

fn collect_call_distances(
    call_graph: &BTreeMap<String, Vec<String>>,
    target: &str,
    depth: usize,
) -> BTreeMap<String, usize> {
    let mut seen = BTreeMap::<String, usize>::new();
    let mut queue = VecDeque::<(String, usize)>::new();

    if let Some(callees) = call_graph.get(target) {
        for callee in callees {
            if callee == target {
                continue;
            }
            seen.entry(callee.clone()).or_insert(1);
        }
    }

    for (name, distance) in &seen {
        queue.push_back((name.clone(), *distance));
    }

    while let Some((name, distance)) = queue.pop_front() {
        if distance >= depth {
            continue;
        }
        if let Some(next) = call_graph.get(&name) {
            for callee in next {
                if callee == target {
                    continue;
                }
                let candidate_distance = distance + 1;
                let should_enqueue = match seen.get(callee) {
                    Some(existing) if *existing <= candidate_distance => false,
                    _ => true,
                };
                if should_enqueue {
                    seen.insert(callee.clone(), candidate_distance);
                    queue.push_back((callee.clone(), candidate_distance));
                }
            }
        }
    }

    seen
}

fn normalize_call_graph(raw: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, Vec<String>> {
    let mut normalized = BTreeMap::<String, BTreeSet<String>>::new();
    for (caller, callees) in raw {
        let caller_name = normalize_function_key(caller);
        for callee in callees {
            normalized
                .entry(caller_name.clone())
                .or_default()
                .insert(normalize_function_key(callee));
        }
        normalized.entry(caller_name).or_default();
    }

    normalized
        .into_iter()
        .map(|(caller, callees)| (caller, callees.into_iter().collect::<Vec<_>>()))
        .collect()
}

fn merge_call_graphs(
    lhs: BTreeMap<String, Vec<String>>,
    rhs: BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, Vec<String>> {
    let mut merged = BTreeMap::<String, BTreeSet<String>>::new();
    for (caller, callees) in lhs.into_iter().chain(rhs) {
        let entry = merged.entry(caller).or_default();
        for callee in callees {
            entry.insert(callee);
        }
    }
    merged
        .into_iter()
        .map(|(caller, callees)| (caller, callees.into_iter().collect()))
        .collect()
}

fn normalize_function_key(raw: &str) -> String {
    raw.rsplit_once("::")
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| raw.to_string())
}

fn collect_syntactic_call_graph(
    project_root: &Path,
) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let mut files = Vec::new();
    collect_context_aic_files(project_root, &mut files)?;
    files.sort();

    let mut graph = BTreeMap::<String, BTreeSet<String>>::new();
    for file in files {
        let source = match fs::read_to_string(&file) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let (program, diagnostics) = parser::parse(&source, &file.to_string_lossy());
        if diagnostics.iter().any(|diag| diag.is_error()) {
            continue;
        }
        let Some(program) = program else {
            continue;
        };

        for item in &program.items {
            match item {
                ast::Item::Function(function) => {
                    graph.entry(function.name.clone()).or_default();
                    collect_block_call_names(&function.body, &function.name, &mut graph);
                }
                ast::Item::Trait(trait_def) => {
                    for method in &trait_def.methods {
                        graph.entry(method.name.clone()).or_default();
                        collect_block_call_names(&method.body, &method.name, &mut graph);
                    }
                }
                ast::Item::Impl(impl_def) => {
                    for method in &impl_def.methods {
                        graph.entry(method.name.clone()).or_default();
                        collect_block_call_names(&method.body, &method.name, &mut graph);
                    }
                }
                ast::Item::Struct(_) | ast::Item::Enum(_) => {}
            }
        }
    }

    Ok(graph
        .into_iter()
        .map(|(caller, callees)| (caller, callees.into_iter().collect()))
        .collect())
}

fn collect_context_aic_files(root: &Path, out: &mut Vec<std::path::PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if matches!(
                path.file_name().and_then(|value| value.to_str()),
                Some(".git" | "target" | ".aic" | ".aic-cache")
            ) {
                continue;
            }
            collect_context_aic_files(&path, out)?;
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) == Some("aic") {
            out.push(path);
        }
    }
    Ok(())
}

fn collect_block_call_names(
    block: &ast::Block,
    caller_name: &str,
    call_graph: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let { expr, .. }
            | ast::Stmt::Assign { expr, .. }
            | ast::Stmt::Expr { expr, .. }
            | ast::Stmt::Assert { expr, .. } => {
                collect_expr_call_names(expr, caller_name, call_graph);
            }
            ast::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    collect_expr_call_names(expr, caller_name, call_graph);
                }
            }
        }
    }

    if let Some(tail) = &block.tail {
        collect_expr_call_names(tail, caller_name, call_graph);
    }
}

fn collect_expr_call_names(
    expr: &ast::Expr,
    caller_name: &str,
    call_graph: &mut BTreeMap<String, BTreeSet<String>>,
) {
    match &expr.kind {
        ast::ExprKind::Call { callee, args, .. } => {
            if let Some(callee_name) = extract_callee_name(callee) {
                call_graph
                    .entry(caller_name.to_string())
                    .or_default()
                    .insert(callee_name);
            }
            collect_expr_call_names(callee, caller_name, call_graph);
            for arg in args {
                collect_expr_call_names(arg, caller_name, call_graph);
            }
        }
        ast::ExprKind::Closure { body, .. } => {
            collect_block_call_names(body, caller_name, call_graph);
        }
        ast::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_expr_call_names(cond, caller_name, call_graph);
            collect_block_call_names(then_block, caller_name, call_graph);
            collect_block_call_names(else_block, caller_name, call_graph);
        }
        ast::ExprKind::While { cond, body } => {
            collect_expr_call_names(cond, caller_name, call_graph);
            collect_block_call_names(body, caller_name, call_graph);
        }
        ast::ExprKind::Loop { body } => {
            collect_block_call_names(body, caller_name, call_graph);
        }
        ast::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                collect_expr_call_names(expr, caller_name, call_graph);
            }
        }
        ast::ExprKind::Continue => {}
        ast::ExprKind::Match { expr, arms } => {
            collect_expr_call_names(expr, caller_name, call_graph);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_expr_call_names(guard, caller_name, call_graph);
                }
                collect_expr_call_names(&arm.body, caller_name, call_graph);
            }
        }
        ast::ExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_call_names(lhs, caller_name, call_graph);
            collect_expr_call_names(rhs, caller_name, call_graph);
        }
        ast::ExprKind::Unary { expr, .. }
        | ast::ExprKind::Borrow { expr, .. }
        | ast::ExprKind::Await { expr }
        | ast::ExprKind::Try { expr } => {
            collect_expr_call_names(expr, caller_name, call_graph);
        }
        ast::ExprKind::UnsafeBlock { block } => {
            collect_block_call_names(block, caller_name, call_graph);
        }
        ast::ExprKind::StructInit { fields, .. } => {
            for (_, field_expr, _) in fields {
                collect_expr_call_names(field_expr, caller_name, call_graph);
            }
        }
        ast::ExprKind::FieldAccess { base, .. } => {
            collect_expr_call_names(base, caller_name, call_graph);
        }
        ast::ExprKind::Int(_)
        | ast::ExprKind::Float(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::Char(_)
        | ast::ExprKind::String(_)
        | ast::ExprKind::Unit
        | ast::ExprKind::Var(_) => {}
    }
}

fn extract_callee_name(expr: &ast::Expr) -> Option<String> {
    let mut segments = Vec::new();
    if !collect_expr_path_segments(expr, &mut segments) {
        return None;
    }
    segments.pop()
}

fn collect_expr_path_segments(expr: &ast::Expr, out: &mut Vec<String>) -> bool {
    match &expr.kind {
        ast::ExprKind::Var(name) => {
            out.push(name.clone());
            true
        }
        ast::ExprKind::FieldAccess { base, field } => {
            if collect_expr_path_segments(base, out) {
                out.push(field.clone());
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

fn collect_caller_distances(
    call_graph: &BTreeMap<String, Vec<String>>,
    target: &str,
    depth: usize,
) -> BTreeMap<String, usize> {
    let mut inverse = BTreeMap::<String, BTreeSet<String>>::new();
    for (caller, callees) in call_graph {
        for callee in callees {
            inverse
                .entry(callee.clone())
                .or_default()
                .insert(caller.clone());
        }
    }

    let mut seen = BTreeMap::<String, usize>::new();
    let mut queue = VecDeque::<(String, usize)>::new();

    if let Some(callers) = inverse.get(target) {
        for caller in callers {
            if caller == target {
                continue;
            }
            seen.entry(caller.clone()).or_insert(1);
        }
    }
    for (name, distance) in &seen {
        queue.push_back((name.clone(), *distance));
    }

    while let Some((name, distance)) = queue.pop_front() {
        if distance >= depth {
            continue;
        }
        if let Some(next) = inverse.get(&name) {
            for caller in next {
                if caller == target {
                    continue;
                }
                let candidate_distance = distance + 1;
                let should_enqueue = match seen.get(caller) {
                    Some(existing) if *existing <= candidate_distance => false,
                    _ => true,
                };
                if should_enqueue {
                    seen.insert(caller.clone(), candidate_distance);
                    queue.push_back((caller.clone(), candidate_distance));
                }
            }
        }
    }

    seen
}

fn collect_type_dependency_names(target: &SymbolRecord) -> anyhow::Result<BTreeSet<String>> {
    let source = fs::read_to_string(&target.location.file)?;
    let (program, diagnostics) = parser::parse(&source, &target.location.file);
    if diagnostics.iter().any(|diag| diag.is_error()) {
        return Ok(BTreeSet::new());
    }

    let Some(program) = program else {
        return Ok(BTreeSet::new());
    };

    let mut names = BTreeSet::new();
    let mut generics = BTreeSet::new();

    for item in &program.items {
        match item {
            ast::Item::Function(function) => {
                if target.kind == SymbolKind::Function
                    && span_matches(target, function.span)
                    && target.name == function.name
                {
                    collect_type_names_for_function(function, &mut names, &mut generics);
                }
            }
            ast::Item::Struct(strukt) => {
                if target.kind == SymbolKind::Struct
                    && span_matches(target, strukt.span)
                    && target.name == strukt.name
                {
                    generics.extend(strukt.generics.iter().map(|param| param.name.clone()));
                    for field in &strukt.fields {
                        collect_type_names_from_type_expr(&field.ty, &mut names);
                    }
                }
            }
            ast::Item::Enum(enm) => {
                if target.kind == SymbolKind::Enum
                    && span_matches(target, enm.span)
                    && target.name == enm.name
                {
                    generics.extend(enm.generics.iter().map(|param| param.name.clone()));
                    for variant in &enm.variants {
                        if let Some(payload) = &variant.payload {
                            collect_type_names_from_type_expr(payload, &mut names);
                        }
                    }
                }
                if target.kind == SymbolKind::Variant
                    && target.container.as_deref() == Some(&enm.name)
                {
                    for variant in &enm.variants {
                        if target.name == variant.name && span_matches(target, variant.span) {
                            if let Some(payload) = &variant.payload {
                                collect_type_names_from_type_expr(payload, &mut names);
                            }
                        }
                    }
                }
            }
            ast::Item::Trait(trait_def) => {
                if target.kind == SymbolKind::Trait
                    && span_matches(target, trait_def.span)
                    && target.name == trait_def.name
                {
                    generics.extend(trait_def.generics.iter().map(|param| param.name.clone()));
                    for method in &trait_def.methods {
                        collect_type_names_for_function(method, &mut names, &mut generics);
                    }
                }
                if target.kind == SymbolKind::Function {
                    for method in &trait_def.methods {
                        if span_matches(target, method.span) && target.name == method.name {
                            collect_type_names_for_function(method, &mut names, &mut generics);
                        }
                    }
                }
            }
            ast::Item::Impl(impl_def) => {
                if target.kind == SymbolKind::Impl
                    && span_matches(target, impl_def.span)
                    && target.name == impl_name(impl_def)
                {
                    for arg in &impl_def.trait_args {
                        collect_type_names_from_type_expr(arg, &mut names);
                    }
                    if let Some(target_ty) = &impl_def.target {
                        collect_type_names_from_type_expr(target_ty, &mut names);
                    }
                    for method in &impl_def.methods {
                        collect_type_names_for_function(method, &mut names, &mut generics);
                    }
                }
                if target.kind == SymbolKind::Function {
                    for method in &impl_def.methods {
                        if span_matches(target, method.span) && target.name == method.name {
                            collect_type_names_for_function(method, &mut names, &mut generics);
                        }
                    }
                }
            }
        }
    }

    for generic in generics {
        names.remove(&generic);
    }

    Ok(names)
}

fn collect_type_names_for_function(
    function: &ast::Function,
    names: &mut BTreeSet<String>,
    generics: &mut BTreeSet<String>,
) {
    generics.extend(function.generics.iter().map(|param| param.name.clone()));
    for param in &function.params {
        collect_type_names_from_type_expr(&param.ty, names);
    }
    collect_type_names_from_type_expr(&function.ret_type, names);
}

fn collect_type_names_from_type_expr(ty: &ast::TypeExpr, out: &mut BTreeSet<String>) {
    match &ty.kind {
        ast::TypeKind::Unit | ast::TypeKind::Hole => {}
        ast::TypeKind::DynTrait { trait_name } => {
            out.insert(trait_name.clone());
        }
        ast::TypeKind::Named { name, args } => {
            out.insert(name.clone());
            for arg in args {
                collect_type_names_from_type_expr(arg, out);
            }
        }
    }
}

fn span_matches(target: &SymbolRecord, span: Span) -> bool {
    target.location.span_start == span.start && target.location.span_end == span.end
}

fn impl_name(impl_def: &ast::ImplDef) -> String {
    if impl_def.is_inherent {
        impl_def
            .target
            .as_ref()
            .and_then(type_expr_named_root)
            .unwrap_or_else(|| impl_def.trait_name.clone())
    } else {
        impl_def.trait_name.clone()
    }
}

fn type_expr_named_root(ty: &ast::TypeExpr) -> Option<String> {
    match &ty.kind {
        ast::TypeKind::Named { name, .. } => Some(name.clone()),
        ast::TypeKind::Unit | ast::TypeKind::DynTrait { .. } | ast::TypeKind::Hole => None,
    }
}

fn collect_related_tests(
    symbols: &[SymbolRecord],
    target_name: &str,
    callers: &[ContextCaller],
) -> Vec<String> {
    let caller_names = callers
        .iter()
        .map(|caller| caller.name.clone())
        .collect::<BTreeSet<_>>();

    let mut tests = symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Function)
        .filter(|symbol| {
            if !is_test_symbol(symbol) {
                return false;
            }
            caller_names.contains(&symbol.name) || symbol.name.contains(target_name)
        })
        .map(|symbol| qualified_name(&symbol.module, &symbol.name))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    tests.sort();
    tests
}

fn is_test_symbol(symbol: &SymbolRecord) -> bool {
    if symbol.name.starts_with("test_") || symbol.name.ends_with("_test") {
        return true;
    }
    symbol
        .module
        .as_deref()
        .map(module_is_test_like)
        .unwrap_or(false)
}

fn module_is_test_like(module: &str) -> bool {
    module
        .split('.')
        .any(|segment| matches!(segment, "test" | "tests" | "spec" | "specs" | "harness"))
}

fn qualified_name(module: &Option<String>, name: &str) -> String {
    match module {
        Some(module) => format!("{module}.{name}"),
        None => name.to_string(),
    }
}

fn relation_rank(relation: &str) -> usize {
    match relation {
        "signature_type" => 0,
        "call" => 1,
        _ => 2,
    }
}

fn truncate_if_needed<T>(items: &mut Vec<T>, limit: Option<usize>) {
    if let Some(limit) = limit {
        items.truncate(limit);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::build_context_report;

    fn write_context_fixture(root: &std::path::Path) {
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(
            root.join("aic.toml"),
            "[package]\nname = \"context_fixture\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write aic.toml");
        fs::write(
            root.join("src/models.aic"),
            concat!(
                "module demo.context.models;\n",
                "pub struct User {\n",
                "    pub age: Int,\n",
                "} invariant age >= 0\n",
            ),
        )
        .expect("write models");
        fs::write(
            root.join("src/validators.aic"),
            concat!(
                "module demo.context.validators;\n",
                "import demo.context.models;\n",
                "\n",
                "pub fn normalize_age(age: Int) -> Int requires age >= 0 ensures result >= 0 {\n",
                "    age\n",
                "}\n",
                "\n",
                "pub fn validate_user(user: User) -> Bool requires user.age >= 0 ensures result == true {\n",
                "    normalize_age(user.age) >= 0\n",
                "}\n",
            ),
        )
        .expect("write validators");
        fs::write(
            root.join("src/workflow.aic"),
            concat!(
                "module demo.context.workflow;\n",
                "import demo.context.models;\n",
                "import demo.context.validators;\n",
                "\n",
                "pub enum AppError {\n",
                "    InvalidInput,\n",
                "}\n",
                "\n",
                "pub fn process_user(user: User) -> Result[Int, AppError] requires user.age >= 0 ensures true {\n",
                "    if validate_user(user) {\n",
                "        Ok(1)\n",
                "    } else {\n",
                "        Err(InvalidInput())\n",
                "    }\n",
                "}\n",
            ),
        )
        .expect("write workflow");
        fs::write(
            root.join("src/main.aic"),
            concat!(
                "module demo.context.app;\n",
                "import demo.context.models;\n",
                "import demo.context.workflow;\n",
                "\n",
                "fn orchestrate() -> Int {\n",
                "    match process_user(User { age: 1 }) {\n",
                "        Ok(v) => v,\n",
                "        Err(_) => 0,\n",
                "    }\n",
                "}\n",
                "\n",
                "fn main() -> Int {\n",
                "    orchestrate()\n",
                "}\n",
            ),
        )
        .expect("write main");
        fs::write(
            root.join("src/tests_support.aic"),
            concat!(
                "module demo.context.tests;\n",
                "import demo.context.models;\n",
                "import demo.context.workflow;\n",
                "\n",
                "fn test_process_user_ok() -> Int {\n",
                "    match process_user(User { age: 1 }) {\n",
                "        Ok(v) => v,\n",
                "        Err(_) => 0,\n",
                "    }\n",
                "}\n",
            ),
        )
        .expect("write tests");
    }

    #[test]
    fn context_report_collects_dependencies_and_callers() {
        let project = tempdir().expect("tempdir");
        write_context_fixture(project.path());

        let report = build_context_report(
            project.path(),
            &["function".to_string(), "process_user".to_string()],
            2,
            None,
        )
        .expect("context report");

        assert_eq!(report.target.name, "process_user");
        assert!(report.signature.contains("fn process_user"));
        assert!(
            report
                .dependencies
                .iter()
                .any(|dependency| dependency.name == "User"
                    && dependency.relation == "signature_type")
        );
        assert!(report
            .dependencies
            .iter()
            .any(|dependency| dependency.name == "validate_user" && dependency.relation == "call"));
        assert!(report
            .dependencies
            .iter()
            .any(|dependency| dependency.name == "normalize_age"
                && dependency.relation == "call"
                && dependency.distance == 2));
        assert!(report
            .callers
            .iter()
            .any(|caller| caller.name == "orchestrate" && caller.distance == 1));
        assert!(report
            .callers
            .iter()
            .any(|caller| caller.name == "test_process_user_ok" && caller.distance == 1));
        assert!(report
            .related_tests
            .iter()
            .any(|name| name.ends_with(".test_process_user_ok")));
    }

    #[test]
    fn context_report_rejects_ambiguous_targets() {
        let project = tempdir().expect("tempdir");
        fs::create_dir_all(project.path().join("src")).expect("mkdir src");
        fs::write(
            project.path().join("src/main.aic"),
            concat!(
                "module demo.main;\n",
                "fn duplicate() -> Int {\n",
                "    0\n",
                "}\n",
            ),
        )
        .expect("write source");
        fs::write(
            project.path().join("src/other.aic"),
            concat!(
                "module demo.other;\n",
                "fn duplicate() -> Int {\n",
                "    1\n",
                "}\n",
            ),
        )
        .expect("write secondary source");

        let err = build_context_report(project.path(), &["duplicate".to_string()], 1, None)
            .expect_err("ambiguous target should fail");
        assert!(
            err.to_string().contains("ambiguous"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn context_report_applies_limit_after_deterministic_sort() {
        let project = tempdir().expect("tempdir");
        write_context_fixture(project.path());

        let report = build_context_report(
            project.path(),
            &["function".to_string(), "process_user".to_string()],
            2,
            Some(2),
        )
        .expect("context report");

        assert_eq!(report.limit, Some(2));
        assert_eq!(report.dependencies.len(), 2);
        assert_eq!(report.callers.len(), 2);
        assert_eq!(report.related_tests.len(), 1);
        assert_eq!(report.dependencies[0].name, "AppError");
        assert_eq!(report.dependencies[1].name, "User");
    }
}
