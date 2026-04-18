#!/usr/bin/env python3
"""Validate a staged AICore self-host compiler against package/example inputs."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
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
    "ir-json": ("ir", "{path}", "--emit", "json"),
    "build": ("build", "{path}", "-o", "{artifact}"),
    "run": ("run", "{path}"),
}
KINDS = {"single-file", "package", "package-member", "workspace"}
EXPECTED = {"pass", "fail", "unsupported"}
DIAGNOSTIC_CODE_RE = re.compile(r"\bE\d{4}\b")
DEFAULT_EXCERPT_CHARS = 2400


@dataclass(frozen=True)
class MatrixResult:
    name: str
    kind: str
    path: str
    action: str
    expected: str
    readiness: bool
    status: str
    reason: str
    command: list[str]
    exit_code: int | None
    timed_out: bool
    duration_ms: int
    timeout_seconds: float
    stdout_path: str
    stderr_path: str
    stdout_excerpt: str
    stderr_excerpt: str
    stdout_sha256: str
    stderr_sha256: str
    stdout_json_sha256: str | None
    stdout_json_error: str | None
    artifact_path: str | None
    artifact_exists: bool | None
    artifact_sha256: str | None
    artifact_size_bytes: int | None
    diagnostic_codes: list[str]

    @property
    def gate_ok(self) -> bool:
        return self.status in {"passed", "unsupported"}

    def to_json(self) -> dict[str, object]:
        return {
            "name": self.name,
            "kind": self.kind,
            "path": self.path,
            "action": self.action,
            "expected": self.expected,
            "readiness": self.readiness,
            "status": self.status,
            "reason": self.reason,
            "command": self.command,
            "exit_code": self.exit_code,
            "timed_out": self.timed_out,
            "duration_ms": self.duration_ms,
            "timeout_seconds": self.timeout_seconds,
            "stdout_path": self.stdout_path,
            "stderr_path": self.stderr_path,
            "stdout_excerpt": self.stdout_excerpt,
            "stderr_excerpt": self.stderr_excerpt,
            "stdout_sha256": self.stdout_sha256,
            "stderr_sha256": self.stderr_sha256,
            "stdout_json_sha256": self.stdout_json_sha256,
            "stdout_json_error": self.stdout_json_error,
            "artifact_path": self.artifact_path,
            "artifact_exists": self.artifact_exists,
            "artifact_sha256": self.artifact_sha256,
            "artifact_size_bytes": self.artifact_size_bytes,
            "diagnostic_codes": self.diagnostic_codes,
        }


def parse_command(value: str) -> list[str]:
    parsed = shlex.split(value)
    if not parsed:
        raise argparse.ArgumentTypeError("command must not be empty")
    return parsed


def valid_timeout(value: Any) -> bool:
    if isinstance(value, bool):
        return False
    return isinstance(value, (int, float)) and value > 0


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
        validate_case(path, index, case, names)
    return manifest


def validate_case(path: Path, index: int, case: Any, names: set[str]) -> None:
    if not isinstance(case, dict):
        raise SystemExit(f"{path}: case {index} must be an object")
    name = case.get("name")
    source_path = case.get("path")
    kind = case.get("kind")
    expected = case.get("expected")
    actions = case.get("actions")
    if not isinstance(name, str) or not name:
        raise SystemExit(f"{path}: case {index} has invalid name")
    if name in names:
        raise SystemExit(f"{path}: duplicate case name {name}")
    names.add(name)
    if not isinstance(source_path, str) or not source_path:
        raise SystemExit(f"{path}: case {name} has invalid path")
    if kind not in KINDS:
        raise SystemExit(f"{path}: case {name} kind must be one of {sorted(KINDS)}")
    if expected not in EXPECTED:
        raise SystemExit(f"{path}: case {name} expected must be pass, fail, or unsupported")
    if expected == "unsupported" and case.get("readiness", False) is True:
        raise SystemExit(f"{path}: case {name} unsupported cases cannot count as readiness coverage")
    if not isinstance(actions, list) or not actions:
        raise SystemExit(f"{path}: case {name} actions must be a non-empty list")
    for action in actions:
        if action not in ACTION_ARGS:
            raise SystemExit(f"{path}: case {name} has unsupported action {action!r}")
    case_timeout = case.get("timeout")
    if case_timeout is not None and not valid_timeout(case_timeout):
        raise SystemExit(f"{path}: case {name} timeout must be a positive number")
    action_timeouts = case.get("timeouts", {})
    if not isinstance(action_timeouts, dict):
        raise SystemExit(f"{path}: case {name} timeouts must be an object when present")
    for action, timeout in action_timeouts.items():
        if action not in actions:
            raise SystemExit(f"{path}: case {name} timeout for non-case action {action!r}")
        if not valid_timeout(timeout):
            raise SystemExit(f"{path}: case {name} action {action} timeout must be positive")
    diagnostic_codes = case.get("diagnostic_codes", {})
    if not isinstance(diagnostic_codes, dict):
        raise SystemExit(f"{path}: case {name} diagnostic_codes must be an object when present")
    for action, codes in diagnostic_codes.items():
        if action not in actions:
            raise SystemExit(f"{path}: case {name} diagnostic code for non-case action {action!r}")
        if not isinstance(codes, list) or not codes:
            raise SystemExit(f"{path}: case {name} action {action} diagnostic codes must be a list")
        for code in codes:
            if not isinstance(code, str) or not DIAGNOSTIC_CODE_RE.fullmatch(code):
                raise SystemExit(f"{path}: case {name} action {action} invalid diagnostic code {code!r}")
    if expected in {"fail", "unsupported"}:
        for action in actions:
            if action not in diagnostic_codes:
                raise SystemExit(
                    f"{path}: case {name} action {action} requires diagnostic_codes for expected={expected}"
                )


def validate_source_paths(manifest: dict[str, Any], root: Path) -> None:
    for case in manifest["cases"]:
        source_path = root / case["path"]
        if not source_path.exists():
            raise SystemExit(f"{case['name']}: source path does not exist: {case['path']}")


def timeout_for_action(case: dict[str, Any], action: str, default_timeout: float) -> float:
    action_timeouts = case.get("timeouts", {})
    if action in action_timeouts:
        return float(action_timeouts[action])
    if "timeout" in case:
        return float(case["timeout"])
    return default_timeout


def artifact_path(artifact_root: Path, case_name: str, action: str) -> Path:
    return artifact_root / "artifacts" / case_name / action / "a.out"


def output_path(artifact_root: Path, case_name: str, action: str, stream: str) -> Path:
    return artifact_root / "outputs" / case_name / action / f"{stream}.txt"


def expand_action_args(action: str, source_path: Path, artifact_root: Path, case_name: str) -> list[str]:
    artifact = artifact_path(artifact_root, case_name, action)
    values: list[str] = []
    for part in ACTION_ARGS[action]:
        values.append(part.format(path=str(source_path), artifact=str(artifact)))
    return values


def run_command(
    stage_compiler: list[str],
    case: dict[str, Any],
    action: str,
    root: Path,
    artifact_root: Path,
    timeout_seconds: float,
) -> MatrixResult:
    source_path = root / case["path"]
    command = [*stage_compiler, *expand_action_args(action, source_path, artifact_root, case["name"])]
    artifact = artifact_path(artifact_root, case["name"], action) if action == "build" else None
    if artifact is not None:
        artifact.parent.mkdir(parents=True, exist_ok=True)
    stdout_file = output_path(artifact_root, case["name"], action, "stdout")
    stderr_file = output_path(artifact_root, case["name"], action, "stderr")
    stdout_file.parent.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.setdefault("AIC_SELFHOST_STAGE_MATRIX", "1")
    process: subprocess.Popen[str] | None = None
    started = time.monotonic()
    try:
        process = subprocess.Popen(
            command,
            cwd=root,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=os.name != "nt",
        )
        stdout, stderr = process.communicate(timeout=timeout_seconds)
        timed_out = False
        exit_code: int | None = process.returncode
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        exit_code = None
        if process is not None:
            kill_process_tree(process)
            stdout, stderr = process.communicate()
        else:
            stdout = decode_timeout_output(exc.stdout)
            stderr = decode_timeout_output(exc.stderr)
        stderr = f"{stderr}\nstage matrix action timed out after {timeout_seconds}s".strip()
    except Exception:
        if process is not None and process.poll() is None:
            kill_process_tree(process)
            process.communicate()
        raise
    duration_ms = int((time.monotonic() - started) * 1000)
    stdout_file.write_text(stdout, encoding="utf-8")
    stderr_file.write_text(stderr, encoding="utf-8")
    stdout_json_sha256: str | None = None
    stdout_json_error: str | None = None
    if action == "ir-json" and not timed_out:
        stdout_json_sha256, stdout_json_error = canonical_json_sha256(stdout)
    diagnostics = diagnostic_codes(stdout, stderr)
    status, reason = classify_result(case, action, exit_code, timed_out, diagnostics, stdout_json_error, artifact)
    return MatrixResult(
        name=case["name"],
        kind=case["kind"],
        path=case["path"],
        action=action,
        expected=case["expected"],
        readiness=readiness_case(case),
        status=status,
        reason=reason,
        command=command,
        exit_code=exit_code,
        timed_out=timed_out,
        duration_ms=duration_ms,
        timeout_seconds=timeout_seconds,
        stdout_path=normalize_path(stdout_file, root),
        stderr_path=normalize_path(stderr_file, root),
        stdout_excerpt=excerpt(normalize_output(stdout, root, artifact_root)),
        stderr_excerpt=excerpt(normalize_output(stderr, root, artifact_root)),
        stdout_sha256=sha256_text(stdout),
        stderr_sha256=sha256_text(stderr),
        stdout_json_sha256=stdout_json_sha256,
        stdout_json_error=stdout_json_error,
        artifact_path=normalize_path(artifact, root) if artifact is not None else None,
        artifact_exists=artifact.exists() if artifact is not None else None,
        artifact_sha256=sha256_file(artifact) if artifact is not None else None,
        artifact_size_bytes=artifact.stat().st_size if artifact is not None and artifact.is_file() else None,
        diagnostic_codes=diagnostics,
    )


def classify_result(
    case: dict[str, Any],
    action: str,
    exit_code: int | None,
    timed_out: bool,
    diagnostics: list[str],
    stdout_json_error: str | None,
    artifact: Path | None,
) -> tuple[str, str]:
    if timed_out:
        return "failed", "action timed out"
    expected = case["expected"]
    if expected == "pass":
        if exit_code != 0:
            return "failed", f"expected pass but exited {exit_code}"
        if action == "ir-json" and stdout_json_error is not None:
            return "failed", f"invalid ir json: {stdout_json_error}"
        if action == "build" and (artifact is None or not artifact.is_file()):
            return "failed", "build action did not create an artifact"
        return "passed", "matched expected pass"
    expected_codes = expected_diagnostic_codes(case, action)
    missing_codes = [code for code in expected_codes if code not in diagnostics]
    if expected == "fail":
        if exit_code == 0:
            return "failed", "expected diagnostic failure but action passed"
        if missing_codes:
            return "failed", f"missing expected diagnostic codes: {', '.join(missing_codes)}"
        return "passed", "matched expected diagnostic failure"
    if exit_code == 0:
        return "failed", "unsupported case unexpectedly passed"
    if missing_codes:
        return "failed", f"unsupported case missed expected diagnostic codes: {', '.join(missing_codes)}"
    return "unsupported", "explicit unsupported non-readiness case"


def readiness_case(case: dict[str, Any]) -> bool:
    if "readiness" in case:
        return bool(case["readiness"])
    return case["expected"] != "unsupported"


def expected_diagnostic_codes(case: dict[str, Any], action: str) -> list[str]:
    return list(case.get("diagnostic_codes", {}).get(action, []))


def diagnostic_codes(stdout: str, stderr: str) -> list[str]:
    return DIAGNOSTIC_CODE_RE.findall(f"{stdout}\n{stderr}")


def canonical_json_sha256(value: str) -> tuple[str | None, str | None]:
    try:
        parsed = json.loads(value)
    except json.JSONDecodeError as exc:
        return None, f"{exc.msg} at line {exc.lineno} column {exc.colno}"
    canonical = json.dumps(parsed, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return sha256_text(canonical), None


def sha256_text(value: str) -> str:
    return f"sha256:{hashlib.sha256(value.encode('utf-8')).hexdigest()}"


def sha256_file(path: Path | None) -> str | None:
    if path is None or not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


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


def normalize_path(path: Path | None, root: Path) -> str | None:
    if path is None:
        return None
    try:
        return str(path.resolve().relative_to(root.resolve()))
    except ValueError:
        return str(path)


def normalize_output(value: str, root: Path, artifact_root: Path) -> str:
    text = value.replace(str(root.resolve()), "$ROOT")
    text = text.replace(str(artifact_root.resolve()), "$ARTIFACTS")
    return text.replace("\r\n", "\n")


def excerpt(value: str, limit: int = DEFAULT_EXCERPT_CHARS) -> str:
    if len(value) <= limit:
        return value
    head = value[: limit // 2]
    tail = value[-(limit // 2) :]
    return f"{head}\n... output truncated ...\n{tail}"


def run_manifest(args: argparse.Namespace, manifest: dict[str, Any]) -> list[MatrixResult]:
    root = args.root.resolve()
    artifact_root = args.artifact_dir
    if not artifact_root.is_absolute():
        artifact_root = root / artifact_root
    validate_source_paths(manifest, root)
    results: list[MatrixResult] = []
    for case in manifest["cases"]:
        for action in case["actions"]:
            results.append(
                run_command(
                    args.stage_compiler,
                    case,
                    action,
                    root,
                    artifact_root,
                    timeout_for_action(case, action, args.timeout),
                )
            )
    return results


def summarize(results: list[MatrixResult]) -> dict[str, object]:
    by_kind: dict[str, dict[str, int]] = {}
    by_action: dict[str, dict[str, int]] = {}
    for result in results:
        add_summary(by_kind, result.kind, result.status)
        add_summary(by_action, result.action, result.status)
    readiness_results = [result for result in results if result.readiness]
    return {
        "result_count": len(results),
        "passed": sum(1 for result in results if result.status == "passed"),
        "failed": sum(1 for result in results if result.status == "failed"),
        "unsupported": sum(1 for result in results if result.status == "unsupported"),
        "readiness_result_count": len(readiness_results),
        "readiness_passed": sum(1 for result in readiness_results if result.status == "passed"),
        "readiness_failed": sum(1 for result in readiness_results if result.status == "failed"),
        "by_kind": by_kind,
        "by_action": by_action,
    }


def add_summary(summary: dict[str, dict[str, int]], key: str, status: str) -> None:
    if key not in summary:
        summary[key] = {"passed": 0, "failed": 0, "unsupported": 0}
    summary[key][status] += 1


def write_report(
    path: Path,
    manifest: dict[str, Any],
    args: argparse.Namespace,
    results: list[MatrixResult],
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "format": "aicore-selfhost-stage-matrix-v1",
        "manifest": {
            "name": manifest.get("name", ""),
            "schema_version": manifest["schema_version"],
            "case_count": len(manifest["cases"]),
        },
        "stage_compiler": args.stage_compiler,
        "artifact_dir": str(args.artifact_dir),
        "ok": all(result.gate_ok for result in results),
        "summary": summarize(results),
        "results": [result.to_json() for result in results],
    }
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def print_summary(results: list[MatrixResult]) -> None:
    summary = summarize(results)
    print(
        "selfhost-stage-matrix: "
        f"{summary['passed']} passed, {summary['unsupported']} unsupported, "
        f"{summary['failed']} failed"
    )
    for result in results:
        if result.status == "failed":
            print(
                f"selfhost-stage-matrix: FAIL {result.name} {result.action}: {result.reason}",
                file=sys.stderr,
            )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=Path("tests/selfhost/stage_matrix_manifest.json"))
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--stage-compiler",
        type=parse_command,
        default=parse_command("target/selfhost-bootstrap/stage2/aic_selfhost"),
        help="staged compiler command prefix",
    )
    parser.add_argument("--artifact-dir", type=Path, default=Path("target/selfhost-stage-matrix"))
    parser.add_argument("--report", type=Path, default=Path("target/selfhost-stage-matrix/report.json"))
    parser.add_argument("--timeout", type=float, default=90.0)
    parser.add_argument("--list", action="store_true", help="validate and list manifest cases")
    args = parser.parse_args()
    if args.timeout <= 0:
        raise SystemExit("--timeout must be greater than zero")
    manifest = load_manifest(args.manifest)
    if args.list:
        validate_source_paths(manifest, args.root.resolve())
        for case in manifest["cases"]:
            readiness = "readiness" if readiness_case(case) else "non-readiness"
            print(
                f"{case['name']} {case['kind']} {case['expected']} "
                f"{','.join(case['actions'])} {readiness} {case['path']}"
            )
        return 0
    results = run_manifest(args, manifest)
    write_report(args.report, manifest, args, results)
    print_summary(results)
    return 0 if all(result.gate_ok for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
