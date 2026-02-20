use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::diagnostics::{Diagnostic, Severity};

pub fn diagnostics_to_sarif(diags: &[Diagnostic], tool_name: &str, tool_version: &str) -> Value {
    let rules = collect_rules(diags);
    let mut file_cache: BTreeMap<String, Option<String>> = BTreeMap::new();

    let results = diags
        .iter()
        .map(|diag| sarif_result(diag, &mut file_cache))
        .collect::<Vec<_>>();

    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": tool_name,
                        "version": tool_version,
                        "informationUri": "https://github.com/keaz/aicore",
                        "rules": rules
                    }
                },
                "results": results
            }
        ]
    })
}

fn collect_rules(diags: &[Diagnostic]) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut rules = Vec::new();
    for diag in diags {
        if !seen.insert(diag.code.clone()) {
            continue;
        }
        rules.push(json!({
            "id": diag.code,
            "shortDescription": {
                "text": rule_short_text(diag)
            },
            "helpUri": "https://github.com/keaz/aicore/blob/main/docs/diagnostic-codes.md"
        }));
    }
    rules
}

fn rule_short_text(diag: &Diagnostic) -> String {
    if diag.message.len() <= 120 {
        return diag.message.clone();
    }
    let mut msg = diag.message.chars().take(117).collect::<String>();
    msg.push_str("...");
    msg
}

fn sarif_result(diag: &Diagnostic, file_cache: &mut BTreeMap<String, Option<String>>) -> Value {
    let level = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    };

    let mut message_text = diag.message.clone();
    if !diag.help.is_empty() {
        message_text.push_str("\nhelp:\n");
        for help in &diag.help {
            message_text.push_str("- ");
            message_text.push_str(help);
            message_text.push('\n');
        }
        message_text = message_text.trim_end().to_string();
    }

    let locations = if let Some(span) = diag.spans.first() {
        vec![location_for_span(
            span.file.as_str(),
            span.start,
            span.end,
            file_cache,
        )]
    } else {
        vec![json!({
            "physicalLocation": {
                "artifactLocation": {
                    "uri": "<unknown>"
                },
                "region": {
                    "startLine": 1,
                    "startColumn": 1,
                    "endLine": 1,
                    "endColumn": 1
                }
            }
        })]
    };

    json!({
        "ruleId": diag.code,
        "level": level,
        "message": {
            "text": message_text
        },
        "locations": locations
    })
}

fn location_for_span(
    file: &str,
    start: usize,
    end: usize,
    file_cache: &mut BTreeMap<String, Option<String>>,
) -> Value {
    let source = file_cache
        .entry(file.to_string())
        .or_insert_with(|| fs::read_to_string(file).ok())
        .clone();

    let (start_line, start_col, end_line, end_col) = if let Some(source) = source {
        let (sl, sc) = offset_to_line_col(&source, start);
        let (el, ec) = offset_to_line_col(&source, end.max(start + 1));
        (sl, sc, el, ec)
    } else {
        (1, 1, 1, 1)
    };

    json!({
        "physicalLocation": {
            "artifactLocation": {
                "uri": to_artifact_uri(file),
            },
            "region": {
                "startLine": start_line,
                "startColumn": start_col,
                "endLine": end_line,
                "endColumn": end_col,
                "charOffset": start,
                "charLength": end.saturating_sub(start),
            }
        }
    })
}

fn to_artifact_uri(file: &str) -> String {
    let path = PathBuf::from(file);
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(stripped) = path.strip_prefix(&cwd) {
            return stripped.to_string_lossy().to_string();
        }
    }
    path.to_string_lossy().to_string()
}

fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    let clamped = offset.min(source.len());
    for (idx, ch) in source.char_indices() {
        if idx >= clamped {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub fn write_sarif(path: &Path, sarif: &Value) -> anyhow::Result<()> {
    let text = serde_json::to_string_pretty(sarif)?;
    fs::write(path, format!("{text}\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::diagnostics::Diagnostic;
    use crate::span::Span;

    use super::diagnostics_to_sarif;

    #[test]
    fn sarif_includes_rule_and_location() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("main.aic");
        fs::write(&file, "fn main() -> Int { missing }\n").expect("write");

        let diag = Diagnostic::error(
            "E1201",
            "unknown symbol 'missing'",
            &file.to_string_lossy(),
            Span::new(18, 25),
        );
        let sarif = diagnostics_to_sarif(&[diag], "aic", "0.1.0");

        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(sarif["runs"][0]["results"][0]["ruleId"], "E1201");
        assert!(
            sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]
                ["startLine"]
                .is_number()
        );
        assert!(sarif["runs"][0]["tool"]["driver"]["rules"].is_array());
    }
}
