#!/usr/bin/env python3
"""Deterministic REST runtime perf/soak gate for parse/router/json/async churn paths."""

from __future__ import annotations

import argparse
import json
import os
import shlex
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_POLICY = ROOT / "benchmarks/service_baseline/rest-runtime-soak-gate.v1.json"
DEFAULT_REPORT = ROOT / "target/e8/rest-runtime-soak-report.json"


def host_target_label() -> str:
    if sys.platform.startswith("linux"):
        return "linux"
    if sys.platform == "darwin":
        return "macos"
    if sys.platform.startswith("win"):
        return "windows"
    raise RuntimeError(f"unsupported host target: {sys.platform}")


def read_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise SystemExit(f"missing policy file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid json in {path}: {exc}") from exc


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def command_str(command: list[str]) -> str:
    return " ".join(shlex.quote(part) for part in command)


def truncate(text: str, limit: int = 600) -> str:
    compact = text.strip()
    if len(compact) <= limit:
        return compact
    return compact[:limit] + "..."


def run_command(command: list[str], cwd: Path, env: dict[str, str]) -> dict[str, Any]:
    start = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )
    elapsed_ms = (time.perf_counter() - start) * 1000.0
    return {
        "command": command,
        "elapsed_ms": round(elapsed_ms, 3),
        "returncode": completed.returncode,
        "stdout_tail": truncate(completed.stdout),
        "stderr_tail": truncate(completed.stderr),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--policy",
        type=Path,
        default=DEFAULT_POLICY,
        help="Path to REST runtime soak policy json",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_REPORT,
        help="Path to write aggregate soak report json",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Validate policy and print planned commands without executing them",
    )
    parser.add_argument(
        "--update-baseline",
        action="store_true",
        help="Update host-target baseline_ms values in the policy from observed medians",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    policy_path = args.policy.resolve()
    policy = read_json(policy_path)

    schema_version = int(policy.get("schema_version", 1))
    warmup_runs = int(policy.get("warmup_runs", 1))
    measure_runs = int(policy.get("measure_runs", 3))
    tolerance_pct = float(policy.get("regression_tolerance_pct", 40.0))
    preflight_commands = policy.get("preflight", [])
    scenarios = policy.get("scenarios", [])
    common_env = {
        str(k): str(v)
        for k, v in (policy.get("env") or {}).items()
    }

    if not scenarios:
        raise SystemExit("policy must contain at least one scenario")
    if measure_runs < 1:
        raise SystemExit("measure_runs must be >= 1")
    if warmup_runs < 0:
        raise SystemExit("warmup_runs must be >= 0")

    target = host_target_label()
    run_env = os.environ.copy()
    run_env.update(common_env)

    if args.dry_run:
        print(f"rest runtime soak gate dry-run target={target}")
        print(f"policy={policy_path}")
        for idx, command in enumerate(preflight_commands):
            print(f"[preflight {idx + 1}] {command_str(command)}")
        for scenario in scenarios:
            thresholds = (scenario.get("thresholds") or {}).get(target)
            if thresholds is None:
                raise SystemExit(
                    f"scenario `{scenario.get('id', '<missing>')}` missing thresholds for target `{target}`"
                )
            print(
                f"[scenario] {scenario.get('id')} baseline={thresholds.get('baseline_ms')}ms "
                f"max={thresholds.get('max_ms')}ms cmd={command_str(scenario.get('command', []))}"
            )
        return 0

    for idx, command in enumerate(preflight_commands):
        if not isinstance(command, list) or not command:
            raise SystemExit(f"invalid preflight command at index {idx}")
        result = run_command(command, ROOT, run_env)
        if result["returncode"] != 0:
            print(
                f"[FAIL] preflight command #{idx + 1} rc={result['returncode']} "
                f"cmd={command_str(command)}"
            )
            if result["stdout_tail"]:
                print(f"stdout: {result['stdout_tail']}")
            if result["stderr_tail"]:
                print(f"stderr: {result['stderr_tail']}")
            return 1

    results: list[dict[str, Any]] = []
    violations: list[str] = []

    for scenario in scenarios:
        scenario_id = str(scenario.get("id", "")).strip()
        description = str(scenario.get("description", "")).strip()
        command = scenario.get("command")
        if not scenario_id:
            raise SystemExit("scenario id is required")
        if not isinstance(command, list) or not command:
            raise SystemExit(f"scenario `{scenario_id}` must define non-empty command list")

        thresholds = (scenario.get("thresholds") or {}).get(target)
        if thresholds is None:
            raise SystemExit(f"scenario `{scenario_id}` missing thresholds for target `{target}`")

        baseline_ms = float(thresholds["baseline_ms"])
        max_ms = float(thresholds["max_ms"])
        regression_limit_ms = baseline_ms * (1.0 + (tolerance_pct / 100.0))

        scenario_env = run_env.copy()
        scenario_env.update({str(k): str(v) for k, v in (scenario.get("env") or {}).items()})

        print(f"[RUN] {scenario_id}: {description}")
        for warmup_idx in range(warmup_runs):
            warmup = run_command(command, ROOT, scenario_env)
            if warmup["returncode"] != 0:
                message = (
                    f"{scenario_id}: warmup #{warmup_idx + 1} failed rc={warmup['returncode']}"
                )
                violations.append(message)
                results.append(
                    {
                        "id": scenario_id,
                        "description": description,
                        "command": command,
                        "samples_ms": [],
                        "observed_ms": None,
                        "baseline_ms": round(baseline_ms, 3),
                        "max_ms": round(max_ms, 3),
                        "regression_limit_ms": round(regression_limit_ms, 3),
                        "delta_ms": None,
                        "delta_pct": None,
                        "within_budget": False,
                        "within_regression_limit": False,
                        "status": "command_failed",
                        "warmup_failure": warmup,
                    }
                )
                print(f"[FAIL] {message}")
                break
        else:
            samples: list[float] = []
            command_failure: dict[str, Any] | None = None
            for run_idx in range(measure_runs):
                measured = run_command(command, ROOT, scenario_env)
                if measured["returncode"] != 0:
                    command_failure = measured
                    break
                samples.append(float(measured["elapsed_ms"]))

            if command_failure is not None:
                message = (
                    f"{scenario_id}: run failed rc={command_failure['returncode']} "
                    f"cmd={command_str(command)}"
                )
                violations.append(message)
                results.append(
                    {
                        "id": scenario_id,
                        "description": description,
                        "command": command,
                        "samples_ms": [round(value, 3) for value in samples],
                        "observed_ms": None,
                        "baseline_ms": round(baseline_ms, 3),
                        "max_ms": round(max_ms, 3),
                        "regression_limit_ms": round(regression_limit_ms, 3),
                        "delta_ms": None,
                        "delta_pct": None,
                        "within_budget": False,
                        "within_regression_limit": False,
                        "status": "command_failed",
                        "command_failure": command_failure,
                    }
                )
                print(f"[FAIL] {message}")
                continue

            observed_ms = float(statistics.median(samples))
            delta_ms = observed_ms - baseline_ms
            delta_pct = 0.0 if baseline_ms == 0 else (delta_ms / baseline_ms) * 100.0
            within_budget = observed_ms <= max_ms
            within_regression = observed_ms <= regression_limit_ms
            passed = within_budget and within_regression

            results.append(
                {
                    "id": scenario_id,
                    "description": description,
                    "command": command,
                    "samples_ms": [round(value, 3) for value in samples],
                    "observed_ms": round(observed_ms, 3),
                    "baseline_ms": round(baseline_ms, 3),
                    "max_ms": round(max_ms, 3),
                    "regression_limit_ms": round(regression_limit_ms, 3),
                    "delta_ms": round(delta_ms, 3),
                    "delta_pct": round(delta_pct, 3),
                    "within_budget": within_budget,
                    "within_regression_limit": within_regression,
                    "status": "pass" if passed else "threshold_failed",
                }
            )

            summary = (
                f"observed={observed_ms:.3f}ms baseline={baseline_ms:.3f}ms "
                f"delta={delta_pct:+.2f}% max={max_ms:.3f}ms "
                f"reg_limit={regression_limit_ms:.3f}ms"
            )
            if passed:
                print(f"[PASS] {scenario_id}: {summary}")
            else:
                violation = f"{scenario_id}: {summary}"
                violations.append(violation)
                print(f"[FAIL] {scenario_id}: {summary}")

    report = {
        "schema_version": schema_version,
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "target": target,
        "policy_path": str(policy_path),
        "warmup_runs": warmup_runs,
        "measure_runs": measure_runs,
        "regression_tolerance_pct": tolerance_pct,
        "results": results,
        "violations": violations,
    }

    output_path = args.output.resolve()
    write_json(output_path, report)
    target_output_path = output_path.with_name(
        f"{output_path.stem}-{target}{output_path.suffix}"
    )
    write_json(target_output_path, report)

    if args.update_baseline:
        scenario_lookup = {str(item.get("id")): item for item in scenarios}
        for item in results:
            if item.get("status") == "command_failed":
                continue
            observed = item.get("observed_ms")
            if observed is None:
                continue
            scenario_id = str(item["id"])
            thresholds = scenario_lookup[scenario_id]["thresholds"][target]
            thresholds["baseline_ms"] = round(float(observed), 3)
        write_json(policy_path, policy)
        print(f"updated baseline values for target `{target}` in {policy_path}")

    if violations:
        print("REST runtime perf/soak gate failed.")
        for violation in violations:
            print(f"- {violation}")
        print("Repro commands:")
        for item in results:
            if item.get("status") != "pass":
                print(f"- {item['id']}: {command_str(item['command'])}")
        print(f"report: {output_path}")
        return 1

    print("REST runtime perf/soak gate passed.")
    print(f"report: {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
