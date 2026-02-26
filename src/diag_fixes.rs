use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use serde::{Deserialize, Serialize};

use crate::diagnostics::{Diagnostic, SuggestedFix};

const SAFE_FIX_CODES: &[&str] = &[
    "E1033", "E1034", "E1041", "E1062", "E2001", "E2005", "E2009", "E6004", "E6006",
];
const FIX_PROTOCOL_VERSION: &str = "1.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixEdit {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixResponse {
    pub protocol_version: String,
    pub phase: String,
    pub mode: String,
    pub ok: bool,
    pub files_changed: Vec<String>,
    pub applied_edits: Vec<FixEdit>,
    pub conflicts: Vec<FixEdit>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CandidateEdit {
    file: String,
    start: usize,
    end: usize,
    replacement: String,
    message: String,
    code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixPlan {
    pub applied_edits: Vec<FixEdit>,
    pub conflicts: Vec<FixEdit>,
}

pub fn collect_safe_fix_plan(diagnostics: &[Diagnostic]) -> FixPlan {
    let mut candidates = diagnostics
        .iter()
        .flat_map(|diag| {
            let Some(primary_span) = diag.spans.first() else {
                return Vec::new();
            };
            if !SAFE_FIX_CODES.contains(&diag.code.as_str()) {
                return Vec::new();
            }
            diag.suggested_fixes
                .iter()
                .filter_map(|fix| {
                    candidate_fix_from_diagnostic(primary_span.file.as_str(), diag, fix)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.start.cmp(&b.start))
            .then(a.end.cmp(&b.end))
            .then(a.replacement.cmp(&b.replacement))
            .then(a.message.cmp(&b.message))
            .then(a.code.cmp(&b.code))
    });

    let mut accepted: Vec<CandidateEdit> = Vec::new();
    let mut conflicts = Vec::new();

    for candidate in candidates {
        let duplicate = accepted.iter().any(|existing| {
            existing.file == candidate.file
                && existing.start == candidate.start
                && existing.end == candidate.end
                && existing.replacement == candidate.replacement
        });
        if duplicate {
            continue;
        }

        if let Some(existing) = accepted.iter_mut().find(|existing| {
            existing.file == candidate.file
                && existing.start == candidate.start
                && existing.end == candidate.end
                && existing.start == existing.end
                && candidate.start == candidate.end
        }) {
            if try_merge_effect_and_capability_insert(existing, &candidate) {
                continue;
            }
        }

        let conflict_with = accepted.iter().find(|existing| {
            existing.file == candidate.file
                && ranges_conflict(existing.start, existing.end, candidate.start, candidate.end)
        });
        if let Some(existing) = conflict_with {
            conflicts.push(FixEdit {
                file: candidate.file,
                start: candidate.start,
                end: candidate.end,
                replacement: candidate.replacement,
                message: format!(
                    "{} (conflicts with {} at {}..{})",
                    candidate.message, existing.code, existing.start, existing.end
                ),
            });
            continue;
        }
        accepted.push(candidate);
    }

    let applied_edits = accepted
        .into_iter()
        .map(|candidate| FixEdit {
            file: candidate.file,
            start: candidate.start,
            end: candidate.end,
            replacement: candidate.replacement,
            message: candidate.message,
        })
        .collect::<Vec<_>>();

    FixPlan {
        applied_edits,
        conflicts,
    }
}

pub fn apply_safe_fixes(diagnostics: &[Diagnostic], dry_run: bool) -> anyhow::Result<FixResponse> {
    let plan = collect_safe_fix_plan(diagnostics);

    let files_changed = if dry_run || !plan.conflicts.is_empty() {
        let mut files = plan
            .applied_edits
            .iter()
            .map(|edit| edit.file.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        files.sort();
        files
    } else {
        apply_edits_to_files(&plan.applied_edits)?
    };

    Ok(FixResponse {
        protocol_version: FIX_PROTOCOL_VERSION.to_string(),
        phase: "fix".to_string(),
        mode: if dry_run {
            "dry-run".to_string()
        } else {
            "apply".to_string()
        },
        ok: plan.conflicts.is_empty(),
        files_changed,
        applied_edits: plan.applied_edits,
        conflicts: plan.conflicts,
        diagnostics: diagnostics.to_vec(),
    })
}

pub fn apply_text_edits(source: &str, edits: &[FixEdit]) -> anyhow::Result<String> {
    let mut ordered = edits.to_vec();
    ordered.sort_by(|a, b| b.start.cmp(&a.start).then(b.end.cmp(&a.end)));

    let mut output = source.to_string();
    for edit in ordered {
        if edit.end < edit.start {
            anyhow::bail!(
                "invalid edit range {}..{} for {}",
                edit.start,
                edit.end,
                edit.file
            );
        }
        if edit.end > output.len() {
            anyhow::bail!(
                "edit out of bounds {}..{} for {} (len={})",
                edit.start,
                edit.end,
                edit.file,
                output.len()
            );
        }
        if !output.is_char_boundary(edit.start) || !output.is_char_boundary(edit.end) {
            anyhow::bail!(
                "edit range {}..{} is not UTF-8 boundary for {}",
                edit.start,
                edit.end,
                edit.file
            );
        }
        output.replace_range(edit.start..edit.end, &edit.replacement);
    }

    Ok(output)
}

fn apply_edits_to_files(edits: &[FixEdit]) -> anyhow::Result<Vec<String>> {
    let mut by_file = BTreeMap::<String, Vec<FixEdit>>::new();
    for edit in edits {
        by_file
            .entry(edit.file.clone())
            .or_default()
            .push(edit.clone());
    }

    let mut changed = Vec::new();
    for (file, file_edits) in by_file {
        let source = fs::read_to_string(&file)?;
        let rewritten = apply_text_edits(&source, &file_edits)?;
        if rewritten != source {
            fs::write(&file, rewritten)?;
            changed.push(file);
        }
    }

    changed.sort();
    changed.dedup();
    Ok(changed)
}

fn candidate_fix_from_diagnostic(
    file: &str,
    diag: &Diagnostic,
    fix: &SuggestedFix,
) -> Option<CandidateEdit> {
    let start = fix.start?;
    let end = fix.end?;
    let replacement = fix.replacement.clone()?;

    if end < start || file.is_empty() {
        return None;
    }

    Some(CandidateEdit {
        file: file.to_string(),
        start,
        end,
        replacement,
        message: fix.message.clone(),
        code: diag.code.clone(),
    })
}

fn ranges_conflict(start_a: usize, end_a: usize, start_b: usize, end_b: usize) -> bool {
    if start_a == end_a && start_b == end_b {
        return start_a == start_b;
    }
    if start_a == end_a {
        return start_b <= start_a && start_a < end_b;
    }
    if start_b == end_b {
        return start_a <= start_b && start_b < end_a;
    }
    start_a < end_b && start_b < end_a
}

fn try_merge_effect_and_capability_insert(
    existing: &mut CandidateEdit,
    candidate: &CandidateEdit,
) -> bool {
    if !is_effect_capability_fix_code(&existing.code)
        || !is_effect_capability_fix_code(&candidate.code)
    {
        return false;
    }

    let existing_effect = extract_named_clause(&existing.replacement, "effects");
    let existing_capability = extract_named_clause(&existing.replacement, "capabilities");
    let candidate_effect = extract_named_clause(&candidate.replacement, "effects");
    let candidate_capability = extract_named_clause(&candidate.replacement, "capabilities");

    if existing_effect.is_none()
        && existing_capability.is_none()
        && candidate_effect.is_none()
        && candidate_capability.is_none()
    {
        return false;
    }

    let effect_clause = candidate_effect.or(existing_effect);
    let capability_clause = candidate_capability.or(existing_capability);
    let mut parts = Vec::new();
    if let Some(effect) = effect_clause {
        parts.push(effect);
    }
    if let Some(capability) = capability_clause {
        parts.push(capability);
    }
    if parts.is_empty() {
        return false;
    }

    let leading_space =
        existing.replacement.starts_with(' ') || candidate.replacement.starts_with(' ');
    let trailing_space =
        existing.replacement.ends_with(' ') || candidate.replacement.ends_with(' ');

    let mut merged = parts.join(" ");
    if leading_space {
        merged = format!(" {merged}");
    }
    if trailing_space {
        merged.push(' ');
    }
    existing.replacement = merged;
    true
}

fn is_effect_capability_fix_code(code: &str) -> bool {
    matches!(code, "E2001" | "E2005" | "E2009")
}

fn extract_named_clause(text: &str, keyword: &str) -> Option<String> {
    let idx = text.find(keyword)?;
    let bytes = text.as_bytes();
    let mut cursor = idx + keyword.len();
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if cursor >= bytes.len() || bytes[cursor] != b'{' {
        return None;
    }
    let mut end = cursor + 1;
    while end < bytes.len() && bytes[end] != b'}' {
        end += 1;
    }
    if end >= bytes.len() {
        return None;
    }
    Some(text[idx..=end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use crate::{diagnostics::SuggestedFix, span::Span};

    use super::{apply_safe_fixes, apply_text_edits, collect_safe_fix_plan, FixEdit};

    fn make_diag(
        code: &str,
        file: &str,
        start: usize,
        end: usize,
        replacement: Option<&str>,
    ) -> crate::diagnostics::Diagnostic {
        crate::diagnostics::Diagnostic::error(code, "fix me", file, Span::new(start, end)).with_fix(
            SuggestedFix {
                message: format!("apply {}", code),
                replacement: replacement.map(str::to_string),
                start: Some(start),
                end: Some(end),
            },
        )
    }

    #[test]
    fn plan_is_deterministic_and_filters_to_safe_codes() {
        let diagnostics = vec![
            make_diag("E2003", "a.aic", 1, 1, Some(";")),
            make_diag("E1033", "b.aic", 4, 4, Some(";")),
            make_diag("E1062", "a.aic", 2, 2, Some(";")),
            make_diag("E1041", "a.aic", 0, 1, Some("0")),
        ];

        let first = collect_safe_fix_plan(&diagnostics);
        let second = collect_safe_fix_plan(&diagnostics);
        assert_eq!(first, second);
        assert_eq!(first.applied_edits.len(), 3);
        assert!(first.conflicts.is_empty());
        assert_eq!(first.applied_edits[0].file, "a.aic");
        assert_eq!(first.applied_edits[1].file, "a.aic");
        assert_eq!(first.applied_edits[2].file, "b.aic");
    }

    #[test]
    fn conflict_detection_is_actionable() {
        let diagnostics = vec![
            make_diag("E1033", "conflict.aic", 10, 12, Some("aa")),
            make_diag("E1062", "conflict.aic", 11, 13, Some("bb")),
        ];

        let plan = collect_safe_fix_plan(&diagnostics);
        assert_eq!(plan.applied_edits.len(), 1);
        assert_eq!(plan.conflicts.len(), 1);
        assert!(plan.conflicts[0].message.contains("conflicts with"));
    }

    #[test]
    fn unused_warning_fix_codes_are_treated_as_safe() {
        let diagnostics = vec![
            make_diag("E6004", "main.aic", 0, 12, Some("")),
            make_diag("E6006", "main.aic", 22, 29, Some("_unused")),
        ];

        let plan = collect_safe_fix_plan(&diagnostics);
        assert_eq!(plan.applied_edits.len(), 2);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn duplicate_effect_fixes_are_coalesced() {
        let diagnostics = vec![
            make_diag("E2001", "main.aic", 30, 30, Some(" effects { io }")),
            make_diag("E2005", "main.aic", 30, 30, Some(" effects { io }")),
        ];

        let plan = collect_safe_fix_plan(&diagnostics);
        assert_eq!(plan.applied_edits.len(), 1);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn effect_and_capability_insertions_are_merged_for_one_pass_fix() {
        let diagnostics = vec![
            make_diag("E2001", "main.aic", 30, 30, Some(" effects { io }")),
            make_diag("E2009", "main.aic", 30, 30, Some(" capabilities { io }")),
        ];

        let plan = collect_safe_fix_plan(&diagnostics);
        assert_eq!(plan.applied_edits.len(), 1);
        assert!(plan.conflicts.is_empty());
        assert!(
            plan.applied_edits[0]
                .replacement
                .contains("effects { io } capabilities { io }"),
            "replacement={}",
            plan.applied_edits[0].replacement
        );
    }

    #[test]
    fn text_edit_application_is_stable() {
        let edits = vec![
            FixEdit {
                file: "x.aic".to_string(),
                start: 3,
                end: 3,
                replacement: "!".to_string(),
                message: "append".to_string(),
            },
            FixEdit {
                file: "x.aic".to_string(),
                start: 1,
                end: 2,
                replacement: "Z".to_string(),
                message: "replace".to_string(),
            },
        ];

        let rewritten = apply_text_edits("abc", &edits).expect("apply edits");
        assert_eq!(rewritten, "aZc!");
    }

    #[test]
    fn apply_mode_writes_when_non_conflicting() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("main.aic");
        let source = "module m.main;\nfn main() -> Int {\n    let x = 1\n    x\n}\n";
        std::fs::write(&file, source).expect("write source");

        let insert_at = source
            .find("\n    x")
            .expect("find insertion point")
            .saturating_sub(0);
        let diagnostics = vec![crate::diagnostics::Diagnostic::error(
            "E1033",
            "expected ';' after let binding",
            &file.to_string_lossy(),
            Span::new(insert_at, insert_at),
        )
        .with_fix(SuggestedFix {
            message: "insert ';' after let binding".to_string(),
            replacement: Some(";".to_string()),
            start: Some(insert_at),
            end: Some(insert_at),
        })];

        let response = apply_safe_fixes(&diagnostics, false).expect("apply fixes");
        assert!(response.ok);
        assert_eq!(response.mode, "apply");
        assert_eq!(
            response.files_changed,
            vec![file.to_string_lossy().to_string()]
        );

        let rewritten = std::fs::read_to_string(&file).expect("read rewritten");
        assert!(rewritten.contains("let x = 1;"));
    }
}
