use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
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

fn tutorial_agent_steps_path() -> PathBuf {
    repo_root().join("docs/tutorial/agent-steps.json")
}

fn std_api_index_doc() -> PathBuf {
    repo_root().join("docs/std-api/index.md")
}

fn std_api_machine_readable_doc() -> PathBuf {
    repo_root().join("docs/std-api/machine-readable.md")
}

fn vscode_extension_manifest_path() -> PathBuf {
    repo_root().join("tools/vscode-aic/package.json")
}

fn vscode_snippets_path() -> PathBuf {
    repo_root().join("tools/vscode-aic/snippets/aic.json")
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

fn run_docs_test_command(doc_path: &Path, command: &str) -> std::process::Output {
    let root = repo_root();
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

    Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(parts)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|err| panic!("failed to execute '{}': {err}", command))
}

fn run_docs_test_commands(doc_path: &Path, commands: &[(bool, String)]) {
    for (expect_failure, command) in commands {
        let output = run_docs_test_command(doc_path, command);
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

const REQUIRED_TUTORIAL_CHAPTERS: &[(u32, &str, &str)] = &[
    (1, "01-hello-world", "docs/tutorial/01-hello-world.md"),
    (
        2,
        "02-types-and-variables",
        "docs/tutorial/02-types-and-variables.md",
    ),
    (3, "03-functions", "docs/tutorial/03-functions.md"),
    (4, "04-control-flow", "docs/tutorial/04-control-flow.md"),
    (
        5,
        "05-structs-and-enums",
        "docs/tutorial/05-structs-and-enums.md",
    ),
    (6, "06-generics", "docs/tutorial/06-generics.md"),
    (7, "07-error-handling", "docs/tutorial/07-error-handling.md"),
    (
        8,
        "08-effects-and-contracts",
        "docs/tutorial/08-effects-and-contracts.md",
    ),
    (
        9,
        "09-modules-and-packages",
        "docs/tutorial/09-modules-and-packages.md",
    ),
    (10, "10-io", "docs/tutorial/10-io.md"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TutorialAgentSteps {
    schema_version: u32,
    format: String,
    ordering: String,
    chapters: Vec<TutorialChapter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TutorialChapter {
    chapter_number: u32,
    chapter_id: String,
    chapter_file: String,
    example_files: Vec<String>,
    steps: Vec<TutorialStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TutorialStep {
    step_id: String,
    action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expect_exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expect_stdout_contains: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assertion: Option<String>,
}

fn output_dir_from_command(command: &str) -> Option<PathBuf> {
    let parts = command.split_whitespace().collect::<Vec<_>>();
    for (idx, token) in parts.iter().enumerate() {
        if *token == "--output" {
            return parts.get(idx + 1).map(PathBuf::from);
        }
        if let Some(value) = token.strip_prefix("--output=") {
            return Some(PathBuf::from(value));
        }
    }
    None
}

fn modules_from_api_json(path: &Path) -> Vec<String> {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let json: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to parse {} as JSON: {err}", path.display()));
    assert_eq!(
        json.get("schema_version").and_then(Value::as_u64),
        Some(1),
        "unexpected schema version in {}",
        path.display()
    );
    json.get("modules")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("missing modules array in {}", path.display()))
        .iter()
        .filter_map(|module| module.get("module").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn snippet_body_text(snippet: &Value) -> String {
    match snippet.get("body") {
        Some(Value::String(line)) => line.to_string(),
        Some(Value::Array(lines)) => lines
            .iter()
            .map(|line| {
                line.as_str()
                    .unwrap_or_else(|| panic!("snippet body line must be a string: {line:?}"))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => panic!("snippet body must be string or array of strings, got {other:?}"),
    }
}

fn assert_step_shape(step: &TutorialStep, chapter_file: &str) {
    match step.action.as_str() {
        "read_chapter" => {
            assert_eq!(
                step.target.as_deref(),
                Some(chapter_file),
                "read_chapter target must point to chapter file"
            );
            assert!(
                step.section
                    .as_deref()
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false),
                "read_chapter section must be non-empty"
            );
            assert!(
                step.command.is_none(),
                "read_chapter must not include command"
            );
            assert!(
                step.expect_exit_code.is_none(),
                "read_chapter must not include expect_exit_code"
            );
            assert!(
                step.expect_stdout_contains.is_none(),
                "read_chapter must not include expect_stdout_contains"
            );
            assert!(
                step.assertion.is_none(),
                "read_chapter must not include assertion"
            );
        }
        "run_command" => {
            let command = step
                .command
                .as_deref()
                .unwrap_or_else(|| panic!("run_command missing command field"));
            assert!(
                command.starts_with("aic "),
                "run_command must invoke aic, got '{}'",
                command
            );
            assert!(
                step.expect_exit_code.is_some(),
                "run_command must include expect_exit_code"
            );
            assert!(step.target.is_none(), "run_command must not include target");
            assert!(
                step.section.is_none(),
                "run_command must not include section"
            );
            assert!(
                step.assertion.is_none(),
                "run_command must not include assertion"
            );
            if let Some(stdout) = &step.expect_stdout_contains {
                assert!(
                    !stdout.is_empty() && stdout.iter().all(|entry| !entry.trim().is_empty()),
                    "expect_stdout_contains must be non-empty when present"
                );
            }
        }
        "verify_concept" => {
            assert!(
                step.assertion
                    .as_deref()
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false),
                "verify_concept must include non-empty assertion"
            );
            assert!(
                step.target.is_none(),
                "verify_concept must not include target"
            );
            assert!(
                step.section.is_none(),
                "verify_concept must not include section"
            );
            assert!(
                step.command.is_none(),
                "verify_concept must not include command"
            );
            assert!(
                step.expect_exit_code.is_none(),
                "verify_concept must not include expect_exit_code"
            );
            assert!(
                step.expect_stdout_contains.is_none(),
                "verify_concept must not include expect_stdout_contains"
            );
        }
        action => panic!("unsupported tutorial action '{}'", action),
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

#[test]
fn tutorial_chapters_and_agent_steps_contract_is_deterministic() {
    let root = repo_root();
    let tutorial_dir = root.join("docs/tutorial");
    let chapter_markdowns = fs::read_dir(&tutorial_dir)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", tutorial_dir.display()))
        .filter_map(|entry| entry.ok().map(|value| value.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .collect::<BTreeSet<_>>();

    for (_, _, chapter_path) in REQUIRED_TUTORIAL_CHAPTERS {
        let chapter_name = Path::new(chapter_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("invalid chapter path '{}'", chapter_path));
        assert!(
            chapter_markdowns.contains(chapter_name),
            "missing required tutorial chapter '{}'",
            chapter_path
        );
    }

    let steps_path = tutorial_agent_steps_path();
    let raw = fs::read_to_string(&steps_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", steps_path.display()));
    assert!(
        raw.ends_with('\n'),
        "{} must end with a newline for deterministic formatting",
        steps_path.display()
    );
    assert!(
        !raw.contains('\t'),
        "{} must use spaces instead of tabs",
        steps_path.display()
    );

    let spec: TutorialAgentSteps = serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err}", steps_path.display()));
    assert_eq!(spec.schema_version, 1, "unexpected tutorial schema version");
    assert_eq!(
        spec.format, "aicore-tutorial-agent-steps",
        "unexpected tutorial format name"
    );
    assert_eq!(
        spec.ordering, "chapter_number_ascending",
        "unexpected tutorial chapter ordering mode"
    );
    assert_eq!(
        spec.chapters.len(),
        REQUIRED_TUTORIAL_CHAPTERS.len(),
        "unexpected tutorial chapter count"
    );

    let expected_chapters = REQUIRED_TUTORIAL_CHAPTERS
        .iter()
        .map(|(num, chapter_id, chapter_file)| (*num, *chapter_id, *chapter_file))
        .collect::<Vec<_>>();
    let actual_chapters = spec
        .chapters
        .iter()
        .map(|chapter| {
            (
                chapter.chapter_number,
                chapter.chapter_id.as_str(),
                chapter.chapter_file.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual_chapters, expected_chapters,
        "tutorial chapter list does not match required chapter set"
    );

    for chapter in &spec.chapters {
        let chapter_path = root.join(&chapter.chapter_file);
        assert!(
            chapter_path.is_file(),
            "chapter file does not exist: {}",
            chapter.chapter_file
        );
        let chapter_stem = Path::new(&chapter.chapter_file)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_else(|| panic!("invalid chapter filename '{}'", chapter.chapter_file));
        assert_eq!(
            chapter.chapter_id, chapter_stem,
            "chapter_id and chapter filename must match"
        );

        assert!(
            !chapter.example_files.is_empty(),
            "chapter {} must include at least one example file",
            chapter.chapter_number
        );
        for example in &chapter.example_files {
            assert!(
                root.join(example).is_file(),
                "chapter {} example file missing: {}",
                chapter.chapter_number,
                example
            );
        }

        assert!(
            !chapter.steps.is_empty(),
            "chapter {} must contain at least one step",
            chapter.chapter_number
        );
        for (index, step) in chapter.steps.iter().enumerate() {
            let expected_step_id = format!("{:02}.{}", chapter.chapter_number, index + 1);
            assert_eq!(
                step.step_id, expected_step_id,
                "unexpected step_id ordering in chapter {}",
                chapter.chapter_number
            );
            assert_step_shape(step, &chapter.chapter_file);
        }
    }

    let canonical = format!(
        "{}\n",
        serde_json::to_string_pretty(&spec)
            .unwrap_or_else(|err| panic!("failed to serialize {}: {err}", steps_path.display()))
    );
    assert_eq!(
        raw,
        canonical,
        "{} must remain canonical pretty JSON to preserve deterministic diffs",
        steps_path.display()
    );
}

#[test]
fn std_api_docs_explain_human_and_machine_readable_outputs() {
    for doc_path in [std_api_index_doc(), std_api_machine_readable_doc()] {
        let text = fs::read_to_string(&doc_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", doc_path.display()));
        assert!(
            text.contains("aic doc"),
            "{} must document the `aic doc` generation command",
            doc_path.display()
        );
        assert!(
            text.contains("index.md"),
            "{} must describe the human-readable output file",
            doc_path.display()
        );
        assert!(
            text.contains("api.json"),
            "{} must describe the machine-readable output file",
            doc_path.display()
        );
        assert!(
            text.contains("human-readable") && text.contains("machine-readable"),
            "{} must clearly distinguish human and machine outputs",
            doc_path.display()
        );
    }
}

#[test]
fn std_api_docs_test_commands_generate_expected_files_for_module_and_std_inputs() {
    let root = repo_root();
    let doc_path = std_api_index_doc();
    let text = fs::read_to_string(&doc_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", doc_path.display()));

    let commands = extract_tagged_lines(
        &text,
        "<!-- std-api:docgen:start -->",
        "<!-- std-api:docgen:end -->",
    );
    let expected_commands = vec![
        "aic doc examples/e4/verified_abs.aic --output target/docs-contract/module-docs"
            .to_string(),
        "aic doc std/fs.aic --output target/docs-contract/std-fs-docs".to_string(),
    ];
    assert_eq!(
        commands,
        expected_commands,
        "std API docgen command list changed unexpectedly in {}",
        doc_path.display()
    );

    let expected_files = extract_tagged_lines(
        &text,
        "<!-- std-api:docgen-files:start -->",
        "<!-- std-api:docgen-files:end -->",
    );
    assert_eq!(
        expected_files,
        vec!["index.md".to_string(), "api.json".to_string()],
        "std API docgen file contract changed unexpectedly in {}",
        doc_path.display()
    );

    for command in &commands {
        let output_dir = output_dir_from_command(command)
            .unwrap_or_else(|| panic!("missing --output in docs command '{}'", command));
        let output_root = root.join(&output_dir);
        if output_root.exists() {
            fs::remove_dir_all(&output_root).unwrap_or_else(|err| {
                panic!(
                    "failed to remove stale output {}: {err}",
                    output_root.display()
                )
            });
        }

        let output = run_docs_test_command(&doc_path, command);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "expected '{}' to pass\nstdout:\n{}\nstderr:\n{}",
            command,
            stdout,
            stderr
        );

        let expected_index = output_root.join("index.md");
        let expected_index_suffix = output_dir.join("index.md");
        assert!(
            stdout.trim().starts_with("generated ")
                && stdout
                    .trim()
                    .ends_with(&expected_index_suffix.display().to_string()),
            "unexpected command output for '{}': {}",
            command,
            stdout.trim()
        );

        for expected in &expected_files {
            let path = output_root.join(expected);
            assert!(
                path.is_file(),
                "missing generated {} for command '{}'",
                path.display(),
                command
            );
        }

        let expected_module = if command.contains("examples/e4/verified_abs.aic") {
            "examples.e4.verified_abs"
        } else if command.contains("std/fs.aic") {
            "std.fs"
        } else {
            panic!("unexpected docs command '{}'", command);
        };

        let index_text = fs::read_to_string(expected_index).unwrap_or_else(|err| {
            panic!("failed to read generated index for '{}': {err}", command)
        });
        assert!(
            index_text.contains(&format!("## {expected_module}")),
            "generated index for '{}' missing module '{}'",
            command,
            expected_module
        );

        let api_modules = modules_from_api_json(&output_root.join("api.json"));
        assert!(
            api_modules.iter().any(|module| module == expected_module),
            "generated api.json for '{}' missing module '{}'",
            command,
            expected_module
        );
    }
}

#[test]
fn vscode_snippets_manifest_registers_expected_aic_prefixes() {
    let manifest_path = vscode_extension_manifest_path();
    let manifest_raw = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest: Value = serde_json::from_str(&manifest_raw)
        .unwrap_or_else(|err| panic!("failed to parse {} as JSON: {err}", manifest_path.display()));

    let snippet_entries = manifest
        .get("contributes")
        .and_then(|contributes| contributes.get("snippets"))
        .and_then(Value::as_array)
        .unwrap_or_else(|| {
            panic!(
                "missing contributes.snippets array in {}",
                manifest_path.display()
            )
        });
    assert!(
        snippet_entries.iter().any(|entry| {
            entry.get("language").and_then(Value::as_str) == Some("aic")
                && entry.get("path").and_then(Value::as_str) == Some("./snippets/aic.json")
        }),
        "{} must register ./snippets/aic.json for language \"aic\"",
        manifest_path.display()
    );

    let snippets_path = vscode_snippets_path();
    let snippets_raw = fs::read_to_string(&snippets_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", snippets_path.display()));
    let snippets_json: Value = serde_json::from_str(&snippets_raw)
        .unwrap_or_else(|err| panic!("failed to parse {} as JSON: {err}", snippets_path.display()));
    let snippets = snippets_json
        .as_object()
        .unwrap_or_else(|| panic!("{} must be a JSON object", snippets_path.display()));

    let mut prefixes = BTreeSet::new();
    for (name, snippet) in snippets {
        let prefix = snippet
            .get("prefix")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("snippet '{}' is missing string prefix", name));
        let description = snippet
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("snippet '{}' is missing string description", name));
        assert!(
            !description.trim().is_empty(),
            "snippet '{}' description must not be empty",
            name
        );
        let body = snippet_body_text(snippet);
        assert!(
            !body.trim().is_empty(),
            "snippet '{}' body must not be empty",
            name
        );
        prefixes.insert(prefix.to_string());
    }

    for expected in [
        "fn", "afn", "struct", "enum", "trait", "impl", "match", "iflet", "test", "mod", "req",
        "ens", "eff",
    ] {
        assert!(
            prefixes.contains(expected),
            "missing snippet prefix '{}' in {}",
            expected,
            snippets_path.display()
        );
    }

    let fn_snippet = snippets
        .values()
        .find(|snippet| snippet.get("prefix").and_then(Value::as_str) == Some("fn"))
        .expect("fn snippet must exist");
    let fn_body = snippet_body_text(fn_snippet);
    assert!(
        fn_body.contains("${1:name}"),
        "fn snippet must expose tabstop for function name"
    );
    assert!(
        fn_body.contains("effects {"),
        "fn snippet must include effects declaration"
    );
}

#[test]
fn vscode_snippets_example_is_listed_in_ci_and_checks() {
    let root = repo_root();
    let example_rel = "examples/vscode/snippets_showcase.aic";
    let example_path = root.join(example_rel);
    assert!(
        example_path.is_file(),
        "snippets example missing: {}",
        example_path.display()
    );

    let examples_ci_path = root.join("scripts/ci/examples.sh");
    let examples_ci = fs::read_to_string(&examples_ci_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", examples_ci_path.display()));
    assert!(
        examples_ci.contains(example_rel),
        "{} must include {} in the CI example matrix",
        examples_ci_path.display(),
        example_rel
    );

    let output = Command::new(env!("CARGO_BIN_EXE_aic"))
        .arg("check")
        .arg(example_rel)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|err| panic!("failed to execute `aic check {example_rel}`: {err}"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected `aic check {}` to pass\nstdout:\n{}\nstderr:\n{}",
        example_rel,
        stdout,
        stderr
    );
}
