#!/usr/bin/env python3
"""Deterministic REST runtime perf/soak gate for parse/router/json/async churn paths.

Each scenario runs one test from a prebuilt cargo test binary. The gate builds
the binaries once and then times the binary itself, so cargo's startup and
freshness checks are not part of a sample.

Shared CI runners change speed between runs. To keep the gate stable without
loosening it, the gate:

- runs the scenarios in interleaved rounds, so a burst of runner load hits
  every scenario instead of all samples of one;
- takes the median of `measure_runs` rounds;
- times a fixed CPU calibration workload in every round. When the host is
  slower than the calibration baseline, the regression limit grows by the same
  factor, capped at `calibration.max_speed_factor`. A faster host never makes
  the limit stricter, and `max_ms` is an absolute cap that never scales;
- applies `regression_floor_ms`, so a scenario of a few milliseconds is not
  failed by scheduler jitter.
"""

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


def calibration_workload(iterations: int) -> float:
    """Time a fixed integer workload that does not depend on repository code."""
    start = time.perf_counter()
    acc = 0
    for value in range(iterations):
        acc = (acc * 31 + value) % 1_000_003
    elapsed_ms = (time.perf_counter() - start) * 1000.0
    if acc < 0:  # keeps the loop from being treated as dead code
        raise AssertionError("unreachable")
    return elapsed_ms


def build_test_binaries(
    targets: list[str], cwd: Path, env: dict[str, str]
) -> dict[str, str]:
    """Build the cargo test targets and return the path of each test binary."""
    command = ["cargo", "test", "--locked", "--no-run", "--message-format=json"]
    for target in targets:
        command += ["--test", target]
    completed = subprocess.run(
        command, cwd=cwd, env=env, text=True, capture_output=True, check=False
    )
    if completed.returncode != 0:
        print(f"[FAIL] build rc={completed.returncode} cmd={command_str(command)}")
        print(f"stderr: {truncate(completed.stderr)}")
        raise SystemExit(1)
    binaries: dict[str, str] = {}
    for line in completed.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") != "compiler-artifact":
            continue
        name = (message.get("target") or {}).get("name")
        executable = message.get("executable")
        if name in targets and executable and (message.get("profile") or {}).get("test"):
            binaries[name] = executable
    missing = [target for target in targets if target not in binaries]
    if missing:
        raise SystemExit(f"cargo did not report a test binary for: {', '.join(missing)}")
    return binaries


def scenario_test(scenario: dict[str, Any]) -> tuple[str, list[str]]:
    scenario_id = scenario.get("id", "<missing>")
    target = scenario.get("test")
    args = scenario.get("args", [])
    if not isinstance(target, str) or not target:
        raise SystemExit(f"scenario `{scenario_id}` must name its cargo test target in `test`")
    if not isinstance(args, list) or not all(isinstance(arg, str) for arg in args):
        raise SystemExit(f"scenario `{scenario_id}` must define `args` as a list of strings")
    return target, args


def repro_command(target: str, args: list[str]) -> list[str]:
    return ["cargo", "test", "--locked", "--test", target, "--", *args]


def target_thresholds(item: dict[str, Any], target: str, what: str) -> dict[str, Any]:
    thresholds = (item.get("thresholds") or {}).get(target)
    if thresholds is None:
        raise SystemExit(f"{what} missing thresholds for target `{target}`")
    return thresholds


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
    measure_runs = int(policy.get("measure_runs", 7))
    tolerance_pct = float(policy.get("regression_tolerance_pct", 45.0))
    floor_ms = float(policy.get("regression_floor_ms", 0.0))
    calibration = policy.get("calibration") or {}
    scenarios = policy.get("scenarios", [])
    common_env = {str(k): str(v) for k, v in (policy.get("env") or {}).items()}

    if "preflight" in policy or any("command" in item for item in scenarios):
        raise SystemExit(
            "policy uses the old `preflight`/`command` fields; name the test target "
            "in `test` and its arguments in `args`"
        )
    if not scenarios:
        raise SystemExit("policy must contain at least one scenario")
    if measure_runs < 1:
        raise SystemExit("measure_runs must be >= 1")
    if warmup_runs < 0:
        raise SystemExit("warmup_runs must be >= 0")
    if floor_ms < 0:
        raise SystemExit("regression_floor_ms must be >= 0")
    calibration_iterations = int(calibration.get("iterations", 0))
    max_speed_factor = float(calibration.get("max_speed_factor", 1.0))
    if calibration_iterations < 1:
        raise SystemExit("calibration.iterations must be >= 1")
    if max_speed_factor < 1.0:
        raise SystemExit("calibration.max_speed_factor must be >= 1.0")

    target = host_target_label()
    calibration_thresholds = target_thresholds(calibration, target, "calibration")
    calibration_baseline_ms = float(calibration_thresholds["baseline_ms"])
    run_env = os.environ.copy()
    run_env.update(common_env)

    ids = [str(item.get("id", "")).strip() for item in scenarios]
    if any(not scenario_id for scenario_id in ids):
        raise SystemExit("scenario id is required")
    if len(set(ids)) != len(ids):
        raise SystemExit("scenario ids must be unique")
    for scenario in scenarios:
        scenario_test(scenario)
        target_thresholds(scenario, target, f"scenario `{scenario['id']}`")

    if args.dry_run:
        print(f"rest runtime soak gate dry-run target={target}")
        print(f"policy={policy_path}")
        print(
            f"[calibration] iterations={calibration_iterations} "
            f"baseline={calibration_baseline_ms}ms max_speed_factor={max_speed_factor}"
        )
        for scenario in scenarios:
            target_name, test_args = scenario_test(scenario)
            thresholds = target_thresholds(scenario, target, f"scenario `{scenario['id']}`")
            print(
                f"[scenario] {scenario['id']} baseline={thresholds.get('baseline_ms')}ms "
                f"max={thresholds.get('max_ms')}ms "
                f"cmd={command_str(['<' + target_name + ' test binary>', *test_args])}"
            )
        return 0

    test_targets = sorted({scenario_test(scenario)[0] for scenario in scenarios})
    binaries = build_test_binaries(test_targets, ROOT, run_env)

    plans: list[dict[str, Any]] = []
    for scenario in scenarios:
        target_name, test_args = scenario_test(scenario)
        scenario_env = run_env.copy()
        scenario_env.update({str(k): str(v) for k, v in (scenario.get("env") or {}).items()})
        plans.append(
            {
                "scenario": scenario,
                "command": [binaries[target_name], *test_args],
                "repro": repro_command(target_name, test_args),
                "env": scenario_env,
                "samples": [],
                "failure": None,
            }
        )

    calibration_samples: list[float] = []
    for round_idx in range(warmup_runs + measure_runs):
        measured_round = round_idx >= warmup_runs
        label = (
            f"measure {round_idx - warmup_runs + 1}/{measure_runs}"
            if measured_round
            else f"warmup {round_idx + 1}/{warmup_runs}"
        )
        print(f"[ROUND] {label}")
        calibration_ms = calibration_workload(calibration_iterations)
        if measured_round:
            calibration_samples.append(calibration_ms)
        for plan in plans:
            if plan["failure"] is not None:
                continue
            result = run_command(plan["command"], ROOT, plan["env"])
            if result["returncode"] != 0:
                plan["failure"] = {"round": label, **result}
                print(
                    f"[FAIL] {plan['scenario']['id']}: {label} failed "
                    f"rc={result['returncode']}"
                )
                continue
            if measured_round:
                plan["samples"].append(float(result["elapsed_ms"]))

    calibration_observed_ms = float(statistics.median(calibration_samples))
    raw_speed_factor = calibration_observed_ms / calibration_baseline_ms
    speed_factor = min(max(raw_speed_factor, 1.0), max_speed_factor)
    print(
        f"[CALIBRATION] observed={calibration_observed_ms:.3f}ms "
        f"baseline={calibration_baseline_ms:.3f}ms host_speed_factor={raw_speed_factor:.3f} "
        f"applied={speed_factor:.3f} (cap {max_speed_factor:.2f})"
    )

    results: list[dict[str, Any]] = []
    violations: list[str] = []
    for plan in plans:
        scenario = plan["scenario"]
        scenario_id = str(scenario["id"])
        thresholds = target_thresholds(scenario, target, f"scenario `{scenario_id}`")
        baseline_ms = float(thresholds["baseline_ms"])
        max_ms = float(thresholds["max_ms"])
        base_limit_ms = max(baseline_ms * (1.0 + tolerance_pct / 100.0), baseline_ms + floor_ms)
        regression_limit_ms = base_limit_ms * speed_factor
        entry: dict[str, Any] = {
            "id": scenario_id,
            "description": str(scenario.get("description", "")).strip(),
            "command": plan["command"],
            "repro": plan["repro"],
            "samples_ms": [round(value, 3) for value in plan["samples"]],
            "baseline_ms": round(baseline_ms, 3),
            "max_ms": round(max_ms, 3),
            "regression_limit_ms": round(regression_limit_ms, 3),
        }

        if plan["failure"] is not None:
            message = (
                f"{scenario_id}: {plan['failure']['round']} failed "
                f"rc={plan['failure']['returncode']} cmd={command_str(plan['repro'])}"
            )
            violations.append(message)
            entry.update(
                {
                    "observed_ms": None,
                    "delta_ms": None,
                    "delta_pct": None,
                    "within_budget": False,
                    "within_regression_limit": False,
                    "status": "command_failed",
                    "command_failure": plan["failure"],
                }
            )
            results.append(entry)
            continue

        observed_ms = float(statistics.median(plan["samples"]))
        delta_ms = observed_ms - baseline_ms
        delta_pct = 0.0 if baseline_ms == 0 else (delta_ms / baseline_ms) * 100.0
        within_budget = observed_ms <= max_ms
        within_regression = observed_ms <= regression_limit_ms
        passed = within_budget and within_regression
        entry.update(
            {
                "observed_ms": round(observed_ms, 3),
                "delta_ms": round(delta_ms, 3),
                "delta_pct": round(delta_pct, 3),
                "within_budget": within_budget,
                "within_regression_limit": within_regression,
                "status": "pass" if passed else "threshold_failed",
            }
        )
        results.append(entry)

        summary = (
            f"observed={observed_ms:.3f}ms baseline={baseline_ms:.3f}ms "
            f"delta={delta_pct:+.2f}% max={max_ms:.3f}ms "
            f"reg_limit={regression_limit_ms:.3f}ms"
        )
        if passed:
            print(f"[PASS] {scenario_id}: {summary}")
        else:
            violations.append(f"{scenario_id}: {summary}")
            print(f"[FAIL] {scenario_id}: {summary}")

    report = {
        "schema_version": schema_version,
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "target": target,
        "policy_path": str(policy_path),
        "warmup_runs": warmup_runs,
        "measure_runs": measure_runs,
        "regression_tolerance_pct": tolerance_pct,
        "regression_floor_ms": floor_ms,
        "calibration": {
            "iterations": calibration_iterations,
            "samples_ms": [round(value, 3) for value in calibration_samples],
            "observed_ms": round(calibration_observed_ms, 3),
            "baseline_ms": round(calibration_baseline_ms, 3),
            "host_speed_factor": round(raw_speed_factor, 3),
            "applied_speed_factor": round(speed_factor, 3),
            "max_speed_factor": max_speed_factor,
        },
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
            observed = item.get("observed_ms")
            if item.get("status") == "command_failed" or observed is None:
                continue
            scenario_lookup[str(item["id"])]["thresholds"][target]["baseline_ms"] = round(
                float(observed), 3
            )
        calibration["thresholds"][target]["baseline_ms"] = round(calibration_observed_ms, 3)
        write_json(policy_path, policy)
        print(f"updated baseline values for target `{target}` in {policy_path}")

    if violations:
        print("REST runtime perf/soak gate failed.")
        for violation in violations:
            print(f"- {violation}")
        print("Repro commands:")
        for item in results:
            if item.get("status") != "pass":
                print(f"- {item['id']}: {command_str(item['repro'])}")
        print(f"report: {output_path}")
        return 1

    print("REST runtime perf/soak gate passed.")
    print(f"report: {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
