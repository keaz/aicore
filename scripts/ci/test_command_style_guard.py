#!/usr/bin/env python3
"""CI policy guard for canonical cargo test command style.

Reject contributor-facing command snippets that use positional test-name
filters without explicit `--test <target>` selection.
"""

from __future__ import annotations

import argparse
import re
import shlex
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SCAN_PATHS = [
    Path("docs"),
    Path(".github"),
    Path("README.md"),
    Path("AGENTS.md"),
]
TEXT_EXTENSIONS = {".md", ".yml", ".yaml", ".txt"}
COMMAND_SEGMENT = re.compile(r"cargo\s+test[^\n`|]*")
OPTION_REQUIRES_VALUE = {
    "-j",
    "--jobs",
    "-p",
    "--package",
    "--features",
    "--manifest-path",
    "--target",
    "--target-dir",
    "--profile",
    "--color",
    "--message-format",
    "--config",
    "--timings",
    "--exclude",
    "--bin",
    "--example",
    "--bench",
}
LONG_OPTION_EQ_PREFIXES = tuple(
    f"{flag}="
    for flag in (
        "--jobs",
        "--package",
        "--features",
        "--manifest-path",
        "--target",
        "--target-dir",
        "--profile",
        "--color",
        "--message-format",
        "--config",
        "--timings",
        "--exclude",
        "--bin",
        "--example",
        "--bench",
    )
)


@dataclass(frozen=True)
class Violation:
    code: str
    path: str
    line: int
    column: int
    command: str
    positional: str
    message: str
    remediation: str


def normalize_display(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.resolve().as_posix()


def iter_scan_files(paths: list[Path]) -> tuple[list[Path], list[Path]]:
    files: list[Path] = []
    missing: list[Path] = []
    for entry in paths:
        candidate = entry if entry.is_absolute() else ROOT / entry
        if not candidate.exists():
            missing.append(candidate)
            continue
        if candidate.is_dir():
            for file in sorted(candidate.rglob("*")):
                if not file.is_file():
                    continue
                if file.suffix in TEXT_EXTENSIONS or file.name in {"README", "README.md"}:
                    files.append(file)
        elif candidate.is_file():
            files.append(candidate)
    files = sorted({path.resolve() for path in files})
    return files, missing


def command_segments(line: str) -> list[tuple[str, int]]:
    segments: list[tuple[str, int]] = []
    for match in COMMAND_SEGMENT.finditer(line):
        segment = match.group(0).strip().rstrip(".,;:)]")
        segments.append((segment, match.start()))
    return segments


def split_shell(command: str) -> list[str]:
    try:
        return shlex.split(command, posix=True)
    except ValueError:
        return command.split()


def ambiguous_positional_filter(command: str) -> str | None:
    tokens = split_shell(command)
    if len(tokens) < 2 or tokens[0] != "cargo" or tokens[1] != "test":
        return None

    args = tokens[2:]
    if not args:
        return None

    if "--test" in args:
        return None

    i = 0
    while i < len(args):
        token = args[i]
        if token == "--":
            break
        if token in OPTION_REQUIRES_VALUE:
            i += 2
            continue
        if token.startswith(LONG_OPTION_EQ_PREFIXES):
            i += 1
            continue
        if token.startswith("-"):
            i += 1
            continue
        return token
    return None


def scan_file(path: Path) -> tuple[list[Violation], int]:
    text = path.read_text(encoding="utf-8")
    violations: list[Violation] = []
    command_count = 0
    for line_no, line in enumerate(text.splitlines(), start=1):
        if "cargo test" not in line:
            continue
        for segment, column_offset in command_segments(line):
            command_count += 1
            positional = ambiguous_positional_filter(segment)
            if positional is None:
                continue
            violations.append(
                Violation(
                    code="TCS001",
                    path=normalize_display(path),
                    line=line_no,
                    column=column_offset + 1,
                    command=segment,
                    positional=positional,
                    message=(
                        "ambiguous cargo test command uses positional filter "
                        f"'{positional}' without `--test <target>`"
                    ),
                    remediation=(
                        "rewrite command to use explicit target selection, e.g. "
                        "`cargo test --locked --test <target> <name_filter>`"
                    ),
                )
            )
    return violations, command_count


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Reject ambiguous cargo test command snippets that omit explicit "
            "`--test <target>` selection."
        )
    )
    parser.add_argument(
        "--path",
        action="append",
        default=[],
        help=(
            "Optional file/dir path to scan (repeatable). Relative paths are resolved "
            "from repo root. Defaults to docs/.github/README.md/AGENTS.md."
        ),
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    scan_paths = [Path(raw) for raw in args.path] if args.path else list(DEFAULT_SCAN_PATHS)
    files, missing = iter_scan_files(scan_paths)

    if missing:
        for path in sorted(missing):
            print(
                f"TCS000 missing scan path: {normalize_display(path)}",
                file=sys.stderr,
            )
        print(
            "remediation: update --path arguments or ensure required paths exist",
            file=sys.stderr,
        )
        return 2

    violations: list[Violation] = []
    command_count = 0
    for file in files:
        file_violations, file_commands = scan_file(file)
        command_count += file_commands
        violations.extend(file_violations)

    violations.sort(key=lambda item: (item.path, item.line, item.column, item.code))

    if violations:
        print("cargo test command-style guard failed", file=sys.stderr)
        for violation in violations:
            print(
                f"{violation.code} {violation.path}:{violation.line}:{violation.column} "
                f"{violation.message}",
                file=sys.stderr,
            )
            print(f"  command: {violation.command}", file=sys.stderr)
            print(f"  remediation: {violation.remediation}", file=sys.stderr)
        print(
            f"checked {len(files)} file(s); scanned {command_count} command snippet(s); "
            f"found {len(violations)} violation(s)",
            file=sys.stderr,
        )
        return 1

    print(
        "cargo test command-style guard passed: "
        f"checked {len(files)} file(s), scanned {command_count} command snippet(s), "
        "0 violations"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
