#!/usr/bin/env python3
"""Generic external protocol integration harness.

This harness is intentionally protocol-agnostic: matrix entries provide
service/version/auth/security metadata plus the commands needed to stand up
containers and run smoke checks.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MATRIX = ROOT / "tests/integration/protocol-harness.matrix.json"
DEFAULT_REPORT = ROOT / "target/e8/integration-harness-report.json"


def _parse_csv(raw: str | None) -> set[str]:
    if not raw:
        return set()
    return {part.strip() for part in raw.split(",") if part.strip()}


def _run_command(command: str, env: dict[str, str], case_id: str) -> subprocess.CompletedProcess[str]:
    print(f"[{case_id}] $ {command}")
    return subprocess.run(
        command,
        shell=True,
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
    )


def _run_with_retries(
    command: str,
    retries: int,
    sleep_seconds: float,
    env: dict[str, str],
    case_id: str,
) -> tuple[bool, str]:
    last_error = ""
    for attempt in range(1, retries + 1):
        completed = _run_command(command, env, case_id)
        if completed.returncode == 0:
            return True, ""
        last_error = (
            f"attempt {attempt}/{retries} failed (exit={completed.returncode})\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
        if attempt < retries:
            time.sleep(sleep_seconds)
    return False, last_error


def _write_report(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _required_field(case: dict[str, Any], key: str) -> str:
    value = case.get(key, "")
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"case `{case.get('id', '<unknown>')}` missing required field `{key}`")
    return value.strip()


def _validate_matrix(matrix: dict[str, Any]) -> None:
    if matrix.get("schema_version") != 1:
        raise ValueError("protocol harness matrix must pin `schema_version` to 1")
    for group in ("offline_cases", "live_cases"):
        cases = matrix.get(group, [])
        if not isinstance(cases, list):
            raise ValueError(f"`{group}` must be an array")
        for case in cases:
            if not isinstance(case, dict):
                raise ValueError(f"entries in `{group}` must be objects")
            for field in ("id", "service", "version", "auth", "security"):
                _required_field(case, field)
            if group == "offline_cases":
                _required_field(case, "command")
            else:
                _required_field(case, "compose_file")
                _required_field(case, "healthcheck_cmd")
                _required_field(case, "smoke_cmd")


def _select_cases(
    cases: list[dict[str, Any]],
    services: set[str],
    case_ids: set[str],
    max_cases: int,
) -> list[dict[str, Any]]:
    selected: list[dict[str, Any]] = []
    for case in cases:
        service = str(case.get("service", "")).strip()
        case_id = str(case.get("id", "")).strip()
        if services and service not in services:
            continue
        if case_ids and case_id not in case_ids:
            continue
        selected.append(case)
    if max_cases > 0:
        return selected[:max_cases]
    return selected


def _case_env(case: dict[str, Any]) -> dict[str, str]:
    env = dict(os.environ)
    env.setdefault("AIC_STD_ROOT", str(ROOT / "std"))
    extra = case.get("env", {})
    if isinstance(extra, dict):
        for key, value in extra.items():
            env[str(key)] = str(value)
    return env


def _run_offline_case(case: dict[str, Any]) -> tuple[bool, str]:
    case_id = str(case["id"])
    env = _case_env(case)
    completed = _run_command(str(case["command"]), env, case_id)
    if completed.returncode == 0:
        return True, ""
    return (
        False,
        (
            f"offline case failed (exit={completed.returncode})\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        ),
    )


def _run_live_case(case: dict[str, Any]) -> tuple[bool, str]:
    case_id = str(case["id"])
    env = _case_env(case)
    compose_file = str(case["compose_file"])

    up_cmd = str(case.get("up_cmd") or f"docker compose -f {compose_file} up -d")
    down_cmd = str(
        case.get("down_cmd") or f"docker compose -f {compose_file} down -v --remove-orphans"
    )
    healthcheck_cmd = str(case["healthcheck_cmd"])
    smoke_cmd = str(case["smoke_cmd"])
    retries = int(case.get("healthcheck_retries", 30))
    sleep_seconds = float(case.get("healthcheck_sleep_seconds", 2))

    up_completed = _run_command(up_cmd, env, case_id)
    if up_completed.returncode != 0:
        return (
            False,
            (
                f"compose up failed (exit={up_completed.returncode})\n"
                f"stdout:\n{up_completed.stdout}\n"
                f"stderr:\n{up_completed.stderr}"
            ),
        )

    error = ""
    ok = True
    try:
        healthy, health_error = _run_with_retries(
            healthcheck_cmd,
            retries,
            sleep_seconds,
            env,
            case_id,
        )
        if not healthy:
            ok = False
            error = f"healthcheck failed after retries\n{health_error}"
            return ok, error

        smoke_completed = _run_command(smoke_cmd, env, case_id)
        if smoke_completed.returncode != 0:
            ok = False
            error = (
                f"smoke command failed (exit={smoke_completed.returncode})\n"
                f"stdout:\n{smoke_completed.stdout}\n"
                f"stderr:\n{smoke_completed.stderr}"
            )
        return ok, error
    finally:
        down_completed = _run_command(down_cmd, env, case_id)
        if down_completed.returncode != 0 and ok:
            ok = False
            error = (
                f"compose down failed (exit={down_completed.returncode})\n"
                f"stdout:\n{down_completed.stdout}\n"
                f"stderr:\n{down_completed.stderr}"
            )


def main() -> int:
    parser = argparse.ArgumentParser(description="AICore external protocol integration harness")
    parser.add_argument(
        "--mode",
        choices=("offline", "live"),
        default=os.getenv("AIC_INTEGRATION_MODE", "offline"),
    )
    parser.add_argument(
        "--matrix",
        default=str(os.getenv("AIC_INTEGRATION_MATRIX", DEFAULT_MATRIX)),
        help="Path to protocol harness matrix json",
    )
    parser.add_argument(
        "--services",
        default=os.getenv("AIC_INTEGRATION_SERVICES", ""),
        help="Optional comma-separated service filter",
    )
    parser.add_argument(
        "--case-ids",
        default=os.getenv("AIC_INTEGRATION_CASE_IDS", ""),
        help="Optional comma-separated case-id filter",
    )
    parser.add_argument(
        "--max-cases",
        type=int,
        default=int(os.getenv("AIC_INTEGRATION_MAX_CASES", "0") or "0"),
        help="Optional max number of selected cases to execute",
    )
    parser.add_argument(
        "--report",
        default=os.getenv("AIC_INTEGRATION_REPORT", str(DEFAULT_REPORT)),
        help="Output report path",
    )
    args = parser.parse_args()

    matrix_path = Path(args.matrix)
    report_path = Path(args.report)
    services = _parse_csv(args.services)
    case_ids = _parse_csv(args.case_ids)

    matrix = json.loads(matrix_path.read_text(encoding="utf-8"))
    _validate_matrix(matrix)

    mode_key = "offline_cases" if args.mode == "offline" else "live_cases"
    default_limit = int(matrix.get("live_smoke_default_max_cases", 0))
    max_cases = args.max_cases if args.max_cases > 0 else (default_limit if args.mode == "live" else 0)
    cases = _select_cases(list(matrix.get(mode_key, [])), services, case_ids, max_cases)

    if args.mode == "live" and os.getenv("AIC_INTEGRATION_LIVE", "0") != "1":
        payload = {
            "mode": args.mode,
            "status": "skipped",
            "reason": "set AIC_INTEGRATION_LIVE=1 to enable live container integration runs",
            "selected_case_count": len(cases),
            "cases": [],
        }
        _write_report(report_path, payload)
        print(payload["reason"])
        return 0

    if args.mode == "live" and shutil.which("docker") is None:
        payload = {
            "mode": args.mode,
            "status": "failed",
            "reason": "docker is required for live integration mode",
            "selected_case_count": len(cases),
            "cases": [],
        }
        _write_report(report_path, payload)
        print(payload["reason"], file=sys.stderr)
        return 1

    results: list[dict[str, Any]] = []
    failed = 0
    started = time.time()
    for case in cases:
        case_started = time.time()
        if args.mode == "offline":
            ok, error = _run_offline_case(case)
        else:
            ok, error = _run_live_case(case)
        elapsed_ms = int((time.time() - case_started) * 1000.0)
        status = "passed" if ok else "failed"
        if not ok:
            failed += 1
        results.append(
            {
                "id": case["id"],
                "service": case["service"],
                "version": case["version"],
                "auth": case["auth"],
                "security": case["security"],
                "status": status,
                "elapsed_ms": elapsed_ms,
                "error": error,
            }
        )

    summary = {
        "mode": args.mode,
        "status": "passed" if failed == 0 else "failed",
        "selected_case_count": len(cases),
        "failed_case_count": failed,
        "elapsed_ms": int((time.time() - started) * 1000.0),
        "cases": results,
    }
    _write_report(report_path, summary)

    print(
        f"integration harness mode={args.mode} selected={len(cases)} failed={failed} "
        f"report={report_path}"
    )
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
