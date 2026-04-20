#!/usr/bin/env python3
"""Scan active repository files for retired Rust reference path references."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


REPORT_FORMAT = "aicore-rust-reference-retirement-reference-scan-v1"
SCHEMA_VERSION = 1
DEFAULT_SCAN_ROOTS = [
    ".github/workflows",
    "docs",
    "scripts",
    "tests",
    "Makefile",
    "README.md",
]
DEFAULT_ALLOW_PATHS = {
    "docs/selfhost/rust-reference-retirement.md",
    "docs/selfhost/rust-reference-retirement.v1.json",
}
GLOB_CHARS = "*?["


def default_repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def read_json(path: Path) -> dict[str, Any]:
    parsed = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(parsed, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return parsed


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def resolve_repo_path(repo_root: Path, raw: str) -> Path:
    path = Path(raw)
    if path.is_absolute():
        return path
    return repo_root / path


def display_path(path: Path, repo_root: Path) -> str:
    resolved = path.resolve()
    try:
        return resolved.relative_to(repo_root.resolve()).as_posix()
    except ValueError:
        return resolved.as_posix()


def has_glob(pattern: str) -> bool:
    return any(char in pattern for char in GLOB_CHARS)


def token_from_pattern(pattern: str) -> str:
    if not has_glob(pattern):
        return pattern
    indexes = [pattern.index(char) for char in GLOB_CHARS if char in pattern]
    prefix = pattern[: min(indexes)]
    slash = prefix.rfind("/")
    if slash >= 0:
        return prefix[: slash + 1]
    return prefix or pattern


def tracked_files(repo_root: Path) -> list[Path]:
    try:
        completed = subprocess.run(
            ["git", "ls-files"],
            cwd=repo_root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except OSError:
        completed = None
    if completed is not None and completed.returncode == 0:
        return [repo_root / line for line in completed.stdout.splitlines() if line.strip()]
    return sorted(path for path in repo_root.rglob("*") if path.is_file())


def is_relative_to(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
    except ValueError:
        return False
    return True


def scan_file_candidates(repo_root: Path, raw_roots: list[str], allow_paths: set[str]) -> tuple[list[Path], list[str]]:
    roots = [resolve_repo_path(repo_root, raw) for raw in raw_roots]
    problems: list[str] = []
    for raw, root in zip(raw_roots, roots):
        if not root.exists():
            problems.append(f"scan root is missing: {raw}")
    candidates: list[Path] = []
    for path in tracked_files(repo_root):
        displayed = display_path(path, repo_root)
        if displayed in allow_paths:
            continue
        if any(path.resolve() == root.resolve() or (root.is_dir() and is_relative_to(path, root)) for root in roots):
            candidates.append(path)
    return sorted(set(candidates)), problems


def load_text(path: Path) -> str | None:
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def retired_class_targets(manifest: dict[str, Any], problems: list[str]) -> list[dict[str, Any]]:
    classes = manifest.get("rust_path_classes")
    if not isinstance(classes, list):
        problems.append("manifest rust_path_classes must be a list")
        return []
    targets: list[dict[str, Any]] = []
    for index, item in enumerate(classes):
        if not isinstance(item, dict):
            problems.append(f"rust_path_classes[{index}] must be an object")
            continue
        class_id = item.get("class")
        decision = item.get("retirement_decision")
        if not isinstance(class_id, str) or not class_id:
            problems.append(f"rust_path_classes[{index}].class must be a non-empty string")
            continue
        if not isinstance(decision, dict):
            problems.append(f"rust_path_classes[{index}].retirement_decision must be an object")
            continue
        if (
            decision.get("intent") != "remove-after-replacement"
            or decision.get("status") != "approved"
            or item.get("removal_allowed") is not True
        ):
            continue
        patterns = item.get("patterns")
        if not isinstance(patterns, list) or not patterns:
            problems.append(f"rust_path_classes[{index}].patterns must be a non-empty list")
            continue
        tokens: list[str] = []
        for pattern_index, pattern in enumerate(patterns):
            if not isinstance(pattern, str) or not pattern.strip():
                problems.append(f"rust_path_classes[{index}].patterns[{pattern_index}] must be a non-empty string")
                continue
            token = token_from_pattern(pattern)
            if token:
                tokens.append(token)
        targets.append(
            {
                "class": class_id,
                "patterns": patterns,
                "reference_tokens": sorted(set(tokens)),
            }
        )
    return targets


def build_report(
    repo_root: Path,
    manifest_path: Path,
    report_path: Path,
    scan_roots: list[str],
    allow_paths: set[str],
) -> dict[str, Any]:
    problems: list[str] = []
    manifest = read_json(manifest_path)
    targets = retired_class_targets(manifest, problems)
    tokens = sorted({token for target in targets for token in target["reference_tokens"]})
    candidates, scan_problems = scan_file_candidates(repo_root, scan_roots, allow_paths)
    problems.extend(scan_problems)

    findings: list[dict[str, Any]] = []
    scanned_files = 0
    skipped_binary_or_non_utf8: list[str] = []
    for path in candidates:
        text = load_text(path)
        if text is None:
            skipped_binary_or_non_utf8.append(display_path(path, repo_root))
            continue
        scanned_files += 1
        for line_number, line in enumerate(text.splitlines(), start=1):
            for token in tokens:
                if token in line:
                    findings.append(
                        {
                            "path": display_path(path, repo_root),
                            "line": line_number,
                            "token": token,
                            "line_text": line.strip(),
                        }
                    )

    return {
        "format": REPORT_FORMAT,
        "schema_version": SCHEMA_VERSION,
        "manifest": display_path(manifest_path, repo_root),
        "repo_root": display_path(repo_root, repo_root),
        "report": display_path(report_path, repo_root),
        "scan_roots": scan_roots,
        "allow_paths": sorted(allow_paths),
        "targeted_classes": targets,
        "targeted_class_count": len(targets),
        "reference_tokens": tokens,
        "reference_token_count": len(tokens),
        "candidate_files": len(candidates),
        "scanned_files": scanned_files,
        "skipped_binary_or_non_utf8": skipped_binary_or_non_utf8,
        "findings": findings,
        "problems": problems,
        "ok": not problems and not findings,
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=default_repo_root())
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("docs/selfhost/rust-reference-retirement.v1.json"),
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=Path("target/selfhost-retirement/reference-scan.json"),
    )
    parser.add_argument("--scan-root", action="append", default=[])
    parser.add_argument("--allow-path", action="append", default=[])
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    repo_root = args.repo_root.resolve()
    manifest_path = resolve_repo_path(repo_root, str(args.manifest))
    report_path = resolve_repo_path(repo_root, str(args.report))
    scan_roots = args.scan_root or DEFAULT_SCAN_ROOTS
    allow_paths = set(DEFAULT_ALLOW_PATHS)
    allow_paths.update(args.allow_path)
    try:
        report = build_report(repo_root, manifest_path, report_path, scan_roots, allow_paths)
        write_json(report_path, report)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"selfhost-retirement-reference-scan: {exc}", file=sys.stderr)
        return 1
    print(
        "selfhost-retirement-reference-scan: "
        f"ok={str(report['ok']).lower()} "
        f"targeted_classes={report['targeted_class_count']} "
        f"findings={len(report['findings'])} "
        f"problems={len(report['problems'])} "
        f"report={display_path(report_path, repo_root)}"
    )
    for problem in report["problems"]:
        print(f"problem: {problem}", file=sys.stderr)
    for finding in report["findings"]:
        print(
            f"finding: {finding['path']}:{finding['line']} references {finding['token']}",
            file=sys.stderr,
        )
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
