#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

AIC=(cargo run --quiet --bin aic --)
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aic-smoke.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

PROJECT_DIR="$TMP_DIR/demo"
MAIN_FILE="$PROJECT_DIR/src/main.aic"
APP_BIN="$PROJECT_DIR/app"

"${AIC[@]}" init "$PROJECT_DIR" >/dev/null

# init output is intentionally not canonical; format once, then enforce stability.
"${AIC[@]}" fmt "$MAIN_FILE"
"${AIC[@]}" fmt --check "$MAIN_FILE"
"${AIC[@]}" check "$MAIN_FILE" >/dev/null
"${AIC[@]}" ir "$MAIN_FILE" --emit json >/dev/null
"${AIC[@]}" build "$MAIN_FILE" -o "$APP_BIN" >/dev/null

"$APP_BIN" >"$TMP_DIR/direct.out"
DIRECT_RESULT="$(tr -d '\r' <"$TMP_DIR/direct.out" | tail -n 1)"
if [[ "$DIRECT_RESULT" != "10" ]]; then
  echo "unexpected direct binary output: '$DIRECT_RESULT'" >&2
  exit 1
fi

"${AIC[@]}" run "$MAIN_FILE" >"$TMP_DIR/run.out"
RUN_RESULT="$(tr -d '\r' <"$TMP_DIR/run.out" | tail -n 1)"
if [[ "$RUN_RESULT" != "10" ]]; then
  echo "unexpected 'aic run' output: '$RUN_RESULT'" >&2
  exit 1
fi
