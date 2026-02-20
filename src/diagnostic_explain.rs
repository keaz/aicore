use serde::{Deserialize, Serialize};

use crate::diagnostic_codes::{is_registered, is_valid_format, REGISTERED_DIAGNOSTIC_CODES};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticExplanation {
    pub code: String,
    pub known: bool,
    pub title: String,
    pub summary: String,
    pub remediation: Vec<String>,
    pub spec_refs: Vec<String>,
    pub examples: Vec<String>,
}

pub fn explain(code: &str) -> DiagnosticExplanation {
    let normalized = code.trim().to_ascii_uppercase();
    if !is_valid_format(&normalized) {
        return DiagnosticExplanation {
            code: normalized,
            known: false,
            title: "invalid diagnostic code format".to_string(),
            summary: "diagnostic codes must use E#### format".to_string(),
            remediation: vec!["use `aic explain E1234` format".to_string()],
            spec_refs: vec!["docs/diagnostic-codes.md".to_string()],
            examples: vec!["aic explain E2001".to_string()],
        };
    }

    if !is_registered(&normalized) {
        return DiagnosticExplanation {
            code: normalized,
            known: false,
            title: "unknown diagnostic code".to_string(),
            summary: "this code is not in the current diagnostic registry".to_string(),
            remediation: vec![
                "check the compiler version and diagnostic registry".to_string(),
                "run `aic diag --json <file>` to inspect emitted codes".to_string(),
            ],
            spec_refs: vec!["docs/diagnostic-codes.md".to_string()],
            examples: vec!["aic diag --json examples/e7/explain_trigger.aic".to_string()],
        };
    }

    let number = normalized[1..].parse::<u32>().unwrap_or_default();
    let (title, summary, remediation, spec_refs, examples) = classify(number, &normalized);

    DiagnosticExplanation {
        code: normalized,
        known: true,
        title: title.to_string(),
        summary: summary.to_string(),
        remediation,
        spec_refs,
        examples,
    }
}

fn classify(
    number: u32,
    code: &str,
) -> (
    &'static str,
    &'static str,
    Vec<String>,
    Vec<String>,
    Vec<String>,
) {
    match number {
        0..=999 => (
            "lexing/parsing front-end diagnostic",
            "the source text does not satisfy the parser grammar or token rules",
            vec![
                "fix the nearest syntax/token issue at the reported span".to_string(),
                "run `aic fmt --check` after edits to confirm canonical syntax".to_string(),
            ],
            vec!["docs/syntax.md".to_string(), "docs/spec.md".to_string()],
            vec!["examples/e7/explain_trigger.aic".to_string()],
        ),
        1000..=1099 => (
            "parser grammar diagnostic",
            "the parser rejected or recovered from invalid grammar in the source",
            vec![
                "fix the token sequence around the highlighted span".to_string(),
                "prefer building incrementally and re-running `aic check`".to_string(),
            ],
            vec!["docs/syntax.md".to_string(), "docs/spec.md".to_string()],
            vec!["aic check examples/e7/explain_trigger.aic".to_string()],
        ),
        1100..=1199 => (
            "name resolution diagnostic",
            "symbol namespaces or declarations conflict during resolution",
            vec![
                "rename conflicting symbols or fix module imports".to_string(),
                "verify module declarations and import aliases are unique".to_string(),
            ],
            vec![
                "docs/type-system.md".to_string(),
                "docs/spec.md".to_string(),
            ],
            vec!["aic check examples/e7/lsp_project/src/main.aic".to_string()],
        ),
        1200..=1299 => (
            "type-system diagnostic",
            "an expression, pattern, or generic instantiation violates typing rules",
            vec![
                "align expression types with function/field/variant expectations".to_string(),
                "add explicit annotations where inference is ambiguous".to_string(),
            ],
            vec![
                "docs/type-system.md".to_string(),
                "docs/spec.md".to_string(),
            ],
            vec!["aic check examples/e3/generic_id.aic".to_string()],
        ),
        1300..=1399 => (
            "pattern matching diagnostic",
            "match arms are non-exhaustive, unreachable, or inconsistent with ADT structure",
            vec![
                "cover all variants/cases or add a wildcard arm".to_string(),
                "remove dead arms caused by earlier exhaustive patterns".to_string(),
            ],
            vec!["docs/type-system.md".to_string()],
            vec!["aic check examples/e3/match_exhaustive.aic".to_string()],
        ),
        2000..=2099 => (
            "effect-system diagnostic",
            "a call requires effects not declared in the enclosing function",
            vec![
                "add missing effects in `effects { ... }` on the caller".to_string(),
                "or refactor to call a pure helper without side effects".to_string(),
            ],
            vec![
                "docs/effect-system.md".to_string(),
                "docs/spec.md".to_string(),
            ],
            vec!["aic check examples/effects_reject.aic".to_string()],
        ),
        2100..=2199 => (
            "module/package diagnostic",
            "module imports, package lock/checksum, or dependency workflow is inconsistent",
            vec![
                "fix module declarations/import paths and rerun checks".to_string(),
                "run `aic lock` when package metadata intentionally changes".to_string(),
            ],
            vec![
                "docs/package-workflow.md".to_string(),
                "docs/spec.md".to_string(),
            ],
            vec!["aic check examples/e6/pkg_app --offline".to_string()],
        ),
        4000..=4099 => (
            "contracts diagnostic",
            "requires/ensures/invariant obligations were violated or not provable",
            vec![
                "repair contract expressions or function behavior".to_string(),
                "use runtime checks for obligations that cannot be statically discharged"
                    .to_string(),
            ],
            vec!["docs/contracts.md".to_string(), "docs/spec.md".to_string()],
            vec!["aic run examples/contracts_abs_fail.aic".to_string()],
        ),
        5000..=5999 => (
            "backend/runtime diagnostic",
            "LLVM emission, ABI lowering, or toolchain validation failed",
            vec![
                "verify clang/LLVM setup and pinned major version".to_string(),
                "reduce unsupported IR patterns before codegen".to_string(),
            ],
            vec!["docs/llvm-backend.md".to_string()],
            vec!["aic build examples/e5/hello_int.aic".to_string()],
        ),
        6000..=6999 => (
            "stdlib compatibility diagnostic",
            "deprecated or incompatible std API usage detected",
            vec![
                "migrate to replacement APIs shown in diagnostic help".to_string(),
                "run `aic std-compat --check` for policy enforcement".to_string(),
            ],
            vec!["docs/std-compatibility.md".to_string()],
            vec!["aic check examples/e6/deprecated_api_use.aic".to_string()],
        ),
        _ => (
            "registered diagnostic",
            "the code is registered; see registry and emitted help text for details",
            vec!["read the specific message/help in diagnostic output".to_string()],
            vec!["docs/diagnostic-codes.md".to_string()],
            vec![format!("aic explain {code}")],
        ),
    }
}

pub fn explain_text(entry: &DiagnosticExplanation) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}: {}\n", entry.code, entry.title));
    out.push_str(&format!("summary: {}\n", entry.summary));
    if !entry.remediation.is_empty() {
        out.push_str("remediation:\n");
        for item in &entry.remediation {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }
    if !entry.spec_refs.is_empty() {
        out.push_str("spec refs:\n");
        for item in &entry.spec_refs {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }
    if !entry.examples.is_empty() {
        out.push_str("examples:\n");
        for item in &entry.examples {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }
    out
}

pub fn registry_explain_coverage() -> bool {
    REGISTERED_DIAGNOSTIC_CODES
        .iter()
        .all(|code| explain(code).known)
}

#[cfg(test)]
mod tests {
    use crate::diagnostic_codes::REGISTERED_DIAGNOSTIC_CODES;

    use super::{explain, registry_explain_coverage};

    #[test]
    fn registered_codes_have_explanations() {
        assert!(registry_explain_coverage());
        for code in REGISTERED_DIAGNOSTIC_CODES {
            assert!(explain(code).known, "missing explain for {code}");
        }
    }

    #[test]
    fn unknown_code_is_deterministic() {
        let unknown = format!("E{}{}{}{}", 9, 9, 9, 9);
        let a = explain(&unknown);
        let b = explain(&unknown.to_lowercase());
        assert!(!a.known);
        assert_eq!(a, b);
        assert!(a.summary.contains("not in the current diagnostic registry"));
    }
}
