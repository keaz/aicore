#!/usr/bin/env python3
"""Audit Rust reference compiler retirement readiness."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


MANIFEST_FORMAT = "aicore-rust-reference-retirement-v1"
REPORT_FORMAT = "aicore-rust-reference-retirement-audit-v1"
BOOTSTRAP_FORMAT = "aicore-selfhost-bootstrap-v1"
PROVENANCE_FORMAT = "aicore-selfhost-release-provenance-v1"
SCHEMA_VERSION = 1
STATUSES = {"deferred", "approved", "retired"}
CLASS_DECISION_INTENTS = {"remove-after-replacement", "retain-non-reference"}
CLASS_DECISION_STATUSES = {"pending", "approved"}
GLOB_CHARS = "*?["
RELEASE_PREFLIGHT_COMMAND = "make release-preflight"
CI_COMMAND = "make ci"
ROLLBACK_FETCH_COMMAND = "git fetch --tags origin"
ROLLBACK_CHECKOUT_COMMAND = "git checkout <last-rust-reference-tag> -- Cargo.toml Cargo.lock src tests"
ROLLBACK_BUILD_COMMAND = "cargo build --locked"
ROLLBACK_AUDIT_COMMAND = "make selfhost-retirement-audit"
ROLLBACK_REQUIRED_COMMANDS = [
    ROLLBACK_FETCH_COMMAND,
    ROLLBACK_CHECKOUT_COMMAND,
    ROLLBACK_BUILD_COMMAND,
    ROLLBACK_AUDIT_COMMAND,
]


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


def sha256_prefixed(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def platform_key(value: str) -> str:
    normalized = value.strip().lower()
    if normalized == "darwin":
        return "macos"
    if normalized.startswith("linux"):
        return "linux"
    return normalized


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


def resolve_evidence_path(evidence_root: Path, raw: str) -> Path:
    path = Path(raw)
    if path.is_absolute():
        return path
    return evidence_root / path


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


def optional_string(value: Any, field: str, problems: list[str]) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str) or value.strip() == "":
        problems.append(f"{field} must be null or a non-empty string")
        return None
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


def verify_sha256(path: Path, expected: str, field: str, problems: list[str]) -> bool:
    if not isinstance(expected, str) or not expected.startswith("sha256:"):
        problems.append(f"{field} must be a sha256: digest")
        return False
    if not path.is_file():
        problems.append(f"{field} target is missing: {path}")
        return False
    actual = sha256_prefixed(path)
    if actual != expected:
        problems.append(f"{field} mismatch for {path}: expected {expected}, found {actual}")
        return False
    return True


def looks_like_commit(value: str) -> bool:
    return 7 <= len(value) <= 64 and all(char in "0123456789abcdefABCDEF" for char in value)


def has_restore_checkout_command(commands: list[str], source_ref: str, restore_paths: list[str]) -> bool:
    prefix = f"git checkout {source_ref} -- "
    for command in commands:
        if not command.startswith(prefix):
            continue
        restored = command[len(prefix) :].split()
        if all(path in restored for path in restore_paths):
            return True
    return False


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


def validate_class_decision_evidence(
    item: dict[str, Any],
    class_index: int,
    evidence_index: int,
    evidence_root: Path,
    allowed_commands: list[str],
    problems: list[str],
) -> tuple[dict[str, Any], bool]:
    prefix = f"rust_path_classes[{class_index}].retirement_decision.evidence[{evidence_index}]"
    command = required_string(item.get("command"), f"{prefix}.command", problems)
    recorded_at = required_string(item.get("recorded_at"), f"{prefix}.recorded_at", problems)
    report_raw = required_string(item.get("report"), f"{prefix}.report", problems)
    report_sha = required_string(item.get("report_sha256"), f"{prefix}.report_sha256", problems)
    if command and command not in allowed_commands:
        problems.append(f"{prefix}.command is not listed in required_replacement_evidence")
    sha_ok = False
    if report_raw and report_sha:
        sha_ok = verify_sha256(
            resolve_evidence_path(evidence_root, report_raw),
            report_sha,
            f"{prefix}.report_sha256",
            problems,
        )
    valid = bool(command) and command in allowed_commands and bool(recorded_at) and sha_ok
    return (
        {
            "command": command,
            "recorded_at": recorded_at,
            "report": report_raw,
            "sha256_ok": sha_ok,
            "valid": valid,
        },
        valid,
    )


def audit_class_decision(
    item: dict[str, Any],
    index: int,
    evidence_root: Path,
    class_id: str,
    removal_allowed: bool,
    required_evidence: list[str],
    problems: list[str],
) -> tuple[dict[str, Any], list[str]]:
    field = f"rust_path_classes[{index}].retirement_decision"
    decision = object_field(item.get("retirement_decision"), field, problems)
    intent = required_string(decision.get("intent"), f"{field}.intent", problems)
    status = required_string(decision.get("status"), f"{field}.status", problems)
    if intent and intent not in CLASS_DECISION_INTENTS:
        problems.append(f"{field}.intent must be one of {sorted(CLASS_DECISION_INTENTS)}")
    if status and status not in CLASS_DECISION_STATUSES:
        problems.append(f"{field}.status must be one of {sorted(CLASS_DECISION_STATUSES)}")
    non_reference_role = optional_string(decision.get("non_reference_role"), f"{field}.non_reference_role", problems)
    if intent == "retain-non-reference" and not non_reference_role:
        problems.append(f"{field}.non_reference_role must be recorded for retained Rust classes")

    raw_evidence = decision.get("evidence", [])
    evidence: list[dict[str, Any]] = []
    if not isinstance(raw_evidence, list):
        problems.append(f"{field}.evidence must be a list")
    else:
        for evidence_index, evidence_item in enumerate(raw_evidence):
            if not isinstance(evidence_item, dict):
                problems.append(f"{field}.evidence[{evidence_index}] must be an object")
                continue
            evidence.append(evidence_item)

    summaries: list[dict[str, Any]] = []
    valid_commands: set[str] = set()
    for evidence_index, evidence_item in enumerate(evidence):
        summary, valid = validate_class_decision_evidence(
            evidence_item,
            index,
            evidence_index,
            evidence_root,
            required_evidence,
            problems,
        )
        summaries.append(summary)
        if valid:
            valid_commands.add(summary["command"])

    missing_evidence = [command for command in required_evidence if command not in valid_commands]
    if status == "approved":
        if not required_evidence:
            problems.append(f"{field}.status cannot be approved without required_replacement_evidence")
        if missing_evidence:
            problems.append(f"{field}.evidence is missing approved command evidence: {', '.join(missing_evidence)}")
        if intent == "remove-after-replacement" and removal_allowed is not True:
            problems.append(f"{field}.removal_allowed must be true when replacement removal is approved")

    blockers: list[str] = []
    if status != "approved":
        if intent == "retain-non-reference":
            blockers.append(f"{class_id} retained Rust role is not approved")
        elif intent == "remove-after-replacement":
            blockers.append(f"{class_id} replacement/removal decision is pending")
        else:
            blockers.append(f"{class_id} retirement decision is not approved")

    return (
        {
            "intent": intent,
            "status": status,
            "non_reference_role": non_reference_role,
            "evidence_count": len(evidence),
            "valid_evidence_commands": sorted(valid_commands),
            "missing_evidence": missing_evidence,
            "evidence": summaries,
        },
        blockers,
    )


def audit_classes(
    manifest: dict[str, Any],
    repo_root: Path,
    evidence_root: Path,
    problems: list[str],
) -> tuple[list[dict[str, Any]], set[str], list[str]]:
    classes: list[dict[str, Any]] = []
    classified_paths: set[str] = set()
    blockers: list[str] = []
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
        retirement_decision, class_blockers = audit_class_decision(
            item,
            index,
            evidence_root,
            class_id,
            removal_allowed,
            evidence,
            problems,
        )
        blockers.extend(class_blockers)
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
                "retirement_decision": retirement_decision,
            }
        )
    return classes, classified_paths, blockers


def validate_bootstrap_evidence(
    item: dict[str, Any],
    index: int,
    evidence_root: Path,
    evidence_platform: str,
    source_commit: str,
    problems: list[str],
) -> dict[str, Any]:
    raw_path = required_string(item.get("bootstrap_report"), f"bake_in.evidence[{index}].bootstrap_report", problems)
    expected_sha = required_string(
        item.get("bootstrap_report_sha256"),
        f"bake_in.evidence[{index}].bootstrap_report_sha256",
        problems,
    )
    result = {
        "path": raw_path,
        "exists": False,
        "sha256_ok": False,
        "ready": False,
        "status": None,
        "platform": None,
        "budget_overrides_ok": False,
    }
    if not raw_path:
        return result
    path = resolve_evidence_path(evidence_root, raw_path)
    result["exists"] = path.is_file()
    if expected_sha:
        result["sha256_ok"] = verify_sha256(
            path, expected_sha, f"bake_in.evidence[{index}].bootstrap_report_sha256", problems
        )
    if not path.is_file():
        problems.append(f"bake_in.evidence[{index}].bootstrap_report is missing: {raw_path}")
        return result
    try:
        report = read_json(path)
    except (OSError, ValueError) as exc:
        problems.append(f"bake_in.evidence[{index}].bootstrap_report is invalid: {exc}")
        return result
    if report.get("format") != BOOTSTRAP_FORMAT:
        problems.append(f"bake_in.evidence[{index}].bootstrap_report format must be {BOOTSTRAP_FORMAT}")
    result["ready"] = report.get("ready") is True
    result["status"] = report.get("status")
    if result["status"] != "supported-ready" or result["ready"] is not True:
        problems.append(f"bake_in.evidence[{index}].bootstrap_report must be supported-ready")
    host = report.get("host")
    host_platform = host.get("platform") if isinstance(host, dict) else None
    normalized_host = platform_key(host_platform) if isinstance(host_platform, str) else ""
    result["platform"] = normalized_host or None
    if normalized_host != evidence_platform:
        problems.append(
            f"bake_in.evidence[{index}].bootstrap_report platform {host_platform!r} "
            f"does not match evidence platform {evidence_platform!r}"
        )
    performance = report.get("performance")
    budget_source = performance.get("budget_source") if isinstance(performance, dict) else None
    overrides = budget_source.get("overrides") if isinstance(budget_source, dict) else None
    result["budget_overrides_ok"] = overrides == {}
    if overrides != {}:
        problems.append(f"bake_in.evidence[{index}].bootstrap_report must use production budget defaults")
    if source_commit:
        result["source_commit"] = source_commit
    return result


def validate_provenance_evidence(
    item: dict[str, Any],
    index: int,
    evidence_root: Path,
    evidence_platform: str,
    source_commit: str,
    problems: list[str],
) -> dict[str, Any]:
    raw_path = required_string(
        item.get("release_provenance"),
        f"bake_in.evidence[{index}].release_provenance",
        problems,
    )
    expected_sha = required_string(
        item.get("release_provenance_sha256"),
        f"bake_in.evidence[{index}].release_provenance_sha256",
        problems,
    )
    result = {
        "path": raw_path,
        "exists": False,
        "sha256_ok": False,
        "source_commit_ok": False,
        "worktree_clean": False,
        "validation_ok": False,
        "canonical_artifact_ok": False,
    }
    if not raw_path:
        return result
    path = resolve_evidence_path(evidence_root, raw_path)
    result["exists"] = path.is_file()
    if expected_sha:
        result["sha256_ok"] = verify_sha256(
            path, expected_sha, f"bake_in.evidence[{index}].release_provenance_sha256", problems
        )
    if not path.is_file():
        problems.append(f"bake_in.evidence[{index}].release_provenance is missing: {raw_path}")
        return result
    try:
        report = read_json(path)
    except (OSError, ValueError) as exc:
        problems.append(f"bake_in.evidence[{index}].release_provenance is invalid: {exc}")
        return result
    if report.get("format") != PROVENANCE_FORMAT:
        problems.append(f"bake_in.evidence[{index}].release_provenance format must be {PROVENANCE_FORMAT}")
    source = report.get("source")
    recorded_commit = source.get("commit") if isinstance(source, dict) else None
    result["source_commit_ok"] = recorded_commit == source_commit
    if recorded_commit != source_commit:
        problems.append(
            f"bake_in.evidence[{index}].release_provenance source commit does not match evidence source_commit"
        )
    result["worktree_clean"] = isinstance(source, dict) and source.get("worktree_dirty") is False
    if result["worktree_clean"] is not True:
        problems.append(f"bake_in.evidence[{index}].release_provenance must record a clean worktree")
    host = report.get("host")
    host_platform = host.get("platform") if isinstance(host, dict) else None
    normalized_host = platform_key(host_platform) if isinstance(host_platform, str) else ""
    if normalized_host != evidence_platform:
        problems.append(
            f"bake_in.evidence[{index}].release_provenance platform {host_platform!r} "
            f"does not match evidence platform {evidence_platform!r}"
        )
    validation = report.get("validation")
    validation_ok = isinstance(validation, dict) and all(
        validation.get(key) is True
        for key in ("bootstrap_ready", "parity_ok", "stage_matrix_ok", "performance_ok")
    )
    validation_ok = validation_ok and isinstance(validation, dict) and validation.get("budget_overrides") == {}
    result["validation_ok"] = validation_ok
    if not validation_ok:
        problems.append(f"bake_in.evidence[{index}].release_provenance validation fields must all pass")
    artifact = report.get("canonical_artifact")
    artifact_path_raw = artifact.get("path") if isinstance(artifact, dict) else None
    artifact_sha = artifact.get("sha256") if isinstance(artifact, dict) else None
    if isinstance(artifact_path_raw, str) and isinstance(artifact_sha, str):
        artifact_path = resolve_evidence_path(evidence_root, artifact_path_raw)
        result["canonical_artifact_ok"] = verify_sha256(
            artifact_path,
            artifact_sha,
            f"bake_in.evidence[{index}].release_provenance.canonical_artifact.sha256",
            problems,
        )
    else:
        problems.append(f"bake_in.evidence[{index}].release_provenance missing canonical artifact")
    return result


def validate_default_build_evidence(
    item: dict[str, Any],
    index: int,
    evidence_root: Path,
    problems: list[str],
) -> dict[str, Any]:
    raw_path = required_string(
        item.get("default_build_artifact"),
        f"bake_in.evidence[{index}].default_build_artifact",
        problems,
    )
    expected_sha = required_string(
        item.get("default_build_sha256"),
        f"bake_in.evidence[{index}].default_build_sha256",
        problems,
    )
    result = {"path": raw_path, "exists": False, "sha256_ok": False}
    if not raw_path:
        return result
    path = resolve_evidence_path(evidence_root, raw_path)
    result["exists"] = path.is_file()
    if expected_sha:
        result["sha256_ok"] = verify_sha256(
            path, expected_sha, f"bake_in.evidence[{index}].default_build_sha256", problems
        )
    if not path.is_file():
        problems.append(f"bake_in.evidence[{index}].default_build_artifact is missing: {raw_path}")
    return result


def audit_bake_in_evidence(
    item: dict[str, Any],
    index: int,
    evidence_root: Path,
    required_platforms: list[str],
    problems: list[str],
) -> tuple[dict[str, Any], bool]:
    platform = required_string(item.get("platform"), f"bake_in.evidence[{index}].platform", problems)
    evidence_platform = platform_key(platform) if platform else ""
    status = required_string(item.get("status"), f"bake_in.evidence[{index}].status", problems)
    source_commit = required_string(
        item.get("source_commit"),
        f"bake_in.evidence[{index}].source_commit",
        problems,
    )
    recorded_at = required_string(item.get("recorded_at"), f"bake_in.evidence[{index}].recorded_at", problems)
    release_command = required_string(
        item.get("release_preflight_command"),
        f"bake_in.evidence[{index}].release_preflight_command",
        problems,
    )
    ci_command = required_string(item.get("ci_command"), f"bake_in.evidence[{index}].ci_command", problems)
    if status not in {"passed", "failed"}:
        problems.append(f"bake_in.evidence[{index}].status must be passed or failed")
    if evidence_platform and evidence_platform not in required_platforms:
        problems.append(f"bake_in.evidence[{index}].platform is not in bake_in.required_platforms")
    if release_command and release_command != RELEASE_PREFLIGHT_COMMAND:
        problems.append(f"bake_in.evidence[{index}].release_preflight_command must be `{RELEASE_PREFLIGHT_COMMAND}`")
    if ci_command and ci_command != CI_COMMAND:
        problems.append(f"bake_in.evidence[{index}].ci_command must be `{CI_COMMAND}`")
    if status == "failed":
        failure_summary = required_string(
            item.get("failure_summary"),
            f"bake_in.evidence[{index}].failure_summary",
            problems,
        )
        return (
            {
                "platform": evidence_platform,
                "status": status,
                "source_commit": source_commit,
                "recorded_at": recorded_at,
                "release_preflight_command": release_command,
                "ci_command": ci_command,
                "failure_summary": failure_summary,
                "valid_for_bake_in": False,
            },
            False,
        )

    bootstrap = validate_bootstrap_evidence(item, index, evidence_root, evidence_platform, source_commit, problems)
    provenance = validate_provenance_evidence(item, index, evidence_root, evidence_platform, source_commit, problems)
    default_build = validate_default_build_evidence(item, index, evidence_root, problems)
    valid = (
        status == "passed"
        and evidence_platform in required_platforms
        and release_command == RELEASE_PREFLIGHT_COMMAND
        and ci_command == CI_COMMAND
        and bootstrap.get("sha256_ok") is True
        and bootstrap.get("ready") is True
        and bootstrap.get("status") == "supported-ready"
        and bootstrap.get("budget_overrides_ok") is True
        and provenance.get("sha256_ok") is True
        and provenance.get("source_commit_ok") is True
        and provenance.get("worktree_clean") is True
        and provenance.get("validation_ok") is True
        and provenance.get("canonical_artifact_ok") is True
        and default_build.get("sha256_ok") is True
    )
    return (
        {
            "platform": evidence_platform,
            "status": status,
            "source_commit": source_commit,
            "recorded_at": recorded_at,
            "release_preflight_command": release_command,
            "ci_command": ci_command,
            "bootstrap": bootstrap,
            "release_provenance": provenance,
            "default_build": default_build,
            "valid_for_bake_in": valid,
        },
        valid,
    )


def audit_bake_in(
    manifest: dict[str, Any],
    evidence_root: Path,
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
    summaries: list[dict[str, Any]] = []
    passed: list[dict[str, Any]] = []
    for index, item in enumerate(evidence):
        summary, valid = audit_bake_in_evidence(item, index, evidence_root, required_platforms, problems)
        summaries.append(summary)
        if valid:
            passed.append(summary)
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
            "evidence": summaries,
        },
        blockers,
    )


def validate_rollback_evidence(
    item: dict[str, Any],
    index: int,
    evidence_root: Path,
    restore_source_ref: str | None,
    restore_source_commit: str | None,
    restore_paths: list[str],
    problems: list[str],
) -> tuple[dict[str, Any], bool]:
    source_ref = required_string(item.get("source_ref"), f"rollback.validation_evidence[{index}].source_ref", problems)
    source_commit = required_string(
        item.get("source_commit"),
        f"rollback.validation_evidence[{index}].source_commit",
        problems,
    )
    recorded_at = required_string(item.get("recorded_at"), f"rollback.validation_evidence[{index}].recorded_at", problems)
    commands = string_list(item.get("commands"), f"rollback.validation_evidence[{index}].commands", problems)
    cargo_log_raw = required_string(
        item.get("cargo_build_log"),
        f"rollback.validation_evidence[{index}].cargo_build_log",
        problems,
    )
    cargo_sha = required_string(
        item.get("cargo_build_sha256"),
        f"rollback.validation_evidence[{index}].cargo_build_sha256",
        problems,
    )
    audit_report_raw = required_string(
        item.get("retirement_audit_report"),
        f"rollback.validation_evidence[{index}].retirement_audit_report",
        problems,
    )
    audit_sha = required_string(
        item.get("retirement_audit_sha256"),
        f"rollback.validation_evidence[{index}].retirement_audit_sha256",
        problems,
    )
    marker_report_raw = required_string(
        item.get("marker_scan_report"),
        f"rollback.validation_evidence[{index}].marker_scan_report",
        problems,
    )
    marker_sha = required_string(
        item.get("marker_scan_sha256"),
        f"rollback.validation_evidence[{index}].marker_scan_sha256",
        problems,
    )

    summary: dict[str, Any] = {
        "source_ref": source_ref,
        "source_commit": source_commit,
        "recorded_at": recorded_at,
        "commands": commands,
        "cargo_build": {"path": cargo_log_raw, "sha256_ok": False},
        "retirement_audit": {"path": audit_report_raw, "sha256_ok": False, "problems_ok": False},
        "marker_scan": {"path": marker_report_raw, "sha256_ok": False},
        "valid_for_rollback": False,
    }

    if source_commit and not looks_like_commit(source_commit):
        problems.append(f"rollback.validation_evidence[{index}].source_commit must be a git commit digest")
    if restore_source_ref is not None and source_ref and source_ref != restore_source_ref:
        problems.append(f"rollback.validation_evidence[{index}].source_ref does not match rollback restore source")
    if restore_source_commit is not None and source_commit and source_commit != restore_source_commit:
        problems.append(
            f"rollback.validation_evidence[{index}].source_commit does not match rollback restore source commit"
        )
    for required in (ROLLBACK_FETCH_COMMAND, ROLLBACK_BUILD_COMMAND, ROLLBACK_AUDIT_COMMAND):
        if required not in commands:
            problems.append(f"rollback.validation_evidence[{index}].commands must include `{required}`")
    checkout_ok = bool(source_ref) and has_restore_checkout_command(commands, source_ref, restore_paths)
    summary["checkout_command_ok"] = checkout_ok
    if not checkout_ok:
        problems.append(
            f"rollback.validation_evidence[{index}].commands must restore all rollback.restore_paths from source_ref"
        )

    cargo_ok = False
    if cargo_log_raw and cargo_sha:
        cargo_ok = verify_sha256(
            resolve_evidence_path(evidence_root, cargo_log_raw),
            cargo_sha,
            f"rollback.validation_evidence[{index}].cargo_build_sha256",
            problems,
        )
    summary["cargo_build"]["sha256_ok"] = cargo_ok

    audit_ok = False
    audit_problems_ok = False
    if audit_report_raw and audit_sha:
        audit_path = resolve_evidence_path(evidence_root, audit_report_raw)
        audit_ok = verify_sha256(
            audit_path,
            audit_sha,
            f"rollback.validation_evidence[{index}].retirement_audit_sha256",
            problems,
        )
        if audit_path.is_file():
            try:
                audit_report = read_json(audit_path)
            except (OSError, ValueError) as exc:
                problems.append(f"rollback.validation_evidence[{index}].retirement_audit_report is invalid: {exc}")
            else:
                if audit_report.get("format") != REPORT_FORMAT:
                    problems.append(
                        f"rollback.validation_evidence[{index}].retirement_audit_report format must be {REPORT_FORMAT}"
                    )
                audit_problems = audit_report.get("problems")
                audit_problems_ok = isinstance(audit_problems, list) and not audit_problems
                if not audit_problems_ok:
                    problems.append(
                        f"rollback.validation_evidence[{index}].retirement_audit_report must have no problems"
                    )
    summary["retirement_audit"]["sha256_ok"] = audit_ok
    summary["retirement_audit"]["problems_ok"] = audit_problems_ok

    marker_ok = False
    if marker_report_raw and marker_sha:
        marker_ok = verify_sha256(
            resolve_evidence_path(evidence_root, marker_report_raw),
            marker_sha,
            f"rollback.validation_evidence[{index}].marker_scan_sha256",
            problems,
        )
    summary["marker_scan"]["sha256_ok"] = marker_ok

    valid = (
        bool(source_ref)
        and bool(source_commit)
        and looks_like_commit(source_commit)
        and bool(recorded_at)
        and ROLLBACK_FETCH_COMMAND in commands
        and ROLLBACK_BUILD_COMMAND in commands
        and ROLLBACK_AUDIT_COMMAND in commands
        and checkout_ok
        and cargo_ok
        and audit_ok
        and audit_problems_ok
        and marker_ok
    )
    summary["valid_for_rollback"] = valid
    return summary, valid


def audit_rollback(
    manifest: dict[str, Any],
    repo_root: Path,
    evidence_root: Path,
    problems: list[str],
) -> tuple[dict[str, Any], list[str]]:
    rollback = object_field(manifest.get("rollback"), "rollback", problems)
    required = required_bool(rollback.get("required"), "rollback.required", problems)
    validated = required_bool(rollback.get("validated"), "rollback.validated", problems)
    restore_source = object_field(rollback.get("restore_source"), "rollback.restore_source", problems)
    restore_source_kind = optional_string(
        restore_source.get("kind"),
        "rollback.restore_source.kind",
        problems,
    )
    restore_source_ref = optional_string(
        restore_source.get("ref"),
        "rollback.restore_source.ref",
        problems,
    )
    restore_source_commit = optional_string(
        restore_source.get("commit"),
        "rollback.restore_source.commit",
        problems,
    )
    if restore_source_kind is not None and restore_source_kind not in {"tag-or-branch", "tag", "branch", "commit"}:
        problems.append("rollback.restore_source.kind must be tag-or-branch, tag, branch, or commit")
    if validated and not restore_source_ref:
        problems.append("rollback.restore_source.ref must be recorded when rollback.validated is true")
    if validated and not restore_source_commit:
        problems.append("rollback.restore_source.commit must be recorded when rollback.validated is true")
    if restore_source_commit is not None and not looks_like_commit(restore_source_commit):
        problems.append("rollback.restore_source.commit must be a git commit digest")

    restore_paths = string_list(rollback.get("restore_paths"), "rollback.restore_paths", problems)
    restore_path_summaries: list[dict[str, Any]] = []
    for raw in restore_paths:
        path = resolve_repo_path(repo_root, raw)
        exists = path.exists()
        if not exists:
            problems.append(f"rollback.restore_paths entry is missing: {raw}")
        restore_path_summaries.append(
            {
                "path": raw,
                "exists": exists,
                "kind": "directory" if path.is_dir() else "file" if path.is_file() else "missing",
            }
        )

    required_commands = string_list(rollback.get("required_commands"), "rollback.required_commands", problems)
    for command in ROLLBACK_REQUIRED_COMMANDS:
        if command not in required_commands:
            problems.append(f"rollback.required_commands must include `{command}`")

    raw_evidence = rollback.get("validation_evidence", [])
    evidence: list[dict[str, Any]] = []
    if not isinstance(raw_evidence, list):
        problems.append("rollback.validation_evidence must be a list")
    else:
        for index, item in enumerate(raw_evidence):
            if not isinstance(item, dict):
                problems.append(f"rollback.validation_evidence[{index}] must be an object")
                continue
            evidence.append(item)

    evidence_summaries: list[dict[str, Any]] = []
    valid_evidence: list[dict[str, Any]] = []
    for index, item in enumerate(evidence):
        summary, valid = validate_rollback_evidence(
            item,
            index,
            evidence_root,
            restore_source_ref,
            restore_source_commit,
            restore_paths,
            problems,
        )
        evidence_summaries.append(summary)
        if valid:
            valid_evidence.append(summary)

    if validated and not valid_evidence:
        problems.append("rollback.validated is true but no valid validation_evidence entry was recorded")

    blockers: list[str] = []
    if required and not validated:
        blockers.append("rollback restore validation is not recorded")
    if required and not valid_evidence:
        blockers.append("rollback requires a validated restore evidence entry")

    return (
        {
            "required": required,
            "validated": validated,
            "restore_source": {
                "kind": restore_source_kind,
                "ref": restore_source_ref,
                "commit": restore_source_commit,
            },
            "restore_paths": restore_path_summaries,
            "required_commands": required_commands,
            "evidence_count": len(evidence),
            "valid_evidence_count": len(valid_evidence),
            "validation_evidence": evidence_summaries,
        },
        blockers,
    )


def build_report(manifest_path: Path, repo_root: Path, evidence_root: Path | None = None) -> dict[str, Any]:
    problems: list[str] = []
    evidence_root = evidence_root or repo_root
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
    classes, classified_paths, class_blockers = audit_classes(manifest, repo_root, evidence_root, problems)
    bake_in, bake_in_blockers = audit_bake_in(manifest, evidence_root, problems)
    rollback, rollback_blockers = audit_rollback(manifest, repo_root, evidence_root, problems)

    tracked = tracked_rust_inventory(repo_root)
    unclassified = sorted(path for path in tracked if path not in classified_paths)
    for path in unclassified:
        problems.append(f"tracked Rust or Cargo path is not classified: {path}")

    rollback_commands = string_list(
        manifest.get("rollback_commands"),
        "rollback_commands",
        problems,
    )
    for command in ROLLBACK_REQUIRED_COMMANDS:
        if command not in rollback_commands:
            problems.append(f"rollback_commands must include `{command}`")

    blockers: list[str] = []
    if status != "approved":
        blockers.append(f"retirement decision status is {status}; approved is required for removal")
    if approval_required and not approval_granted:
        blockers.append("approval required before Rust reference removal")
    blockers.extend(bake_in_blockers)
    blockers.extend(rollback_blockers)
    blockers.extend(class_blockers)

    removal_allowed = not problems and not blockers
    return {
        "format": REPORT_FORMAT,
        "schema_version": SCHEMA_VERSION,
        "manifest": display_path(manifest_path, repo_root),
        "evidence_root": display_path(evidence_root, repo_root),
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
        "rollback": rollback,
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
        "--evidence-root",
        type=Path,
        help="base directory for relative evidence artifact paths; defaults to the repository root",
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
    evidence_root = (
        resolve_repo_path(repo_root, str(args.evidence_root)).resolve()
        if args.evidence_root is not None
        else repo_root
    )
    try:
        report = build_report(manifest_path, repo_root, evidence_root)
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
