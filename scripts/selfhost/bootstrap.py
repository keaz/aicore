#!/usr/bin/env python3
"""Bounded AICore self-host bootstrap readiness gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import shlex
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

try:
    import resource
except ImportError:  # pragma: no cover - resource is unavailable on non-POSIX hosts.
    resource = None


DEFAULT_MAX_ARTIFACT_BYTES = 512 * 1024 * 1024
DEFAULT_MAX_PEAK_RSS_BYTES = 16 * 1024 * 1024 * 1024


@dataclass(frozen=True)
class StepResult:
    name: str
    command: list[str]
    exit_code: int
    duration_ms: int
    stdout: str
    stderr: str
    timed_out: bool
    artifact: str | None
    artifact_exists: bool | None
    artifact_sha256: str | None
    artifact_size_bytes: int | None
    child_peak_rss_bytes: int | None

    def to_json(self) -> dict[str, object]:
        return {
            "name": self.name,
            "command": " ".join(shlex.quote(part) for part in self.command),
            "exit_code": self.exit_code,
            "duration_ms": self.duration_ms,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "timed_out": self.timed_out,
            "artifact": self.artifact,
            "artifact_exists": self.artifact_exists,
            "artifact_sha256": self.artifact_sha256,
            "artifact_size_bytes": self.artifact_size_bytes,
            "child_peak_rss_bytes": self.child_peak_rss_bytes,
        }


@dataclass(frozen=True)
class ResourceBudgets:
    max_step_ms: int | None
    max_total_ms: int | None
    max_artifact_bytes: int | None
    max_peak_rss_bytes: int | None


def positive_int(value: str) -> int:
    parsed = int(value, 10)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("value must be a positive integer")
    return parsed


def optional_budget_arg(value: str) -> int | None:
    if value.lower() in ("0", "none", "off", "disabled"):
        return None
    return positive_int(value)


def env_budget(name: str, default: int | None) -> int | None:
    raw = os.environ.get(name)
    if raw is None or raw.strip() == "":
        return default
    return optional_budget_arg(raw.strip())


def bootstrap_budgets(args: argparse.Namespace) -> ResourceBudgets:
    return ResourceBudgets(
        max_step_ms=args.max_step_ms,
        max_total_ms=args.max_total_ms,
        max_artifact_bytes=args.max_artifact_bytes,
        max_peak_rss_bytes=args.max_peak_rss_bytes,
    )


def sha256_file(path: Path) -> str | None:
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def file_size_bytes(path: Path | None) -> int | None:
    if path is None or not path.is_file():
        return None
    return path.stat().st_size


def child_peak_rss_bytes() -> int | None:
    if resource is None:
        return None
    raw = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    if raw <= 0:
        return None
    if sys.platform == "darwin":
        return int(raw)
    return int(raw) * 1024


def stripped_sha256_file(path: Path) -> str | None:
    if not path.is_file():
        return None
    command = strip_command_for_platform(sys.platform)
    with tempfile.TemporaryDirectory(prefix="aicore-selfhost-strip-") as raw_tmp:
        tmp = Path(raw_tmp) / path.name
        shutil.copy2(path, tmp)
        completed = subprocess.run(
            command + [str(tmp)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if completed.returncode != 0:
            return None
        return sha256_file(tmp)


def strip_command_for_platform(platform_name: str) -> list[str]:
    if platform_name == "darwin":
        return ["strip", "-S", "-x"]
    return ["strip", "--strip-all"]


def run_step(
    name: str,
    command: list[str],
    cwd: Path,
    timeout_s: int,
    artifact: Path | None = None,
) -> StepResult:
    started = time.monotonic()
    timed_out = False
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_s,
            check=False,
        )
        exit_code = completed.returncode
        stdout = completed.stdout
        stderr = completed.stderr
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        exit_code = 124
        stdout = exc.stdout if isinstance(exc.stdout, str) else ""
        stderr = exc.stderr if isinstance(exc.stderr, str) else ""
        stderr = f"{stderr}\nbootstrap step timed out after {timeout_s}s".strip()
    duration_ms = int((time.monotonic() - started) * 1000)
    artifact_exists = artifact.is_file() if artifact is not None else None
    artifact_size = file_size_bytes(artifact)
    return StepResult(
        name=name,
        command=command,
        exit_code=exit_code,
        duration_ms=duration_ms,
        stdout=stdout,
        stderr=stderr,
        timed_out=timed_out,
        artifact=str(artifact) if artifact is not None else None,
        artifact_exists=artifact_exists,
        artifact_sha256=sha256_file(artifact) if artifact is not None else None,
        artifact_size_bytes=artifact_size,
        child_peak_rss_bytes=child_peak_rss_bytes(),
    )


def macos_host_preflight(cwd: Path) -> StepResult | None:
    if sys.platform != "darwin":
        return None
    command = ["DevToolsSecurity", "-status"]
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=5,
            check=False,
        )
        stdout = completed.stdout
        stderr = completed.stderr
        exit_code = completed.returncode
    except FileNotFoundError:
        stdout = ""
        stderr = "DevToolsSecurity was not found; skipping macOS developer-mode preflight"
        exit_code = 0
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout if isinstance(exc.stdout, str) else ""
        stderr = exc.stderr if isinstance(exc.stderr, str) else ""
        stderr = f"{stderr}\nmacOS developer-mode preflight timed out".strip()
        exit_code = 124
    combined = f"{stdout}\n{stderr}".lower()
    if exit_code == 0 and "developer mode is currently disabled" in combined:
        stderr = (
            f"{stderr}\n"
            "macOS Developer Mode is disabled; continuing because the self-host materializer ad-hoc signs "
            "Mach-O outputs. If an externally produced unsigned artifact hangs in _dyld_start, enable Terminal "
            "as a developer tool with `spctl developer-mode enable-terminal`, approve it in System Settings > "
            "Privacy & Security > Developer Tools, restart the terminal/Codex session, and rerun the gate."
        ).strip()
    duration_ms = int((time.monotonic() - started) * 1000)
    return StepResult(
        name="host-preflight",
        command=command,
        exit_code=exit_code,
        duration_ms=duration_ms,
        stdout=stdout,
        stderr=stderr,
        timed_out=exit_code == 124,
        artifact=None,
        artifact_exists=None,
        artifact_sha256=None,
        artifact_size_bytes=None,
        child_peak_rss_bytes=child_peak_rss_bytes(),
    )


def parity_command(candidate: Path, artifact_dir: Path, report: Path) -> list[str]:
    env = (
        f"SELFHOST_PARITY_MANIFEST=tests/selfhost/rust_vs_selfhost_manifest.json "
        f"SELFHOST_CANDIDATE={shlex.quote(str(candidate))} "
        f"SELFHOST_ARTIFACT_DIR={shlex.quote(str(artifact_dir))} "
        f"SELFHOST_PARITY_REPORT={shlex.quote(str(report))}"
    )
    return ["sh", "-c", f"{env} make selfhost-parity"]


def stage_matrix_command(stage_compiler: Path, artifact_dir: Path, report: Path) -> list[str]:
    env = (
        f"SELFHOST_STAGE_COMPILER={shlex.quote(str(stage_compiler))} "
        f"SELFHOST_STAGE_MATRIX_MANIFEST=tests/selfhost/stage_matrix_manifest.json "
        f"SELFHOST_STAGE_MATRIX_ARTIFACT_DIR={shlex.quote(str(artifact_dir))} "
        f"SELFHOST_STAGE_MATRIX_REPORT={shlex.quote(str(report))}"
    )
    return ["sh", "-c", f"{env} make selfhost-stage-matrix"]


def stage0_command(stage0: Path) -> list[str]:
    raw = os.environ.get("AIC_SELFHOST_STAGE0") or os.environ.get("AIC")
    if raw is None or raw.strip() == "":
        command = ["cargo", "run", "--quiet", "--bin", "aic", "--"]
    else:
        command = shlex.split(raw)
    return command + ["build", "compiler/aic/tools/aic_selfhost", "-o", str(stage0)]


def reproducibility(stage1: Path, stage2: Path) -> dict[str, object]:
    strip_command = strip_command_for_platform(sys.platform)
    strip_command_text = " ".join(strip_command)
    stage1_digest = sha256_file(stage1)
    stage2_digest = sha256_file(stage2)
    exact_matches = stage1_digest is not None and stage1_digest == stage2_digest
    stage1_stripped_digest = stripped_sha256_file(stage1)
    stage2_stripped_digest = stripped_sha256_file(stage2)
    stripped_matches = (
        stage1_stripped_digest is not None and stage1_stripped_digest == stage2_stripped_digest
    )
    allowed_differences: list[str] = []
    if not exact_matches and stripped_matches:
        allowed_differences.append(
            f"non-loadable symbol/debug table differences; {strip_command_text} artifacts match"
        )
    return {
        "stage1": str(stage1),
        "stage2": str(stage2),
        "stage1_sha256": stage1_digest,
        "stage2_sha256": stage2_digest,
        "exact_matches": exact_matches,
        "strip_command": strip_command_text,
        "stage1_stripped_sha256": stage1_stripped_digest,
        "stage2_stripped_sha256": stage2_stripped_digest,
        "stripped_matches": stripped_matches,
        "matches": exact_matches or stripped_matches,
        "allowed_differences": allowed_differences,
    }


def resource_budget_report(
    steps: list[StepResult],
    budgets: ResourceBudgets,
) -> dict[str, object]:
    total_duration_ms = sum(step.duration_ms for step in steps)
    max_step_duration_ms = max((step.duration_ms for step in steps), default=0)
    max_artifact_size_bytes = max(
        (step.artifact_size_bytes or 0 for step in steps),
        default=0,
    )
    max_peak_rss_bytes = max(
        (step.child_peak_rss_bytes or 0 for step in steps),
        default=0,
    )
    violations: list[str] = []
    if budgets.max_step_ms is not None and max_step_duration_ms > budgets.max_step_ms:
        violations.append(
            f"max step duration {max_step_duration_ms}ms exceeded budget {budgets.max_step_ms}ms"
        )
    if budgets.max_total_ms is not None and total_duration_ms > budgets.max_total_ms:
        violations.append(
            f"total duration {total_duration_ms}ms exceeded budget {budgets.max_total_ms}ms"
        )
    if (
        budgets.max_artifact_bytes is not None
        and max_artifact_size_bytes > budgets.max_artifact_bytes
    ):
        violations.append(
            "max artifact size "
            f"{max_artifact_size_bytes} bytes exceeded budget {budgets.max_artifact_bytes} bytes"
        )
    if budgets.max_peak_rss_bytes is not None and max_peak_rss_bytes > budgets.max_peak_rss_bytes:
        violations.append(
            "child peak RSS "
            f"{max_peak_rss_bytes} bytes exceeded budget {budgets.max_peak_rss_bytes} bytes"
        )
    return {
        "ok": len(violations) == 0,
        "violations": violations,
        "budgets": {
            "max_step_ms": budgets.max_step_ms,
            "max_total_ms": budgets.max_total_ms,
            "max_artifact_bytes": budgets.max_artifact_bytes,
            "max_peak_rss_bytes": budgets.max_peak_rss_bytes,
        },
        "observed": {
            "total_duration_ms": total_duration_ms,
            "max_step_duration_ms": max_step_duration_ms,
            "max_artifact_size_bytes": max_artifact_size_bytes,
            "max_child_peak_rss_bytes": max_peak_rss_bytes,
        },
    }


def readiness_status(
    mode: str,
    steps: list[StepResult],
    repro: dict[str, object],
    performance: dict[str, object],
) -> tuple[str, list[str]]:
    reasons: list[str] = []
    by_name = {step.name: step for step in steps}
    host_preflight = by_name.get("host-preflight")
    if host_preflight is not None and host_preflight.exit_code != 0:
        if host_preflight.timed_out:
            reasons.append("host preflight timed out")
        else:
            reasons.append(f"host preflight failed with exit code {host_preflight.exit_code}")
    for required in ("stage0", "stage1", "stage2", "parity", "stage-matrix"):
        step = by_name.get(required)
        if step is None:
            reasons.append(f"{required} did not run")
        elif step.exit_code != 0:
            reason = f"{required} failed with exit code {step.exit_code}"
            if step.timed_out:
                reason = f"{required} timed out"
            reasons.append(reason)
        elif step.artifact_exists is False:
            reasons.append(f"{required} artifact was not produced")
    if repro.get("matches") is not True:
        reasons.append("stage1/stage2 artifacts are not reproducible")
    for violation in performance.get("violations", []):
        reasons.append(f"resource budget violation: {violation}")
    if reasons:
        return ("experimental", reasons)
    if mode == "default":
        return ("default-ready", [])
    return ("supported-ready", [])


def run_bootstrap(args: argparse.Namespace) -> int:
    root = Path(args.repo_root).resolve()
    out_dir = Path(args.out_dir)
    if not out_dir.is_absolute():
        out_dir = root / out_dir
    out_dir.mkdir(parents=True, exist_ok=True)

    stage0 = out_dir / "stage0" / "aic_selfhost"
    stage1 = out_dir / "stage1" / "aic_selfhost"
    stage2 = out_dir / "stage2" / "aic_selfhost"
    for parent in (stage0.parent, stage1.parent, stage2.parent):
        parent.mkdir(parents=True, exist_ok=True)

    steps: list[StepResult] = []
    preflight = macos_host_preflight(root)
    if preflight is not None:
        steps.append(preflight)

    host_ready = preflight is None or preflight.exit_code == 0
    if host_ready:
        steps.append(
            run_step(
                "stage0",
                stage0_command(stage0),
                root,
                args.timeout,
                stage0,
            )
        )
    if steps[-1].exit_code == 0 and stage0.is_file():
        steps.append(
            run_step(
                "stage1",
                [str(stage0), "build", "compiler/aic/tools/aic_selfhost", "-o", str(stage1)],
                root,
                args.timeout,
                stage1,
            )
        )
    if len(steps) >= 2 and steps[-1].exit_code == 0 and stage1.is_file():
        steps.append(
            run_step(
                "stage2",
                [str(stage1), "build", "compiler/aic/tools/aic_selfhost", "-o", str(stage2)],
                root,
                args.timeout,
                stage2,
            )
        )

    parity_candidate = stage2 if stage2.is_file() else stage1 if stage1.is_file() else stage0
    parity_report = out_dir / "parity-report.json"
    if host_ready and parity_candidate.is_file():
        steps.append(
            run_step(
                "parity",
                parity_command(parity_candidate, out_dir / "parity-artifacts", parity_report),
                root,
                args.timeout,
                parity_report,
            )
        )

    stage_matrix_report = out_dir / "stage-matrix-report.json"
    if host_ready and parity_candidate.is_file():
        steps.append(
            run_step(
                "stage-matrix",
                stage_matrix_command(
                    parity_candidate,
                    out_dir / "stage-matrix-artifacts",
                    stage_matrix_report,
                ),
                root,
                args.timeout,
                stage_matrix_report,
            )
        )

    repro = reproducibility(stage1, stage2)
    performance = resource_budget_report(steps, bootstrap_budgets(args))
    status, reasons = readiness_status(args.mode, steps, repro, performance)
    report = {
        "format": "aicore-selfhost-bootstrap-v1",
        "mode": args.mode,
        "status": status,
        "ready": not reasons,
        "reasons": reasons,
        "stage0": str(stage0),
        "stage1": str(stage1),
        "stage2": str(stage2),
        "parity_report": str(parity_report),
        "stage_matrix_report": str(stage_matrix_report),
        "reproducibility": repro,
        "performance": performance,
        "steps": [step.to_json() for step in steps],
        "policy": {
            "experimental": "stage0/parity/stage-matrix may run, but stage1/stage2 failures, reproducibility failures, stage matrix regressions, or resource budget violations keep self-hosting unsupported",
            "supported": "stage0, stage1, stage2, parity, stage matrix, reproducibility, and resource budgets must pass",
            "default": "same as supported, plus explicit release approval outside this script",
        },
    }
    report_path = Path(args.report)
    if not report_path.is_absolute():
        report_path = root / report_path
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"selfhost-bootstrap: status={status} report={report_path}")
    for reason in reasons:
        print(f"selfhost-bootstrap: {reason}", file=sys.stderr)
    if reasons and not args.allow_incomplete:
        return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", default=os.getcwd())
    parser.add_argument("--out-dir", default="target/selfhost-bootstrap")
    parser.add_argument("--report", default="target/selfhost-bootstrap/report.json")
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument(
        "--max-step-ms",
        type=optional_budget_arg,
        default=env_budget("AIC_SELFHOST_MAX_STEP_MS", 900_000),
        help="maximum duration for any bootstrap step; use 0/off to disable",
    )
    parser.add_argument(
        "--max-total-ms",
        type=optional_budget_arg,
        default=env_budget("AIC_SELFHOST_MAX_TOTAL_MS", 3_600_000),
        help="maximum total duration for the bootstrap gate; use 0/off to disable",
    )
    parser.add_argument(
        "--max-artifact-bytes",
        type=optional_budget_arg,
        default=env_budget("AIC_SELFHOST_MAX_ARTIFACT_BYTES", DEFAULT_MAX_ARTIFACT_BYTES),
        help="maximum size for any produced bootstrap artifact; use 0/off to disable",
    )
    parser.add_argument(
        "--max-peak-rss-bytes",
        type=optional_budget_arg,
        default=env_budget("AIC_SELFHOST_MAX_PEAK_RSS_BYTES", DEFAULT_MAX_PEAK_RSS_BYTES),
        help="maximum cumulative child peak RSS observed by the bootstrap process; use 0/off to disable",
    )
    parser.add_argument("--mode", choices=("experimental", "supported", "default"), default="supported")
    parser.add_argument(
        "--allow-incomplete",
        action="store_true",
        help="write a report and exit 0 even when readiness is experimental",
    )
    return run_bootstrap(parser.parse_args())


if __name__ == "__main__":
    raise SystemExit(main())
