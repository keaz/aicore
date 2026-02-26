use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::codegen::{compile_with_clang, emit_llvm};
use crate::contracts::lower_runtime_asserts;
use crate::driver::{has_errors, run_frontend};
use crate::formatter::format_program;
use crate::ir_builder;
use crate::parser;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessMode {
    All,
    RunPass,
    CompileFail,
    Golden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldenMode {
    Legacy,
    Update,
    Check,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCase {
    pub category: String,
    pub file: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessReport {
    pub root: String,
    pub mode: HarnessMode,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub by_category: BTreeMap<String, usize>,
    pub cases: Vec<HarnessCase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReplayMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayMetadata {
    pub replay_id: String,
    pub artifact_path: String,
    pub seed: u64,
    pub time_ms: String,
    pub mock_no_real_io: bool,
    pub mock_io_capture: bool,
    pub trace_id: Option<String>,
    pub generated_at_ms: u64,
}

pub fn run_harness(root: &Path, mode: HarnessMode) -> anyhow::Result<HarnessReport> {
    run_harness_with_golden_mode(root, mode, GoldenMode::Legacy)
}

pub fn run_harness_with_golden_mode(
    root: &Path,
    mode: HarnessMode,
    golden_mode: GoldenMode,
) -> anyhow::Result<HarnessReport> {
    let root = root.to_path_buf();
    let mut files = Vec::new();
    collect_aic_files(&root, &mut files)?;
    files.sort();

    let mut report = HarnessReport {
        root: root.to_string_lossy().to_string(),
        mode,
        total: 0,
        passed: 0,
        failed: 0,
        by_category: BTreeMap::new(),
        cases: Vec::new(),
        replay: None,
    };

    for file in files {
        let category = classify(&file);
        let Some(category) = category else {
            continue;
        };
        if !mode_matches(mode, &category) {
            continue;
        }

        let result = match category.as_str() {
            "run-pass" => run_pass_case(&file),
            "compile-fail" => compile_fail_case(&file),
            "golden" => golden_case(&file, golden_mode),
            _ => continue,
        };

        let (passed, details) = match result {
            Ok(msg) => (true, msg),
            Err(err) => (false, err.to_string()),
        };

        report.total += 1;
        *report.by_category.entry(category.clone()).or_default() += 1;
        if passed {
            report.passed += 1;
        } else {
            report.failed += 1;
        }
        report.cases.push(HarnessCase {
            category,
            file: file.to_string_lossy().to_string(),
            passed,
            details,
        });
    }

    Ok(report)
}

fn mode_matches(mode: HarnessMode, category: &str) -> bool {
    match mode {
        HarnessMode::All => true,
        HarnessMode::RunPass => category == "run-pass",
        HarnessMode::CompileFail => category == "compile-fail",
        HarnessMode::Golden => category == "golden",
    }
}

fn classify(path: &Path) -> Option<String> {
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if name == "run-pass" || name == "compile-fail" || name == "golden" {
            return Some(name.to_string());
        }
    }
    None
}

fn run_pass_case(path: &Path) -> anyhow::Result<String> {
    let source = fs::read_to_string(path)?;
    let expected = parse_expect_line(&source, "// expect:")
        .ok_or_else(|| anyhow::anyhow!("missing `// expect:` in {}", path.display()))?;

    let front = run_frontend(path)?;
    if has_errors(&front.diagnostics) {
        anyhow::bail!(
            "expected run-pass but got diagnostics: {:#?}",
            front.diagnostics
        )
    }

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &path.to_string_lossy())
        .map_err(|diags| anyhow::anyhow!("llvm generation failed: {:#?}", diags))?;

    let tmp = std::env::temp_dir().join(format!(
        "aicore-harness-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir_all(&tmp)?;
    let exe = tmp.join("run-pass-bin");
    compile_with_clang(&llvm.llvm_ir, &exe, &tmp)?;

    let mut command = Command::new(&exe);
    command.env("AIC_TEST_MODE", "1");
    if std::env::var_os("AIC_TEST_SEED").is_none() {
        command.env("AIC_TEST_SEED", "0");
    }
    if std::env::var_os("AIC_TEST_TIME_MS").is_none() {
        command.env("AIC_TEST_TIME_MS", "1767225600000");
    }
    let output = command.output()?;
    let _ = fs::remove_dir_all(&tmp);
    if !output.status.success() {
        anyhow::bail!(
            "program exited with status {:?}",
            output.status.code().unwrap_or_default()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let got = stdout
        .trim_end()
        .lines()
        .last()
        .unwrap_or_default()
        .to_string();
    if got != expected {
        anyhow::bail!("expected output `{expected}`, got `{got}`")
    }

    Ok(format!("output={expected}"))
}

fn compile_fail_case(path: &Path) -> anyhow::Result<String> {
    let source = fs::read_to_string(path)?;
    let expected_codes = parse_expect_codes(&source);

    let front = run_frontend(path)?;
    if !has_errors(&front.diagnostics) {
        anyhow::bail!("expected compile-fail but file typechecked")
    }

    if expected_codes.is_empty() {
        return Ok("diagnostics observed".to_string());
    }

    let observed = front
        .diagnostics
        .iter()
        .map(|d| d.code.clone())
        .collect::<Vec<_>>();

    for code in &expected_codes {
        if !observed.iter().any(|d| d == code) {
            anyhow::bail!("missing expected diagnostic {code}; observed={observed:?}")
        }
    }

    Ok(format!("matched diagnostics={}", expected_codes.join(",")))
}

fn golden_case(path: &Path, mode: GoldenMode) -> anyhow::Result<String> {
    let source = fs::read_to_string(path)?;
    let (program, parse_diags) = parser::parse(&source, &path.to_string_lossy());
    if parse_diags.iter().any(|d| d.is_error()) {
        anyhow::bail!("golden parse failed: {:#?}", parse_diags)
    }

    let Some(program) = program else {
        anyhow::bail!("golden parse returned no AST")
    };

    let ir = ir_builder::build(&program);
    let formatted = format_program(&ir);

    let actual = normalize_source(&formatted);
    match mode {
        GoldenMode::Legacy => {
            if normalize_source(&source) != actual {
                anyhow::bail!("golden formatting mismatch")
            }
            Ok("format-stable".to_string())
        }
        GoldenMode::Update => {
            let snapshot_path = golden_snapshot_path(path);
            fs::write(&snapshot_path, &actual)?;
            Ok(format!("snapshot-updated={}", snapshot_path.display()))
        }
        GoldenMode::Check => {
            let snapshot_path = golden_snapshot_path(path);
            let expected = fs::read_to_string(&snapshot_path).map_err(|err| {
                anyhow::anyhow!(
                    "missing golden snapshot {} for {} (run `aic test {} --mode golden --update-golden`) ({err})",
                    snapshot_path.display(),
                    path.display(),
                    path.parent().unwrap_or_else(|| Path::new(".")).display()
                )
            })?;
            let expected = normalize_source(&expected);
            if expected != actual {
                anyhow::bail!(
                    "golden snapshot mismatch for {}\n  snapshot: {}\n{}",
                    path.display(),
                    snapshot_path.display(),
                    render_golden_diff(&expected, &actual)
                );
            }
            Ok(format!("snapshot-match={}", snapshot_path.display()))
        }
    }
}

fn parse_expect_line(source: &str, marker: &str) -> Option<String> {
    source
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(marker).map(|s| s.trim().to_string()))
}

fn parse_expect_codes(source: &str) -> Vec<String> {
    source
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            line.strip_prefix("// expect-error:")
                .map(|s| s.trim().to_string())
        })
        .collect()
}

fn golden_snapshot_path(path: &Path) -> PathBuf {
    let mut snapshot_name = OsString::from(path.file_name().unwrap_or_default());
    snapshot_name.push(".golden");
    path.with_file_name(snapshot_name)
}

fn render_golden_diff(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let total = expected_lines.len().max(actual_lines.len());

    let mut diff = String::from("--- expected\n+++ actual\n");
    for line_index in 0..total {
        let expected_line = expected_lines.get(line_index).copied().unwrap_or("<EOF>");
        let actual_line = actual_lines.get(line_index).copied().unwrap_or("<EOF>");
        if expected_line == actual_line {
            continue;
        }
        diff.push_str(&format!(
            "@@ line {} @@\n- {}\n+ {}\n",
            line_index + 1,
            expected_line,
            actual_line
        ));
    }
    diff
}

fn normalize_source(source: &str) -> String {
    let mut normalized = source.replace("\r\n", "\n");
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn collect_aic_files(root: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        if path.is_dir() {
            if name == ".git" || name == "target" || name == ".aic-cache" {
                continue;
            }
            collect_aic_files(&path, out)?;
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) == Some("aic") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{run_harness, HarnessMode};

    #[test]
    fn harness_discovers_and_runs_all_categories() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        fs::create_dir_all(root.join("run-pass")).expect("mkdir run-pass");
        fs::create_dir_all(root.join("compile-fail")).expect("mkdir compile-fail");
        fs::create_dir_all(root.join("golden")).expect("mkdir golden");

        fs::write(
            root.join("run-pass/ok.aic"),
            "// expect:\nfn main() -> Int {\n    0\n}\n",
        )
        .expect("write run pass");

        fs::write(
            root.join("compile-fail/bad.aic"),
            "// expect-error: E2001\nimport std.io;\n\nfn io_side_effect() -> Unit effects { io } {\n  print_int(1)\n}\n\nfn main() -> Int {\n  io_side_effect();\n  0\n}\n",
        )
        .expect("write compile fail");

        fs::write(
            root.join("golden/fmt.aic"),
            "fn main() -> Int {\n    1\n}\n",
        )
        .expect("write golden");

        let report = run_harness(root, HarnessMode::All).expect("run harness");
        assert_eq!(report.total, 3);
        assert_eq!(report.failed, 0, "report={:#?}", report);
    }

    #[test]
    fn harness_run_pass_uses_deterministic_test_mode_seed_and_time() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("run-pass")).expect("mkdir run-pass");

        fs::write(
            root.join("run-pass/deterministic_time_rand.aic"),
            concat!(
                "// expect: 42\n",
                "import std.io;\n",
                "import std.time;\n",
                "import std.rand;\n",
                "\n",
                "fn main() -> Int effects { io, time, rand } capabilities { io, time, rand } {\n",
                "    let now = now_ms();\n",
                "    let first = random_int();\n",
                "    seed(0);\n",
                "    let replay = random_int();\n",
                "    if now == 1767225600000 && first == replay {\n",
                "        print_int(42);\n",
                "    } else {\n",
                "        print_int(0);\n",
                "    };\n",
                "    0\n",
                "}\n",
            ),
        )
        .expect("write deterministic run-pass case");

        let report = run_harness(root, HarnessMode::RunPass).expect("run harness");
        assert_eq!(report.total, 1);
        assert_eq!(report.failed, 0, "report={:#?}", report);
    }
}
