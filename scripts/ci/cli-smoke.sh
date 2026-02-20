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
OBJ_ARTIFACT="$PROJECT_DIR/app.o"
LIB_ARTIFACT="$PROJECT_DIR/libapp.a"
DBG_BIN="$PROJECT_DIR/app-debug"
DOC_DIR="$PROJECT_DIR/docs/api"
REPRO_MANIFEST="$TMP_DIR/repro-manifest.json"
SBOM_JSON="$TMP_DIR/sbom.json"
PROVENANCE_JSON="$TMP_DIR/provenance.json"

"${AIC[@]}" init "$PROJECT_DIR" >/dev/null

# init output must already be canonical.
"${AIC[@]}" fmt --check "$MAIN_FILE"
"${AIC[@]}" check "$MAIN_FILE" >/dev/null
"${AIC[@]}" ir "$MAIN_FILE" --emit json >/dev/null
if "${AIC[@]}" check "$MAIN_FILE" --json --sarif >/dev/null 2>&1; then
  echo "expected usage error for --json + --sarif conflict" >&2
  exit 1
fi
"${AIC[@]}" build "$MAIN_FILE" -o "$APP_BIN" >/dev/null
"${AIC[@]}" build "$MAIN_FILE" --artifact obj -o "$OBJ_ARTIFACT" >/dev/null
"${AIC[@]}" build "$MAIN_FILE" --artifact lib -o "$LIB_ARTIFACT" >/dev/null
"${AIC[@]}" build "$MAIN_FILE" --debug-info -o "$DBG_BIN" >/dev/null
"${AIC[@]}" lock "$PROJECT_DIR" >/dev/null
"${AIC[@]}" check "$PROJECT_DIR" --offline >/dev/null
"${AIC[@]}" doc "$MAIN_FILE" -o "$DOC_DIR" >/dev/null
"${AIC[@]}" std-compat --check >/dev/null
"${AIC[@]}" release manifest --root "$ROOT_DIR" --output "$REPRO_MANIFEST" --source-date-epoch 1700000000 >/dev/null
"${AIC[@]}" release verify-manifest --root "$ROOT_DIR" --manifest "$REPRO_MANIFEST" >/dev/null
"${AIC[@]}" release sbom --root "$ROOT_DIR" --output "$SBOM_JSON" --source-date-epoch 1700000000 >/dev/null
AIC_SIGNING_KEY="cli-smoke-signing-key" "${AIC[@]}" release provenance --artifact "$APP_BIN" --sbom "$SBOM_JSON" --manifest "$REPRO_MANIFEST" --output "$PROVENANCE_JSON" --key-env AIC_SIGNING_KEY --key-id cli-smoke >/dev/null
AIC_SIGNING_KEY="cli-smoke-signing-key" "${AIC[@]}" release verify-provenance --provenance "$PROVENANCE_JSON" --key-env AIC_SIGNING_KEY >/dev/null
"${AIC[@]}" release policy --check >/dev/null
"${AIC[@]}" release security-audit --json >/dev/null
"${AIC[@]}" explain E2001 >/dev/null
"${AIC[@]}" contract --json >/dev/null
"${AIC[@]}" test examples/e7/harness --json >/dev/null
if "${AIC[@]}" check "examples/e7/diag_errors.aic" --sarif >"$TMP_DIR/diag.sarif"; then
  echo "expected diagnostics failure for diag_errors.aic" >&2
  exit 1
fi
python3 -m json.tool "$TMP_DIR/diag.sarif" >/dev/null

[[ -f "$OBJ_ARTIFACT" ]]
[[ -f "$LIB_ARTIFACT" ]]
[[ -f "$DBG_BIN" ]]
[[ -f "$PROJECT_DIR/aic.lock" ]]
[[ -f "$DOC_DIR/index.md" ]]
[[ -f "$DOC_DIR/api.json" ]]

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

"${AIC[@]}" run "$MAIN_FILE" --sandbox ci >"$TMP_DIR/run-sandbox.out"
SANDBOX_RESULT="$(tr -d '\r' <"$TMP_DIR/run-sandbox.out" | tail -n 1)"
if [[ "$SANDBOX_RESULT" != "10" ]]; then
  echo "unexpected 'aic run --sandbox ci' output: '$SANDBOX_RESULT'" >&2
  exit 1
fi
