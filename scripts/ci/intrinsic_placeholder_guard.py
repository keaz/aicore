#!/usr/bin/env python3
"""CI policy guard for AGX1 runtime intrinsic declarations.

Rejects source-level body implementations for AGX1 runtime-bound intrinsic
symbols. These APIs must remain declaration-only (`intrinsic fn ...;`) and be
backed by runtime/codegen lowering.
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_POLICY_PATHS = [
    Path("std/concurrent.aic"),
    Path("std/net.aic"),
    Path("std/proc.aic"),
]
# Explicit exemptions for negative fixtures / local custom scans.
DEFAULT_EXEMPTIONS = {
    Path("examples/core/intrinsic_declaration_invalid_body.aic"),
    Path("examples/verify/intrinsics/invalid_bindings.aic"),
}

# AGX1 intrinsic names with a real function-body opening on the header line.
# Declaration-only signatures end with ';' and therefore do not match.
BODY_HEADER_LINE = re.compile(
    r"^\s*(?:intrinsic\s+)?fn\s+"
    r"(?P<name>aic_(?:conc|net|proc)_[A-Za-z0-9_]*_intrinsic)\b[^\n]*\{\s*$"
)


@dataclass(frozen=True)
class Violation:
    code: str
    path: str
    line: int
    column: int
    intrinsic: str
    message: str
    remediation: str



def normalize_display(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.resolve().as_posix()



def normalize_exemption(raw: str) -> str:
    candidate = Path(raw)
    if candidate.is_absolute():
        return candidate.resolve().as_posix()
    return (ROOT / candidate).resolve().as_posix()



def iter_policy_files(paths: list[Path]) -> tuple[list[Path], list[Path]]:
    files: list[Path] = []
    missing: list[Path] = []
    for entry in paths:
        candidate = entry if entry.is_absolute() else ROOT / entry
        if not candidate.exists():
            missing.append(candidate)
            continue
        if candidate.is_dir():
            files.extend(sorted(candidate.rglob("*.aic")))
        elif candidate.suffix == ".aic":
            files.append(candidate)
    files = sorted({path.resolve() for path in files})
    return files, missing



def scan_file(path: Path, exemptions: set[str]) -> list[Violation]:
    if path.resolve().as_posix() in exemptions:
        return []

    text = path.read_text(encoding="utf-8")
    violations: list[Violation] = []
    for line_no, line in enumerate(text.splitlines(), start=1):
        match = BODY_HEADER_LINE.match(line)
        if match is None:
            continue
        intrinsic = match.group("name")
        column = match.start("name") + 1
        violations.append(
            Violation(
                code="AGX1P001",
                path=normalize_display(path),
                line=line_no,
                column=column,
                intrinsic=intrinsic,
                message=(
                    f"runtime-bound intrinsic '{intrinsic}' must be declaration-only "
                    "(`intrinsic fn ...;`)"
                ),
                remediation=(
                    "remove the function body, keep `intrinsic fn ...;`, and route "
                    "behavior through runtime lowering plus a public wrapper"
                ),
            )
        )
    return violations



def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Reject source-level placeholder/fake-success intrinsic stubs in AGX1 "
            "runtime-bound std modules."
        )
    )
    parser.add_argument(
        "--path",
        action="append",
        default=[],
        help=(
            "Optional file/dir path to scan (repeatable). Relative paths are resolved "
            "from repo root. Defaults to AGX1 std policy paths."
        ),
    )
    parser.add_argument(
        "--exempt",
        action="append",
        default=[],
        help="Optional file exemption path (repeatable).",
    )
    return parser.parse_args()



def main() -> int:
    args = parse_args()
    policy_paths = [Path(raw) for raw in args.path] if args.path else list(DEFAULT_POLICY_PATHS)

    files, missing = iter_policy_files(policy_paths)
    if missing:
        for path in sorted(missing):
            print(
                f"AGX1P000 missing policy path: {normalize_display(path)}",
                file=sys.stderr,
            )
        print(
            "remediation: update policy paths passed to intrinsic_placeholder_guard.py",
            file=sys.stderr,
        )
        return 2

    exemptions = {
        path.resolve().as_posix() for path in (ROOT / rel for rel in DEFAULT_EXEMPTIONS)
    }
    exemptions.update(normalize_exemption(raw) for raw in args.exempt)

    violations: list[Violation] = []
    for file in files:
        violations.extend(scan_file(file, exemptions))

    violations.sort(
        key=lambda item: (
            item.path,
            item.line,
            item.column,
            item.code,
            item.intrinsic,
        )
    )

    if violations:
        print("AGX1 intrinsic placeholder guard failed", file=sys.stderr)
        for violation in violations:
            print(
                f"{violation.code} {violation.path}:{violation.line}:{violation.column} "
                f"{violation.message}",
                file=sys.stderr,
            )
            print(f"  remediation: {violation.remediation}", file=sys.stderr)
        print(
            f"checked {len(files)} file(s); found {len(violations)} violation(s)",
            file=sys.stderr,
        )
        return 1

    print(
        f"AGX1 intrinsic placeholder guard passed: checked {len(files)} file(s), 0 violations"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
