use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn parity_script() -> PathBuf {
    repo_root().join("scripts/selfhost/parity.py")
}

fn stage_matrix_script() -> PathBuf {
    repo_root().join("scripts/selfhost/stage_matrix.py")
}

fn run_parity(args: &[String]) -> std::process::Output {
    Command::new("python3")
        .arg(parity_script())
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("run parity script")
}

fn run_stage_matrix(args: &[String]) -> std::process::Output {
    Command::new("python3")
        .arg(stage_matrix_script())
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("run stage matrix script")
}

fn write_fake_compiler(path: &Path, variant: &str) {
    fs::write(
        path,
        format!(
            r#"#!/usr/bin/env python3
import json
import os
import sys

variant = {variant:?}
args = sys.argv[1:]
action = args[0]
source = args[1] if len(args) > 1 else ""
name = os.path.basename(source)

if action == "build":
    out = args[args.index("-o") + 1]
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w", encoding="utf-8") as handle:
        handle.write("artifact:" + variant + ":" + name)

payload = {{"action": action, "source": name, "variant": variant}}
if action == "ir":
    payload["emit"] = args[-1]
print(json.dumps(payload, sort_keys=True))

if "fail" in name:
    sys.exit(1)
sys.exit(0)
"#
        ),
    )
    .expect("write fake compiler");
}

fn write_fake_stage_compiler(path: &Path) {
    fs::write(
        path,
        r#"#!/usr/bin/env python3
import json
import os
import sys

args = sys.argv[1:]
action = args[0]
source = args[1] if len(args) > 1 else ""
name = os.path.basename(source)

if "unsupported_workspace" in source:
    print("error[E5202]: self-host driver could not read source input", file=sys.stderr)
    sys.exit(1)

if "fail" in name:
    print("error[E1258]: type 'Bool' does not satisfy trait bound 'Order'", file=sys.stderr)
    sys.exit(1)

if action == "build":
    out = args[args.index("-o") + 1]
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w", encoding="utf-8") as handle:
        handle.write("stage-matrix-artifact:" + name)
    print("built " + out)
elif action == "ir":
    print(json.dumps({"format": "aicore-selfhost-ir-v1", "source": name}, sort_keys=True))
elif action == "run":
    print("ran " + name)
else:
    print("check: ok")
sys.exit(0)
"#,
    )
    .expect("write fake stage compiler");
}

fn write_manifest(path: &Path) {
    fs::write(
        path,
        r#"{
  "schema_version": 1,
  "name": "test-selfhost-parity",
  "cases": [
    {
      "name": "pass_case",
      "path": "pass.aic",
      "expected": "pass",
      "actions": ["check", "ir-json", "build"]
    },
    {
      "name": "fail_case",
      "path": "fail.aic",
      "expected": "fail",
      "actions": ["check"]
    }
  ]
}
"#,
    )
    .expect("write manifest");
}

fn write_ir_json_compiler(path: &Path, payload: &str) {
    fs::write(
        path,
        format!(
            r#"#!/usr/bin/env python3
import sys

payload = {payload:?}
action = sys.argv[1]
if action == "ir":
    print(payload)
    sys.exit(0)
print("ok")
sys.exit(0)
"#
        ),
    )
    .expect("write ir json compiler");
}

#[test]
fn selfhost_parity_manifest_lists_cases() {
    let output = run_parity(&[
        "--manifest".into(),
        "tests/selfhost/parity_manifest.json".into(),
        "--list".into(),
    ]);

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("core_language_tour pass check,ir-json"));
    assert!(stdout.contains("effects_reject fail check"));
    assert!(stdout.contains("source_diagnostics_check pass check,run"));

    let candidate_output = run_parity(&[
        "--manifest".into(),
        "tests/selfhost/rust_vs_selfhost_manifest.json".into(),
        "--list".into(),
    ]);
    assert!(
        candidate_output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&candidate_output.stdout),
        String::from_utf8_lossy(&candidate_output.stderr)
    );
    let candidate_stdout = String::from_utf8_lossy(&candidate_output.stdout);
    assert!(candidate_stdout.contains("core_async_ping pass check,ir-json"));
    assert!(candidate_stdout.contains("core_loop_control pass check,ir-json"));
    assert!(candidate_stdout.contains("core_tuple_field_access pass check,ir-json"));
    assert!(candidate_stdout.contains("core_option_result_flow pass check,ir-json"));
    assert!(
        candidate_stdout.contains("selfhost_backend_loop_break_tail pass check,ir-json,build,run")
    );
    assert!(candidate_stdout.contains("type_arithmetic_mismatch fail check"));
    assert!(candidate_stdout.contains("trait_bound_invalid fail check"));
    assert!(candidate_stdout.contains("resource_use_after_close fail check"));
}

#[test]
fn selfhost_stage_matrix_manifest_lists_cases() {
    let output = run_stage_matrix(&[
        "--manifest".into(),
        "tests/selfhost/stage_matrix_manifest.json".into(),
        "--list".into(),
    ]);

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("core_loop_control_single_file single-file pass check,ir-json"));
    assert!(stdout
        .contains("backend_loop_break_tail_executable single-file pass check,ir-json,build,run"));
    assert!(stdout.contains("selfhost_driver_package package pass check,ir-json,build,run"));
    assert!(stdout.contains("compiler_source_package_member package-member pass check,ir-json"));
    assert!(stdout.contains("trait_bound_negative_diagnostic single-file fail check"));
    assert!(stdout.contains(
        "workspace_root_currently_unsupported workspace unsupported check non-readiness"
    ));
}

#[test]
fn selfhost_conformance_coverage_maps_manifest_cases() {
    let root = repo_root();
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("tests/selfhost/rust_vs_selfhost_manifest.json"))
            .expect("read rust-vs-selfhost manifest"),
    )
    .expect("manifest json");
    let coverage: Value = serde_json::from_str(
        &fs::read_to_string(root.join("tests/selfhost/conformance_coverage.json"))
            .expect("read conformance coverage"),
    )
    .expect("coverage json");

    assert_eq!(coverage["schema_version"], 1);
    assert_eq!(
        coverage["manifest"],
        "tests/selfhost/rust_vs_selfhost_manifest.json"
    );

    let cases = manifest["cases"].as_array().expect("manifest cases");
    assert!(
        cases.len() >= 30,
        "expanded conformance manifest should contain at least 30 cases"
    );
    let mut manifest_names = BTreeSet::new();
    let mut pass_count = 0;
    let mut fail_count = 0;
    for case in cases {
        let name = case["name"].as_str().expect("case name").to_string();
        assert!(manifest_names.insert(name), "duplicate manifest case name");
        match case["expected"].as_str().expect("expected") {
            "pass" => pass_count += 1,
            "fail" => fail_count += 1,
            other => panic!("unexpected expected value {other}"),
        }
    }
    assert!(
        pass_count >= 15,
        "expected broad positive conformance coverage"
    );
    assert!(
        fail_count >= 15,
        "expected broad negative conformance coverage"
    );

    let required_areas: BTreeSet<&str> = [
        "parser_syntax",
        "resolver_visibility",
        "semantics_generics_traits",
        "typecheck_core",
        "effects_capabilities_contracts",
        "borrow_resource",
        "ir_lowering",
        "backend_build_run",
        "deterministic_output",
    ]
    .into_iter()
    .collect();
    let mut seen_areas = BTreeSet::new();
    let mut covered_cases = BTreeSet::new();
    for area in coverage["areas"].as_array().expect("coverage areas") {
        let id = area["id"].as_str().expect("area id");
        assert!(seen_areas.insert(id.to_string()), "duplicate area id");
        let area_cases = area["cases"].as_array().expect("area cases");
        assert!(!area_cases.is_empty(), "coverage area {id} has no cases");
        for value in area_cases {
            let case_name = value.as_str().expect("coverage case").to_string();
            assert!(
                manifest_names.contains(&case_name),
                "coverage references unknown case {case_name}"
            );
            covered_cases.insert(case_name);
        }
    }
    for required in required_areas {
        assert!(
            seen_areas.contains(required),
            "missing required conformance coverage area {required}"
        );
    }
    for case_name in manifest_names {
        assert!(
            covered_cases.contains(&case_name),
            "manifest case {case_name} is missing from coverage map"
        );
    }
}

#[test]
fn selfhost_compiler_support_packages_are_real_sources() {
    let root = repo_root();
    for path in [
        "compiler/aic/aic.workspace.toml",
        "compiler/aic/libs/source/aic.toml",
        "compiler/aic/libs/source/src/main.aic",
        "compiler/aic/libs/diagnostics/aic.toml",
        "compiler/aic/libs/diagnostics/src/main.aic",
        "compiler/aic/libs/syntax/aic.toml",
        "compiler/aic/libs/syntax/src/main.aic",
        "compiler/aic/libs/lexer/aic.toml",
        "compiler/aic/libs/lexer/src/main.aic",
        "compiler/aic/libs/parser/aic.toml",
        "compiler/aic/libs/parser/src/main.aic",
        "compiler/aic/libs/ast/aic.toml",
        "compiler/aic/libs/ast/src/main.aic",
        "compiler/aic/libs/ir/aic.toml",
        "compiler/aic/libs/ir/src/main.aic",
        "compiler/aic/libs/frontend/aic.toml",
        "compiler/aic/libs/frontend/src/main.aic",
        "compiler/aic/libs/semantics/aic.toml",
        "compiler/aic/libs/semantics/src/main.aic",
        "compiler/aic/libs/typecheck/aic.toml",
        "compiler/aic/libs/typecheck/src/main.aic",
        "compiler/aic/libs/backend_llvm/aic.toml",
        "compiler/aic/libs/backend_llvm/src/main.aic",
        "compiler/aic/libs/driver/aic.toml",
        "compiler/aic/libs/driver/src/main.aic",
        "compiler/aic/tools/aic_selfhost/aic.toml",
        "compiler/aic/tools/aic_selfhost/src/main.aic",
        "compiler/aic/tools/source_diagnostics_check/aic.toml",
        "compiler/aic/tools/source_diagnostics_check/src/main.aic",
        "tests/selfhost/rust_vs_selfhost_manifest.json",
        "tests/selfhost/stage_matrix_manifest.json",
        "scripts/selfhost/stage_matrix.py",
        "docs/selfhost/stage-matrix.md",
        "tests/selfhost/cases/borrow_invalid.aic",
        "tests/selfhost/cases/resource_invalid.aic",
    ] {
        assert!(root.join(path).is_file(), "missing {path}");
    }

    let source = fs::read_to_string(root.join("compiler/aic/libs/source/src/main.aic"))
        .expect("read source lib");
    assert!(source.contains("pub fn merge"));
    assert!(source.contains("pub fn contains_span"));

    let diagnostics = fs::read_to_string(root.join("compiler/aic/libs/diagnostics/src/main.aic"))
        .expect("read diagnostics lib");
    assert!(diagnostics.contains("pub fn error"));
    assert!(diagnostics.contains("pub fn machine_fix"));

    let syntax = fs::read_to_string(root.join("compiler/aic/libs/syntax/src/main.aic"))
        .expect("read syntax lib");
    assert!(syntax.contains("pub enum TokenKind"));
    assert!(syntax.contains("pub fn same_kind"));
    assert!(syntax.contains("pub fn is_identifier_start"));
    assert!(syntax.contains("pub fn is_keyword_lexeme"));
    assert!(syntax.contains("lexeme == \"priv\""));

    let lexer = fs::read_to_string(root.join("compiler/aic/libs/lexer/src/main.aic"))
        .expect("read lexer lib");
    assert!(lexer.contains("pub fn lex_all"));
    assert!(lexer.contains("pub fn scan_token_at"));
    assert!(lexer.contains("pub fn scan_significant_token_at"));
    assert!(lexer.contains("fn scan_quoted_literal"));

    let parser = fs::read_to_string(root.join("compiler/aic/libs/parser/src/main.aic"))
        .expect("read parser lib");
    assert!(parser.contains("pub struct ParserCursor"));
    assert!(parser.contains("pub struct ParseModulePath"));
    assert!(parser.contains("pub fn parser_cursor_from_source"));
    assert!(parser.contains("pub fn expect_identifier"));
    assert!(parser.contains("pub fn parse_module_path"));
    assert!(parser.contains("pub fn parse_module_declaration"));
    assert!(parser.contains("pub fn parse_import_declaration"));
    assert!(parser.contains("pub fn parse_visibility"));
    assert!(parser.contains("pub fn parse_item_kind"));
    assert!(parser.contains("pub fn parse_item_header"));
    assert!(parser.contains("pub fn parse_type_ref"));
    assert!(parser.contains("pub fn parse_optional_where_clause"));
    assert!(parser.contains("pub fn parse_param_list"));
    assert!(parser.contains("pub fn parse_function_signature"));
    assert!(parser.contains("pub fn parse_optional_generic_params"));
    assert!(parser.contains("pub fn parse_struct_field"));
    assert!(parser.contains("pub fn parse_struct_field_list"));
    assert!(parser.contains("pub fn parse_struct_declaration"));
    assert!(parser.contains("pub fn parse_enum_variant"));
    assert!(parser.contains("pub fn parse_enum_variant_list"));
    assert!(parser.contains("pub fn parse_enum_declaration"));
    assert!(parser.contains("pub fn parse_type_alias_declaration"));
    assert!(parser.contains("pub fn parse_const_declaration"));
    assert!(parser.contains("pub fn parse_raw_text_until_semicolon"));
    assert!(parser.contains("pub fn parse_expr"));
    assert!(parser.contains("pub fn parse_expression"));
    assert!(parser.contains("pub fn parse_pattern"));
    assert!(parser.contains("pub fn parse_block"));
    assert!(parser.contains("pub fn parse_function_declaration"));
    assert!(parser.contains("pub fn parse_trait_method_signature"));
    assert!(parser.contains("pub fn parse_trait_method_list"));
    assert!(parser.contains("pub fn parse_trait_declaration"));
    assert!(parser.contains("pub fn parse_impl_method"));
    assert!(parser.contains("pub fn parse_impl_method_list"));
    assert!(parser.contains("pub fn parse_impl_declaration"));
    assert!(parser.contains("pub struct ParseAttribute"));
    assert!(parser.contains("pub struct ParseProgram"));
    assert!(parser.contains("pub fn parse_attribute"));
    assert!(parser.contains("pub fn parse_attribute_list"));
    assert!(parser.contains("pub fn parse_program"));
    assert!(parser.contains("pub fn parse_source_program"));

    let ast =
        fs::read_to_string(root.join("compiler/aic/libs/ast/src/main.aic")).expect("read ast lib");
    assert!(ast.contains("pub enum AstItemKind"));
    assert!(ast.contains("pub enum AstTypeKind"));
    assert!(ast.contains("pub enum AstExprKind"));
    assert!(ast.contains("pub enum AstPatternKind"));
    assert!(ast.contains("pub enum AstStatementKind"));
    assert!(ast.contains("pub enum AstAttributeValueKind"));
    assert!(ast.contains("pub struct AstType"));
    assert!(ast.contains("pub struct AstTypeNode"));
    assert!(ast.contains("pub struct AstExpr"));
    assert!(ast.contains("pub struct AstPattern"));
    assert!(ast.contains("pub struct AstPatternNode"));
    assert!(ast.contains("pub struct AstMatchArmNode"));
    assert!(ast.contains("pub struct AstStatement"));
    assert!(ast.contains("pub struct AstBlock"));
    assert!(ast.contains("pub struct AstGenericParam"));
    assert!(ast.contains("pub struct AstGenericParamList"));
    assert!(ast.contains("pub struct AstModuleDecl"));
    assert!(ast.contains("pub struct AstImportDecl"));
    assert!(ast.contains("pub struct AstParam"));
    assert!(ast.contains("pub struct AstField"));
    assert!(ast.contains("pub struct AstStructDecl"));
    assert!(ast.contains("pub struct AstEnumVariant"));
    assert!(ast.contains("pub struct AstEnumDecl"));
    assert!(ast.contains("pub struct AstTypeAliasDecl"));
    assert!(ast.contains("pub struct AstConstDecl"));
    assert!(ast.contains("pub struct AstFunctionDecl"));
    assert!(ast.contains("pub struct AstTraitDecl"));
    assert!(ast.contains("pub struct AstImplMethod"));
    assert!(ast.contains("pub requires_expr: Option[AstExpr]"));
    assert!(ast.contains("pub ensures_expr: Option[AstExpr]"));
    assert!(ast.contains("pub struct AstImplDecl"));
    assert!(ast.contains("pub struct AstAttribute"));
    assert!(ast.contains("pub struct AstAttributeArg"));
    assert!(ast.contains("pub struct AstProgramItem"));
    assert!(ast.contains("pub struct AstProgram"));
    assert!(ast.contains("pub struct AstSourceMapEntry"));
    assert!(ast.contains("pub fn ast_name_from_token"));
    assert!(ast.contains("pub fn module_decl"));
    assert!(ast.contains("pub fn import_decl"));
    assert!(ast.contains("pub fn ast_attribute"));
    assert!(ast.contains("pub fn ast_attribute_arg"));
    assert!(ast.contains("pub fn ast_program_item"));
    assert!(ast.contains("pub fn ast_program"));
    assert!(ast.contains("pub fn ast_param_with_attrs"));
    assert!(ast.contains("pub fn function_signature_with_attrs"));
    assert!(ast.contains("pub fn ast_field_with_attrs"));
    assert!(ast.contains("pub fn enum_variant_with_attrs"));
    assert!(ast.contains("pub fn ast_expr"));
    assert!(ast.contains("pub fn expr_with_child_roles"));
    assert!(ast.contains("pub fn ast_pattern"));
    assert!(ast.contains("pub fn ast_statement"));
    assert!(ast.contains("pub fn ast_block"));
    assert!(ast.contains("pub fn ast_match_arm"));
    assert!(ast.contains("pub fn function_decl"));
    assert!(ast.contains("pub fn ast_param"));
    assert!(ast.contains("pub fn ast_field"));
    assert!(ast.contains("pub fn struct_decl"));
    assert!(ast.contains("pub fn enum_variant"));
    assert!(ast.contains("pub fn enum_decl"));
    assert!(ast.contains("pub fn type_alias_decl"));
    assert!(ast.contains("pub fn const_decl"));
    assert!(ast.contains("pub fn trait_decl"));
    assert!(ast.contains("pub fn impl_method"));
    assert!(ast.contains("pub fn impl_decl"));
    assert!(ast.contains("pub generic_params: AstGenericParamList"));
    assert!(ast.contains("pub fn named_type"));
    assert!(ast.contains("pub fn dyn_trait_type"));
    assert!(ast.contains("pub fn generic_params_text"));
    assert!(ast.contains("pub fn param_count"));
    assert!(ast.contains("pub fn type_text"));
    assert!(ast.contains("pub fn expr_text"));
    assert!(ast.contains("pub fn expr_match_arm_count"));
    assert!(ast.contains("pub fn pattern_text"));
    assert!(ast.contains("pub fn statement_count"));
    assert!(ast.contains("pub fn block_text"));
    assert!(ast.contains("pub fn block_has_required_tail"));
    assert!(ast.contains("pub fn function_signature_text"));
    assert!(ast.contains("pub fn impl_method_text"));
    assert!(ast.contains("pub fn field_count"));
    assert!(ast.contains("pub fn variant_count"));
    assert!(ast.contains("pub fn trait_method_count"));
    assert!(ast.contains("pub fn impl_method_count"));
    assert!(ast.contains("pub fn attribute_arg_count"));
    assert!(ast.contains("pub fn program_import_count"));
    assert!(ast.contains("pub fn program_item_count"));
    assert!(ast.contains("pub fn program_source_map_count"));
    assert!(ast.contains("pub fn program_has_module"));
    assert!(ast.contains("pub fn program_item_attr_count"));
    assert!(ast.contains("pub fn program_item_name"));
    assert!(ast.contains("pub fn program_item_visibility"));
    assert!(ast.contains("pub fn param_attr_count"));
    assert!(ast.contains("pub fn function_signature_attr_count"));
    assert!(ast.contains("pub fn field_attr_count"));
    assert!(ast.contains("pub fn enum_variant_attr_count"));
    assert!(ast.contains("pub fn impl_method_attr_count"));
    assert!(ast.contains("pub fn literal_from_token"));

    let ir =
        fs::read_to_string(root.join("compiler/aic/libs/ir/src/main.aic")).expect("read ir lib");
    assert!(ir.contains("pub struct IrSymbolId"));
    assert!(ir.contains("pub fn next_symbol_id"));
    assert!(ir.contains("pub fn is_concrete_type"));
    assert!(ir.contains("pub struct IrProgram"));
    assert!(ir.contains("pub enum IrItem"));
    assert!(ir.contains("pub struct IrFunctionDef"));
    assert!(ir.contains("pub struct IrBlock"));
    assert!(ir.contains("pub struct IrStatement"));
    assert!(ir.contains("pub struct IrExpr"));
    assert!(ir.contains("pub struct IrPattern"));
    assert!(ir.contains("pub struct IrGenericInstantiation"));
    assert!(ir.contains("pub fn lower_checked_program"));
    assert!(ir.contains("pub fn ir_program_digest"));
    assert!(ir.contains("E5001"));
    assert!(ir.contains("E5002"));
    assert!(ir.contains("E5003"));
    assert!(ir.contains("E5004"));

    let frontend = fs::read_to_string(root.join("compiler/aic/libs/frontend/src/main.aic"))
        .expect("read frontend lib");
    assert!(frontend.contains("pub enum ResolverSymbolKind"));
    assert!(frontend.contains("pub enum ResolverNamespace"));
    assert!(frontend.contains("pub enum ResolverReferenceKind"));
    assert!(frontend.contains("pub struct ResolveUnit"));
    assert!(frontend.contains("pub struct ResolvedSymbol"));
    assert!(frontend.contains("pub struct ResolvedImport"));
    assert!(frontend.contains("pub struct ResolvedReference"));
    assert!(frontend.contains("pub struct ResolverOutput"));
    assert!(frontend.contains("pub fn resolve_unit"));
    assert!(frontend.contains("pub fn resolve_program"));
    assert!(frontend.contains("pub fn resolve_units"));
    assert!(frontend.contains("pub fn resolver_symbol_count"));
    assert!(frontend.contains("pub fn resolver_import_count"));
    assert!(frontend.contains("pub fn resolver_reference_count"));
    assert!(frontend.contains("pub fn resolver_diagnostic_count"));
    assert!(frontend.contains("pub fn resolver_has_diagnostic_code"));
    assert!(frontend.contains("pub fn resolver_has_symbol"));
    assert!(frontend.contains("pub fn resolver_has_member"));
    assert!(frontend.contains("pub fn resolver_has_reference"));
    assert!(frontend.contains("fn should_resolve_variant_pattern"));

    let semantics = fs::read_to_string(root.join("compiler/aic/libs/semantics/src/main.aic"))
        .expect("read semantics lib");
    assert!(semantics.contains("pub enum SemanticOwnerKind"));
    assert!(semantics.contains("pub enum SemanticImplKind"));
    assert!(semantics.contains("pub struct SemanticGenericParam"));
    assert!(semantics.contains("pub struct SemanticTraitBound"));
    assert!(semantics.contains("pub struct SemanticTraitIndex"));
    assert!(semantics.contains("pub struct SemanticImplIndex"));
    assert!(semantics.contains("pub struct SemanticTraitMethod"));
    assert!(semantics.contains("pub struct SemanticOutput"));
    assert!(semantics.contains("pub fn analyze_program"));
    assert!(semantics.contains("pub fn analyze_units"));
    assert!(semantics.contains("pub fn analyze_resolved_units"));
    assert!(semantics.contains("pub fn semantic_generic_count"));
    assert!(semantics.contains("pub fn semantic_trait_bound_count"));
    assert!(semantics.contains("pub fn semantic_trait_count"));
    assert!(semantics.contains("pub fn semantic_impl_count"));
    assert!(semantics.contains("pub fn semantic_trait_method_count"));
    assert!(semantics.contains("pub fn semantic_diagnostic_count"));
    assert!(semantics.contains("pub fn semantic_has_diagnostic_code"));
    assert!(semantics.contains("pub fn semantic_has_generic"));
    assert!(semantics.contains("pub fn semantic_has_trait_bound"));
    assert!(semantics.contains("pub fn semantic_has_trait"));
    assert!(semantics.contains("pub fn semantic_has_impl"));
    assert!(semantics.contains("pub fn semantic_has_conflicting_impl"));
    assert!(semantics.contains("pub fn semantic_has_trait_method"));

    let typecheck = fs::read_to_string(root.join("compiler/aic/libs/typecheck/src/main.aic"))
        .expect("read typecheck lib");
    assert!(typecheck.contains("pub enum TypecheckTypeKind"));
    assert!(typecheck.contains("pub enum TypecheckValueKind"));
    assert!(typecheck.contains("pub struct TypecheckType"));
    assert!(typecheck.contains("pub struct TypecheckBinding"));
    assert!(typecheck.contains("pub struct TypecheckFunction"));
    assert!(typecheck.contains("pub struct TypecheckInstantiation"));
    assert!(typecheck.contains("pub struct TypecheckOutput"));
    assert!(typecheck.contains("pub fn typecheck_program"));
    assert!(typecheck.contains("pub fn typecheck_units"));
    assert!(typecheck.contains("pub fn typecheck_resolved_units"));
    assert!(typecheck.contains("pub fn typecheck_function_count"));
    assert!(typecheck.contains("pub fn typecheck_binding_count"));
    assert!(typecheck.contains("pub fn typecheck_instantiation_count"));
    assert!(typecheck.contains("pub fn typecheck_diagnostic_count"));
    assert!(typecheck.contains("pub fn typecheck_has_diagnostic_code"));
    assert!(typecheck.contains("pub fn typecheck_has_function"));
    assert!(typecheck.contains("pub fn typecheck_has_binding"));
    assert!(typecheck.contains("pub fn typecheck_has_instantiation"));
    assert!(typecheck.contains("fn direct_expr_at"));
    assert!(typecheck.contains("fn generic_arg_texts"));
    assert!(typecheck.contains("fn tuple_arg_texts"));
    assert!(typecheck.contains("fn infer_tuple_expr"));
    assert!(typecheck.contains("fn tuple_field_type"));
    assert!(typecheck.contains("fn find_consistent_type_alias"));
    assert!(typecheck.contains(
        "fn infer_int_literal(expr: AstExpr, env: TypecheckEnv, units: Vec[ResolveUnit]"
    ));
    assert!(typecheck.contains("fn check_generic_bounds"));
    assert!(typecheck.contains("struct EffectFunctionEntry"));
    assert!(typecheck.contains("fn append_effect_capability_contract_diagnostics"));
    assert!(typecheck.contains("fn append_transitive_effect_diagnostics"));
    assert!(typecheck.contains("fn append_capability_authority_diagnostics"));
    assert!(typecheck.contains("fn append_static_contract_diagnostic"));
    assert!(typecheck.contains("struct EffectPath"));
    assert!(typecheck.contains("E2005"));
    assert!(typecheck.contains("E2009"));
    assert!(typecheck.contains("E4003"));
    assert!(typecheck.contains("E4005"));
    assert!(typecheck.contains("struct OwnershipState"));
    assert!(typecheck.contains("struct ActiveBorrow"));
    assert!(typecheck.contains("struct ResourceProtocolOp"));
    assert!(typecheck.contains("fn append_ownership_resource_diagnostics"));
    assert!(typecheck.contains("fn check_ownership_expr"));
    assert!(typecheck.contains("fn check_resource_protocol_call"));
    assert!(typecheck.contains("loop_break_types: Vec[String]"));
    assert!(typecheck.contains("fn loop_break_type_stack_set_top"));
    assert!(typecheck.contains("E1274"));
    assert!(typecheck.contains("E1263"));
    assert!(typecheck.contains("E1265"));
    assert!(typecheck.contains("E1277"));
    assert!(typecheck.contains("E1278"));
    assert!(typecheck.contains("E2006"));

    let backend = fs::read_to_string(root.join("compiler/aic/libs/backend_llvm/src/main.aic"))
        .expect("read backend llvm lib");
    assert!(backend.contains("module compiler.backend_llvm"));
    assert!(backend.contains("pub enum BackendArtifactKind"));
    assert!(backend.contains("pub enum BackendNativeLinkKind"));
    assert!(backend.contains("pub struct BackendOptions"));
    assert!(backend.contains("pub struct BackendArtifact"));
    assert!(backend.contains("pub fn backend_mangle_symbol"));
    assert!(backend.contains("pub fn backend_artifact_file_name"));
    assert!(backend.contains("pub fn validate_backend_program"));
    assert!(backend.contains("pub fn backend_program_features"));
    assert!(backend.contains("pub fn emit_llvm_text"));
    assert!(backend.contains("pub fn emit_backend_artifact"));
    assert!(backend.contains("pub fn backend_artifact_ok"));
    assert!(backend.contains("pub fn backend_has_diagnostic_code"));
    assert!(backend.contains("\"String\""));
    assert!(backend.contains("%aic.String = type { i8*, i64, i64 }"));
    assert!(backend.contains("fn llvm_escape_string_data"));
    assert!(backend.contains("fn emit_string_literal_global"));
    assert!(backend.contains("fn emit_string_replace_call"));
    assert!(backend.contains("aic_rt_string_replace"));
    assert!(backend.contains("fn llvm_integer_extension_op"));
    assert!(backend.contains("sext"));
    assert!(backend.contains("fn emit_env_arg_or_empty_return"));
    assert!(backend.contains("aic_rt_env_arg_at"));
    assert!(backend.contains("generic-definition-metadata"));
    assert!(backend.contains("native-link-metadata"));
    assert!(backend.contains("E5101"));
    assert!(backend.contains("E5102"));
    assert!(backend.contains("E5103"));
    assert!(backend.contains("E5104"));
    assert!(backend.contains("E5105"));

    let driver = fs::read_to_string(root.join("compiler/aic/libs/driver/src/main.aic"))
        .expect("read selfhost driver lib");
    assert!(driver.contains("module compiler.driver"));
    assert!(driver.contains("pub struct DriverSource"));
    assert!(driver.contains("pub struct DriverCompileResult"));
    assert!(driver.contains("pub struct DriverCommandResult"));
    assert!(driver.contains("pub fn driver_compile_source"));
    assert!(driver.contains("pub fn driver_check_source"));
    assert!(driver.contains("pub fn driver_check_sources"));
    assert!(driver.contains("pub fn driver_ir_json_source"));
    assert!(driver.contains("pub fn driver_ir_json_sources"));
    assert!(driver.contains("pub fn driver_build_source"));
    assert!(driver.contains("pub fn driver_build_sources"));
    assert!(driver.contains("pub fn driver_run_source"));
    assert!(driver.contains("pub fn driver_manifest_main_path"));
    assert!(driver.contains("driver_sources_with_synthetic_imports_for_all"));
    assert!(driver.contains("pub intrinsic fn trim(s: String) -> String"));
    assert!(driver.contains("type ProcExitStatus = Int32;"));
    assert!(!driver.contains("pub type ProcExitStatus = Int32;"));
    assert!(driver.contains("E5200"));
    assert!(driver.contains("E5201"));
    assert!(driver.contains("E5205"));

    let selfhost_tool =
        fs::read_to_string(root.join("compiler/aic/tools/aic_selfhost/src/main.aic"))
            .expect("read aic_selfhost tool");
    assert!(selfhost_tool.contains("module compiler.tools.aic_selfhost"));
    assert!(selfhost_tool.contains("compiler_source_path_for_import"));
    assert!(selfhost_tool.contains("read_source_bundle"));
    assert!(selfhost_tool.contains("driver_check_sources"));
    assert!(selfhost_tool.contains("driver_ir_json_sources"));
    assert!(selfhost_tool.contains("driver_build_sources"));
    assert!(selfhost_tool.contains("materialize_native"));
    assert!(selfhost_tool.contains("runtime_c_command"));
    assert!(selfhost_tool.contains("cat src/codegen/runtime/part01.c"));
    assert!(selfhost_tool.contains("aic_selfhost_runtime_"));
    assert!(selfhost_tool.contains("-x c "));
    assert!(selfhost_tool.contains("src/codegen/runtime/part01.c"));
    assert!(selfhost_tool.contains("src/codegen/runtime/part05.c"));
    assert!(selfhost_tool.contains("-DAIC_RT_TLS_OPENSSL=0"));
    assert!(selfhost_tool.contains("AIC_SELFHOST_OS"));
    assert!(selfhost_tool.contains("AIC_SELFHOST_STACK_FLAG"));
    assert!(selfhost_tool.contains("uname -s"));
    assert!(selfhost_tool.contains("[ \\\"$AIC_SELFHOST_OS\\\" = 'Darwin' ]"));
    assert!(selfhost_tool.contains("AIC_SELFHOST_CODESIGN"));
    assert!(selfhost_tool.contains("AIC_CODESIGN"));
    assert!(selfhost_tool.contains("--force --sign -"));
    assert!(selfhost_tool.contains("-Wl,-z,stack-size=67108864"));
    assert!(selfhost_tool.contains("-pthread -lm"));
    assert!(selfhost_tool.contains("proc.run"));
    assert!(selfhost_tool
        .contains("fn proc_status_to_int(value: ProcExitStatus) -> Int {\n    value\n}"));
    assert!(selfhost_tool.contains("source_path_for_input"));

    let makefile = fs::read_to_string(root.join("Makefile")).expect("read Makefile");
    assert!(makefile.contains("selfhost-parity-candidate"));
    assert!(makefile.contains("selfhost-stage-matrix"));
    assert!(makefile.contains("selfhost-bootstrap"));
    assert!(makefile.contains("selfhost-bootstrap-report"));

    let bootstrap = fs::read_to_string(root.join("scripts/selfhost/bootstrap.py"))
        .expect("read bootstrap script");
    assert!(bootstrap.contains("aicore-selfhost-bootstrap-v1"));
    assert!(bootstrap.contains("stage0"));
    assert!(bootstrap.contains("stage1"));
    assert!(bootstrap.contains("stage2"));
    assert!(bootstrap.contains("stage-matrix"));
    assert!(bootstrap.contains("stage_matrix_report"));
    assert!(bootstrap.contains("SELFHOST_STAGE_MATRIX_REPORT"));
    assert!(bootstrap.contains("allow-incomplete"));
    assert!(bootstrap.contains("host-preflight"));
    assert!(bootstrap.contains("host_report"));
    assert!(bootstrap.contains("host_preflight_command"));
    assert!(bootstrap.contains("command -v cargo"));
    assert!(bootstrap.contains("command -v clang"));
    assert!(bootstrap.contains("command -v strip"));
    assert!(bootstrap.contains("command -v codesign"));
    assert!(bootstrap.contains("Developer Mode is disabled"));
    assert!(bootstrap.contains("ad-hoc signs"));
    assert!(bootstrap.contains("\"--strip-all\""));
    assert!(bootstrap.contains("strip\", \"-S\", \"-x"));
    assert!(bootstrap.contains("strip_command"));
    assert!(bootstrap.contains("default=900"));
    assert!(bootstrap.contains("stripped_matches"));
    assert!(bootstrap.contains("AIC_SELFHOST_STAGE0"));
    assert!(bootstrap.contains("resource_budget_report"));
    assert!(bootstrap.contains("performance"));
    assert!(bootstrap.contains("artifact_size_bytes"));
    assert!(bootstrap.contains("child_peak_rss_bytes"));
    assert!(bootstrap.contains("AIC_SELFHOST_MAX_STEP_MS"));
    assert!(bootstrap.contains("AIC_SELFHOST_MAX_ARTIFACT_BYTES"));

    let stage_matrix = fs::read_to_string(root.join("scripts/selfhost/stage_matrix.py"))
        .expect("read stage matrix");
    assert!(stage_matrix.contains("aicore-selfhost-stage-matrix-v1"));
    assert!(stage_matrix.contains("diagnostic_codes"));
    assert!(stage_matrix.contains("unsupported"));
    assert!(stage_matrix.contains("artifact_sha256"));
    assert!(stage_matrix.contains("stdout_json_sha256"));

    let selfhost_docs =
        fs::read_to_string(root.join("docs/selfhost/README.md")).expect("read selfhost docs");
    assert!(selfhost_docs.contains("make selfhost-bootstrap-report"));
    assert!(selfhost_docs.contains("make selfhost-bootstrap"));
    assert!(selfhost_docs.contains("make selfhost-stage-matrix"));
    assert!(selfhost_docs.contains("docs/selfhost/stage-matrix.md"));
    assert!(selfhost_docs.contains("experimental"));
    assert!(selfhost_docs.contains("supported"));
    assert!(selfhost_docs.contains("default"));
    assert!(selfhost_docs.contains("host-preflight"));
    assert!(selfhost_docs.contains("CI and Release Gates"));
    assert!(selfhost_docs.contains("Self-Host Bootstrap (${{ matrix.os }})"));
    assert!(selfhost_docs.contains("Release Self-Host Bootstrap (${{ matrix.os }})"));
    assert!(selfhost_docs.contains("AIC_SELFHOST_BOOTSTRAP_TIMEOUT=3600"));

    let parser = fs::read_to_string(root.join("compiler/aic/libs/parser/src/main.aic"))
        .expect("read parser lib");
    assert!(parser.contains("trait method signatures cannot declare requires/ensures contracts"));
    assert!(parser.contains("E1089"));
    assert!(parser.contains("impl_method(attributed_signature, requires_expr, ensures_expr"));

    let source_diagnostics_check =
        fs::read_to_string(root.join("compiler/aic/tools/source_diagnostics_check/src/main.aic"))
            .expect("read source diagnostics check tool");
    assert!(source_diagnostics_check.contains("fn valid_effect_contract_positive_cases"));
    assert!(source_diagnostics_check.contains("fn valid_effect_contract_negative_cases"));
    assert!(source_diagnostics_check.contains("fn valid_ownership_resource_positive_cases"));
    assert!(source_diagnostics_check.contains("fn valid_ownership_resource_negative_cases"));
    assert!(source_diagnostics_check.contains("fn valid_ir_lowering_positive_cases"));
    assert!(source_diagnostics_check.contains("fn valid_ir_lowering_negative_cases"));
    assert!(source_diagnostics_check.contains("fn valid_ir_serialization_positive_cases"));
    assert!(source_diagnostics_check.contains("fn valid_ir_serialization_negative_cases"));
    assert!(source_diagnostics_check.contains("fn valid_backend_positive_cases"));
    assert!(source_diagnostics_check.contains("fn backend_negative_status_code"));
    assert!(source_diagnostics_check.contains("fn valid_backend_negative_cases"));
    assert!(source_diagnostics_check.contains("fn valid_backend_frontend"));
    assert!(source_diagnostics_check.contains("fn valid_driver_positive_cases"));
    assert!(source_diagnostics_check.contains("fn valid_driver_negative_cases"));
    assert!(source_diagnostics_check.contains("fn valid_driver_frontend"));
    assert!(source_diagnostics_check.contains("emit_backend_artifact"));
    assert!(source_diagnostics_check.contains("backend_has_diagnostic_code"));
    assert!(source_diagnostics_check.contains("driver_build_source"));
    assert!(source_diagnostics_check.contains("vec.vec_len(state.sources)"));
    assert!(source_diagnostics_check.contains("aic_state_source_count"));
    assert!(source_diagnostics_check.contains("Ok(contents)"));
    assert!(source_diagnostics_check.contains("Err(cause)"));
    assert!(source_diagnostics_check.contains("fn backend_string_join_positive_status_code"));
    assert!(source_diagnostics_check.contains("string.join(parts"));
    assert!(source_diagnostics_check.contains("aic_rt_string_join"));
    assert!(source_diagnostics_check.contains("fn backend_explicit_return_positive_status_code"));
    assert!(source_diagnostics_check.contains("return value;"));
    assert!(source_diagnostics_check.contains("fn early() -> Int { return 7; 99 }"));
    assert!(source_diagnostics_check.contains("fn backend_branch_return_positive_status_code"));
    assert!(source_diagnostics_check.contains("if flag { return 1; 99 } else { return 2; 98 }"));
    assert!(source_diagnostics_check.contains("fn backend_range_for_positive_status_code"));
    assert!(source_diagnostics_check.contains("for value in 0..limit"));
    assert!(source_diagnostics_check.contains("fn backend_loop_control_positive_status_code"));
    assert!(source_diagnostics_check.contains("continue; ();"));
    assert!(source_diagnostics_check.contains("break; ();"));
    assert!(source_diagnostics_check.contains("fn backend_vec_iter_for_positive_status_code"));
    assert!(source_diagnostics_check.contains("for value in values"));
    assert!(source_diagnostics_check.contains("fn backend_loop_value_positive_status_code"));
    assert!(
        source_diagnostics_check.contains("loop { if flag { break 7; () } else { break 5; () } }")
    );
    assert!(source_diagnostics_check.contains("let value = loop { break 7; () }; value"));
    assert!(source_diagnostics_check.contains("loop_break_mismatch"));
    assert!(source_diagnostics_check.contains("backend_positive_cases_status_code"));
    assert!(source_diagnostics_check.contains("unsupported_for"));
    assert!(source_diagnostics_check.contains("backend_has_diagnostic_message"));
    assert!(source_diagnostics_check.contains("range-for and Vec[Int]"));
    assert!(source_diagnostics_check.contains("fn backend_vec_get_option_positive_status_code"));
    assert!(source_diagnostics_check.contains("match vec.get(tokens, index)"));
    assert!(source_diagnostics_check.contains("aic_rt_vec_get"));
    assert!(source_diagnostics_check.contains("aic_rt_stack_ensure_min"));

    let backend = fs::read_to_string(root.join("compiler/aic/libs/backend_llvm/src/main.aic"))
        .expect("read backend llvm lib");
    assert!(!backend.contains("type BackendLocal = String"));
    assert!(backend.contains("struct BackendLocals"));
    assert!(backend.contains("names: Vec[String]"));
    assert!(backend.contains("type_names: Vec[String]"));
    assert!(backend.contains("llvm_types: Vec[String]"));
    assert!(backend.contains("value_names: Vec[String]"));
    assert!(backend.contains("fn backend_local_named_index(locals: BackendLocals"));
    assert!(backend.contains("match vec.get(locals.names, index)"));
    assert!(backend.contains("fn vec_len_expr_can_emit"));
    assert!(backend.contains("emit_vec_len_expr_value"));
    assert!(backend.contains("fn string_join_expr_can_emit"));
    assert!(backend.contains("fn emit_string_join_call"));
    assert!(backend.contains("declare void @aic_rt_string_join"));
    assert!(backend.contains("fn vec_get_expr_can_emit"));
    assert!(backend.contains("fn emit_vec_get_probe"));
    assert!(backend.contains("fn option_vec_get_match_expr_can_emit_return"));
    assert!(backend.contains("declare i64 @aic_rt_vec_get"));
    assert!(backend.contains("fn for_expr_can_emit_with_locals"));
    assert!(backend.contains("fn for_iter_node_can_emit_with_locals"));
    assert!(backend.contains("fn emit_for_iter_statement"));
    assert!(backend.contains("program_uses_iterator_for(program)"));
    assert!(backend.contains("fn emit_for_expr_statement"));
    assert!(backend.contains("fn emit_unit_loop_statement_branch_node"));
    assert!(backend.contains("fn emit_if_node_loop_statement"));
    assert!(backend.contains("fn node_is_unit_loop_control"));
    assert!(backend.contains("fn loop_expr_can_emit_value_with_type"));
    assert!(backend.contains("fn emit_loop_expr_value"));
    assert!(backend.contains("fn emit_loop_expr_statement"));
    assert!(backend.contains("unsupported_backend_expr_message"));
    assert!(backend.contains("unsupported_backend_statement_message"));
    assert!(backend.contains("unsafe blocks"));
    assert!(backend.contains("template literals"));
    assert!(backend.contains("range-for and Vec[Int]"));
    assert!(backend.contains(".break.slot"));
    assert!(backend.contains("loop.body."));
    assert!(backend.contains("for.iter.cond."));
    assert!(backend.contains("icmp slt i64"));
    assert!(backend.contains("br label %"));
    assert!(backend.contains("declare void @aic_rt_stack_ensure_min"));
    assert!(backend.contains("call void @aic_rt_stack_ensure_min(i64 67108864)"));

    let ir =
        fs::read_to_string(root.join("compiler/aic/libs/ir/src/main.aic")).expect("read ir lib");
    assert!(ir.contains("pub struct IrSerializationReport"));
    assert!(ir.contains("pub fn ir_program_to_json"));
    assert!(ir.contains("pub fn ir_lowering_result_to_json"));
    assert!(ir.contains("pub fn ir_program_to_debug_text"));
    assert!(ir.contains("pub fn validate_ir_serialization_contract"));
    assert!(ir.contains("pub fn ir_program_to_parity_artifact_json"));
    assert!(ir.contains("E5010"));
    assert!(ir.contains("E5011"));
    assert!(ir.contains("E5012"));
    assert!(ir.contains("E5013"));

    let ast =
        fs::read_to_string(root.join("compiler/aic/libs/ast/src/main.aic")).expect("read ast lib");
    assert!(ast.contains("pub patterns: Vec[AstPatternNode]"));
    assert!(ast.contains("pub match_arms: Vec[AstMatchArmNode]"));
    assert!(ast.contains("value.patterns, value.match_arms"));
}

#[test]
fn selfhost_bootstrap_ci_and_release_gates_are_wired() {
    let root = repo_root();

    let makefile = fs::read_to_string(root.join("Makefile")).expect("read Makefile");
    for token in [
        "AIC_SELFHOST_BOOTSTRAP_TIMEOUT ?= 900",
        "selfhost-bootstrap:",
        "scripts/selfhost/bootstrap.py --mode supported --timeout \"$(AIC_SELFHOST_BOOTSTRAP_TIMEOUT)\"",
        "release-preflight: ci selfhost-bootstrap repro-check security-audit",
    ] {
        assert!(makefile.contains(token), "Makefile missing token: {token}");
    }

    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci workflow");
    for token in [
        "selfhost-bootstrap:",
        "Self-Host Bootstrap (${{ matrix.os }})",
        "os: [ubuntu-latest, macos-latest]",
        "timeout-minutes: 150",
        "AIC_SELFHOST_BOOTSTRAP_TIMEOUT: \"3600\"",
        "AIC_SELFHOST_MAX_STEP_MS: \"3600000\"",
        "AIC_SELFHOST_MAX_TOTAL_MS: \"7200000\"",
        "AIC_SELFHOST_MAX_ARTIFACT_BYTES: \"536870912\"",
        "AIC_SELFHOST_MAX_PEAK_RSS_BYTES: \"17179869184\"",
        "Host tool preflight",
        "command -v cargo",
        "command -v clang",
        "command -v strip",
        "command -v codesign",
        "codesign: available",
        "Supported self-host bootstrap gate",
        "make selfhost-bootstrap",
        "Upload self-host bootstrap reports",
        "actions/upload-artifact@v4",
        "if: always()",
        "selfhost-bootstrap-${{ matrix.os }}",
        "target/selfhost-bootstrap/report.json",
        "target/selfhost-bootstrap/parity-report.json",
        "target/selfhost-bootstrap/stage-matrix-report.json",
    ] {
        assert!(ci.contains(token), "ci workflow missing token: {token}");
    }

    let release = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect("read release workflow");
    for token in [
        "release-selfhost-bootstrap:",
        "Release Self-Host Bootstrap (${{ matrix.os }})",
        "os: [ubuntu-latest, macos-latest]",
        "timeout-minutes: 150",
        "AIC_SELFHOST_BOOTSTRAP_TIMEOUT: \"3600\"",
        "AIC_SELFHOST_MAX_STEP_MS: \"3600000\"",
        "AIC_SELFHOST_MAX_TOTAL_MS: \"7200000\"",
        "AIC_SELFHOST_MAX_ARTIFACT_BYTES: \"536870912\"",
        "AIC_SELFHOST_MAX_PEAK_RSS_BYTES: \"17179869184\"",
        "Host tool preflight",
        "command -v cargo",
        "command -v clang",
        "command -v strip",
        "command -v codesign",
        "codesign: available",
        "make selfhost-bootstrap",
        "release-selfhost-bootstrap-${{ matrix.os }}",
        "target/selfhost-bootstrap/report.json",
        "target/selfhost-bootstrap/parity-report.json",
        "target/selfhost-bootstrap/stage-matrix-report.json",
        "- release-selfhost-bootstrap",
    ] {
        assert!(
            release.contains(token),
            "release workflow missing token: {token}"
        );
    }
}

#[test]
fn selfhost_bootstrap_host_preflight_command_lists_required_tools() {
    let root = repo_root();
    let script = root.join("scripts/selfhost/bootstrap.py");
    let output = Command::new("python3")
        .arg("-c")
        .arg(format!(
            r#"
import importlib.util
import sys
spec = importlib.util.spec_from_file_location("bootstrap", {script:?})
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
command = module.host_preflight_command()
assert command[0:2] == ["sh", "-c"]
body = command[2]
for token in [
    "command -v cargo",
    "cargo --version",
    "command -v clang",
    "clang --version",
    "command -v strip",
    "command -v codesign",
    "codesign: available",
    "DevToolsSecurity -status",
]:
    assert token in body, token
host = module.host_report()
assert host["platform"]
assert host["system"]
assert host["python_version"]
"#,
            script = script.to_string_lossy()
        ))
        .output()
        .expect("run bootstrap host preflight probe");
    assert!(
        output.status.success(),
        "bootstrap host preflight probe failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn selfhost_bootstrap_uses_platform_artifact_normalization() {
    let root = repo_root();
    let script = root.join("scripts/selfhost/bootstrap.py");
    let output = Command::new("python3")
        .arg("-c")
        .arg(format!(
            r#"
import importlib.util
import sys
spec = importlib.util.spec_from_file_location("bootstrap", {script:?})
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
assert module.strip_command_for_platform("darwin") == ["strip", "-S", "-x"]
assert module.strip_command_for_platform("linux") == ["strip", "--strip-all"]
assert module.strip_command_for_platform("freebsd") == ["strip", "--strip-all"]
"#,
            script = script.to_string_lossy()
        ))
        .output()
        .expect("run bootstrap normalization probe");
    assert!(
        output.status.success(),
        "bootstrap normalization probe failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn selfhost_bootstrap_reports_resource_budget_violations() {
    let root = repo_root();
    let script = root.join("scripts/selfhost/bootstrap.py");
    let output = Command::new("python3")
        .arg("-c")
        .arg(format!(
            r#"
import importlib.util
import sys
spec = importlib.util.spec_from_file_location("bootstrap", {script:?})
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)

def step(name, duration_ms, artifact_size_bytes, child_peak_rss_bytes):
    return module.StepResult(
        name=name,
        command=["true"],
        exit_code=0,
        duration_ms=duration_ms,
        stdout="",
        stderr="",
        timed_out=False,
        artifact=name,
        artifact_exists=True,
        artifact_sha256="sha256:" + name,
        artifact_size_bytes=artifact_size_bytes,
        child_peak_rss_bytes=child_peak_rss_bytes,
    )

steps = [
    step("stage0", 10, 20, 30),
    step("stage1", 200, 300, 400),
    step("stage2", 10, 20, 30),
    step("parity", 10, 20, 30),
    step("stage-matrix", 10, 20, 30),
]
passing = module.resource_budget_report(
    steps,
    module.ResourceBudgets(
        max_step_ms=500,
        max_total_ms=500,
        max_artifact_bytes=500,
        max_peak_rss_bytes=500,
    ),
)
assert passing["ok"] is True
assert passing["observed"]["max_step_duration_ms"] == 200
assert passing["observed"]["max_artifact_size_bytes"] == 300
assert passing["observed"]["max_child_peak_rss_bytes"] == 400

failing = module.resource_budget_report(
    steps,
    module.ResourceBudgets(
        max_step_ms=100,
        max_total_ms=100,
        max_artifact_bytes=100,
        max_peak_rss_bytes=100,
    ),
)
assert failing["ok"] is False
assert len(failing["violations"]) == 4
status, reasons = module.readiness_status("supported", steps, {{"matches": True}}, failing)
assert status == "experimental"
assert any("resource budget violation" in reason for reason in reasons)
"#,
            script = script.to_string_lossy()
        ))
        .output()
        .expect("run bootstrap budget probe");
    assert!(
        output.status.success(),
        "bootstrap budget probe failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn aic_selfhost_driver_tool_handles_supported_and_negative_commands() {
    let root = repo_root();
    let tmp = tempdir().expect("tempdir");
    let bin = tmp.path().join("aic_selfhost");
    let ok_source = tmp.path().join("ok.aic");
    let bad_source = tmp.path().join("bad.aic");
    let package_dir = tmp.path().join("pkg");
    let package_src = package_dir.join("src");
    let artifact = tmp.path().join("ok");

    fs::write(
        &ok_source,
        "module smoke.main; fn add(x: Int, y: Int) -> Int { x + y } fn main() -> Int { add(0, 0) }\n",
    )
    .expect("write ok source");
    fs::write(
        &bad_source,
        "module smoke.main; fn main() -> Int { missing }\n",
    )
    .expect("write bad source");
    fs::create_dir_all(&package_src).expect("package src");
    fs::write(
        package_dir.join("aic.toml"),
        "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write package manifest");
    fs::write(
        package_src.join("main.aic"),
        "module pkg.main; fn main() -> Int { 0 }\n",
    )
    .expect("write package main");

    let build = Command::new(env!("CARGO_BIN_EXE_aic"))
        .arg("build")
        .arg("compiler/aic/tools/aic_selfhost")
        .arg("-o")
        .arg(&bin)
        .arg("--release")
        .current_dir(&root)
        .output()
        .expect("build aic_selfhost");
    assert!(
        build.status.success(),
        "aic_selfhost build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let self_check = Command::new(&bin)
        .arg("check")
        .arg("compiler/aic/tools/aic_selfhost")
        .current_dir(&root)
        .output()
        .expect("selfhost self check");
    assert!(
        self_check.status.success(),
        "selfhost tool did not check its own source graph\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&self_check.stdout),
        String::from_utf8_lossy(&self_check.stderr)
    );
    assert!(String::from_utf8_lossy(&self_check.stdout).contains("check: ok"));

    let check = Command::new(&bin)
        .arg("check")
        .arg(&ok_source)
        .current_dir(&root)
        .output()
        .expect("selfhost check");
    assert!(
        check.status.success(),
        "check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(String::from_utf8_lossy(&check.stdout).contains("check: ok"));

    let ir = Command::new(&bin)
        .arg("ir")
        .arg(&ok_source)
        .arg("--emit")
        .arg("json")
        .current_dir(&root)
        .output()
        .expect("selfhost ir json");
    assert!(
        ir.status.success(),
        "ir failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&ir.stdout),
        String::from_utf8_lossy(&ir.stderr)
    );
    assert!(String::from_utf8_lossy(&ir.stdout).contains("\"format\":\"aicore-selfhost-ir-v1\""));

    let ir_alias = Command::new(&bin)
        .arg("ir-json")
        .arg(&ok_source)
        .current_dir(&root)
        .output()
        .expect("selfhost ir-json alias");
    assert!(
        ir_alias.status.success(),
        "ir-json alias failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&ir_alias.stdout),
        String::from_utf8_lossy(&ir_alias.stderr)
    );

    let built = Command::new(&bin)
        .arg("build")
        .arg(&ok_source)
        .arg("-o")
        .arg(&artifact)
        .current_dir(&root)
        .output()
        .expect("selfhost build");
    assert!(
        built.status.success(),
        "build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(String::from_utf8_lossy(&built.stdout).contains("built "));
    assert!(
        fs::metadata(&artifact)
            .expect("built artifact metadata")
            .len()
            > 0
    );
    let artifact_run = Command::new(&artifact)
        .current_dir(&root)
        .status()
        .expect("run built artifact");
    assert!(
        artifact_run.success(),
        "built artifact did not run successfully"
    );

    let package_check = Command::new(&bin)
        .arg("check")
        .arg(&package_dir)
        .current_dir(&root)
        .output()
        .expect("selfhost package check");
    assert!(
        package_check.status.success(),
        "package check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&package_check.stdout),
        String::from_utf8_lossy(&package_check.stderr)
    );

    let bad = Command::new(&bin)
        .arg("check")
        .arg(&bad_source)
        .current_dir(&root)
        .output()
        .expect("selfhost negative check");
    assert!(!bad.status.success(), "negative check unexpectedly passed");
    assert!(String::from_utf8_lossy(&bad.stderr).contains("E1208"));

    let run = Command::new(&bin)
        .arg("run")
        .arg(&ok_source)
        .current_dir(&root)
        .output()
        .expect("selfhost run");
    assert!(
        run.status.success(),
        "run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let unsupported = Command::new(&bin)
        .arg("fmt")
        .arg(&ok_source)
        .current_dir(&root)
        .output()
        .expect("selfhost unsupported command");
    assert!(
        !unsupported.status.success(),
        "unsupported command unexpectedly passed"
    );
    assert!(String::from_utf8_lossy(&unsupported.stderr).contains("E5200"));

    let parity_report = tmp.path().join("selfhost-parity-report.json");
    let parity_artifacts = tmp.path().join("selfhost-parity-artifacts");
    let parity = run_parity(&[
        "--manifest".into(),
        "tests/selfhost/rust_vs_selfhost_manifest.json".into(),
        "--candidate".into(),
        bin.to_string_lossy().to_string(),
        "--artifact-dir".into(),
        parity_artifacts.to_string_lossy().to_string(),
        "--report".into(),
        parity_report.to_string_lossy().to_string(),
        "--timeout".into(),
        "60".into(),
    ]);
    assert!(
        parity.status.success(),
        "selfhost candidate parity failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&parity.stdout),
        String::from_utf8_lossy(&parity.stderr)
    );
    let parity_json: Value =
        serde_json::from_str(&fs::read_to_string(parity_report).expect("read parity report"))
            .expect("parity json");
    assert_eq!(parity_json["ok"], true);
    assert!(parity_json["results"]
        .as_array()
        .expect("results")
        .iter()
        .any(|result| result["comparison_mode"] == "selfhost-ir-json"));
    assert!(parity_json["results"]
        .as_array()
        .expect("results")
        .iter()
        .any(|result| result["comparison_mode"] == "diagnostic-code"));

    let stage_report = tmp.path().join("selfhost-stage-matrix-report.json");
    let stage_artifacts = tmp.path().join("selfhost-stage-matrix-artifacts");
    let stage_matrix = run_stage_matrix(&[
        "--manifest".into(),
        "tests/selfhost/stage_matrix_manifest.json".into(),
        "--stage-compiler".into(),
        bin.to_string_lossy().to_string(),
        "--artifact-dir".into(),
        stage_artifacts.to_string_lossy().to_string(),
        "--report".into(),
        stage_report.to_string_lossy().to_string(),
        "--timeout".into(),
        "90".into(),
    ]);
    assert!(
        stage_matrix.status.success(),
        "selfhost stage matrix failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&stage_matrix.stdout),
        String::from_utf8_lossy(&stage_matrix.stderr)
    );
    let stage_json: Value =
        serde_json::from_str(&fs::read_to_string(stage_report).expect("read stage matrix report"))
            .expect("stage matrix json");
    assert_eq!(stage_json["ok"], true);
    assert_eq!(stage_json["summary"]["failed"], 0);
    assert!(stage_json["results"]
        .as_array()
        .expect("results")
        .iter()
        .any(|result| result["kind"] == "package" && result["action"] == "run"));
    assert!(stage_json["results"]
        .as_array()
        .expect("results")
        .iter()
        .any(|result| result["expected"] == "fail" && result["status"] == "passed"));
    assert!(stage_json["results"]
        .as_array()
        .expect("results")
        .iter()
        .any(|result| result["expected"] == "unsupported"
            && result["status"] == "unsupported"
            && result["readiness"] == false));
}

#[test]
fn selfhost_parity_fake_compilers_match_and_write_report() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("pass.aic"), "fn main() -> Int { 0 }\n").expect("write pass");
    fs::write(
        tmp.path().join("fail.aic"),
        "fn main() -> Int { missing_symbol() }\n",
    )
    .expect("write fail");
    let manifest = tmp.path().join("manifest.json");
    write_manifest(&manifest);
    let compiler = tmp.path().join("fake_compiler.py");
    write_fake_compiler(&compiler, "same");
    let report = tmp.path().join("report.json");
    let artifact_dir = tmp.path().join("artifacts");

    let output = run_parity(&[
        "--manifest".into(),
        manifest.to_string_lossy().to_string(),
        "--root".into(),
        tmp.path().to_string_lossy().to_string(),
        "--reference".into(),
        format!("python3 {}", compiler.to_string_lossy()),
        "--candidate".into(),
        format!("python3 {}", compiler.to_string_lossy()),
        "--artifact-dir".into(),
        artifact_dir.to_string_lossy().to_string(),
        "--report".into(),
        report.to_string_lossy().to_string(),
        "--timeout".into(),
        "5".into(),
    ]);

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report_json: Value =
        serde_json::from_str(&fs::read_to_string(report).expect("read report")).expect("json");
    assert_eq!(report_json["format"], "aicore-selfhost-parity-v1");
    assert_eq!(report_json["ok"], true);
    assert_eq!(report_json["results"].as_array().expect("results").len(), 4);
}

#[test]
fn selfhost_stage_matrix_fake_compiler_writes_report() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("pass.aic"), "fn main() -> Int { 0 }\n").expect("write pass");
    fs::write(
        tmp.path().join("fail.aic"),
        "fn main() -> Int { missing_symbol() }\n",
    )
    .expect("write fail");
    fs::create_dir(tmp.path().join("unsupported_workspace")).expect("create unsupported workspace");
    let manifest = tmp.path().join("stage-matrix.json");
    fs::write(
        &manifest,
        r#"{
  "schema_version": 1,
  "name": "test-stage-matrix",
  "cases": [
    {
      "name": "pass_case",
      "kind": "single-file",
      "path": "pass.aic",
      "expected": "pass",
      "actions": ["check", "ir-json", "build", "run"]
    },
    {
      "name": "fail_case",
      "kind": "single-file",
      "path": "fail.aic",
      "expected": "fail",
      "actions": ["check"],
      "diagnostic_codes": {
        "check": ["E1258"]
      }
    },
    {
      "name": "unsupported_workspace",
      "kind": "workspace",
      "path": "unsupported_workspace",
      "expected": "unsupported",
      "readiness": false,
      "actions": ["check"],
      "diagnostic_codes": {
        "check": ["E5202"]
      }
    }
  ]
}
"#,
    )
    .expect("write stage matrix manifest");
    let compiler = tmp.path().join("fake_stage_compiler.py");
    write_fake_stage_compiler(&compiler);
    let report = tmp.path().join("stage-report.json");
    let artifact_dir = tmp.path().join("stage-artifacts");

    let output = run_stage_matrix(&[
        "--manifest".into(),
        manifest.to_string_lossy().to_string(),
        "--root".into(),
        tmp.path().to_string_lossy().to_string(),
        "--stage-compiler".into(),
        format!("python3 {}", compiler.to_string_lossy()),
        "--artifact-dir".into(),
        artifact_dir.to_string_lossy().to_string(),
        "--report".into(),
        report.to_string_lossy().to_string(),
        "--timeout".into(),
        "5".into(),
    ]);

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report_json: Value =
        serde_json::from_str(&fs::read_to_string(report).expect("read stage report"))
            .expect("stage report json");
    assert_eq!(report_json["format"], "aicore-selfhost-stage-matrix-v1");
    assert_eq!(report_json["ok"], true);
    assert_eq!(report_json["summary"]["result_count"], 6);
    assert_eq!(report_json["summary"]["passed"], 5);
    assert_eq!(report_json["summary"]["unsupported"], 1);
    assert_eq!(report_json["summary"]["failed"], 0);
    assert_eq!(report_json["summary"]["readiness_passed"], 5);
    assert_eq!(report_json["summary"]["readiness_failed"], 0);
    assert!(report_json["results"]
        .as_array()
        .expect("results")
        .iter()
        .any(|result| result["action"] == "build"
            && result["artifact_exists"] == true
            && result["artifact_sha256"]
                .as_str()
                .expect("artifact digest")
                .starts_with("sha256:")));
    assert!(report_json["results"]
        .as_array()
        .expect("results")
        .iter()
        .any(|result| result["status"] == "unsupported"
            && result["readiness"] == false
            && result["diagnostic_codes"]
                .as_array()
                .expect("diagnostic codes")
                .iter()
                .any(|code| code == "E5202")));
}

#[test]
fn selfhost_stage_matrix_rejects_unsupported_readiness_cases() {
    let tmp = tempdir().expect("tempdir");
    let manifest = tmp.path().join("bad-stage-matrix.json");
    fs::write(
        &manifest,
        r#"{
  "schema_version": 1,
  "name": "bad-stage-matrix",
  "cases": [
    {
      "name": "bad_workspace",
      "kind": "workspace",
      "path": "workspace",
      "expected": "unsupported",
      "readiness": true,
      "actions": ["check"]
    }
  ]
}
"#,
    )
    .expect("write bad stage matrix manifest");

    let output = run_stage_matrix(&[
        "--manifest".into(),
        manifest.to_string_lossy().to_string(),
        "--root".into(),
        tmp.path().to_string_lossy().to_string(),
        "--list".into(),
    ]);

    assert!(
        !output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("unsupported cases cannot count as readiness coverage"));
}

#[test]
fn selfhost_parity_ir_json_uses_canonical_json_fingerprint() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("pass.aic"), "fn main() -> Int { 0 }\n").expect("write pass");
    let manifest = tmp.path().join("manifest.json");
    fs::write(
        &manifest,
        r#"{
  "schema_version": 1,
  "name": "test-selfhost-ir-json",
  "cases": [
    {
      "name": "pass_case",
      "path": "pass.aic",
      "expected": "pass",
      "actions": ["ir-json"]
    }
  ]
}
"#,
    )
    .expect("write manifest");
    let reference = tmp.path().join("reference.py");
    let candidate = tmp.path().join("candidate.py");
    write_ir_json_compiler(&reference, "{\"schema_version\":1,\"b\":2,\"a\":1}");
    write_ir_json_compiler(
        &candidate,
        "{\n  \"a\": 1,\n  \"schema_version\": 1,\n  \"b\": 2\n}",
    );
    let report = tmp.path().join("ir-report.json");

    let output = run_parity(&[
        "--manifest".into(),
        manifest.to_string_lossy().to_string(),
        "--root".into(),
        tmp.path().to_string_lossy().to_string(),
        "--reference".into(),
        format!("python3 {}", reference.to_string_lossy()),
        "--candidate".into(),
        format!("python3 {}", candidate.to_string_lossy()),
        "--report".into(),
        report.to_string_lossy().to_string(),
        "--timeout".into(),
        "5".into(),
    ]);

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report_json: Value =
        serde_json::from_str(&fs::read_to_string(report).expect("read report")).expect("json");
    let result = &report_json["results"][0];
    assert_eq!(result["ok"], true);
    assert_eq!(result["reference"]["comparison_kind"], "canonical_json");
    assert_eq!(result["candidate"]["comparison_kind"], "canonical_json");
    assert_eq!(
        result["reference"]["stdout_json_fingerprint"],
        result["candidate"]["stdout_json_fingerprint"]
    );
    assert!(result["reference"]["stdout_json_error"].is_null());
    assert!(result["candidate"]["stdout_json_error"].is_null());
}

#[test]
fn selfhost_parity_ir_json_rejects_malformed_candidate_output() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("pass.aic"), "fn main() -> Int { 0 }\n").expect("write pass");
    let manifest = tmp.path().join("manifest.json");
    fs::write(
        &manifest,
        r#"{
  "schema_version": 1,
  "name": "test-selfhost-ir-json-malformed",
  "cases": [
    {
      "name": "pass_case",
      "path": "pass.aic",
      "expected": "pass",
      "actions": ["ir-json"]
    }
  ]
}
"#,
    )
    .expect("write manifest");
    let reference = tmp.path().join("reference.py");
    let candidate = tmp.path().join("candidate.py");
    write_ir_json_compiler(&reference, "{\"schema_version\":1}");
    write_ir_json_compiler(&candidate, "{");
    let report = tmp.path().join("ir-malformed-report.json");

    let output = run_parity(&[
        "--manifest".into(),
        manifest.to_string_lossy().to_string(),
        "--root".into(),
        tmp.path().to_string_lossy().to_string(),
        "--reference".into(),
        format!("python3 {}", reference.to_string_lossy()),
        "--candidate".into(),
        format!("python3 {}", candidate.to_string_lossy()),
        "--report".into(),
        report.to_string_lossy().to_string(),
        "--timeout".into(),
        "5".into(),
    ]);

    assert!(
        !output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report_json: Value =
        serde_json::from_str(&fs::read_to_string(report).expect("read report")).expect("json");
    assert_eq!(report_json["ok"], false);
    assert_eq!(
        report_json["results"][0]["reason"],
        "candidate emitted invalid ir json"
    );
    assert!(report_json["results"][0]["candidate"]["stdout_json_error"].is_string());
}

#[test]
fn selfhost_parity_reports_candidate_mismatch() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("pass.aic"), "fn main() -> Int { 0 }\n").expect("write pass");
    let manifest = tmp.path().join("manifest.json");
    fs::write(
        &manifest,
        r#"{
  "schema_version": 1,
  "name": "test-selfhost-mismatch",
  "cases": [
    {
      "name": "pass_case",
      "path": "pass.aic",
      "expected": "pass",
      "actions": ["check"]
    }
  ]
}
"#,
    )
    .expect("write manifest");
    let reference = tmp.path().join("reference.py");
    let candidate = tmp.path().join("candidate.py");
    write_fake_compiler(&reference, "reference");
    write_fake_compiler(&candidate, "candidate");
    let report = tmp.path().join("mismatch-report.json");

    let output = run_parity(&[
        "--manifest".into(),
        manifest.to_string_lossy().to_string(),
        "--root".into(),
        tmp.path().to_string_lossy().to_string(),
        "--reference".into(),
        format!("python3 {}", reference.to_string_lossy()),
        "--candidate".into(),
        format!("python3 {}", candidate.to_string_lossy()),
        "--report".into(),
        report.to_string_lossy().to_string(),
        "--timeout".into(),
        "5".into(),
    ]);

    assert!(
        !output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("fingerprint mismatch"), "stderr={stderr}");
    let report_json: Value =
        serde_json::from_str(&fs::read_to_string(report).expect("read report")).expect("json");
    assert_eq!(report_json["ok"], false);
    assert_eq!(report_json["results"][0]["reason"], "fingerprint mismatch");
    assert!(report_json["results"][0]["diff"]["stdout"]
        .as_str()
        .expect("stdout diff")
        .contains("--- reference stdout"));
}

#[test]
fn selfhost_parity_reports_timeout() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("pass.aic"), "fn main() -> Int { 0 }\n").expect("write pass");
    let manifest = tmp.path().join("manifest.json");
    fs::write(
        &manifest,
        r#"{
  "schema_version": 1,
  "name": "test-selfhost-timeout",
  "cases": [
    {
      "name": "pass_case",
      "path": "pass.aic",
      "expected": "pass",
      "actions": ["check"]
    }
  ]
}
"#,
    )
    .expect("write manifest");
    let reference = tmp.path().join("sleeping.py");
    fs::write(
        &reference,
        r#"#!/usr/bin/env python3
import time
time.sleep(20)
"#,
    )
    .expect("write sleeping compiler");
    let candidate = tmp.path().join("candidate.py");
    write_fake_compiler(&candidate, "candidate");
    let report = tmp.path().join("timeout-report.json");

    let output = run_parity(&[
        "--manifest".into(),
        manifest.to_string_lossy().to_string(),
        "--root".into(),
        tmp.path().to_string_lossy().to_string(),
        "--reference".into(),
        format!("python3 {}", reference.to_string_lossy()),
        "--candidate".into(),
        format!("python3 {}", candidate.to_string_lossy()),
        "--report".into(),
        report.to_string_lossy().to_string(),
        "--timeout".into(),
        "0.2".into(),
    ]);

    assert!(
        !output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report_json: Value =
        serde_json::from_str(&fs::read_to_string(report).expect("read report")).expect("json");
    assert_eq!(report_json["ok"], false);
    assert_eq!(report_json["results"][0]["reason"], "timeout");
}

#[test]
fn selfhost_parity_honors_action_specific_timeout() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("pass.aic"), "fn main() -> Int { 0 }\n").expect("write pass");
    let manifest = tmp.path().join("manifest.json");
    fs::write(
        &manifest,
        r#"{
  "schema_version": 1,
  "name": "test-selfhost-action-timeout",
  "cases": [
    {
      "name": "pass_case",
      "path": "pass.aic",
      "expected": "pass",
      "actions": ["check"],
      "timeouts": {
        "check": 1
      }
    }
  ]
}
"#,
    )
    .expect("write manifest");
    let compiler = tmp.path().join("sleep_then_ok.py");
    fs::write(
        &compiler,
        r#"#!/usr/bin/env python3
import time
time.sleep(0.3)
print("ok")
"#,
    )
    .expect("write sleeping compiler");
    let report = tmp.path().join("action-timeout-report.json");

    let output = run_parity(&[
        "--manifest".into(),
        manifest.to_string_lossy().to_string(),
        "--root".into(),
        tmp.path().to_string_lossy().to_string(),
        "--reference".into(),
        format!("python3 {}", compiler.to_string_lossy()),
        "--candidate".into(),
        format!("python3 {}", compiler.to_string_lossy()),
        "--report".into(),
        report.to_string_lossy().to_string(),
        "--timeout".into(),
        "0.1".into(),
    ]);

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report_json: Value =
        serde_json::from_str(&fs::read_to_string(report).expect("read report")).expect("json");
    assert_eq!(report_json["ok"], true);
    assert_eq!(
        report_json["results"][0]["reference"]["timeout_seconds"].as_f64(),
        Some(1.0)
    );
    assert_eq!(
        report_json["results"][0]["candidate"]["timeout_seconds"].as_f64(),
        Some(1.0)
    );
}

#[test]
fn selfhost_parity_rejects_invalid_manifest_timeout() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("pass.aic"), "fn main() -> Int { 0 }\n").expect("write pass");
    let manifest = tmp.path().join("manifest.json");
    fs::write(
        &manifest,
        r#"{
  "schema_version": 1,
  "name": "test-selfhost-invalid-timeout",
  "cases": [
    {
      "name": "pass_case",
      "path": "pass.aic",
      "expected": "pass",
      "actions": ["check"],
      "timeouts": {
        "run": 1
      }
    }
  ]
}
"#,
    )
    .expect("write manifest");

    let output = run_parity(&[
        "--manifest".into(),
        manifest.to_string_lossy().to_string(),
        "--root".into(),
        tmp.path().to_string_lossy().to_string(),
        "--list".into(),
    ]);

    assert!(
        !output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("timeout for non-case action"));
}
