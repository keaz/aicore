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
            if expect_failure {
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
