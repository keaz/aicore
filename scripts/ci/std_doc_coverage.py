#!/usr/bin/env python3
"""Enforce and optionally autofix std module doc-comment coverage.

Checks:
- module-level docs (`///`) before `module ...;`
- docs for `struct`, `enum`, `trait`
- docs for enum variants
- docs for `fn` and `intrinsic fn` declarations (including impl methods)
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path


MODULE_RE = re.compile(r"^(\s*)module\s+([A-Za-z0-9_.]+)\s*;")
TYPE_RE = re.compile(
    r"^(\s*)(struct|enum|trait)\s+([A-Za-z_][A-Za-z0-9_]*)(?:\[[^\]]*\])?\b"
)
FN_RE = re.compile(
    r"^(\s*)(?:intrinsic\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)(?:\[[^\]]*\])?\s*\("
)
ENUM_START_RE = re.compile(
    r"^(\s*)enum\s+([A-Za-z_][A-Za-z0-9_]*)(?:\[[^\]]*\])?\s*\{"
)
ENUM_VARIANT_RE = re.compile(
    r"^(\s*)([A-Za-z_][A-Za-z0-9_]*)(?:\([^)]*\))?,?\s*$"
)


@dataclass
class Missing:
    kind: str
    name: str
    line: int


@dataclass
class Coverage:
    module: int = 0
    type_decl: int = 0
    enum_variant: int = 0
    function: int = 0
    module_missing: int = 0
    type_missing: int = 0
    enum_variant_missing: int = 0
    function_missing: int = 0

    def missing_total(self) -> int:
        return (
            self.module_missing
            + self.type_missing
            + self.enum_variant_missing
            + self.function_missing
        )


def has_doc_comment_before(lines: list[str], index: int) -> bool:
    i = index - 1
    while i >= 0 and lines[i].strip() == "":
        i -= 1
    return i >= 0 and lines[i].lstrip().startswith("///")


def module_topic(module_name: str) -> str:
    tail = module_name.split(".")[-1]
    return tail.replace("_", " ")


def module_doc_block(indent: str, module_name: str) -> list[str]:
    topic = module_topic(module_name)
    return [
        f"{indent}/// The `{module_name}` module provides standard-library `{topic}` APIs.",
        f"{indent}///",
        f"{indent}/// ## Example",
        f"{indent}/// ```aic",
        f"{indent}/// import {module_name};",
        f"{indent}/// ```",
    ]


def type_doc_block(indent: str, kind: str, name: str, module_name: str) -> list[str]:
    return [
        f"{indent}/// `{name}` is a `{kind}` in `{module_name}`.",
        f"{indent}///",
        f"{indent}/// ## Example",
        f"{indent}/// ```aic",
        f"{indent}/// // Use `{name}` through `{module_name}` APIs.",
        f"{indent}/// ```",
    ]


def variant_doc_block(indent: str, variant: str) -> list[str]:
    return [f"{indent}/// `{variant}` enum variant."]


def function_doc_block(indent: str, name: str, module_name: str) -> list[str]:
    return [
        f"{indent}/// `{name}` in `{module_name}`.",
        f"{indent}///",
        f"{indent}/// ## Example",
        f"{indent}/// ```aic",
        f"{indent}/// // Import `{module_name}` and call `{name}` with module-specific arguments.",
        f"{indent}/// ```",
        f"{indent}///",
        f"{indent}/// ## Effects",
        f"{indent}/// See the function signature for required effects and capabilities.",
        f"{indent}///",
        f"{indent}/// ## Errors",
        f"{indent}/// Returns module-defined error variants when the operation fails.",
    ]


def analyze_and_fix(path: Path, fix: bool) -> tuple[Coverage, list[Missing], bool]:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    coverage = Coverage()
    missing: list[Missing] = []
    inserts: list[tuple[int, list[str]]] = []

    module_name = f"std.{path.stem}"
    in_enum = False

    for idx, line in enumerate(lines):
        module_match = MODULE_RE.match(line)
        if module_match:
            coverage.module += 1
            module_name = module_match.group(2)
            if not has_doc_comment_before(lines, idx):
                coverage.module_missing += 1
                missing.append(Missing("module", module_name, idx + 1))
                if fix:
                    inserts.append(
                        (idx, module_doc_block(module_match.group(1), module_name))
                    )
            continue

        enum_start = ENUM_START_RE.match(line)
        if enum_start:
            in_enum = True

        type_match = TYPE_RE.match(line)
        if type_match:
            coverage.type_decl += 1
            if not has_doc_comment_before(lines, idx):
                coverage.type_missing += 1
                kind = type_match.group(2)
                name = type_match.group(3)
                missing.append(Missing(kind, name, idx + 1))
                if fix:
                    inserts.append(
                        (
                            idx,
                            type_doc_block(
                                type_match.group(1),
                                kind,
                                name,
                                module_name,
                            ),
                        )
                    )

        fn_match = FN_RE.match(line)
        if fn_match:
            coverage.function += 1
            if not has_doc_comment_before(lines, idx):
                coverage.function_missing += 1
                name = fn_match.group(2)
                missing.append(Missing("fn", name, idx + 1))
                if fix:
                    inserts.append(
                        (
                            idx,
                            function_doc_block(fn_match.group(1), name, module_name),
                        )
                    )

        if in_enum:
            stripped = line.strip()
            if stripped == "" or stripped.startswith("///"):
                continue
            if stripped.startswith("}"):
                in_enum = False
                continue
            variant_match = ENUM_VARIANT_RE.match(line)
            if variant_match:
                variant = variant_match.group(2)
                # Skip obvious non-variant lines (defensive).
                if variant in {"match", "if", "while", "for"}:
                    continue
                coverage.enum_variant += 1
                if not has_doc_comment_before(lines, idx):
                    coverage.enum_variant_missing += 1
                    missing.append(Missing("variant", variant, idx + 1))
                    if fix:
                        inserts.append(
                            (
                                idx,
                                variant_doc_block(variant_match.group(1), variant),
                            )
                        )

    changed = False
    if fix and inserts:
        out = list(lines)
        offset = 0
        for index, block in sorted(inserts, key=lambda item: item[0]):
            insert_at = index + offset
            out[insert_at:insert_at] = block
            offset += len(block)
        new_text = "\n".join(out) + ("\n" if text.endswith("\n") else "")
        if new_text != text:
            path.write_text(new_text, encoding="utf-8")
            changed = True
    return coverage, missing, changed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--std-dir",
        default="std",
        help="Path to std module directory (default: std)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Fail when missing doc comments are detected",
    )
    parser.add_argument(
        "--fix",
        action="store_true",
        help="Insert templated doc comments for missing declarations",
    )
    args = parser.parse_args()

    if args.check and args.fix:
        print("std-doc-coverage: use either --check or --fix, not both", file=sys.stderr)
        return 2

    std_dir = Path(args.std_dir)
    files = sorted(std_dir.glob("*.aic"))
    if not files:
        print(f"std-doc-coverage: no .aic files under {std_dir}", file=sys.stderr)
        return 2

    total = Coverage()
    all_missing: list[tuple[Path, Missing]] = []
    changed_files = 0

    for path in files:
        coverage, missing, changed = analyze_and_fix(path, args.fix)
        if changed:
            changed_files += 1
        total.module += coverage.module
        total.type_decl += coverage.type_decl
        total.enum_variant += coverage.enum_variant
        total.function += coverage.function
        total.module_missing += coverage.module_missing
        total.type_missing += coverage.type_missing
        total.enum_variant_missing += coverage.enum_variant_missing
        total.function_missing += coverage.function_missing
        for item in missing:
            all_missing.append((path, item))

    if args.fix:
        print(
            "std-doc-coverage: fixed {} file(s); remaining missing docs={}".format(
                changed_files, total.missing_total()
            )
        )
        return 0

    missing_total = total.missing_total()
    if missing_total == 0:
        print(
            "std-doc-coverage: ok (module={}, type={}, variant={}, fn={})".format(
                total.module,
                total.type_decl,
                total.enum_variant,
                total.function,
            )
        )
        return 0

    print(
        "std-doc-coverage: missing docs={} (module={}, type={}, variant={}, fn={})".format(
            missing_total,
            total.module_missing,
            total.type_missing,
            total.enum_variant_missing,
            total.function_missing,
        ),
        file=sys.stderr,
    )
    for path, item in all_missing[:200]:
        print(
            f"{path}:{item.line}: missing {item.kind} doc for `{item.name}`",
            file=sys.stderr,
        )
    if len(all_missing) > 200:
        print(
            f"... and {len(all_missing) - 200} more missing doc entries",
            file=sys.stderr,
        )
    return 1 if args.check else 0


if __name__ == "__main__":
    raise SystemExit(main())
