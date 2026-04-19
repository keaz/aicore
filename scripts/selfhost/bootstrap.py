#!/usr/bin/env python3
"""Bounded AICore self-host bootstrap readiness gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
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


BOOTSTRAP_BUDGET_FORMAT = "aicore-selfhost-bootstrap-budgets-v1"
BOOTSTRAP_PERFORMANCE_FORMAT = "aicore-selfhost-bootstrap-performance-v1"
BOOTSTRAP_PERFORMANCE_TREND_FORMAT = "aicore-selfhost-bootstrap-performance-trend-v1"
REQUIRED_BOOTSTRAP_STEPS = (
    "host-preflight",
    "stage0",
    "stage1",
    "stage2",
    "parity",
    "stage-matrix",
)
REQUIRED_BASELINE_FIELDS = (
    "total_duration_ms",
    "max_step_duration_ms",
    "max_artifact_size_bytes",
    "max_child_peak_rss_bytes",
    "reproducibility_duration_ms",
)


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
    per_step_max_ms: dict[str, int] | None = None
    max_reproducibility_ms: int | None = None
    source: str | None = None
    schema_version: int | None = None
    platform: str | None = None
    baseline: dict[str, object] | None = None
    overrides: dict[str, str] | None = None


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
    try:
        return optional_budget_arg(raw.strip())
    except (argparse.ArgumentTypeError, ValueError) as exc:
        raise ValueError(
            f"{name} must be a positive integer, 0, off, none, or disabled"
        ) from exc


def platform_budget_key(platform_name: str) -> str:
    if platform_name == "darwin":
        return "macos"
    if platform_name.startswith("linux"):
        return "linux"
    return platform_name


def require_int_field(data: dict[str, object], field: str, context: str) -> int:
    value = data.get(field)
    if not isinstance(value, int) or value <= 0:
        raise ValueError(f"{context}.{field} must be a positive integer")
    return value


def load_budget_manifest(path: Path, platform_name: str) -> ResourceBudgets:
    raw = json.loads(path.read_text())
    if raw.get("format") != BOOTSTRAP_BUDGET_FORMAT:
        raise ValueError(f"budget manifest format must be {BOOTSTRAP_BUDGET_FORMAT}")
    schema_version = raw.get("schema_version")
    if schema_version != 1:
        raise ValueError("budget manifest schema_version must be 1")
    platforms = raw.get("platforms")
    if not isinstance(platforms, dict):
        raise ValueError("budget manifest platforms must be an object")
    key = platform_budget_key(platform_name)
    platform_entry = platforms.get(key)
    if not isinstance(platform_entry, dict):
        raise ValueError(f"budget manifest has no supported platform entry for {key}")
    budgets = platform_entry.get("budgets")
    if not isinstance(budgets, dict):
        raise ValueError(f"budget manifest platform {key} is missing budgets")
    per_step_raw = budgets.get("per_step_ms")
    if not isinstance(per_step_raw, dict):
        raise ValueError(f"budget manifest platform {key} is missing per_step_ms")
    per_step: dict[str, int] = {}
    for step in REQUIRED_BOOTSTRAP_STEPS:
        per_step[step] = require_int_field(
            per_step_raw,
            step,
            f"platforms.{key}.budgets.per_step_ms",
        )
    baseline = platform_entry.get("baseline")
    if not isinstance(baseline, dict):
        raise ValueError(f"budget manifest platform {key} is missing baseline")
    for field in REQUIRED_BASELINE_FIELDS:
        require_int_field(baseline, field, f"platforms.{key}.baseline")
    baseline_steps = baseline.get("steps")
    if not isinstance(baseline_steps, dict):
        raise ValueError(f"budget manifest platform {key} baseline is missing steps")
    for step in REQUIRED_BOOTSTRAP_STEPS:
        step_baseline = baseline_steps.get(step)
        if not isinstance(step_baseline, dict):
            raise ValueError(f"budget manifest platform {key} baseline is missing {step}")
        require_int_field(step_baseline, "duration_ms", f"platforms.{key}.baseline.steps.{step}")
    return ResourceBudgets(
        max_step_ms=require_int_field(budgets, "max_step_ms", f"platforms.{key}.budgets"),
        max_total_ms=require_int_field(budgets, "max_total_ms", f"platforms.{key}.budgets"),
        max_artifact_bytes=require_int_field(
            budgets,
            "max_artifact_bytes",
            f"platforms.{key}.budgets",
        ),
        max_peak_rss_bytes=require_int_field(
            budgets,
            "max_peak_rss_bytes",
            f"platforms.{key}.budgets",
        ),
        per_step_max_ms=per_step,
        max_reproducibility_ms=require_int_field(
            budgets,
            "max_reproducibility_ms",
            f"platforms.{key}.budgets",
        ),
        source=str(path),
        schema_version=schema_version,
        platform=key,
        baseline=baseline,
        overrides={},
    )


def override_budget(
    default: int | None,
    env_name: str,
    arg_value: int | None,
) -> tuple[int | None, str | None]:
    if arg_value is not None:
        return (arg_value, "cli")
    if env_name in os.environ and os.environ.get(env_name, "").strip() != "":
        return (env_budget(env_name, default), env_name)
    return (default, None)


def bootstrap_budgets(args: argparse.Namespace, root: Path) -> ResourceBudgets:
    manifest_path = Path(args.budget_manifest)
    if not manifest_path.is_absolute():
        manifest_path = root / manifest_path
    manifest = load_budget_manifest(manifest_path, sys.platform)
    max_step_ms, max_step_source = override_budget(
        manifest.max_step_ms,
        "AIC_SELFHOST_MAX_STEP_MS",
        args.max_step_ms,
    )
    max_total_ms, max_total_source = override_budget(
        manifest.max_total_ms,
        "AIC_SELFHOST_MAX_TOTAL_MS",
        args.max_total_ms,
    )
    max_artifact_bytes, max_artifact_source = override_budget(
        manifest.max_artifact_bytes,
        "AIC_SELFHOST_MAX_ARTIFACT_BYTES",
        args.max_artifact_bytes,
    )
    max_peak_rss_bytes, max_peak_rss_source = override_budget(
        manifest.max_peak_rss_bytes,
        "AIC_SELFHOST_MAX_PEAK_RSS_BYTES",
        args.max_peak_rss_bytes,
    )
    max_reproducibility_ms, max_reproducibility_source = override_budget(
        manifest.max_reproducibility_ms,
        "AIC_SELFHOST_MAX_REPRODUCIBILITY_MS",
        args.max_reproducibility_ms,
    )
    overrides = {
        key: source
        for key, source in {
            "max_step_ms": max_step_source,
            "max_total_ms": max_total_source,
            "max_artifact_bytes": max_artifact_source,
            "max_peak_rss_bytes": max_peak_rss_source,
            "max_reproducibility_ms": max_reproducibility_source,
        }.items()
        if source is not None
    }
    return ResourceBudgets(
        max_step_ms=max_step_ms,
        max_total_ms=max_total_ms,
        max_artifact_bytes=max_artifact_bytes,
        max_peak_rss_bytes=max_peak_rss_bytes,
        per_step_max_ms=manifest.per_step_max_ms,
        max_reproducibility_ms=max_reproducibility_ms,
        source=manifest.source,
        schema_version=manifest.schema_version,
        platform=manifest.platform,
        baseline=manifest.baseline,
        overrides=overrides,
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


def host_report() -> dict[str, object]:
    return {
        "platform": sys.platform,
        "system": platform.system(),
        "machine": platform.machine(),
        "python_version": platform.python_version(),
    }


def host_preflight_command() -> list[str]:
    script = "\n".join(
        [
            "set -eu",
            "command -v cargo",
            "cargo --version",
            "command -v clang",
            "clang --version",
            "command -v strip",
            'if [ "$(uname -s)" = "Darwin" ]; then',
            "  command -v codesign",
            "  codesign -h >/dev/null 2>&1 || true",
            '  echo "codesign: available"',
            "  if command -v DevToolsSecurity >/dev/null 2>&1; then",
            "    DevToolsSecurity -status || true",
            "  else",
            '    echo "DevToolsSecurity was not found; skipping macOS developer-mode preflight" >&2',
            "  fi",
            "fi",
        ]
    )
    return ["sh", "-c", script]


def host_preflight(cwd: Path) -> StepResult:
    command = host_preflight_command()
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=15,
            check=False,
        )
        stdout = completed.stdout
        stderr = completed.stderr
        exit_code = completed.returncode
    except FileNotFoundError:
        stdout = ""
        stderr = "host preflight shell was not found"
        exit_code = 127
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
    reproducibility_duration_ms: int | None = None,
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
    step_observations = {
        step.name: {
            "duration_ms": step.duration_ms,
            "artifact_size_bytes": step.artifact_size_bytes,
            "artifact_exists": step.artifact_exists,
            "child_peak_rss_bytes": step.child_peak_rss_bytes,
            "exit_code": step.exit_code,
            "timed_out": step.timed_out,
        }
        for step in steps
    }
    per_step_max_ms = budgets.per_step_max_ms or {}
    by_name = {step.name: step for step in steps}
    for step_name, budget_ms in per_step_max_ms.items():
        step = by_name.get(step_name)
        if step is None:
            violations.append(f"{step_name} missing required duration metric")
            continue
        if step.duration_ms > budget_ms:
            violations.append(
                f"{step_name} duration {step.duration_ms}ms exceeded budget {budget_ms}ms"
            )
        if budgets.max_peak_rss_bytes is not None and step.child_peak_rss_bytes is None:
            violations.append(f"{step_name} missing child peak RSS metric")
        if step.artifact is not None and step.artifact_size_bytes is None:
            violations.append(f"{step_name} missing artifact size metric")
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
    if budgets.max_reproducibility_ms is not None:
        if reproducibility_duration_ms is None:
            violations.append("reproducibility comparison missing duration metric")
        elif reproducibility_duration_ms > budgets.max_reproducibility_ms:
            violations.append(
                "reproducibility comparison duration "
                f"{reproducibility_duration_ms}ms exceeded budget "
                f"{budgets.max_reproducibility_ms}ms"
            )
    return {
        "ok": len(violations) == 0,
        "violations": violations,
        "budget_source": {
            "path": budgets.source,
            "schema_version": budgets.schema_version,
            "platform": budgets.platform,
            "overrides": budgets.overrides or {},
        },
        "baseline": budgets.baseline or {},
        "budgets": {
            "max_step_ms": budgets.max_step_ms,
            "max_total_ms": budgets.max_total_ms,
            "max_artifact_bytes": budgets.max_artifact_bytes,
            "max_peak_rss_bytes": budgets.max_peak_rss_bytes,
            "per_step_ms": budgets.per_step_max_ms or {},
            "max_reproducibility_ms": budgets.max_reproducibility_ms,
        },
        "observed": {
            "total_duration_ms": total_duration_ms,
            "max_step_duration_ms": max_step_duration_ms,
            "max_artifact_size_bytes": max_artifact_size_bytes,
            "max_child_peak_rss_bytes": max_peak_rss_bytes,
            "reproducibility_duration_ms": reproducibility_duration_ms,
            "steps": step_observations,
        },
    }


def performance_report_document(
    host: dict[str, object],
    status: str,
    ready: bool,
    performance: dict[str, object],
    repro: dict[str, object],
    steps: list[StepResult],
) -> dict[str, object]:
    return {
        "format": BOOTSTRAP_PERFORMANCE_FORMAT,
        "host": host,
        "status": status,
        "ready": ready,
        "budget_source": performance.get("budget_source", {}),
        "performance": performance,
        "reproducibility": {
            "matches": repro.get("matches"),
            "exact_matches": repro.get("exact_matches"),
            "stripped_matches": repro.get("stripped_matches"),
            "duration_ms": repro.get("duration_ms"),
        },
        "steps": [step.to_json() for step in steps],
    }


def performance_trend_document(
    host: dict[str, object],
    status: str,
    performance: dict[str, object],
) -> dict[str, object]:
    observed = performance.get("observed", {})
    if not isinstance(observed, dict):
        observed = {}
    return {
        "format": BOOTSTRAP_PERFORMANCE_TREND_FORMAT,
        "host": host,
        "status": status,
        "ok": performance.get("ok"),
        "budget_source": performance.get("budget_source", {}),
        "budgets": performance.get("budgets", {}),
        "baseline": performance.get("baseline", {}),
        "metrics": {
            "total_duration_ms": observed.get("total_duration_ms"),
            "max_step_duration_ms": observed.get("max_step_duration_ms"),
            "max_artifact_size_bytes": observed.get("max_artifact_size_bytes"),
            "max_child_peak_rss_bytes": observed.get("max_child_peak_rss_bytes"),
            "reproducibility_duration_ms": observed.get("reproducibility_duration_ms"),
        },
        "steps": observed.get("steps", {}),
        "violations": performance.get("violations", []),
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
    try:
        budgets = bootstrap_budgets(args, root)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        print(f"selfhost-bootstrap: invalid performance budget configuration: {exc}", file=sys.stderr)
        return 2

    stage0 = out_dir / "stage0" / "aic_selfhost"
    stage1 = out_dir / "stage1" / "aic_selfhost"
    stage2 = out_dir / "stage2" / "aic_selfhost"
    for parent in (stage0.parent, stage1.parent, stage2.parent):
        parent.mkdir(parents=True, exist_ok=True)

    steps: list[StepResult] = []
    preflight = host_preflight(root)
    steps.append(preflight)

    host_ready = preflight.exit_code == 0
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

    repro_started = time.monotonic()
    repro = reproducibility(stage1, stage2)
    repro["duration_ms"] = int((time.monotonic() - repro_started) * 1000)
    performance = resource_budget_report(steps, budgets, repro["duration_ms"])
    status, reasons = readiness_status(args.mode, steps, repro, performance)
    host = host_report()
    ready = not reasons
    performance_report_path = Path(args.performance_report)
    if not performance_report_path.is_absolute():
        performance_report_path = root / performance_report_path
    performance_trend_path = Path(args.performance_trend)
    if not performance_trend_path.is_absolute():
        performance_trend_path = root / performance_trend_path
    report = {
        "format": "aicore-selfhost-bootstrap-v1",
        "mode": args.mode,
        "status": status,
        "ready": ready,
        "reasons": reasons,
        "host": host,
        "stage0": str(stage0),
        "stage1": str(stage1),
        "stage2": str(stage2),
        "parity_report": str(parity_report),
        "stage_matrix_report": str(stage_matrix_report),
        "performance_report": str(performance_report_path),
        "performance_trend": str(performance_trend_path),
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
    performance_report_path.parent.mkdir(parents=True, exist_ok=True)
    performance_report_path.write_text(
        json.dumps(
            performance_report_document(host, status, ready, performance, repro, steps),
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    performance_trend_path.parent.mkdir(parents=True, exist_ok=True)
    performance_trend_path.write_text(
        json.dumps(performance_trend_document(host, status, performance), indent=2, sort_keys=True)
        + "\n"
    )
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
    parser.add_argument(
        "--performance-report",
        default="target/selfhost-bootstrap/performance-report.json",
    )
    parser.add_argument(
        "--performance-trend",
        default="target/selfhost-bootstrap/performance-trend.json",
    )
    parser.add_argument(
        "--budget-manifest",
        default="docs/selfhost/bootstrap-budgets.v1.json",
    )
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument(
        "--max-step-ms",
        type=optional_budget_arg,
        default=None,
        help="maximum duration for any bootstrap step; use 0/off to disable",
    )
    parser.add_argument(
        "--max-total-ms",
        type=optional_budget_arg,
        default=None,
        help="maximum total duration for the bootstrap gate; use 0/off to disable",
    )
    parser.add_argument(
        "--max-artifact-bytes",
        type=optional_budget_arg,
        default=None,
        help="maximum size for any produced bootstrap artifact; use 0/off to disable",
    )
    parser.add_argument(
        "--max-peak-rss-bytes",
        type=optional_budget_arg,
        default=None,
        help="maximum cumulative child peak RSS observed by the bootstrap process; use 0/off to disable",
    )
    parser.add_argument(
        "--max-reproducibility-ms",
        type=optional_budget_arg,
        default=None,
        help="maximum duration for stage1/stage2 reproducibility comparison; use 0/off to disable",
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
