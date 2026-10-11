use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Serialize;

use crate::ast::{self, TypeKind};
use crate::codegen::{
    builtin_intrinsic_lowering, intrinsic_binding_expectation, IntrinsicBindingExpectation,
    IntrinsicSignatureShape,
};
use crate::diagnostics::Severity;
use crate::parser;
use crate::span::Span;

const VERIFY_INTRINSICS_SCHEMA_VERSION: &str = "1.0";
const ISSUE_PARSE_DIAGNOSTIC: &str = "VI1000";
const ISSUE_UNSUPPORTED_ABI: &str = "VI1001";
const ISSUE_MISSING_LOWERING: &str = "VI1002";
const ISSUE_SIGNATURE_MISMATCH: &str = "VI1003";
const ISSUE_MISSING_RUNTIME_SYMBOL: &str = "VI1004";

#[derive(Debug, Clone, Serialize)]
pub struct VerifyIntrinsicsReport {
    pub schema_version: &'static str,
    pub input: String,
    pub files_scanned: usize,
    pub intrinsic_declarations: usize,
    /// Declarations bound to a runtime symbol with a matching signature.
    pub verified_bindings: usize,
    /// Declarations lowered by name with dedicated code generation.
    pub builtin_lowerings: usize,
    pub issue_count: usize,
    pub ok: bool,
    pub issues: Vec<VerifyIntrinsicsIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyIntrinsicsIssue {
    pub code: &'static str,
    pub kind: &'static str,
    pub file: String,
    pub span: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intrinsic: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_signatures: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abi: Option<String>,
}

#[derive(Debug, Clone)]
struct IntrinsicDeclaration {
    file: String,
    name: String,
    abi: Option<String>,
    params: Vec<String>,
    ret: String,
    span: Span,
}

impl IntrinsicDeclaration {
    fn signature(&self) -> String {
        format!("({}) -> {}", self.params.join(", "), self.ret)
    }
}

/// Where the verifier looks up backend lowerings. Production code uses the
/// code generator's tables; tests substitute tables with a mapping removed.
pub struct IntrinsicLoweringTables<'a> {
    pub runtime_binding: &'a dyn Fn(&str) -> Option<&'static IntrinsicBindingExpectation>,
    pub builtin_lowering: &'a dyn Fn(&str) -> bool,
}

pub fn verify_intrinsics(input: &Path) -> anyhow::Result<VerifyIntrinsicsReport> {
    verify_intrinsics_with_tables(
        input,
        &IntrinsicLoweringTables {
            runtime_binding: &intrinsic_binding_expectation,
            builtin_lowering: &builtin_intrinsic_lowering,
        },
    )
}

pub fn verify_intrinsics_with_tables(
    input: &Path,
    tables: &IntrinsicLoweringTables<'_>,
) -> anyhow::Result<VerifyIntrinsicsReport> {
    let mut files = Vec::new();
    collect_aic_files(input, &mut files)?;
    if files.is_empty() {
        anyhow::bail!("no .aic files found at {}", input.display());
    }

    let mut declarations = Vec::new();
    let mut issues = Vec::new();

    for file in &files {
        let source = fs::read_to_string(file)
            .with_context(|| format!("failed to read source file {}", file.display()))?;
        let file_display = file.display().to_string();
        let (program, parse_diags) = parser::parse(&source, &file_display);

        for diag in parse_diags
            .into_iter()
            .filter(|diag| diag.severity == Severity::Error)
        {
            let span = diag
                .spans
                .iter()
                .find(|span| span.file == file_display)
                .map(|span| Span::new(span.start, span.end))
                .unwrap_or_else(|| Span::new(0, 0));
            issues.push(VerifyIntrinsicsIssue {
                code: ISSUE_PARSE_DIAGNOSTIC,
                kind: "parse_diagnostic",
                file: file_display.clone(),
                span,
                intrinsic: None,
                message: format!("{}: {}", diag.code, diag.message),
                found_signature: None,
                expected_signatures: None,
                runtime_symbol: None,
                abi: None,
            });
        }

        if let Some(program) = program.as_ref() {
            collect_intrinsic_declarations(program, &file_display, &mut declarations);
        }
    }

    let mut verified_bindings = 0usize;
    let mut builtin_lowerings = 0usize;
    for decl in declarations.iter() {
        let abi = decl.abi.clone().unwrap_or_default();
        if abi != "runtime" {
            issues.push(VerifyIntrinsicsIssue {
                code: ISSUE_UNSUPPORTED_ABI,
                kind: "unsupported_abi",
                file: decl.file.clone(),
                span: decl.span,
                intrinsic: Some(decl.name.clone()),
                message: format!(
                    "intrinsic '{}' must use runtime abi; found '{}'",
                    decl.name,
                    if abi.is_empty() {
                        "<unset>"
                    } else {
                        abi.as_str()
                    }
                ),
                found_signature: Some(decl.signature()),
                expected_signatures: None,
                runtime_symbol: None,
                abi: decl.abi.clone(),
            });
            continue;
        }

        let Some(binding) = (tables.runtime_binding)(&decl.name) else {
            if (tables.builtin_lowering)(&decl.name) {
                builtin_lowerings += 1;
                continue;
            }
            issues.push(VerifyIntrinsicsIssue {
                code: ISSUE_MISSING_LOWERING,
                kind: "missing_lowering",
                file: decl.file.clone(),
                span: decl.span,
                intrinsic: Some(decl.name.clone()),
                message: format!("missing codegen lowering for intrinsic '{}'", decl.name),
                found_signature: Some(decl.signature()),
                expected_signatures: None,
                runtime_symbol: None,
                abi: decl.abi.clone(),
            });
            continue;
        };

        if binding.runtime_symbol.is_empty() {
            issues.push(VerifyIntrinsicsIssue {
                code: ISSUE_MISSING_RUNTIME_SYMBOL,
                kind: "missing_runtime_symbol",
                file: decl.file.clone(),
                span: decl.span,
                intrinsic: Some(decl.name.clone()),
                message: format!(
                    "intrinsic '{}' has lowering metadata without a runtime symbol",
                    decl.name
                ),
                found_signature: Some(decl.signature()),
                expected_signatures: Some(expected_signatures(binding)),
                runtime_symbol: None,
                abi: decl.abi.clone(),
            });
            continue;
        }

        if signature_matches(binding, decl) {
            verified_bindings += 1;
            continue;
        }

        issues.push(VerifyIntrinsicsIssue {
            code: ISSUE_SIGNATURE_MISMATCH,
            kind: "signature_mismatch",
            file: decl.file.clone(),
            span: decl.span,
            intrinsic: Some(decl.name.clone()),
            message: format!(
                "intrinsic '{}' signature does not match backend lowering expectations",
                decl.name
            ),
            found_signature: Some(decl.signature()),
            expected_signatures: Some(expected_signatures(binding)),
            runtime_symbol: Some(binding.runtime_symbol.to_string()),
            abi: decl.abi.clone(),
        });
    }

    issues.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then_with(|| left.span.start.cmp(&right.span.start))
            .then_with(|| left.span.end.cmp(&right.span.end))
            .then_with(|| left.code.cmp(right.code))
            .then_with(|| left.intrinsic.cmp(&right.intrinsic))
    });

    let issue_count = issues.len();
    Ok(VerifyIntrinsicsReport {
        schema_version: VERIFY_INTRINSICS_SCHEMA_VERSION,
        input: input.display().to_string(),
        files_scanned: files.len(),
        intrinsic_declarations: declarations.len(),
        verified_bindings,
        builtin_lowerings,
        issue_count,
        ok: issue_count == 0,
        issues,
    })
}

fn signature_matches(binding: &IntrinsicBindingExpectation, decl: &IntrinsicDeclaration) -> bool {
    binding.signatures.iter().any(|shape| {
        shape.params.len() == decl.params.len()
            && shape
                .params
                .iter()
                .zip(decl.params.iter())
                .all(|(expected, actual)| *expected == actual)
            && shape.ret == decl.ret
    })
}

fn expected_signatures(binding: &IntrinsicBindingExpectation) -> Vec<String> {
    binding.signatures.iter().map(signature_shape).collect()
}

fn signature_shape(shape: &IntrinsicSignatureShape) -> String {
    format!("({}) -> {}", shape.params.join(", "), shape.ret)
}

fn collect_intrinsic_declarations(
    program: &ast::Program,
    file: &str,
    out: &mut Vec<IntrinsicDeclaration>,
) {
    for item in &program.items {
        match item {
            ast::Item::Function(func) => push_intrinsic_function(func, file, out),
            ast::Item::Trait(trait_def) => {
                for method in &trait_def.methods {
                    push_intrinsic_function(method, file, out);
                }
            }
            ast::Item::Impl(impl_def) => {
                for method in &impl_def.methods {
                    push_intrinsic_function(method, file, out);
                }
            }
            ast::Item::Struct(_) | ast::Item::Enum(_) => {}
        }
    }
}

fn push_intrinsic_function(func: &ast::Function, file: &str, out: &mut Vec<IntrinsicDeclaration>) {
    if !func.is_intrinsic {
        return;
    }
    out.push(IntrinsicDeclaration {
        file: file.to_string(),
        name: func.name.clone(),
        abi: func.intrinsic_abi.clone(),
        params: func
            .params
            .iter()
            .map(|param| render_type(&param.ty))
            .collect(),
        ret: render_type(&func.ret_type),
        span: func.span,
    });
}

fn render_type(ty: &ast::TypeExpr) -> String {
    match &ty.kind {
        TypeKind::Unit => "()".to_string(),
        TypeKind::Hole => "_".to_string(),
        TypeKind::DynTrait { trait_name } => format!("dyn {trait_name}"),
        TypeKind::Named { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}[{}]",
                    name,
                    args.iter().map(render_type).collect::<Vec<_>>().join(", ")
                )
            }
        }
    }
}

fn collect_aic_files(path: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if path.is_file() {
        if path.extension().and_then(|ext| ext.to_str()) == Some("aic") {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }

    if !path.is_dir() {
        anyhow::bail!("path '{}' is not a file or directory", path.display());
    }

    let mut entries = fs::read_dir(path)
        .with_context(|| format!("failed to read directory {}", path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to enumerate directory {}", path.display()))?;

    entries.sort_by(|left, right| left.path().cmp(&right.path()));

    for entry in entries {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_aic_files(&entry_path, out)?;
        } else if entry_path.extension().and_then(|ext| ext.to_str()) == Some("aic") {
            out.push(entry_path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use super::{verify_intrinsics, verify_intrinsics_with_tables, IntrinsicLoweringTables};
    use crate::codegen::{
        builtin_intrinsic_lowering, builtin_intrinsic_lowerings, intrinsic_binding_expectation,
        intrinsic_binding_expectations,
    };

    fn std_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("std")
    }

    #[test]
    fn std_intrinsics_are_fully_covered() {
        let report = verify_intrinsics(&std_root()).expect("verify std");
        assert!(report.ok, "issues: {:#?}", report.issues);
        assert_eq!(
            report.verified_bindings + report.builtin_lowerings,
            report.intrinsic_declarations,
            "every std intrinsic declaration must have a backend lowering"
        );
        assert_eq!(
            report.verified_bindings,
            intrinsic_binding_expectations().len()
        );
        assert_eq!(
            report.builtin_lowerings,
            builtin_intrinsic_lowerings().len()
        );
    }

    #[test]
    fn removing_a_builtin_lowering_fails_verification() {
        let removed = "aic_vec_push_intrinsic";
        assert!(builtin_intrinsic_lowering(removed));
        let builtin = |name: &str| name != removed && builtin_intrinsic_lowering(name);
        let report = verify_intrinsics_with_tables(
            &std_root(),
            &IntrinsicLoweringTables {
                runtime_binding: &intrinsic_binding_expectation,
                builtin_lowering: &builtin,
            },
        )
        .expect("verify std");
        assert!(!report.ok);
        assert_eq!(report.issue_count, 1, "{:#?}", report.issues);
        let issue = &report.issues[0];
        assert_eq!(issue.code, "VI1002");
        assert_eq!(issue.intrinsic.as_deref(), Some(removed));
        assert!(issue.file.ends_with("vec.aic"), "{issue:#?}");
    }

    #[test]
    fn removing_a_runtime_binding_fails_verification() {
        let removed = intrinsic_binding_expectations()[0].intrinsic;
        let runtime = |name: &str| {
            if name == removed {
                None
            } else {
                intrinsic_binding_expectation(name)
            }
        };
        let report = verify_intrinsics_with_tables(
            &std_root(),
            &IntrinsicLoweringTables {
                runtime_binding: &runtime,
                builtin_lowering: &builtin_intrinsic_lowering,
            },
        )
        .expect("verify std");
        assert_eq!(report.issue_count, 1, "{:#?}", report.issues);
        assert_eq!(report.issues[0].code, "VI1002");
        assert_eq!(report.issues[0].intrinsic.as_deref(), Some(removed));
    }

    #[test]
    fn unknown_intrinsic_declaration_is_reported() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("bad.aic");
        fs::write(
            &file,
            "intrinsic fn aic_made_up_intrinsic(x: Int) -> Int;\n",
        )
        .expect("write source");
        let report = verify_intrinsics(&file).expect("verify");
        assert!(!report.ok);
        assert_eq!(report.issues[0].code, "VI1002");
        assert_eq!(report.builtin_lowerings, 0);
    }

    #[test]
    fn backend_lowered_std_functions_have_no_bodies() {
        let mut files = Vec::new();
        super::collect_aic_files(&std_root(), &mut files).expect("collect std");
        let mut offenders = Vec::new();
        for file in files {
            let source = fs::read_to_string(&file).expect("read std file");
            let (program, _) = crate::parser::parse(&source, &file.display().to_string());
            for item in program.expect("std parses").items {
                let crate::ast::Item::Function(func) = item else {
                    continue;
                };
                let lowered = builtin_intrinsic_lowering(&func.name)
                    || intrinsic_binding_expectation(&func.name).is_some();
                if lowered && !func.is_intrinsic {
                    offenders.push(format!("{}: {}", file.display(), func.name));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "backend-lowered functions must be `intrinsic fn` declarations, not bodies: {offenders:#?}"
        );
    }

    #[test]
    fn builtin_lowering_table_is_sorted_and_disjoint_from_runtime_bindings() {
        let names = builtin_intrinsic_lowerings();
        assert!(
            names.windows(2).all(|pair| pair[0] < pair[1]),
            "BUILTIN_INTRINSIC_LOWERINGS must stay sorted and unique for binary search"
        );
        for name in names {
            assert!(
                intrinsic_binding_expectation(name).is_none(),
                "{name} is in both lowering tables"
            );
        }
    }
}
