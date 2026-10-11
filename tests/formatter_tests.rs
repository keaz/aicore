//! End-to-end checks for `aic fmt`: comment and hole preservation through the
//! CLI, and a corpus run over every checked-in `.aic` file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aicore::formatter::{format_source, FormatError};
use aicore::lexer::lex_with_comments;
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_aic(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("run aic")
}

fn sorted_comments(source: &str) -> Vec<String> {
    let (_, comments, _) = lex_with_comments(source, "corpus.aic");
    let mut texts = comments
        .into_iter()
        .map(|comment| comment.text)
        .collect::<Vec<_>>();
    texts.sort();
    texts
}

#[test]
fn fmt_cli_keeps_comments_and_typed_holes() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    let source = "// keep me\nfn plus_one(x: _) -> _ { x + 1 }\n\nfn main() -> Int {\n    plus_one(1) // call\n}\n";
    fs::write(&path, source).expect("write source");
    let path_arg = path.to_string_lossy().to_string();

    let output = run_aic(&["fmt", &path_arg]);
    assert!(
        output.status.success(),
        "aic fmt failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let formatted = fs::read_to_string(&path).expect("read formatted");
    assert_eq!(
        formatted,
        "// keep me\nfn plus_one(x: _) -> _ {\n    x + 1\n}\n\nfn main() -> Int {\n    plus_one(1) // call\n}\n"
    );

    let check = run_aic(&["fmt", &path_arg, "--check"]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "formatted file must pass fmt --check\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn fmt_cli_check_reports_unformatted_file_without_writing() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    let source = "// note\nfn main() -> Int { 0 }\n";
    fs::write(&path, source).expect("write source");

    let output = run_aic(&["fmt", &path.to_string_lossy(), "--check"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("format check failed"));
    assert_eq!(fs::read_to_string(&path).expect("read"), source);
}

#[test]
fn fmt_cli_leaves_file_with_parse_errors_untouched() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    let source = "// note\nfn main( -> Int { 0 }\n";
    fs::write(&path, source).expect("write source");

    let output = run_aic(&["fmt", &path.to_string_lossy()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("error["));
    assert_eq!(fs::read_to_string(&path).expect("read"), source);
}

fn collect_aic_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !matches!(
                name.to_str(),
                Some("target" | "node_modules" | ".git" | ".aic-cache")
            ) {
                collect_aic_files(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "aic") {
            out.push(path);
        }
    }
}

/// Every checked-in source that parses must format without tripping the
/// formatter's self-check, keep all of its comments, and reach a fixed point.
#[test]
fn formatter_preserves_comments_and_is_idempotent_on_repository_sources() {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in ["examples", "std", "tests", "compiler", "tools", "docs"] {
        collect_aic_files(&root.join(dir), &mut files);
    }
    files.sort();
    assert!(
        files.len() > 100,
        "expected a corpus, found {}",
        files.len()
    );

    let mut formatted_count = 0;
    let mut failures = Vec::new();
    for path in &files {
        let source = fs::read_to_string(path).expect("read source");
        let name = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        let formatted = match format_source(&source, &name) {
            Ok(formatted) => formatted,
            Err(FormatError::Diagnostics(_)) => continue,
            Err(err) => {
                failures.push(format!("{name}: {err}"));
                continue;
            }
        };
        formatted_count += 1;
        if sorted_comments(&formatted) != sorted_comments(&source) {
            failures.push(format!("{name}: comments changed"));
        }
        match format_source(&formatted, &name) {
            Ok(again) if again == formatted => {}
            Ok(_) => failures.push(format!("{name}: not idempotent")),
            Err(err) => failures.push(format!("{name}: reformat failed: {err}")),
        }
    }
    assert!(
        formatted_count > 100,
        "only {formatted_count} files formatted"
    );
    assert!(
        failures.is_empty(),
        "formatter failures:\n{}",
        failures.join("\n")
    );
}
