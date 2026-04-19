#!/usr/bin/env python3
"""Audit Rust reference compiler retirement readiness."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


MANIFEST_FORMAT = "aicore-rust-reference-retirement-v1"
REPORT_FORMAT = "aicore-rust-reference-retirement-audit-v1"
SCHEMA_VERSION = 1
STATUSES = {"deferred", "approved", "retired"}
GLOB_CHARS = "*?["


def default_repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def read_json(path: Path) -> dict[str, Any]:
    try:
        parsed = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise ValueError(f"{path}: invalid JSON: {exc}") from exc
    if not isinstance(parsed, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return parsed


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def display_path(path: Path, repo_root: Path) -> str:
    resolved = path.resolve()
    try:
        return resolved.relative_to(repo_root.resolve()).as_posix()
    except ValueError:
        return resolved.as_posix()


def resolve_repo_path(repo_root: Path, raw: str) -> Path:
    path = Path(raw)
    if path.is_absolute():
        return path
    return repo_root / path


def has_glob(pattern: str) -> bool:
    return any(char in pattern for char in GLOB_CHARS)


def expand_pattern(repo_root: Path, pattern: str) -> list[Path]:
    if has_glob(pattern):
        return sorted(path for path in repo_root.glob(pattern) if path.is_file())
    path = resolve_repo_path(repo_root, pattern)
    if path.exists():
        return [path]
    return []


def string_list(value: Any, field: str, problems: list[str], *, required: bool = True) -> list[str]:
    if not isinstance(value, list):
        problems.append(f"{field} must be a list")
        return []
    if required and not value:
        problems.append(f"{field} must not be empty")
        return []
    items: list[str] = []
    for index, item in enumerate(value):
        if not isinstance(item, str) or item.strip() == "":
            problems.append(f"{field}[{index}] must be a non-empty string")
            continue
        items.append(item)
    return items


def object_list(value: Any, field: str, problems: list[str]) -> list[dict[str, Any]]:
    if not isinstance(value, list) or not value:
        problems.append(f"{field} must be a non-empty list")
        return []
    items: list[dict[str, Any]] = []
    for index, item in enumerate(value):
        if not isinstance(item, dict):
            problems.append(f"{field}[{index}] must be an object")
            continue
        items.append(item)
    return items


def object_field(value: Any, field: str, problems: list[str]) -> dict[str, Any]:
    if not isinstance(value, dict):
        problems.append(f"{field} must be an object")
        return {}
    return value


def required_string(value: Any, field: str, problems: list[str]) -> str:
    if not isinstance(value, str) or value.strip() == "":
        problems.append(f"{field} must be a non-empty string")
        return ""
    return value


def required_bool(value: Any, field: str, problems: list[str]) -> bool:
    if not isinstance(value, bool):
        problems.append(f"{field} must be a boolean")
        return False
    return value


def required_positive_int(value: Any, field: str, problems: list[str]) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 1:
        problems.append(f"{field} must be a positive integer")
        return 0
    return value


def tracked_rust_inventory(repo_root: Path) -> list[str]:
    try:
        completed = subprocess.run(
            ["git", "ls-files", "*.rs", "Cargo.toml", "Cargo.lock"],
            cwd=repo_root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except OSError:
        completed = None
    if completed is not None and completed.returncode == 0:
        return sorted(line for line in completed.stdout.splitlines() if line.strip())
    paths = [path for path in repo_root.glob("src/**/*.rs")]
    paths.extend(repo_root.glob("tests/**/*.rs"))
    for name in ("Cargo.toml", "Cargo.lock"):
        path = repo_root / name
        if path.exists():
            paths.append(path)
    return sorted(display_path(path, repo_root) for path in paths if path.is_file())


def audit_active_paths(
    manifest: dict[str, Any],
    repo_root: Path,
    problems: list[str],
) -> list[dict[str, Any]]:
    active: list[dict[str, Any]] = []
    for raw in string_list(manifest.get("required_active_paths"), "required_active_paths", problems):
        path = resolve_repo_path(repo_root, raw)
        exists = path.exists()
        if not exists:
            problems.append(f"required active path is missing: {raw}")
        active.append(
            {
                "path": raw,
                "exists": exists,
                "kind": "directory" if path.is_dir() else "file" if path.is_file() else "missing",
            }
        )
    return active


def audit_docs(
    manifest: dict[str, Any],
    repo_root: Path,
    problems: list[str],
) -> list[dict[str, Any]]:
    docs: list[dict[str, Any]] = []
    for index, item in enumerate(object_list(manifest.get("required_docs_tokens"), "required_docs_tokens", problems)):
        doc_path = required_string(item.get("path"), f"required_docs_tokens[{index}].path", problems)
        tokens = string_list(
            item.get("tokens"),
            f"required_docs_tokens[{index}].tokens",
            problems,
        )
        path = resolve_repo_path(repo_root, doc_path)
        missing_tokens: list[str] = []
        exists = path.is_file()
        if exists:
            contents = path.read_text(encoding="utf-8")
            missing_tokens = [token for token in tokens if token not in contents]
            for token in missing_tokens:
                problems.append(f"{doc_path} missing required token: {token}")
        else:
            problems.append(f"required document is missing: {doc_path}")
            missing_tokens = tokens
        docs.append(
            {
                "path": doc_path,
                "exists": exists,
                "missing_tokens": missing_tokens,
                "token_count": len(tokens),
            }
        )
    return docs


def audit_classes(
    manifest: dict[str, Any],
    repo_root: Path,
    problems: list[str],
) -> tuple[list[dict[str, Any]], set[str]]:
    classes: list[dict[str, Any]] = []
    classified_paths: set[str] = set()
    seen_classes: set[str] = set()
    for index, item in enumerate(object_list(manifest.get("rust_path_classes"), "rust_path_classes", problems)):
        class_id = required_string(item.get("class"), f"rust_path_classes[{index}].class", problems)
        if class_id in seen_classes:
            problems.append(f"duplicate rust path class: {class_id}")
        seen_classes.add(class_id)
        description = required_string(
            item.get("description"), f"rust_path_classes[{index}].description", problems
        )
        replacement_owner = required_string(
            item.get("replacement_owner"), f"rust_path_classes[{index}].replacement_owner", problems
        )
        removal_allowed = required_bool(
            item.get("removal_allowed"), f"rust_path_classes[{index}].removal_allowed", problems
        )
        patterns = string_list(item.get("patterns"), f"rust_path_classes[{index}].patterns", problems)
        evidence = string_list(
            item.get("required_replacement_evidence", []),
            f"rust_path_classes[{index}].required_replacement_evidence",
            problems,
            required=False,
        )
        matched: list[str] = []
        for pattern in patterns:
            matches = expand_pattern(repo_root, pattern)
            if not matches:
                problems.append(f"rust path class {class_id} pattern matched no files: {pattern}")
            for path in matches:
                displayed = display_path(path, repo_root)
                matched.append(displayed)
                classified_paths.add(displayed)
        matched = sorted(set(matched))
        if not matched:
            problems.append(f"rust path class {class_id} has no matched files")
        classes.append(
            {
                "class": class_id,
                "description": description,
                "replacement_owner": replacement_owner,
                "removal_allowed": removal_allowed,
                "patterns": patterns,
                "matched_paths": matched,
                "required_replacement_evidence": evidence,
            }
        )
    return classes, classified_paths


def audit_bake_in(
    manifest: dict[str, Any],
    problems: list[str],
) -> tuple[dict[str, Any], list[str]]:
    bake_in = object_field(manifest.get("bake_in"), "bake_in", problems)
    required_runs = required_positive_int(
        bake_in.get("required_release_preflight_runs"),
        "bake_in.required_release_preflight_runs",
        problems,
    )
    required_platforms = string_list(
        bake_in.get("required_platforms"),
        "bake_in.required_platforms",
        problems,
    )
    raw_evidence = bake_in.get("evidence", [])
    evidence: list[dict[str, Any]] = []
    if not isinstance(raw_evidence, list):
        problems.append("bake_in.evidence must be a list")
    else:
        for index, item in enumerate(raw_evidence):
            if not isinstance(item, dict):
                problems.append(f"bake_in.evidence[{index}] must be an object")
                continue
            evidence.append(item)
    passed: list[dict[str, Any]] = []
    for index, item in enumerate(evidence):
        platform = required_string(item.get("platform"), f"bake_in.evidence[{index}].platform", problems)
        status = required_string(item.get("status"), f"bake_in.evidence[{index}].status", problems)
        report = required_string(item.get("report"), f"bake_in.evidence[{index}].report", problems)
        if status not in {"passed", "failed"}:
            problems.append(f"bake_in.evidence[{index}].status must be passed or failed")
        if platform and status == "passed" and report:
            passed.append(item)
    platforms_with_passes = sorted(
        {item["platform"] for item in passed if isinstance(item.get("platform"), str)}
    )
    blockers: list[str] = []
    if len(passed) < required_runs:
        blockers.append(
            f"bake-in requires {required_runs} passing release preflight run(s); recorded {len(passed)}"
        )
    for platform in required_platforms:
        if platform not in platforms_with_passes:
            blockers.append(f"bake-in requires passing release evidence for {platform}")
    return (
        {
            "required_release_preflight_runs": required_runs,
            "required_platforms": required_platforms,
            "passed_release_preflight_runs": len(passed),
            "platforms_with_passes": platforms_with_passes,
            "evidence_count": len(evidence),
        },
        blockers,
    )


def build_report(manifest_path: Path, repo_root: Path) -> dict[str, Any]:
    problems: list[str] = []
    manifest = read_json(manifest_path)
    if manifest.get("format") != MANIFEST_FORMAT:
        problems.append(f"manifest format must be {MANIFEST_FORMAT}")
    if manifest.get("schema_version") != SCHEMA_VERSION:
        problems.append(f"manifest schema_version must be {SCHEMA_VERSION}")
    status = required_string(manifest.get("status"), "status", problems)
    if status and status not in STATUSES:
        problems.append(f"status must be one of {sorted(STATUSES)}")
    decision_record = required_string(manifest.get("decision_record"), "decision_record", problems)
    if decision_record:
        decision_path = resolve_repo_path(repo_root, decision_record)
        if not decision_path.is_file():
            problems.append(f"decision record is missing: {decision_record}")

    controlled_default_issue = manifest.get("controlled_default_issue")
    retirement_issue = manifest.get("retirement_issue")
    if controlled_default_issue != 418:
        problems.append("controlled_default_issue must be 418")
    if retirement_issue != 419:
        problems.append("retirement_issue must be 419")

    approval = object_field(manifest.get("approval"), "approval", problems)
    approval_required = required_bool(approval.get("required"), "approval.required", problems)
    approval_granted = required_bool(approval.get("approved"), "approval.approved", problems)
    approver = approval.get("approver")
    if approval_granted and (not isinstance(approver, str) or approver.strip() == ""):
        problems.append("approval.approver must be recorded when approval.approved is true")

    active_paths = audit_active_paths(manifest, repo_root, problems)
    docs = audit_docs(manifest, repo_root, problems)
    classes, classified_paths = audit_classes(manifest, repo_root, problems)
    bake_in, bake_in_blockers = audit_bake_in(manifest, problems)

    tracked = tracked_rust_inventory(repo_root)
    unclassified = sorted(path for path in tracked if path not in classified_paths)
    for path in unclassified:
        problems.append(f"tracked Rust or Cargo path is not classified: {path}")

    rollback_commands = string_list(
        manifest.get("rollback_commands"),
        "rollback_commands",
        problems,
    )

    blockers: list[str] = []
    if status != "approved":
        blockers.append(f"retirement decision status is {status}; approved is required for removal")
    if approval_required and not approval_granted:
        blockers.append("approval required before Rust reference removal")
    blockers.extend(bake_in_blockers)
    for item in classes:
        if item["removal_allowed"] is not True:
            blockers.append(f"{item['class']} is still marked removal_allowed=false")

    removal_allowed = not problems and not blockers
    return {
        "format": REPORT_FORMAT,
        "schema_version": SCHEMA_VERSION,
        "manifest": display_path(manifest_path, repo_root),
        "status": status,
        "controlled_default_issue": controlled_default_issue,
        "retirement_issue": retirement_issue,
        "decision_record": decision_record,
        "approval": {
            "required": approval_required,
            "approved": approval_granted,
            "approver": approver if isinstance(approver, str) else None,
        },
        "bake_in": bake_in,
        "rust_path_classes": classes,
        "tracked_rust_inventory": {
            "total": len(tracked),
            "classified": len(sorted(classified_paths.intersection(tracked))),
            "unclassified": unclassified,
        },
        "required_active_paths": active_paths,
        "docs": docs,
        "rollback_commands": rollback_commands,
        "problems": problems,
        "blockers": blockers,
        "removal_allowed": removal_allowed,
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=default_repo_root(),
        help="repository root; defaults to the current script location",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("docs/selfhost/rust-reference-retirement.v1.json"),
        help="retirement inventory manifest",
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=Path("target/selfhost-retirement/report.json"),
        help="path for the audit report",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail only when the manifest or inventory is inconsistent",
    )
    parser.add_argument(
        "--require-approved",
        action="store_true",
        help="also fail while retirement blockers remain",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    repo_root = args.repo_root.resolve()
    manifest_path = resolve_repo_path(repo_root, str(args.manifest))
    report_path = resolve_repo_path(repo_root, str(args.report))
    try:
        report = build_report(manifest_path, repo_root)
    except (OSError, ValueError) as exc:
        print(f"selfhost-retirement-audit: {exc}", file=sys.stderr)
        return 1

    write_json(report_path, report)
    print(
        "selfhost-retirement-audit: "
        f"status={report['status']} "
        f"removal_allowed={str(report['removal_allowed']).lower()} "
        f"problems={len(report['problems'])} "
        f"blockers={len(report['blockers'])} "
        f"report={display_path(report_path, repo_root)}"
    )
    for problem in report["problems"]:
        print(f"problem: {problem}", file=sys.stderr)
    if report["problems"]:
        return 1
    if args.require_approved and report["blockers"]:
        print("Rust reference retirement is blocked:", file=sys.stderr)
        for blocker in report["blockers"]:
            print(f"- {blocker}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
