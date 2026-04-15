#!/usr/bin/env python3
"""Bounded AICore self-host bootstrap readiness gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path


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
        }


def sha256_file(path: Path) -> str | None:
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


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
    )


def parity_command(candidate: Path, artifact_dir: Path, report: Path) -> list[str]:
    env = (
        f"SELFHOST_PARITY_MANIFEST=tests/selfhost/rust_vs_selfhost_manifest.json "
        f"SELFHOST_CANDIDATE={shlex.quote(str(candidate))} "
        f"SELFHOST_ARTIFACT_DIR={shlex.quote(str(artifact_dir))} "
        f"SELFHOST_PARITY_REPORT={shlex.quote(str(report))}"
    )
    return ["sh", "-c", f"{env} make selfhost-parity"]


def reproducibility(stage1: Path, stage2: Path) -> dict[str, object]:
    stage1_digest = sha256_file(stage1)
    stage2_digest = sha256_file(stage2)
    return {
        "stage1": str(stage1),
        "stage2": str(stage2),
        "stage1_sha256": stage1_digest,
        "stage2_sha256": stage2_digest,
        "matches": stage1_digest is not None and stage1_digest == stage2_digest,
        "allowed_differences": [],
    }


def readiness_status(mode: str, steps: list[StepResult], repro: dict[str, object]) -> tuple[str, list[str]]:
    reasons: list[str] = []
    by_name = {step.name: step for step in steps}
    for required in ("stage0", "stage1", "stage2", "parity"):
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
    steps.append(
        run_step(
            "stage0",
            ["cargo", "run", "--quiet", "--bin", "aic", "--", "build", "compiler/aic/tools/aic_selfhost", "-o", str(stage0)],
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
    if parity_candidate.is_file():
        steps.append(
            run_step(
                "parity",
                parity_command(parity_candidate, out_dir / "parity-artifacts", parity_report),
                root,
                args.timeout,
                parity_report,
            )
        )

    repro = reproducibility(stage1, stage2)
    status, reasons = readiness_status(args.mode, steps, repro)
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
        "reproducibility": repro,
        "steps": [step.to_json() for step in steps],
        "policy": {
            "experimental": "stage0/parity may run, but stage1/stage2 failures keep self-hosting unsupported",
            "supported": "stage0, stage1, stage2, parity, and reproducibility must pass",
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
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--mode", choices=("experimental", "supported", "default"), default="supported")
    parser.add_argument(
        "--allow-incomplete",
        action="store_true",
        help="write a report and exit 0 even when readiness is experimental",
    )
    return run_bootstrap(parser.parse_args())


if __name__ == "__main__":
    raise SystemExit(main())
