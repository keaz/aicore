//! Check/build parity gate.
//!
//! Every `.aic` source under `examples/` and `tests/` that passes the
//! checker must also lower through code generation without an `E5xxx`
//! diagnostic, except the codes in `REACHABLE_CODEGEN_CODES`, which are
//! documented in `docs/diagnostic-codes.md` as reachable after a clean
//! `aic check`. Agents rely on `aic check` as the fast gate, so a program it
//! accepts must build.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use aicore::codegen::{emit_llvm_with_resolution_and_options, CodegenOptions};
use aicore::contracts::lower_runtime_asserts;
use aicore::diagnostics::Severity;
use aicore::driver::{has_errors, run_frontend};

/// Codegen diagnostics that may follow a clean `aic check`, with the reason
/// in `docs/diagnostic-codes.md`. Keep this list in sync with that page.
const REACHABLE_CODEGEN_CODES: &[&str] = &[
    // Mutual polymorphic recursion (`f[T]` -> `g[Option[T]]` -> `f`), which
    // the checker only detects in its direct form (E1293).
    "E5037",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            collect_sources(&path, out);
        } else if path.extension().map(|ext| ext == "aic").unwrap_or(false) {
            out.push(path);
        }
    }
}

/// Returns `Some(codes)` with the `E5xxx` codes codegen reported for a
/// source that passed the checker, or `None` when the checker rejected it.
fn codegen_codes_after_clean_check(path: &Path) -> Option<Vec<String>> {
    let front = run_frontend(path).ok()?;
    if has_errors(&front.diagnostics) {
        return None;
    }
    let lowered = lower_runtime_asserts(&front.ir);
    let file = path.to_string_lossy().to_string();
    match emit_llvm_with_resolution_and_options(
        &lowered,
        Some(&front.resolution),
        &file,
        CodegenOptions::default(),
    ) {
        Ok(_) => Some(Vec::new()),
        Err(diagnostics) => Some(
            diagnostics
                .into_iter()
                .filter(|diag| matches!(diag.severity, Severity::Error))
                .map(|diag| format!("{} {}", diag.code, diag.message))
                .collect(),
        ),
    }
}

#[test]
fn every_source_that_passes_check_also_lowers_in_codegen() {
    let root = repo_root();
    let mut sources = Vec::new();
    collect_sources(&root.join("examples"), &mut sources);
    collect_sources(&root.join("tests"), &mut sources);
    sources.sort();
    assert!(sources.len() > 200, "found only {} sources", sources.len());

    let workers = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);
    let (tx, rx) = mpsc::channel();
    let chunks = sources
        .chunks(sources.len().div_ceil(workers))
        .map(|chunk| chunk.to_vec())
        .collect::<Vec<_>>();
    let mut handles = Vec::new();
    for chunk in chunks {
        let tx = tx.clone();
        handles.push(
            thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || {
                    for path in chunk {
                        let result = codegen_codes_after_clean_check(&path);
                        tx.send((path, result)).expect("send result");
                    }
                })
                .expect("spawn worker"),
        );
    }
    drop(tx);

    let mut checked = 0usize;
    let mut violations = Vec::new();
    for (path, result) in rx {
        let Some(codes) = result else {
            continue;
        };
        checked += 1;
        let unexpected = codes
            .into_iter()
            .filter(|entry| {
                let code = entry.split_whitespace().next().unwrap_or_default();
                code.starts_with("E5") && !REACHABLE_CODEGEN_CODES.contains(&code)
            })
            .collect::<Vec<_>>();
        if !unexpected.is_empty() {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            violations.push(format!("{rel}: {}", unexpected.join("; ")));
        }
    }
    for handle in handles {
        handle.join().expect("worker panicked");
    }
    assert!(checked > 150, "only {checked} sources passed the checker");
    violations.sort();
    assert!(
        violations.is_empty(),
        "{} source(s) pass `aic check` but fail in codegen:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

fn frontend_for_source(
    source: &str,
) -> (aicore::driver::FrontendOutput, PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(&path, source).expect("write source");
    let front = run_frontend(&path).expect("frontend");
    (front, path, dir)
}

fn error_codes(front: &aicore::driver::FrontendOutput) -> Vec<String> {
    front
        .diagnostics
        .iter()
        .filter(|diag| matches!(diag.severity, Severity::Error))
        .map(|diag| diag.code.clone())
        .collect()
}

fn codegen_error_codes(front: &aicore::driver::FrontendOutput, path: &Path) -> Vec<String> {
    let lowered = lower_runtime_asserts(&front.ir);
    match emit_llvm_with_resolution_and_options(
        &lowered,
        Some(&front.resolution),
        &path.to_string_lossy(),
        CodegenOptions::default(),
    ) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics.into_iter().map(|diag| diag.code).collect(),
    }
}

#[test]
fn literal_suffixes_survive_imports() {
    // Suffix metadata used to live in a per-parse table keyed by byte
    // offsets, so parsing an imported module dropped the entry file's
    // suffixes: `300u8` passed the checker and `65u8` lowered as `Int`.
    let (front, _path, _dir) = frontend_for_source(
        "import std.vec;\n\nfn main() -> Int {\n    let x = 300u8;\n    0\n}\n",
    );
    assert_eq!(error_codes(&front), vec!["E1204".to_string()]);

    let (front, path, _dir) = frontend_for_source(
        "import std.vec;\n\nfn main() -> Int {\n    let mut raw: Vec[UInt8] = new_vec();\n    raw = push(raw, 65u8);\n    raw = push(raw, 7u8);\n    vec_len(raw)\n}\n",
    );
    assert!(error_codes(&front).is_empty(), "{:#?}", front.diagnostics);
    assert!(codegen_error_codes(&front, &path).is_empty());
}

#[test]
fn qualified_std_calls_run_converting_wrappers() {
    // `proc.wait(handle)` takes a `ProcHandle` and returns a
    // `ProcExitStatus`; it must call the std wrapper, not lower straight to
    // the `Int`-based intrinsic.
    let (front, path, _dir) = frontend_for_source(
        "import std.proc;\n\nfn check(handle: ProcHandle) -> Int effects { proc } capabilities { proc } {\n    let running = match proc.is_running(handle) {\n        Ok(flag) => if flag { 1 } else { 0 },\n        Err(_) => 0,\n    };\n    let waited = match proc.wait(handle) {\n        Ok(_) => 1,\n        Err(_) => 0,\n    };\n    running + waited\n}\n\nfn main() -> Int effects { proc } capabilities { proc } {\n    check(0u32)\n}\n",
    );
    assert!(error_codes(&front).is_empty(), "{:#?}", front.diagnostics);
    assert!(codegen_error_codes(&front, &path).is_empty());
}

#[test]
fn resolved_typed_holes_build() {
    let (front, path, _dir) = frontend_for_source(
        "struct Counter {\n    value: _,\n}\n\nfn plus_one(x: _) -> _ {\n    let y: _ = x + 1;\n    y\n}\n\nfn main() -> Int {\n    let counter = Counter { value: 41 };\n    plus_one(counter.value)\n}\n",
    );
    assert!(error_codes(&front).is_empty(), "{:#?}", front.diagnostics);
    assert!(codegen_error_codes(&front, &path).is_empty());
}
