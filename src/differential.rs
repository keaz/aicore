use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::diagnostics::Diagnostic;
use crate::formatter::format_program;
use crate::ir;
use crate::ir_builder;
use crate::parser;
use crate::resolver;
use crate::typecheck;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DifferentialCaseResult {
    pub file: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DifferentialReport {
    pub total: usize,
    pub matched: usize,
    pub diverged: usize,
    pub cases: Vec<DifferentialCaseResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SemanticSnapshot {
    ir: Value,
    diagnostics: Vec<SemanticDiagnostic>,
    function_effect_usage: BTreeMap<String, Vec<String>>,
    generic_instantiations: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SemanticDiagnostic {
    code: String,
    severity: String,
    message: String,
}

pub fn run_roundtrip_corpus(path: &Path) -> anyhow::Result<DifferentialReport> {
    let mut files = collect_aic_files(path)?;
    files.sort();

    let mut report = DifferentialReport {
        total: 0,
        matched: 0,
        diverged: 0,
        cases: Vec::new(),
    };

    for file in files {
        report.total += 1;
        let result = run_roundtrip_file(&file)?;
        if result.passed {
            report.matched += 1;
        } else {
            report.diverged += 1;
        }
        report.cases.push(result);
    }

    Ok(report)
}

pub fn run_roundtrip_file(path: &Path) -> anyhow::Result<DifferentialCaseResult> {
    let source = fs::read_to_string(path)?;

    let (program1, parse_diags1) = parser::parse(&source, &path.to_string_lossy());
    if parse_diags1.iter().any(Diagnostic::is_error) {
        return Ok(DifferentialCaseResult {
            file: path.to_string_lossy().to_string(),
            passed: false,
            details: format!("source parse failed: {parse_diags1:#?}"),
        });
    }

    let Some(program1) = program1 else {
        return Ok(DifferentialCaseResult {
            file: path.to_string_lossy().to_string(),
            passed: false,
            details: "source parse returned no AST".to_string(),
        });
    };

    let ir1 = ir_builder::build(&program1);
    let formatted = format_program(&ir1);

    let (program2, parse_diags2) = parser::parse(&formatted, &path.to_string_lossy());
    if parse_diags2.iter().any(Diagnostic::is_error) {
        return Ok(DifferentialCaseResult {
            file: path.to_string_lossy().to_string(),
            passed: false,
            details: format!("roundtrip parse failed: {parse_diags2:#?}"),
        });
    }

    let Some(program2) = program2 else {
        return Ok(DifferentialCaseResult {
            file: path.to_string_lossy().to_string(),
            passed: false,
            details: "roundtrip parse returned no AST".to_string(),
        });
    };

    let ir2 = ir_builder::build(&program2);

    let snapshot1 = build_semantic_snapshot(&ir1, &path.to_string_lossy())?;
    let snapshot2 = build_semantic_snapshot(&ir2, &path.to_string_lossy())?;

    if snapshot1 == snapshot2 {
        return Ok(DifferentialCaseResult {
            file: path.to_string_lossy().to_string(),
            passed: true,
            details: "semantic-equivalent".to_string(),
        });
    }

    let left = serde_json::to_string_pretty(&snapshot1)?;
    let right = serde_json::to_string_pretty(&snapshot2)?;
    let detail = format_divergence(&left, &right);

    Ok(DifferentialCaseResult {
        file: path.to_string_lossy().to_string(),
        passed: false,
        details: detail,
    })
}

fn build_semantic_snapshot(program: &ir::Program, file: &str) -> anyhow::Result<SemanticSnapshot> {
    let type_map = program
        .types
        .iter()
        .map(|ty| (u64::from(ty.id.0), ty.repr.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut ir_value = serde_json::to_value(program)?;
    normalize_ir_value(&mut ir_value, &type_map);

    let (resolution, resolve_diags) = resolver::resolve(program, file);
    let typecheck_out = typecheck::check(program, &resolution, file);

    let mut diagnostics = Vec::new();
    for diag in resolve_diags.iter().chain(typecheck_out.diagnostics.iter()) {
        diagnostics.push(SemanticDiagnostic {
            code: diag.code.clone(),
            severity: format!("{:?}", diag.severity).to_lowercase(),
            message: diag.message.clone(),
        });
    }
    diagnostics.sort_by(|a, b| {
        a.code
            .cmp(&b.code)
            .then(a.severity.cmp(&b.severity))
            .then(a.message.cmp(&b.message))
    });

    let mut effect_usage = BTreeMap::new();
    for (name, effects) in &typecheck_out.function_effect_usage {
        let values = effects.iter().cloned().collect::<Vec<_>>();
        effect_usage.insert(name.clone(), values);
    }

    let mut generic_instantiations = serde_json::to_value(&typecheck_out.generic_instantiations)?;
    normalize_generic_instantiations(&mut generic_instantiations);

    Ok(SemanticSnapshot {
        ir: ir_value,
        diagnostics,
        function_effect_usage: effect_usage,
        generic_instantiations,
    })
}

fn normalize_ir_value(value: &mut Value, type_map: &BTreeMap<u64, String>) {
    match value {
        Value::Object(map) => {
            let span_like = map.contains_key("start")
                && map.contains_key("end")
                && map
                    .keys()
                    .all(|key| matches!(key.as_str(), "start" | "end" | "file" | "label"));
            if span_like {
                *value = Value::String("<span>".to_string());
                return;
            }

            map.remove("span");
            map.remove("node");
            map.remove("symbol");
            map.remove("schema_version");
            map.remove("id");

            for key in ["ret_type", "ty", "payload"] {
                if let Some(ty_value) = map.get_mut(key) {
                    remap_type_id_to_repr(ty_value, type_map);
                }
            }

            for nested in map.values_mut() {
                normalize_ir_value(nested, type_map);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_ir_value(item, type_map);
            }
        }
        _ => {}
    }
}

fn remap_type_id_to_repr(value: &mut Value, type_map: &BTreeMap<u64, String>) {
    if value.is_null() {
        return;
    }
    if let Some(id) = value.as_u64() {
        if let Some(repr) = type_map.get(&id) {
            *value = Value::String(repr.clone());
        }
        return;
    }
    if let Value::Object(map) = value {
        if map.len() == 1 {
            if let Some((_, inner)) = map.iter().next() {
                if let Some(id) = inner.as_u64() {
                    if let Some(repr) = type_map.get(&id) {
                        *value = Value::String(repr.clone());
                    }
                }
            }
        }
    }
}

fn normalize_generic_instantiations(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("id");
            map.remove("symbol");
            for nested in map.values_mut() {
                normalize_generic_instantiations(nested);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_generic_instantiations(item);
            }
        }
        _ => {}
    }
}

fn format_divergence(left: &str, right: &str) -> String {
    match first_diff_line(left, right) {
        Some(line) => {
            let left_line = left.lines().nth(line).unwrap_or_default();
            let right_line = right.lines().nth(line).unwrap_or_default();
            format!(
                "diverged at line {}: left=`{}` right=`{}`",
                line + 1,
                left_line.trim(),
                right_line.trim()
            )
        }
        None => "diverged with equivalent line rendering".to_string(),
    }
}

fn first_diff_line(left: &str, right: &str) -> Option<usize> {
    let mut line = 0usize;
    let mut left_iter = left.lines();
    let mut right_iter = right.lines();

    loop {
        match (left_iter.next(), right_iter.next()) {
            (Some(a), Some(b)) if a == b => {
                line += 1;
            }
            (Some(_), Some(_)) => return Some(line),
            (None, None) => return None,
            _ => return Some(line),
        }
    }
}

fn collect_aic_files(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if path.is_file() {
        out.push(path.to_path_buf());
        return Ok(out);
    }

    if !path.exists() {
        return Ok(out);
    }

    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let next = entry.path();
        if next.is_dir() {
            out.extend(collect_aic_files(&next)?);
            continue;
        }
        if next.extension().and_then(|s| s.to_str()) == Some("aic") {
            out.push(next);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::first_diff_line;

    #[test]
    fn reports_first_diff_line() {
        let left = "a\nb\nc\n";
        let right = "a\nb\nd\n";
        assert_eq!(first_diff_line(left, right), Some(2));
    }
}
