//! Repository hygiene gate.
//!
//! `make ci` and `make examples-run` must leave `git status` clean. These
//! tests pin the `.gitignore` rules for every artefact the toolchain and the
//! CI scripts write inside the checkout, check that source paths stay
//! visible to git, and reject tracked build or scratch files.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Returns `false` when the crate is built outside a git checkout (for
/// example from a packaged source archive), where these checks do not apply.
fn in_git_checkout(root: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn is_ignored(root: &Path, path: &str) -> bool {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "--quiet", "--no-index", path])
        .status()
        .expect("run git check-ignore");
    match status.code() {
        Some(0) => true,
        Some(1) => false,
        other => panic!("git check-ignore failed for {path}: {other:?}"),
    }
}

/// Paths written by `aic build`, `aic test`, `aic testgen`, the example and
/// self-host scripts, editors and the OS.
const ARTEFACT_PATHS: &[&str] = &[
    "build.json",
    "examples/e7/task_cli_tooling/build.json",
    ".aic/artifacts/exe/0123/a.out",
    "examples/e6/pkg_app/.aic-cache/util-72c0fbf599bea618/aic.toml",
    "examples/e7/task_cli_tooling/.aic-cache/state.json",
    "examples/test/.aic-replay/replay.json",
    "examples/test/test_results.json",
    "examples/e7/property_framework/test_results.json",
    "examples/io/ffi_string_view_demo/native/ffi_string_view.o",
    "tests/generated/case_1.aic",
    ".ci-local-bin/python3",
    "scripts/selfhost/__pycache__/parity.cpython-312.pyc",
    "tools/vscode-aic/.vscode-test/vscode-darwin-arm64/README.md",
    "tools/vscode-aic/src/test/fixtures/workspace/.vscode/launch.json",
    ".DS_Store",
    "examples/agent/.DS_Store",
    ".vscode/settings.json",
    ".codex/hooks.json",
    "graphify-out/GRAPH_REPORT.md",
    ".codex_tmp_parser_main.aic",
];

/// Source paths that must stay visible to git even though they resemble
/// ignored artefacts.
const SOURCE_PATHS: &[&str] = &[
    "src/main.rs",
    "std/io.aic",
    "examples/e6/std_smoke.aic",
    "examples/e6/pkg_app/aic.lock",
    "examples/agent/protocol_build.json",
    "tests/golden_tests.rs",
    "tests/selfhost/parity_manifest.json",
    "tools/vscode-aic/package.json",
    "tools/vscode-aic/src/extension.ts",
    "docs/reference/types.md",
    "Cargo.lock",
];

#[test]
fn toolchain_and_ci_artefacts_are_ignored() {
    let root = repo_root();
    if !in_git_checkout(&root) {
        eprintln!("skipping: {} is not a git checkout", root.display());
        return;
    }
    let missing = ARTEFACT_PATHS
        .iter()
        .filter(|path| !is_ignored(&root, path))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        ".gitignore does not cover these artefact paths: {missing:?}"
    );
}

#[test]
fn source_paths_are_not_ignored() {
    let root = repo_root();
    if !in_git_checkout(&root) {
        eprintln!("skipping: {} is not a git checkout", root.display());
        return;
    }
    let hidden = SOURCE_PATHS
        .iter()
        .filter(|path| is_ignored(&root, path))
        .collect::<Vec<_>>();
    assert!(
        hidden.is_empty(),
        ".gitignore hides these source paths: {hidden:?}"
    );
}

fn is_tracked_artefact(path: &str) -> bool {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    let segments = path.split('/').collect::<Vec<_>>();
    let dirs = &segments[..segments.len().saturating_sub(1)];
    matches!(
        file_name,
        "build.json" | "test_results.json" | ".DS_Store" | "Cargo.lock_bk" | "="
    ) || path == "out.txt"
        || file_name.ends_with(".o")
        || file_name.starts_with(".codex_tmp_")
        || dirs.iter().any(|dir| {
            matches!(
                *dir,
                ".aic" | ".aic-cache" | ".aic-replay" | "__pycache__" | "graphify-out"
            )
        })
}

#[test]
fn no_build_or_scratch_files_are_tracked() {
    let root = repo_root();
    if !in_git_checkout(&root) {
        eprintln!("skipping: {} is not a git checkout", root.display());
        return;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "-z"])
        .output()
        .expect("run git ls-files");
    assert!(output.status.success(), "git ls-files failed");
    let listing = String::from_utf8(output.stdout).expect("utf-8 paths");
    let tracked = listing
        .split('\0')
        .filter(|path| !path.is_empty() && is_tracked_artefact(path))
        .collect::<Vec<_>>();
    assert!(
        tracked.is_empty(),
        "build or scratch files are tracked; remove them with `git rm --cached`: {tracked:?}"
    );
}

#[test]
fn artefact_classifier_matches_expected_paths() {
    for path in [
        "build.json",
        "examples/test/test_results.json",
        "examples/io/native/demo.o",
        "examples/e6/pkg_app/.aic-cache/util-1/aic.toml",
        "examples/test/.aic-replay/a.json",
        ".aic/artifacts/exe/a.out",
        "scripts/selfhost/__pycache__/x.pyc",
        "compiler/.DS_Store",
        ".codex_tmp_driver_main.aic",
        "out.txt",
        "=",
        "Cargo.lock_bk",
    ] {
        assert!(is_tracked_artefact(path), "{path} should be an artefact");
    }
    for path in [
        "examples/agent/protocol_build.json",
        "examples/io/out.txt.md",
        "docs/out.txt",
        "src/codegen/runtime/io.c",
        "std/io.aic",
        "examples/e6/pkg_app/aic.lock",
        "tests/golden/aic_cache.aic",
    ] {
        assert!(!is_tracked_artefact(path), "{path} should be source");
    }
}
