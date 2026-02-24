use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::ast::{BinOp, Block, Expr, ExprKind, Function, Item, Stmt};
use crate::parser;
use crate::span::Span;

pub const METRICS_REPORT_SCHEMA_VERSION: &str = "1.0";
pub const DEFAULT_MAX_CYCLOMATIC: u32 = 15;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MetricsThresholds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cyclomatic: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cognitive: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lines: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_params: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_nesting_depth: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MetricsThresholdOverrides {
    pub max_cyclomatic: Option<u32>,
    pub max_cognitive: Option<u32>,
    pub max_lines: Option<u32>,
    pub max_params: Option<u32>,
    pub max_nesting_depth: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FunctionMetrics {
    pub name: String,
    pub cyclomatic_complexity: u32,
    pub cognitive_complexity: u32,
    pub lines: usize,
    pub params: usize,
    pub effects: Vec<String>,
    pub max_nesting_depth: u32,
    pub rating: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricsViolation {
    pub function: String,
    pub metric: String,
    pub actual: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricsCheckResult {
    pub passed: bool,
    pub thresholds: MetricsThresholds,
    pub violations: Vec<MetricsViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricsReport {
    pub phase: String,
    pub schema_version: String,
    pub input: String,
    pub functions: Vec<FunctionMetrics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<MetricsCheckResult>,
}

pub fn build_report(input: &Path) -> anyhow::Result<MetricsReport> {
    let source = fs::read_to_string(input)
        .with_context(|| format!("failed to read metrics input '{}'", input.display()))?;
    let file_name = input.to_string_lossy().to_string();
    let (program, _diagnostics) = parser::parse(&source, &file_name);
    let line_index = LineIndex::new(&source);

    let mut functions = Vec::new();
    if let Some(program) = program {
        let mut analyzed = collect_functions(&program.items)
            .into_iter()
            .map(|function| (function.span.start, analyze_function(function, &line_index)))
            .collect::<Vec<_>>();

        analyzed.sort_by(|(left_start, left), (right_start, right)| {
            left.name
                .cmp(&right.name)
                .then_with(|| left_start.cmp(right_start))
        });
        functions = analyzed.into_iter().map(|(_, metrics)| metrics).collect();
    }

    Ok(MetricsReport {
        phase: "metrics".to_string(),
        schema_version: METRICS_REPORT_SCHEMA_VERSION.to_string(),
        input: render_path(input),
        functions,
        check: None,
    })
}

pub fn resolve_thresholds(
    mut config: MetricsThresholds,
    overrides: MetricsThresholdOverrides,
) -> MetricsThresholds {
    if let Some(value) = overrides.max_cyclomatic {
        config.max_cyclomatic = Some(value);
    }
    if let Some(value) = overrides.max_cognitive {
        config.max_cognitive = Some(value);
    }
    if let Some(value) = overrides.max_lines {
        config.max_lines = Some(value);
    }
    if let Some(value) = overrides.max_params {
        config.max_params = Some(value);
    }
    if let Some(value) = overrides.max_nesting_depth {
        config.max_nesting_depth = Some(value);
    }
    if config.max_cyclomatic.is_none() {
        config.max_cyclomatic = Some(DEFAULT_MAX_CYCLOMATIC);
    }
    config
}

pub fn apply_thresholds(report: &mut MetricsReport, thresholds: MetricsThresholds) {
    let mut violations = Vec::new();
    for function in &report.functions {
        push_violation(
            &mut violations,
            &function.name,
            "cyclomatic_complexity",
            function.cyclomatic_complexity,
            thresholds.max_cyclomatic,
        );
        push_violation(
            &mut violations,
            &function.name,
            "cognitive_complexity",
            function.cognitive_complexity,
            thresholds.max_cognitive,
        );
        push_violation(
            &mut violations,
            &function.name,
            "lines",
            saturating_to_u32(function.lines),
            thresholds.max_lines,
        );
        push_violation(
            &mut violations,
            &function.name,
            "params",
            saturating_to_u32(function.params),
            thresholds.max_params,
        );
        push_violation(
            &mut violations,
            &function.name,
            "max_nesting_depth",
            function.max_nesting_depth,
            thresholds.max_nesting_depth,
        );
    }
    violations.sort_by(|left, right| {
        left.function
            .cmp(&right.function)
            .then_with(|| left.metric.cmp(&right.metric))
    });

    report.check = Some(MetricsCheckResult {
        passed: violations.is_empty(),
        thresholds,
        violations,
    });
}

fn push_violation(
    out: &mut Vec<MetricsViolation>,
    function: &str,
    metric: &str,
    actual: u32,
    max: Option<u32>,
) {
    if let Some(max) = max {
        if actual > max {
            out.push(MetricsViolation {
                function: function.to_string(),
                metric: metric.to_string(),
                actual,
                max,
            });
        }
    }
}

fn collect_functions(items: &[Item]) -> Vec<&Function> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Function(function) => out.push(function),
            Item::Trait(def) => out.extend(def.methods.iter()),
            Item::Impl(def) => out.extend(def.methods.iter()),
            Item::Struct(_) | Item::Enum(_) => {}
        }
    }
    out
}

fn analyze_function(function: &Function, line_index: &LineIndex) -> FunctionMetrics {
    let mut analyzer = ComplexityAnalyzer::new();
    analyzer.visit_block(&function.body, 0);
    if let Some(requires) = &function.requires {
        analyzer.visit_expr(requires, 0);
    }
    if let Some(ensures) = &function.ensures {
        analyzer.visit_expr(ensures, 0);
    }

    FunctionMetrics {
        name: function.name.clone(),
        cyclomatic_complexity: analyzer.cyclomatic,
        cognitive_complexity: analyzer.cognitive,
        lines: line_index.lines_for_span(function.span),
        params: function.params.len(),
        effects: sorted_unique_effects(function),
        max_nesting_depth: analyzer.max_nesting_depth,
        rating: rating_for_metrics(
            analyzer.cyclomatic,
            analyzer.cognitive,
            analyzer.max_nesting_depth,
        )
        .to_string(),
    }
}

fn sorted_unique_effects(function: &Function) -> Vec<String> {
    let mut effects = function.effects.clone();
    effects.sort();
    effects.dedup();
    effects
}

fn rating_for_metrics(cyclomatic: u32, cognitive: u32, nesting: u32) -> &'static str {
    if cyclomatic <= 5 && cognitive <= 8 && nesting <= 1 {
        "A"
    } else if cyclomatic <= 10 && cognitive <= 15 && nesting <= 2 {
        "B"
    } else if cyclomatic <= 15 && cognitive <= 25 && nesting <= 3 {
        "C"
    } else if cyclomatic <= 25 && cognitive <= 40 && nesting <= 5 {
        "D"
    } else {
        "F"
    }
}

fn render_path(path: &Path) -> String {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(relative) = canonical.strip_prefix(&cwd) {
            return relative.to_string_lossy().replace('\\', "/");
        }
    }
    canonical.to_string_lossy().replace('\\', "/")
}

fn saturating_to_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

#[derive(Debug)]
struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (idx, ch) in source.char_indices() {
            if ch == '\n' {
                starts.push(idx + 1);
            }
        }
        Self {
            starts,
            len: source.len(),
        }
    }

    fn line_for_offset(&self, offset: usize) -> usize {
        let clamped = offset.min(self.len);
        match self.starts.binary_search(&clamped) {
            Ok(index) => index + 1,
            Err(index) => index.max(1),
        }
    }

    fn lines_for_span(&self, span: Span) -> usize {
        let start = self.line_for_offset(span.start);
        let end_offset = if span.end > span.start {
            span.end.saturating_sub(1)
        } else {
            span.end
        };
        let end = self.line_for_offset(end_offset);
        end.saturating_sub(start) + 1
    }
}

#[derive(Debug)]
struct ComplexityAnalyzer {
    cyclomatic: u32,
    cognitive: u32,
    max_nesting_depth: u32,
}

impl ComplexityAnalyzer {
    fn new() -> Self {
        Self {
            cyclomatic: 1,
            cognitive: 0,
            max_nesting_depth: 0,
        }
    }

    fn visit_block(&mut self, block: &Block, depth: u32) {
        for stmt in &block.stmts {
            self.visit_stmt(stmt, depth);
        }
        if let Some(tail) = &block.tail {
            self.visit_expr(tail, depth);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt, depth: u32) {
        match stmt {
            Stmt::Let { expr, .. } => self.visit_expr(expr, depth),
            Stmt::Assign { expr, .. } => self.visit_expr(expr, depth),
            Stmt::Expr { expr, .. } => self.visit_expr(expr, depth),
            Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    self.visit_expr(expr, depth);
                }
            }
            Stmt::Assert { expr, .. } => self.visit_expr(expr, depth),
        }
    }

    fn visit_expr(&mut self, expr: &Expr, depth: u32) {
        match &expr.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::String(_)
            | ExprKind::Unit
            | ExprKind::Var(_)
            | ExprKind::Continue => {}
            ExprKind::Call { callee, args } => {
                self.visit_expr(callee, depth);
                for arg in args {
                    self.visit_expr(arg, depth);
                }
            }
            ExprKind::Closure { body, .. } => {
                self.visit_block(body, depth);
            }
            ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                self.bump_decision(depth);
                self.visit_expr(cond, depth);
                self.visit_block(then_block, depth + 1);
                self.visit_block(else_block, depth + 1);
            }
            ExprKind::While { cond, body } => {
                self.bump_decision(depth);
                self.visit_expr(cond, depth);
                self.visit_block(body, depth + 1);
            }
            ExprKind::Loop { body } => {
                self.bump_decision(depth);
                self.visit_block(body, depth + 1);
            }
            ExprKind::Break { expr } => {
                if let Some(expr) = expr {
                    self.visit_expr(expr, depth);
                }
            }
            ExprKind::Match { expr, arms } => {
                self.bump_decision(depth);
                self.visit_expr(expr, depth);
                for arm in arms {
                    self.bump_decision(depth);
                    if let Some(guard) = &arm.guard {
                        self.bump_decision(depth + 1);
                        self.visit_expr(guard, depth + 1);
                    }
                    self.visit_expr(&arm.body, depth + 1);
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                if matches!(op, BinOp::And | BinOp::Or) {
                    self.bump_decision(depth);
                }
                self.visit_expr(lhs, depth);
                self.visit_expr(rhs, depth);
            }
            ExprKind::Unary { expr, .. } => self.visit_expr(expr, depth),
            ExprKind::Borrow { expr, .. } => self.visit_expr(expr, depth),
            ExprKind::Await { expr } => self.visit_expr(expr, depth),
            ExprKind::Try { expr } => self.visit_expr(expr, depth),
            ExprKind::UnsafeBlock { block } => self.visit_block(block, depth),
            ExprKind::StructInit { fields, .. } => {
                for (_, value, _) in fields {
                    self.visit_expr(value, depth);
                }
            }
            ExprKind::FieldAccess { base, .. } => self.visit_expr(base, depth),
        }
    }

    fn bump_decision(&mut self, depth: u32) {
        self.cyclomatic = self.cyclomatic.saturating_add(1);
        self.cognitive = self.cognitive.saturating_add(1 + depth);
        self.max_nesting_depth = self.max_nesting_depth.max(depth + 1);
    }
}
