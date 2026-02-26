use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsStr;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Number, Value};

use crate::codegen::{compile_with_clang, emit_llvm};
use crate::contracts::lower_runtime_asserts;
use crate::driver::{has_errors, run_frontend};
use crate::test_harness::{HarnessCase, HarnessMode, HarnessReport};

const PROPERTY_CATEGORY: &str = "property-test";
const DEFAULT_ITERATIONS: usize = 100;
const MAX_SHRINK_PASSES: usize = 32;

static PROPERTY_TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
struct PropertyCase {
    file: PathBuf,
    name: String,
    iterations: usize,
    params: Vec<PropertyParam>,
    discovery_error: Option<String>,
}

#[derive(Debug, Clone)]
struct PropertyParam {
    name: String,
    ty: PropertyType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PropertyType {
    Int,
    Float,
    Bool,
    String,
    Vec(Box<PropertyType>),
    Option(Box<PropertyType>),
}

#[derive(Debug, Clone)]
struct PropertyBinary {
    temp_dir: PathBuf,
    exe: PathBuf,
}

#[derive(Debug, Clone)]
struct PropertyRunFailure {
    status_code: Option<i32>,
    stdout: String,
    stderr: String,
}

pub fn run_property_tests(
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
        discovered.extend(discover_property_tests(&source, &file));
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
    let results = Arc::new(Mutex::new(Vec::<(PropertyCase, bool, String)>::new()));

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

            let (passed, details) = match run_property_case(&case, seed) {
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
            .map_err(|_| anyhow::anyhow!("property test worker thread panicked"))?;
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
            .entry(PROPERTY_CATEGORY.to_string())
            .or_default() += 1;

        if passed {
            report.passed += 1;
        } else {
            report.failed += 1;
        }

        report.cases.push(HarnessCase {
            category: PROPERTY_CATEGORY.to_string(),
            file: format!("{}::{}", case.file.to_string_lossy(), case.name),
            passed,
            details,
        });
    }

    Ok(report)
}

fn run_property_case(case: &PropertyCase, seed: u64) -> anyhow::Result<String> {
    if let Some(message) = &case.discovery_error {
        anyhow::bail!("property discovery failed: {message}");
    }

    let source = fs::read_to_string(&case.file)?;
    let transformed = transform_source_for_property_case(&source, case)?;
    let binary = compile_property_case(&case.file, &transformed)?;

    let case_seed = compute_case_seed(seed, &case.file, &case.name);
    let mut rng = Lcg64::new(case_seed);

    for iteration in 0..case.iterations {
        let args = generate_argument_object(&case.params, &mut rng);
        let iter_seed = case_seed.wrapping_add(iteration as u64);
        match run_property_binary(&binary.exe, &args, iter_seed) {
            Ok(()) => continue,
            Err(first_failure) => {
                let shrunk = shrink_counterexample(&binary.exe, case, &args, iter_seed)?;
                let counterexample = serde_json::to_string(&args)?;
                let shrunk_value = serde_json::to_string(&shrunk)?;
                let _ = fs::remove_dir_all(&binary.temp_dir);
                anyhow::bail!(
                    "iteration={iteration} seed={iter_seed} counterexample={counterexample} shrunk={shrunk_value} failure={}",
                    format_failure(&first_failure)
                );
            }
        }
    }

    let _ = fs::remove_dir_all(&binary.temp_dir);
    Ok(format!(
        "passed iterations={} seed={}",
        case.iterations, case_seed
    ))
}

fn compile_property_case(_file: &Path, transformed: &str) -> anyhow::Result<PropertyBinary> {
    let temp_dir = std::env::temp_dir().join(format!(
        "aicore-property-test-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos(),
        PROPERTY_TEST_COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(&temp_dir)?;

    let test_file = temp_dir.join("property_case.aic");
    fs::write(&test_file, transformed)?;

    let front = run_frontend(&test_file)?;
    if has_errors(&front.diagnostics) {
        let _ = fs::remove_dir_all(&temp_dir);
        anyhow::bail!("property test typecheck failed: {:#?}", front.diagnostics);
    }

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &test_file.to_string_lossy())
        .map_err(|diags| anyhow::anyhow!("llvm generation failed: {:#?}", diags))?;

    let exe = temp_dir.join("property-test-bin");
    compile_with_clang(&llvm.llvm_ir, &exe, &temp_dir)?;

    Ok(PropertyBinary { temp_dir, exe })
}
fn run_property_binary(exe: &Path, args: &Value, seed: u64) -> Result<(), PropertyRunFailure> {
    let args_json = serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string());

    let mut command = Command::new(exe);
    command.env("AIC_PROP_ARGS_JSON", args_json);
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

    match command.output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(PropertyRunFailure {
            status_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }),
        Err(err) => Err(PropertyRunFailure {
            status_code: None,
            stdout: String::new(),
            stderr: err.to_string(),
        }),
    }
}

fn format_failure(failure: &PropertyRunFailure) -> String {
    let status = failure
        .status_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "signal".to_string());
    let stdout = truncate_text(&failure.stdout, 256);
    let stderr = truncate_text(&failure.stderr, 256);
    format!("status={status} stdout={stdout:?} stderr={stderr:?}")
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        let Some(ch) = chars.next() else {
            return out;
        };
        out.push(ch);
    }
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

fn shrink_counterexample(
    exe: &Path,
    case: &PropertyCase,
    original: &Value,
    seed: u64,
) -> anyhow::Result<Value> {
    let mut current = original.clone();

    for _ in 0..MAX_SHRINK_PASSES {
        let mut changed = false;

        for param in &case.params {
            let current_value = current.get(&param.name).cloned().unwrap_or(Value::Null);
            let candidates = shrink_candidates(&param.ty, &current_value);
            for candidate in candidates {
                let mut next = current.clone();
                if let Some(map) = next.as_object_mut() {
                    map.insert(param.name.clone(), candidate);
                } else {
                    anyhow::bail!("property arguments must be a JSON object");
                }

                if run_property_binary(exe, &next, seed).is_err() {
                    current = next;
                    changed = true;
                    break;
                }
            }
        }

        if !changed {
            break;
        }
    }

    Ok(current)
}

fn shrink_candidates(ty: &PropertyType, value: &Value) -> Vec<Value> {
    match ty {
        PropertyType::Int => shrink_int_candidates(value),
        PropertyType::Float => shrink_float_candidates(value),
        PropertyType::Bool => shrink_bool_candidates(value),
        PropertyType::String => shrink_string_candidates(value),
        PropertyType::Vec(inner) => shrink_vec_candidates(inner, value),
        PropertyType::Option(inner) => shrink_option_candidates(inner, value),
    }
}

fn shrink_int_candidates(value: &Value) -> Vec<Value> {
    let Some(current) = value.as_i64() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    push_unique(&mut out, Value::Number(Number::from(0)));
    push_unique(&mut out, Value::Number(Number::from(current / 2)));
    let step = if current > 0 {
        current.saturating_sub(1)
    } else {
        current.saturating_add(1)
    };
    push_unique(&mut out, Value::Number(Number::from(step)));
    if current != 0 {
        push_unique(&mut out, Value::Number(Number::from(current.signum())));
    }
    out.retain(|candidate| candidate != value);
    out
}

fn shrink_float_candidates(value: &Value) -> Vec<Value> {
    let Some(current) = value.as_f64() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    push_unique_float(&mut out, 0.0);
    push_unique_float(&mut out, current / 2.0);
    if current > 0.0 {
        push_unique_float(&mut out, 1.0);
    } else if current < 0.0 {
        push_unique_float(&mut out, -1.0);
    }
    out.retain(|candidate| candidate != value);
    out
}

fn shrink_bool_candidates(value: &Value) -> Vec<Value> {
    match value.as_bool() {
        Some(true) => vec![Value::Bool(false)],
        _ => Vec::new(),
    }
}

fn shrink_string_candidates(value: &Value) -> Vec<Value> {
    let Some(current) = value.as_str() else {
        return Vec::new();
    };
    if current.is_empty() {
        return Vec::new();
    }

    let chars = current.chars().collect::<Vec<_>>();
    let half = chars.iter().take(chars.len() / 2).collect::<String>();
    let first = chars.first().map(|ch| ch.to_string()).unwrap_or_default();

    let mut out = Vec::new();
    push_unique(&mut out, Value::String(String::new()));
    if !half.is_empty() {
        push_unique(&mut out, Value::String(half));
    }
    push_unique(&mut out, Value::String(first));
    out.retain(|candidate| candidate != value);
    out
}

fn shrink_vec_candidates(inner: &PropertyType, value: &Value) -> Vec<Value> {
    let Some(current) = value.as_array() else {
        return Vec::new();
    };

    let mut out = Vec::new();

    if !current.is_empty() {
        push_unique(&mut out, Value::Array(Vec::new()));

        let half_len = current.len() / 2;
        if half_len > 0 {
            push_unique(&mut out, Value::Array(current[..half_len].to_vec()));
        }

        push_unique(
            &mut out,
            Value::Array(current[..current.len() - 1].to_vec()),
        );
    }

    for (index, element) in current.iter().enumerate() {
        for shrunk in shrink_candidates(inner, element) {
            let mut next = current.to_vec();
            next[index] = shrunk;
            push_unique(&mut out, Value::Array(next));
        }
    }

    out.retain(|candidate| candidate != value);
    out
}

fn shrink_option_candidates(inner: &PropertyType, value: &Value) -> Vec<Value> {
    if value.is_null() {
        return Vec::new();
    }

    let mut out = Vec::new();
    push_unique(&mut out, Value::Null);
    for shrunk in shrink_candidates(inner, value) {
        push_unique(&mut out, shrunk);
    }
    out.retain(|candidate| candidate != value);
    out
}

fn push_unique(values: &mut Vec<Value>, candidate: Value) {
    if !values.iter().any(|current| current == &candidate) {
        values.push(candidate);
    }
}

fn push_unique_float(values: &mut Vec<Value>, candidate: f64) {
    if !candidate.is_finite() {
        return;
    }
    let Some(number) = Number::from_f64(candidate) else {
        return;
    };
    push_unique(values, Value::Number(number));
}

fn generate_argument_object(params: &[PropertyParam], rng: &mut Lcg64) -> Value {
    let mut out = Map::new();
    for param in params {
        out.insert(param.name.clone(), generate_value(&param.ty, rng, 0));
    }
    Value::Object(out)
}

fn generate_value(ty: &PropertyType, rng: &mut Lcg64, depth: usize) -> Value {
    match ty {
        PropertyType::Int => Value::Number(Number::from(rng.range_i64(-100, 100))),
        PropertyType::Float => {
            let numerator = rng.range_i64(-10_000, 10_000) as f64;
            let denominator = rng.range_i64(1, 100) as f64;
            let value = numerator / denominator;
            Value::Number(Number::from_f64(value).unwrap_or_else(|| Number::from(0)))
        }
        PropertyType::Bool => Value::Bool(rng.next_bool()),
        PropertyType::String => Value::String(generate_ascii_string(rng)),
        PropertyType::Vec(inner) => {
            let max_len = if depth >= 4 { 2 } else { 4 };
            let len = rng.range_i64(0, max_len) as usize;
            let mut values = Vec::with_capacity(len);
            for _ in 0..len {
                values.push(generate_value(inner, rng, depth + 1));
            }
            Value::Array(values)
        }
        PropertyType::Option(inner) => {
            if rng.next_bool() {
                Value::Null
            } else {
                generate_value(inner, rng, depth + 1)
            }
        }
    }
}

fn generate_ascii_string(rng: &mut Lcg64) -> String {
    let len = rng.range_i64(0, 12) as usize;
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let code = rng.range_i64(32, 126) as u8;
        out.push(char::from(code));
    }
    out
}

fn discover_property_tests(source: &str, file: &Path) -> Vec<PropertyCase> {
    let mut out = Vec::new();
    let lines = source.lines().collect::<Vec<_>>();
    let mut pending_property: Option<Result<usize, String>> = None;

    let mut index = 0usize;
    while index < lines.len() {
        let trimmed = lines[index].trim();

        if trimmed.starts_with("#[property") {
            pending_property = Some(parse_property_iterations(trimmed));
            index += 1;
            continue;
        }

        let Some(iteration_result) = pending_property.take() else {
            index += 1;
            continue;
        };

        if trimmed.is_empty() || trimmed.starts_with("//") {
            pending_property = Some(iteration_result);
            index += 1;
            continue;
        }

        let mut signature = trimmed.to_string();
        while !signature.contains('{') && index + 1 < lines.len() {
            index += 1;
            let next = lines[index].trim();
            signature.push(' ');
            signature.push_str(next);
            if next.contains('{') {
                break;
            }
        }

        match parse_property_signature(&signature) {
            Ok((name, params)) => {
                let mut typed_params = Vec::new();
                let mut parse_error = None;
                for (param_name, raw_ty) in params {
                    match parse_property_type(&raw_ty) {
                        Ok(ty) => typed_params.push(PropertyParam {
                            name: param_name,
                            ty,
                        }),
                        Err(err) => {
                            parse_error = Some(format!(
                                "unsupported property parameter type `{raw_ty}` in `{name}`: {err}"
                            ));
                            break;
                        }
                    }
                }

                let iterations = iteration_result.unwrap_or(DEFAULT_ITERATIONS);
                out.push(PropertyCase {
                    file: file.to_path_buf(),
                    name,
                    iterations,
                    params: typed_params,
                    discovery_error: parse_error,
                });
            }
            Err(err) => {
                out.push(PropertyCase {
                    file: file.to_path_buf(),
                    name: "<invalid-property>".to_string(),
                    iterations: iteration_result.unwrap_or(DEFAULT_ITERATIONS),
                    params: Vec::new(),
                    discovery_error: Some(err),
                });
            }
        }

        index += 1;
    }

    out
}

fn parse_property_iterations(raw_attr: &str) -> Result<usize, String> {
    let trimmed = raw_attr.trim();
    if trimmed == "#[property]" {
        return Ok(DEFAULT_ITERATIONS);
    }

    let prefix = "#[property(";
    let suffix = ")]";
    if !trimmed.starts_with(prefix) || !trimmed.ends_with(suffix) {
        return Err("expected #[property] or #[property(iterations = N)]".to_string());
    }

    let inner = &trimmed[prefix.len()..trimmed.len() - suffix.len()];
    for segment in inner.split(',') {
        let part = segment.trim();
        let Some(value) = part.strip_prefix("iterations") else {
            continue;
        };
        let Some(raw_number) = value.trim().strip_prefix('=') else {
            return Err("expected iterations = N".to_string());
        };
        let parsed = raw_number
            .trim()
            .parse::<usize>()
            .map_err(|_| "iterations must be a positive integer".to_string())?;
        if parsed == 0 {
            return Err("iterations must be greater than zero".to_string());
        }
        return Ok(parsed);
    }

    Err("missing iterations = N in #[property(...)]".to_string())
}

fn parse_property_signature(signature: &str) -> Result<(String, Vec<(String, String)>), String> {
    let mut line = signature.trim();
    if let Some(rest) = line.strip_prefix("pub ") {
        line = rest.trim_start();
    }
    let Some(rest) = line.strip_prefix("fn ") else {
        return Err("property attribute must attach to a function declaration".to_string());
    };

    let mut name = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(ch) = chars.peek().copied() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            name.push(ch);
            chars.next();
        } else {
            break;
        }
    }

    if name.is_empty() {
        return Err("unable to parse property function name".to_string());
    }

    let remainder = &rest[name.len()..];
    let open_index = remainder
        .find('(')
        .ok_or_else(|| "property function signature missing '('".to_string())?;
    let params_start = open_index + 1;

    let mut depth = 1i32;
    let mut params_end = None;
    for (offset, ch) in remainder[params_start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    params_end = Some(params_start + offset);
                    break;
                }
            }
            _ => {}
        }
    }

    let params_end =
        params_end.ok_or_else(|| "property function signature missing ')'".to_string())?;
    let params_raw = &remainder[params_start..params_end];

    let mut params = Vec::new();
    for part in split_top_level(params_raw, ',') {
        let segment = part.trim();
        if segment.is_empty() {
            continue;
        }
        let Some(colon_index) = segment.find(':') else {
            return Err(format!(
                "parameter `{segment}` is missing a type annotation"
            ));
        };

        let raw_name = segment[..colon_index].trim();
        let param_name = raw_name
            .strip_prefix("mut ")
            .map(str::trim)
            .unwrap_or(raw_name)
            .to_string();
        if param_name.is_empty() {
            return Err("parameter name must not be empty".to_string());
        }

        let raw_ty = segment[colon_index + 1..].trim().to_string();
        if raw_ty.is_empty() {
            return Err(format!("parameter `{param_name}` is missing a type"));
        }

        params.push((param_name, raw_ty));
    }

    Ok((name, params))
}

fn parse_property_type(raw: &str) -> anyhow::Result<PropertyType> {
    let trimmed = raw.trim();
    match trimmed {
        "Int" => return Ok(PropertyType::Int),
        "Float" => return Ok(PropertyType::Float),
        "Bool" => return Ok(PropertyType::Bool),
        "String" => return Ok(PropertyType::String),
        _ => {}
    }

    if let Some(inner) = strip_outer_generic(trimmed, "Vec") {
        return Ok(PropertyType::Vec(Box::new(parse_property_type(inner)?)));
    }

    if let Some(inner) = strip_outer_generic(trimmed, "Option") {
        return Ok(PropertyType::Option(Box::new(parse_property_type(inner)?)));
    }

    anyhow::bail!("supported types are Int, Float, Bool, String, Vec[T], Option[T]")
}

fn strip_outer_generic<'a>(raw: &'a str, name: &str) -> Option<&'a str> {
    let rest = raw.strip_prefix(name)?.trim_start();
    if !rest.starts_with('[') || !rest.ends_with(']') {
        return None;
    }

    let mut depth = 0i32;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    if idx != rest.len() - 1 {
                        return None;
                    }
                    return Some(rest[1..rest.len() - 1].trim());
                }
            }
            _ => {}
        }
    }

    None
}

fn split_top_level(raw: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut square_depth = 0i32;
    let mut round_depth = 0i32;
    let mut brace_depth = 0i32;

    for ch in raw.chars() {
        match ch {
            '[' => square_depth += 1,
            ']' => square_depth -= 1,
            '(' => round_depth += 1,
            ')' => round_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            _ => {}
        }

        if ch == delimiter && square_depth == 0 && round_depth == 0 && brace_depth == 0 {
            parts.push(current.trim().to_string());
            current.clear();
            continue;
        }

        current.push(ch);
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    parts
}

fn transform_source_for_property_case(source: &str, case: &PropertyCase) -> anyhow::Result<String> {
    if source.contains("fn main(") {
        anyhow::bail!(
            "property tests must not define `main`; test runner injects its own entrypoint"
        );
    }

    let stripped = strip_test_attributes(source);
    let mut with_effects = stripped;
    let mut property_names = discover_property_tests(source, Path::new("memory"))
        .into_iter()
        .filter_map(|entry| {
            if entry.discovery_error.is_none() && entry.name != "<invalid-property>" {
                Some(entry.name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if !property_names.iter().any(|name| name == &case.name) {
        property_names.push(case.name.clone());
    }
    property_names.sort();
    property_names.dedup();
    for name in property_names {
        with_effects = add_io_effect_to_property_fn(&with_effects, &name)?;
    }
    let with_io_import = ensure_import(&with_effects, "import std.io;");
    let with_env_import = ensure_import(&with_io_import, "import std.env;");
    let with_json_import = ensure_import(&with_env_import, "import std.json;");
    let rewritten = rewrite_assert_calls(&with_json_import);
    let stem = sanitize_identifier(&case.name);
    let struct_name = format!("PropertyArgs{stem}");
    let run_name = format!("property_run_{}", stem.to_ascii_lowercase());
    let exec_name = format!("property_exec_{}", stem.to_ascii_lowercase());

    let struct_fields = case
        .params
        .iter()
        .map(|param| format!("    {}: {},", param.name, render_property_type(&param.ty)))
        .collect::<Vec<_>>()
        .join("\n");

    let arg_expr = case
        .params
        .iter()
        .map(|param| format!("decoded.{}", param.name))
        .collect::<Vec<_>>()
        .join(", ");

    Ok(format!(
        "{}\n{}\nstruct {} {{\n{}\n}}\n\nfn {}(decoded: {}) -> Int effects {{ io, fs, net, time, rand, env, proc, concurrency }} {{\n    {}({});\n    0\n}}\n\nfn {}(raw: String) -> Int effects {{ io, fs, net, time, rand, env, proc, concurrency }} {{\n    let parsed = match parse(raw) {{\n        Ok(value) => value,\n        Err(_) => encode_null(),\n    }};\n\n    let marker: Option[{}] = None();\n    match decode_with(parsed, marker) {{\n        Ok(value) => {}(value),\n        Err(_) => 3,\n    }}\n}}\n\nfn main() -> Int effects {{ io, fs, net, time, rand, env, proc, concurrency }} {{\n    let raw = match env.get(\"AIC_PROP_ARGS_JSON\") {{\n        Ok(value) => value,\n        Err(_) => \"\",\n    }};\n    {}(raw)\n}}\n",
        rewritten,
        ASSERT_HELPERS,
        struct_name,
        struct_fields,
        exec_name,
        struct_name,
        case.name,
        arg_expr,
        run_name,
        struct_name,
        exec_name,
        run_name,
    ))
}

fn sanitize_identifier(raw: &str) -> String {
    let mut out = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();

    if out.is_empty() {
        out.push('P');
        return out;
    }

    let starts_with_letter = out
        .chars()
        .next()
        .map(|ch| ch.is_ascii_alphabetic())
        .unwrap_or(false);
    if !starts_with_letter {
        out.insert(0, 'P');
    }

    out
}

fn render_property_type(ty: &PropertyType) -> String {
    match ty {
        PropertyType::Int => "Int".to_string(),
        PropertyType::Float => "Float".to_string(),
        PropertyType::Bool => "Bool".to_string(),
        PropertyType::String => "String".to_string(),
        PropertyType::Vec(inner) => format!("Vec[{}]", render_property_type(inner)),
        PropertyType::Option(inner) => format!("Option[{}]", render_property_type(inner)),
    }
}

fn strip_test_attributes(source: &str) -> String {
    let mut out = String::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[test]")
            || trimmed.starts_with("#[should_panic]")
            || trimmed.starts_with("#[property")
        {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
fn add_io_effect_to_property_fn(source: &str, function_name: &str) -> anyhow::Result<String> {
    let mut out = String::new();
    let mut in_signature = false;
    let mut signature_has_effects = false;
    let mut found = false;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if !in_signature && !found {
            if let Some(name) = parse_fn_name(trimmed) {
                if name == function_name {
                    in_signature = true;
                    signature_has_effects = trimmed.contains("effects {");
                }
            }
        }

        if in_signature {
            if trimmed.contains("effects {") {
                signature_has_effects = true;
            }

            if let Some(brace_index) = line.rfind('{') {
                if signature_has_effects {
                    out.push_str(line);
                } else {
                    let (prefix, suffix) = line.split_at(brace_index);
                    out.push_str(prefix.trim_end());
                    out.push_str(" effects { io } ");
                    out.push_str(suffix);
                }
                out.push('\n');
                in_signature = false;
                found = true;
                continue;
            }
        }

        out.push_str(line);
        out.push('\n');
    }

    if !found {
        anyhow::bail!(
            "property function '{}' must keep '{{' in its signature declaration",
            function_name
        );
    }

    Ok(out)
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

fn ensure_import(source: &str, import_line: &str) -> String {
    if source.lines().any(|line| line.trim() == import_line) {
        return source.to_string();
    }

    let mut out = String::new();
    let mut inserted = false;
    for line in source.lines() {
        out.push_str(line);
        out.push('\n');
        if !inserted && line.trim_start().starts_with("module ") {
            out.push_str(import_line);
            out.push('\n');
            inserted = true;
        }
    }

    if !inserted {
        format!("{}\n{}", import_line, out)
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

fn compute_case_seed(seed: u64, file: &Path, name: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    file.to_string_lossy().hash(&mut hasher);
    name.hash(&mut hasher);
    hasher.finish()
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

#[derive(Debug, Clone)]
struct Lcg64 {
    state: u64,
}

impl Lcg64 {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9e3779b97f4a7c15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }

    fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    fn range_i64(&mut self, min_inclusive: i64, max_inclusive: i64) -> i64 {
        if min_inclusive >= max_inclusive {
            return min_inclusive;
        }
        let span = (max_inclusive - min_inclusive + 1) as u64;
        min_inclusive + (self.next_u64() % span) as i64
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        discover_property_tests, parse_property_iterations, parse_property_signature,
        run_property_tests,
    };

    #[test]
    fn parses_property_iterations_attribute() {
        assert_eq!(parse_property_iterations("#[property]"), Ok(100));
        assert_eq!(
            parse_property_iterations("#[property(iterations = 42)]"),
            Ok(42)
        );
        assert!(parse_property_iterations("#[property(iterations = 0)]").is_err());
    }

    #[test]
    fn parses_property_signature_with_supported_types() {
        let signature = "fn prop(v: Vec[Int], maybe: Option[String], flag: Bool) -> () {";
        let parsed = parse_property_signature(signature).expect("parse signature");
        assert_eq!(parsed.0, "prop");
        assert_eq!(parsed.1.len(), 3);
        assert_eq!(parsed.1[0].0, "v");
        assert_eq!(parsed.1[0].1, "Vec[Int]");
    }

    #[test]
    fn discovers_property_tests_and_iterations() {
        let source = r#"
#[property]
fn prop_default(x: Int) -> () {
    assert_eq(x, x);
}

#[property(iterations = 12)]
fn prop_custom(flag: Bool) -> () {
    assert(flag || !flag);
}
"#;
        let discovered = discover_property_tests(source, std::path::Path::new("tests.aic"));
        assert_eq!(discovered.len(), 2);
        assert_eq!(discovered[0].name, "prop_default");
        assert_eq!(discovered[0].iterations, 100);
        assert_eq!(discovered[1].name, "prop_custom");
        assert_eq!(discovered[1].iterations, 12);
    }

    #[test]
    fn runs_property_tests_and_reports_seed_and_counterexample() {
        let dir = tempdir().expect("tempdir");
        let test_file = dir.path().join("properties.aic");
        fs::write(
            &test_file,
            r#"
#[property(iterations = 4)]
fn prop_generators_cover_all(
    i: Int,
    f: Float,
    b: Bool,
    s: String
) -> () {
    assert_eq(i, i);
    assert(b || !b);
}

#[property(iterations = 6)]
fn prop_fails(x: Int) -> () {
    assert_eq(x + 1, x);
}
"#,
        )
        .expect("write property tests");

        let report = run_property_tests(dir.path(), None, 7).expect("run property tests");
        assert_eq!(report.total, 2, "report={report:#?}");
        assert_eq!(report.passed, 1, "report={report:#?}");
        assert_eq!(report.failed, 1, "report={report:#?}");

        let failing = report
            .cases
            .iter()
            .find(|case| case.file.ends_with("::prop_fails"))
            .expect("prop_fails case");
        assert!(!failing.passed, "case={failing:#?}");
        assert!(failing.details.contains("seed="), "case={failing:#?}");
        assert!(
            failing.details.contains("counterexample="),
            "case={failing:#?}"
        );
        assert!(failing.details.contains("shrunk="), "case={failing:#?}");
    }
}
