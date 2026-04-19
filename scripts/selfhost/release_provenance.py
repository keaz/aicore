#!/usr/bin/env python3
"""Generate and verify release provenance for self-host compiler artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


PROVENANCE_FORMAT = "aicore-selfhost-release-provenance-v1"
PROVENANCE_SCHEMA_VERSION = 1
BOOTSTRAP_FORMAT = "aicore-selfhost-bootstrap-v1"
PARITY_FORMAT = "aicore-selfhost-parity-v1"
STAGE_MATRIX_FORMAT = "aicore-selfhost-stage-matrix-v1"
PERFORMANCE_FORMAT = "aicore-selfhost-bootstrap-performance-v1"
PERFORMANCE_TREND_FORMAT = "aicore-selfhost-bootstrap-performance-trend-v1"
STAGES = ("stage0", "stage1", "stage2")
REPORTS = (
    ("bootstrap", None, BOOTSTRAP_FORMAT),
    ("parity", "parity_report", PARITY_FORMAT),
    ("stage_matrix", "stage_matrix_report", STAGE_MATRIX_FORMAT),
    ("performance", "performance_report", PERFORMANCE_FORMAT),
    ("performance_trend", "performance_trend", PERFORMANCE_TREND_FORMAT),
)


def platform_key(name: str) -> str:
    normalized = name.lower()
    if normalized == "darwin":
        return "macos"
    if normalized.startswith("linux"):
        return "linux"
    return normalized


def arch_key(machine: str) -> str:
    normalized = machine.lower()
    if normalized in ("x86_64", "amd64"):
        return "x64"
    if normalized in ("aarch64", "arm64"):
        return "arm64"
    return normalized or "unknown"


def expected_strip_command(platform_name: str) -> str:
    key = platform_key(platform_name)
    if key == "macos":
        return "strip -S -x"
    if key == "linux":
        return "strip --strip-all"
    raise ValueError(f"unsupported self-host release platform: {key}")


def read_json(path: Path) -> dict[str, object]:
    with path.open("r", encoding="utf-8") as handle:
        parsed = json.load(handle)
    if not isinstance(parsed, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return parsed


def write_json(path: Path, payload: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def sha256_hex(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_prefixed(path: Path) -> str:
    return f"sha256:{sha256_hex(path)}"


def require_file(path: Path, label: str, errors: list[str]) -> bool:
    if not path.is_file():
        errors.append(f"missing required {label}: {path}")
        return False
    return True


def resolve_path(raw: object, repo_root: Path, field: str, errors: list[str]) -> Path:
    if not isinstance(raw, str) or raw.strip() == "":
        errors.append(f"missing required path field: {field}")
        return repo_root / "__missing__"
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


def checksum_entry(kind: str, path: Path, repo_root: Path) -> dict[str, object]:
    return {
        "kind": kind,
        "path": display_path(path, repo_root),
        "sha256": sha256_prefixed(path),
        "bytes": path.stat().st_size,
    }


def checksum_hex(entry: dict[str, object]) -> str:
    digest = entry.get("sha256")
    if not isinstance(digest, str) or not digest.startswith("sha256:"):
        raise ValueError(f"invalid checksum entry digest: {entry}")
    return digest.removeprefix("sha256:")


def write_checksum_file(path: Path, entries: list[dict[str, object]]) -> None:
    lines = []
    for entry in entries:
        entry_path = entry.get("path")
        if not isinstance(entry_path, str):
            raise ValueError(f"invalid checksum entry path: {entry}")
        lines.append(f"{checksum_hex(entry)}  {entry_path}\n")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("".join(lines), encoding="utf-8")


def parse_checksum_file(path: Path) -> dict[str, str]:
    parsed: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        trimmed = line.strip()
        if trimmed == "" or trimmed.startswith("#"):
            continue
        parts = trimmed.split(maxsplit=1)
        if len(parts) != 2:
            raise ValueError(f"invalid checksum line in {path}: {line}")
        digest, name = parts
        name = name.strip().lstrip("*")
        parsed[name] = digest.lower()
    return parsed


def normalize_digest(path: Path, command_text: str) -> str:
    command = shlex.split(command_text)
    if not command:
        raise ValueError("normalization command is empty")
    with tempfile.TemporaryDirectory(prefix="aicore-selfhost-release-strip-") as tmp_dir:
        tmp_path = Path(tmp_dir) / path.name
        shutil.copy2(path, tmp_path)
        completed = subprocess.run(
            command + [str(tmp_path)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if completed.returncode != 0:
            raise ValueError(
                f"normalization command `{command_text}` failed for {path}: "
                f"{completed.stderr.strip() or completed.stdout.strip()}"
            )
        return sha256_prefixed(tmp_path)


def command_version(command: list[str]) -> str:
    try:
        completed = subprocess.run(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except OSError as exc:
        return f"unavailable: {exc}"
    output = (completed.stdout or completed.stderr).strip().splitlines()
    if not output:
        return "available" if completed.returncode == 0 else f"unavailable: exit {completed.returncode}"
    return output[0]


def toolchain_versions(platform_name: str) -> dict[str, str]:
    strip_path = shutil.which("strip") or "strip"
    versions = {
        "cargo": command_version(["cargo", "--version"]),
        "rustc": command_version(["rustc", "--version"]),
        "clang": command_version(["clang", "--version"]),
        "strip": command_version([strip_path, "--version"]),
    }
    if platform_key(platform_name) == "macos":
        versions["codesign"] = command_version(["codesign", "-h"])
    return versions


def git_value(repo_root: Path, args: list[str]) -> str | None:
    try:
        completed = subprocess.run(
            ["git"] + args,
            cwd=repo_root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except OSError:
        return None
    if completed.returncode != 0:
        return None
    return completed.stdout.strip()


def source_commit(repo_root: Path, override: str | None) -> str:
    if override:
        return override
    env_override = os.environ.get("AIC_SELFHOST_RELEASE_SOURCE_COMMIT")
    if env_override and env_override.strip():
        return env_override.strip()
    commit = git_value(repo_root, ["rev-parse", "HEAD"])
    if commit:
        return commit
    raise ValueError("could not determine source commit; pass --source-commit")


def worktree_dirty(repo_root: Path) -> bool:
    status = git_value(repo_root, ["status", "--porcelain", "--untracked-files=no"])
    return bool(status)


def require_report_ok(
    name: str,
    path: Path,
    expected_format: str,
    errors: list[str],
) -> dict[str, object]:
    if not require_file(path, f"{name} report", errors):
        return {}
    try:
        report = read_json(path)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        errors.append(f"{name} report is invalid JSON: {exc}")
        return {}
    if report.get("format") != expected_format:
        errors.append(f"{name} report format must be {expected_format}")
    ok = report.get("ok")
    if name == "bootstrap":
        ok = report.get("ready")
    if name == "performance":
        performance = report.get("performance")
        ok = isinstance(performance, dict) and performance.get("ok") is True
    if ok is not True:
        errors.append(f"{name} report is not passing")
    return report


def bootstrap_step(bootstrap: dict[str, object], name: str) -> dict[str, object]:
    steps = bootstrap.get("steps")
    if not isinstance(steps, list):
        return {}
    for step in steps:
        if isinstance(step, dict) and step.get("name") == name:
            return step
    return {}


def collect_stage_entries(
    bootstrap: dict[str, object],
    repo_root: Path,
    strip_command: str,
    errors: list[str],
) -> dict[str, dict[str, object]]:
    stages: dict[str, dict[str, object]] = {}
    repro = bootstrap.get("reproducibility")
    if not isinstance(repro, dict):
        errors.append("bootstrap report is missing reproducibility object")
        repro = {}
    if repro.get("matches") is not True:
        errors.append("bootstrap reproducibility result is not passing")

    for stage in STAGES:
        path = resolve_path(bootstrap.get(stage), repo_root, stage, errors)
        step = bootstrap_step(bootstrap, stage)
        if not require_file(path, f"{stage} artifact", errors):
            continue
        raw_digest = sha256_prefixed(path)
        normalized_digest = ""
        try:
            normalized_digest = normalize_digest(path, strip_command)
        except ValueError as exc:
            errors.append(f"{stage} normalization failed: {exc}")
        step_digest = step.get("artifact_sha256")
        if step_digest != raw_digest:
            errors.append(
                f"{stage} raw digest mismatch: bootstrap report has {step_digest}, actual is {raw_digest}"
            )
        if step.get("artifact_exists") is not True:
            errors.append(f"{stage} bootstrap step did not record an existing artifact")
        if stage in ("stage1", "stage2"):
            raw_key = f"{stage}_sha256"
            stripped_key = f"{stage}_stripped_sha256"
            if repro.get(raw_key) != raw_digest:
                errors.append(
                    f"{stage} reproducibility raw digest mismatch: report has {repro.get(raw_key)}, actual is {raw_digest}"
                )
            if normalized_digest and repro.get(stripped_key) != normalized_digest:
                errors.append(
                    f"{stage} reproducibility normalized digest mismatch: report has {repro.get(stripped_key)}, actual is {normalized_digest}"
                )
        stages[stage] = {
            "path": display_path(path, repo_root),
            "raw_sha256": raw_digest,
            "normalized_sha256": normalized_digest,
            "bytes": path.stat().st_size,
            "normalization": {
                "command": strip_command,
            },
        }
    return stages


def report_paths(
    bootstrap_report: Path,
    bootstrap: dict[str, object],
    repo_root: Path,
    errors: list[str],
) -> dict[str, Path]:
    paths = {"bootstrap": bootstrap_report}
    for name, field, _format in REPORTS:
        if field is None:
            continue
        paths[name] = resolve_path(bootstrap.get(field), repo_root, field, errors)
    return paths


def canonical_artifact_name(platform_name: str, machine: str) -> str:
    return f"aicore-selfhost-compiler-{platform_key(platform_name)}-{arch_key(machine)}"


def fail_if_errors(errors: list[str]) -> None:
    if not errors:
        return
    for error in errors:
        print(f"selfhost-release-provenance: {error}", file=sys.stderr)
    raise SystemExit(1)


def generate(args: argparse.Namespace) -> int:
    repo_root = Path(args.repo_root).resolve()
    bootstrap_report = Path(args.bootstrap_report)
    if not bootstrap_report.is_absolute():
        bootstrap_report = repo_root / bootstrap_report
    out_dir = Path(args.out_dir)
    if not out_dir.is_absolute():
        out_dir = repo_root / out_dir
    out_dir.mkdir(parents=True, exist_ok=True)

    errors: list[str] = []
    bootstrap = require_report_ok("bootstrap", bootstrap_report, BOOTSTRAP_FORMAT, errors)
    if not bootstrap:
        fail_if_errors(errors)

    host = bootstrap.get("host") if isinstance(bootstrap.get("host"), dict) else {}
    host_platform = args.platform or str(host.get("platform") or sys.platform)
    key = platform_key(host_platform)
    if key not in ("linux", "macos"):
        errors.append(f"unsupported self-host release platform: {key}")
    machine = str(host.get("machine") or platform.machine())
    repro = bootstrap.get("reproducibility") if isinstance(bootstrap.get("reproducibility"), dict) else {}
    strip_command = str(repro.get("strip_command") or "")
    try:
        expected = expected_strip_command(key)
        if strip_command != expected:
            errors.append(
                f"normalization command mismatch for {key}: expected `{expected}`, report has `{strip_command}`"
            )
    except ValueError as exc:
        errors.append(str(exc))

    performance = bootstrap.get("performance") if isinstance(bootstrap.get("performance"), dict) else {}
    if performance.get("ok") is not True:
        errors.append("bootstrap performance budget gate is not passing")
    budget_source = performance.get("budget_source")
    overrides = budget_source.get("overrides") if isinstance(budget_source, dict) else None
    if overrides not in ({}, None):
        errors.append("release provenance requires checked-in budget defaults with no overrides")

    reports = report_paths(bootstrap_report, bootstrap, repo_root, errors)
    report_documents: dict[str, dict[str, object]] = {}
    for name, _field, expected_format in REPORTS:
        report_documents[name] = require_report_ok(name, reports[name], expected_format, errors)

    stages = collect_stage_entries(bootstrap, repo_root, strip_command, errors)
    if set(stages.keys()) != set(STAGES):
        errors.append("stage artifact set is incomplete")
    fail_if_errors(errors)

    release_artifact = out_dir / canonical_artifact_name(key, machine)
    shutil.copy2(resolve_path(bootstrap["stage2"], repo_root, "stage2", errors), release_artifact)

    artifact_entry = checksum_entry("selfhost-compiler", release_artifact, repo_root)
    stage_entries = [
        checksum_entry(f"{stage}-compiler", resolve_path(bootstrap[stage], repo_root, stage, errors), repo_root)
        for stage in STAGES
    ]
    report_entries = [
        checksum_entry(f"{name}-report", reports[name], repo_root)
        for name, _field, _expected in REPORTS
    ]
    checksum_entries = [artifact_entry] + stage_entries + report_entries
    artifact_checksum = out_dir / f"{release_artifact.name}.sha256"
    checksums_path = out_dir / "selfhost-release-checksums.sha256"
    write_checksum_file(artifact_checksum, [artifact_entry])
    write_checksum_file(checksums_path, checksum_entries)

    commit = source_commit(repo_root, args.source_commit)
    provenance_path = Path(args.provenance)
    if not provenance_path.is_absolute():
        provenance_path = repo_root / provenance_path
    provenance = {
        "format": PROVENANCE_FORMAT,
        "schema_version": PROVENANCE_SCHEMA_VERSION,
        "source": {
            "commit": commit,
            "worktree_dirty": worktree_dirty(repo_root),
        },
        "host": host,
        "platform": {
            "key": key,
            "machine": machine,
            "artifact_name": release_artifact.name,
        },
        "toolchain": toolchain_versions(key),
        "canonical_artifact": {
            "path": display_path(release_artifact, repo_root),
            "sha256": artifact_entry["sha256"],
            "bytes": artifact_entry["bytes"],
            "checksum_path": display_path(artifact_checksum, repo_root),
        },
        "stages": stages,
        "reports": {
            name: {
                "path": display_path(path, repo_root),
                "sha256": sha256_prefixed(path),
                "bytes": path.stat().st_size,
                "format": report_documents[name].get("format"),
                "ok": report_documents[name].get("ok")
                if name not in ("bootstrap", "performance")
                else (
                    report_documents[name].get("ready")
                    if name == "bootstrap"
                    else report_documents[name].get("performance", {}).get("ok")
                ),
            }
            for name, path in reports.items()
        },
        "reproducibility": repro,
        "validation": {
            "bootstrap_status": bootstrap.get("status"),
            "bootstrap_ready": bootstrap.get("ready"),
            "parity_ok": report_documents["parity"].get("ok"),
            "stage_matrix_ok": report_documents["stage_matrix"].get("ok"),
            "performance_ok": performance.get("ok"),
            "budget_overrides": overrides or {},
        },
        "normalization": {
            "command": strip_command,
            "expected_command": expected_strip_command(key),
        },
        "checksums": {
            "path": display_path(checksums_path, repo_root),
            "format": "sha256sum",
            "entries": checksum_entries,
        },
    }
    write_json(provenance_path, provenance)
    provenance_checksum = out_dir / f"{provenance_path.name}.sha256"
    write_checksum_file(provenance_checksum, [checksum_entry("selfhost-provenance", provenance_path, repo_root)])
    print(
        "selfhost-release-provenance: "
        f"artifact={release_artifact} provenance={provenance_path} checksums={checksums_path}"
    )
    return 0


def resolve_recorded_path(raw: object, repo_root: Path) -> Path:
    if not isinstance(raw, str) or raw.strip() == "":
        raise ValueError("recorded path must be a non-empty string")
    path = Path(raw)
    if path.is_absolute():
        return path
    return repo_root / path


def verify_entry(entry: dict[str, object], repo_root: Path, errors: list[str]) -> None:
    raw_path = entry.get("path")
    recorded_sha = entry.get("sha256")
    if not isinstance(recorded_sha, str):
        errors.append(f"checksum entry missing sha256: {entry}")
        return
    try:
        path = resolve_recorded_path(raw_path, repo_root)
    except ValueError as exc:
        errors.append(str(exc))
        return
    if not require_file(path, str(raw_path), errors):
        return
    actual = sha256_prefixed(path)
    if actual != recorded_sha:
        errors.append(f"{raw_path} checksum mismatch: expected {recorded_sha}, got {actual}")


def verify(args: argparse.Namespace) -> int:
    repo_root = Path(args.repo_root).resolve()
    provenance_path = Path(args.provenance)
    if not provenance_path.is_absolute():
        provenance_path = repo_root / provenance_path
    errors: list[str] = []
    if not require_file(provenance_path, "self-host provenance", errors):
        fail_if_errors(errors)
    try:
        provenance = read_json(provenance_path)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        errors.append(f"provenance is invalid JSON: {exc}")
        fail_if_errors(errors)

    if provenance.get("format") != PROVENANCE_FORMAT:
        errors.append(f"provenance format must be {PROVENANCE_FORMAT}")
    if provenance.get("schema_version") != PROVENANCE_SCHEMA_VERSION:
        errors.append(f"provenance schema_version must be {PROVENANCE_SCHEMA_VERSION}")
    validation = provenance.get("validation")
    if not isinstance(validation, dict):
        errors.append("provenance validation object is missing")
    else:
        for field in ("bootstrap_ready", "parity_ok", "stage_matrix_ok", "performance_ok"):
            if validation.get(field) is not True:
                errors.append(f"provenance validation field {field} is not true")
        if validation.get("budget_overrides") not in ({}, None):
            errors.append("provenance validation records budget overrides")

    canonical = provenance.get("canonical_artifact")
    if isinstance(canonical, dict):
        verify_entry(
            {
                "path": canonical.get("path"),
                "sha256": canonical.get("sha256"),
            },
            repo_root,
            errors,
        )
    else:
        errors.append("provenance canonical_artifact object is missing")

    stages = provenance.get("stages")
    if not isinstance(stages, dict):
        errors.append("provenance stages object is missing")
    else:
        for stage in STAGES:
            entry = stages.get(stage)
            if not isinstance(entry, dict):
                errors.append(f"provenance missing {stage} stage entry")
                continue
            verify_entry(
                {
                    "path": entry.get("path"),
                    "sha256": entry.get("raw_sha256"),
                },
                repo_root,
                errors,
            )
            try:
                path = resolve_recorded_path(entry.get("path"), repo_root)
                normalized = normalize_digest(
                    path,
                    str(entry.get("normalization", {}).get("command") or ""),
                )
                if normalized != entry.get("normalized_sha256"):
                    errors.append(
                        f"{stage} normalized checksum mismatch: expected {entry.get('normalized_sha256')}, got {normalized}"
                    )
            except ValueError as exc:
                errors.append(f"{stage} normalization verification failed: {exc}")

    reports = provenance.get("reports")
    if not isinstance(reports, dict):
        errors.append("provenance reports object is missing")
    else:
        for name, _field, expected_format in REPORTS:
            entry = reports.get(name)
            if not isinstance(entry, dict):
                errors.append(f"provenance missing {name} report entry")
                continue
            verify_entry(entry, repo_root, errors)
            if entry.get("format") != expected_format:
                errors.append(f"{name} report provenance format must be {expected_format}")
            if entry.get("ok") is not True:
                errors.append(f"{name} report provenance status is not passing")

    checksums = provenance.get("checksums")
    if not isinstance(checksums, dict):
        errors.append("provenance checksums object is missing")
    else:
        try:
            checksum_path = resolve_recorded_path(checksums.get("path"), repo_root)
            if require_file(checksum_path, "checksum manifest", errors):
                parsed = parse_checksum_file(checksum_path)
                for entry in checksums.get("entries", []):
                    if not isinstance(entry, dict):
                        errors.append(f"invalid checksum provenance entry: {entry}")
                        continue
                    verify_entry(entry, repo_root, errors)
                    entry_path = entry.get("path")
                    if isinstance(entry_path, str):
                        expected = checksum_hex(entry)
                        actual = parsed.get(entry_path)
                        if actual != expected:
                            errors.append(
                                f"checksum file entry for {entry_path} mismatch: expected {expected}, got {actual}"
                            )
        except (OSError, ValueError) as exc:
            errors.append(f"checksum manifest verification failed: {exc}")

    fail_if_errors(errors)
    print(f"selfhost-release-provenance: verification ok ({provenance_path})")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)

    generate_parser = subcommands.add_parser("generate")
    generate_parser.add_argument("--repo-root", default=os.getcwd())
    generate_parser.add_argument(
        "--bootstrap-report",
        default="target/selfhost-bootstrap/report.json",
    )
    generate_parser.add_argument("--out-dir", default="target/selfhost-release")
    generate_parser.add_argument(
        "--provenance",
        default="target/selfhost-release/provenance.json",
    )
    generate_parser.add_argument("--platform")
    generate_parser.add_argument("--source-commit")
    generate_parser.set_defaults(func=generate)

    verify_parser = subcommands.add_parser("verify")
    verify_parser.add_argument("--repo-root", default=os.getcwd())
    verify_parser.add_argument(
        "--provenance",
        default="target/selfhost-release/provenance.json",
    )
    verify_parser.set_defaults(func=verify)

    args = parser.parse_args()
    try:
        return args.func(args)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        print(f"selfhost-release-provenance: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
