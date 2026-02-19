use std::fs;
use std::path::Path;

use aicore::contracts::verify_static;
use aicore::effects::check_effect_declarations;
use aicore::formatter::format_program;
use aicore::ir_builder::build;
use aicore::parser::parse;
use aicore::project::init_project;
use aicore::resolver::resolve;
use aicore::typecheck::check;
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
