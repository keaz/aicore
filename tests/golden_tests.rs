use std::fs;
use std::path::Path;

use aicore::formatter::format_program;
use aicore::ir_builder::build;
use aicore::parser::parse;

fn run_golden_case(file_name: &str) {
    let path = Path::new("tests/golden").join(file_name);
    let source = fs::read_to_string(&path).expect("read golden file");

    let (program, diags) = parse(&source, &path.to_string_lossy());
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
    let ir1 = build(&program.expect("program"));

    let printed = format_program(&ir1);
    assert_eq!(printed, source, "formatter mismatch for {}", file_name);

    let (program2, diags2) = parse(&printed, &path.to_string_lossy());
    assert!(
        diags2.is_empty(),
        "roundtrip parse diagnostics: {diags2:#?}"
    );
    let ir2 = build(&program2.expect("program"));

    let j1 = serde_json::to_value(&ir1).expect("json ir1");
    let j2 = serde_json::to_value(&ir2).expect("json ir2");
    assert_eq!(j1, j2, "IR changed on roundtrip for {}", file_name);
}

#[test]
fn golden_case01_simple_fn() {
    run_golden_case("case01_simple_fn.aic");
}

#[test]
fn golden_case02_if_expr() {
    run_golden_case("case02_if_expr.aic");
}

#[test]
fn golden_case03_option_match() {
    run_golden_case("case03_option_match.aic");
}

#[test]
fn golden_case04_effects() {
    run_golden_case("case04_effects.aic");
}

#[test]
fn golden_case05_contracts() {
    run_golden_case("case05_contracts.aic");
}

#[test]
fn golden_case06_struct_invariant() {
    run_golden_case("case06_struct_invariant.aic");
}

#[test]
fn golden_case07_enum() {
    run_golden_case("case07_enum.aic");
}

#[test]
fn golden_case08_generics() {
    run_golden_case("case08_generics.aic");
}

#[test]
fn golden_case09_module_import() {
    run_golden_case("case09_module_import.aic");
}

#[test]
fn golden_case10_match_bool() {
    run_golden_case("case10_match_bool.aic");
}

#[test]
fn golden_case11_result_match() {
    run_golden_case("case11_result_match.aic");
}

#[test]
fn golden_case12_async_await() {
    run_golden_case("case12_async_await.aic");
}

#[test]
fn golden_case13_trait_impl() {
    run_golden_case("case13_trait_impl.aic");
}

#[test]
fn golden_case14_result_propagation() {
    run_golden_case("case14_result_propagation.aic");
}
