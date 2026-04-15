#!/usr/bin/env python3
"""Compare AICore compiler behavior across reference and candidate commands."""

from __future__ import annotations

import argparse
import json
import os
import signal
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ACTION_ARGS: dict[str, tuple[str, ...]] = {
    "check": ("check", "{path}"),
    "check-json": ("check", "{path}", "--json"),
    "fmt": ("fmt", "{path}"),
    "fmt-check": ("fmt", "{path}", "--check"),
    "ir-json": ("ir", "{path}", "--emit", "json"),
    "build": ("build", "{path}", "-o", "{artifact}"),
    "run": ("run", "{path}"),
}


@dataclass(frozen=True)
class CommandResult:
    role: str
    command: list[str]
    exit_code: int | None
    timed_out: bool
    duration_ms: int
    stdout: str
    stderr: str
    artifact_exists: bool | None
    artifact_fingerprint: str | None
    stdout_json_fingerprint: str | None = None
    stdout_json_error: str | None = None

    def fingerprint(self, action: str) -> dict[str, Any]:
        stdout_fingerprint = fingerprint_text(self.stdout)
        comparison_kind = "text"
        if action == "ir-json" and self.stdout_json_fingerprint is not None:
            stdout_fingerprint = self.stdout_json_fingerprint
            comparison_kind = "canonical_json"
        return {
            "exit_code": self.exit_code,
            "timed_out": self.timed_out,
            "comparison_kind": comparison_kind,
            "stdout_fingerprint": stdout_fingerprint,
            "stderr_fingerprint": fingerprint_text(self.stderr),
            "artifact_exists": self.artifact_exists,
            "artifact_fingerprint": self.artifact_fingerprint,
            "stdout_json_fingerprint": self.stdout_json_fingerprint,
            "stdout_json_error": self.stdout_json_error,
        }


@dataclass(frozen=True)
class CaseResult:
    name: str
    path: str
    action: str
    ok: bool
    reason: str
    reference: CommandResult
    candidate: CommandResult

    def to_json(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "path": self.path,
            "action": self.action,
            "ok": self.ok,
            "reason": self.reason,
            "reference": {
                "command": self.reference.command,
                "duration_ms": self.reference.duration_ms,
                **self.reference.fingerprint(self.action),
            },
            "candidate": {
                "command": self.candidate.command,
                "duration_ms": self.candidate.duration_ms,
                **self.candidate.fingerprint(self.action),
            },
        }


def fingerprint_bytes(values: bytes) -> str:
    value = 0xCBF29CE484222325
    for byte in values:
        value ^= byte
        value = (value * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"fnv1a64:{value:016x}"


def fingerprint_text(value: str) -> str:
    return fingerprint_bytes(value.encode("utf-8"))


def fingerprint_file(path: Path) -> str:
    value = 0xCBF29CE484222325
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            for byte in chunk:
                value ^= byte
                value = (value * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"fnv1a64:{value:016x}"


def canonical_json_fingerprint(value: str) -> tuple[str | None, str | None]:
    try:
        parsed = json.loads(value)
    except json.JSONDecodeError as exc:
        return None, f"{exc.msg} at line {exc.lineno} column {exc.colno}"
    canonical = json.dumps(parsed, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return fingerprint_text(canonical), None


def parse_command(value: str) -> list[str]:
    parsed = shlex.split(value)
    if not parsed:
        raise argparse.ArgumentTypeError("command must not be empty")
    return parsed


def load_manifest(path: Path) -> dict[str, Any]:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"{path}: invalid JSON: {exc}") from exc

    if manifest.get("schema_version") != 1:
        raise SystemExit(f"{path}: expected schema_version 1")
    cases = manifest.get("cases")
    if not isinstance(cases, list) or not cases:
        raise SystemExit(f"{path}: cases must be a non-empty list")
    names: set[str] = set()
    for index, case in enumerate(cases):
        if not isinstance(case, dict):
            raise SystemExit(f"{path}: case {index} must be an object")
        name = case.get("name")
        source_path = case.get("path")
        actions = case.get("actions")
        expected = case.get("expected")
        if not isinstance(name, str) or not name:
            raise SystemExit(f"{path}: case {index} has invalid name")
        if name in names:
            raise SystemExit(f"{path}: duplicate case name {name}")
        names.add(name)
        if not isinstance(source_path, str) or not source_path:
            raise SystemExit(f"{path}: case {name} has invalid path")
        if not isinstance(actions, list) or not actions:
            raise SystemExit(f"{path}: case {name} actions must be a non-empty list")
        for action in actions:
            if action not in ACTION_ARGS:
                raise SystemExit(f"{path}: case {name} has unsupported action {action!r}")
        if expected not in {"pass", "fail"}:
            raise SystemExit(f"{path}: case {name} expected must be pass or fail")
    return manifest


def expand_action_args(
    action: str, source_path: Path, artifact_root: Path, role: str, case_name: str
) -> list[str]:
    artifact = artifact_path(artifact_root, role, case_name, action)
    values: list[str] = []
    for part in ACTION_ARGS[action]:
        values.append(part.format(path=str(source_path), artifact=str(artifact)))
    return values


def artifact_path(artifact_root: Path, role: str, case_name: str, action: str) -> Path:
    return artifact_root / role / case_name / action / "a.out"


def run_command(
    role: str,
    command_prefix: list[str],
    action: str,
    source_path: Path,
    artifact_root: Path,
    case_name: str,
    cwd: Path,
    timeout_seconds: float,
) -> CommandResult:
    action_args = expand_action_args(action, source_path, artifact_root, role, case_name)
    command = [*command_prefix, *action_args]
    started = time.monotonic()
    artifact = artifact_path(artifact_root, role, case_name, action)
    artifact_dir = artifact.parent
    artifact_dir.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.setdefault("AIC_SELFHOST_PARITY", "1")
    process: subprocess.Popen[str] | None = None
    try:
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=os.name != "nt",
        )
        stdout_raw, stderr_raw = process.communicate(timeout=timeout_seconds)
        timed_out = False
        exit_code: int | None = process.returncode
        stdout = normalize_output(stdout_raw, cwd, artifact_root)
        stderr = normalize_output(stderr_raw, cwd, artifact_root)
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        exit_code = None
        if process is not None:
            kill_process_tree(process)
            stdout_raw, stderr_raw = process.communicate()
        else:
            stdout_raw = decode_timeout_output(exc.stdout)
            stderr_raw = decode_timeout_output(exc.stderr)
        stdout = normalize_output(stdout_raw, cwd, artifact_root)
        stderr = normalize_output(stderr_raw, cwd, artifact_root)
    except Exception:
        if process is not None and process.poll() is None:
            kill_process_tree(process)
            process.communicate()
        raise
    duration_ms = int((time.monotonic() - started) * 1000)
    artifact_exists: bool | None = None
    artifact_fingerprint: str | None = None
    if action == "build":
        artifact_exists = artifact.exists()
        artifact_fingerprint = fingerprint_file(artifact) if artifact_exists else None
    stdout_json_fingerprint: str | None = None
    stdout_json_error: str | None = None
    if action == "ir-json":
        stdout_json_fingerprint, stdout_json_error = canonical_json_fingerprint(stdout)
    return CommandResult(
        role=role,
        command=command,
        exit_code=exit_code,
        timed_out=timed_out,
        duration_ms=duration_ms,
        stdout=stdout,
        stderr=stderr,
        artifact_exists=artifact_exists,
        artifact_fingerprint=artifact_fingerprint,
        stdout_json_fingerprint=stdout_json_fingerprint,
        stdout_json_error=stdout_json_error,
    )


def kill_process_tree(process: subprocess.Popen[str]) -> None:
    if os.name == "nt":
        process.kill()
        return
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        return


def decode_timeout_output(value: str | bytes | None) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return value


def normalize_output(value: str, cwd: Path, artifact_root: Path | None = None) -> str:
    root = str(cwd.resolve())
    text = value.replace(root, "$ROOT")
    if artifact_root is not None:
        artifact_abs = str(artifact_root.resolve())
        try:
            artifact_rel = os.path.relpath(artifact_root.resolve(), cwd.resolve())
        except ValueError:
            artifact_rel = str(artifact_root)
        for base in {artifact_abs, artifact_rel, str(artifact_root)}:
            text = text.replace(f"{base}/reference/", f"{base}/$ROLE/")
            text = text.replace(f"{base}/candidate/", f"{base}/$ROLE/")
    return text.replace("\r\n", "\n")


def expected_matches(exit_code: int | None, expected: str) -> bool:
    if exit_code is None:
        return False
    if expected == "pass":
        return exit_code == 0
    return exit_code != 0


def compare_results(
    name: str,
    path: str,
    action: str,
    expected: str,
    reference: CommandResult,
    candidate: CommandResult,
) -> CaseResult:
    if reference.timed_out or candidate.timed_out:
        return CaseResult(name, path, action, False, "timeout", reference, candidate)
    if not expected_matches(reference.exit_code, expected):
        return CaseResult(
            name,
            path,
            action,
            False,
            f"reference did not satisfy expected={expected}",
            reference,
            candidate,
        )
    if not expected_matches(candidate.exit_code, expected):
        return CaseResult(
            name,
            path,
            action,
            False,
            f"candidate did not satisfy expected={expected}",
            reference,
            candidate,
        )
    if action == "ir-json" and expected == "pass":
        if reference.stdout_json_error is not None:
            return CaseResult(
                name,
                path,
                action,
                False,
                "reference emitted invalid ir json",
                reference,
                candidate,
            )
        if candidate.stdout_json_error is not None:
            return CaseResult(
                name,
                path,
                action,
                False,
                "candidate emitted invalid ir json",
                reference,
                candidate,
            )
    if reference.fingerprint(action) != candidate.fingerprint(action):
        return CaseResult(name, path, action, False, "fingerprint mismatch", reference, candidate)
    return CaseResult(name, path, action, True, "matched", reference, candidate)


def run_manifest(args: argparse.Namespace, manifest: dict[str, Any]) -> list[CaseResult]:
    cwd = args.root.resolve()
    validate_source_paths(manifest, cwd)
    results: list[CaseResult] = []
    for case in manifest["cases"]:
        source_path = cwd / case["path"]
        for action in case["actions"]:
            reference = run_command(
                "reference",
                args.reference,
                action,
                source_path,
                args.artifact_dir,
                case["name"],
                cwd,
                args.timeout,
            )
            candidate = run_command(
                "candidate",
                args.candidate,
                action,
                source_path,
                args.artifact_dir,
                case["name"],
                cwd,
                args.timeout,
            )
            results.append(
                compare_results(
                    case["name"],
                    case["path"],
                    action,
                    case["expected"],
                    reference,
                    candidate,
                )
            )
    return results


def validate_source_paths(manifest: dict[str, Any], cwd: Path) -> None:
    for case in manifest["cases"]:
        source_path = cwd / case["path"]
        if not source_path.exists():
            raise SystemExit(f"{case['name']}: source path does not exist: {case['path']}")


def write_report(path: Path, manifest: dict[str, Any], results: list[CaseResult]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "format": "aicore-selfhost-parity-v1",
        "manifest": {
            "name": manifest.get("name", ""),
            "schema_version": manifest["schema_version"],
            "case_count": len(manifest["cases"]),
        },
        "ok": all(result.ok for result in results),
        "results": [result.to_json() for result in results],
    }
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def print_summary(results: list[CaseResult]) -> None:
    total = len(results)
    failures = [result for result in results if not result.ok]
    print(f"selfhost-parity: {total - len(failures)}/{total} comparisons matched")
    for failure in failures:
        print(
            f"selfhost-parity: FAIL {failure.name} {failure.action}: {failure.reason}",
            file=sys.stderr,
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=Path("tests/selfhost/parity_manifest.json"))
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--reference",
        type=parse_command,
        default=parse_command("cargo run --quiet --bin aic --"),
        help="reference compiler command prefix",
    )
    parser.add_argument(
        "--candidate",
        type=parse_command,
        default=None,
        help="candidate compiler command prefix; defaults to --reference",
    )
    parser.add_argument("--artifact-dir", type=Path, default=Path("target/selfhost-parity"))
    parser.add_argument("--report", type=Path, default=Path("target/selfhost-parity/report.json"))
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--list", action="store_true", help="validate and list manifest cases")
    args = parser.parse_args()
    if args.timeout <= 0:
        raise SystemExit("--timeout must be greater than zero")
    if args.candidate is None:
        args.candidate = args.reference

    manifest = load_manifest(args.manifest)
    if args.list:
        validate_source_paths(manifest, args.root.resolve())
        for case in manifest["cases"]:
            print(f"{case['name']} {case['expected']} {','.join(case['actions'])} {case['path']}")
        return 0

    results = run_manifest(args, manifest)
    write_report(args.report, manifest, results)
    print_summary(results)
    return 0 if all(result.ok for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
