#!/usr/bin/env python3
"""Deterministic validator/normalizer for Wave 5 numeric API inventory."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

REQUIRED_ROW_FIELDS = [
    "module",
    "symbol",
    "current_signature",
    "category",
    "target_type_policy",
    "migration_action",
    "compatibility_wrapper",
    "linked_issue",
]

CANONICAL_TOP_LEVEL_KEYS = ["wave", "source_issue", "ordering", "row_count", "rows"]
DEFAULT_JSON_PATH = Path("docs/numeric-api-adoption-wave5.json")


def normalize_rows(rows: list[dict[str, object]]) -> list[dict[str, object]]:
    normalized: list[dict[str, object]] = []
    for idx, row in enumerate(rows):
        missing = [key for key in REQUIRED_ROW_FIELDS if key not in row]
        if missing:
            raise ValueError(f"row {idx} missing required fields: {', '.join(missing)}")
        ordered_row = {key: row[key] for key in REQUIRED_ROW_FIELDS}
        normalized.append(ordered_row)

    normalized.sort(
        key=lambda row: (
            str(row["module"]),
            str(row["symbol"]),
            str(row["current_signature"]),
        )
    )
    return normalized


def normalize_document(doc: dict[str, object]) -> dict[str, object]:
    rows_obj = doc.get("rows")
    if not isinstance(rows_obj, list):
        raise ValueError("top-level 'rows' must be a list")

    normalized_rows = normalize_rows(rows_obj)

    normalized_doc: dict[str, object] = {
        "wave": str(doc.get("wave", "5A")),
        "source_issue": str(doc.get("source_issue", "#330")),
        "ordering": ["module", "symbol", "current_signature"],
        "row_count": len(normalized_rows),
        "rows": normalized_rows,
    }

    extra_keys = sorted(key for key in doc.keys() if key not in CANONICAL_TOP_LEVEL_KEYS)
    if extra_keys:
        raise ValueError(f"unexpected top-level keys: {', '.join(extra_keys)}")

    return normalized_doc


def render_json(doc: dict[str, object]) -> str:
    return json.dumps(doc, indent=2, ensure_ascii=True) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate and normalize docs/numeric-api-adoption-wave5.json"
    )
    parser.add_argument(
        "--path",
        type=Path,
        default=DEFAULT_JSON_PATH,
        help=f"Path to inventory JSON (default: {DEFAULT_JSON_PATH})",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Validate determinism without writing changes",
    )
    args = parser.parse_args()

    path = args.path
    raw_text = path.read_text(encoding="utf-8")
    parsed = json.loads(raw_text)
    if not isinstance(parsed, dict):
        raise ValueError("inventory JSON root must be an object")

    normalized = normalize_document(parsed)
    normalized_text = render_json(normalized)

    if args.check:
        if raw_text != normalized_text:
            print(f"{path}: not normalized", file=sys.stderr)
            return 1
        print(f"{path}: OK")
        return 0

    path.write_text(normalized_text, encoding="utf-8")
    print(f"{path}: normalized {normalized['row_count']} rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
