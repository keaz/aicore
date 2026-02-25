#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

if [[ -x "$ROOT_DIR/target/debug/aic" ]]; then
  AIC=("$ROOT_DIR/target/debug/aic")
else
  AIC=(cargo run --quiet --bin aic --)
fi
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aic-repro.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

MANIFEST_A="$TMP_DIR/repro-a.json"
MANIFEST_B="$TMP_DIR/repro-b.json"

echo "repro-check: generating manifest A"
"${AIC[@]}" release manifest --root "$ROOT_DIR" --output "$MANIFEST_A" --source-date-epoch 1700000000 >/dev/null
echo "repro-check: generating manifest B"
"${AIC[@]}" release manifest --root "$ROOT_DIR" --output "$MANIFEST_B" --source-date-epoch 1700000000 >/dev/null

echo "repro-check: comparing manifests"
if ! diff -u "$MANIFEST_A" "$MANIFEST_B" >/dev/null; then
  echo "reproducibility check failed: manifests differ across identical runs" >&2
  diff -u "$MANIFEST_A" "$MANIFEST_B" >&2 || true
  exit 1
fi

echo "repro-check: verifying manifest"
"${AIC[@]}" release verify-manifest --root "$ROOT_DIR" --manifest "$MANIFEST_A" >/dev/null

echo "reproducibility check: ok"
