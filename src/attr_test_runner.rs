use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::codegen::{compile_with_clang, emit_llvm};
use crate::contracts::lower_runtime_asserts;
use crate::driver::{has_errors, run_frontend};
use crate::test_harness::{HarnessCase, HarnessMode, HarnessReport};

const ATTRIBUTE_CATEGORY: &str = "attribute-test";

static ATTR_TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
struct AttributeTestCase {
    file: PathBuf,
    name: String,
    should_panic: bool,
}

pub fn run_attribute_tests(
    root: &Path,
    filter: Option<&str>,
    seed: u64,
) -> anyhow::Result<HarnessReport> {
    let mut files = Vec::new();
    collect_aic_files(root, &mut files)?;
    files.sort();

    let mut discovered = Vec::new();
    for file in files {
        if is_fixture_harness_path(&file) {
            continue;
        }
        let source = fs::read_to_string(&file)?;
        discovered.extend(discover_attribute_tests(&source, &file));
    }

    if let Some(raw_filter) = filter {
        let needle = raw_filter.trim().to_ascii_lowercase();
        if !needle.is_empty() {
            discovered.retain(|case| {
                case.name.to_ascii_lowercase().contains(&needle)
                    || case
                        .file
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .contains(&needle)
            });
        }
    }

    let mut report = HarnessReport {
        root: root.to_string_lossy().to_string(),
        mode: HarnessMode::All,
        total: 0,
        passed: 0,
        failed: 0,
        by_category: BTreeMap::new(),
        cases: Vec::new(),
    };

    if discovered.is_empty() {
        return Ok(report);
    }

    let worker_count = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .min(discovered.len())
        .max(1);

    let queue = Arc::new(Mutex::new(VecDeque::from(discovered)));
    let results = Arc::new(Mutex::new(Vec::<(AttributeTestCase, bool, String)>::new()));

    let mut handles = Vec::new();
    for _ in 0..worker_count {
        let queue = Arc::clone(&queue);
        let results = Arc::clone(&results);
        handles.push(std::thread::spawn(move || loop {
            let next = {
                let mut lock = queue.lock().expect("queue lock poisoned");
                lock.pop_front()
            };

            let Some(case) = next else {
                break;
            };

            let (passed, details) = match run_attribute_case(&case, seed) {
                Ok(details) => (true, details),
                Err(err) => (false, err.to_string()),
            };

            let mut lock = results.lock().expect("results lock poisoned");
            lock.push((case, passed, details));
        }));
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("attribute test worker thread panicked"))?;
    }

    let mut finished = {
        let mut lock = results.lock().expect("results lock poisoned");
        std::mem::take(&mut *lock)
    };
    finished.sort_by(|lhs, rhs| {
        lhs.0
            .file
            .cmp(&rhs.0.file)
            .then(lhs.0.name.cmp(&rhs.0.name))
    });

    for (case, passed, details) in finished {
        report.total += 1;
        *report
            .by_category
            .entry(ATTRIBUTE_CATEGORY.to_string())
            .or_default() += 1;

        if passed {
            report.passed += 1;
        } else {
            report.failed += 1;
        }

        report.cases.push(HarnessCase {
            category: ATTRIBUTE_CATEGORY.to_string(),
            file: format!("{}::{}", case.file.to_string_lossy(), case.name),
            passed,
            details,
        });
    }

    Ok(report)
}

fn discover_attribute_tests(source: &str, file: &Path) -> Vec<AttributeTestCase> {
    let mut out = Vec::new();
    let mut pending_test = false;
    let mut pending_should_panic = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[test]") {
            pending_test = true;
            continue;
        }

        if trimmed.starts_with("#[should_panic]") {
            if pending_test {
                pending_should_panic = true;
            }
            continue;
        }

        if pending_test {
            if let Some(name) = parse_fn_name(trimmed) {
                out.push(AttributeTestCase {
                    file: file.to_path_buf(),
                    name,
                    should_panic: pending_should_panic,
                });
                pending_test = false;
                pending_should_panic = false;
                continue;
            }

            // Attribute block must attach to a function declaration.
            if !trimmed.is_empty() && !trimmed.starts_with("//") {
                pending_test = false;
                pending_should_panic = false;
            }
        }
    }

    out
}

fn parse_fn_name(trimmed_line: &str) -> Option<String> {
    let line = if let Some(rest) = trimmed_line.strip_prefix("pub ") {
        rest
    } else {
        trimmed_line
    };

    let rest = line.strip_prefix("fn ")?;
    let name = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();

    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn run_attribute_case(case: &AttributeTestCase, seed: u64) -> anyhow::Result<String> {
    let source = fs::read_to_string(&case.file)?;
    let transformed = transform_source_for_test(&source, &case.name)?;

    let temp_dir = std::env::temp_dir().join(format!(
        "aicore-attr-test-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos(),
        ATTR_TEST_COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(&temp_dir)?;

    let test_file = temp_dir.join("test_case.aic");
    fs::write(&test_file, transformed)?;

    let front = run_frontend(&test_file)?;
    if has_errors(&front.diagnostics) {
        let _ = fs::remove_dir_all(&temp_dir);
        anyhow::bail!("attribute test typecheck failed: {:#?}", front.diagnostics);
    }

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &test_file.to_string_lossy())
        .map_err(|diags| anyhow::anyhow!("llvm generation failed: {:#?}", diags))?;

    let exe = temp_dir.join("attr-test-bin");
    compile_with_clang(&llvm.llvm_ir, &exe, &temp_dir)?;

    let mut command = Command::new(&exe);
    command.env("AIC_TEST_MODE", "1");
    command.env("AIC_TEST_SEED", seed.to_string());
    if std::env::var_os("AIC_TEST_TIME_MS").is_none() {
        command.env("AIC_TEST_TIME_MS", "1767225600000");
    }
    if std::env::var_os("AIC_TEST_NO_REAL_IO").is_none() {
        command.env("AIC_TEST_NO_REAL_IO", "1");
    }
    if std::env::var_os("AIC_TEST_IO_CAPTURE").is_none() {
        command.env("AIC_TEST_IO_CAPTURE", "1");
    }

    let output = command.output()?;
    let _ = fs::remove_dir_all(&temp_dir);

    if case.should_panic {
        if output.status.success() {
            anyhow::bail!(
                "expected panic but test exited successfully (status={:?})",
                output.status.code()
            );
        }
        return Ok("panic-observed".to_string());
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!(
            "test exited with status {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            stdout,
            stderr
        );
    }

    Ok("passed".to_string())
}

fn transform_source_for_test(source: &str, test_name: &str) -> anyhow::Result<String> {
    let test_names = discover_attribute_tests(source, Path::new("memory"))
        .into_iter()
        .map(|case| case.name)
        .collect::<Vec<_>>();
    if !test_names.iter().any(|name| name == test_name) {
        anyhow::bail!("did not find test function '{}' in source", test_name);
    }

    let stripped = strip_test_attributes(source);
    if stripped.contains("fn main(") {
        anyhow::bail!(
            "attribute tests must not define `main`; test runner injects its own entrypoint"
        );
    }

    let patched = add_io_effect_to_test_fns(&stripped, &test_names)?;
    let with_import = ensure_std_io_import(&patched);
    let rewritten = rewrite_assert_calls(&with_import);

    Ok(format!(
        "{}\n{}\nfn main() -> Int effects {{ io, fs, net, time, rand, env, proc, concurrency }} {{\n    {}();\n    0\n}}\n",
        rewritten, ASSERT_HELPERS, test_name
    ))
}

fn strip_test_attributes(source: &str) -> String {
    let mut out = String::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[test]") || trimmed.starts_with("#[should_panic]") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn add_io_effect_to_test_fns(source: &str, test_names: &[String]) -> anyhow::Result<String> {
    let mut out = String::new();
    let mut patched_count = 0usize;

    for line in source.lines() {
        let trimmed = line.trim_start();
        let indent_len = line.len() - trimmed.len();
        let indent = &line[..indent_len];
        if let Some(name) = parse_fn_name(trimmed) {
            if test_names.iter().any(|test_name| test_name == &name) {
                let brace_idx = trimmed.rfind('{').ok_or_else(|| {
                    anyhow::anyhow!(
                        "test function '{}' must keep '{{' on the same line as the signature",
                        name
                    )
                })?;

                let mut prefix = trimmed[..brace_idx].trim_end().to_string();
                if !prefix.contains("->") {
                    prefix.push_str(" -> ()");
                }
                if !prefix.contains("effects {") {
                    prefix.push_str(" effects { io }");
                }
                let suffix = &trimmed[brace_idx..];

                out.push_str(indent);
                out.push_str(&prefix);
                out.push_str(suffix);
                out.push('\n');
                patched_count += 1;
                continue;
            }
        }

        out.push_str(line);
        out.push('\n');
    }
    if patched_count == 0 {
        anyhow::bail!("did not find any discovered test functions in transformed source");
    }

    Ok(out)
}
fn ensure_std_io_import(source: &str) -> String {
    if source.lines().any(|line| line.trim() == "import std.io;") {
        return source.to_string();
    }

    let mut out = String::new();
    let mut inserted = false;
    for line in source.lines() {
        out.push_str(line);
        out.push('\n');
        if !inserted && line.trim_start().starts_with("module ") {
            out.push_str("import std.io;\n");
            inserted = true;
        }
    }

    if !inserted {
        format!("import std.io;\n{}", out)
    } else {
        out
    }
}

fn rewrite_assert_calls(source: &str) -> String {
    source
        .replace("assert_eq(", "test_assert_eq_internal(")
        .replace("assert_ne(", "test_assert_ne_internal(")
        .replace("assert(", "test_assert_internal(")
}

fn is_fixture_harness_path(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str();
        name == OsStr::new("run-pass")
            || name == OsStr::new("compile-fail")
            || name == OsStr::new("golden")
    })
}

fn collect_aic_files(root: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();

        if path.is_dir() {
            if name == ".git" || name == "target" || name == ".aic-cache" {
                continue;
            }
            collect_aic_files(&path, out)?;
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) == Some("aic") {
            out.push(path);
        }
    }

    Ok(())
}

const ASSERT_HELPERS: &str = r#"
fn test_assert_internal(cond: Bool) -> () effects { io } {
    if cond {
        ()
    } else {
        panic("assert failed")
    }
}

fn test_assert_eq_internal[T](left: T, right: T) -> () effects { io } {
    if left == right {
        ()
    } else {
        panic("assert_eq failed: left != right")
    }
}

fn test_assert_ne_internal[T](left: T, right: T) -> () effects { io } {
    if left != right {
        ()
    } else {
        panic("assert_ne failed: left == right")
    }
}
"#;

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{discover_attribute_tests, run_attribute_tests, transform_source_for_test};

    #[test]
    fn discovers_test_and_should_panic_attributes() {
        let source = r#"
#[test]
fn test_addition() -> () { assert_eq(1 + 1, 2); }

#[test]
#[should_panic]
fn test_fails() -> () { assert_eq(1, 2); }
"#;
        let discovered = discover_attribute_tests(source, std::path::Path::new("tests.aic"));
        assert_eq!(discovered.len(), 2);
        assert_eq!(discovered[0].name, "test_addition");
        assert!(!discovered[0].should_panic);
        assert_eq!(discovered[1].name, "test_fails");
        assert!(discovered[1].should_panic);
    }

    #[test]
    fn transform_rewrites_assert_calls_and_injects_main() {
        let source = r#"
#[test]
fn test_addition() -> () {
    assert_eq(1 + 1, 2);
    assert_ne(1, 2);
    assert(true);
}
"#;

        let transformed = transform_source_for_test(source, "test_addition").expect("transform");
        assert!(!transformed.contains("#[test]"));
        assert!(transformed.contains("test_assert_eq_internal("));
        assert!(transformed.contains("test_assert_ne_internal("));
        assert!(transformed.contains("test_assert_internal(true)"));
        assert!(transformed.contains(
            "fn main() -> Int effects { io, fs, net, time, rand, env, proc, concurrency }"
        ));
    }

    #[test]
    fn runs_attribute_tests_with_filter_and_should_panic() {
        let dir = tempdir().expect("tempdir");
        let test_file = dir.path().join("tests.aic");
        fs::write(
            &test_file,
            r#"
#[test]
fn test_addition() -> () {
    assert_eq(1 + 1, 2);
    assert(true);
    assert_ne(1, 2);
}

#[test]
#[should_panic]
fn test_division_by_zero() -> () {
    assert_eq(1, 2);
}
"#,
        )
        .expect("write tests");

        let all = run_attribute_tests(dir.path(), None, 0).expect("run all");
        assert_eq!(all.total, 2, "report={all:#?}");
        assert_eq!(all.failed, 0, "report={all:#?}");

        let filtered = run_attribute_tests(dir.path(), Some("addition"), 0).expect("run filtered");
        assert_eq!(filtered.total, 1, "report={filtered:#?}");
        assert_eq!(filtered.failed, 0, "report={filtered:#?}");
    }
}
