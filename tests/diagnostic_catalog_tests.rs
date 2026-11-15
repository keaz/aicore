//! Diagnostic catalog contract.
//!
//! `docs/errors/catalog.md` has one row per registered diagnostic code:
//!
//! ```text
//! | `E1234` | Mode | Description | Trigger | Fix |
//! ```
//!
//! For the runnable modes the trigger and fix columns are single-file AIC
//! programs, and this test runs them:
//!
//! - `check`: the frontend (`aic check`) must report the code for the trigger.
//! - `warn-unused`: as `check`, plus the unused-analysis warnings that
//!   `aic check --warn-unused` adds.
//! - `build`: the trigger passes the frontend and code generation reports
//!   the code.
//!
//! In every runnable mode the fix must not report the code and must have no
//! error-severity diagnostics at all.
//!
//! The other modes (`cli`, `package`, `selfhost`, `internal`, `reserved`,
//! `retired`) describe how the code is produced in prose. They are not run,
//! but every registered code must still have exactly one row.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use aicore::codegen::{emit_llvm_with_resolution_and_options, CodegenOptions};
use aicore::contracts::lower_runtime_asserts;
use aicore::diagnostic_codes::REGISTERED_DIAGNOSTIC_CODES;
use aicore::diagnostics::{Diagnostic, Severity};
use aicore::driver::{has_errors, run_frontend};
use aicore::unused_analysis::analyze_unused_warnings;

const RUNNABLE_MODES: &[&str] = &["check", "warn-unused", "build"];
const DESCRIBED_MODES: &[&str] = &[
    "cli", "package", "selfhost", "internal", "reserved", "retired",
];

#[derive(Debug, Clone)]
struct CatalogRow {
    line: usize,
    code: String,
    mode: String,
    description: String,
    trigger: String,
    fix: String,
}

fn catalog_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/errors/catalog.md");
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

/// Splits a Markdown table row into cells. A `|` inside a code span or
/// escaped as `\|` belongs to the cell; `\|` is unescaped to `|`.
fn split_cells(line: &str) -> Option<Vec<String>> {
    let inner = line.trim().strip_prefix('|')?.strip_suffix('|')?;
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut in_code = false;
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                current.push('|');
            }
            '`' => {
                in_code = !in_code;
                current.push(ch);
            }
            '|' if !in_code => cells.push(std::mem::take(&mut current).trim().to_string()),
            _ => current.push(ch),
        }
    }
    cells.push(current.trim().to_string());
    Some(cells)
}

fn is_code_cell(cell: &str) -> bool {
    let bytes = cell.as_bytes();
    bytes.len() == 7
        && bytes[0] == b'`'
        && bytes[6] == b'`'
        && bytes[1] == b'E'
        && bytes[2..6].iter().all(u8::is_ascii_digit)
}

/// Returns the program inside a cell that is exactly one code span.
fn program_of(cell: &str) -> Option<&str> {
    let program = cell.strip_prefix('`')?.strip_suffix('`')?;
    (!program.contains('`') && !program.trim().is_empty()).then_some(program)
}

/// Parses every catalog row whose first cell is a diagnostic code. Returns
/// the rows and the format problems found on the way.
fn parse_catalog(text: &str) -> (Vec<CatalogRow>, Vec<String>) {
    let mut rows = Vec::new();
    let mut problems = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        let Some(cells) = split_cells(line) else {
            continue;
        };
        if !cells.first().is_some_and(|cell| is_code_cell(cell)) {
            continue;
        }
        if cells.len() != 5 {
            problems.push(format!(
                "line {line_no}: expected 5 cells (code, mode, description, trigger, fix), found {}",
                cells.len()
            ));
            continue;
        }
        let mut cells = cells.into_iter();
        let mut next = || cells.next().expect("five cells");
        rows.push(CatalogRow {
            line: line_no,
            code: next().trim_matches('`').to_string(),
            mode: next(),
            description: next(),
            trigger: next(),
            fix: next(),
        });
    }
    (rows, problems)
}

fn validate_row(row: &CatalogRow, problems: &mut Vec<String>) {
    let at = format!("line {} ({})", row.line, row.code);
    if !RUNNABLE_MODES.contains(&row.mode.as_str()) && !DESCRIBED_MODES.contains(&row.mode.as_str())
    {
        problems.push(format!("{at}: unknown mode `{}`", row.mode));
    }
    if row.description.is_empty() {
        problems.push(format!("{at}: empty description"));
    }
    if RUNNABLE_MODES.contains(&row.mode.as_str()) {
        if program_of(&row.trigger).is_none() {
            problems.push(format!(
                "{at}: trigger must be one code span holding a program"
            ));
        }
        if program_of(&row.fix).is_none() {
            problems.push(format!("{at}: fix must be one code span holding a program"));
        }
    } else if row.trigger.is_empty() || row.fix.is_empty() {
        problems.push(format!(
            "{at}: `{}` rows must say how the code is produced and resolved",
            row.mode
        ));
    }
}

fn catalog_rows() -> Vec<CatalogRow> {
    let (rows, mut problems) = parse_catalog(&catalog_text());
    for row in &rows {
        validate_row(row, &mut problems);
    }
    assert!(
        problems.is_empty(),
        "docs/errors/catalog.md has malformed rows:\n{}",
        problems.join("\n")
    );
    rows
}

#[test]
fn every_registered_code_has_exactly_one_catalog_row() {
    let rows = catalog_rows();
    let mut by_code = BTreeMap::<&str, Vec<usize>>::new();
    for row in &rows {
        by_code.entry(row.code.as_str()).or_default().push(row.line);
    }
    let registered = REGISTERED_DIAGNOSTIC_CODES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    let missing = registered
        .iter()
        .filter(|code| !by_code.contains_key(*code))
        .copied()
        .collect::<Vec<_>>();
    let unknown = by_code
        .keys()
        .filter(|code| !registered.contains(*code))
        .copied()
        .collect::<Vec<_>>();
    let duplicated = by_code
        .iter()
        .filter(|(_, lines)| lines.len() > 1)
        .map(|(code, lines)| format!("{code} (lines {lines:?})"))
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "registered codes without a catalog row: {}",
        missing.join(", ")
    );
    assert!(
        unknown.is_empty(),
        "catalog rows for unregistered codes: {}",
        unknown.join(", ")
    );
    assert!(
        duplicated.is_empty(),
        "codes with more than one catalog row: {}",
        duplicated.join(", ")
    );

    let summary = format!("({} codes)", REGISTERED_DIAGNOSTIC_CODES.len());
    assert!(
        catalog_text().contains(&summary),
        "catalog introduction must state the registry size as `{summary}`"
    );
}

/// Shared trigger programs or descriptions mean a row was filled with a
/// placeholder instead of a program for its own code. Reserved codes have
/// nothing to describe beyond being reserved, so they may share text.
#[test]
fn catalog_rows_do_not_share_triggers_or_descriptions() {
    let rows = catalog_rows();
    let mut triggers = BTreeMap::<&str, Vec<&str>>::new();
    let mut descriptions = BTreeMap::<&str, Vec<&str>>::new();
    for row in &rows {
        if RUNNABLE_MODES.contains(&row.mode.as_str()) {
            triggers.entry(&row.trigger).or_default().push(&row.code);
        }
        if row.mode == "reserved" {
            continue;
        }
        descriptions
            .entry(&row.description)
            .or_default()
            .push(&row.code);
    }
    let shared = triggers
        .iter()
        .chain(descriptions.iter())
        .filter(|(_, codes)| codes.len() > 1)
        .map(|(text, codes)| format!("{} share {text}", codes.join(", ")))
        .collect::<Vec<_>>();
    assert!(
        shared.is_empty(),
        "catalog rows must each have their own trigger and description:\n{}",
        shared.join("\n")
    );
}

struct Report {
    /// Codes of every diagnostic, any severity, from the requested stages.
    codes: Vec<String>,
    /// `code message` of every error-severity diagnostic.
    errors: Vec<String>,
}

fn describe_errors(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|diag| matches!(diag.severity, Severity::Error))
        .map(|diag| format!("{} {}", diag.code, diag.message))
        .collect()
}

/// Runs `source` through the stages a mode stands for and collects what
/// they report.
fn run_program(source: &str, mode: &str) -> Report {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("main.aic");
    fs::write(&path, source).expect("write program");
    let front = match run_frontend(&path) {
        Ok(front) => front,
        Err(err) => {
            return Report {
                codes: Vec::new(),
                errors: vec![format!("frontend failed: {err:#}")],
            }
        }
    };
    let mut diagnostics = front.diagnostics.clone();
    if mode == "warn-unused" {
        diagnostics.extend(analyze_unused_warnings(
            &front.ast,
            &front.resolution,
            &front.item_modules,
            &path.to_string_lossy(),
            source,
        ));
    }
    if mode == "build" && !has_errors(&front.diagnostics) {
        let lowered = lower_runtime_asserts(&front.ir);
        if let Err(codegen) = emit_llvm_with_resolution_and_options(
            &lowered,
            Some(&front.resolution),
            &path.to_string_lossy(),
            CodegenOptions::default(),
        ) {
            diagnostics.extend(codegen);
        }
    }
    Report {
        codes: diagnostics.iter().map(|diag| diag.code.clone()).collect(),
        errors: describe_errors(&diagnostics),
    }
}

fn check_row(row: &CatalogRow) -> Vec<String> {
    let mut failures = Vec::new();
    let at = format!("{} [{}] (catalog line {})", row.code, row.mode, row.line);
    let trigger = program_of(&row.trigger).expect("validated trigger");
    let fix = program_of(&row.fix).expect("validated fix");

    let triggered = run_program(trigger, &row.mode);
    if !triggered.codes.contains(&row.code) {
        failures.push(format!(
            "{at}: trigger does not report {}; reported {:?}\n    trigger: {trigger}\n    errors: {:?}",
            row.code, triggered.codes, triggered.errors
        ));
    }

    let fixed = run_program(fix, &row.mode);
    if fixed.codes.contains(&row.code) {
        failures.push(format!(
            "{at}: fix still reports {}\n    fix: {fix}",
            row.code
        ));
    }
    if !fixed.errors.is_empty() {
        failures.push(format!(
            "{at}: fix has errors {:?}\n    fix: {fix}",
            fixed.errors
        ));
    }
    failures
}

#[test]
fn runnable_catalog_rows_trigger_their_code_and_fixes_are_clean() {
    let rows = catalog_rows()
        .into_iter()
        .filter(|row| RUNNABLE_MODES.contains(&row.mode.as_str()))
        .collect::<Vec<_>>();
    assert!(
        rows.len() > 200,
        "expected most catalog rows to be runnable, found {}",
        rows.len()
    );

    let workers = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();
    for chunk in rows.chunks(rows.len().div_ceil(workers)) {
        let chunk = chunk.to_vec();
        let tx = tx.clone();
        handles.push(
            thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || {
                    for row in chunk {
                        tx.send(check_row(&row)).expect("send result");
                    }
                })
                .expect("spawn worker"),
        );
    }
    drop(tx);
    let mut failures = rx.into_iter().flatten().collect::<Vec<_>>();
    for handle in handles {
        handle.join().expect("worker panicked");
    }
    failures.sort();
    assert!(
        failures.is_empty(),
        "{} catalog row problem(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn split_cells_keeps_escaped_and_code_span_pipes() {
    let cells = split_cells(
        r"| `E1289` | check | Closure writes a capture. | `fn main() -> Int { let f = \|\| -> Int { 1 }; f() }` | `fn main() -> Int { 0 }` |",
    )
    .expect("table row");
    assert_eq!(cells.len(), 5);
    assert_eq!(
        program_of(&cells[3]),
        Some("fn main() -> Int { let f = || -> Int { 1 }; f() }")
    );
    assert_eq!(split_cells("not a row"), None);
}

#[test]
fn parse_catalog_reports_malformed_rows() {
    let text = "| Code | Mode |\n|---|---|\n| `E0001` | check | too few |\n| `E0004` | check | Bad escape. | `fn main() -> Int { 0 }` | `fn main() -> Int { 0 }` |\n";
    let (rows, problems) = parse_catalog(text);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].code, "E0004");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("line 3:"), "{problems:?}");

    let mut found = Vec::new();
    let bad_mode = CatalogRow {
        mode: "snippet".to_string(),
        ..rows[0].clone()
    };
    validate_row(&bad_mode, &mut found);
    let prose_trigger = CatalogRow {
        trigger: "Run `aic check` on a package".to_string(),
        ..rows[0].clone()
    };
    validate_row(&prose_trigger, &mut found);
    let empty_described = CatalogRow {
        mode: "cli".to_string(),
        fix: String::new(),
        ..rows[0].clone()
    };
    validate_row(&empty_described, &mut found);
    assert_eq!(found.len(), 3, "{found:?}");
}

#[test]
fn run_program_reports_codes_for_each_mode() {
    let parse_error = run_program("fn main() -> Int { $ }", "check");
    assert!(parse_error.codes.contains(&"E0001".to_string()));
    assert!(!parse_error.errors.is_empty());

    let clean = run_program("fn main() -> Int { 0 }", "build");
    assert!(clean.codes.is_empty(), "{:?}", clean.codes);
    assert!(clean.errors.is_empty(), "{:?}", clean.errors);

    let unused = run_program("fn main() -> Int { let value = 1; 0 }", "warn-unused");
    assert!(unused.codes.contains(&"E6006".to_string()));
    assert!(unused.errors.is_empty(), "{:?}", unused.errors);
    let plain = run_program("fn main() -> Int { let value = 1; 0 }", "check");
    assert!(!plain.codes.contains(&"E6006".to_string()));
}
