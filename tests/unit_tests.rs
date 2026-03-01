use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::{collections::BTreeSet, path::PathBuf};

use aicore::codegen::emit_llvm;
use aicore::contracts::verify_static;
use aicore::diagnostics::Severity;
use aicore::effects::check_effect_declarations;
use aicore::formatter::format_program;
use aicore::ir_builder::build;
use aicore::parser::parse;
use aicore::project::init_project;
use aicore::resolver::resolve;
use aicore::toolchain::ENV_AIC_STD_ROOT;
use aicore::typecheck::check;
use aicore::{driver::has_errors, driver::run_frontend};
use tempfile::tempdir;

fn lower(source: &str) -> aicore::ir::Program {
    let (program, diags) = parse(source, "unit.aic");
    assert!(diags.is_empty(), "parse diagnostics: {diags:#?}");
    build(&program.expect("program"))
}

fn symbol_ids(ir: &aicore::ir::Program) -> Vec<u32> {
    ir.symbols.iter().map(|s| s.id.0).collect()
}

fn type_ids(ir: &aicore::ir::Program) -> Vec<u32> {
    ir.types.iter().map(|t| t.id.0).collect()
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock")
}

struct ScopedEnvVar {
    name: &'static str,
    previous: Option<String>,
}

impl ScopedEnvVar {
    fn set(name: &'static str, value: String) -> Self {
        let previous = std::env::var(name).ok();
        std::env::set_var(name, value);
        Self { name, previous }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            std::env::set_var(self.name, value);
        } else {
            std::env::remove_var(self.name);
        }
    }
}

fn protocol_replay_tests_enabled() -> bool {
    matches!(
        std::env::var("AIC_ENABLE_PROTOCOL_REPLAY"),
        Ok(value) if value == "1" || value.eq_ignore_ascii_case("true")
    )
}

fn assert_delegate_call(
    source: &str,
    file: &str,
    function_name: &str,
    delegate_name: &str,
    arity: usize,
) {
    let (program, diags) = parse(source, file);
    assert!(diags.is_empty(), "parse diagnostics: {diags:#?}");
    let program = program.expect("program");

    let function = program
        .items
        .iter()
        .find_map(|item| match item {
            aicore::ast::Item::Function(function) if function.name == function_name => {
                Some(function)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing function `{function_name}`"));

    assert!(
        function.body.stmts.is_empty(),
        "`{function_name}` should be a direct delegation wrapper"
    );
    let tail = function
        .body
        .tail
        .as_ref()
        .unwrap_or_else(|| panic!("`{function_name}` should have a tail expression"));

    match &tail.kind {
        aicore::ast::ExprKind::Call { callee, args, .. } => {
            match &callee.kind {
                aicore::ast::ExprKind::Var(name) => {
                    assert_eq!(
                        name, delegate_name,
                        "`{function_name}` should call `{delegate_name}`"
                    );
                }
                _ => panic!("`{function_name}` delegate callee must be a variable name"),
            }
            assert_eq!(
                args.len(),
                arity,
                "`{function_name}` should forward {arity} arguments"
            );
        }
        _ => panic!("`{function_name}` tail must be a call expression"),
    }
}

fn assert_intrinsic_declaration(source: &str, file: &str, function_name: &str, arity: usize) {
    let (program, diags) = parse(source, file);
    assert!(diags.is_empty(), "parse diagnostics: {diags:#?}");
    let program = program.expect("program");

    let function = program
        .items
        .iter()
        .find_map(|item| match item {
            aicore::ast::Item::Function(function) if function.name == function_name => {
                Some(function)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing function `{function_name}`"));

    assert!(
        function.is_intrinsic,
        "`{function_name}` should be declared with `intrinsic fn`"
    );
    assert_eq!(
        function.params.len(),
        arity,
        "`{function_name}` should expose {arity} parameters"
    );
    assert!(
        function.body.stmts.is_empty() && function.body.tail.is_none(),
        "`{function_name}` intrinsic declaration must not include a body"
    );
}

#[test]
fn unit_parse_module_and_imports() {
    let src = "module a.b; import std.io; fn main() -> Int { 0 }";
    let (program, diags) = parse(src, "unit.aic");
    assert!(diags.is_empty());
    let program = program.expect("program");
    assert!(program.module.is_some());
    assert_eq!(program.imports.len(), 1);
}

#[test]
fn unit_parse_function_generics() {
    let src = "fn id[T](x: T) -> T { x }";
    let (program, diags) = parse(src, "unit.aic");
    assert!(diags.is_empty());
    let program = program.expect("program");
    match &program.items[0] {
        aicore::ast::Item::Function(f) => assert_eq!(f.generics.len(), 1),
        _ => panic!("expected fn"),
    }
}

#[test]
fn unit_intrinsic_declaration_typechecks_and_serializes_metadata() {
    let src = r#"
module std.intrinsic_test;

intrinsic fn aic_path_is_abs_intrinsic(path: String) -> Bool effects { fs };

fn main() -> Int { 0 }
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);

    let ir_json = serde_json::to_value(&ir).expect("serialize ir");
    let functions = ir_json["items"]
        .as_array()
        .expect("items array")
        .iter()
        .filter_map(|item| item.get("Function"))
        .collect::<Vec<_>>();
    let intrinsic = functions
        .iter()
        .find(|func| func["name"] == "aic_path_is_abs_intrinsic")
        .expect("intrinsic function in IR JSON");
    assert_eq!(intrinsic["is_intrinsic"], true);
    assert_eq!(intrinsic["intrinsic_abi"], "runtime");
}

#[test]
fn unit_intrinsic_declaration_with_body_reports_e1093() {
    let src = r#"intrinsic fn aic_bad_intrinsic() -> Int { 1 }"#;
    let (_program, diags) = parse(src, "unit.aic");
    assert!(diags.iter().any(|d| d.code == "E1093"), "diags={diags:#?}");
}
#[test]
fn unit_parse_struct_literal_expression() {
    let src = "struct S { x: Int } fn f() -> Int { let s = S { x: 1 }; s.x }";
    let (program, diags) = parse(src, "unit.aic");
    assert!(diags.is_empty(), "diags={diags:#?}");
    assert!(program.is_some());
}

#[test]
fn unit_tuple_types_destructure_and_match_typecheck() {
    let src = r#"
fn swap(a: Int, b: Int) -> (Int, Int) {
    (b, a)
}

fn tuple_first[T, U](pair: (T, U)) -> T {
    pair.0
}

fn main() -> Int {
    let pair = swap(2, 40);
    let (left, right) = pair;
    let matched = match pair {
        (40, value) => value,
        _ => 0,
    };
    tuple_first((left + matched, right))
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
}

#[test]
fn unit_struct_impl_methods_and_method_call_typecheck() {
    let src = r#"
struct User { age: Int }

impl User {
    fn new(age: Int) -> User {
        User { age: age }
    }

    fn age_plus(self) -> Int {
        self.age + 12
    }

    fn is_adult(self) -> Bool {
        self.age >= 18
    }
}

fn main() -> Int {
    let user = User::new(30);
    if user.is_adult() {
        user.age_plus()
    } else {
        0
    }
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
}

#[test]
fn unit_enum_impl_methods_and_chained_calls_typecheck() {
    let src = r#"
enum Wrap {
    Empty,
    Value(Int),
}

impl Wrap {
    fn unwrap_or(self: Wrap, fallback: Int) -> Int {
        match self {
            Value(value) => value,
            Empty => fallback,
        }
    }

    fn map(self: Wrap, mapper: Fn(Int) -> Int) -> Wrap {
        match self {
            Value(value) => Value(mapper(value)),
            Empty => Empty(),
        }
    }

    fn and_then(self: Wrap, mapper: Fn(Int) -> Wrap) -> Wrap {
        match self {
            Value(value) => mapper(value),
            Empty => Empty(),
        }
    }
}

fn add_one(x: Int) -> Int {
    x + 1
}

fn keep_even(x: Int) -> Wrap {
    if x % 2 == 0 { Value(x) } else { Empty() }
}

fn main() -> Int {
    let a = Value(41).map(add_one).and_then(keep_even).unwrap_or(0);
    let b = Empty().map(add_one).unwrap_or(7);
    a + b
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
}

#[test]
fn unit_resolver_duplicate_field() {
    let src = "struct S { x: Int, x: Int }";
    let ir = lower(src);
    let (_res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.iter().any(|d| d.code == "E1101"));
}

#[test]
fn unit_typecheck_unknown_symbol() {
    let src = "fn f() -> Int { missing }";
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1208"));
}

#[test]
fn unit_typecheck_rejects_implicit_int_float_coercion() {
    let src = "fn bad() -> Float { 1 + 2.0 }";
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        out.diagnostics.iter().any(|d| d.code == "E1230"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_trait_bounded_generic_accepts_multiple_impl_types() {
    let src = r#"
trait Order[T];
impl Order[Int];
impl Order[Bool];

fn pick[T: Order](a: T, b: T) -> T {
    a
}

fn as_int(v: Bool) -> Int {
    match v {
        true => 1,
        false => 0,
    }
}

fn demo() -> Int {
    let x = pick(41, 42);
    let y = pick(true, false);
    x + as_int(y)
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !out.diagnostics.iter().any(|d| d.code == "E1258"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_trait_bounded_generic_reports_missing_impl() {
    let src = r#"
trait Order[T];
impl Order[Int];

fn pick[T: Order](a: T, b: T) -> T {
    a
}

fn demo() -> Bool {
    pick(true, false)
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1258"));
}

#[test]
fn unit_trait_method_static_dispatch_generic_call_typechecks() {
    let src = r#"
trait Score[T] {
    fn score(self: T) -> Int;
}

struct Meter { value: Int }

impl Score[Meter] {
    fn score(self: Meter) -> Int {
        self.value + 1
    }
}

fn eval[T: Score](x: T) -> Int {
    x.score()
}

fn main() -> Int {
    eval(Meter { value: 41 })
}
"#;
    let (program, parse_diags) = parse(src, "unit.aic");
    assert!(
        parse_diags.is_empty(),
        "parse diagnostics: {parse_diags:#?}"
    );
    let ir = build(&program.expect("program"));
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
}

#[test]
fn unit_dyn_trait_object_safety_rejects_trait_generics() {
    let src = r#"
trait Score[T] {
    fn score(self: T) -> Int;
}

struct Meter { value: Int }

impl Score[Meter] {
    fn score(self: Meter) -> Int {
        self.value
    }
}

fn main() -> Int {
    let _h: dyn Score = Meter { value: 1 };
    0
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        out.diagnostics.iter().any(|d| {
            d.code == "E1214"
                && d.message.contains("object-safe")
                && d.message.contains("trait generics")
        }),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_dyn_trait_object_safety_rejects_self_in_return_type() {
    let src = r#"
trait Cloneable {
    fn clone(self: Self) -> Self;
}

struct Item { value: Int }

impl Cloneable[Item] {
    fn clone(self: Item) -> Item {
        self
    }
}

fn main() -> Int {
    let _h: dyn Cloneable = Item { value: 1 };
    0
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        out.diagnostics.iter().any(|d| {
            d.code == "E1214"
                && d.message.contains("object-safe")
                && d.message.contains("return type")
        }),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_trait_method_impl_missing_required_method_reports_diagnostic() {
    let src = r#"
trait Score[T] {
    fn score(self: T) -> Int;
}

struct Meter { value: Int }

impl Score[Meter] {
}
"#;
    let (program, parse_diags) = parse(src, "unit.aic");
    assert!(
        parse_diags.is_empty(),
        "parse diagnostics: {parse_diags:#?}"
    );
    let ir = build(&program.expect("program"));
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    let mut all_diags = resolve_diags;
    all_diags.extend(out.diagnostics);
    assert!(
        all_diags.iter().any(|d| {
            d.message.contains("missing")
                && d.message.contains("method")
                && d.message.contains("score")
        }),
        "diags={all_diags:#?}"
    );
}

#[test]
fn unit_trait_method_impl_signature_mismatch_reports_diagnostic() {
    let src = r#"
trait Score[T] {
    fn score(self: T) -> Int;
}

struct Meter { value: Int }

impl Score[Meter] {
    fn score(self: Meter) -> Bool {
        true
    }
}
"#;
    let (program, parse_diags) = parse(src, "unit.aic");
    assert!(
        parse_diags.is_empty(),
        "parse diagnostics: {parse_diags:#?}"
    );
    let ir = build(&program.expect("program"));
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    let mut all_diags = resolve_diags;
    all_diags.extend(out.diagnostics);
    assert!(
        all_diags.iter().any(|d| {
            d.message.contains("score")
                && (d.message.contains("signature")
                    || d.message.contains("mismatch")
                    || d.message.contains("return"))
        }),
        "diags={all_diags:#?}"
    );
}

#[test]
fn unit_drop_trait_surface_with_self_receiver_typechecks() {
    let src = r#"
trait Drop[T] {
    fn drop(self) -> ();
}

struct Probe { id: Int }

impl Drop[Probe] {
    fn drop(self) -> () {
        let _id = self.id;
        ()
    }
}

fn main() -> Int {
    let probe = Probe { id: 41 };
    probe.id
}
"#;
    let (program, parse_diags) = parse(src, "unit.aic");
    assert!(
        parse_diags.is_empty(),
        "parse diagnostics: {parse_diags:#?}"
    );
    let ir = build(&program.expect("program"));
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
}

#[test]
fn unit_drop_trait_impl_missing_drop_reports_diagnostic() {
    let src = r#"
trait Drop[T] {
    fn drop(self) -> ();
}

struct Probe { id: Int }

impl Drop[Probe] {
}
"#;
    let (program, parse_diags) = parse(src, "unit.aic");
    assert!(
        parse_diags.is_empty(),
        "parse diagnostics: {parse_diags:#?}"
    );
    let ir = build(&program.expect("program"));
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    let mut all_diags = resolve_diags;
    all_diags.extend(out.diagnostics);
    assert!(
        all_diags
            .iter()
            .any(|d| d.message.contains("Drop") && d.message.contains("drop")),
        "diags={all_diags:#?}"
    );
}

#[test]
fn unit_drop_trait_impl_signature_mismatch_reports_diagnostic() {
    let src = r#"
trait Drop[T] {
    fn drop(self) -> ();
}

struct Probe { id: Int }

impl Drop[Probe] {
    fn drop(self, extra: Int) -> () {
        let _extra = extra;
        ()
    }
}
"#;
    let (program, parse_diags) = parse(src, "unit.aic");
    assert!(
        parse_diags.is_empty(),
        "parse diagnostics: {parse_diags:#?}"
    );
    let ir = build(&program.expect("program"));
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    let mut all_diags = resolve_diags;
    all_diags.extend(out.diagnostics);
    assert!(
        all_diags
            .iter()
            .any(|d| d.message.contains("Drop.drop") || d.message.contains("Drop")),
        "diags={all_diags:#?}"
    );
}

#[test]
fn unit_where_clause_multiple_bounds_are_enforced() {
    let src = r#"
trait A[T];
trait B[T];
impl A[Int];
impl B[Int];

fn accept[T](x: T) -> T where T: A + B {
    x
}

fn main() -> Int {
    accept(41)
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
}

#[test]
fn unit_where_clause_missing_bound_reports_e1258() {
    let src = r#"
trait A[T];
trait B[T];
impl A[Int];

fn accept[T](x: T) -> T where T: A + B {
    x
}

fn main() -> Int {
    accept(41)
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1258"));
}

#[test]
fn unit_async_call_requires_await_for_value_use() {
    let src = r#"
async fn ping() -> Int {
    41
}

async fn main() -> Int {
    await ping() + 1
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.code == "E1256" || d.code == "E1257"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_await_outside_async_function_is_rejected() {
    let src = r#"
async fn ping() -> Int { 1 }
fn bad() -> Int { await ping() }
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1256"));
}

#[test]
fn unit_await_non_async_value_is_rejected() {
    let src = r#"
async fn main() -> Int {
    await 1
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1257"));
}

#[test]
fn unit_await_net_async_submit_bridge_typechecks() {
    let src = r#"
enum NetError {
    NotFound,
    Timeout,
    Io,
}

struct AsyncIntOp {
    handle: Int,
}

fn async_accept_submit(listener: Int, timeout_ms: Int) -> Result[AsyncIntOp, NetError] {
    if timeout_ms > 0 {
        Ok(AsyncIntOp { handle: listener + timeout_ms })
    } else {
        Err(Timeout)
    }
}

async fn main() -> Int {
    let accepted = await async_accept_submit(0, 10);
    let a = match accepted {
        Ok(_) => 1,
        Err(_) => 1,
    };
    a
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.code == "E1256" || d.code == "E1257"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_fn_type_and_closure_typecheck() {
    let src = r#"
fn apply(f: Fn(Int) -> Int, x: Int) -> Int {
    f(x)
}

fn main() -> Int {
    let inc = |x: Int| -> Int { x + 1 };
    apply(inc, 41)
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
}

#[test]
fn unit_closure_parameter_requires_explicit_type() {
    let src = r#"
fn apply(f: Fn(Int) -> Int, x: Int) -> Int {
    f(x)
}

fn main() -> Int {
    let inc = |x| -> Int { x + 1 };
    apply(inc, 41)
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        out.diagnostics.iter().any(|d| d.code == "E1280"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_closure_parameter_is_inferred_from_fn_context() {
    let src = r#"
fn apply(f: Fn(Int) -> Int, x: Int) -> Int {
    f(x)
}

fn main() -> Int {
    apply(|x| -> Int { x + 1 }, 41)
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
}

#[test]
fn unit_zero_arg_closure_typechecks() {
    let src = r#"
fn apply(f: Fn() -> Int) -> Int {
    f()
}

fn main() -> Int {
    let one = || -> Int { 1 };
    apply(one)
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
}

#[test]
fn unit_break_outside_loop_is_rejected() {
    let src = r#"
fn bad() -> Int {
    break 1
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1275"));
}

#[test]
fn unit_continue_outside_loop_is_rejected() {
    let src = r#"
fn bad() -> Int {
    continue
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1276"));
}

#[test]
fn unit_while_condition_must_be_bool() {
    let src = r#"
fn bad() -> Int {
    while 1 {
        ()
    };
    0
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1273"));
}

#[test]
fn unit_loop_break_type_mismatch_is_rejected() {
    let src = r#"
fn bad() -> Int {
    let _x = loop {
        if true {
            break 1
        } else {
            break false
        }
    };
    0
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1274"));
}

#[test]
fn unit_for_range_loop_infers_binding_type_and_allows_control_flow() {
    let src = r#"
fn sum(n: Int) -> Int {
    let mut total = 0;
    for i in 0..n {
        if i == 2 {
            continue;
        } else {
            ()
        };
        if i == 7 {
            break;
        } else {
            ()
        };
        total = total + i;
    };
    total
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);
}

#[test]
fn unit_for_range_function_loop_infers_int_binding() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
import std.vec;

fn sum(n: Int) -> Int {
    let mut total = 0;
    for i in range(0, n) {
        total = total + i;
    };
    total
}
"#,
    )
    .expect("write source");
    let front = run_frontend(&path).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diags={:#?}",
        front.diagnostics
    );
}

#[test]
fn unit_template_literal_requires_string_interpolation_values() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
import std.string;

fn main(age: Int) -> String {
    f"age={age}"
}
"#,
    )
    .expect("write source");
    let front = run_frontend(&path).expect("frontend");
    assert!(
        has_errors(&front.diagnostics),
        "diags={:#?}",
        front.diagnostics
    );
    assert!(
        front
            .diagnostics
            .iter()
            .any(|d| d.message.contains("expected 'String'") && d.message.contains("found 'Int'")),
        "diags={:#?}",
        front.diagnostics
    );
}
#[test]
fn unit_for_vec_loop_binding_type_is_checked() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
import std.vec;

fn bad(v: Vec[Int]) -> Int {
    for item in v {
        let item_bool: Bool = item;
    };
    0
}
"#,
    )
    .expect("write source");
    let front = run_frontend(&path).expect("frontend");
    assert!(
        front.diagnostics.iter().any(|d| d.code == "E1204"),
        "diags={:#?}",
        front.diagnostics
    );
}

#[test]
fn unit_extern_call_requires_explicit_unsafe_boundary() {
    let src = r#"
extern "C" fn c_abs(x: Int) -> Int;

fn wrap(x: Int) -> Int {
    c_abs(x)
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    let Some(diag) = out.diagnostics.iter().find(|d| d.code == "E2122") else {
        panic!("expected E2122, got {:#?}", out.diagnostics);
    };
    assert!(
        diag.help.iter().any(|h| h.contains("unsafe { ... }")),
        "expected unsafe fix hint in E2122 help, got {diag:#?}"
    );
}

#[test]
fn unit_extern_call_inside_unsafe_block_is_allowed() {
    let src = r#"
extern "C" fn c_abs(x: Int) -> Int;

fn wrap(x: Int) -> Int {
    unsafe { c_abs(x) }
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !out.diagnostics.iter().any(|d| d.code == "E2122"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_extern_abi_mismatch_reports_e2120() {
    let src = r#"
extern "Rust" fn c_abs(x: Int) -> Int;
fn wrap(x: Int) -> Int {
    unsafe { c_abs(x) }
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    let Some(diag) = out.diagnostics.iter().find(|d| d.code == "E2120") else {
        panic!("expected E2120, got {:#?}", out.diagnostics);
    };
    assert!(
        diag.help.iter().any(|h| h.contains("extern \"C\"")),
        "expected fix hint in E2120 help, got {diag:#?}"
    );
}

#[test]
fn unit_extern_signature_rejects_non_c_abi_types() {
    let src = r#"
extern "C" fn c_strlen(s: String) -> Int;
fn wrap(s: String) -> Int {
    unsafe { c_strlen(s) }
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2123"));
}

#[test]
fn unit_unsafe_fn_call_requires_explicit_unsafe_boundary() {
    let src = r#"
unsafe fn unchecked_add_one(x: Int) -> Int {
    x + 1
}

fn wrap(x: Int) -> Int {
    unchecked_add_one(x)
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2122"));
}

#[test]
fn unit_unsafe_fn_call_inside_unsafe_block_is_allowed() {
    let src = r#"
unsafe fn unchecked_add_one(x: Int) -> Int {
    x + 1
}

fn wrap(x: Int) -> Int {
    unsafe { unchecked_add_one(x) }
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !out.diagnostics.iter().any(|d| d.code == "E2122"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_result_propagation_accepts_matching_result_types() {
    let src = r#"
fn parse_num(x: Int) -> Result[Int, Int] {
    Ok(x)
}

fn bump(x: Int) -> Result[Int, Int] {
    let v = parse_num(x)?;
    if true { Ok(v + 1) } else { Err(0) }
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.code == "E1260" || d.code == "E1261" || d.code == "E1262"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_result_propagation_rejects_non_result_operand() {
    let src = r#"
fn bad() -> Result[Int, Int] {
    let x = 1?;
    Ok(x)
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1260"));
}

#[test]
fn unit_result_propagation_requires_result_return_type() {
    let src = r#"
fn parse_num(x: Int) -> Result[Int, Int] {
    Ok(x)
}

fn bad(x: Int) -> Int {
    parse_num(x)?
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1261"));
}

#[test]
fn unit_result_propagation_reports_error_type_mismatch() {
    let src = r#"
fn parse_num(x: Int) -> Result[Int, Bool] {
    if x > 0 { Ok(x) } else { Err(false) }
}

fn bad(x: Int) -> Result[Int, Int] {
    let v = parse_num(x)?;
    Ok(v)
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1262"));
}

#[test]
fn unit_mutable_assignment_succeeds_for_mut_binding() {
    let src = r#"
fn main() -> Int {
    let mut x = 1;
    x = x + 1;
    x
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.code == "E1266" || d.code == "E1269"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_assignment_to_immutable_binding_reports_e1266() {
    let src = r#"
fn main() -> Int {
    let x = 1;
    x = 2;
    x
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1266"));
}

#[test]
fn unit_assignment_type_mismatch_reports_e1269() {
    let src = r#"
fn main() -> Int {
    let mut x = 1;
    x = true;
    x
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1269"));
}

#[test]
fn unit_conflicting_mutable_borrow_reports_e1263() {
    let src = r#"
fn main() -> Int {
    let mut x = 1;
    let a = &mut x;
    let b = &mut x;
    x
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1263"));
}

#[test]
fn unit_shared_borrow_while_mutable_borrow_active_reports_e1264() {
    let src = r#"
fn main() -> Int {
    let mut x = 1;
    let m = &mut x;
    let s = &x;
    x
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1264"));
}

#[test]
fn unit_assignment_while_borrowed_reports_e1265() {
    let src = r#"
fn main() -> Int {
    let mut x = 1;
    let r = &x;
    x = 2;
    x
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1265"));
}

#[test]
fn unit_mutable_borrow_of_immutable_binding_reports_e1267() {
    let src = r#"
fn main() -> Int {
    let x = 1;
    let r = &mut x;
    x
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1267"));
}

#[test]
fn unit_borrow_target_must_be_local_variable_e1268() {
    let src = r#"
fn main() -> Int {
    let r = &(1 + 2);
    0
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1268"));
}

#[test]
fn unit_borrow_scope_does_not_leak_from_branch() {
    let src = r#"
fn main() -> Int {
    let mut x = 1;
    if true {
        let r = &x;
        0
    } else {
        0
    };
    x = 2;
    x
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !out.diagnostics.iter().any(|d| d.code == "E1265"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_use_after_move_reports_e1270() {
    let src = r#"
struct BoxedInt { value: Int }

fn main() -> Int {
    let b = BoxedInt { value: 1 };
    let moved = b;
    b.value
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1270"));
}

#[test]
fn unit_move_while_borrowed_reports_e1271() {
    let src = r#"
struct BoxedInt { value: Int }

fn main() -> Int {
    let b = BoxedInt { value: 1 };
    let keep = &b;
    let moved = b;
    moved.value
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1271"));
}

#[test]
fn unit_field_borrow_conflict_reports_e1263() {
    let src = r#"
struct Pair { left: Int, right: Int }

fn main() -> Int {
    let mut pair = Pair { left: 1, right: 2 };
    let left_ref = &pair.left;
    let whole_mut = &mut pair;
    pair.right
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1263"));
}

#[test]
fn unit_assignment_while_field_borrowed_reports_e1265() {
    let src = r#"
struct Pair { left: Int, right: Int }

fn main() -> Int {
    let mut pair = Pair { left: 1, right: 2 };
    let left_ref = &pair.left;
    pair = Pair { left: 3, right: 4 };
    pair.right
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1265"));
}

#[test]
fn unit_reinitialize_after_move_is_allowed() {
    let src = r#"
struct BoxedInt { value: Int }

fn main() -> Int {
    let mut b = BoxedInt { value: 1 };
    let moved = b;
    let first = moved.value;
    b = BoxedInt { value: first + 1 };
    b.value
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !out.diagnostics.iter().any(|d| matches!(
            d.code.as_str(),
            "E1263" | "E1264" | "E1265" | "E1270" | "E1271"
        )),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_call_boundary_conflicting_borrows_report_e1263() {
    let src = r#"
fn takes_mut(x: RefMut[Int]) -> Int {
    0
}

fn main() -> Int {
    let mut x = 1;
    let keep = &x;
    takes_mut(&mut x)
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1263"));
}

#[test]
fn unit_typecheck_non_exhaustive_option_match() {
    let src = r#"
fn f(x: Option[Int]) -> Int {
    match x {
        Some(v) => v,
    }
}
"#;
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1247"));
}

#[test]
fn unit_contract_must_be_bool() {
    let src = "fn f() -> Int ensures 1 { 1 }";
    let ir = lower(src);
    let (res, _) = resolve(&ir, "unit.aic");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1201"));
}

#[test]
fn unit_effect_decl_unknown() {
    let src = "fn f() -> () effects { weird } { () }";
    let ir = lower(src);
    let diags = check_effect_declarations(&ir, "unit.aic");
    assert!(diags.iter().any(|d| d.code == "E2003"));
}

#[test]
fn unit_effect_decl_duplicate() {
    let src = "fn f() -> () effects { io, io } { () }";
    let ir = lower(src);
    let diags = check_effect_declarations(&ir, "unit.aic");
    assert!(diags.iter().any(|d| d.code == "E2004"));
}

#[test]
fn unit_frontend_canonicalizes_effect_signature_order() {
    let dir = tempdir().expect("tempdir");
    let source_path = dir.path().join("main.aic");
    fs::write(
        &source_path,
        r#"
fn main() -> Int effects { time, io, fs } capabilities { time, io, fs } {
    0
}
"#,
    )
    .expect("write source");

    let out = run_frontend(&source_path).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics={:#?}",
        out.diagnostics
    );
    let func = out
        .ir
        .items
        .iter()
        .find_map(|item| match item {
            aicore::ir::Item::Function(func) if func.name == "main" => Some(func),
            _ => None,
        })
        .expect("main function");
    assert_eq!(
        func.effects,
        vec!["fs".to_string(), "io".to_string(), "time".to_string()]
    );
}

#[test]
fn unit_frontend_reports_transitive_effect_path() {
    let dir = tempdir().expect("tempdir");
    let source_path = dir.path().join("main.aic");
    fs::write(
        &source_path,
        r#"
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
"#,
    )
    .expect("write source");

    let out = run_frontend(&source_path).expect("frontend");
    let diag = out
        .diagnostics
        .iter()
        .find(|d| d.code == "E2005")
        .expect("missing E2005");
    assert!(diag.message.contains("top -> middle -> leaf"));
}

#[test]
fn unit_contract_static_false() {
    let src = "fn f() -> Int requires false { 1 }";
    let ir = lower(src);
    let diags = verify_static(&ir, "unit.aic");
    assert!(diags.iter().any(|d| d.code == "E4001"));
}

#[test]
fn unit_formatter_is_stable() {
    let src = "fn f(x: Int) -> Int { x + 1 }";
    let ir = lower(src);
    let a = format_program(&ir);
    let b = format_program(&ir);
    assert_eq!(a, b);
}

#[test]
fn unit_formatter_preserves_named_call_arguments() {
    let src = r#"
fn connect(host: Int, port: Int, timeout_ms: Int, retry: Bool) -> Int {
    if retry { host + port + timeout_ms } else { 0 }
}

fn main() -> Int {
    connect(timeout_ms: 30, retry: true, host: 10, port: 2)
}
"#;
    let ir = lower(src);
    let formatted = format_program(&ir);
    assert!(
        formatted.contains("connect(timeout_ms: 30, retry: true, host: 10, port: 2)"),
        "formatted={formatted}"
    );
    let _ = lower(&formatted);
}

#[test]
fn unit_ir_interns_single_int_type() {
    let src = "fn f(x: Int) -> Int { x } fn g(y: Int) -> Int { y }";
    let ir = lower(src);
    let count = ir.types.iter().filter(|t| t.repr == "Int").count();
    assert_eq!(count, 1);
}

#[test]
fn unit_syntax_showcase_parses_cleanly() {
    let path = Path::new("examples/e1/syntax_showcase.aic");
    let source = fs::read_to_string(path).expect("read syntax showcase");
    let (program, diags) = parse(&source, &path.to_string_lossy());
    assert!(diags.is_empty(), "diags={diags:#?}");
    assert!(program.is_some());
}

#[test]
fn unit_undocumented_function_form_fails_with_stable_code() {
    // Return type arrow is mandatory in frozen grammar v1.
    let src = "fn missing_arrow() { 0 }";
    let (_program, diags) = parse(src, "unit.aic");
    assert!(diags.iter().any(|d| d.code == "E1006"), "diags={diags:#?}");
}

#[test]
fn unit_ir_ids_are_stable_after_format_roundtrip() {
    let src = r#"
fn pick(x: Int, y: Int) -> Int {
    let z = if x > y { x } else { y };
    z
}
"#;
    let ir1 = lower(src);
    let canonical = format_program(&ir1);
    let ir2 = lower(&canonical);

    assert_eq!(symbol_ids(&ir1), symbol_ids(&ir2));
    assert_eq!(type_ids(&ir1), type_ids(&ir2));
}

#[test]
fn unit_symbol_ids_are_dense_from_one() {
    let src = r#"
fn alpha(a: Int) -> Int { let x = a; x }
fn beta(b: Int) -> Int { let y = b; y }
"#;
    let ir = lower(src);
    let ids = symbol_ids(&ir);
    let expected: Vec<u32> = (1..=ids.len() as u32).collect();
    assert_eq!(ids, expected);
}

#[test]
fn unit_formatter_idempotent_for_syntax_showcase() {
    let path = Path::new("examples/e1/syntax_showcase.aic");
    let source = fs::read_to_string(path).expect("read showcase");
    let ir = lower(&source);
    let once = format_program(&ir);
    let ir2 = lower(&once);
    let twice = format_program(&ir2);
    assert_eq!(once, twice);
}

#[test]
fn unit_init_project_emits_canonical_source() {
    let dir = tempdir().expect("tempdir");
    init_project(dir.path()).expect("init project");
    assert!(
        !dir.path().join("std").exists(),
        "init should no longer copy std into each project"
    );
    let main = dir.path().join("src/main.aic");
    let source = fs::read_to_string(&main).expect("read main");
    let ir = lower(&source);
    let formatted = format_program(&ir);
    assert_eq!(source, formatted, "init project source must be canonical");
}

#[test]
fn unit_std_io_public_apis_delegate_to_runtime_intrinsics() {
    let io_source = fs::read_to_string("std/io.aic").expect("read std/io.aic");

    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "print_int",
        "aic_io_print_int_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "print_str",
        "aic_io_print_str_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "print_float",
        "aic_io_print_float_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "read_line",
        "aic_io_read_line_intrinsic",
        0,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "read_int",
        "aic_io_read_int_intrinsic",
        0,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "read_char",
        "aic_io_read_char_intrinsic",
        0,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "prompt",
        "aic_io_prompt_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "eprint_str",
        "aic_io_eprint_str_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "eprint_int",
        "aic_io_eprint_int_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "println_str",
        "aic_io_println_str_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "println_int",
        "aic_io_println_int_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "print_bool",
        "aic_io_print_bool_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "println_bool",
        "aic_io_println_bool_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "flush_stdout",
        "aic_io_flush_stdout_intrinsic",
        0,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "flush_stderr",
        "aic_io_flush_stderr_intrinsic",
        0,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "panic",
        "aic_io_panic_intrinsic",
        1,
    );
    assert_delegate_call(
        &io_source,
        "std/io.aic",
        "install_mock_reader",
        "aic_io_mock_reader_install_intrinsic",
        1,
    );
    assert!(
        io_source.contains("aic_io_mock_writer_take_stdout_intrinsic"),
        "std/io.aic must expose stdout mock capture intrinsic"
    );
    assert!(
        io_source.contains("aic_io_mock_writer_take_stderr_intrinsic"),
        "std/io.aic must expose stderr mock capture intrinsic"
    );
}

#[test]
fn unit_std_fs_public_apis_delegate_to_runtime_intrinsics() {
    let fs_source = fs::read_to_string("std/fs.aic").expect("read std/fs.aic");

    assert_delegate_call(
        &fs_source,
        "std/fs.aic",
        "exists",
        "aic_fs_exists_intrinsic",
        1,
    );
    assert_delegate_call(
        &fs_source,
        "std/fs.aic",
        "read_text",
        "aic_fs_read_text_intrinsic",
        1,
    );
    assert_delegate_call(
        &fs_source,
        "std/fs.aic",
        "write_text",
        "aic_fs_write_text_intrinsic",
        2,
    );
    assert_delegate_call(
        &fs_source,
        "std/fs.aic",
        "append_text",
        "aic_fs_append_text_intrinsic",
        2,
    );
    assert_delegate_call(&fs_source, "std/fs.aic", "copy", "aic_fs_copy_intrinsic", 2);
    assert_delegate_call(&fs_source, "std/fs.aic", "move", "aic_fs_move_intrinsic", 2);
    assert_delegate_call(
        &fs_source,
        "std/fs.aic",
        "delete",
        "aic_fs_delete_intrinsic",
        1,
    );
    assert_delegate_call(
        &fs_source,
        "std/fs.aic",
        "metadata",
        "aic_fs_metadata_intrinsic",
        1,
    );
    assert_delegate_call(
        &fs_source,
        "std/fs.aic",
        "walk_dir",
        "aic_fs_walk_dir_intrinsic",
        1,
    );
    assert_delegate_call(
        &fs_source,
        "std/fs.aic",
        "temp_file",
        "aic_fs_temp_file_intrinsic",
        1,
    );
    assert_delegate_call(
        &fs_source,
        "std/fs.aic",
        "temp_dir",
        "aic_fs_temp_dir_intrinsic",
        1,
    );
}

#[test]
fn unit_std_env_public_apis_delegate_to_runtime_intrinsics() {
    let env_source = fs::read_to_string("std/env.aic").expect("read std/env.aic");

    assert_delegate_call(
        &env_source,
        "std/env.aic",
        "get",
        "aic_env_get_intrinsic",
        1,
    );
    assert_delegate_call(
        &env_source,
        "std/env.aic",
        "set",
        "aic_env_set_intrinsic",
        2,
    );
    assert_delegate_call(
        &env_source,
        "std/env.aic",
        "remove",
        "aic_env_remove_intrinsic",
        1,
    );
    assert_delegate_call(
        &env_source,
        "std/env.aic",
        "cwd",
        "aic_env_cwd_intrinsic",
        0,
    );
    assert_delegate_call(
        &env_source,
        "std/env.aic",
        "set_cwd",
        "aic_env_set_cwd_intrinsic",
        1,
    );
}

#[test]
fn unit_std_map_public_apis_delegate_to_runtime_intrinsics() {
    let map_source = fs::read_to_string("std/map.aic").expect("read std/map.aic");

    assert_delegate_call(
        &map_source,
        "std/map.aic",
        "new_map",
        "aic_map_new_intrinsic",
        0,
    );
    assert_delegate_call(
        &map_source,
        "std/map.aic",
        "insert",
        "aic_map_insert_intrinsic",
        3,
    );
    assert_delegate_call(
        &map_source,
        "std/map.aic",
        "get",
        "aic_map_get_intrinsic",
        2,
    );
    assert_delegate_call(
        &map_source,
        "std/map.aic",
        "contains_key",
        "aic_map_contains_key_intrinsic",
        2,
    );
    assert_delegate_call(
        &map_source,
        "std/map.aic",
        "remove",
        "aic_map_remove_intrinsic",
        2,
    );
    assert_delegate_call(
        &map_source,
        "std/map.aic",
        "size",
        "aic_map_size_intrinsic",
        1,
    );
    assert_delegate_call(
        &map_source,
        "std/map.aic",
        "keys",
        "aic_map_keys_intrinsic",
        1,
    );
    assert_delegate_call(
        &map_source,
        "std/map.aic",
        "values",
        "aic_map_values_intrinsic",
        1,
    );
    assert_delegate_call(
        &map_source,
        "std/map.aic",
        "entries",
        "aic_map_entries_intrinsic",
        1,
    );
}

#[test]
fn unit_std_vec_public_apis_delegate_to_runtime_intrinsics() {
    let vec_source = fs::read_to_string("std/vec.aic").expect("read std/vec.aic");

    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "new_vec",
        "aic_vec_new_intrinsic",
        0,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "new_vec_with_capacity",
        "aic_vec_new_with_capacity_intrinsic",
        1,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "vec_of",
        "aic_vec_of_intrinsic",
        1,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "get",
        "aic_vec_get_intrinsic",
        2,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "first",
        "aic_vec_first_intrinsic",
        1,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "last",
        "aic_vec_last_intrinsic",
        1,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "push",
        "aic_vec_push_intrinsic",
        2,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "pop",
        "aic_vec_pop_intrinsic",
        1,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "set",
        "aic_vec_set_intrinsic",
        3,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "insert",
        "aic_vec_insert_intrinsic",
        3,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "remove_at",
        "aic_vec_remove_at_intrinsic",
        2,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "contains",
        "aic_vec_contains_intrinsic",
        2,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "index_of",
        "aic_vec_index_of_intrinsic",
        2,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "reverse",
        "aic_vec_reverse_intrinsic",
        1,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "slice",
        "aic_vec_slice_intrinsic",
        3,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "append",
        "aic_vec_append_intrinsic",
        2,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "clear",
        "aic_vec_clear_intrinsic",
        1,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "reserve",
        "aic_vec_reserve_intrinsic",
        2,
    );
    assert_delegate_call(
        &vec_source,
        "std/vec.aic",
        "shrink_to_fit",
        "aic_vec_shrink_to_fit_intrinsic",
        1,
    );
}

#[test]
fn unit_std_path_public_apis_delegate_to_runtime_intrinsics() {
    let path_source = fs::read_to_string("std/path.aic").expect("read std/path.aic");

    assert_delegate_call(
        &path_source,
        "std/path.aic",
        "join",
        "aic_path_join_intrinsic",
        2,
    );
    assert_delegate_call(
        &path_source,
        "std/path.aic",
        "basename",
        "aic_path_basename_intrinsic",
        1,
    );
    assert_delegate_call(
        &path_source,
        "std/path.aic",
        "dirname",
        "aic_path_dirname_intrinsic",
        1,
    );
    assert_delegate_call(
        &path_source,
        "std/path.aic",
        "extension",
        "aic_path_extension_intrinsic",
        1,
    );
    assert_delegate_call(
        &path_source,
        "std/path.aic",
        "is_abs",
        "aic_path_is_abs_intrinsic",
        1,
    );
}

#[test]
fn unit_std_proc_public_apis_delegate_to_runtime_intrinsics() {
    let proc_source = fs::read_to_string("std/proc.aic").expect("read std/proc.aic");

    assert_delegate_call(
        &proc_source,
        "std/proc.aic",
        "spawn",
        "aic_proc_spawn_intrinsic",
        1,
    );
    assert_delegate_call(
        &proc_source,
        "std/proc.aic",
        "wait",
        "aic_proc_wait_intrinsic",
        1,
    );
    assert_delegate_call(
        &proc_source,
        "std/proc.aic",
        "kill",
        "aic_proc_kill_intrinsic",
        1,
    );
    assert_delegate_call(
        &proc_source,
        "std/proc.aic",
        "run",
        "aic_proc_run_intrinsic",
        1,
    );
    assert_delegate_call(
        &proc_source,
        "std/proc.aic",
        "pipe",
        "aic_proc_pipe_intrinsic",
        2,
    );

    for (name, arity) in [
        ("aic_proc_spawn_intrinsic", 1usize),
        ("aic_proc_wait_intrinsic", 1usize),
        ("aic_proc_kill_intrinsic", 1usize),
        ("aic_proc_run_intrinsic", 1usize),
        ("aic_proc_pipe_intrinsic", 2usize),
        ("aic_proc_run_with_intrinsic", 2usize),
        ("aic_proc_is_running_intrinsic", 1usize),
        ("aic_proc_current_pid_intrinsic", 0usize),
        ("aic_proc_run_timeout_intrinsic", 2usize),
        ("aic_proc_pipe_chain_intrinsic", 1usize),
    ] {
        assert_intrinsic_declaration(&proc_source, "std/proc.aic", name, arity);
    }
}
#[test]
fn unit_std_net_public_apis_delegate_to_runtime_intrinsics() {
    let net_source = fs::read_to_string("std/net.aic").expect("read std/net.aic");

    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_listen",
        "aic_net_tcp_listen_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_local_addr",
        "aic_net_tcp_local_addr_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_accept",
        "aic_net_tcp_accept_intrinsic",
        2,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_connect",
        "aic_net_tcp_connect_intrinsic",
        2,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_send",
        "aic_net_tcp_send_intrinsic",
        2,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_close",
        "aic_net_tcp_close_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_set_nodelay",
        "aic_net_tcp_set_nodelay_intrinsic",
        2,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_get_nodelay",
        "aic_net_tcp_get_nodelay_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_set_keepalive",
        "aic_net_tcp_set_keepalive_intrinsic",
        2,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_get_keepalive",
        "aic_net_tcp_get_keepalive_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_set_send_buffer_size",
        "aic_net_tcp_set_send_buffer_size_intrinsic",
        2,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_get_send_buffer_size",
        "aic_net_tcp_get_send_buffer_size_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_set_recv_buffer_size",
        "aic_net_tcp_set_recv_buffer_size_intrinsic",
        2,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "tcp_get_recv_buffer_size",
        "aic_net_tcp_get_recv_buffer_size_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "udp_bind",
        "aic_net_udp_bind_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "udp_local_addr",
        "aic_net_udp_local_addr_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "udp_send_to",
        "aic_net_udp_send_to_intrinsic",
        3,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "udp_recv_from",
        "aic_net_udp_recv_from_intrinsic",
        3,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "udp_close",
        "aic_net_udp_close_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "dns_lookup",
        "aic_net_dns_lookup_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "dns_reverse",
        "aic_net_dns_reverse_intrinsic",
        1,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "async_accept_submit",
        "aic_net_async_accept_submit_intrinsic",
        2,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "async_tcp_send_submit",
        "aic_net_async_send_submit_intrinsic",
        2,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "async_tcp_recv_submit",
        "aic_net_async_recv_submit_intrinsic",
        3,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "async_wait_int",
        "aic_net_async_wait_int_intrinsic",
        2,
    );
    assert_delegate_call(
        &net_source,
        "std/net.aic",
        "async_shutdown",
        "aic_net_async_shutdown_intrinsic",
        0,
    );

    for (name, arity) in [
        ("aic_net_tcp_listen_intrinsic", 1usize),
        ("aic_net_tcp_local_addr_intrinsic", 1usize),
        ("aic_net_tcp_accept_intrinsic", 2usize),
        ("aic_net_tcp_connect_intrinsic", 2usize),
        ("aic_net_tcp_send_intrinsic", 2usize),
        ("aic_net_tcp_recv_intrinsic", 3usize),
        ("aic_net_tcp_close_intrinsic", 1usize),
        ("aic_net_tcp_set_nodelay_intrinsic", 2usize),
        ("aic_net_tcp_get_nodelay_intrinsic", 1usize),
        ("aic_net_tcp_set_keepalive_intrinsic", 2usize),
        ("aic_net_tcp_get_keepalive_intrinsic", 1usize),
        ("aic_net_tcp_set_send_buffer_size_intrinsic", 2usize),
        ("aic_net_tcp_get_send_buffer_size_intrinsic", 1usize),
        ("aic_net_tcp_set_recv_buffer_size_intrinsic", 2usize),
        ("aic_net_tcp_get_recv_buffer_size_intrinsic", 1usize),
        ("aic_net_udp_bind_intrinsic", 1usize),
        ("aic_net_udp_local_addr_intrinsic", 1usize),
        ("aic_net_udp_send_to_intrinsic", 3usize),
        ("aic_net_udp_recv_from_intrinsic", 3usize),
        ("aic_net_udp_close_intrinsic", 1usize),
        ("aic_net_dns_lookup_intrinsic", 1usize),
        ("aic_net_dns_reverse_intrinsic", 1usize),
        ("aic_net_async_accept_submit_intrinsic", 2usize),
        ("aic_net_async_send_submit_intrinsic", 2usize),
        ("aic_net_async_recv_submit_intrinsic", 3usize),
        ("aic_net_async_wait_int_intrinsic", 2usize),
        ("aic_net_async_wait_string_intrinsic", 2usize),
        ("aic_net_async_shutdown_intrinsic", 0usize),
    ] {
        assert_intrinsic_declaration(&net_source, "std/net.aic", name, arity);
    }
}

#[test]
fn unit_std_net_tcp_stream_adapter_delegates_to_tcp_byte_apis() {
    let net_source = fs::read_to_string("std/net.aic").expect("read std/net.aic");

    assert!(
        net_source.contains("struct TcpStream {"),
        "std/net.aic must expose TcpStream handle wrapper"
    );
    assert!(
        net_source.contains("fn tcp_stream(handle: Int) -> TcpStream"),
        "std/net.aic must expose tcp_stream adapter constructor"
    );
    assert!(
        net_source.contains("fn tcp_stream_send(stream: TcpStream, payload: Bytes) -> Result[Int, NetError] effects { net }"),
        "std/net.aic must expose tcp_stream_send adapter API"
    );
    assert!(
        net_source.contains("fn tcp_stream_recv(stream: TcpStream, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net }"),
        "std/net.aic must expose tcp_stream_recv adapter API"
    );
    assert!(
        net_source.contains(
            "fn tcp_stream_close(stream: TcpStream) -> Result[Bool, NetError] effects { net }"
        ),
        "std/net.aic must expose tcp_stream_close adapter API"
    );
    assert!(
        net_source.contains(
            "fn tcp_stream_set_nodelay(stream: TcpStream, enabled: Bool) -> Result[Bool, NetError] effects { net }"
        ),
        "std/net.aic must expose tcp_stream_set_nodelay adapter API"
    );
    assert!(
        net_source.contains(
            "fn tcp_stream_get_nodelay(stream: TcpStream) -> Result[Bool, NetError] effects { net }"
        ),
        "std/net.aic must expose tcp_stream_get_nodelay adapter API"
    );
    assert!(
        net_source.contains(
            "fn tcp_stream_set_keepalive(stream: TcpStream, enabled: Bool) -> Result[Bool, NetError] effects { net }"
        ),
        "std/net.aic must expose tcp_stream_set_keepalive adapter API"
    );
    assert!(
        net_source.contains(
            "fn tcp_stream_get_keepalive(stream: TcpStream) -> Result[Bool, NetError] effects { net }"
        ),
        "std/net.aic must expose tcp_stream_get_keepalive adapter API"
    );
    assert!(
        net_source.contains(
            "fn tcp_stream_set_send_buffer_size(stream: TcpStream, size_bytes: Int) -> Result[Bool, NetError] effects { net }"
        ),
        "std/net.aic must expose tcp_stream_set_send_buffer_size adapter API"
    );
    assert!(
        net_source.contains(
            "fn tcp_stream_get_send_buffer_size(stream: TcpStream) -> Result[Int, NetError] effects { net }"
        ),
        "std/net.aic must expose tcp_stream_get_send_buffer_size adapter API"
    );
    assert!(
        net_source.contains(
            "fn tcp_stream_set_recv_buffer_size(stream: TcpStream, size_bytes: Int) -> Result[Bool, NetError] effects { net }"
        ),
        "std/net.aic must expose tcp_stream_set_recv_buffer_size adapter API"
    );
    assert!(
        net_source.contains(
            "fn tcp_stream_get_recv_buffer_size(stream: TcpStream) -> Result[Int, NetError] effects { net }"
        ),
        "std/net.aic must expose tcp_stream_get_recv_buffer_size adapter API"
    );
    assert!(
        net_source.contains("tcp_send(stream.handle, payload)"),
        "std/net.aic tcp_stream_send must delegate to tcp_send"
    );
    assert!(
        net_source.contains("tcp_recv(stream.handle, max_bytes, timeout_ms)"),
        "std/net.aic tcp_stream_recv must delegate to tcp_recv"
    );
    assert!(
        net_source.contains("tcp_close(stream.handle)"),
        "std/net.aic tcp_stream_close must delegate to tcp_close"
    );
    assert!(
        net_source.contains("tcp_set_nodelay(stream.handle, enabled)"),
        "std/net.aic tcp_stream_set_nodelay must delegate to tcp_set_nodelay"
    );
    assert!(
        net_source.contains("tcp_get_nodelay(stream.handle)"),
        "std/net.aic tcp_stream_get_nodelay must delegate to tcp_get_nodelay"
    );
    assert!(
        net_source.contains("tcp_set_keepalive(stream.handle, enabled)"),
        "std/net.aic tcp_stream_set_keepalive must delegate to tcp_set_keepalive"
    );
    assert!(
        net_source.contains("tcp_get_keepalive(stream.handle)"),
        "std/net.aic tcp_stream_get_keepalive must delegate to tcp_get_keepalive"
    );
    assert!(
        net_source.contains("tcp_set_send_buffer_size(stream.handle, size_bytes)"),
        "std/net.aic tcp_stream_set_send_buffer_size must delegate to tcp_set_send_buffer_size"
    );
    assert!(
        net_source.contains("tcp_get_send_buffer_size(stream.handle)"),
        "std/net.aic tcp_stream_get_send_buffer_size must delegate to tcp_get_send_buffer_size"
    );
    assert!(
        net_source.contains("tcp_set_recv_buffer_size(stream.handle, size_bytes)"),
        "std/net.aic tcp_stream_set_recv_buffer_size must delegate to tcp_set_recv_buffer_size"
    );
    assert!(
        net_source.contains("tcp_get_recv_buffer_size(stream.handle)"),
        "std/net.aic tcp_stream_get_recv_buffer_size must delegate to tcp_get_recv_buffer_size"
    );
}

#[test]
fn unit_std_net_tcp_stream_exact_and_framed_reads_are_deadline_based() {
    let source = fs::read_to_string("std/net.aic").expect("read std/net.aic");

    assert!(
        source.contains(
            "fn tcp_stream_recv_exact_deadline(stream: TcpStream, expected_bytes: Int, deadline_ms: Int) -> Result[Bytes, NetError] effects { net, time }"
        ),
        "std/net.aic must expose deadline-based exact stream reads"
    );
    assert!(
        source.contains(
            "fn tcp_stream_recv_exact(stream: TcpStream, expected_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net, time }"
        ),
        "std/net.aic must expose timeout-based exact stream reads"
    );
    assert!(
        source.contains("deadline_after_ms(timeout_ms)"),
        "std/net.aic timeout wrappers must build monotonic deadlines"
    );
    assert!(
        source.contains("remaining_ms(deadline_ms)"),
        "std/net.aic deadline APIs must compute per-iteration remaining budget"
    );
    assert!(
        source.contains("fn tcp_stream_frame_len_be(header: Bytes) -> Result[Int, NetError]"),
        "std/net.aic must expose shared 4-byte big-endian frame parser"
    );
    assert!(
        source.contains(
            "fn tcp_stream_recv_framed_deadline(stream: TcpStream, max_frame_bytes: Int, deadline_ms: Int) -> Result[Bytes, NetError] effects { net, time }"
        ),
        "std/net.aic must expose deadline-based framed reads"
    );
    assert!(
        source.contains(
            "fn tcp_stream_recv_framed(stream: TcpStream, max_frame_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net, time }"
        ),
        "std/net.aic must expose timeout-based framed reads"
    );
    assert!(
        source.contains("tcp_stream_recv_exact_deadline(stream, 4, deadline_ms)"),
        "std/net.aic framed reads must consume exact 4-byte frame headers"
    );
    assert!(
        source.contains("tcp_stream_recv_exact_deadline(stream, payload_len, deadline_ms)"),
        "std/net.aic framed reads must consume exact payload lengths"
    );
    assert!(
        source.contains("failure = net_timeout_error();"),
        "std/net.aic exact read deadline path must map budget exhaustion to Timeout"
    );
}

#[test]
fn unit_std_url_public_apis_delegate_to_runtime_intrinsics() {
    let url_source = fs::read_to_string("std/url.aic").expect("read std/url.aic");

    assert_delegate_call(
        &url_source,
        "std/url.aic",
        "parse",
        "aic_url_parse_intrinsic",
        1,
    );
    assert_delegate_call(
        &url_source,
        "std/url.aic",
        "normalize",
        "aic_url_normalize_intrinsic",
        1,
    );
    assert_delegate_call(
        &url_source,
        "std/url.aic",
        "net_addr",
        "aic_url_net_addr_intrinsic",
        1,
    );
}

#[test]
fn unit_std_crypto_public_apis_delegate_to_runtime_intrinsics() {
    let source = fs::read_to_string("std/crypto.aic").expect("read std/crypto.aic");

    assert_delegate_call(
        &source,
        "std/crypto.aic",
        "md5",
        "aic_crypto_md5_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/crypto.aic",
        "md5_bytes",
        "aic_crypto_md5_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/crypto.aic",
        "sha256",
        "aic_crypto_sha256_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/crypto.aic",
        "hmac_sha256",
        "aic_crypto_hmac_sha256_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/crypto.aic",
        "hex_encode",
        "aic_crypto_hex_encode_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/crypto.aic",
        "base64_encode",
        "aic_crypto_base64_encode_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/crypto.aic",
        "secure_eq",
        "aic_crypto_secure_eq_intrinsic",
        2,
    );

    for (intrinsic, arity) in [
        ("aic_crypto_md5_intrinsic", 1usize),
        ("aic_crypto_sha256_intrinsic", 1usize),
        ("aic_crypto_sha256_raw_intrinsic", 1usize),
        ("aic_crypto_hmac_sha256_intrinsic", 2usize),
        ("aic_crypto_hmac_sha256_raw_intrinsic", 2usize),
        ("aic_crypto_pbkdf2_sha256_intrinsic", 4usize),
        ("aic_crypto_hex_encode_intrinsic", 1usize),
        ("aic_crypto_hex_decode_intrinsic", 1usize),
        ("aic_crypto_base64_encode_intrinsic", 1usize),
        ("aic_crypto_base64_decode_intrinsic", 1usize),
        ("aic_crypto_random_bytes_intrinsic", 1usize),
        ("aic_crypto_secure_eq_intrinsic", 2usize),
    ] {
        assert_intrinsic_declaration(&source, "std/crypto.aic", intrinsic, arity);
    }
}

#[test]
fn unit_std_crypto_bytes_apis_bridge_bytes_at_intrinsic_boundary() {
    let source = fs::read_to_string("std/crypto.aic").expect("read std/crypto.aic");

    assert!(
        source.contains("aic_crypto_sha256_raw_intrinsic(data)"),
        "std/crypto.aic sha256_raw must delegate to raw intrinsic"
    );
    assert!(
        source.contains("aic_crypto_hmac_sha256_raw_intrinsic(key.data, message.data)"),
        "std/crypto.aic hmac_sha256_raw must pass Bytes.data into intrinsic boundary"
    );
    assert!(
        source.contains(
            "aic_crypto_pbkdf2_sha256_intrinsic(password, salt.data, iterations, key_length)"
        ),
        "std/crypto.aic pbkdf2_sha256 must pass salt Bytes.data into intrinsic boundary"
    );
    assert!(
        source.contains("aic_crypto_hex_encode_intrinsic(data.data)"),
        "std/crypto.aic hex_encode must pass Bytes.data"
    );
    assert!(
        source.contains("aic_crypto_base64_encode_intrinsic(data.data)"),
        "std/crypto.aic base64_encode must pass Bytes.data"
    );
    assert!(
        source.contains("data: aic_crypto_random_bytes_intrinsic(count)"),
        "std/crypto.aic random_bytes must bridge runtime bytes into Bytes wrapper"
    );
    assert!(
        source.contains("fn random_bytes(count: Int) -> Bytes effects { rand }"),
        "std/crypto.aic random_bytes must require rand effect"
    );
    assert!(
        source.contains("aic_crypto_secure_eq_intrinsic(a.data, b.data)"),
        "std/crypto.aic secure_eq must compare raw byte payloads"
    );
    assert!(
        !source.contains("fn aic_crypto_md5_intrinsic(data: String) -> String {"),
        "std/crypto.aic intrinsic declarations must remain declaration-only"
    );
}

#[test]
fn unit_std_tls_public_apis_delegate_to_runtime_intrinsics() {
    let source = fs::read_to_string("std/tls.aic").expect("read std/tls.aic");

    assert!(
        source.contains("fn tls_connect(tcp_fd: Int, hostname: String, config: TlsConfig)"),
        "std/tls.aic must expose hostname-aware tls_connect wrapper"
    );
    assert!(
        source.contains("fn tls_upgrade(tcp_fd: Int, hostname: String, config: TlsConfig)"),
        "std/tls.aic must expose explicit tls_upgrade helper"
    );
    assert!(
        source.contains("fn tls_connect_with_config(tcp_fd: Int, config: TlsConfig)"),
        "std/tls.aic must preserve direct config-based TLS wrap helper"
    );
    assert!(
        source.contains("let raw = aic_tls_connect_intrinsic("),
        "std/tls.aic connect helper must call the TLS connect intrinsic"
    );
    assert!(
        source.contains("let raw = aic_tls_connect_addr_intrinsic("),
        "std/tls.aic tls_connect_addr must call the TLS connect-addr intrinsic"
    );
    assert!(
        source.contains("let raw = aic_tls_accept_intrinsic("),
        "std/tls.aic tls_accept_timeout must call the TLS accept intrinsic"
    );
    assert_delegate_call(
        &source,
        "std/tls.aic",
        "tls_send",
        "aic_tls_send_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/tls.aic",
        "tls_send_bytes",
        "aic_tls_send_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/tls.aic",
        "tls_recv",
        "aic_tls_recv_intrinsic",
        3,
    );
    assert!(
        source.contains("let raw = aic_tls_recv_intrinsic(stream.handle, max_bytes, timeout_ms);"),
        "std/tls.aic tls_recv_bytes must call the TLS recv intrinsic"
    );
    assert_delegate_call(
        &source,
        "std/tls.aic",
        "tls_close",
        "aic_tls_close_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/tls.aic",
        "tls_peer_subject",
        "aic_tls_peer_subject_intrinsic",
        1,
    );
    assert!(
        source.contains("let raw = aic_tls_version_intrinsic(stream.handle);"),
        "std/tls.aic tls_version must call the TLS version intrinsic"
    );
    assert!(
        source.contains("fn tls_peer_cn(stream: TlsStream) -> Result[String, TlsError]"),
        "std/tls.aic must expose tls_peer_cn helper"
    );

    for (intrinsic, arity) in [
        ("aic_tls_connect_intrinsic", 10usize),
        ("aic_tls_connect_addr_intrinsic", 11usize),
        ("aic_tls_accept_intrinsic", 9usize),
        ("aic_tls_send_intrinsic", 2usize),
        ("aic_tls_send_timeout_intrinsic", 3usize),
        ("aic_tls_recv_intrinsic", 3usize),
        ("aic_tls_async_send_submit_intrinsic", 3usize),
        ("aic_tls_async_recv_submit_intrinsic", 3usize),
        ("aic_tls_async_wait_int_intrinsic", 2usize),
        ("aic_tls_async_wait_string_intrinsic", 2usize),
        ("aic_tls_async_shutdown_intrinsic", 0usize),
        ("aic_tls_close_intrinsic", 1usize),
        ("aic_tls_peer_subject_intrinsic", 1usize),
        ("aic_tls_version_intrinsic", 1usize),
    ] {
        assert_intrinsic_declaration(&source, "std/tls.aic", intrinsic, arity);
    }
}

#[test]
fn unit_std_tls_bytes_apis_bridge_bytes_at_intrinsic_boundary() {
    let source = fs::read_to_string("std/tls.aic").expect("read std/tls.aic");

    assert!(
        source.contains("aic_tls_send_intrinsic(stream.handle, data.data)"),
        "std/tls.aic tls_send_bytes must pass Bytes.data into intrinsic boundary"
    );
    assert!(
        source.contains("let raw = aic_tls_recv_intrinsic(stream.handle, max_bytes, timeout_ms);"),
        "std/tls.aic tls_recv_bytes must bridge runtime String into Bytes wrapper"
    );
    assert!(
        source.contains("Ok(data) => Ok(Bytes { data: data })"),
        "std/tls.aic tls_recv_bytes must wrap String payload as Bytes"
    );
    assert!(
        source
            .contains("aic_tls_async_send_submit_intrinsic(stream.handle, data.data, timeout_ms)"),
        "std/tls.aic tls_async_send_submit must pass Bytes.data into intrinsic boundary"
    );
    assert!(
        source.contains("let raw = aic_tls_async_wait_string_intrinsic(op, timeout_ms);"),
        "std/tls.aic tls_async_wait_string must bridge runtime String into Bytes wrapper"
    );
    assert!(
        source.contains("aic_tls_async_shutdown_intrinsic()"),
        "std/tls.aic tls_async_shutdown must delegate to intrinsic"
    );
    assert!(
        source.contains("Ok(op) => tls_async_wait_int(op, timeout_ms)"),
        "std/tls.aic tls_async_send must compose submit + wait_int"
    );
    assert!(
        source.contains("Ok(op) => tls_async_wait_string(op, timeout_ms)"),
        "std/tls.aic tls_async_recv must compose submit + wait_string"
    );
    assert!(
        source.contains("verify_server: true"),
        "std/tls.aic default_tls_config must verify server certificates by default"
    );
    assert!(
        source.contains("ca_cert_path: None()"),
        "std/tls.aic default_tls_config must default to system CA bundle"
    );
    assert!(
        source.contains("fn unsafe_insecure_tls_config(server_name: Option[String]) -> TlsConfig"),
        "std/tls.aic must expose explicit unsafe override helper"
    );
    assert!(
        source.contains("Timeout,"),
        "std/tls.aic TlsError must expose Timeout variant for typed timeout handling"
    );
    assert!(
        source.contains("verify_server: false"),
        "std/tls.aic unsafe override helper must explicitly disable server verification"
    );
}

#[test]
fn unit_std_tls_byte_stream_adapter_bridges_tcp_and_tls_byte_paths() {
    let source = fs::read_to_string("std/tls.aic").expect("read std/tls.aic");

    assert!(
        source.contains("enum ByteStream {"),
        "std/tls.aic must define protocol-agnostic ByteStream"
    );
    assert!(
        source.contains("Tcp(TcpStream)"),
        "std/tls.aic ByteStream must include TCP variant"
    );
    assert!(
        source.contains("Tls(TlsStream)"),
        "std/tls.aic ByteStream must include TLS variant"
    );
    assert!(
        source.contains("enum ByteStreamError {"),
        "std/tls.aic must define ByteStreamError"
    );
    assert!(
        source.contains("Net(NetError)"),
        "std/tls.aic ByteStreamError must preserve NetError variants"
    );
    assert!(
        source.contains("Tls(TlsError)"),
        "std/tls.aic ByteStreamError must preserve TlsError variants"
    );
    assert!(
        source.contains("fn byte_stream_from_tcp(handle: Int) -> ByteStream"),
        "std/tls.aic must expose TCP-handle adapter constructor"
    );
    assert!(
        source.contains("Tcp(tcp_stream(handle))"),
        "std/tls.aic byte_stream_from_tcp must wrap handle via TcpStream adapter"
    );
    assert!(
        source.contains("fn byte_stream_from_tls(stream: TlsStream) -> ByteStream"),
        "std/tls.aic must expose TLS adapter constructor"
    );
    assert!(
        source.contains("fn byte_stream_send(stream: ByteStream, payload: Bytes) -> Result[Int, ByteStreamError] effects { net }"),
        "std/tls.aic must expose byte_stream_send"
    );
    assert!(
        source.contains("Tcp(tcp) => byte_stream_map_net(tcp_stream_send(tcp, payload))"),
        "std/tls.aic byte_stream_send must delegate TCP branch to tcp_stream_send"
    );
    assert!(
        source.contains("Tls(tls) => byte_stream_map_tls(tls_send_bytes(tls, payload))"),
        "std/tls.aic byte_stream_send must delegate TLS branch to tls_send_bytes"
    );
    assert!(
        source.contains(
            "Tcp(tcp) => byte_stream_map_net(tcp_stream_recv(tcp, max_bytes, timeout_ms))"
        ),
        "std/tls.aic byte_stream_recv must delegate TCP branch to tcp_stream_recv"
    );
    assert!(
        source.contains(
            "Tls(tls) => byte_stream_map_tls(tls_recv_bytes(tls, max_bytes, timeout_ms))"
        ),
        "std/tls.aic byte_stream_recv must delegate TLS branch to tls_recv_bytes"
    );
    assert!(
        source.contains("Tcp(tcp) => byte_stream_map_net(tcp_stream_close(tcp))"),
        "std/tls.aic byte_stream_close must delegate TCP branch to tcp_stream_close"
    );
    assert!(
        source.contains("Tls(tls) => byte_stream_map_tls(tls_close(tls))"),
        "std/tls.aic byte_stream_close must delegate TLS branch to tls_close"
    );
}

#[test]
fn unit_std_tls_exact_and_framed_reads_have_deadline_and_byte_stream_adapters() {
    let source = fs::read_to_string("std/tls.aic").expect("read std/tls.aic");

    assert!(
        source.contains(
            "fn tls_recv_exact_deadline(stream: TlsStream, expected_bytes: Int, deadline_ms: Int) -> Result[Bytes, TlsError] effects { net, time }"
        ),
        "std/tls.aic must expose deadline-based exact TLS reads"
    );
    assert!(
        source.contains(
            "fn tls_recv_exact(stream: TlsStream, expected_bytes: Int, timeout_ms: Int) -> Result[Bytes, TlsError] effects { net, time }"
        ),
        "std/tls.aic must expose timeout-based exact TLS reads"
    );
    assert!(
        source.contains(
            "fn tls_recv_framed_deadline(stream: TlsStream, max_frame_bytes: Int, deadline_ms: Int) -> Result[Bytes, TlsError] effects { net, time }"
        ),
        "std/tls.aic must expose deadline-based framed TLS reads"
    );
    assert!(
        source.contains(
            "fn tls_recv_framed(stream: TlsStream, max_frame_bytes: Int, timeout_ms: Int) -> Result[Bytes, TlsError] effects { net, time }"
        ),
        "std/tls.aic must expose timeout-based framed TLS reads"
    );
    assert!(
        source.contains("fn tls_frame_len_be(header: Bytes) -> Result[Int, TlsError]"),
        "std/tls.aic must expose shared frame-header parser for TLS framed reads"
    );
    assert!(
        source.contains("tls_recv_exact_deadline(stream, 4, deadline_ms)"),
        "std/tls.aic framed reads must consume exact 4-byte frame headers"
    );
    assert!(
        source.contains("tls_recv_exact_deadline(stream, payload_len, deadline_ms)"),
        "std/tls.aic framed reads must consume exact payload lengths"
    );
    assert!(
        source.contains(
            "fn byte_stream_recv_exact_deadline(stream: ByteStream, expected_bytes: Int, deadline_ms: Int) -> Result[Bytes, ByteStreamError] effects { net, time }"
        ),
        "std/tls.aic must expose ByteStream deadline exact reads"
    );
    assert!(
        source.contains(
            "fn byte_stream_recv_framed_deadline(stream: ByteStream, max_frame_bytes: Int, deadline_ms: Int) -> Result[Bytes, ByteStreamError] effects { net, time }"
        ),
        "std/tls.aic must expose ByteStream deadline framed reads"
    );
    assert!(
        source.contains("Tcp(tcp) => byte_stream_map_net(tcp_stream_recv_exact_deadline(tcp, expected_bytes, deadline_ms))"),
        "std/tls.aic ByteStream exact reads must delegate TCP branch to std.net exact reads"
    );
    assert!(
        source.contains("Tls(tls) => byte_stream_map_tls(tls_recv_exact_deadline(tls, expected_bytes, deadline_ms))"),
        "std/tls.aic ByteStream exact reads must delegate TLS branch to tls exact reads"
    );
    assert!(
        source.contains("Tcp(tcp) => byte_stream_map_net(tcp_stream_recv_framed_deadline(tcp, max_frame_bytes, deadline_ms))"),
        "std/tls.aic ByteStream framed reads must delegate TCP branch to std.net framed reads"
    );
    assert!(
        source.contains("Tls(tls) => byte_stream_map_tls(tls_recv_framed_deadline(tls, max_frame_bytes, deadline_ms))"),
        "std/tls.aic ByteStream framed reads must delegate TLS branch to tls framed reads"
    );
}

#[test]
fn unit_tls_policy_manifest_matches_runtime_defaults() {
    let manifest_text = fs::read_to_string("docs/security-ops/tls-policy.v1.json")
        .expect("read docs/security-ops/tls-policy.v1.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse tls policy manifest json");

    assert_eq!(
        manifest.get("schema_version").and_then(|v| v.as_i64()),
        Some(1),
        "tls policy schema_version must be pinned to 1"
    );
    assert_eq!(
        manifest
            .get("defaults")
            .and_then(|v| v.get("verify_server"))
            .and_then(|v| v.as_bool()),
        Some(true),
        "tls policy default verify_server must stay secure-by-default"
    );
    assert_eq!(
        manifest
            .get("defaults")
            .and_then(|v| v.get("min_protocol"))
            .and_then(|v| v.as_str()),
        Some("TLS1.2"),
        "tls policy default minimum protocol must remain TLS1.2"
    );
    assert!(
        manifest_text.contains("\"AIC_TLS_POLICY_UNSAFE\""),
        "tls policy manifest must include unsafe override audit tag"
    );

    let runtime_dir = "src/codegen/runtime";
    let mut part_paths = fs::read_dir(runtime_dir)
        .expect("read src/codegen/runtime")
        .map(|entry| entry.expect("runtime dir entry").path())
        .collect::<Vec<_>>();
    part_paths.sort();
    let mut runtime_source = String::new();
    for part in part_paths {
        runtime_source.push_str(
            &fs::read_to_string(&part)
                .unwrap_or_else(|_| panic!("read runtime source part {}", part.display())),
        );
    }
    assert!(
        runtime_source.contains("AIC_TLS_POLICY_UNSAFE verify_server=false disables certificate and hostname validation"),
        "runtime must emit explicit audit warning when TLS verification is disabled"
    );
}

#[test]
fn unit_secure_error_contract_module_and_manifest_are_in_sync() {
    let source = fs::read_to_string("std/secure_errors.aic").expect("read std/secure_errors.aic");
    assert!(
        source.contains("struct SecureErrorInfo"),
        "std/secure_errors.aic must expose unified secure error info struct"
    );
    assert!(
        source.contains("fn buffer_error_info(err: BufferError) -> SecureErrorInfo"),
        "std/secure_errors.aic must map buffer errors"
    );
    assert!(
        source.contains("fn crypto_error_info(err: CryptoError) -> SecureErrorInfo"),
        "std/secure_errors.aic must map crypto errors"
    );
    assert!(
        source.contains("fn tls_error_info(err: TlsError) -> SecureErrorInfo"),
        "std/secure_errors.aic must map tls errors"
    );
    assert!(
        source.contains("fn pool_error_info(err: PoolErrorContract) -> SecureErrorInfo"),
        "std/secure_errors.aic must map pool contract errors"
    );
    assert!(
        source
            .contains("Timeout => secure_error_info(\"tls\", \"TLS_TIMEOUT\", \"timeout\", true)"),
        "std/secure_errors.aic must map TLS timeout to TLS_TIMEOUT contract code"
    );

    let manifest_text = fs::read_to_string("docs/errors/secure-networking-error-contract.v1.json")
        .expect("read docs/errors/secure-networking-error-contract.v1.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse secure networking error contract json");
    assert_eq!(
        manifest.get("schema_version").and_then(|v| v.as_i64()),
        Some(1),
        "secure networking error contract schema_version must be pinned to 1"
    );
    assert_eq!(
        manifest
            .get("modules")
            .and_then(|v| v.get("tls"))
            .and_then(|v| v.get("TLS_PROTOCOL_ERROR"))
            .and_then(|v| v.get("category"))
            .and_then(|v| v.as_str()),
        Some("protocol"),
        "TLS_PROTOCOL_ERROR category must stay protocol"
    );
    assert_eq!(
        manifest
            .get("modules")
            .and_then(|v| v.get("tls"))
            .and_then(|v| v.get("TLS_TIMEOUT"))
            .and_then(|v| v.get("category"))
            .and_then(|v| v.as_str()),
        Some("timeout"),
        "TLS_TIMEOUT category must stay timeout"
    );
}

#[test]
fn unit_tls_timeout_is_typed_across_std_codegen_runtime_and_docs() {
    let tls_source = fs::read_to_string("std/tls.aic").expect("read std/tls.aic");
    assert!(
        tls_source.contains("Timeout,"),
        "std/tls.aic must expose Timeout in TlsError variants"
    );

    let codegen_source = fs::read_to_string("src/codegen/generator_json_regex.rs")
        .expect("read src/codegen/generator_json_regex.rs");
    assert!(
        codegen_source.contains("(8, \"Timeout\")"),
        "codegen must map TLS runtime timeout status code 8 to TlsError::Timeout"
    );

    let runtime_source =
        fs::read_to_string("src/codegen/runtime/part04.c").expect("read runtime part04");
    assert!(
        runtime_source.contains("if (net_error == 4) {\n        return 8;\n    }"),
        "runtime TLS net->TLS mapping must map NetError::Timeout to TlsError::Timeout"
    );
    assert!(
        !runtime_source.contains("has no Timeout variant"),
        "runtime must not carry stale comments that map TLS timeout to Io"
    );

    let io_api =
        fs::read_to_string("docs/io-api-reference.md").expect("read docs/io-api-reference.md");
    let tls_api = fs::read_to_string("docs/std-api/tls.md").expect("read docs/std-api/tls.md");
    let async_runtime =
        fs::read_to_string("docs/async-event-loop.md").expect("read docs/async-event-loop.md");
    assert!(
        io_api.contains("TlsError::Timeout"),
        "io-api-reference must document typed TLS timeout behavior"
    );
    assert!(
        tls_api.contains("TlsError::Timeout"),
        "std TLS API docs must document typed TLS timeout behavior"
    );
    assert!(
        async_runtime.contains("TlsError::Timeout"),
        "async-event-loop docs must document typed TLS async wait timeout behavior"
    );
}

#[test]
fn unit_io_docs_bytes_first_signatures_match_std_net_contract() {
    let io_api =
        fs::read_to_string("docs/io-api-reference.md").expect("read docs/io-api-reference.md");
    let net_runtime = fs::read_to_string("docs/io-runtime/net-time-rand.md")
        .expect("read docs/io-runtime/net-time-rand.md");
    let async_runtime =
        fs::read_to_string("docs/async-event-loop.md").expect("read docs/async-event-loop.md");
    let lifecycle = fs::read_to_string("docs/io-runtime/lifecycle-playbook.md")
        .expect("read docs/io-runtime/lifecycle-playbook.md");

    for (name, doc) in [
        ("io-api-reference", &io_api),
        ("net-time-rand", &net_runtime),
    ] {
        assert!(
            doc.contains("payload: Bytes"),
            "{name} must document UdpPacket payload as Bytes"
        );
        assert!(
            doc.contains(
                "fn tcp_send(handle: Int, payload: Bytes) -> Result[Int, NetError] effects { net }"
            ),
            "{name} must document bytes-first tcp_send signature"
        );
        assert!(
            doc.contains("fn tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net }"),
            "{name} must document bytes-first tcp_recv signature"
        );
        assert!(
            doc.contains("fn udp_send_to(handle: Int, addr: String, payload: Bytes) -> Result[Int, NetError] effects { net }"),
            "{name} must document bytes-first udp_send_to signature"
        );
        assert!(
            doc.contains("fn tcp_set_nodelay(handle: Int, enabled: Bool) -> Result[Bool, NetError] effects { net }"),
            "{name} must document tcp_set_nodelay signature"
        );
        assert!(
            doc.contains("fn tcp_set_keepalive(handle: Int, enabled: Bool) -> Result[Bool, NetError] effects { net }"),
            "{name} must document tcp_set_keepalive signature"
        );
        assert!(
            doc.contains("fn tcp_set_send_buffer_size(handle: Int, size_bytes: Int) -> Result[Bool, NetError] effects { net }"),
            "{name} must document tcp_set_send_buffer_size signature"
        );
        assert!(
            doc.contains(
                "fn tcp_get_send_buffer_size(handle: Int) -> Result[Int, NetError] effects { net }"
            ),
            "{name} must document tcp_get_send_buffer_size signature"
        );
        assert!(
            !doc.contains("fn tcp_send(handle: Int, payload: String) -> Result[Int, NetError] effects { net }"),
            "{name} still documents stale string tcp_send signature"
        );
        assert!(
            !doc.contains("fn tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[String, NetError] effects { net }"),
            "{name} still documents stale string tcp_recv signature"
        );
        assert!(
            !doc.contains("fn udp_send_to(handle: Int, addr: String, payload: String) -> Result[Int, NetError] effects { net }"),
            "{name} still documents stale string udp_send_to signature"
        );
    }

    assert!(
        async_runtime.contains("async_wait_string(op, timeout_ms) -> Result[Bytes, NetError]"),
        "async runtime docs must use bytes-first async_wait_string signature"
    );
    assert!(
        !async_runtime.contains("async_wait_string(op, timeout_ms) -> Result[String, NetError]"),
        "async runtime docs still document stale string async_wait_string signature"
    );

    assert!(
        lifecycle.contains("Timeout => Bytes { data: \"\" }"),
        "lifecycle network timeout template must use Bytes fallback values"
    );
}

#[test]
fn unit_tls_docs_include_async_submit_wait_bytes_contract() {
    let io_api =
        fs::read_to_string("docs/io-api-reference.md").expect("read docs/io-api-reference.md");
    let tls_api = fs::read_to_string("docs/std-api/tls.md").expect("read docs/std-api/tls.md");
    let async_runtime =
        fs::read_to_string("docs/async-event-loop.md").expect("read docs/async-event-loop.md");

    for (name, doc) in [("io-api-reference", &io_api), ("std-api-tls", &tls_api)] {
        assert!(
            doc.contains("fn tls_async_send_submit(stream: TlsStream, data: Bytes, timeout_ms: Int) -> Result[AsyncIntOp, TlsError] effects { net, concurrency }"),
            "{name} must document tls_async_send_submit bytes-first signature"
        );
        assert!(
            doc.contains("fn tls_async_recv_submit(stream: TlsStream, max_bytes: Int, timeout_ms: Int) -> Result[AsyncStringOp, TlsError] effects { net, concurrency }"),
            "{name} must document tls_async_recv_submit signature"
        );
        assert!(
            doc.contains("fn tls_async_wait_string(op: AsyncStringOp, timeout_ms: Int) -> Result[Bytes, TlsError] effects { net, concurrency }"),
            "{name} must document tls_async_wait_string bytes-first signature"
        );
        assert!(
            doc.contains(
                "fn tls_async_shutdown() -> Result[Bool, TlsError] effects { net, concurrency }"
            ),
            "{name} must document tls_async_shutdown signature"
        );
        assert!(
            !doc.contains("fn tls_async_wait_string(op: AsyncStringOp, timeout_ms: Int) -> Result[String, TlsError] effects { net, concurrency }"),
            "{name} still documents stale string tls_async_wait_string signature"
        );
    }

    assert!(
        async_runtime.contains(
            "tls_async_send_submit(stream, data, timeout_ms) -> Result[AsyncIntOp, TlsError]"
        ),
        "async runtime docs must include tls_async_send_submit"
    );
    assert!(
        async_runtime.contains("tls_async_wait_string(op, timeout_ms) -> Result[Bytes, TlsError]"),
        "async runtime docs must include bytes-first tls_async_wait_string"
    );
    assert!(
        io_api.contains("examples/io/tls_async_submit_wait.aic"),
        "io api reference must include the runnable tls async submit/wait example"
    );
    assert!(
        tls_api.contains("examples/io/tls_async_submit_wait.aic"),
        "std tls docs must include the runnable tls async submit/wait example"
    );
    assert!(
        async_runtime.contains("examples/io/tls_async_submit_wait.aic"),
        "async runtime docs must include tls async submit/wait example coverage"
    );
}

#[test]
fn unit_postgres_tls_scram_replay_contract_and_example_are_in_sync() {
    if !protocol_replay_tests_enabled() {
        return;
    }

    let source = fs::read_to_string("examples/io/postgres_tls_scram_reference.aic")
        .expect("read examples/io/postgres_tls_scram_reference.aic");
    let manifest_text = fs::read_to_string("docs/security-ops/postgres-tls-scram-replay.v1.json")
        .expect("read docs/security-ops/postgres-tls-scram-replay.v1.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse postgres tls scram replay manifest");

    assert_eq!(
        manifest.get("schema_version").and_then(|v| v.as_i64()),
        Some(1),
        "postgres tls replay schema_version must be pinned to 1"
    );
    assert_eq!(
        manifest.get("deterministic").and_then(|v| v.as_bool()),
        Some(true),
        "postgres tls replay contract must stay deterministic"
    );
    assert_eq!(
        manifest.get("example").and_then(|v| v.as_str()),
        Some("examples/io/postgres_tls_scram_reference.aic"),
        "postgres tls replay contract must point at canonical example"
    );

    let scenarios = manifest
        .get("scenarios")
        .and_then(|v| v.as_array())
        .expect("scenarios array");
    assert_eq!(
        scenarios.len(),
        5,
        "postgres tls replay must define success + four negative scenarios"
    );

    let mut ids = BTreeSet::new();
    for scenario in scenarios {
        let id = scenario
            .get("id")
            .and_then(|v| v.as_str())
            .expect("scenario id");
        let expected_code = scenario
            .get("expected_code")
            .and_then(|v| v.as_str())
            .expect("expected_code");
        let expected_print_int = scenario
            .get("expected_print_int")
            .and_then(|v| v.as_i64())
            .expect("expected_print_int");
        ids.insert(id.to_string());
        assert!(
            source.contains(expected_code),
            "canonical replay example must encode typed failure `{expected_code}`"
        );
        assert!(
            expected_print_int > 0,
            "scenario `{id}` must define a non-zero deterministic score"
        );
    }

    assert!(
        ids.contains("success")
            && ids.contains("bad-cert")
            && ids.contains("auth-failure")
            && ids.contains("timeout")
            && ids.contains("pool-exhausted"),
        "replay contract must preserve required scenario ids"
    );
    assert!(
        source.contains("fn replay_suite_score() -> Int"),
        "canonical replay example must expose replay suite entrypoint"
    );
    assert!(
        source.contains("default_tls_config()")
            && source.contains("unsafe_insecure_tls_config(Some(\"db.internal\"))"),
        "canonical replay example must document both secure-default and unsafe-audit TLS paths"
    );
}

#[test]
fn unit_std_string_public_apis_delegate_to_runtime_intrinsics() {
    let string_source = fs::read_to_string("std/string.aic").expect("read std/string.aic");

    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "len",
        "aic_string_len_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "contains",
        "aic_string_contains_intrinsic",
        2,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "starts_with",
        "aic_string_starts_with_intrinsic",
        2,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "ends_with",
        "aic_string_ends_with_intrinsic",
        2,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "index_of",
        "aic_string_index_of_intrinsic",
        2,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "last_index_of",
        "aic_string_last_index_of_intrinsic",
        2,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "substring",
        "aic_string_substring_intrinsic",
        3,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "char_at",
        "aic_string_char_at_intrinsic",
        2,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "split",
        "aic_string_split_intrinsic",
        2,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "split_first",
        "aic_string_split_first_intrinsic",
        2,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "trim",
        "aic_string_trim_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "trim_start",
        "aic_string_trim_start_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "trim_end",
        "aic_string_trim_end_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "to_upper",
        "aic_string_to_upper_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "to_lower",
        "aic_string_to_lower_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "replace",
        "aic_string_replace_intrinsic",
        3,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "repeat",
        "aic_string_repeat_intrinsic",
        2,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "parse_int",
        "aic_string_parse_int_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "parse_float",
        "aic_string_parse_float_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "int_to_string",
        "aic_string_int_to_string_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "float_to_string",
        "aic_string_float_to_string_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "bool_to_string",
        "aic_string_bool_to_string_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "is_valid_utf8",
        "aic_string_is_valid_utf8_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "is_ascii",
        "aic_string_is_ascii_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "bytes_to_string_lossy",
        "aic_string_bytes_to_string_lossy_intrinsic",
        1,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "join",
        "aic_string_join_intrinsic",
        2,
    );
    assert_delegate_call(
        &string_source,
        "std/string.aic",
        "format",
        "aic_string_format_intrinsic",
        2,
    );
}

#[test]
fn unit_std_char_public_apis_delegate_to_runtime_intrinsics() {
    let char_source = fs::read_to_string("std/char.aic").expect("read std/char.aic");

    assert_delegate_call(
        &char_source,
        "std/char.aic",
        "is_digit",
        "aic_char_is_digit_intrinsic",
        1,
    );
    assert_delegate_call(
        &char_source,
        "std/char.aic",
        "is_alpha",
        "aic_char_is_alpha_intrinsic",
        1,
    );
    assert_delegate_call(
        &char_source,
        "std/char.aic",
        "is_whitespace",
        "aic_char_is_whitespace_intrinsic",
        1,
    );
    assert_delegate_call(
        &char_source,
        "std/char.aic",
        "char_to_int",
        "aic_char_to_int_intrinsic",
        1,
    );
    assert_delegate_call(
        &char_source,
        "std/char.aic",
        "int_to_char",
        "aic_char_int_to_char_intrinsic",
        1,
    );
    assert_delegate_call(
        &char_source,
        "std/char.aic",
        "chars",
        "aic_char_chars_intrinsic",
        1,
    );
    assert_delegate_call(
        &char_source,
        "std/char.aic",
        "from_chars",
        "aic_char_from_chars_intrinsic",
        1,
    );
}

#[test]
fn unit_std_math_public_apis_delegate_to_runtime_intrinsics() {
    let math_source = fs::read_to_string("std/math.aic").expect("read std/math.aic");

    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "abs",
        "aic_math_abs_intrinsic",
        1,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "abs_float",
        "aic_math_abs_float_intrinsic",
        1,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "min",
        "aic_math_min_intrinsic",
        2,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "max",
        "aic_math_max_intrinsic",
        2,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "pow",
        "aic_math_pow_intrinsic",
        2,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "sqrt",
        "aic_math_sqrt_intrinsic",
        1,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "floor",
        "aic_math_floor_intrinsic",
        1,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "ceil",
        "aic_math_ceil_intrinsic",
        1,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "round",
        "aic_math_round_intrinsic",
        1,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "log",
        "aic_math_log_intrinsic",
        1,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "sin",
        "aic_math_sin_intrinsic",
        1,
    );
    assert_delegate_call(
        &math_source,
        "std/math.aic",
        "cos",
        "aic_math_cos_intrinsic",
        1,
    );
}

#[test]
fn unit_std_http_public_apis_delegate_to_runtime_intrinsics() {
    let http_source = fs::read_to_string("std/http.aic").expect("read std/http.aic");

    assert_delegate_call(
        &http_source,
        "std/http.aic",
        "parse_method",
        "aic_http_parse_method_intrinsic",
        1,
    );
    assert_delegate_call(
        &http_source,
        "std/http.aic",
        "method_name",
        "aic_http_method_name_intrinsic",
        1,
    );
    assert_delegate_call(
        &http_source,
        "std/http.aic",
        "status_reason",
        "aic_http_status_reason_intrinsic",
        1,
    );
    assert_delegate_call(
        &http_source,
        "std/http.aic",
        "validate_header",
        "aic_http_validate_header_intrinsic",
        2,
    );
    assert_delegate_call(
        &http_source,
        "std/http.aic",
        "validate_target",
        "aic_http_validate_target_intrinsic",
        1,
    );
    assert_delegate_call(
        &http_source,
        "std/http.aic",
        "header",
        "aic_http_header_intrinsic",
        2,
    );
    assert_delegate_call(
        &http_source,
        "std/http.aic",
        "request",
        "aic_http_request_intrinsic",
        4,
    );
    assert_delegate_call(
        &http_source,
        "std/http.aic",
        "response",
        "aic_http_response_intrinsic",
        3,
    );
}

#[test]
fn unit_std_http_server_public_apis_delegate_to_runtime_intrinsics() {
    let source = fs::read_to_string("std/http_server.aic").expect("read std/http_server.aic");

    assert_delegate_call(
        &source,
        "std/http_server.aic",
        "listen",
        "aic_http_server_listen_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/http_server.aic",
        "accept",
        "aic_http_server_accept_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/http_server.aic",
        "read_request",
        "aic_http_server_read_request_intrinsic",
        3,
    );
    assert_delegate_call(
        &source,
        "std/http_server.aic",
        "write_response",
        "aic_http_server_write_response_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/http_server.aic",
        "close",
        "aic_http_server_close_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/http_server.aic",
        "text_response",
        "aic_http_server_text_response_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/http_server.aic",
        "json_response",
        "aic_http_server_json_response_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/http_server.aic",
        "header",
        "aic_http_server_header_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/http_server.aic",
        "error_response",
        "text_response",
        2,
    );
}

#[test]
fn unit_std_router_public_apis_delegate_to_runtime_intrinsics() {
    let source = fs::read_to_string("std/router.aic").expect("read std/router.aic");

    assert_delegate_call(
        &source,
        "std/router.aic",
        "new_router",
        "aic_router_new_intrinsic",
        0,
    );
    assert_delegate_call(
        &source,
        "std/router.aic",
        "add",
        "aic_router_add_intrinsic",
        4,
    );
    assert_delegate_call(
        &source,
        "std/router.aic",
        "match_route",
        "aic_router_match_intrinsic",
        3,
    );
}

#[test]
fn unit_std_time_public_apis_delegate_to_runtime_intrinsics() {
    let time_source = fs::read_to_string("std/time.aic").expect("read std/time.aic");

    assert_delegate_call(
        &time_source,
        "std/time.aic",
        "now_ms",
        "aic_time_now_ms_intrinsic",
        0,
    );
    assert_delegate_call(
        &time_source,
        "std/time.aic",
        "monotonic_ms",
        "aic_time_monotonic_ms_intrinsic",
        0,
    );
    assert_delegate_call(
        &time_source,
        "std/time.aic",
        "sleep_ms",
        "aic_time_sleep_ms_intrinsic",
        1,
    );
    assert_delegate_call(
        &time_source,
        "std/time.aic",
        "parse_rfc3339",
        "aic_time_parse_rfc3339_intrinsic",
        1,
    );
    assert_delegate_call(
        &time_source,
        "std/time.aic",
        "parse_iso8601",
        "aic_time_parse_iso8601_intrinsic",
        1,
    );
    assert_delegate_call(
        &time_source,
        "std/time.aic",
        "format_rfc3339",
        "aic_time_format_rfc3339_intrinsic",
        1,
    );
    assert_delegate_call(
        &time_source,
        "std/time.aic",
        "format_iso8601",
        "aic_time_format_iso8601_intrinsic",
        1,
    );
}

#[test]
fn unit_std_rand_public_apis_delegate_to_runtime_intrinsics() {
    let rand_source = fs::read_to_string("std/rand.aic").expect("read std/rand.aic");

    assert_delegate_call(
        &rand_source,
        "std/rand.aic",
        "seed",
        "aic_rand_seed_intrinsic",
        1,
    );
    assert_delegate_call(
        &rand_source,
        "std/rand.aic",
        "random_int",
        "aic_rand_int_intrinsic",
        0,
    );
    assert_delegate_call(
        &rand_source,
        "std/rand.aic",
        "random_range",
        "aic_rand_range_intrinsic",
        2,
    );
}

#[test]
fn unit_std_regex_public_apis_delegate_to_runtime_intrinsics() {
    let regex_source = fs::read_to_string("std/regex.aic").expect("read std/regex.aic");

    assert_delegate_call(
        &regex_source,
        "std/regex.aic",
        "compile_with_flags",
        "aic_regex_compile_intrinsic",
        2,
    );
    assert_delegate_call(
        &regex_source,
        "std/regex.aic",
        "is_match",
        "aic_regex_is_match_intrinsic",
        2,
    );
    assert_delegate_call(
        &regex_source,
        "std/regex.aic",
        "find",
        "aic_regex_find_intrinsic",
        2,
    );
    assert_delegate_call(
        &regex_source,
        "std/regex.aic",
        "captures",
        "aic_regex_captures_intrinsic",
        2,
    );
    assert_delegate_call(
        &regex_source,
        "std/regex.aic",
        "replace",
        "aic_regex_replace_intrinsic",
        3,
    );
}

#[test]
fn unit_std_json_public_apis_delegate_to_runtime_intrinsics() {
    let json_source = fs::read_to_string("std/json.aic").expect("read std/json.aic");

    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "parse",
        "aic_json_parse_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "stringify",
        "aic_json_stringify_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "encode_int",
        "aic_json_encode_int_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "encode_float",
        "aic_json_encode_float_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "encode_bool",
        "aic_json_encode_bool_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "encode_string",
        "aic_json_encode_string_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "encode_null",
        "aic_json_encode_null_intrinsic",
        0,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "decode_int",
        "aic_json_decode_int_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "decode_float",
        "aic_json_decode_float_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "decode_bool",
        "aic_json_decode_bool_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "decode_string",
        "aic_json_decode_string_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "object_empty",
        "aic_json_object_empty_intrinsic",
        0,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "object_set",
        "aic_json_object_set_intrinsic",
        3,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "object_get",
        "aic_json_object_get_intrinsic",
        2,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "kind",
        "aic_json_kind_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "encode",
        "aic_json_serde_encode_intrinsic",
        1,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "decode_with",
        "aic_json_serde_decode_intrinsic",
        2,
    );
    assert_delegate_call(
        &json_source,
        "std/json.aic",
        "schema",
        "aic_json_serde_schema_intrinsic",
        1,
    );
}

#[test]
fn unit_std_config_load_json_has_success_and_error_paths() {
    let source = fs::read_to_string("std/config.aic").expect("read std/config.aic");

    assert!(source.contains(
        "fn load_json(path: String) -> Result[Map[String, String], ConfigError] effects { fs }"
    ));
    assert!(source.contains("match read_text(path)"));
    assert!(source.contains("match parse(text)"));
    assert!(source.contains("match decode_with(parsed, marker)"));
    assert!(source.contains("Ok(config) => Ok(config)"));
    assert!(source.contains("Err(err) => Err(map_fs_error(err))"));
    assert!(source.contains("Err(err) => Err(map_json_error(err))"));
}

#[test]
fn unit_std_config_env_prefix_and_missing_key_paths_present() {
    let source = fs::read_to_string("std/config.aic").expect("read std/config.aic");

    assert!(source
        .contains("fn load_env_prefix(prefix: String) -> Map[String, String] effects { env }"));
    assert!(source.contains("string.starts_with(entry.key, prefix)"));
    assert!(source.contains("substring(entry.key, prefix_len, key_len)"));

    assert!(source.contains(
        "fn get_or_default(config: Map[String, String], key: String, fallback: String) -> String"
    ));
    assert!(source.contains("Some(value) => value"));
    assert!(source.contains("None => fallback"));

    assert!(source.contains(
        "fn require(config: Map[String, String], key: String) -> Result[String, ConfigError]"
    ));
    assert!(source.contains("Some(value) => Ok(value)"));
    assert!(source.contains("None => Err(MissingKey())"));
}

#[test]
fn unit_std_concurrency_public_apis_delegate_to_runtime_intrinsics() {
    let source = fs::read_to_string("std/concurrent.aic").expect("read std/concurrent.aic");

    assert!(source.contains("enum SelectResult[A, B] {"));
    assert!(source.contains("First(A),"));
    assert!(source.contains("Second(B),"));
    assert!(source.contains("Timeout,"));
    assert!(source.contains("Closed,"));
    assert!(source.contains("fn select2[A, B](rx1: Receiver[A], rx2: Receiver[B], timeout_ms: Int) -> SelectResult[A, B]"));
    assert!(source.contains("fn select_any[T](receivers: Vec[Receiver[T]], timeout_ms: Int) -> Result[(Int, T), ChannelError] effects { concurrency, env }"));
    assert!(source.contains("turn = (turn + 1) % count"));
    assert!(source.contains("struct Scope {"));
    assert!(source.contains("struct Arc[T] {"));
    assert!(source.contains("struct AtomicInt {"));
    assert!(source.contains("struct AtomicBool {"));
    assert!(source.contains("struct ThreadLocal[T] {"));
    assert!(source.contains("fn scoped[T](f: Fn(Scope) -> T) -> T effects { concurrency }"));
    assert!(source.contains(
        "fn scope_spawn[T](scope: Scope, f: Fn() -> T) -> Task[T] effects { concurrency }"
    ));
    assert!(source.contains("fn arc_new[T](value: T) -> Arc[T] effects { concurrency }"));
    assert!(source.contains("fn arc_clone[T](a: Arc[T]) -> Arc[T] effects { concurrency }"));
    assert!(source.contains(
        "fn arc_get[T](a: Arc[T]) -> Result[T, ConcurrencyError] effects { concurrency }"
    ));
    assert!(source.contains("fn arc_strong_count[T](a: Arc[T]) -> Int effects { concurrency }"));
    assert!(source.contains("fn atomic_int(initial: Int) -> AtomicInt effects { concurrency }"));
    assert!(source.contains("fn atomic_load(a: AtomicInt) -> Int effects { concurrency }"));
    assert!(
        source.contains("fn atomic_store(a: AtomicInt, value: Int) -> () effects { concurrency }")
    );
    assert!(
        source.contains("fn atomic_add(a: AtomicInt, delta: Int) -> Int effects { concurrency }")
    );
    assert!(
        source.contains("fn atomic_sub(a: AtomicInt, delta: Int) -> Int effects { concurrency }")
    );
    assert!(source.contains(
        "fn atomic_cas(a: AtomicInt, expected: Int, desired: Int) -> Bool effects { concurrency }"
    ));
    assert!(source.contains("fn atomic_bool(initial: Bool) -> AtomicBool effects { concurrency }"));
    assert!(source.contains("fn atomic_load_bool(a: AtomicBool) -> Bool effects { concurrency }"));
    assert!(source.contains(
        "fn atomic_store_bool(a: AtomicBool, value: Bool) -> () effects { concurrency }"
    ));
    assert!(source.contains(
        "fn atomic_swap_bool(a: AtomicBool, desired: Bool) -> Bool effects { concurrency }"
    ));
    assert!(source
        .contains("fn thread_local[T](init: Fn() -> T) -> ThreadLocal[T] effects { concurrency }"));
    assert!(source.contains("fn tl_get[T](tl: ThreadLocal[T]) -> T effects { concurrency }"));
    assert!(
        source.contains("fn tl_set[T](tl: ThreadLocal[T], value: T) -> () effects { concurrency }")
    );
    assert!(source.contains(
        "fn bytes_channel() -> (Sender[Bytes], Receiver[Bytes]) effects { concurrency }"
    ));
    assert!(source.contains(
        "fn buffered_bytes_channel(capacity: Int) -> (Sender[Bytes], Receiver[Bytes]) effects { concurrency }"
    ));
    assert!(source.contains(
        "fn send_bytes(tx: Sender[Bytes], value: Bytes) -> Result[Bool, ChannelError] effects { concurrency }"
    ));
    assert!(source.contains(
        "fn try_send_bytes(tx: Sender[Bytes], value: Bytes) -> Result[Bool, ChannelError] effects { concurrency }"
    ));
    assert!(source.contains(
        "fn recv_bytes(rx: Receiver[Bytes]) -> Result[Bytes, ChannelError] effects { concurrency }"
    ));
    assert!(source.contains(
        "fn try_recv_bytes(rx: Receiver[Bytes]) -> Result[Bytes, ChannelError] effects { concurrency }"
    ));
    assert!(source.contains("fn recv_bytes_timeout("));
    assert!(source.contains(
        "intrinsic fn aic_conc_payload_store_value_intrinsic[T](payload: T) -> Result[Int, ConcurrencyError] effects { concurrency };"
    ));
    assert!(source.contains(
        "intrinsic fn aic_conc_payload_take_value_intrinsic[T](payload_id: Int, marker: Option[T]) -> Result[T, ConcurrencyError] effects { concurrency };"
    ));
    assert!(source.contains("match store_payload_for_channel(value)"));
    assert!(source.contains("take_payload_for_channel(payload_id, rx)"));
    assert!(source.contains("match aic_conc_payload_store_intrinsic(value.data)"));
    assert!(source.contains("Ok(payload) => Ok(Bytes { data: payload })"));

    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "spawn_task",
        "aic_conc_spawn_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "join_task",
        "aic_conc_join_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "timeout_task",
        "aic_conc_join_timeout_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "cancel_task",
        "aic_conc_cancel_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "spawn_group",
        "aic_conc_spawn_group_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "select_first",
        "aic_conc_select_first_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "channel_int",
        "aic_conc_channel_int_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "send_int",
        "aic_conc_send_int_intrinsic",
        3,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "recv_int",
        "aic_conc_recv_int_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "close_channel",
        "aic_conc_close_channel_intrinsic",
        1,
    );
    assert!(source.contains("aic_conc_atomic_int_intrinsic"));
    assert!(source.contains("aic_conc_atomic_load_intrinsic"));
    assert!(source.contains("aic_conc_atomic_store_intrinsic"));
    assert!(source.contains("aic_conc_atomic_add_intrinsic"));
    assert!(source.contains("aic_conc_atomic_sub_intrinsic"));
    assert!(source.contains("aic_conc_atomic_cas_intrinsic"));
    assert!(source.contains("aic_conc_atomic_bool_intrinsic"));
    assert!(source.contains("aic_conc_atomic_load_bool_intrinsic"));
    assert!(source.contains("aic_conc_atomic_store_bool_intrinsic"));
    assert!(source.contains("aic_conc_atomic_swap_bool_intrinsic"));
    assert!(source.contains("aic_conc_tl_new_intrinsic"));
    assert!(source.contains("aic_conc_tl_get_intrinsic"));
    assert!(source.contains("aic_conc_tl_set_intrinsic"));
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "mutex_int",
        "aic_conc_mutex_int_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "lock_int",
        "aic_conc_mutex_lock_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "unlock_int",
        "aic_conc_mutex_unlock_intrinsic",
        2,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "close_mutex",
        "aic_conc_mutex_close_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "close_rwlock",
        "aic_conc_rwlock_close_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "scope_cancel",
        "aic_conc_scope_cancel_intrinsic",
        1,
    );
    assert_delegate_call(
        &source,
        "std/concurrent.aic",
        "scope_join_all",
        "aic_conc_scope_join_all_intrinsic",
        1,
    );

    for (name, arity) in [
        ("aic_conc_spawn_intrinsic", 2usize),
        ("aic_conc_join_intrinsic", 1usize),
        ("aic_conc_join_timeout_intrinsic", 2usize),
        ("aic_conc_cancel_intrinsic", 1usize),
        ("aic_conc_spawn_fn_intrinsic", 1usize),
        ("aic_conc_spawn_fn_named_intrinsic", 2usize),
        ("aic_conc_join_value_intrinsic", 1usize),
        ("aic_conc_scope_new_intrinsic", 0usize),
        ("aic_conc_scope_spawn_fn_intrinsic", 2usize),
        ("aic_conc_scope_join_all_intrinsic", 1usize),
        ("aic_conc_scope_cancel_intrinsic", 1usize),
        ("aic_conc_scope_close_intrinsic", 1usize),
        ("aic_conc_spawn_group_intrinsic", 2usize),
        ("aic_conc_select_first_intrinsic", 2usize),
        ("aic_conc_channel_int_intrinsic", 1usize),
        ("aic_conc_channel_int_buffered_intrinsic", 1usize),
        ("aic_conc_send_int_intrinsic", 3usize),
        ("aic_conc_try_send_int_intrinsic", 2usize),
        ("aic_conc_recv_int_intrinsic", 2usize),
        ("aic_conc_try_recv_int_intrinsic", 1usize),
        ("aic_conc_select_recv_int_intrinsic", 3usize),
        ("aic_conc_close_channel_intrinsic", 1usize),
        ("aic_conc_arc_new_intrinsic", 1usize),
        ("aic_conc_arc_clone_intrinsic", 1usize),
        ("aic_conc_arc_get_intrinsic", 1usize),
        ("aic_conc_arc_strong_count_intrinsic", 1usize),
        ("aic_conc_atomic_int_intrinsic", 1usize),
        ("aic_conc_atomic_load_intrinsic", 1usize),
        ("aic_conc_atomic_store_intrinsic", 2usize),
        ("aic_conc_atomic_add_intrinsic", 2usize),
        ("aic_conc_atomic_sub_intrinsic", 2usize),
        ("aic_conc_atomic_cas_intrinsic", 3usize),
        ("aic_conc_atomic_bool_intrinsic", 1usize),
        ("aic_conc_atomic_load_bool_intrinsic", 1usize),
        ("aic_conc_atomic_store_bool_intrinsic", 2usize),
        ("aic_conc_atomic_swap_bool_intrinsic", 2usize),
        ("aic_conc_tl_new_intrinsic", 1usize),
        ("aic_conc_tl_get_intrinsic", 1usize),
        ("aic_conc_tl_set_intrinsic", 2usize),
        ("aic_conc_mutex_int_intrinsic", 1usize),
        ("aic_conc_mutex_lock_intrinsic", 2usize),
        ("aic_conc_mutex_unlock_intrinsic", 2usize),
        ("aic_conc_mutex_close_intrinsic", 1usize),
        ("aic_conc_rwlock_int_intrinsic", 1usize),
        ("aic_conc_rwlock_read_intrinsic", 2usize),
        ("aic_conc_rwlock_write_lock_intrinsic", 2usize),
        ("aic_conc_rwlock_write_unlock_intrinsic", 2usize),
        ("aic_conc_rwlock_close_intrinsic", 1usize),
        ("aic_conc_payload_store_value_intrinsic", 1usize),
        ("aic_conc_payload_take_value_intrinsic", 2usize),
    ] {
        assert_intrinsic_declaration(&source, "std/concurrent.aic", name, arity);
    }
}
#[test]
fn unit_diagnostic_registry_covers_all_emitted_codes() {
    let mut files = Vec::new();
    collect_rs_files(Path::new("src"), &mut files);
    collect_rs_files(Path::new("tests"), &mut files);

    let mut seen = BTreeSet::new();
    for path in files {
        if path.ends_with("src/diagnostic_codes.rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read rust file");
        for code in extract_diag_codes(&text) {
            seen.insert(code);
        }
    }

    for code in &seen {
        assert!(
            aicore::diagnostic_codes::is_registered(code),
            "missing registry entry for {code}"
        );
    }
}

#[test]
fn unit_multi_file_package_loads_and_typechecks() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::create_dir_all(root.join("std")).expect("mkdir std");

    fs::write(
        root.join("aic.toml"),
        "[package]\nname = \"demo\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write manifest");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import app.math;
import std.io;

fn main() -> Int effects { io } capabilities { io } {
    print_int(add(1, 2));
    0
}
"#,
    )
    .expect("write main");

    fs::write(
        root.join("src/math.aic"),
        r#"module app.math;

pub fn add(x: Int, y: Int) -> Int {
    x + y
}
"#,
    )
    .expect("write math");

    fs::write(
        root.join("std/io.aic"),
        r#"module std.io;

fn print_int(x: Int) -> () effects { io } {
    ()
}
"#,
    )
    .expect("write io");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics: {:#?}",
        out.diagnostics
    );
    assert!(out.ir.items.len() >= 2);
}

#[test]
fn unit_std_modules_can_be_resolved_from_global_std_root() {
    let _env_guard = env_lock();

    let dir = tempdir().expect("tempdir");
    let std_root = dir.path().join("global-std");
    let root = dir.path().join("project");
    fs::create_dir_all(std_root.join("std")).expect("mkdir global std");
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        std_root.join("std/io.aic"),
        r#"module std.io;

fn sentinel() -> Int {
    42
}
"#,
    )
    .expect("write io");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.io;

fn main() -> Int {
    sentinel()
}
"#,
    )
    .expect("write main");

    let _std_root_override = ScopedEnvVar::set(
        ENV_AIC_STD_ROOT,
        std_root.join("std").to_string_lossy().to_string(),
    );
    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");

    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_std_module_smoke_compiles_with_effects() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.io;
import std.fs;
import std.net;
import std.time;
import std.rand;
import std.env;
import std.path;
import std.map;
import std.set;
import std.proc;
import std.log;
import std.string;
import std.bytes;
import std.vec;
import std.deque;
import std.option;
import std.result;
import std.concurrent;
import std.http_server;
import std.router;

fn main() -> Int effects { io, fs, net, time, rand, env, proc, concurrency } capabilities { io, fs, net, time, rand, env, proc, concurrency } {
    let _exists = exists("foo.txt");
    let _read = read_text("foo.txt");
    let _write = write_text("foo.txt", "ok");
    let _append = append_text("foo.txt", "!");
    let _copy = copy("foo.txt", "bar.txt");
    let _move = move("bar.txt", "baz.txt");
    let _delete = delete("baz.txt");
    let _meta = metadata("foo.txt");
    let _walk = walk_dir(".");
    let _tmp_file = temp_file("unit_");
    let _tmp_dir = fs.temp_dir("unit_");
    let _env_get = env.get("HOME");
    let _env_set = env.set("AIC_UNIT_TMP", "1");
    let _env_rm = env.remove("AIC_UNIT_TMP");
    let _cwd = cwd();
    let _set_cwd = set_cwd(".");
    let _path_join = path.join("foo", "bar.txt");
    let _header_map: Map[String, String] = map.new_map();
    let _header_map = map.insert(_header_map, "accept", "application/json");
    let _header_value = map.get(_header_map, "accept");
    let _header_has = map.contains_key(_header_map, "accept");
    let _header_keys = map.keys(_header_map);
    let _header_values = map.values(_header_map);
    let _header_entries = map.entries(_header_map);
    let _header_size = map.size(_header_map);
    let _header_removed = map.remove(_header_map, "accept");
    let _set0: Set[String] = set.new_set();
    let _set1 = set.add(_set0, "accept");
    let _set2 = set.add(_set1, "x-id");
    let _set3 = set.discard(_set2, "accept");
    let _set_has = set.has(_set3, "x-id");
    let _set4: Set[String] = set.new_set();
    let _set4 = set.add(_set4, "trace-id");
    let _set_union = set.union(_set3, _set4);
    let _set_inter = set.intersection(_set_union, _set3);
    let _set_diff = set.difference(_set_union, _set3);
    let _set_vec = set.to_vec(_set_union);
    let _set_size = set.set_size(_set_union);
    let _int_set0: Set[Int] = set.new_set();
    let _int_set1 = set.add(_int_set0, 7);
    let _int_set2 = set.discard(_int_set1, 7);
    let _int_set_has = set.has(_int_set2, 7);
    let _bool_set0: Set[Bool] = set.new_set();
    let _bool_set1 = set.add(_bool_set0, true);
    let _bool_set2 = set.discard(_bool_set1, false);
    let _bool_set_has = set.has(_bool_set2, true);
    let _log_level = Info();
    log.set_level(_log_level);
    log.set_json_output(true);
    log.info("unit-smoke");
    let _base = basename(_path_join);
    let _dir = dirname(_path_join);
    let _ext = extension(_path_join);
    let _abs = is_abs(_path_join);
    let _spawn = proc.spawn("echo smoke");
    let _wait = wait(1);
    let _kill = kill(1);
    let _run = run("echo smoke");
    let _pipe = pipe("echo smoke", "cat");
    let _listen = tcp_listen("127.0.0.1:0");
    let _local = tcp_local_addr(1);
    let _accept = tcp_accept(1, 1);
    let _connect = tcp_connect("127.0.0.1:80", 1);
    let _send = tcp_send(1, bytes.from_string("ping"));
    let _recv = tcp_recv(1, 16, 1);
    let _close = tcp_close(1);
    let _udp = udp_bind("127.0.0.1:0");
    let _udp_local = udp_local_addr(1);
    let _udp_send = udp_send_to(1, "127.0.0.1:9", bytes.from_string("ping"));
    let _udp_recv = udp_recv_from(1, 16, 1);
    let _udp_close = udp_close(1);
    let _dns = dns_lookup("localhost");
    let _dns_rev = dns_reverse("127.0.0.1");
    let _srv_listen = http_server.listen("127.0.0.1:0");
    let _srv_accept = http_server.accept(1, 1);
    let _srv_read = http_server.read_request(1, 1024, 1);
    let _srv_resp = http_server.text_response(200, "ok");
    let _srv_write = http_server.write_response(1, _srv_resp);
    let _srv_close = http_server.close(1);
    let _srv_json = http_server.json_response(200, "{\"ok\":true}");
    let _srv_err = http_server.error_response(500, "boom");
    let _srv_hdr = http_server.header(_srv_json, "content-type");
    let _router_new = router.new_router();
    let _router_add = router.add(Router { handle: 1 }, "GET", "/health", 1);
    let _router_match = router.match_route(Router { handle: 1 }, "GET", "/health");
    let _ts = now_ms();
    let _mono = monotonic_ms();
    let _deadline = deadline_after_ms(5);
    let _remain = remaining_ms(_deadline);
    let _expired = timeout_expired(_deadline);
    sleep_until(_deadline);
    seed(123);
    let _r = random_int();
    let _rr = random_range(1, 5);
    let _rb = random_bool();
    let _spawn_task = spawn_task(21, 1);
    let _join_task = join_task(Task { handle: 1 });
    let _timeout_task = timeout_task(Task { handle: 1 }, 1);
    let _cancel_task = cancel_task(Task { handle: 1 });
    let _group = spawn_group(vec.vec_of(21), 1);
    let _first = select_first(vec.vec_of(Task { handle: 1 }), 1);
    let _chan = channel_int(2);
    let _send = send_int(IntChannel { handle: 1 }, 1, 1);
    let _recv = recv_int(IntChannel { handle: 1 }, 1);
    let _close_chan = close_channel(IntChannel { handle: 1 });
    let _mutex = mutex_int(0);
    let _lock = lock_int(IntMutex { handle: 1 }, 1);
    let _unlock = unlock_int(IntMutex { handle: 1 }, 1);
    let _close_mutex = close_mutex(IntMutex { handle: 1 });
    let _gm = new_mutex(vec.vec_of(1));
    let _glock = lock(_gm);
    let _grw = new_rwlock(vec.vec_of(1));
    let _gread = read_lock(_grw);
    let _gwrite = write_lock(_grw);
    let _grw_close = close_rwlock(_grw);
    let _n = string.len("abc");
    let _contains = string.contains("abc", "b");
    let _starts = string.starts_with("abc", "a");
    let _ends = string.ends_with("abc", "c");
    let _index = string.index_of("abc", "b");
    let _last = last_index_of("abcb", "b");
    let _sub = substring("abc", 0, 2);
    let _char = char_at("abc", 1);
    let _parts = split("GET /api/users HTTP/1.1", " ");
    let _first = split_first("Content-Type: application/json", ":");
    let _trim = trim("  hi  ");
    let _trim_start = trim_start("  hi");
    let _trim_end = trim_end("hi  ");
    let _upper = to_upper("abc");
    let _lower = to_lower("ABC");
    let _replace = replace("a-b-c", "-", "/");
    let _repeat = repeat("ab", 2);
    let _parse = parse_int("42");
    let _int_s = int_to_string(42);
    let _bool_s = bool_to_string(true);
    let _bytes = string_to_bytes("hello");
    let _bytes_ok = bytes_to_string(_bytes);
    let _bytes_lossy = bytes_to_string_lossy(string_to_bytes("hello"));
    let _bytes_valid = string.is_valid_utf8(string_to_bytes("hello"));
    let _byte_len = byte_length("hello");
    let _ascii = is_ascii("hello");
    let _joined_parts = string.join(_parts, "|");
    let _v0: Vec[Int] = vec.new_vec();
    let _v0_cap: Vec[Int] = vec.new_vec_with_capacity(4);
    let _v0_reserve = vec.reserve(_v0_cap, 2);
    let _v0_shrink = vec.shrink_to_fit(_v0_reserve);
    let _v1 = vec.push(_v0, 1);
    let _v2 = vec.insert(_v1, 0, 0);
    let _v3 = vec.set(_v2, 1, 2);
    let _v4 = vec.remove_at(_v3, 0);
    let _v5 = vec.pop(_v4);
    let _v6 = vec.reverse(_v5);
    let _v7 = vec.slice(_v6, 0, 1);
    let _v8 = vec.append(_v7, vec.vec_of(9));
    let _v9 = vec.clear(_v8);
    let _vg = vec.get(_v9, 0);
    let _vf = vec.first(_v9);
    let _vl = vec.last(_v9);
    let _vc = vec.contains(_v9, 0);
    let _vi = vec.index_of(_v9, 0);
    let _ve = vec.is_empty(_v9);
    let _v_sorted = vec.sort(_v9, |a: Int, b: Int| -> Bool { a < b });
    let _v_find = vec.find(_v_sorted, |x: Int| -> Bool { x == 9 });
    let _v_any = vec.any(_v_sorted, |x: Int| -> Bool { x >= 0 });
    let _v_all = vec.all(_v_sorted, |x: Int| -> Bool { x >= 0 });
    let _v_count = vec.count(_v_sorted, |x: Int| -> Bool { x == 9 });
    let _v_zip = vec.zip(_v_sorted, vec.vec_of("x"));
    let _v_enum = vec.enumerate(_v_sorted);
    let _dq0: Deque[Int] = new_deque();
    let _dq1 = push_back(_dq0, 10);
    let _dq2 = push_front(_dq1, 5);
    let (_dq_front, _dq3) = pop_front(_dq2);
    let (_dq_back, _dq4) = pop_back(_dq3);
    let _dq_len = deque_len(_dq4);
    let _q0: Queue[Int] = new_queue();
    let _q1 = enqueue(_q0, 7);
    let (_q_head, _q2) = dequeue(_q1);
    let _q_len = queue_len(_q2);
    print_int(1);
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_std_io_effect_is_enforced() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.io;

fn main() -> Int {
    read_line();
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2001"));
}

#[test]
fn unit_std_effects_are_enforced() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.fs;

fn main() -> Int {
    if exists("foo.txt") { 1 } else { 0 }
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2001"));
}

#[test]
fn unit_std_fs_read_effect_is_enforced() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.fs;

fn main() -> Int {
    read_text("foo.txt");
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2001"));
}

#[test]
fn unit_std_env_effect_is_enforced() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.env;

fn main() -> Int {
    get("HOME");
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2001"));
}

#[test]
fn unit_std_proc_effect_is_enforced() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.proc;

fn main() -> Int {
    run("echo hi");
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2001"));
}

#[test]
fn unit_std_time_effect_is_enforced() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.time;

fn main() -> Int {
    now_ms()
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2001"));
}

#[test]
fn unit_std_rand_effect_is_enforced() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.rand;

fn main() -> Int {
    random_int()
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2001"));
}

#[test]
fn unit_std_concurrency_effect_is_enforced() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.concurrent;

fn main() -> Int {
    let _task = spawn_task(1, 1);
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2001"));
}

#[test]
fn unit_std_concurrency_spawn_rejects_non_send_payload() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.concurrent;
import std.fs;

struct Payload {
    file: FileHandle,
}

fn main() -> Int effects { concurrency } capabilities { concurrency } {
    let payload = Payload { file: FileHandle { handle: 1 } };
    let _task: Task[Payload] = spawn(|| -> Payload { payload });
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
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
fn unit_std_concurrency_send_rejects_non_send_payload() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.concurrent;
import std.fs;

fn main() -> Int effects { concurrency } capabilities { concurrency } {
    let pair: (Sender[FileHandle], Receiver[FileHandle]) = buffered_channel(1);
    let tx = pair.0;
    let _sent = send(tx, FileHandle { handle: 2 });
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "E1258" && d.message.contains("Send")),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_std_concurrency_arc_payload_is_send_safe_for_spawn() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.concurrent;
import std.fs;

struct Payload {
    file: FileHandle,
}

fn main() -> Int effects { concurrency } capabilities { concurrency } {
    let shared: Arc[Payload] = Arc { handle: 1 };
    let _task: Task[Arc[Payload]] = spawn(|| -> Arc[Payload] { shared });
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.code == "E1258" && d.message.contains("Send")),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_std_concurrency_atomic_wrappers_are_send_safe_for_spawn() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.concurrent;

fn main() -> Int effects { concurrency } capabilities { concurrency } {
    let counter = atomic_int(0);
    let flag = atomic_bool(false);
    let _t1: Task[AtomicInt] = spawn(|| -> AtomicInt { counter });
    let _t2: Task[AtomicBool] = spawn(|| -> AtomicBool { flag });
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.code == "E1258" && d.message.contains("Send")),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_std_concurrency_thread_local_wrapper_is_send_safe_for_spawn() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.concurrent;

fn main() -> Int effects { concurrency } capabilities { concurrency } {
    let tl = thread_local(|| -> Int { 1 });
    let _task: Task[Int] = spawn(|| -> Int {
        tl_set(tl, 2);
        tl_get(tl)
    });
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.code == "E1258" && d.message.contains("Send")),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_std_pool_public_apis_are_present() {
    let source = fs::read_to_string("std/pool.aic").expect("read std/pool.aic");
    assert!(source.contains("enum PoolError {"));
    assert!(source.contains("struct PoolConfig {"));
    assert!(source.contains("struct Pool[T] {"));
    assert!(source.contains("struct PooledConn[T] {"));
    assert!(source.contains("struct PoolStats {"));
    assert!(source.contains("fn new_pool[T]("));
    assert!(source.contains(
        "fn acquire[T](pool: Pool[T]) -> Result[PooledConn[T], PoolError] effects { concurrency }"
    ));
    assert!(source.contains("fn release[T](conn: PooledConn[T]) -> () effects { concurrency }"));
    assert!(source.contains("fn discard[T](conn: PooledConn[T]) -> () effects { concurrency }"));
    assert!(source.contains("fn pool_stats[T](pool: Pool[T]) -> PoolStats effects { concurrency }"));
    assert!(source.contains("fn close_pool[T](pool: Pool[T]) -> () effects { concurrency }"));
}

#[test]
fn unit_std_pool_module_typechecks_for_basic_usage() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.pool;

struct Conn {
    id: Int,
    healthy: Bool,
}

fn main() -> Int effects { concurrency } capabilities { concurrency } {
    let create_cb: Fn() -> Result[Conn, PoolError] =
        || -> Result[Conn, PoolError] { Ok(Conn { id: 1, healthy: true }) };
    let check_cb: Fn(Conn) -> Bool = |conn: Conn| -> Bool { conn.healthy };
    let destroy_cb: Fn(Conn) -> () = |conn: Conn| -> () { () };

    let pool_result: Result[Pool[Conn], PoolError] = new_pool(
        PoolConfig {
            min_size: 1,
            max_size: 2,
            acquire_timeout_ms: 10,
            idle_timeout_ms: 5,
            max_lifetime_ms: 20,
            health_check_ms: 5,
        },
        create_cb,
        check_cb,
        destroy_cb,
    );
    let pool: Pool[Conn] = match pool_result {
        Ok(p) => p,
        Err(_) => Pool { handle: 0 },
    };

    let stats0 = pool_stats(pool);
    let maybe_id = match acquire(pool) {
        Ok(conn) => if true {
            let id = conn.value.id;
            release(conn);
            id
        } else {
            0
        },
        Err(_) => 0,
    };

    let stats1 = pool_stats(pool);
    close_pool(pool);
    if stats0.total >= 0 && stats1.idle >= 0 && maybe_id >= 0 {
        0
    } else {
        1
    }
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "unexpected diagnostics={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_std_env_cwd_requires_fs_effect() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.env;

fn main() -> Int effects { env } {
    cwd();
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2001"));
}

#[test]
fn unit_deprecated_std_api_emits_warning() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.time;

fn main() -> Int effects { time } capabilities { time } {
    now()
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "unexpected errors: {:#?}",
        out.diagnostics
    );
    assert!(out
        .diagnostics
        .iter()
        .any(|d| { d.code == "E6001" && matches!(d.severity, Severity::Warning) }));
}

#[test]
fn unit_deprecated_std_concurrency_apis_emit_migration_hints() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.concurrent;

fn unwrap_channel(v: Result[IntChannel, ConcurrencyError]) -> IntChannel {
    match v {
        Ok(ch) => ch,
        Err(_) => IntChannel { handle: 0 },
    }
}

fn unwrap_mutex(v: Result[IntMutex, ConcurrencyError]) -> IntMutex {
    match v {
        Ok(m) => m,
        Err(_) => IntMutex { handle: 0 },
    }
}

fn main() -> Int effects { concurrency } capabilities { concurrency } {
    let ch = unwrap_channel(channel_int(1));
    let _sent = send_int(ch, 7, 1000);
    let _recv = recv_int(ch, 1000);
    let m = unwrap_mutex(mutex_int(1));
    let v = lock_int(m, 1000);
    let _released = match v {
        Ok(value) => unlock_int(m, value + 1),
        Err(_) => unlock_int(m, 0),
    };
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "unexpected errors: {:#?}",
        out.diagnostics
    );

    let warnings = out
        .diagnostics
        .iter()
        .filter(|d| d.code == "E6001" && matches!(d.severity, Severity::Warning))
        .collect::<Vec<_>>();
    assert!(
        warnings.len() >= 6,
        "expected deprecation warnings for channel_int/send_int/recv_int/mutex_int/lock_int/unlock_int, got {warnings:#?}"
    );
    assert!(warnings.iter().any(|d| d
        .help
        .iter()
        .any(|h| h.contains("std.concurrent.channel[Int]"))));
    assert!(warnings.iter().any(|d| d
        .help
        .iter()
        .any(|h| h.contains("std.concurrent.send[Int]"))));
    assert!(warnings.iter().any(|d| d
        .help
        .iter()
        .any(|h| h.contains("std.concurrent.recv[Int]"))));
    assert!(warnings.iter().any(|d| d
        .help
        .iter()
        .any(|h| h.contains("std.concurrent.new_mutex[Int]"))));
    assert!(warnings.iter().any(|d| d
        .help
        .iter()
        .any(|h| h.contains("std.concurrent.lock[Int]"))));
    assert!(warnings.iter().any(|d| d
        .help
        .iter()
        .any(|h| h.contains("std.concurrent.unlock_guard[Int]"))));
}

#[test]
fn unit_missing_module_reports_e2100() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        "module app.main;\nimport app.missing;\nfn main() -> Int { 0 }\n",
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2100"));
}

#[test]
fn unit_unimported_transitive_symbol_reports_e2102() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import app.math;

fn main() -> Int {
    hidden()
}
"#,
    )
    .expect("write main");

    fs::write(
        root.join("src/math.aic"),
        r#"module app.math;
import app.util;

fn add(x: Int, y: Int) -> Int {
    x + y
}
"#,
    )
    .expect("write math");

    fs::write(
        root.join("src/util.aic"),
        r#"module app.util;

fn hidden() -> Int {
    1
}
"#,
    )
    .expect("write util");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2102"));
}

#[test]
fn unit_root_entry_allows_imported_module_internal_dependencies() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"import app.api;

fn main() -> Int {
    api.handle()
}
"#,
    )
    .expect("write main");

    fs::write(
        root.join("src/api.aic"),
        r#"module app.api;
import app.util;

pub fn handle() -> Int {
    answer()
}
"#,
    )
    .expect("write api");

    fs::write(
        root.join("src/util.aic"),
        r#"module app.util;

pub fn answer() -> Int {
    42
}
"#,
    )
    .expect("write util");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_qualified_module_call_resolves() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import app.math;

fn main() -> Int {
    math.add(40, 2)
}
"#,
    )
    .expect("write main");

    fs::write(
        root.join("src/math.aic"),
        r#"module app.math;

pub fn add(x: Int, y: Int) -> Int {
    x + y
}
"#,
    )
    .expect("write math");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_ambiguous_imported_symbol_reports_e2104() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import app.math;
import app.more;

fn main() -> Int {
    add(1, 2)
}
"#,
    )
    .expect("write main");

    fs::write(
        root.join("src/math.aic"),
        r#"module app.math;

pub fn add(x: Int, y: Int) -> Int {
    x + y
}
"#,
    )
    .expect("write math");

    fs::write(
        root.join("src/more.aic"),
        r#"module app.more;

pub fn add(x: Int, y: Int) -> Int {
    x - y
}
"#,
    )
    .expect("write more");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2104"));
}

#[test]
fn unit_pub_and_pub_crate_functions_are_cross_module_accessible() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import app.lib;

fn main() -> Int {
    lib.public_add(20, 22) + lib.crate_add(0, 0)
}
"#,
    )
    .expect("write main");

    fs::write(
        root.join("src/lib.aic"),
        r#"module app.lib;

pub fn public_add(x: Int, y: Int) -> Int {
    x + y
}

pub(crate) fn crate_add(x: Int, y: Int) -> Int {
    x + y
}

fn hidden_add(x: Int, y: Int) -> Int {
    x + y
}
"#,
    )
    .expect("write lib");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_private_function_access_reports_e2102() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import app.lib;

fn main() -> Int {
    lib.hidden_add(1, 2)
}
"#,
    )
    .expect("write main");

    fs::write(
        root.join("src/lib.aic"),
        r#"module app.lib;

fn hidden_add(x: Int, y: Int) -> Int {
    x + y
}
"#,
    )
    .expect("write lib");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2102"));
}

#[test]
fn unit_struct_field_visibility_blocks_private_field_access() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import app.model;

fn main() -> Int {
    let person = model.new_person(42);
    person.secret
}
"#,
    )
    .expect("write main");

    fs::write(
        root.join("src/model.aic"),
        r#"module app.model;

pub struct Person {
    pub age: Int,
    secret: Int,
}

pub fn new_person(age: Int) -> Person {
    Person { age: age, secret: 7 }
}
"#,
    )
    .expect("write model");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2102"));
}

#[test]
fn unit_user_intrinsic_calls_are_rejected_even_when_qualified() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.string;

fn main() -> Int {
    let _raw = aic_string_len_intrinsic("abc");
    let _qualified = string.aic_string_len_intrinsic("abc");
    0
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    let intrinsic_errors = out
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == "E2102" && d.message.contains("private runtime implementation detail")
        })
        .count();
    assert!(intrinsic_errors >= 2, "diagnostics={:#?}", out.diagnostics);
}

#[test]
fn unit_malformed_pub_visibility_reports_e1090() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
pub(package) fn main() -> Int {
    0
}
"#,
    )
    .expect("write source");
    let out = run_frontend(&path).expect("frontend");
    assert!(
        out.diagnostics.iter().any(|d| d.code == "E1090"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_visibility_modifier_on_type_alias_reports_e1091() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
pub type Count = Int;

fn main() -> Int {
    let value: Count = 1;
    value
}
"#,
    )
    .expect("write source");
    let out = run_frontend(&path).expect("frontend");
    assert!(
        out.diagnostics.iter().any(|d| d.code == "E1091"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_named_arguments_allow_reordered_calls() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
fn connect(host: Int, port: Int, timeout_ms: Int, retry: Bool) -> Int {
    if retry {
        host + port + timeout_ms
    } else {
        0
    }
}

fn main() -> Int {
    connect(timeout_ms: 30, retry: true, host: 10, port: 2)
}
"#,
    )
    .expect("write source");
    let out = run_frontend(&path).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_named_arguments_allow_positional_then_named() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
fn connect(host: Int, port: Int, timeout_ms: Int, retry: Bool) -> Int {
    if retry {
        host + port + timeout_ms
    } else {
        0
    }
}

fn main() -> Int {
    connect(10, port: 2, timeout_ms: 30, retry: true)
}
"#,
    )
    .expect("write source");
    let out = run_frontend(&path).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_named_arguments_unknown_name_reports_suggestion() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
fn connect(host: Int, port: Int, timeout_ms: Int, retry: Bool) -> Int {
    if retry {
        host + port + timeout_ms
    } else {
        0
    }
}

fn main() -> Int {
    connect(host: 10, porrt: 2, timeout_ms: 30, retry: true)
}
"#,
    )
    .expect("write source");
    let out = run_frontend(&path).expect("frontend");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "E1213" && d.message.contains("unknown named argument 'porrt'")),
        "diags={:#?}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| { d.help.iter().any(|h| h.contains("did you mean 'port'")) }),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_named_arguments_reject_positional_after_named() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
fn connect(host: Int, port: Int, timeout_ms: Int, retry: Bool) -> Int {
    if retry {
        host + port + timeout_ms
    } else {
        0
    }
}

fn main() -> Int {
    connect(host: 10, 2, timeout_ms: 30, retry: true)
}
"#,
    )
    .expect("write source");
    let out = run_frontend(&path).expect("frontend");
    assert!(
        out.diagnostics.iter().any(|d| d.code == "E1092"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_namespace_type_value_shadowing_passes() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;

struct Token {
    x: Int,
}

fn Token(x: Int) -> Int {
    x
}

fn main() -> Int {
    let t = Token { x: 7 };
    Token(t.x)
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_namespace_type_collision_reports_e1100() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;

struct Token {
    x: Int,
}

enum Token {
    A,
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1100"));
}

#[test]
fn unit_parser_recovery_reports_multiple_errors() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;

fn main() -> Int {
    let x = ;
    let y = ;
    return
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        out.diagnostics.len() >= 3,
        "expected multiple diagnostics, got {:#?}",
        out.diagnostics
    );
    assert!(out.diagnostics.iter().any(|d| d.code == "E1041"));
}

#[test]
fn unit_generic_function_and_struct_inference_passes() {
    let src = r#"
struct Box[T] { value: T }

fn id[T](x: T) -> T { x }

fn unbox[T](b: Box[T]) -> T { b.value }

fn main() -> Int {
    let b = Box { value: id(41) };
    unbox(b) + 1
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        out.diagnostics.is_empty(),
        "type diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_generic_inference_refines_from_first_use() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
import std.vec;

fn main() -> Int {
    let values = vec.new_vec();
    let values_next = vec.push(values, 41);
    vec.vec_len(values_next)
}
"#,
    )
    .expect("write source");
    let out = run_frontend(&path).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_vec_capacity_apis_typecheck_and_infer() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(
        &path,
        r#"
import std.vec;

fn main() -> Int {
    let mut values = vec.new_vec_with_capacity(2);
    values = vec.push(values, 1);
    values = vec.reserve(values, 4);
    values = vec.push(values, 2);
    values = vec.shrink_to_fit(values);
    vec.vec_len(values)
}
"#,
    )
    .expect("write source");
    let out = run_frontend(&path).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_generic_constraint_mismatch_reports_e1214() {
    let src = r#"
fn pair_first[T](x: T, y: T) -> T { x }

fn main() -> Int {
    pair_first(1, true)
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1214"));
}

#[test]
fn unit_wrong_generic_arity_reports_e1250() {
    let src = r#"
fn main() -> Int {
    let x: Option[Int, Int] = None;
    0
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1250"));
}

#[test]
fn unit_generic_instantiation_metadata_is_deduped_and_stable() {
    let src = r#"
fn map_option[T](x: Option[T]) -> Option[T] {
    match x {
        Some(v) => Some(v),
        None => None(),
    }
}

fn main() -> Int {
    let first = map_option(Some(41));
    let second = map_option(first);
    match second {
        Some(v) => v,
        None => 0,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");

    let out1 = check(&ir, &res, "unit.aic");
    assert!(
        out1.diagnostics.is_empty(),
        "type diags={:#?}",
        out1.diagnostics
    );
    let out2 = check(&ir, &res, "unit.aic");
    assert_eq!(
        out1.generic_instantiations, out2.generic_instantiations,
        "instantiation metadata must be deterministic"
    );

    let map_option_instantiations = out1
        .generic_instantiations
        .iter()
        .filter(|inst| {
            inst.kind == aicore::ir::GenericInstantiationKind::Function && inst.name == "map_option"
        })
        .collect::<Vec<_>>();
    assert_eq!(
        map_option_instantiations.len(),
        1,
        "expected deduplicated instantiation"
    );
    assert_eq!(
        map_option_instantiations[0].type_args,
        vec!["Int".to_string()]
    );
}

#[test]
fn unit_frontend_ir_contains_generic_instantiation_metadata() {
    let dir = tempdir().expect("tempdir");
    let source_path = dir.path().join("main.aic");
    fs::write(
        &source_path,
        r#"
fn id[T](x: T) -> T { x }

fn main() -> Int {
    id(41)
}
"#,
    )
    .expect("write source");

    let out = run_frontend(&source_path).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics={:#?}",
        out.diagnostics
    );
    assert!(
        out.ir
            .generic_instantiations
            .iter()
            .any(|inst| inst.name == "id" && inst.type_args == vec!["Int".to_string()]),
        "expected concrete generic instantiation in IR"
    );
}

#[test]
fn unit_struct_literal_duplicate_field_reports_e1254() {
    let src = r#"
struct Pair {
    x: Int,
}

fn main() -> Int {
    let p = Pair { x: 1, x: 2 };
    p.x
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1254"));
}

#[test]
fn unit_variant_payload_mismatch_reports_e1216() {
    let src = r#"
enum Response {
    Success(Int),
}

fn main() -> Int {
    let resp = Success(true);
    0
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1216"));
}

#[test]
fn unit_variant_arity_mismatch_reports_e1215() {
    let src = r#"
enum Response {
    Success(Int),
}

fn main() -> Int {
    let resp = Success();
    0
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1215"));
}

#[test]
fn unit_field_access_unknown_member_reports_e1228() {
    let src = r#"
struct Pair {
    x: Int,
}

fn main() -> Int {
    let p = Pair { x: 1 };
    p.y
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1228"));
}

#[test]
fn unit_bool_match_non_exhaustive_reports_e1246() {
    let src = r#"
fn f(x: Bool) -> Int {
    match x {
        true => 1,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1246"));
}

#[test]
fn unit_result_match_non_exhaustive_reports_e1248() {
    let src = r#"
fn f(x: Result[Int, Int]) -> Int {
    match x {
        Ok(v) => v,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1248"));
}

#[test]
fn unit_unreachable_match_arm_reports_e1251() {
    let src = r#"
fn f(x: Bool) -> Int {
    match x {
        _ => 1,
        true => 2,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1251"));
}

#[test]
fn unit_duplicate_pattern_binding_reports_e1252() {
    let src = r#"
fn f(x: Option[Int]) -> Int {
    match x {
        Some(v, v) => v,
        None => 0,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1252"));
}

#[test]
fn unit_or_pattern_bool_match_is_exhaustive() {
    let src = r#"
fn f(x: Bool) -> Int {
    match x {
        true | false => 1,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !has_errors(&out.diagnostics),
        "unexpected diagnostics={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_guarded_arm_does_not_satisfy_exhaustiveness() {
    let src = r#"
fn f(x: Bool, allow: Bool) -> Int {
    match x {
        true if allow => 1,
        false => 0,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1246"));
}

#[test]
fn unit_match_guard_must_be_bool_reports_e1270() {
    let src = r#"
fn f(x: Bool) -> Int {
    match x {
        true if 1 => 1,
        false => 0,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1270"));
}

#[test]
fn unit_or_pattern_binding_set_mismatch_reports_e1271() {
    let src = r#"
fn f(x: Option[Int]) -> Int {
    match x {
        Some(v) | None => 1,
        _ => 0,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1271"));
}

#[test]
fn unit_or_pattern_binding_type_mismatch_reports_e1272() {
    let src = r#"
enum Mixed {
    A(Int),
    B(Bool),
}

fn f(x: Mixed) -> Int {
    match x {
        A(v) | B(v) => 1,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1272"));
}

#[test]
fn unit_or_pattern_redundant_arm_reports_e1251() {
    let src = r#"
fn f(x: Bool) -> Int {
    match x {
        true | false => 1,
        true => 2,
    }
}
"#;
    let ir = lower(src);
    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1251"));
}

#[test]
fn unit_parser_rejects_null_literal_with_e1051() {
    let src = "fn main() -> Int { null }";
    let (_program, diags) = parse(src, "unit.aic");
    assert!(diags.iter().any(|d| d.code == "E1051"));
}

#[test]
fn unit_typecheck_rejects_null_symbol_at_ir_boundary() {
    let mut ir = lower("fn main() -> Int { 0 }");
    let symbol = ir
        .symbols
        .iter_mut()
        .find(|s| matches!(s.kind, aicore::ir::SymbolKind::Function))
        .expect("function symbol");
    symbol.name = "null".to_string();

    let (res, diags) = resolve(&ir, "unit.aic");
    assert!(diags.is_empty(), "resolver diags={diags:#?}");
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1253"));
}

#[test]
fn unit_type_alias_and_const_usage_typechecks() {
    let src = r#"
type Count = Int;
type Score[T] = Result[T, Int];

const BASE: Count = 40;
const BONUS: Count = BASE + 2;

fn wrap(v: Count) -> Score[Count] {
    Ok(v)
}

fn main() -> Count {
    let value: Score[Count] = wrap(BONUS);
    match value {
        Ok(n) => n,
        Err(e) => e,
    }
}
"#;
    let (program, parse_diags) = parse(src, "unit.aic");
    assert!(parse_diags.is_empty(), "parse diags={parse_diags:#?}");
    let ir = build(&program.expect("program"));
    let formatted = format_program(&ir);
    assert!(formatted.contains("type Count = Int;"));
    assert!(formatted.contains("const BONUS: Count = BASE + 2;"));

    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(
        resolve_diags.is_empty(),
        "resolver diags={resolve_diags:#?}"
    );
    let out = check(&ir, &res, "unit.aic");
    assert!(
        !has_errors(&out.diagnostics),
        "typecheck diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_codegen_supports_type_alias_and_const_items() {
    let src = r#"
type Count = Int;
type Score[T] = Result[T, Int];

const BASE: Count = 40;
const STEP: Count = 1 + 1;
const READY: Bool = (STEP == 2) && true;
const BONUS: Count = BASE + STEP;
const FINAL: Count = -(-BONUS);

fn wrap(v: Count) -> Score[Count] {
    Ok(v)
}

fn main() -> Count {
    let score: Score[Count] = wrap(FINAL);
    if READY {
        match score {
            Ok(v) => v,
            Err(e) => e,
        }
    } else {
        0
    }
}
"#;
    let ir = lower(src);
    let llvm = emit_llvm(&ir, "unit.aic").expect("emit llvm");
    assert!(
        !llvm.llvm_ir.contains("__aic_type_alias__"),
        "internal type alias pseudo-items must not be emitted as runtime functions"
    );
    assert!(
        !llvm.llvm_ir.contains("__aic_const__"),
        "internal const pseudo-items must not be emitted as runtime functions"
    );
    assert!(llvm.llvm_ir.contains("define i64 @aic_main()"));
}

#[test]
fn unit_codegen_reports_unsupported_const_initializer_forms() {
    let src = r#"
const BAD: Int = if true { 1 } else { 2 };
fn main() -> Int { BAD }
"#;
    let ir = lower(src);
    let diags = match emit_llvm(&ir, "unit.aic") {
        Ok(_) => panic!("expected codegen failure"),
        Err(diags) => diags,
    };
    assert!(
        diags.iter().any(|d| {
            d.code == "E5023"
                && d.message
                    .contains("const 'BAD' initializer uses unsupported `if` expression")
        }),
        "diags={diags:#?}"
    );
}

#[test]
fn unit_codegen_reports_missing_runtime_lowering_for_intrinsic_declaration() {
    let src = r#"
module std.codegen_intrinsic_test;

intrinsic fn aic_missing_runtime_intrinsic(x: Int) -> Int;

fn main() -> Int {
    aic_missing_runtime_intrinsic(1)
}
"#;
    let ir = lower(src);
    let diags = match emit_llvm(&ir, "unit.aic") {
        Ok(_) => panic!("expected codegen failure"),
        Err(diags) => diags,
    };
    assert!(
        diags.iter().any(|d| {
            d.code == "E5020"
                && d.message.contains(
                    "missing runtime lowering for intrinsic 'aic_missing_runtime_intrinsic'",
                )
        }),
        "diags={diags:#?}"
    );
}

#[test]
fn unit_type_alias_name_is_preserved_in_return_diagnostic() {
    let src = r#"
type Meter = Int;
fn bad() -> Meter {
    true
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(
        resolve_diags.is_empty(),
        "resolver diags={resolve_diags:#?}"
    );
    let out = check(&ir, &res, "unit.aic");
    assert!(out
        .diagnostics
        .iter()
        .any(|d| d.code == "E1202" && d.message.contains("Meter")));
}

#[test]
fn unit_const_initializer_rejects_function_calls() {
    let src = r#"
fn value() -> Int { 1 }
const BAD: Int = value();
fn main() -> Int { BAD }
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(
        resolve_diags.is_empty(),
        "resolver diags={resolve_diags:#?}"
    );
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1287"));
}

#[test]
fn unit_generic_type_alias_arity_is_checked() {
    let src = r#"
type Wrap[T] = Option[T];
fn bad(x: Wrap[Int, Int]) -> Int {
    0
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = resolve(&ir, "unit.aic");
    assert!(
        resolve_diags.is_empty(),
        "resolver diags={resolve_diags:#?}"
    );
    let out = check(&ir, &res, "unit.aic");
    assert!(out.diagnostics.iter().any(|d| d.code == "E1250"));
}

#[test]
fn unit_error_context_chain_helpers_typecheck() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.io;
import std.fs;
import std.net;
import std.proc;
import std.env;

fn io_code(err: IoError) -> Int {
    match err {
        EndOfInput => 1,
        InvalidInput => 2,
        Io => 3,
    }
}

fn main() -> Int {
    let fs_chain_ctx = with_context(from_fs_error_with_context(NotFound(), "open config"), "bootstrap");
    let chain = error_chain(fs_chain_ctx);

    let score =
        io_code(io_error(from_fs_error_with_context(NotFound(), "open config"))) * 1000 +
        io_code(io_error(from_net_error_with_context(Timeout(), "dial upstream"))) * 100 +
        io_code(io_error(from_proc_error_with_context(InvalidInput(), "spawn child"))) * 10 +
        io_code(io_error(from_env_error_with_context(NotFound(), "load HOME")));

    if chain == "open config -> fs.NotFound -> io.EndOfInput -> bootstrap" {
        score
    } else {
        0
    }
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics={:#?}",
        out.diagnostics
    );
}

#[test]
fn unit_io_error_mapping_without_context_remains_compatible() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.io;
import std.fs;
import std.net;
import std.proc;
import std.env;

fn io_code(err: IoError) -> Int {
    match err {
        EndOfInput => 1,
        InvalidInput => 2,
        Io => 3,
    }
}

fn main() -> Int {
    io_code(from_fs_error(NotFound())) * 10000 +
    io_code(from_fs_error(InvalidInput())) * 1000 +
    io_code(from_net_error(Timeout())) * 100 +
    io_code(from_proc_error(InvalidInput())) * 10 +
    io_code(from_env_error(NotFound()))
}
"#,
    )
    .expect("write main");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diagnostics={:#?}",
        out.diagnostics
    );
}

fn collect_rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, out);
            } else if path.extension().and_then(|x| x.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
}

fn extract_diag_codes(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 5 <= bytes.len() {
        if bytes[i] == b'E'
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
            && bytes[i + 4].is_ascii_digit()
        {
            out.push(text[i..i + 5].to_string());
            i += 5;
            continue;
        }
        i += 1;
    }
    out
}

#[test]
fn unit_std_bytes_intrinsics_are_runtime_backed_and_public_apis_delegate() {
    let bytes_source = fs::read_to_string("std/bytes.aic").expect("read std/bytes.aic");

    assert_delegate_call(
        &bytes_source,
        "std/bytes.aic",
        "byte_len",
        "aic_bytes_len_intrinsic",
        1,
    );
    assert_delegate_call(
        &bytes_source,
        "std/bytes.aic",
        "to_string_lossy",
        "aic_bytes_to_string_lossy_intrinsic",
        1,
    );
    assert_delegate_call(
        &bytes_source,
        "std/bytes.aic",
        "is_valid_utf8",
        "aic_bytes_is_valid_utf8_intrinsic",
        1,
    );

    assert!(
        bytes_source.contains("aic_string_len_intrinsic(data)"),
        "std/bytes.aic must implement byte_len via string intrinsic"
    );
    assert!(
        bytes_source.contains("aic_string_format_intrinsic(\"{0}{1}\", pieces)"),
        "std/bytes.aic must implement concat via string intrinsic"
    );
    assert!(
        bytes_source.contains("aic_string_is_valid_utf8_intrinsic(data)"),
        "std/bytes.aic must validate UTF-8 via intrinsic-backed string API"
    );
    assert!(
        bytes_source.contains("aic_string_bytes_to_string_lossy_intrinsic(data)"),
        "std/bytes.aic must decode lossy via intrinsic-backed string API"
    );
    assert!(
        bytes_source
            .contains("intrinsic fn aic_bytes_byte_at_intrinsic(data: String, index: Int) -> Int;"),
        "std/bytes.aic must declare byte_at runtime intrinsic binding"
    );
    assert!(
        bytes_source.contains(
            "intrinsic fn aic_bytes_from_byte_values_intrinsic(values: Vec[Int]) -> String;"
        ),
        "std/bytes.aic must declare from_byte_values runtime intrinsic binding"
    );
    assert!(
        bytes_source.contains("aic_bytes_byte_at_intrinsic(data.data, index)"),
        "std/bytes.aic byte_at must bridge Bytes.data into runtime intrinsic"
    );
    assert!(
        bytes_source.contains("aic_string_substring_intrinsic(data.data, start, end)"),
        "std/bytes.aic byte_slice must use string substring intrinsic on byte indices"
    );
    assert!(
        bytes_source.contains("aic_bytes_from_byte_values_intrinsic(values)"),
        "std/bytes.aic from_byte_values must bridge Vec[Int] through runtime intrinsic"
    );
    assert!(
        bytes_source.contains("fn to_byte_values(data: Bytes) -> Vec[Int]"),
        "std/bytes.aic must expose byte-vector conversion helper"
    );

    assert!(
        !bytes_source.contains("fn aic_bytes_len_intrinsic(data: String) -> Int {\n    0"),
        "std/bytes.aic len intrinsic must not be a constant stub"
    );
    assert!(
        !bytes_source.contains(
            "fn aic_bytes_concat_intrinsic(left: String, right: String) -> String {\n    \"\""
        ),
        "std/bytes.aic concat intrinsic must not be a constant stub"
    );
    assert!(
        !bytes_source
            .contains("fn aic_bytes_is_valid_utf8_intrinsic(data: String) -> Bool {\n    false"),
        "std/bytes.aic UTF-8 intrinsic must not be a constant stub"
    );
    assert!(
        !bytes_source
            .contains("fn aic_bytes_to_string_lossy_intrinsic(data: String) -> String {\n    \"\""),
        "std/bytes.aic lossy intrinsic must not be a constant stub"
    );
}

#[test]
fn unit_std_buffer_intrinsics_are_declared_and_public_apis_delegate() {
    let source = fs::read_to_string("std/buffer.aic").expect("read std/buffer.aic");

    for (wrapper, intrinsic, arity) in [
        ("new_buffer", "aic_buffer_new_intrinsic", 1usize),
        (
            "new_growable_buffer",
            "aic_buffer_new_growable_intrinsic",
            2usize,
        ),
        (
            "buffer_from_bytes",
            "aic_buffer_from_bytes_intrinsic",
            1usize,
        ),
        ("buffer_to_bytes", "aic_buffer_to_bytes_intrinsic", 1usize),
        ("buf_position", "aic_buffer_position_intrinsic", 1usize),
        ("buf_remaining", "aic_buffer_remaining_intrinsic", 1usize),
        ("buf_seek", "aic_buffer_seek_intrinsic", 2usize),
        ("buf_reset", "aic_buffer_reset_intrinsic", 1usize),
        ("buf_close", "aic_buffer_close_intrinsic", 1usize),
        ("buf_read_u8", "aic_buffer_read_u8_intrinsic", 1usize),
        (
            "buf_read_i16_be",
            "aic_buffer_read_i16_be_intrinsic",
            1usize,
        ),
        (
            "buf_read_u16_be",
            "aic_buffer_read_u16_be_intrinsic",
            1usize,
        ),
        (
            "buf_read_i32_be",
            "aic_buffer_read_i32_be_intrinsic",
            1usize,
        ),
        (
            "buf_read_u32_be",
            "aic_buffer_read_u32_be_intrinsic",
            1usize,
        ),
        (
            "buf_read_i64_be",
            "aic_buffer_read_i64_be_intrinsic",
            1usize,
        ),
        (
            "buf_read_u64_be",
            "aic_buffer_read_u64_be_intrinsic",
            1usize,
        ),
        (
            "buf_read_i16_le",
            "aic_buffer_read_i16_le_intrinsic",
            1usize,
        ),
        (
            "buf_read_u16_le",
            "aic_buffer_read_u16_le_intrinsic",
            1usize,
        ),
        (
            "buf_read_i32_le",
            "aic_buffer_read_i32_le_intrinsic",
            1usize,
        ),
        (
            "buf_read_u32_le",
            "aic_buffer_read_u32_le_intrinsic",
            1usize,
        ),
        (
            "buf_read_i64_le",
            "aic_buffer_read_i64_le_intrinsic",
            1usize,
        ),
        (
            "buf_read_u64_le",
            "aic_buffer_read_u64_le_intrinsic",
            1usize,
        ),
        ("buf_read_bytes", "aic_buffer_read_bytes_intrinsic", 2usize),
        (
            "buf_read_cstring",
            "aic_buffer_read_cstring_intrinsic",
            1usize,
        ),
        (
            "buf_read_length_prefixed",
            "aic_buffer_read_length_prefixed_intrinsic",
            1usize,
        ),
        ("buf_write_u8", "aic_buffer_write_u8_intrinsic", 2usize),
        (
            "buf_write_i16_be",
            "aic_buffer_write_i16_be_intrinsic",
            2usize,
        ),
        (
            "buf_write_u16_be",
            "aic_buffer_write_u16_be_intrinsic",
            2usize,
        ),
        (
            "buf_write_i32_be",
            "aic_buffer_write_i32_be_intrinsic",
            2usize,
        ),
        (
            "buf_write_u32_be",
            "aic_buffer_write_u32_be_intrinsic",
            2usize,
        ),
        (
            "buf_write_i64_be",
            "aic_buffer_write_i64_be_intrinsic",
            2usize,
        ),
        (
            "buf_write_u64_be",
            "aic_buffer_write_u64_be_intrinsic",
            2usize,
        ),
        (
            "buf_write_i16_le",
            "aic_buffer_write_i16_le_intrinsic",
            2usize,
        ),
        (
            "buf_write_u16_le",
            "aic_buffer_write_u16_le_intrinsic",
            2usize,
        ),
        (
            "buf_write_i32_le",
            "aic_buffer_write_i32_le_intrinsic",
            2usize,
        ),
        (
            "buf_write_u32_le",
            "aic_buffer_write_u32_le_intrinsic",
            2usize,
        ),
        (
            "buf_write_i64_le",
            "aic_buffer_write_i64_le_intrinsic",
            2usize,
        ),
        (
            "buf_write_u64_le",
            "aic_buffer_write_u64_le_intrinsic",
            2usize,
        ),
        (
            "buf_write_bytes",
            "aic_buffer_write_bytes_intrinsic",
            2usize,
        ),
        (
            "buf_write_cstring",
            "aic_buffer_write_cstring_intrinsic",
            2usize,
        ),
        (
            "buf_write_string_prefixed",
            "aic_buffer_write_string_prefixed_intrinsic",
            2usize,
        ),
        (
            "buf_patch_u16_be",
            "aic_buffer_patch_u16_be_intrinsic",
            3usize,
        ),
        (
            "buf_patch_u32_be",
            "aic_buffer_patch_u32_be_intrinsic",
            3usize,
        ),
        (
            "buf_patch_u64_be",
            "aic_buffer_patch_u64_be_intrinsic",
            3usize,
        ),
        (
            "buf_patch_u16_le",
            "aic_buffer_patch_u16_le_intrinsic",
            3usize,
        ),
        (
            "buf_patch_u32_le",
            "aic_buffer_patch_u32_le_intrinsic",
            3usize,
        ),
        (
            "buf_patch_u64_le",
            "aic_buffer_patch_u64_le_intrinsic",
            3usize,
        ),
    ] {
        assert_delegate_call(&source, "std/buffer.aic", wrapper, intrinsic, arity);
    }

    for (intrinsic, arity) in [
        ("aic_buffer_new_intrinsic", 1usize),
        ("aic_buffer_new_growable_intrinsic", 2usize),
        ("aic_buffer_from_bytes_intrinsic", 1usize),
        ("aic_buffer_to_bytes_intrinsic", 1usize),
        ("aic_buffer_position_intrinsic", 1usize),
        ("aic_buffer_remaining_intrinsic", 1usize),
        ("aic_buffer_seek_intrinsic", 2usize),
        ("aic_buffer_reset_intrinsic", 1usize),
        ("aic_buffer_close_intrinsic", 1usize),
        ("aic_buffer_read_u8_intrinsic", 1usize),
        ("aic_buffer_read_i16_be_intrinsic", 1usize),
        ("aic_buffer_read_u16_be_intrinsic", 1usize),
        ("aic_buffer_read_i32_be_intrinsic", 1usize),
        ("aic_buffer_read_u32_be_intrinsic", 1usize),
        ("aic_buffer_read_i64_be_intrinsic", 1usize),
        ("aic_buffer_read_u64_be_intrinsic", 1usize),
        ("aic_buffer_read_i16_le_intrinsic", 1usize),
        ("aic_buffer_read_u16_le_intrinsic", 1usize),
        ("aic_buffer_read_i32_le_intrinsic", 1usize),
        ("aic_buffer_read_u32_le_intrinsic", 1usize),
        ("aic_buffer_read_i64_le_intrinsic", 1usize),
        ("aic_buffer_read_u64_le_intrinsic", 1usize),
        ("aic_buffer_read_bytes_intrinsic", 2usize),
        ("aic_buffer_read_cstring_intrinsic", 1usize),
        ("aic_buffer_read_length_prefixed_intrinsic", 1usize),
        ("aic_buffer_write_u8_intrinsic", 2usize),
        ("aic_buffer_write_i16_be_intrinsic", 2usize),
        ("aic_buffer_write_u16_be_intrinsic", 2usize),
        ("aic_buffer_write_i32_be_intrinsic", 2usize),
        ("aic_buffer_write_u32_be_intrinsic", 2usize),
        ("aic_buffer_write_i64_be_intrinsic", 2usize),
        ("aic_buffer_write_u64_be_intrinsic", 2usize),
        ("aic_buffer_write_i16_le_intrinsic", 2usize),
        ("aic_buffer_write_u16_le_intrinsic", 2usize),
        ("aic_buffer_write_i32_le_intrinsic", 2usize),
        ("aic_buffer_write_u32_le_intrinsic", 2usize),
        ("aic_buffer_write_i64_le_intrinsic", 2usize),
        ("aic_buffer_write_u64_le_intrinsic", 2usize),
        ("aic_buffer_write_bytes_intrinsic", 2usize),
        ("aic_buffer_write_cstring_intrinsic", 2usize),
        ("aic_buffer_write_string_prefixed_intrinsic", 2usize),
        ("aic_buffer_patch_u16_be_intrinsic", 3usize),
        ("aic_buffer_patch_u32_be_intrinsic", 3usize),
        ("aic_buffer_patch_u64_be_intrinsic", 3usize),
        ("aic_buffer_patch_u16_le_intrinsic", 3usize),
        ("aic_buffer_patch_u32_le_intrinsic", 3usize),
        ("aic_buffer_patch_u64_le_intrinsic", 3usize),
    ] {
        assert_intrinsic_declaration(&source, "std/buffer.aic", intrinsic, arity);
    }

    assert!(
        source
            .contains("fn buf_peek_u8(buf: ByteBuffer, position: Int) -> Result[Int, BufferError]"),
        "std/buffer.aic must expose buf_peek_u8 random-access helper"
    );
    assert!(
        source.contains("fn buf_size(buf: ByteBuffer) -> Int"),
        "std/buffer.aic must expose buf_size helper"
    );
    assert!(
        source.contains("fn buf_slice(buf: ByteBuffer, start: Int, length: Int) -> Result[ByteBuffer, BufferError]"),
        "std/buffer.aic must expose buf_slice helper"
    );
    assert!(
        source.contains("fn buf_read_u16_be(buf: ByteBuffer) -> Result[Int, BufferError]"),
        "std/buffer.aic must expose unsigned read helpers"
    );
    assert!(
        source.contains(
            "fn buf_write_u32_le(buf: ByteBuffer, value: Int) -> Result[(), BufferError]"
        ),
        "std/buffer.aic must expose unsigned write helpers"
    );
    assert!(
        source.contains("fn buf_patch_u32_be(buf: ByteBuffer, offset: Int, value: Int) -> Result[(), BufferError]"),
        "std/buffer.aic must expose patch-at-offset helpers"
    );
    assert!(
        source.contains("let _restore = buf_seek(buf, current);"),
        "std/buffer.aic buf_peek_u8 must restore cursor position after peeking"
    );
    assert!(
        source.contains("let sliced = byte_slice(raw, start, end);"),
        "std/buffer.aic buf_slice must compose through std.bytes byte_slice"
    );
}

#[test]
fn unit_std_fs_bytes_apis_bridge_bytes_at_intrinsic_boundary() {
    let fs_source = fs::read_to_string("std/fs.aic").expect("read std/fs.aic");

    assert!(
        fs_source.contains("fn aic_fs_read_bytes_intrinsic(path: String) -> Result[String, FsError] effects { fs }"),
        "std/fs.aic read_bytes intrinsic must use runtime String payload"
    );
    assert!(
        fs_source.contains("fn aic_fs_write_bytes_intrinsic(path: String, content: String) -> Result[Bool, FsError] effects { fs }"),
        "std/fs.aic write_bytes intrinsic must use runtime String payload"
    );
    assert!(
        fs_source.contains("fn aic_fs_append_bytes_intrinsic(path: String, content: String) -> Result[Bool, FsError] effects { fs }"),
        "std/fs.aic append_bytes intrinsic must use runtime String payload"
    );
    assert!(
        fs_source.contains("Ok(data) => Ok(Bytes { data: data })"),
        "std/fs.aic read_bytes must wrap runtime String into Bytes"
    );
    assert!(
        fs_source.contains("aic_fs_write_bytes_intrinsic(path, content.data)"),
        "std/fs.aic write_bytes must pass Bytes.data into runtime intrinsic"
    );
    assert!(
        fs_source.contains("aic_fs_append_bytes_intrinsic(path, content.data)"),
        "std/fs.aic append_bytes must pass Bytes.data into runtime intrinsic"
    );
    assert!(
        fs_source.contains("let out: Result[String, FsError] = Err(Io());"),
        "std/fs.aic read_bytes intrinsic fallback must avoid fake success"
    );
}

#[test]
fn unit_std_net_bytes_apis_bridge_bytes_at_intrinsic_boundary() {
    let net_source = fs::read_to_string("std/net.aic").expect("read std/net.aic");

    assert!(
        net_source.contains("fn aic_net_tcp_send_intrinsic(handle: Int, payload: String) -> Result[Int, NetError] effects { net }"),
        "std/net.aic tcp_send intrinsic must use runtime String payload"
    );
    assert!(
        net_source.contains("fn aic_net_tcp_recv_intrinsic(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[String, NetError] effects { net }"),
        "std/net.aic tcp_recv intrinsic must use runtime String payload"
    );
    assert!(
        net_source.contains("fn aic_net_udp_send_to_intrinsic(handle: Int, addr: String, payload: String) -> Result[Int, NetError] effects { net }"),
        "std/net.aic udp_send_to intrinsic must use runtime String payload"
    );
    assert!(
        net_source.contains("fn aic_net_async_send_submit_intrinsic(handle: Int, payload: String) -> Result[AsyncIntOp, NetError] effects { net, concurrency }"),
        "std/net.aic async send intrinsic must use runtime String payload"
    );
    assert!(
        net_source.contains("fn aic_net_async_wait_string_intrinsic(op: AsyncStringOp, timeout_ms: Int) -> Result[String, NetError] effects { net, concurrency }"),
        "std/net.aic async wait intrinsic must use runtime String payload"
    );

    assert!(
        net_source.contains("aic_net_tcp_send_intrinsic(handle, payload.data)"),
        "std/net.aic tcp_send must pass Bytes.data"
    );
    assert!(
        net_source.contains("aic_net_udp_send_to_intrinsic(handle, addr, payload.data)"),
        "std/net.aic udp_send_to must pass Bytes.data"
    );
    assert!(
        net_source.contains("aic_net_async_send_submit_intrinsic(handle, payload.data)"),
        "std/net.aic async_tcp_send_submit must pass Bytes.data"
    );
    assert!(
        net_source.contains("let raw = aic_net_tcp_recv_intrinsic(handle, max_bytes, timeout_ms);"),
        "std/net.aic tcp_recv must bridge runtime String to Bytes"
    );
    assert!(
        net_source.contains("let raw = aic_net_async_wait_string_intrinsic(op, timeout_ms);"),
        "std/net.aic async_wait_string must bridge runtime String to Bytes"
    );
    assert!(
        !net_source.contains("fn aic_net_tcp_send_intrinsic(handle: Int, payload: String) -> Result[Int, NetError] effects { net } {"),
        "std/net.aic tcp_send intrinsic must remain declaration-only"
    );
    assert!(
        !net_source.contains("fn aic_net_async_wait_string_intrinsic(op: AsyncStringOp, timeout_ms: Int) -> Result[String, NetError] effects { net, concurrency } {"),
        "std/net.aic async wait intrinsic must remain declaration-only"
    );
}
