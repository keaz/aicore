#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-check}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

AIC=(cargo run --quiet --bin aic --)

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
)
check_fail=(
  "examples/effects_reject.aic"
)
run_pass=(
  "examples/option_match.aic"
  "examples/contracts_abs_ok.aic"
)
run_fail=(
  "examples/contracts_abs_fail.aic"
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
  if "${AIC[@]}" run "$file" >/tmp/aic-example.out 2>/tmp/aic-example.err; then
    echo "expected run failure but passed: $file" >&2
    exit 1
  fi
  if ! grep -q "ensures failed" /tmp/aic-example.err && ! grep -q "ensures failed" /tmp/aic-example.out; then
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
    for f in "${run_fail[@]}"; do
      expect_run_fail "$f"
    done
    ;;
  *)
    echo "usage: $0 [check|run]" >&2
    exit 2
    ;;
esac
