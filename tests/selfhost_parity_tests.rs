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

fn run_parity(args: &[String]) -> std::process::Output {
    Command::new("python3")
        .arg(parity_script())
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("run parity script")
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
        "compiler/aic/tools/source_diagnostics_check/aic.toml",
        "compiler/aic/tools/source_diagnostics_check/src/main.aic",
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
    assert!(backend.contains("generic-definition-metadata"));
    assert!(backend.contains("native-link-metadata"));
    assert!(backend.contains("E5101"));
    assert!(backend.contains("E5102"));
    assert!(backend.contains("E5103"));
    assert!(backend.contains("E5104"));
    assert!(backend.contains("E5105"));

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
    assert!(source_diagnostics_check.contains("fn valid_backend_negative_cases"));
    assert!(source_diagnostics_check.contains("fn valid_backend_frontend"));
    assert!(source_diagnostics_check.contains("emit_backend_artifact"));
    assert!(source_diagnostics_check.contains("backend_has_diagnostic_code"));

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
