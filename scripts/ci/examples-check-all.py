#!/usr/bin/env python3
"""Run `aic check` on every example file and compare with the expectations.

Every `*.aic` file under `examples/` must pass `aic check`, unless
`scripts/ci/example-check-failures.txt` lists it. A listed file must fail
`aic check` and report exactly the listed diagnostic codes. The gate also fails
when the list names a file that no longer exists, names a file twice, or has
no reason for an entry, so the list cannot go stale unnoticed.

Usage: examples-check-all.py <aic command...>
"""

from __future__ import annotations

import concurrent.futures
import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPECTATIONS = ROOT / "scripts" / "ci" / "example-check-failures.txt"
CODE_RE = re.compile(r"\b(E\d{4})\b")
ENTRY_RE = re.compile(r"^(\S+)\s+((?:E\d{4},)*E\d{4})\s+#\s*(\S.*)$")


def load_expectations() -> tuple[dict[str, frozenset[str]], list[str]]:
    expected: dict[str, frozenset[str]] = {}
    errors: list[str] = []
    for number, raw in enumerate(EXPECTATIONS.read_text().splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        match = ENTRY_RE.match(line)
        if not match:
            errors.append(
                f"{EXPECTATIONS.name}:{number}: expected '<path> <E1234[,E5678]> # <reason>', got: {raw}"
            )
            continue
        path, codes, _reason = match.groups()
        if path in expected:
            errors.append(f"{EXPECTATIONS.name}:{number}: {path} is listed twice")
            continue
        if not (ROOT / path).is_file():
            errors.append(f"{EXPECTATIONS.name}:{number}: {path} does not exist")
            continue
        expected[path] = frozenset(codes.split(","))
    return expected, errors


def check(aic: list[str], path: str) -> tuple[str, bool, frozenset[str], str]:
    result = subprocess.run(
        [*aic, "check", path],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    output = result.stdout + result.stderr
    return path, result.returncode == 0, frozenset(CODE_RE.findall(output)), output


def main() -> int:
    aic = sys.argv[1:]
    if not aic:
        print(__doc__, file=sys.stderr)
        return 2
    expected, errors = load_expectations()
    files = sorted(
        str(path.relative_to(ROOT)) for path in (ROOT / "examples").rglob("*.aic")
    )
    workers = max(1, min(8, os.cpu_count() or 1))
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        results = list(pool.map(lambda path: check(aic, path), files))

    for path, passed, codes, output in results:
        want = expected.get(path)
        if want is None:
            if not passed:
                errors.append(
                    f"{path}: aic check failed ({', '.join(sorted(codes)) or 'no code'}); "
                    f"fix the example or list it in {EXPECTATIONS.name}\n{output.rstrip()}"
                )
        elif passed:
            errors.append(
                f"{path}: aic check passed, but {EXPECTATIONS.name} expects "
                f"{','.join(sorted(want))}; remove the entry"
            )
        elif codes != want:
            errors.append(
                f"{path}: aic check reported {','.join(sorted(codes)) or 'no code'}, "
                f"{EXPECTATIONS.name} expects {','.join(sorted(want))}\n{output.rstrip()}"
            )

    if errors:
        for error in errors:
            print(f"examples-check-all: {error}", file=sys.stderr)
        return 1
    failing = sum(1 for path in files if path in expected)
    print(
        f"examples-check-all: ok ({len(files)} files, {len(files) - failing} pass, "
        f"{failing} fail as expected)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
