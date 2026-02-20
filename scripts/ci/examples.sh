#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-check}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

AIC=(cargo run --quiet --bin aic --)
ARTIFACT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aic-examples.XXXXXX")"
trap 'rm -rf "$ARTIFACT_DIR"' EXIT

check_pass=(
  "examples/option_match.aic"
  "examples/contracts_abs_ok.aic"
  "examples/contracts_abs_fail.aic"
  "examples/non_empty_string.aic"
  "examples/e3/infer_let.aic"
  "examples/e3/generic_id.aic"
  "examples/e3/generic_option_map.aic"
  "examples/e3/result_payloads.aic"
  "examples/e3/match_exhaustive.aic"
  "examples/e3/option_only_absence.aic"
  "examples/e4/effect_decl.aic"
  "examples/e4/contracts_all_returns.aic"
  "examples/e4/non_empty_string_ctor.aic"
  "examples/e4/verified_abs.aic"
  "examples/e5/hello_int.aic"
  "examples/e5/enum_match.aic"
  "examples/e5/generic_pair.aic"
  "examples/e5/string_len.aic"
  "examples/e5/object_link_main.aic"
  "examples/e5/panic_line_map.aic"
  "examples/e6/std_smoke.aic"
  "examples/e6/deps_checksum.aic"
  "examples/e6/doc_sample.aic"
  "examples/e6/deprecated_api_use.aic"
  "examples/e6/pkg_app"
  "examples/e7/cli_smoke.aic"
  "examples/e7/test_harness_sample.aic"
  "examples/e7/lsp_project"
)
check_fail=(
  "examples/effects_reject.aic"
  "examples/e4/transitive_effect_violation.aic"
  "examples/e7/diag_errors.aic"
)
run_pass=(
  "examples/option_match.aic"
  "examples/contracts_abs_ok.aic"
  "examples/e4/verified_abs.aic"
  "examples/e5/hello_int.aic"
  "examples/e5/enum_match.aic"
  "examples/e5/generic_pair.aic"
  "examples/e5/string_len.aic"
  "examples/e6/std_smoke.aic"
  "examples/e6/deps_checksum.aic"
  "examples/e6/pkg_app"
  "examples/e7/cli_smoke.aic"
)
run_fail=(
  "examples/contracts_abs_fail.aic:ensures failed"
  "examples/e4/contracts_all_returns.aic:ensures failed"
  "examples/e5/panic_line_map.aic:AICore panic at"
)

expect_check_fail() {
  local file="$1"
  if "${AIC[@]}" check "$file" >/tmp/aic-example.out 2>/tmp/aic-example.err; then
    echo "expected check failure but passed: $file" >&2
    cat /tmp/aic-example.out >&2 || true
    cat /tmp/aic-example.err >&2 || true
    exit 1
  fi
}

expect_run_fail() {
  local file="$1"
  local marker="${2:-ensures failed}"
  if "${AIC[@]}" run "$file" >/tmp/aic-example.out 2>/tmp/aic-example.err; then
    echo "expected run failure but passed: $file" >&2
    exit 1
  fi
  if ! grep -q "$marker" /tmp/aic-example.err && ! grep -q "$marker" /tmp/aic-example.out; then
    echo "expected contract failure marker not found for: $file" >&2
    cat /tmp/aic-example.out >&2 || true
    cat /tmp/aic-example.err >&2 || true
    exit 1
  fi
}

expect_run_value() {
  local file="$1"
  local expected="$2"
  "${AIC[@]}" run "$file" >/tmp/aic-example.out
  local got
  got="$(tr -d '\r' </tmp/aic-example.out | tail -n 1)"
  if [[ "$got" != "$expected" ]]; then
    echo "unexpected output for $file: expected '$expected' got '$got'" >&2
    cat /tmp/aic-example.out >&2 || true
    exit 1
  fi
}

expect_build_artifact() {
  local file="$1"
  local artifact="$2"
  local out="$3"
  "${AIC[@]}" build "$file" --artifact "$artifact" -o "$out" >/dev/null
  if [[ ! -f "$out" ]]; then
    echo "expected artifact missing: $out" >&2
    exit 1
  fi
}

expect_file_exists() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "expected file missing: $path" >&2
    exit 1
  fi
}

case "$MODE" in
  check)
    for f in "${check_pass[@]}"; do
      "${AIC[@]}" check "$f" >/dev/null
    done
    for f in "${check_fail[@]}"; do
      expect_check_fail "$f"
    done
    ;;
  run)
    expect_run_value "examples/option_match.aic" "42"
    expect_run_value "examples/contracts_abs_ok.aic" "7"
    expect_run_value "examples/e4/verified_abs.aic" "7"
    expect_run_value "examples/e5/hello_int.aic" "42"
    expect_run_value "examples/e5/enum_match.aic" "42"
    expect_run_value "examples/e5/generic_pair.aic" "42"
    expect_run_value "examples/e5/string_len.aic" "5"
    expect_run_value "examples/e6/std_smoke.aic" "1"
    expect_run_value "examples/e6/deps_checksum.aic" "42"
    expect_run_value "examples/e6/pkg_app" "42"
    expect_run_value "examples/e7/cli_smoke.aic" "42"
    "${AIC[@]}" lock "examples/e6/pkg_app" >/dev/null
    "${AIC[@]}" check "examples/e6/pkg_app" --offline >/dev/null
    if "${AIC[@]}" check "examples/e7/diag_errors.aic" --sarif >"$ARTIFACT_DIR/diag_errors.sarif"; then
      echo "expected sarif check failure but passed: examples/e7/diag_errors.aic" >&2
      exit 1
    fi
    python3 -m json.tool "$ARTIFACT_DIR/diag_errors.sarif" >/dev/null
    "${AIC[@]}" explain "E2001" >/dev/null
    if "${AIC[@]}" explain "E9999" >/tmp/aic-example.out; then
      echo "expected explain unknown-code failure but passed" >&2
      exit 1
    fi
    "${AIC[@]}" contract --json >/tmp/aic-example.out
    python3 -m json.tool /tmp/aic-example.out >/dev/null
    "${AIC[@]}" test "examples/e7/harness" --json >"$ARTIFACT_DIR/harness_report.json"
    python3 -m json.tool "$ARTIFACT_DIR/harness_report.json" >/dev/null
    grep -q '"failed": 0' "$ARTIFACT_DIR/harness_report.json"
    DOC_DIR="$ARTIFACT_DIR/doc_sample"
    "${AIC[@]}" doc "examples/e6/doc_sample.aic" -o "$DOC_DIR" >/dev/null
    expect_file_exists "$DOC_DIR/index.md"
    expect_file_exists "$DOC_DIR/api.json"
    expect_build_artifact "examples/e5/object_link_main.aic" "obj" "$ARTIFACT_DIR/object_link_main.o"
    expect_build_artifact "examples/e5/object_link_main.aic" "lib" "$ARTIFACT_DIR/libobject_link_main.a"
    for entry in "${run_fail[@]}"; do
      file="${entry%%:*}"
      marker="${entry#*:}"
      expect_run_fail "$file" "$marker"
    done
    ;;
  *)
    echo "usage: $0 [check|run]" >&2
    exit 2
    ;;
esac
