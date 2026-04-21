#!/usr/bin/env python3
"""Create machine-checkable Rust retirement evidence entries."""

from __future__ import annotations

import argparse
import copy
import json
import shutil
import sys
from pathlib import Path
from typing import Any

from retirement_audit import (
    BOOTSTRAP_FORMAT,
    CI_COMMAND,
    PROVENANCE_FORMAT,
    RELEASE_PREFLIGHT_COMMAND,
    ROLLBACK_AUDIT_COMMAND,
    ROLLBACK_BUILD_COMMAND,
    ROLLBACK_FETCH_COMMAND,
    resolve_evidence_path,
    looks_like_commit,
    platform_key,
    read_json,
    sha256_prefixed,
    write_json,
)


DEFAULT_RESTORE_PATHS = ["Cargo.toml", "Cargo.lock", "src", "tests"]


def require_file(raw: str, field: str) -> Path:
    path = Path(raw)
    if not path.is_file():
        raise ValueError(f"{field} is missing: {raw}")
    return path


def evidence_path_value(path: Path, path_base: Path | None, field: str) -> str:
    if path_base is None:
        return str(path)
    try:
        return path.resolve().relative_to(path_base.resolve()).as_posix()
    except ValueError as exc:
        raise ValueError(f"{field} must be inside --path-base") from exc


def load_entry(path: Path) -> dict[str, Any]:
    entry = read_json(path)
    return entry


def required_entry_string(entry: dict[str, Any], field: str) -> str:
    value = entry.get(field)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"entry {field} must be a non-empty string")
    return value


def parse_class_entry(value: str) -> tuple[str, Path]:
    if "=" not in value:
        raise ValueError("--class-entry must be CLASS=PATH")
    class_id, raw_path = value.split("=", 1)
    class_id = class_id.strip()
    if not class_id:
        raise ValueError("--class-entry class id must not be empty")
    path = Path(raw_path)
    if not path.is_file():
        raise ValueError(f"--class-entry path is missing: {raw_path}")
    return class_id, path


def validate_commit(value: str, field: str) -> None:
    if not looks_like_commit(value):
        raise ValueError(f"{field} must be a git commit digest")


def command_set(entries: list[dict[str, Any]]) -> set[str]:
    commands: set[str] = set()
    for entry in entries:
        command = entry.get("command")
        if isinstance(command, str):
            commands.add(command)
    return commands


def bundle_relative_path(path: Path, bundle_root: Path) -> str:
    return evidence_path_value(path, bundle_root, "bundle path")


def ensure_parent(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)


def copy_into_bundle(source: Path, destination: Path) -> Path:
    ensure_parent(destination)
    shutil.copy2(source, destination)
    return destination


def normalize_bake_in_entry(args: argparse.Namespace) -> None:
    entry = load_entry(args.entry)
    source_root = args.source_evidence_root.resolve()
    bundle_root = args.bundle_root.resolve()
    platform = platform_key(required_entry_string(entry, "platform"))
    bootstrap_source = resolve_evidence_path(
        source_root,
        required_entry_string(entry, "bootstrap_report"),
    )
    provenance_source = resolve_evidence_path(
        source_root,
        required_entry_string(entry, "release_provenance"),
    )
    default_build_source = resolve_evidence_path(
        source_root,
        required_entry_string(entry, "default_build_artifact"),
    )
    bootstrap_source = require_file(str(bootstrap_source), "entry bootstrap_report")
    provenance_source = require_file(str(provenance_source), "entry release_provenance")
    default_build_source = require_file(str(default_build_source), "entry default_build_artifact")

    bootstrap_destination = copy_into_bundle(
        bootstrap_source,
        bundle_root / platform / "bootstrap" / bootstrap_source.name,
    )
    default_build_destination = copy_into_bundle(
        default_build_source,
        bundle_root / platform / "default" / default_build_source.name,
    )

    provenance_payload = read_json(provenance_source)
    artifact = provenance_payload.get("canonical_artifact")
    artifact_path_raw = artifact.get("path") if isinstance(artifact, dict) else None
    if not isinstance(artifact_path_raw, str) or not artifact_path_raw.strip():
        raise ValueError("entry release_provenance is missing canonical_artifact.path")
    artifact_source = resolve_evidence_path(source_root, artifact_path_raw)
    artifact_source = require_file(str(artifact_source), "entry release_provenance canonical artifact")
    artifact_destination = copy_into_bundle(
        artifact_source,
        bundle_root / platform / "release" / artifact_source.name,
    )
    normalized_provenance = copy.deepcopy(provenance_payload)
    canonical_artifact = normalized_provenance.get("canonical_artifact")
    if not isinstance(canonical_artifact, dict):
        raise ValueError("entry release_provenance canonical_artifact must be an object")
    canonical_artifact["path"] = bundle_relative_path(artifact_destination, bundle_root)
    provenance_destination = bundle_root / platform / "release" / provenance_source.name
    write_json(provenance_destination, normalized_provenance)

    normalized_entry = copy.deepcopy(entry)
    normalized_entry["bootstrap_report"] = bundle_relative_path(bootstrap_destination, bundle_root)
    normalized_entry["bootstrap_report_sha256"] = sha256_prefixed(bootstrap_destination)
    normalized_entry["release_provenance"] = bundle_relative_path(provenance_destination, bundle_root)
    normalized_entry["release_provenance_sha256"] = sha256_prefixed(provenance_destination)
    normalized_entry["default_build_artifact"] = bundle_relative_path(default_build_destination, bundle_root)
    normalized_entry["default_build_sha256"] = sha256_prefixed(default_build_destination)
    write_json(args.out, normalized_entry)


def normalize_rollback_entry(args: argparse.Namespace) -> None:
    entry = load_entry(args.entry)
    source_root = args.source_evidence_root.resolve()
    bundle_root = args.bundle_root.resolve()

    cargo_source = resolve_evidence_path(
        source_root,
        required_entry_string(entry, "cargo_build_log"),
    )
    audit_source = resolve_evidence_path(
        source_root,
        required_entry_string(entry, "retirement_audit_report"),
    )
    marker_source = resolve_evidence_path(
        source_root,
        required_entry_string(entry, "marker_scan_report"),
    )
    cargo_source = require_file(str(cargo_source), "entry cargo_build_log")
    audit_source = require_file(str(audit_source), "entry retirement_audit_report")
    marker_source = require_file(str(marker_source), "entry marker_scan_report")

    cargo_destination = copy_into_bundle(
        cargo_source,
        bundle_root / "rollback" / cargo_source.name,
    )
    audit_destination = copy_into_bundle(
        audit_source,
        bundle_root / "rollback" / audit_source.name,
    )
    marker_destination = copy_into_bundle(
        marker_source,
        bundle_root / "rollback" / marker_source.name,
    )

    normalized_entry = copy.deepcopy(entry)
    normalized_entry["cargo_build_log"] = bundle_relative_path(cargo_destination, bundle_root)
    normalized_entry["cargo_build_sha256"] = sha256_prefixed(cargo_destination)
    normalized_entry["retirement_audit_report"] = bundle_relative_path(audit_destination, bundle_root)
    normalized_entry["retirement_audit_sha256"] = sha256_prefixed(audit_destination)
    normalized_entry["marker_scan_report"] = bundle_relative_path(marker_destination, bundle_root)
    normalized_entry["marker_scan_sha256"] = sha256_prefixed(marker_destination)
    write_json(args.out, normalized_entry)


def validate_bake_in_inputs(platform: str, source_commit: str, bootstrap: Path, provenance: Path) -> None:
    problems: list[str] = []
    bootstrap_report = read_json(bootstrap)
    if bootstrap_report.get("format") != BOOTSTRAP_FORMAT:
        problems.append(f"--bootstrap-report format must be {BOOTSTRAP_FORMAT}")
    if bootstrap_report.get("status") != "supported-ready" or bootstrap_report.get("ready") is not True:
        problems.append("--bootstrap-report must be supported-ready")
    bootstrap_host = bootstrap_report.get("host")
    bootstrap_platform = bootstrap_host.get("platform") if isinstance(bootstrap_host, dict) else None
    if not isinstance(bootstrap_platform, str) or platform_key(bootstrap_platform) != platform:
        problems.append("--bootstrap-report platform must match --platform")
    performance = bootstrap_report.get("performance")
    budget_source = performance.get("budget_source") if isinstance(performance, dict) else None
    overrides = budget_source.get("overrides") if isinstance(budget_source, dict) else None
    if overrides != {}:
        problems.append("--bootstrap-report must use production budget defaults")

    provenance_report = read_json(provenance)
    if provenance_report.get("format") != PROVENANCE_FORMAT:
        problems.append(f"--release-provenance format must be {PROVENANCE_FORMAT}")
    provenance_host = provenance_report.get("host")
    provenance_platform = provenance_host.get("platform") if isinstance(provenance_host, dict) else None
    if not isinstance(provenance_platform, str) or platform_key(provenance_platform) != platform:
        problems.append("--release-provenance platform must match --platform")
    source = provenance_report.get("source")
    recorded_commit = source.get("commit") if isinstance(source, dict) else None
    if recorded_commit != source_commit:
        problems.append("--release-provenance source commit must match --source-commit")
    if not isinstance(source, dict) or source.get("worktree_dirty") is not False:
        problems.append("--release-provenance must record a clean worktree")
    validation = provenance_report.get("validation")
    validation_ok = isinstance(validation, dict) and all(
        validation.get(key) is True
        for key in ("bootstrap_ready", "parity_ok", "stage_matrix_ok", "performance_ok")
    )
    validation_ok = validation_ok and isinstance(validation, dict) and validation.get("budget_overrides") == {}
    if not validation_ok:
        problems.append("--release-provenance validation fields must all pass")
    if problems:
        raise ValueError("; ".join(problems))


def write_bake_in_entry(args: argparse.Namespace) -> None:
    platform = platform_key(args.platform)
    if platform not in {"linux", "macos"}:
        raise ValueError("--platform must normalize to linux or macos")
    validate_commit(args.source_commit, "--source-commit")
    bootstrap = require_file(args.bootstrap_report, "--bootstrap-report")
    provenance = require_file(args.release_provenance, "--release-provenance")
    default_build = require_file(args.default_build_artifact, "--default-build-artifact")
    validate_bake_in_inputs(platform, args.source_commit, bootstrap, provenance)
    write_json(
        args.out,
        {
            "platform": platform,
            "status": "passed",
            "source_commit": args.source_commit,
            "recorded_at": args.recorded_at,
            "release_preflight_command": RELEASE_PREFLIGHT_COMMAND,
            "ci_command": CI_COMMAND,
            "bootstrap_report": evidence_path_value(bootstrap, args.path_base, "--bootstrap-report"),
            "bootstrap_report_sha256": sha256_prefixed(bootstrap),
            "release_provenance": evidence_path_value(provenance, args.path_base, "--release-provenance"),
            "release_provenance_sha256": sha256_prefixed(provenance),
            "default_build_artifact": evidence_path_value(
                default_build,
                args.path_base,
                "--default-build-artifact",
            ),
            "default_build_sha256": sha256_prefixed(default_build),
        },
    )


def write_rollback_entry(args: argparse.Namespace) -> None:
    validate_commit(args.source_commit, "--source-commit")
    cargo_log = require_file(args.cargo_build_log, "--cargo-build-log")
    audit_report = require_file(args.retirement_audit_report, "--retirement-audit-report")
    marker_report = require_file(args.marker_scan_report, "--marker-scan-report")
    restore_paths = args.restore_path or DEFAULT_RESTORE_PATHS
    checkout_command = f"git checkout {args.source_ref} -- {' '.join(restore_paths)}"
    write_json(
        args.out,
        {
            "source_ref": args.source_ref,
            "source_commit": args.source_commit,
            "recorded_at": args.recorded_at,
            "commands": [
                ROLLBACK_FETCH_COMMAND,
                checkout_command,
                ROLLBACK_BUILD_COMMAND,
                ROLLBACK_AUDIT_COMMAND,
            ],
            "cargo_build_log": evidence_path_value(cargo_log, args.path_base, "--cargo-build-log"),
            "cargo_build_sha256": sha256_prefixed(cargo_log),
            "retirement_audit_report": evidence_path_value(
                audit_report,
                args.path_base,
                "--retirement-audit-report",
            ),
            "retirement_audit_sha256": sha256_prefixed(audit_report),
            "marker_scan_report": evidence_path_value(marker_report, args.path_base, "--marker-scan-report"),
            "marker_scan_sha256": sha256_prefixed(marker_report),
        },
    )


def write_class_entry(args: argparse.Namespace) -> None:
    report = require_file(args.report, "--report")
    write_json(
        args.out,
        {
            "command": args.command,
            "recorded_at": args.recorded_at,
            "report": evidence_path_value(report, args.path_base, "--report"),
            "report_sha256": sha256_prefixed(report),
        },
    )


def find_class(manifest: dict[str, Any], class_id: str) -> dict[str, Any]:
    classes = manifest.get("rust_path_classes")
    if not isinstance(classes, list):
        raise ValueError("manifest rust_path_classes must be a list")
    for item in classes:
        if isinstance(item, dict) and item.get("class") == class_id:
            return item
    raise ValueError(f"unknown rust path class: {class_id}")


def approve_class_if_requested(
    class_item: dict[str, Any],
    class_id: str,
    approve_classes: set[str],
    allow_removal_classes: set[str],
) -> None:
    if class_id not in approve_classes:
        return
    decision = class_item.get("retirement_decision")
    if not isinstance(decision, dict):
        raise ValueError(f"{class_id} missing retirement_decision")
    intent = decision.get("intent")
    if intent == "remove-after-replacement":
        if class_id not in allow_removal_classes:
            raise ValueError(f"{class_id} approval requires --allow-removal-class")
        class_item["removal_allowed"] = True
    required = class_item.get("required_replacement_evidence")
    evidence = decision.get("evidence")
    if not isinstance(required, list) or not all(isinstance(item, str) for item in required):
        raise ValueError(f"{class_id} required_replacement_evidence must be a string list")
    if not isinstance(evidence, list):
        raise ValueError(f"{class_id} retirement_decision.evidence must be a list")
    missing = [command for command in required if command not in command_set(evidence)]
    if missing:
        raise ValueError(f"{class_id} cannot be approved; missing evidence for: {', '.join(missing)}")
    decision["status"] = "approved"


def assemble_manifest(args: argparse.Namespace) -> None:
    manifest = read_json(args.manifest)
    bake_in = manifest.get("bake_in")
    if not isinstance(bake_in, dict):
        raise ValueError("manifest bake_in must be an object")
    bake_evidence = bake_in.setdefault("evidence", [])
    if not isinstance(bake_evidence, list):
        raise ValueError("manifest bake_in.evidence must be a list")
    for entry_path in args.bake_in_entry:
        bake_evidence.append(load_entry(entry_path))

    rollback_entries = [load_entry(path) for path in args.rollback_entry]
    if rollback_entries:
        rollback = manifest.get("rollback")
        if not isinstance(rollback, dict):
            raise ValueError("manifest rollback must be an object")
        validation_evidence = rollback.setdefault("validation_evidence", [])
        if not isinstance(validation_evidence, list):
            raise ValueError("manifest rollback.validation_evidence must be a list")
        for entry in rollback_entries:
            validation_evidence.append(entry)
        first = rollback_entries[0]
        rollback["validated"] = True
        restore_source = rollback.setdefault("restore_source", {})
        if not isinstance(restore_source, dict):
            raise ValueError("manifest rollback.restore_source must be an object")
        restore_source["ref"] = first.get("source_ref")
        restore_source["commit"] = first.get("source_commit")

    for raw in args.class_entry:
        class_id, entry_path = parse_class_entry(raw)
        class_item = find_class(manifest, class_id)
        decision = class_item.get("retirement_decision")
        if not isinstance(decision, dict):
            raise ValueError(f"{class_id} missing retirement_decision")
        evidence = decision.setdefault("evidence", [])
        if not isinstance(evidence, list):
            raise ValueError(f"{class_id} retirement_decision.evidence must be a list")
        evidence.append(load_entry(entry_path))

    approve_classes = set(args.approve_class)
    allow_removal_classes = set(args.allow_removal_class)
    for class_id in approve_classes:
        approve_class_if_requested(find_class(manifest, class_id), class_id, approve_classes, allow_removal_classes)

    if args.approver:
        approval = manifest.get("approval")
        if not isinstance(approval, dict):
            raise ValueError("manifest approval must be an object")
        approval["approved"] = True
        approval["approver"] = args.approver
        manifest["status"] = "approved"

    write_json(args.out, manifest)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    bake = subparsers.add_parser("bake-in-entry", help="write one passing bake-in evidence entry")
    bake.add_argument("--platform", required=True)
    bake.add_argument("--source-commit", required=True)
    bake.add_argument("--recorded-at", required=True)
    bake.add_argument("--bootstrap-report", required=True)
    bake.add_argument("--release-provenance", required=True)
    bake.add_argument("--default-build-artifact", required=True)
    bake.add_argument("--path-base", type=Path)
    bake.add_argument("--out", type=Path, required=True)
    bake.set_defaults(func=write_bake_in_entry)

    normalize_bake = subparsers.add_parser(
        "normalize-bake-in-entry",
        help="copy a bake-in entry and its artifacts into a shared evidence bundle",
    )
    normalize_bake.add_argument("--entry", type=Path, required=True)
    normalize_bake.add_argument("--source-evidence-root", type=Path, required=True)
    normalize_bake.add_argument("--bundle-root", type=Path, required=True)
    normalize_bake.add_argument("--out", type=Path, required=True)
    normalize_bake.set_defaults(func=normalize_bake_in_entry)

    rollback = subparsers.add_parser("rollback-entry", help="write one rollback validation evidence entry")
    rollback.add_argument("--source-ref", required=True)
    rollback.add_argument("--source-commit", required=True)
    rollback.add_argument("--recorded-at", required=True)
    rollback.add_argument("--cargo-build-log", required=True)
    rollback.add_argument("--retirement-audit-report", required=True)
    rollback.add_argument("--marker-scan-report", required=True)
    rollback.add_argument("--restore-path", action="append", default=[])
    rollback.add_argument("--path-base", type=Path)
    rollback.add_argument("--out", type=Path, required=True)
    rollback.set_defaults(func=write_rollback_entry)

    normalize_rollback = subparsers.add_parser(
        "normalize-rollback-entry",
        help="copy a rollback entry and its artifacts into a shared evidence bundle",
    )
    normalize_rollback.add_argument("--entry", type=Path, required=True)
    normalize_rollback.add_argument("--source-evidence-root", type=Path, required=True)
    normalize_rollback.add_argument("--bundle-root", type=Path, required=True)
    normalize_rollback.add_argument("--out", type=Path, required=True)
    normalize_rollback.set_defaults(func=normalize_rollback_entry)

    class_entry = subparsers.add_parser("class-entry", help="write one class decision evidence entry")
    class_entry.add_argument("--command", required=True)
    class_entry.add_argument("--recorded-at", required=True)
    class_entry.add_argument("--report", required=True)
    class_entry.add_argument("--path-base", type=Path)
    class_entry.add_argument("--out", type=Path, required=True)
    class_entry.set_defaults(func=write_class_entry)

    assemble = subparsers.add_parser("assemble-manifest", help="merge evidence entries into a candidate manifest")
    assemble.add_argument("--manifest", type=Path, default=Path("docs/selfhost/rust-reference-retirement.v1.json"))
    assemble.add_argument("--out", type=Path, required=True)
    assemble.add_argument("--bake-in-entry", type=Path, action="append", default=[])
    assemble.add_argument("--rollback-entry", type=Path, action="append", default=[])
    assemble.add_argument("--class-entry", action="append", default=[], metavar="CLASS=PATH")
    assemble.add_argument("--approve-class", action="append", default=[])
    assemble.add_argument("--allow-removal-class", action="append", default=[])
    assemble.add_argument("--approver")
    assemble.set_defaults(func=assemble_manifest)

    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        args.func(args)
    except (OSError, ValueError) as exc:
        print(f"selfhost-retirement-evidence: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
