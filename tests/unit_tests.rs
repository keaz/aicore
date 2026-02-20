use std::fs;
use std::path::Path;
use std::{collections::BTreeSet, path::PathBuf};

use aicore::contracts::verify_static;
use aicore::diagnostics::Severity;
use aicore::effects::check_effect_declarations;
use aicore::formatter::format_program;
use aicore::ir_builder::build;
use aicore::parser::parse;
use aicore::project::init_project;
use aicore::resolver::resolve;
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
fn unit_parse_struct_literal_expression() {
    let src = "struct S { x: Int } fn f() -> Int { let s = S { x: 1 }; s.x }";
    let (program, diags) = parse(src, "unit.aic");
    assert!(diags.is_empty(), "diags={diags:#?}");
    assert!(program.is_some());
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
fn main() -> Int effects { time, io, fs } {
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
    let main = dir.path().join("src/main.aic");
    let source = fs::read_to_string(&main).expect("read main");
    let ir = lower(&source);
    let formatted = format_program(&ir);
    assert_eq!(source, formatted, "init project source must be canonical");
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

fn main() -> Int effects { io } {
    print_int(add(1, 2));
    0
}
"#,
    )
    .expect("write main");

    fs::write(
        root.join("src/math.aic"),
        r#"module app.math;

fn add(x: Int, y: Int) -> Int {
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
import std.string;
import std.vec;
import std.option;
import std.result;

fn main() -> Int effects { io, fs, net, time, rand } {
    let _exists = exists("foo.txt");
    let _handle = tcp_connect("localhost:80");
    let _ts = now_ms();
    let _r = random_int();
    let _n = len("abc");
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
fn unit_deprecated_std_api_emits_warning() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");

    fs::write(
        root.join("src/main.aic"),
        r#"module app.main;
import std.time;

fn main() -> Int effects { time } {
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

fn add(x: Int, y: Int) -> Int {
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
    0
}
"#,
    )
    .expect("write main");

    fs::write(
        root.join("src/math.aic"),
        r#"module app.math;

fn add(x: Int, y: Int) -> Int {
    x + y
}
"#,
    )
    .expect("write math");

    fs::write(
        root.join("src/more.aic"),
        r#"module app.more;

fn add(x: Int, y: Int) -> Int {
    x - y
}
"#,
    )
    .expect("write more");

    let out = run_frontend(&root.join("src/main.aic")).expect("frontend");
    assert!(out.diagnostics.iter().any(|d| d.code == "E2104"));
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
