use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn recipe_docs() -> Vec<PathBuf> {
    vec![
        repo_root().join("docs/agent-recipes/feature-loop.md"),
        repo_root().join("docs/agent-recipes/bugfix-loop.md"),
        repo_root().join("docs/agent-recipes/refactor-loop.md"),
        repo_root().join("docs/agent-recipes/diagnostics-loop.md"),
    ]
}

fn rest_guide_doc() -> PathBuf {
    repo_root().join("docs/ai-agent-rest-guide.md")
}

fn extract_docs_test_commands(doc: &str) -> Vec<(bool, String)> {
    let mut commands = Vec::new();
    let mut in_block = false;
    for raw in doc.lines() {
        let line = raw.trim();
        if line == "<!-- docs-test:start -->" {
            in_block = true;
            continue;
        }
        if line == "<!-- docs-test:end -->" {
            in_block = false;
            continue;
        }
        if !in_block || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("! ") {
            commands.push((true, rest.trim().to_string()));
        } else {
            commands.push((false, line.to_string()));
        }
    }
    commands
}

fn extract_tagged_lines(doc: &str, start_marker: &str, end_marker: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut in_block = false;
    for raw in doc.lines() {
        let line = raw.trim();
        if line == start_marker {
            in_block = true;
            continue;
        }
        if line == end_marker {
            in_block = false;
            continue;
        }
        if !in_block || line.is_empty() || line.starts_with('#') {
            continue;
        }
        lines.push(line.to_string());
    }
    lines
}

fn run_docs_test_commands(doc_path: &Path, commands: &[(bool, String)]) {
    let root = repo_root();
    for (expect_failure, command) in commands {
        let mut parts = command.split_whitespace().collect::<Vec<_>>();
        assert!(!parts.is_empty(), "empty command in {}", doc_path.display());
        let binary = parts.remove(0);
        assert_eq!(
            binary,
            "aic",
            "docs-test command must start with `aic` in {}: {}",
            doc_path.display(),
            command
        );

        let output = Command::new(env!("CARGO_BIN_EXE_aic"))
            .args(parts)
            .current_dir(&root)
            .output()
            .unwrap_or_else(|err| panic!("failed to execute '{}': {err}", command));

        let ok = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if *expect_failure {
            assert!(
                !ok,
                "expected docs-test command to fail in {}:\n  {}\nstdout:\n{}\nstderr:\n{}",
                doc_path.display(),
                command,
                stdout,
                stderr
            );
        } else {
            assert!(
                ok,
                "expected docs-test command to pass in {}:\n  {}\nstdout:\n{}\nstderr:\n{}",
                doc_path.display(),
                command,
                stdout,
                stderr
            );
        }
    }
}

#[test]
fn recipe_docs_have_required_sections_and_protocol_refs() {
    for doc_path in recipe_docs() {
        let text = fs::read_to_string(&doc_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", doc_path.display()));
        assert!(
            text.contains("## Protocol Example"),
            "missing protocol section in {}",
            doc_path.display()
        );
        assert!(
            text.contains("## Fallback Behavior"),
            "missing fallback section in {}",
            doc_path.display()
        );
        assert!(
            text.contains("examples/agent/protocol_") || text.contains("aic contract --json"),
            "missing protocol fixture references in {}",
            doc_path.display()
        );
        let commands = extract_docs_test_commands(&text);
        assert!(
            !commands.is_empty(),
            "missing docs-test command block in {}",
            doc_path.display()
        );
    }
}

#[test]
fn recipe_docs_tests_are_executable() {
    let root = repo_root();
    fs::create_dir_all(root.join("target/agent-recipes")).expect("mkdir target/agent-recipes");

    for doc_path in recipe_docs() {
        let text = fs::read_to_string(&doc_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", doc_path.display()));
        for (expect_failure, command) in extract_docs_test_commands(&text) {
            run_docs_test_commands(&doc_path, &[(expect_failure, command)]);
        }
    }
}

#[test]
fn docs_recipe_directory_contains_expected_files() {
    let root = repo_root();
    for path in [
        "docs/agent-recipes/README.md",
        "docs/agent-recipes/feature-loop.md",
        "docs/agent-recipes/bugfix-loop.md",
        "docs/agent-recipes/refactor-loop.md",
        "docs/agent-recipes/diagnostics-loop.md",
    ] {
        assert!(
            Path::new(&root.join(path)).is_file(),
            "missing recipe doc {}",
            path
        );
    }
}

#[test]
fn rest_guide_has_required_sections_and_policy() {
    let doc_path = rest_guide_doc();
    let text = fs::read_to_string(&doc_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", doc_path.display()));
    for required in [
        "## 1. Non-Negotiable Delivery Rules",
        "## 2. Architecture Map",
        "## 3. Where To Change What",
        "## 4. Deterministic End-To-End Workflow",
        "## 5. Diagnostics Cookbook (REST-Focused)",
        "## 6. Runnable REST Example Set",
        "## 7. Agent Task Checklist (Issue Closure Gate)",
    ] {
        assert!(
            text.contains(required),
            "missing section '{}' in {}",
            required,
            doc_path.display()
        );
    }
    assert!(
        text.contains("Do not ship stubs/placeholders/dummy"),
        "missing no-stub policy in {}",
        doc_path.display()
    );
    assert!(
        text.contains("make ci"),
        "missing make ci verification gate in {}",
        doc_path.display()
    );
    assert!(
        text.contains("AGENTS.md"),
        "missing AGENTS.md alignment in {}",
        doc_path.display()
    );
}

#[test]
fn rest_guide_references_existing_paths_and_ci_examples() {
    let root = repo_root();
    let doc_path = rest_guide_doc();
    let text = fs::read_to_string(&doc_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", doc_path.display()));

    let paths = extract_tagged_lines(
        &text,
        "<!-- rest-guide:paths:start -->",
        "<!-- rest-guide:paths:end -->",
    );
    assert!(
        !paths.is_empty(),
        "missing path references in {}",
        doc_path.display()
    );
    for rel in paths {
        assert!(
            root.join(&rel).exists(),
            "guide path does not exist: {}",
            rel
        );
    }

    let examples = extract_tagged_lines(
        &text,
        "<!-- rest-guide:examples:start -->",
        "<!-- rest-guide:examples:end -->",
    );
    assert!(
        examples.len() >= 4,
        "expected at least 4 guide examples in {}",
        doc_path.display()
    );

    let ci_script = fs::read_to_string(root.join("scripts/ci/examples.sh"))
        .expect("read scripts/ci/examples.sh");
    let mut categories = Vec::new();
    for entry in examples {
        let mut parts = entry.split_whitespace();
        let category = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("");
        assert!(
            !category.is_empty() && !path.is_empty() && parts.next().is_none(),
            "invalid rest-guide example entry: '{}'",
            entry
        );
        categories.push(category.to_string());
        assert!(
            root.join(path).is_file(),
            "rest-guide example path does not exist: {}",
            path
        );
        assert!(
            ci_script.contains(&format!("\"{path}\"")),
            "example missing from examples.sh lists: {}",
            path
        );
        let run_wired = ci_script.contains(&format!("expect_run_value \"{path}\""))
            || ci_script.contains(&format!("expect_run_exit_code \"{path}\""))
            || ci_script.contains(&format!("{path}:"));
        assert!(
            run_wired,
            "example missing from runnable CI validations in examples.sh: {}",
            path
        );
    }

    for required_category in [
        "request_parsing",
        "routing",
        "json_roundtrip",
        "error_paths",
    ] {
        assert!(
            categories.iter().any(|c| c == required_category),
            "missing rest-guide example category '{}'",
            required_category
        );
    }
}

#[test]
fn rest_guide_docs_test_commands_are_executable() {
    let doc_path = rest_guide_doc();
    let text = fs::read_to_string(&doc_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", doc_path.display()));
    let commands = extract_docs_test_commands(&text);
    assert!(
        !commands.is_empty(),
        "missing docs-test command block in {}",
        doc_path.display()
    );
    run_docs_test_commands(&doc_path, &commands);
}
