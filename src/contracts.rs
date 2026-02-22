use std::collections::BTreeMap;

use crate::ast::{BinOp, UnaryOp};
use crate::diagnostics::{Diagnostic, Severity};
use crate::ir;

type RangeEnv = BTreeMap<String, IntRange>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProofState {
    True,
    False,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IntRange {
    lower: Option<i64>,
    upper: Option<i64>,
}

impl IntRange {
    fn unbounded() -> Self {
        Self {
            lower: None,
            upper: None,
        }
    }

    fn singleton(value: i64) -> Self {
        Self {
            lower: Some(value),
            upper: Some(value),
        }
    }

    fn add(self, other: Self) -> Self {
        Self {
            lower: checked_add_bound(self.lower, other.lower),
            upper: checked_add_bound(self.upper, other.upper),
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            lower: checked_sub_bound(self.lower, other.upper),
            upper: checked_sub_bound(self.upper, other.lower),
        }
    }

    fn neg(self) -> Self {
        Self {
            lower: self.upper.and_then(|v| v.checked_neg()),
            upper: self.lower.and_then(|v| v.checked_neg()),
        }
    }
}

#[derive(Debug, Clone)]
struct TailPath {
    env: RangeEnv,
    result_expr: Option<ir::Expr>,
}

#[derive(Debug, Clone)]
struct InvariantSpec {
    name: String,
    span: crate::span::Span,
    generics: Vec<ir::GenericParam>,
    fields: Vec<ir::Field>,
    invariant: ir::Expr,
}

pub fn verify_static(program: &ir::Program, file: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for item in &program.items {
        match item {
            ir::Item::Function(func) => {
                let param_env = unconstrained_param_env(func);
                if let Some(req) = &func.requires {
                    match prove_expression(req, &param_env, None) {
                        ProofState::False => diagnostics.push(
                            Diagnostic::error(
                                "E4001",
                                format!("requires contract for '{}' is always false", func.name),
                                file,
                                req.span,
                            )
                            .with_help("fix the contract or function preconditions"),
                        ),
                        ProofState::True => diagnostics.push(discharge_note(
                            file,
                            req.span,
                            format!(
                                "requires contract for '{}' discharged at compile time",
                                func.name
                            ),
                        )),
                        ProofState::Unknown => diagnostics.push(residual_note(
                            file,
                            req.span,
                            format!(
                                "requires contract for '{}' kept as residual runtime obligation",
                                func.name
                            ),
                        )),
                    }
                }

                if let Some(ens) = &func.ensures {
                    match prove_ensures(func, ens, &param_env) {
                        ProofState::False => diagnostics.push(
                            Diagnostic::error(
                                "E4002",
                                format!("ensures contract for '{}' is always false", func.name),
                                file,
                                ens.span,
                            )
                            .with_help("fix the postcondition expression"),
                        ),
                        ProofState::True => diagnostics.push(discharge_note(
                            file,
                            ens.span,
                            format!(
                                "ensures contract for '{}' discharged at compile time",
                                func.name
                            ),
                        )),
                        ProofState::Unknown => diagnostics.push(residual_note(
                            file,
                            ens.span,
                            format!(
                                "ensures contract for '{}' kept as residual runtime obligation",
                                func.name
                            ),
                        )),
                    }
                }
            }
            ir::Item::Struct(strukt) => {
                if let Some(inv) = &strukt.invariant {
                    let field_env = unconstrained_field_env(strukt);
                    match prove_expression(inv, &field_env, None) {
                        ProofState::False => diagnostics.push(
                            Diagnostic::error(
                                "E4004",
                                format!("invariant for struct '{}' is always false", strukt.name),
                                file,
                                inv.span,
                            )
                            .with_help("fix the invariant expression"),
                        ),
                        ProofState::True => diagnostics.push(discharge_note(
                            file,
                            inv.span,
                            format!(
                                "invariant for struct '{}' discharged at compile time",
                                strukt.name
                            ),
                        )),
                        ProofState::Unknown => diagnostics.push(residual_note(
                            file,
                            inv.span,
                            format!(
                                "invariant for struct '{}' kept as residual runtime obligation",
                                strukt.name
                            ),
                        )),
                    }
                }
            }
            ir::Item::Enum(_) | ir::Item::Trait(_) | ir::Item::Impl(_) => {}
        }
    }

    diagnostics
}

pub fn lower_runtime_asserts(program: &ir::Program) -> ir::Program {
    let mut lowered = program.clone();
    let mut alloc = IdAlloc::from_program(&lowered);

    let invariant_specs = collect_invariant_specs(&lowered);
    let helper_names = invariant_specs
        .keys()
        .map(|name| (name.clone(), invariant_helper_name(name)))
        .collect::<BTreeMap<_, _>>();
    let field_orders = invariant_specs
        .iter()
        .map(|(name, spec)| {
            (
                name.clone(),
                spec.fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    for item in &mut lowered.items {
        let ir::Item::Function(func) = item else {
            continue;
        };

        if func.is_extern {
            continue;
        }

        let param_env = unconstrained_param_env(func);
        let requires_discharged = func
            .requires
            .as_ref()
            .map(|req| prove_expression(req, &param_env, None) == ProofState::True)
            .unwrap_or(false);
        let ensures_discharged = func
            .ensures
            .as_ref()
            .map(|ens| prove_ensures(func, ens, &param_env) == ProofState::True)
            .unwrap_or(false);

        rewrite_struct_inits_in_block(&mut func.body, &helper_names, &field_orders, &mut alloc);

        if let Some(req) = &func.requires {
            if !requires_discharged {
                let req_clone = clone_expr(req, &mut alloc);
                let stmt = ir::Stmt::Assert {
                    expr: req_clone,
                    message: contract_message("requires", &func.name),
                    span: func.span,
                };
                let mut stmts = vec![stmt];
                stmts.extend(std::mem::take(&mut func.body.stmts));
                func.body.stmts = stmts;
            }
        }

        if let Some(ens) = func.ensures.clone() {
            if !ensures_discharged {
                lower_ensures_in_block(
                    &mut func.body,
                    &ens,
                    func.ret_type,
                    &func.name,
                    &mut alloc,
                    true,
                );
            }
        }
    }

    let helper_functions =
        build_invariant_helpers(&mut lowered, &invariant_specs, &helper_names, &mut alloc);
    for helper in helper_functions {
        lowered.items.push(ir::Item::Function(helper));
    }

    lowered
}

fn discharge_note(file: &str, span: crate::span::Span, message: String) -> Diagnostic {
    let mut diag = Diagnostic::error("E4005", message, file, span);
    diag.severity = Severity::Note;
    diag
}

fn residual_note(file: &str, span: crate::span::Span, message: String) -> Diagnostic {
    let mut diag = Diagnostic::error("E4003", message, file, span);
    diag.severity = Severity::Note;
    diag
}

fn contract_message(kind: &str, owner: &str) -> String {
    format!("contract_violation{{kind:{kind},owner:{owner}}}: {kind} failed in {owner}")
}

fn unconstrained_param_env(func: &ir::Function) -> RangeEnv {
    func.params
        .iter()
        .map(|param| (param.name.clone(), IntRange::unbounded()))
        .collect::<RangeEnv>()
}

fn unconstrained_field_env(strukt: &ir::StructDef) -> RangeEnv {
    strukt
        .fields
        .iter()
        .map(|field| (field.name.clone(), IntRange::unbounded()))
        .collect::<RangeEnv>()
}

fn prove_ensures(func: &ir::Function, ensures: &ir::Expr, env: &RangeEnv) -> ProofState {
    let direct = prove_expression(ensures, env, None);
    if direct != ProofState::Unknown {
        return direct;
    }

    let Some(paths) = collect_tail_paths(&func.body, env) else {
        return ProofState::Unknown;
    };
    if paths.is_empty() {
        return ProofState::Unknown;
    }

    let mut saw_unknown = false;
    for path in paths {
        match prove_expression(ensures, &path.env, path.result_expr.as_ref()) {
            ProofState::False => return ProofState::False,
            ProofState::True => {}
            ProofState::Unknown => saw_unknown = true,
        }
    }

    if saw_unknown {
        ProofState::Unknown
    } else {
        ProofState::True
    }
}

fn collect_tail_paths(block: &ir::Block, env: &RangeEnv) -> Option<Vec<TailPath>> {
    if !block.stmts.is_empty() {
        return None;
    }

    match &block.tail {
        Some(tail) => collect_expr_paths(tail, env),
        None => Some(vec![TailPath {
            env: env.clone(),
            result_expr: None,
        }]),
    }
}

fn collect_expr_paths(expr: &ir::Expr, env: &RangeEnv) -> Option<Vec<TailPath>> {
    match &expr.kind {
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            let true_env = refine_env_for_condition(env, cond, true);
            let false_env = refine_env_for_condition(env, cond, false);

            let mut out = Vec::new();
            if let Some(env) = true_env {
                out.extend(collect_tail_paths(then_block, &env)?);
            }
            if let Some(env) = false_env {
                out.extend(collect_tail_paths(else_block, &env)?);
            }
            Some(out)
        }
        _ => Some(vec![TailPath {
            env: env.clone(),
            result_expr: Some(expr.clone()),
        }]),
    }
}

fn refine_env_for_condition(env: &RangeEnv, cond: &ir::Expr, truth: bool) -> Option<RangeEnv> {
    let ir::ExprKind::Binary { op, lhs, rhs } = &cond.kind else {
        return Some(env.clone());
    };
    let cmp = if truth { *op } else { invert_cmp(*op)? };
    let Some((name, cmp, value)) = extract_var_const_cmp(lhs, rhs, cmp) else {
        return Some(env.clone());
    };

    let mut next = env.clone();
    let mut range = next.get(&name).copied().unwrap_or_else(IntRange::unbounded);

    match cmp {
        BinOp::Lt => {
            range.upper = min_bound(range.upper, value.checked_sub(1));
        }
        BinOp::Le => {
            range.upper = min_bound(range.upper, Some(value));
        }
        BinOp::Gt => {
            range.lower = max_bound(range.lower, value.checked_add(1));
        }
        BinOp::Ge => {
            range.lower = max_bound(range.lower, Some(value));
        }
        BinOp::Eq => {
            range.lower = Some(value);
            range.upper = Some(value);
        }
        BinOp::Ne => {}
        _ => return Some(env.clone()),
    }

    if bounds_conflict(range.lower, range.upper) {
        return None;
    }
    next.insert(name, range);
    Some(next)
}

fn invert_cmp(op: BinOp) -> Option<BinOp> {
    match op {
        BinOp::Lt => Some(BinOp::Ge),
        BinOp::Le => Some(BinOp::Gt),
        BinOp::Gt => Some(BinOp::Le),
        BinOp::Ge => Some(BinOp::Lt),
        BinOp::Eq => Some(BinOp::Ne),
        BinOp::Ne => Some(BinOp::Eq),
        _ => None,
    }
}

fn extract_var_const_cmp(
    lhs: &ir::Expr,
    rhs: &ir::Expr,
    op: BinOp,
) -> Option<(String, BinOp, i64)> {
    match (&lhs.kind, &rhs.kind) {
        (ir::ExprKind::Var(name), ir::ExprKind::Int(value)) => Some((name.clone(), op, *value)),
        (ir::ExprKind::Int(value), ir::ExprKind::Var(name)) => {
            Some((name.clone(), flip_cmp(op)?, *value))
        }
        _ => None,
    }
}

fn flip_cmp(op: BinOp) -> Option<BinOp> {
    match op {
        BinOp::Lt => Some(BinOp::Gt),
        BinOp::Le => Some(BinOp::Ge),
        BinOp::Gt => Some(BinOp::Lt),
        BinOp::Ge => Some(BinOp::Le),
        BinOp::Eq => Some(BinOp::Eq),
        BinOp::Ne => Some(BinOp::Ne),
        _ => None,
    }
}

fn prove_expression(expr: &ir::Expr, env: &RangeEnv, result_expr: Option<&ir::Expr>) -> ProofState {
    eval_logic(expr, env, result_expr)
}

fn eval_logic(expr: &ir::Expr, env: &RangeEnv, result_expr: Option<&ir::Expr>) -> ProofState {
    match &expr.kind {
        ir::ExprKind::Bool(value) => {
            if *value {
                ProofState::True
            } else {
                ProofState::False
            }
        }
        ir::ExprKind::Unary {
            op: UnaryOp::Not,
            expr,
        } => match eval_logic(expr, env, result_expr) {
            ProofState::True => ProofState::False,
            ProofState::False => ProofState::True,
            ProofState::Unknown => ProofState::Unknown,
        },
        ir::ExprKind::Binary {
            op: BinOp::And,
            lhs,
            rhs,
        } => match (
            eval_logic(lhs, env, result_expr),
            eval_logic(rhs, env, result_expr),
        ) {
            (ProofState::False, _) | (_, ProofState::False) => ProofState::False,
            (ProofState::True, ProofState::True) => ProofState::True,
            _ => ProofState::Unknown,
        },
        ir::ExprKind::Binary {
            op: BinOp::Or,
            lhs,
            rhs,
        } => match (
            eval_logic(lhs, env, result_expr),
            eval_logic(rhs, env, result_expr),
        ) {
            (ProofState::True, _) | (_, ProofState::True) => ProofState::True,
            (ProofState::False, ProofState::False) => ProofState::False,
            _ => ProofState::Unknown,
        },
        ir::ExprKind::Binary { op, lhs, rhs } => eval_comparison(*op, lhs, rhs, env, result_expr),
        _ => match eval_const(expr) {
            Some(Value::Bool(value)) => {
                if value {
                    ProofState::True
                } else {
                    ProofState::False
                }
            }
            _ => ProofState::Unknown,
        },
    }
}

fn eval_comparison(
    op: BinOp,
    lhs: &ir::Expr,
    rhs: &ir::Expr,
    env: &RangeEnv,
    result_expr: Option<&ir::Expr>,
) -> ProofState {
    if same_int_expr(lhs, rhs) {
        return match op {
            BinOp::Eq | BinOp::Le | BinOp::Ge => ProofState::True,
            BinOp::Ne | BinOp::Lt | BinOp::Gt => ProofState::False,
            _ => ProofState::Unknown,
        };
    }

    let Some(left) = eval_int_range(lhs, env, result_expr) else {
        return ProofState::Unknown;
    };
    let Some(right) = eval_int_range(rhs, env, result_expr) else {
        return ProofState::Unknown;
    };

    match op {
        BinOp::Lt => {
            if let (Some(lu), Some(rl)) = (left.upper, right.lower) {
                if lu < rl {
                    return ProofState::True;
                }
            }
            if let (Some(ll), Some(ru)) = (left.lower, right.upper) {
                if ll >= ru {
                    return ProofState::False;
                }
            }
            ProofState::Unknown
        }
        BinOp::Le => {
            if let (Some(lu), Some(rl)) = (left.upper, right.lower) {
                if lu <= rl {
                    return ProofState::True;
                }
            }
            if let (Some(ll), Some(ru)) = (left.lower, right.upper) {
                if ll > ru {
                    return ProofState::False;
                }
            }
            ProofState::Unknown
        }
        BinOp::Gt => {
            if let (Some(ll), Some(ru)) = (left.lower, right.upper) {
                if ll > ru {
                    return ProofState::True;
                }
            }
            if let (Some(lu), Some(rl)) = (left.upper, right.lower) {
                if lu <= rl {
                    return ProofState::False;
                }
            }
            ProofState::Unknown
        }
        BinOp::Ge => {
            if let (Some(ll), Some(ru)) = (left.lower, right.upper) {
                if ll >= ru {
                    return ProofState::True;
                }
            }
            if let (Some(lu), Some(rl)) = (left.upper, right.lower) {
                if lu < rl {
                    return ProofState::False;
                }
            }
            ProofState::Unknown
        }
        BinOp::Eq => {
            if left.lower == left.upper
                && left.lower.is_some()
                && left.lower == right.lower
                && right.lower == right.upper
            {
                return ProofState::True;
            }
            if ranges_disjoint(left, right) {
                return ProofState::False;
            }
            ProofState::Unknown
        }
        BinOp::Ne => match eval_comparison(BinOp::Eq, lhs, rhs, env, result_expr) {
            ProofState::True => ProofState::False,
            ProofState::False => ProofState::True,
            ProofState::Unknown => ProofState::Unknown,
        },
        _ => ProofState::Unknown,
    }
}

fn eval_int_range(
    expr: &ir::Expr,
    env: &RangeEnv,
    result_expr: Option<&ir::Expr>,
) -> Option<IntRange> {
    match &expr.kind {
        ir::ExprKind::Int(value) => Some(IntRange::singleton(*value)),
        ir::ExprKind::Var(name) if name == "result" => {
            let value = result_expr?;
            eval_int_range(value, env, None)
        }
        ir::ExprKind::Var(name) => Some(env.get(name).copied().unwrap_or_else(IntRange::unbounded)),
        ir::ExprKind::Unary {
            op: UnaryOp::Neg,
            expr,
        } => Some(eval_int_range(expr, env, result_expr)?.neg()),
        ir::ExprKind::Binary {
            op: BinOp::Add,
            lhs,
            rhs,
        } => {
            Some(eval_int_range(lhs, env, result_expr)?.add(eval_int_range(rhs, env, result_expr)?))
        }
        ir::ExprKind::Binary {
            op: BinOp::Sub,
            lhs,
            rhs,
        } => {
            Some(eval_int_range(lhs, env, result_expr)?.sub(eval_int_range(rhs, env, result_expr)?))
        }
        _ => match eval_const(expr) {
            Some(Value::Int(value)) => Some(IntRange::singleton(value)),
            _ => None,
        },
    }
}

fn same_int_expr(lhs: &ir::Expr, rhs: &ir::Expr) -> bool {
    match (&lhs.kind, &rhs.kind) {
        (ir::ExprKind::Int(a), ir::ExprKind::Int(b)) => a == b,
        (ir::ExprKind::Var(a), ir::ExprKind::Var(b)) => a == b,
        (
            ir::ExprKind::Unary {
                op: UnaryOp::Neg,
                expr: a,
            },
            ir::ExprKind::Unary {
                op: UnaryOp::Neg,
                expr: b,
            },
        ) => same_int_expr(a, b),
        (
            ir::ExprKind::Binary {
                op: op_a,
                lhs: lhs_a,
                rhs: rhs_a,
            },
            ir::ExprKind::Binary {
                op: op_b,
                lhs: lhs_b,
                rhs: rhs_b,
            },
        ) if op_a == op_b && matches!(op_a, BinOp::Add | BinOp::Sub) => {
            same_int_expr(lhs_a, lhs_b) && same_int_expr(rhs_a, rhs_b)
        }
        _ => false,
    }
}

fn ranges_disjoint(lhs: IntRange, rhs: IntRange) -> bool {
    if let (Some(lu), Some(rl)) = (lhs.upper, rhs.lower) {
        if lu < rl {
            return true;
        }
    }
    if let (Some(ru), Some(ll)) = (rhs.upper, lhs.lower) {
        if ru < ll {
            return true;
        }
    }
    false
}

fn checked_add_bound(lhs: Option<i64>, rhs: Option<i64>) -> Option<i64> {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => lhs.checked_add(rhs),
        _ => None,
    }
}

fn checked_sub_bound(lhs: Option<i64>, rhs: Option<i64>) -> Option<i64> {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => lhs.checked_sub(rhs),
        _ => None,
    }
}

fn bounds_conflict(lower: Option<i64>, upper: Option<i64>) -> bool {
    match (lower, upper) {
        (Some(lower), Some(upper)) => lower > upper,
        _ => false,
    }
}

fn min_bound(lhs: Option<i64>, rhs: Option<i64>) -> Option<i64> {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => Some(lhs.min(rhs)),
        (Some(lhs), None) => Some(lhs),
        (None, Some(rhs)) => Some(rhs),
        (None, None) => None,
    }
}

fn max_bound(lhs: Option<i64>, rhs: Option<i64>) -> Option<i64> {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => Some(lhs.max(rhs)),
        (Some(lhs), None) => Some(lhs),
        (None, Some(rhs)) => Some(rhs),
        (None, None) => None,
    }
}

fn collect_invariant_specs(program: &ir::Program) -> BTreeMap<String, InvariantSpec> {
    let mut specs = BTreeMap::new();
    for item in &program.items {
        let ir::Item::Struct(strukt) = item else {
            continue;
        };
        let Some(invariant) = &strukt.invariant else {
            continue;
        };
        specs.insert(
            strukt.name.clone(),
            InvariantSpec {
                name: strukt.name.clone(),
                span: strukt.span,
                generics: strukt.generics.clone(),
                fields: strukt.fields.clone(),
                invariant: invariant.clone(),
            },
        );
    }
    specs
}

fn invariant_helper_name(struct_name: &str) -> String {
    format!("__aic_invariant_ctor_{}", struct_name)
}

fn build_invariant_helpers(
    program: &mut ir::Program,
    specs: &BTreeMap<String, InvariantSpec>,
    helper_names: &BTreeMap<String, String>,
    alloc: &mut IdAlloc,
) -> Vec<ir::Function> {
    let mut helpers = Vec::new();

    for (name, spec) in specs {
        let helper_name = helper_names
            .get(name)
            .cloned()
            .unwrap_or_else(|| invariant_helper_name(name));
        let ret_repr = if spec.generics.is_empty() {
            name.clone()
        } else {
            format!(
                "{}[{}]",
                name,
                spec.generics
                    .iter()
                    .map(|g| g.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let ret_type = intern_type(program, &ret_repr, alloc);

        let params = spec
            .fields
            .iter()
            .map(|field| ir::Param {
                symbol: ir::SymbolId(alloc.next_symbol()),
                name: field.name.clone(),
                ty: field.ty,
                span: field.span,
            })
            .collect::<Vec<_>>();

        let field_env = spec
            .fields
            .iter()
            .map(|field| (field.name.clone(), IntRange::unbounded()))
            .collect::<RangeEnv>();
        let invariant_discharged =
            prove_expression(&spec.invariant, &field_env, None) == ProofState::True;

        let mut stmts = Vec::new();
        if !invariant_discharged {
            stmts.push(ir::Stmt::Assert {
                expr: clone_expr(&spec.invariant, alloc),
                message: contract_message("invariant", &spec.name),
                span: spec.invariant.span,
            });
        }

        let tail = ir::Expr {
            node: ir::NodeId(alloc.next_node()),
            kind: ir::ExprKind::StructInit {
                name: spec.name.clone(),
                fields: params
                    .iter()
                    .map(|param| {
                        (
                            param.name.clone(),
                            ir::Expr {
                                node: ir::NodeId(alloc.next_node()),
                                kind: ir::ExprKind::Var(param.name.clone()),
                                span: param.span,
                            },
                            param.span,
                        )
                    })
                    .collect::<Vec<_>>(),
            },
            span: spec.span,
        };

        helpers.push(ir::Function {
            symbol: ir::SymbolId(alloc.next_symbol()),
            name: helper_name,
            is_async: false,
            is_unsafe: false,
            is_extern: false,
            extern_abi: None,
            generics: spec.generics.clone(),
            params,
            ret_type,
            effects: Vec::new(),
            requires: None,
            ensures: None,
            body: ir::Block {
                node: ir::NodeId(alloc.next_node()),
                stmts,
                tail: Some(Box::new(tail)),
                span: spec.span,
            },
            span: spec.span,
        });
    }

    helpers
}

fn rewrite_struct_inits_in_block(
    block: &mut ir::Block,
    helper_names: &BTreeMap<String, String>,
    field_orders: &BTreeMap<String, Vec<String>>,
    alloc: &mut IdAlloc,
) {
    for stmt in &mut block.stmts {
        match stmt {
            ir::Stmt::Let { expr, .. }
            | ir::Stmt::Assign { expr, .. }
            | ir::Stmt::Expr { expr, .. }
            | ir::Stmt::Assert { expr, .. } => {
                rewrite_struct_inits_in_expr(expr, helper_names, field_orders, alloc);
            }
            ir::Stmt::Return {
                expr: Some(expr), ..
            } => rewrite_struct_inits_in_expr(expr, helper_names, field_orders, alloc),
            ir::Stmt::Return { expr: None, .. } => {}
        }
    }
    if let Some(tail) = &mut block.tail {
        rewrite_struct_inits_in_expr(tail, helper_names, field_orders, alloc);
    }
}

fn rewrite_struct_inits_in_expr(
    expr: &mut ir::Expr,
    helper_names: &BTreeMap<String, String>,
    field_orders: &BTreeMap<String, Vec<String>>,
    alloc: &mut IdAlloc,
) {
    match &mut expr.kind {
        ir::ExprKind::Call { callee, args } => {
            rewrite_struct_inits_in_expr(callee, helper_names, field_orders, alloc);
            for arg in args {
                rewrite_struct_inits_in_expr(arg, helper_names, field_orders, alloc);
            }
        }
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            rewrite_struct_inits_in_expr(cond, helper_names, field_orders, alloc);
            rewrite_struct_inits_in_block(then_block, helper_names, field_orders, alloc);
            rewrite_struct_inits_in_block(else_block, helper_names, field_orders, alloc);
        }
        ir::ExprKind::While { cond, body } => {
            rewrite_struct_inits_in_expr(cond, helper_names, field_orders, alloc);
            rewrite_struct_inits_in_block(body, helper_names, field_orders, alloc);
        }
        ir::ExprKind::Loop { body } => {
            rewrite_struct_inits_in_block(body, helper_names, field_orders, alloc);
        }
        ir::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                rewrite_struct_inits_in_expr(expr, helper_names, field_orders, alloc);
            }
        }
        ir::ExprKind::Continue => {}
        ir::ExprKind::Match { expr, arms } => {
            rewrite_struct_inits_in_expr(expr, helper_names, field_orders, alloc);
            for arm in arms {
                rewrite_struct_inits_in_expr(&mut arm.body, helper_names, field_orders, alloc);
            }
        }
        ir::ExprKind::Binary { lhs, rhs, .. } => {
            rewrite_struct_inits_in_expr(lhs, helper_names, field_orders, alloc);
            rewrite_struct_inits_in_expr(rhs, helper_names, field_orders, alloc);
        }
        ir::ExprKind::Unary { expr, .. } => {
            rewrite_struct_inits_in_expr(expr, helper_names, field_orders, alloc);
        }
        ir::ExprKind::Borrow { expr, .. } => {
            rewrite_struct_inits_in_expr(expr, helper_names, field_orders, alloc);
        }
        ir::ExprKind::Await { expr } => {
            rewrite_struct_inits_in_expr(expr, helper_names, field_orders, alloc);
        }
        ir::ExprKind::Try { expr } => {
            rewrite_struct_inits_in_expr(expr, helper_names, field_orders, alloc);
        }
        ir::ExprKind::UnsafeBlock { block } => {
            rewrite_struct_inits_in_block(block, helper_names, field_orders, alloc);
        }
        ir::ExprKind::StructInit { name, fields } => {
            for (_, value, _) in fields.iter_mut() {
                rewrite_struct_inits_in_expr(value, helper_names, field_orders, alloc);
            }
            let Some(helper_name) = helper_names.get(name).cloned() else {
                return;
            };
            let Some(order) = field_orders.get(name) else {
                return;
            };
            let mut args = Vec::new();
            for field_name in order {
                let Some((_, value, _)) = fields.iter().find(|(name, _, _)| name == field_name)
                else {
                    return;
                };
                args.push(clone_expr(value, alloc));
            }
            expr.kind = ir::ExprKind::Call {
                callee: Box::new(ir::Expr {
                    node: ir::NodeId(alloc.next_node()),
                    kind: ir::ExprKind::Var(helper_name),
                    span: expr.span,
                }),
                args,
            };
        }
        ir::ExprKind::FieldAccess { base, .. } => {
            rewrite_struct_inits_in_expr(base, helper_names, field_orders, alloc);
        }
        ir::ExprKind::Int(_)
        | ir::ExprKind::Bool(_)
        | ir::ExprKind::String(_)
        | ir::ExprKind::Unit
        | ir::ExprKind::Var(_) => {}
    }
}

fn lower_ensures_in_block(
    block: &mut ir::Block,
    ensures: &ir::Expr,
    ret_type: ir::TypeId,
    function_name: &str,
    alloc: &mut IdAlloc,
    instrument_tail: bool,
) {
    let mut lowered_stmts = Vec::new();
    for stmt in std::mem::take(&mut block.stmts) {
        match stmt {
            ir::Stmt::Let {
                symbol,
                name,
                mutable,
                ty,
                mut expr,
                span,
            } => {
                lower_ensures_in_expr(&mut expr, ensures, ret_type, function_name, alloc);
                lowered_stmts.push(ir::Stmt::Let {
                    symbol,
                    name,
                    mutable,
                    ty,
                    expr,
                    span,
                });
            }
            ir::Stmt::Assign {
                target,
                mut expr,
                span,
            } => {
                lower_ensures_in_expr(&mut expr, ensures, ret_type, function_name, alloc);
                lowered_stmts.push(ir::Stmt::Assign { target, expr, span });
            }
            ir::Stmt::Expr { mut expr, span } => {
                lower_ensures_in_expr(&mut expr, ensures, ret_type, function_name, alloc);
                lowered_stmts.push(ir::Stmt::Expr { expr, span });
            }
            ir::Stmt::Assert {
                mut expr,
                message,
                span,
            } => {
                lower_ensures_in_expr(&mut expr, ensures, ret_type, function_name, alloc);
                lowered_stmts.push(ir::Stmt::Assert {
                    expr,
                    message,
                    span,
                });
            }
            ir::Stmt::Return { expr, span } => {
                let mut lowered =
                    lower_exit_with_ensures(expr, span, ensures, ret_type, function_name, alloc);
                lowered_stmts.append(&mut lowered);
            }
        }
    }
    block.stmts = lowered_stmts;

    if instrument_tail {
        let tail_expr = block.tail.take().map(|tail| *tail);
        let mut lowered = lower_exit_with_ensures(
            tail_expr,
            block.span,
            ensures,
            ret_type,
            function_name,
            alloc,
        );
        block.stmts.append(&mut lowered);
        block.tail = None;
    } else if let Some(tail) = &mut block.tail {
        lower_ensures_in_expr(tail, ensures, ret_type, function_name, alloc);
    }
}

fn lower_ensures_in_expr(
    expr: &mut ir::Expr,
    ensures: &ir::Expr,
    ret_type: ir::TypeId,
    function_name: &str,
    alloc: &mut IdAlloc,
) {
    match &mut expr.kind {
        ir::ExprKind::Call { callee, args } => {
            lower_ensures_in_expr(callee, ensures, ret_type, function_name, alloc);
            for arg in args {
                lower_ensures_in_expr(arg, ensures, ret_type, function_name, alloc);
            }
        }
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            lower_ensures_in_expr(cond, ensures, ret_type, function_name, alloc);
            lower_ensures_in_block(then_block, ensures, ret_type, function_name, alloc, false);
            lower_ensures_in_block(else_block, ensures, ret_type, function_name, alloc, false);
        }
        ir::ExprKind::While { cond, body } => {
            lower_ensures_in_expr(cond, ensures, ret_type, function_name, alloc);
            lower_ensures_in_block(body, ensures, ret_type, function_name, alloc, false);
        }
        ir::ExprKind::Loop { body } => {
            lower_ensures_in_block(body, ensures, ret_type, function_name, alloc, false);
        }
        ir::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                lower_ensures_in_expr(expr, ensures, ret_type, function_name, alloc);
            }
        }
        ir::ExprKind::Continue => {}
        ir::ExprKind::Match { expr, arms } => {
            lower_ensures_in_expr(expr, ensures, ret_type, function_name, alloc);
            for arm in arms {
                lower_ensures_in_expr(&mut arm.body, ensures, ret_type, function_name, alloc);
            }
        }
        ir::ExprKind::Binary { lhs, rhs, .. } => {
            lower_ensures_in_expr(lhs, ensures, ret_type, function_name, alloc);
            lower_ensures_in_expr(rhs, ensures, ret_type, function_name, alloc);
        }
        ir::ExprKind::Unary { expr, .. } => {
            lower_ensures_in_expr(expr, ensures, ret_type, function_name, alloc);
        }
        ir::ExprKind::Borrow { expr, .. } => {
            lower_ensures_in_expr(expr, ensures, ret_type, function_name, alloc);
        }
        ir::ExprKind::Await { expr } => {
            lower_ensures_in_expr(expr, ensures, ret_type, function_name, alloc);
        }
        ir::ExprKind::Try { expr } => {
            lower_ensures_in_expr(expr, ensures, ret_type, function_name, alloc);
        }
        ir::ExprKind::UnsafeBlock { block } => {
            lower_ensures_in_block(block, ensures, ret_type, function_name, alloc, false);
        }
        ir::ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields.iter_mut() {
                lower_ensures_in_expr(value, ensures, ret_type, function_name, alloc);
            }
        }
        ir::ExprKind::FieldAccess { base, .. } => {
            lower_ensures_in_expr(base, ensures, ret_type, function_name, alloc);
        }
        ir::ExprKind::Int(_)
        | ir::ExprKind::Bool(_)
        | ir::ExprKind::String(_)
        | ir::ExprKind::Unit
        | ir::ExprKind::Var(_) => {}
    }
}

fn lower_exit_with_ensures(
    expr: Option<ir::Expr>,
    span: crate::span::Span,
    ensures: &ir::Expr,
    ret_type: ir::TypeId,
    function_name: &str,
    alloc: &mut IdAlloc,
) -> Vec<ir::Stmt> {
    let mut exit_expr = expr.unwrap_or_else(|| ir::Expr {
        node: ir::NodeId(alloc.next_node()),
        kind: ir::ExprKind::Unit,
        span,
    });
    lower_ensures_in_expr(&mut exit_expr, ensures, ret_type, function_name, alloc);

    let result_symbol = ir::SymbolId(alloc.next_symbol());
    let result_name = format!("__aic_result_{}", result_symbol.0);
    let let_stmt = ir::Stmt::Let {
        symbol: result_symbol,
        name: result_name.clone(),
        mutable: false,
        ty: Some(ret_type),
        expr: exit_expr,
        span,
    };
    let assert_stmt = ir::Stmt::Assert {
        expr: substitute_result_var(ensures, &result_name, alloc),
        message: contract_message("ensures", function_name),
        span,
    };
    let return_stmt = ir::Stmt::Return {
        expr: Some(ir::Expr {
            node: ir::NodeId(alloc.next_node()),
            kind: ir::ExprKind::Var(result_name),
            span,
        }),
        span,
    };
    vec![let_stmt, assert_stmt, return_stmt]
}

#[derive(Debug, Clone)]
enum Value {
    Int(i64),
    Bool(bool),
}

fn eval_const(expr: &ir::Expr) -> Option<Value> {
    match &expr.kind {
        ir::ExprKind::Int(v) => Some(Value::Int(*v)),
        ir::ExprKind::Bool(v) => Some(Value::Bool(*v)),
        ir::ExprKind::Unary { op, expr } => {
            let v = eval_const(expr)?;
            match (op, v) {
                (UnaryOp::Neg, Value::Int(i)) => Some(Value::Int(-i)),
                (UnaryOp::Not, Value::Bool(b)) => Some(Value::Bool(!b)),
                _ => None,
            }
        }
        ir::ExprKind::Binary { op, lhs, rhs } => {
            let l = eval_const(lhs)?;
            let r = eval_const(rhs)?;
            match (op, l, r) {
                (BinOp::Add, Value::Int(a), Value::Int(b)) => Some(Value::Int(a + b)),
                (BinOp::Sub, Value::Int(a), Value::Int(b)) => Some(Value::Int(a - b)),
                (BinOp::Mul, Value::Int(a), Value::Int(b)) => Some(Value::Int(a * b)),
                (BinOp::Div, Value::Int(a), Value::Int(b)) => Some(Value::Int(a / b)),
                (BinOp::Mod, Value::Int(a), Value::Int(b)) => Some(Value::Int(a % b)),
                (BinOp::Eq, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a == b)),
                (BinOp::Eq, Value::Bool(a), Value::Bool(b)) => Some(Value::Bool(a == b)),
                (BinOp::Ne, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a != b)),
                (BinOp::Ne, Value::Bool(a), Value::Bool(b)) => Some(Value::Bool(a != b)),
                (BinOp::Lt, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a < b)),
                (BinOp::Le, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a <= b)),
                (BinOp::Gt, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a > b)),
                (BinOp::Ge, Value::Int(a), Value::Int(b)) => Some(Value::Bool(a >= b)),
                (BinOp::And, Value::Bool(a), Value::Bool(b)) => Some(Value::Bool(a && b)),
                (BinOp::Or, Value::Bool(a), Value::Bool(b)) => Some(Value::Bool(a || b)),
                _ => None,
            }
        }
        _ => None,
    }
}

struct IdAlloc {
    next_symbol: u32,
    next_node: u32,
    next_type: u32,
}

impl IdAlloc {
    fn from_program(program: &ir::Program) -> Self {
        let next_symbol = program.symbols.iter().map(|s| s.id.0).max().unwrap_or(0) + 1;
        let next_node = max_node(program) + 1;
        let next_type = program.types.iter().map(|t| t.id.0).max().unwrap_or(0) + 1;
        Self {
            next_symbol,
            next_node,
            next_type,
        }
    }

    fn next_symbol(&mut self) -> u32 {
        let out = self.next_symbol;
        self.next_symbol += 1;
        out
    }

    fn next_node(&mut self) -> u32 {
        let out = self.next_node;
        self.next_node += 1;
        out
    }

    fn next_type(&mut self) -> u32 {
        let out = self.next_type;
        self.next_type += 1;
        out
    }
}

fn intern_type(program: &mut ir::Program, repr: &str, alloc: &mut IdAlloc) -> ir::TypeId {
    if let Some(existing) = program.types.iter().find(|ty| ty.repr == repr) {
        return existing.id;
    }

    let id = ir::TypeId(alloc.next_type());
    program.types.push(ir::TypeDef {
        id,
        repr: repr.to_string(),
    });
    id
}

fn max_node(program: &ir::Program) -> u32 {
    let mut max = 0;
    for item in &program.items {
        if let ir::Item::Function(func) = item {
            max = max.max(max_node_block(&func.body));
            if let Some(req) = &func.requires {
                max = max.max(max_node_expr(req));
            }
            if let Some(ens) = &func.ensures {
                max = max.max(max_node_expr(ens));
            }
        }
        if let ir::Item::Struct(s) = item {
            if let Some(inv) = &s.invariant {
                max = max.max(max_node_expr(inv));
            }
        }
    }
    max
}

fn max_node_block(block: &ir::Block) -> u32 {
    let mut max = block.node.0;
    for stmt in &block.stmts {
        match stmt {
            ir::Stmt::Let { expr, .. } => max = max.max(max_node_expr(expr)),
            ir::Stmt::Assign { expr, .. } => max = max.max(max_node_expr(expr)),
            ir::Stmt::Expr { expr, .. } => max = max.max(max_node_expr(expr)),
            ir::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    max = max.max(max_node_expr(expr));
                }
            }
            ir::Stmt::Assert { expr, .. } => max = max.max(max_node_expr(expr)),
        }
    }
    if let Some(tail) = &block.tail {
        max = max.max(max_node_expr(tail));
    }
    max
}

fn max_node_expr(expr: &ir::Expr) -> u32 {
    let mut max = expr.node.0;
    match &expr.kind {
        ir::ExprKind::Call { callee, args } => {
            max = max.max(max_node_expr(callee));
            for arg in args {
                max = max.max(max_node_expr(arg));
            }
        }
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            max = max.max(max_node_expr(cond));
            max = max.max(max_node_block(then_block));
            max = max.max(max_node_block(else_block));
        }
        ir::ExprKind::Match { expr, arms } => {
            max = max.max(max_node_expr(expr));
            for arm in arms {
                max = max.max(max_node_pattern(&arm.pattern));
                if let Some(guard) = &arm.guard {
                    max = max.max(max_node_expr(guard));
                }
                max = max.max(max_node_expr(&arm.body));
            }
        }
        ir::ExprKind::Binary { lhs, rhs, .. } => {
            max = max.max(max_node_expr(lhs));
            max = max.max(max_node_expr(rhs));
        }
        ir::ExprKind::Unary { expr, .. } => {
            max = max.max(max_node_expr(expr));
        }
        ir::ExprKind::Borrow { expr, .. } => {
            max = max.max(max_node_expr(expr));
        }
        ir::ExprKind::Await { expr } | ir::ExprKind::Try { expr } => {
            max = max.max(max_node_expr(expr));
        }
        ir::ExprKind::UnsafeBlock { block } => {
            max = max.max(max_node_block(block));
        }
        ir::ExprKind::StructInit { fields, .. } => {
            for (_, expr, _) in fields {
                max = max.max(max_node_expr(expr));
            }
        }
        ir::ExprKind::FieldAccess { base, .. } => {
            max = max.max(max_node_expr(base));
        }
        _ => {}
    }
    max
}

fn max_node_pattern(pattern: &ir::Pattern) -> u32 {
    let mut max = pattern.node.0;
    match &pattern.kind {
        ir::PatternKind::Or { patterns } => {
            for part in patterns {
                max = max.max(max_node_pattern(part));
            }
        }
        ir::PatternKind::Variant { args, .. } => {
            for arg in args {
                max = max.max(max_node_pattern(arg));
            }
        }
        _ => {}
    }
    max
}

fn clone_expr(expr: &ir::Expr, alloc: &mut IdAlloc) -> ir::Expr {
    let kind = match &expr.kind {
        ir::ExprKind::Int(v) => ir::ExprKind::Int(*v),
        ir::ExprKind::Bool(v) => ir::ExprKind::Bool(*v),
        ir::ExprKind::String(v) => ir::ExprKind::String(v.clone()),
        ir::ExprKind::Unit => ir::ExprKind::Unit,
        ir::ExprKind::Var(v) => ir::ExprKind::Var(v.clone()),
        ir::ExprKind::Call { callee, args } => ir::ExprKind::Call {
            callee: Box::new(clone_expr(callee, alloc)),
            args: args.iter().map(|a| clone_expr(a, alloc)).collect(),
        },
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => ir::ExprKind::If {
            cond: Box::new(clone_expr(cond, alloc)),
            then_block: clone_block(then_block, alloc),
            else_block: clone_block(else_block, alloc),
        },
        ir::ExprKind::While { cond, body } => ir::ExprKind::While {
            cond: Box::new(clone_expr(cond, alloc)),
            body: clone_block(body, alloc),
        },
        ir::ExprKind::Loop { body } => ir::ExprKind::Loop {
            body: clone_block(body, alloc),
        },
        ir::ExprKind::Break { expr } => ir::ExprKind::Break {
            expr: expr.as_ref().map(|expr| Box::new(clone_expr(expr, alloc))),
        },
        ir::ExprKind::Continue => ir::ExprKind::Continue,
        ir::ExprKind::Match { expr, arms } => ir::ExprKind::Match {
            expr: Box::new(clone_expr(expr, alloc)),
            arms: arms
                .iter()
                .map(|arm| ir::MatchArm {
                    pattern: clone_pattern(&arm.pattern, alloc),
                    guard: arm.guard.as_ref().map(|g| clone_expr(g, alloc)),
                    body: clone_expr(&arm.body, alloc),
                    span: arm.span,
                })
                .collect(),
        },
        ir::ExprKind::Binary { op, lhs, rhs } => ir::ExprKind::Binary {
            op: *op,
            lhs: Box::new(clone_expr(lhs, alloc)),
            rhs: Box::new(clone_expr(rhs, alloc)),
        },
        ir::ExprKind::Unary { op, expr } => ir::ExprKind::Unary {
            op: *op,
            expr: Box::new(clone_expr(expr, alloc)),
        },
        ir::ExprKind::Borrow { mutable, expr } => ir::ExprKind::Borrow {
            mutable: *mutable,
            expr: Box::new(clone_expr(expr, alloc)),
        },
        ir::ExprKind::Await { expr } => ir::ExprKind::Await {
            expr: Box::new(clone_expr(expr, alloc)),
        },
        ir::ExprKind::Try { expr } => ir::ExprKind::Try {
            expr: Box::new(clone_expr(expr, alloc)),
        },
        ir::ExprKind::UnsafeBlock { block } => ir::ExprKind::UnsafeBlock {
            block: clone_block(block, alloc),
        },
        ir::ExprKind::StructInit { name, fields } => ir::ExprKind::StructInit {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, expr, span)| (name.clone(), clone_expr(expr, alloc), *span))
                .collect(),
        },
        ir::ExprKind::FieldAccess { base, field } => ir::ExprKind::FieldAccess {
            base: Box::new(clone_expr(base, alloc)),
            field: field.clone(),
        },
    };

    ir::Expr {
        node: ir::NodeId(alloc.next_node()),
        kind,
        span: expr.span,
    }
}

fn clone_block(block: &ir::Block, alloc: &mut IdAlloc) -> ir::Block {
    ir::Block {
        node: ir::NodeId(alloc.next_node()),
        stmts: block
            .stmts
            .iter()
            .map(|stmt| match stmt {
                ir::Stmt::Let {
                    symbol,
                    name,
                    mutable,
                    ty,
                    expr,
                    span,
                } => ir::Stmt::Let {
                    symbol: *symbol,
                    name: name.clone(),
                    mutable: *mutable,
                    ty: *ty,
                    expr: clone_expr(expr, alloc),
                    span: *span,
                },
                ir::Stmt::Assign { target, expr, span } => ir::Stmt::Assign {
                    target: target.clone(),
                    expr: clone_expr(expr, alloc),
                    span: *span,
                },
                ir::Stmt::Expr { expr, span } => ir::Stmt::Expr {
                    expr: clone_expr(expr, alloc),
                    span: *span,
                },
                ir::Stmt::Return { expr, span } => ir::Stmt::Return {
                    expr: expr.as_ref().map(|e| clone_expr(e, alloc)),
                    span: *span,
                },
                ir::Stmt::Assert {
                    expr,
                    message,
                    span,
                } => ir::Stmt::Assert {
                    expr: clone_expr(expr, alloc),
                    message: message.clone(),
                    span: *span,
                },
            })
            .collect(),
        tail: block.tail.as_ref().map(|e| Box::new(clone_expr(e, alloc))),
        span: block.span,
    }
}

fn clone_pattern(pattern: &ir::Pattern, alloc: &mut IdAlloc) -> ir::Pattern {
    let kind = match &pattern.kind {
        ir::PatternKind::Wildcard => ir::PatternKind::Wildcard,
        ir::PatternKind::Var(v) => ir::PatternKind::Var(v.clone()),
        ir::PatternKind::Int(v) => ir::PatternKind::Int(*v),
        ir::PatternKind::Bool(v) => ir::PatternKind::Bool(*v),
        ir::PatternKind::Unit => ir::PatternKind::Unit,
        ir::PatternKind::Or { patterns } => ir::PatternKind::Or {
            patterns: patterns.iter().map(|p| clone_pattern(p, alloc)).collect(),
        },
        ir::PatternKind::Variant { name, args } => ir::PatternKind::Variant {
            name: name.clone(),
            args: args.iter().map(|a| clone_pattern(a, alloc)).collect(),
        },
    };
    ir::Pattern {
        node: ir::NodeId(alloc.next_node()),
        kind,
        span: pattern.span,
    }
}

fn substitute_result_var(expr: &ir::Expr, result_name: &str, alloc: &mut IdAlloc) -> ir::Expr {
    let kind = match &expr.kind {
        ir::ExprKind::Var(name) if name == "result" => ir::ExprKind::Var(result_name.to_string()),
        ir::ExprKind::Int(v) => ir::ExprKind::Int(*v),
        ir::ExprKind::Bool(v) => ir::ExprKind::Bool(*v),
        ir::ExprKind::String(v) => ir::ExprKind::String(v.clone()),
        ir::ExprKind::Unit => ir::ExprKind::Unit,
        ir::ExprKind::Var(v) => ir::ExprKind::Var(v.clone()),
        ir::ExprKind::Call { callee, args } => ir::ExprKind::Call {
            callee: Box::new(substitute_result_var(callee, result_name, alloc)),
            args: args
                .iter()
                .map(|a| substitute_result_var(a, result_name, alloc))
                .collect(),
        },
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => ir::ExprKind::If {
            cond: Box::new(substitute_result_var(cond, result_name, alloc)),
            then_block: clone_block(then_block, alloc),
            else_block: clone_block(else_block, alloc),
        },
        ir::ExprKind::While { cond, body } => ir::ExprKind::While {
            cond: Box::new(substitute_result_var(cond, result_name, alloc)),
            body: clone_block(body, alloc),
        },
        ir::ExprKind::Loop { body } => ir::ExprKind::Loop {
            body: clone_block(body, alloc),
        },
        ir::ExprKind::Break { expr } => ir::ExprKind::Break {
            expr: expr
                .as_ref()
                .map(|expr| Box::new(substitute_result_var(expr, result_name, alloc))),
        },
        ir::ExprKind::Continue => ir::ExprKind::Continue,
        ir::ExprKind::Match { expr, arms } => ir::ExprKind::Match {
            expr: Box::new(substitute_result_var(expr, result_name, alloc)),
            arms: arms
                .iter()
                .map(|arm| ir::MatchArm {
                    pattern: clone_pattern(&arm.pattern, alloc),
                    guard: arm
                        .guard
                        .as_ref()
                        .map(|g| substitute_result_var(g, result_name, alloc)),
                    body: substitute_result_var(&arm.body, result_name, alloc),
                    span: arm.span,
                })
                .collect(),
        },
        ir::ExprKind::Binary { op, lhs, rhs } => ir::ExprKind::Binary {
            op: *op,
            lhs: Box::new(substitute_result_var(lhs, result_name, alloc)),
            rhs: Box::new(substitute_result_var(rhs, result_name, alloc)),
        },
        ir::ExprKind::Unary { op, expr } => ir::ExprKind::Unary {
            op: *op,
            expr: Box::new(substitute_result_var(expr, result_name, alloc)),
        },
        ir::ExprKind::Borrow { mutable, expr } => ir::ExprKind::Borrow {
            mutable: *mutable,
            expr: Box::new(substitute_result_var(expr, result_name, alloc)),
        },
        ir::ExprKind::Await { expr } => ir::ExprKind::Await {
            expr: Box::new(substitute_result_var(expr, result_name, alloc)),
        },
        ir::ExprKind::Try { expr } => ir::ExprKind::Try {
            expr: Box::new(substitute_result_var(expr, result_name, alloc)),
        },
        ir::ExprKind::UnsafeBlock { block } => ir::ExprKind::UnsafeBlock {
            block: clone_block(block, alloc),
        },
        ir::ExprKind::StructInit { name, fields } => ir::ExprKind::StructInit {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, expr, span)| {
                    (
                        name.clone(),
                        substitute_result_var(expr, result_name, alloc),
                        *span,
                    )
                })
                .collect(),
        },
        ir::ExprKind::FieldAccess { base, field } => ir::ExprKind::FieldAccess {
            base: Box::new(substitute_result_var(base, result_name, alloc)),
            field: field.clone(),
        },
    };

    ir::Expr {
        node: ir::NodeId(alloc.next_node()),
        kind,
        span: expr.span,
    }
}

#[cfg(test)]
mod tests {
    use crate::ir;
    use crate::{ir_builder::build, parser::parse};

    use super::{lower_runtime_asserts, verify_static};

    #[test]
    fn static_contract_false_is_reported() {
        let src = "fn f() -> Int ensures false { 1 }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let diags = verify_static(&ir, "test.aic");
        assert!(diags.iter().any(|d| d.code == "E4002"));
    }

    #[test]
    fn lowering_inserts_asserts() {
        let src = "fn f(x: Int) -> Int requires x > 0 ensures result > 0 { x }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let lowered = lower_runtime_asserts(&ir);
        let func = match &lowered.items[0] {
            ir::Item::Function(f) => f,
            _ => panic!(),
        };
        assert!(func
            .body
            .stmts
            .iter()
            .any(|s| matches!(s, ir::Stmt::Assert { .. })));
    }

    #[test]
    fn lowering_skips_discharged_requires_and_ensures() {
        let src = "fn id(x: Int) -> Int requires x == x ensures result == result { x }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let lowered = lower_runtime_asserts(&ir);
        let func = match &lowered.items[0] {
            ir::Item::Function(f) => f,
            _ => panic!(),
        };
        assert!(
            !func
                .body
                .stmts
                .iter()
                .any(|stmt| matches!(stmt, ir::Stmt::Assert { .. })),
            "lowered={:#?}",
            func.body
        );
    }

    #[test]
    fn static_verifier_discharges_abs_postcondition() {
        let src = r#"
fn abs(x: Int) -> Int ensures result >= 0 {
    if x >= 0 { x } else { 0 - x }
}
"#;
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let diags = verify_static(&ir, "test.aic");
        assert!(
            !diags.iter().any(|d| d.code == "E4002"),
            "diags={:#?}",
            diags
        );
        assert!(
            diags.iter().any(|d| d.code == "E4005"),
            "diags={:#?}",
            diags
        );
    }

    #[test]
    fn static_verifier_keeps_unknown_obligations_runtime_checked() {
        let src = r#"
fn choose(x: Int, y: Int) -> Int ensures result >= y {
    x
}
"#;
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let diags = verify_static(&ir, "test.aic");
        assert!(
            !diags.iter().any(|d| d.code == "E4002"),
            "diags={:#?}",
            diags
        );
        assert!(
            !diags.iter().any(|d| d.code == "E4005"),
            "diags={:#?}",
            diags
        );
        assert!(
            diags.iter().any(|d| d.code == "E4003"),
            "diags={:#?}",
            diags
        );
    }

    #[test]
    fn lowering_instruments_ensures_on_explicit_returns() {
        let src = r#"
fn f(x: Int) -> Int ensures result >= 0 {
    if x >= 0 {
        return x;
    } else {
        return 0 - x;
    };
    0
}
"#;
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let lowered = lower_runtime_asserts(&ir);
        let func = match &lowered.items[0] {
            ir::Item::Function(f) => f,
            _ => panic!(),
        };
        assert!(
            count_asserts_in_block(&func.body) >= 2,
            "lowered={:#?}",
            func.body
        );
    }

    #[test]
    fn lowering_rewrites_struct_init_to_invariant_helper() {
        let src = r#"
struct NonEmpty {
    value: Int,
} invariant value > 0

fn make(x: Int) -> NonEmpty {
    NonEmpty { value: x }
}
"#;
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty(), "parse diagnostics={:#?}", d);
        let ir = build(&program.expect("program"));
        let lowered = lower_runtime_asserts(&ir);

        let helper = lowered
            .items
            .iter()
            .find_map(|item| match item {
                ir::Item::Function(func) if func.name == "__aic_invariant_ctor_NonEmpty" => {
                    Some(func)
                }
                _ => None,
            })
            .expect("missing synthesized invariant helper");
        assert!(
            helper
                .body
                .stmts
                .iter()
                .any(|stmt| matches!(stmt, ir::Stmt::Assert { .. })),
            "helper body should assert invariant"
        );

        let make_fn = lowered
            .items
            .iter()
            .find_map(|item| match item {
                ir::Item::Function(func) if func.name == "make" => Some(func),
                _ => None,
            })
            .expect("missing make function");
        let tail = make_fn.body.tail.as_ref().expect("make tail");
        let ir::ExprKind::Call { callee, .. } = &tail.kind else {
            panic!("expected tail call to invariant helper");
        };
        let ir::ExprKind::Var(name) = &callee.kind else {
            panic!("expected callee var");
        };
        assert_eq!(name, "__aic_invariant_ctor_NonEmpty");
    }

    fn count_asserts_in_block(block: &ir::Block) -> usize {
        let mut count = 0;
        for stmt in &block.stmts {
            match stmt {
                ir::Stmt::Assert { .. } => count += 1,
                ir::Stmt::Let { expr, .. }
                | ir::Stmt::Assign { expr, .. }
                | ir::Stmt::Expr { expr, .. } => {
                    count += count_asserts_in_expr(expr);
                }
                ir::Stmt::Return {
                    expr: Some(expr), ..
                } => {
                    count += count_asserts_in_expr(expr);
                }
                ir::Stmt::Return { expr: None, .. } => {}
            }
        }
        if let Some(tail) = &block.tail {
            count += count_asserts_in_expr(tail);
        }
        count
    }

    fn count_asserts_in_expr(expr: &ir::Expr) -> usize {
        match &expr.kind {
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                count_asserts_in_expr(cond)
                    + count_asserts_in_block(then_block)
                    + count_asserts_in_block(else_block)
            }
            ir::ExprKind::While { cond, body } => {
                count_asserts_in_expr(cond) + count_asserts_in_block(body)
            }
            ir::ExprKind::Loop { body } => count_asserts_in_block(body),
            ir::ExprKind::Break { expr } => {
                expr.as_ref().map_or(0, |expr| count_asserts_in_expr(expr))
            }
            ir::ExprKind::Continue => 0,
            ir::ExprKind::Match { expr, arms } => {
                count_asserts_in_expr(expr)
                    + arms
                        .iter()
                        .map(|arm| count_asserts_in_expr(&arm.body))
                        .sum::<usize>()
            }
            ir::ExprKind::Call { callee, args } => {
                count_asserts_in_expr(callee)
                    + args.iter().map(count_asserts_in_expr).sum::<usize>()
            }
            ir::ExprKind::Binary { lhs, rhs, .. } => {
                count_asserts_in_expr(lhs) + count_asserts_in_expr(rhs)
            }
            ir::ExprKind::Unary { expr, .. } => count_asserts_in_expr(expr),
            ir::ExprKind::Borrow { expr, .. } => count_asserts_in_expr(expr),
            ir::ExprKind::Await { expr } => count_asserts_in_expr(expr),
            ir::ExprKind::Try { expr } => count_asserts_in_expr(expr),
            ir::ExprKind::UnsafeBlock { block } => count_asserts_in_block(block),
            ir::ExprKind::StructInit { fields, .. } => fields
                .iter()
                .map(|(_, value, _)| count_asserts_in_expr(value))
                .sum(),
            ir::ExprKind::FieldAccess { base, .. } => count_asserts_in_expr(base),
            ir::ExprKind::Int(_)
            | ir::ExprKind::Bool(_)
            | ir::ExprKind::String(_)
            | ir::ExprKind::Unit
            | ir::ExprKind::Var(_) => 0,
        }
    }
}
