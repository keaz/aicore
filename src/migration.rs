use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::ir::{migrate_json_to_current, CURRENT_IR_SCHEMA_VERSION};
use crate::lexer::{self, TokenKind};

pub const MIGRATION_REPORT_SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReport {
    pub schema_version: String,
    pub root: String,
    pub dry_run: bool,
    pub files_scanned: usize,
    pub files_changed: usize,
    pub edits_planned: usize,
    pub high_risk_edits: usize,
    pub warnings: Vec<String>,
    pub files: Vec<FileMigration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMigration {
    pub path: String,
    pub file_kind: String,
    pub changed: bool,
    pub highest_risk: Option<String>,
    pub edits: Vec<MigrationEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationEdit {
    pub rule: String,
    pub risk: String,
    pub description: String,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone)]
struct PendingEdit {
    start: usize,
    end: usize,
    replacement: String,
    rule: String,
    risk: String,
    description: String,
}

pub fn run_migration(path: &Path, dry_run: bool) -> anyhow::Result<MigrationReport> {
    if !path.exists() {
        anyhow::bail!("migration path does not exist: {}", path.display());
    }

    if path.is_file() && !is_supported_file(path) {
        anyhow::bail!(
            "unsupported migration input file `{}`; expected .aic or .json",
            path.display()
        );
    }

    let root = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    let files = collect_candidates(path)?;
    let mut warnings = Vec::new();
    let mut migrated_files = Vec::new();

    for file in files {
        let rel = stable_rel_path(&root, &file);
        if file.extension().and_then(|ext| ext.to_str()) == Some("aic") {
            let source = fs::read_to_string(&file)
                .with_context(|| format!("failed to read source {}", file.display()))?;
            let (rewritten, edits) = migrate_source_text(&source);
            let changed = !edits.is_empty();
            if changed && !dry_run {
                fs::write(&file, rewritten).with_context(|| {
                    format!("failed to write migrated source {}", file.display())
                })?;
            }
            migrated_files.push(FileMigration {
                path: rel,
                file_kind: "source".to_string(),
                changed,
                highest_risk: highest_risk(&edits),
                edits,
            });
            continue;
        }

        let raw = fs::read_to_string(&file)
            .with_context(|| format!("failed to read JSON file {}", file.display()))?;

        let strict_json = path.is_file();
        match migrate_ir_json_text(&raw) {
            Ok(Some((rewritten, edit))) => {
                if !dry_run {
                    fs::write(&file, rewritten).with_context(|| {
                        format!("failed to write migrated IR JSON {}", file.display())
                    })?;
                }
                migrated_files.push(FileMigration {
                    path: rel,
                    file_kind: "ir-json".to_string(),
                    changed: true,
                    highest_risk: Some(edit.risk.clone()),
                    edits: vec![edit],
                });
            }
            Ok(None) => {
                migrated_files.push(FileMigration {
                    path: rel,
                    file_kind: "ir-json".to_string(),
                    changed: false,
                    highest_risk: None,
                    edits: Vec::new(),
                });
            }
            Err(err) => {
                if strict_json {
                    return Err(err).with_context(|| {
                        format!("failed to migrate IR JSON file {}", file.display())
                    });
                }
                warnings.push(format!("skipped {}: {}", rel, err));
                migrated_files.push(FileMigration {
                    path: rel,
                    file_kind: "ir-json".to_string(),
                    changed: false,
                    highest_risk: None,
                    edits: Vec::new(),
                });
            }
        }
    }

    migrated_files.sort_by(|a, b| a.path.cmp(&b.path).then(a.file_kind.cmp(&b.file_kind)));
    warnings.sort();

    let files_scanned = migrated_files.len();
    let files_changed = migrated_files.iter().filter(|f| f.changed).count();
    let edits_planned = migrated_files.iter().map(|f| f.edits.len()).sum::<usize>();
    let high_risk_edits = migrated_files
        .iter()
        .flat_map(|f| f.edits.iter())
        .filter(|edit| edit.risk == "high")
        .count();

    Ok(MigrationReport {
        schema_version: MIGRATION_REPORT_SCHEMA_VERSION.to_string(),
        root: path.display().to_string(),
        dry_run,
        files_scanned,
        files_changed,
        edits_planned,
        high_risk_edits,
        warnings,
        files: migrated_files,
    })
}

pub fn write_report(path: &Path, report: &MigrationReport) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create report parent {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(report)?;
    fs::write(path, json).with_context(|| format!("failed to write report {}", path.display()))?;
    Ok(())
}

fn collect_candidates(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut out = Vec::new();
    collect_candidates_recursive(path, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_candidates_recursive(root: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(root)
        .with_context(|| format!("failed to read directory {}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to enumerate directory {}", root.display()))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            collect_candidates_recursive(&path, out)?;
            continue;
        }

        if is_supported_file(&path) {
            if path.extension().and_then(|ext| ext.to_str()) == Some("json")
                && !is_ir_candidate_json(&path)
            {
                continue;
            }
            out.push(path);
        }
    }
    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | "target" | "node_modules")
    )
}

fn is_supported_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("aic" | "json")
    )
}

fn is_ir_candidate_json(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower == "ir.json"
        || lower.ends_with(".ir.json")
        || lower.ends_with("_ir.json")
        || lower.contains("legacy-ir")
        || lower.contains("legacy_ir")
}

fn migrate_source_text(source: &str) -> (String, Vec<MigrationEdit>) {
    let (tokens, _) = lexer::lex(source, "<migration>");
    let mut pending = Vec::new();

    for window in tokens.windows(6) {
        if matches!(
            (&window[0].kind, &window[1].kind, &window[2].kind, &window[3].kind, &window[4].kind, &window[5].kind),
            (
                TokenKind::Ident(std_name),
                TokenKind::Dot,
                TokenKind::Ident(time_name),
                TokenKind::Dot,
                TokenKind::Ident(now_name),
                TokenKind::LParen
            ) if std_name == "std" && time_name == "time" && now_name == "now"
        ) {
            pending.push(PendingEdit {
                start: window[4].span.start,
                end: window[4].span.end,
                replacement: "now_ms".to_string(),
                rule: "MIG001".to_string(),
                risk: "low".to_string(),
                description: "replace deprecated std.time.now call with std.time.now_ms"
                    .to_string(),
            });
        }
    }

    for token in &tokens {
        if matches!(token.kind, TokenKind::KwNull) {
            pending.push(PendingEdit {
                start: token.span.start,
                end: token.span.end,
                replacement: "None()".to_string(),
                rule: "MIG002".to_string(),
                risk: "high".to_string(),
                description: "replace legacy null literal with Option form None()".to_string(),
            });
        }
    }

    pending.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    pending.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.rule == b.rule);

    let mut rewritten = source.to_string();
    for edit in pending.iter().rev() {
        rewritten.replace_range(edit.start..edit.end, &edit.replacement);
    }

    let edits = pending
        .into_iter()
        .map(|edit| {
            let before = source
                .get(edit.start..edit.end)
                .unwrap_or_default()
                .to_string();
            let (start_line, start_col) = offset_to_line_col(source, edit.start);
            let (end_line, end_col) = offset_to_line_col(source, edit.end);
            MigrationEdit {
                rule: edit.rule,
                risk: edit.risk,
                description: edit.description,
                start_line,
                start_col,
                end_line,
                end_col,
                before,
                after: edit.replacement,
            }
        })
        .collect::<Vec<_>>();

    (rewritten, edits)
}

fn migrate_ir_json_text(raw: &str) -> anyhow::Result<Option<(String, MigrationEdit)>> {
    let input_value: serde_json::Value = serde_json::from_str(raw)
        .context("input is not valid JSON; expected an IR JSON document")?;
    let migrated = migrate_json_to_current(raw)?;
    let migrated_value = serde_json::to_value(&migrated)?;

    if migrated_value == input_value {
        return Ok(None);
    }

    if migrated.schema_version != CURRENT_IR_SCHEMA_VERSION {
        anyhow::bail!(
            "migrated schema_version {} does not match current {}",
            migrated.schema_version,
            CURRENT_IR_SCHEMA_VERSION
        );
    }

    let from_schema = input_value
        .get("schema_version")
        .and_then(|value| value.as_u64())
        .map(|value| value.to_string())
        .unwrap_or_else(|| "missing".to_string());
    let to_schema = migrated.schema_version.to_string();

    let (end_line, end_col) = offset_to_line_col(raw, raw.len());
    let edit = MigrationEdit {
        rule: "MIG003".to_string(),
        risk: "medium".to_string(),
        description: "migrate IR JSON schema_version to the current compiler schema".to_string(),
        start_line: 1,
        start_col: 1,
        end_line,
        end_col,
        before: format!("schema_version={from_schema}"),
        after: format!("schema_version={to_schema}"),
    };

    Ok(Some((serde_json::to_string_pretty(&migrated)?, edit)))
}

fn highest_risk(edits: &[MigrationEdit]) -> Option<String> {
    edits
        .iter()
        .map(|edit| edit.risk.as_str())
        .max_by_key(|risk| risk_rank(risk))
        .map(ToString::to_string)
}

fn risk_rank(risk: &str) -> u8 {
    match risk {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn stable_rel_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.display().to_string().replace('\\', "/")
}

fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (idx, ch) in source.char_indices() {
        if idx >= offset {
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

#[cfg(test)]
mod tests {
    use super::{migrate_ir_json_text, migrate_source_text};

    #[test]
    fn source_migration_rewrites_known_breakages() {
        let source = r#"module demo.main;
import std.time;
fn main() -> Int {
    let a = std.time.now();
    let b = null;
    a + 1
}
"#;
        let (rewritten, edits) = migrate_source_text(source);
        assert!(rewritten.contains("std.time.now_ms()"));
        assert!(rewritten.contains("let b = None();"));
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().any(|edit| edit.rule == "MIG001"));
        assert!(edits.iter().any(|edit| edit.rule == "MIG002"));
    }

    #[test]
    fn ir_migration_updates_legacy_schema() {
        let legacy = r#"{
  "module": null,
  "imports": [],
  "items": [],
  "symbols": [],
  "types": [],
  "span": { "start": 0, "end": 0 }
}"#;
        let migrated = migrate_ir_json_text(legacy).expect("migrate");
        let (rewritten, edit) = migrated.expect("migration should produce changes");
        assert!(rewritten.contains("\"schema_version\""));
        assert_eq!(edit.rule, "MIG003");
        assert_eq!(edit.risk, "medium");
    }
}
