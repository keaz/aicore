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
  "examples/interop/wasm_hello_world.aic"
  "examples/io/fs_backup.aic"
  "examples/io/fs_all_ops.aic"
  "examples/io/raii_file_cleanup.aic"
  "examples/io/drop_trait_cleanup.aic"
  "examples/io/stream_copy.aic"
  "examples/io/error_handling.aic"
  "examples/io/interactive_cli.aic"
  "examples/io/stderr_logging.aic"
  "examples/io/cli_file_pipeline.aic"
  "examples/io/process_pipeline.aic"
  "examples/io/tcp_echo.aic"
  "examples/io/tcp_echo_client.aic"
  "examples/io/async_net_event_loop.aic"
  "examples/io/async_await_submit_bridge.aic"
  "examples/io/http_server_hello.aic"
  "examples/io/http_router.aic"
  "examples/io/retry_with_jitter.aic"
  "examples/io/worker_pool.aic"
  "examples/io/structured_concurrency.aic"
  "examples/io/interactive_greeter.aic"
  "examples/io/file_processor.aic"
  "examples/io/log_tee.aic"
  "examples/io/env_config.aic"
  "examples/io/subprocess_pipeline.aic"
  "examples/data/bitwise_protocol.aic"
  "examples/data/log_parse_regex.aic"
  "examples/data/join_module_qualification.aic"
  "examples/data/map_headers.aic"
  "examples/data/deque_workloads.aic"
  "examples/data/float_ops.aic"
  "examples/data/config_json.aic"
  "examples/data/serde_models.aic"
  "examples/data/serde_negative_cases.aic"
  "examples/data/string_ops.aic"
  "examples/data/char_ops.aic"
  "examples/data/template_literals.aic"
  "examples/data/http_types.aic"
  "examples/data/audit_timestamps.aic"
  "examples/data/ingest_transform_emit.aic"
  "examples/data/data_stack_negative_cases.aic"
  "examples/data/url_http_negative_cases.aic"
  "examples/e6/deps_checksum.aic"
  "examples/e6/doc_sample.aic"
  "examples/e6/deprecated_api_use.aic"
  "examples/pkg/ffi_zlib.aic"
  "examples/pkg/policy_enforced_project"
  "examples/pkg/workspace_demo"
  "examples/e6/pkg_app"
  "examples/e7/cli_smoke.aic"
  "examples/e7/test_harness_sample.aic"
  "examples/e7/lsp_project"
  "examples/vscode/snippets_showcase.aic"
  "examples/vscode/inlay_hints_demo.aic"
  "examples/vscode/semantic_highlighting_showcase.aic"
  "examples/vscode/status_bar_demo.aic"
  "examples/vscode/symbol_outline.aic"
  "examples/vscode/marketplace_packaging_demo.aic"
  "examples/vscode/folding_selection_showcase.aic"
  "examples/vscode/auto_import_workspace"
  "examples/vscode/call_hierarchy_showcase.aic"
  "examples/vscode/extension_test_suite_demo.aic"
  "examples/agent/lsp_workspace"
  "examples/agent/incremental_demo/app"
  "examples/e8/conformance_pack/syntax/module_import_match.aic"
  "examples/e8/conformance_pack/typing/generics_inference.aic"
  "examples/e8/conformance_pack/codegen/enum_codegen.aic"
  "examples/e8/roundtrip_random_seed.aic"
  "examples/e8/matrix_program.aic"
  "examples/e8/large_project_bench/bench01_math.aic"
  "examples/e8/large_project_bench/bench02_adt.aic"
  "examples/e8/large_project_bench/bench03_effects_contracts.aic"
  "examples/e9/sandbox_smoke.aic"
  "examples/ops/sandbox_profiles/fs_blocked_demo.aic"
  "examples/ops/sandbox_profiles/net_blocked_demo.aic"
  "examples/ops/sandbox_profiles/proc_blocked_demo.aic"
  "examples/ops/sandbox_profiles/time_blocked_demo.aic"
  "examples/ops/observability_demo/main.aic"
  "examples/core/async_ping.aic"
  "examples/core/trait_sort.aic"
  "examples/core/trait_methods.aic"
  "examples/core/borrow_checker_completeness.aic"
  "examples/core/result_propagation.aic"
  "examples/core/mut_vec.aic"
  "examples/core/vec_capacity.aic"
  "examples/core/opt_levels_demo.aic"
  "examples/core/loop_control.aic"
  "examples/core/closure_fn_values.aic"
  "examples/core/leak_check_closure_capture.aic"
  "examples/core/tuple_types.aic"
  "examples/core/named_function_arguments.aic"
  "examples/core/struct_methods.aic"
  "examples/core/struct_defaults.aic"
  "examples/core/visibility_modifiers_demo"
  "examples/core/enum_methods_option_result.aic"
  "examples/core/cross_compile_targets.aic"
  "examples/core/pattern_or.aic"
  "examples/core/pattern_guard_check.aic"
  "examples/verify/file_protocol.aic"
  "examples/verify/range_proofs.aic"
  "examples/verify/qv_contract_proof_fixed.aic"
)
check_fail=(
  "examples/effects_reject.aic"
  "examples/e4/transitive_effect_violation.aic"
  "examples/e7/diag_errors.aic"
  "examples/vscode/error_lens_diagnostics.aic"
  "examples/io/effect_misuse_fs.aic"
  "examples/e8/conformance_pack/diagnostics/missing_effect.aic"
  "examples/verify/file_protocol_invalid.aic"
  "examples/verify/qv_contract_proof_fail.aic"
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
  "examples/io/fs_backup.aic"
  "examples/io/fs_all_ops.aic"
  "examples/io/raii_file_cleanup.aic"
  "examples/io/drop_trait_cleanup.aic"
  "examples/io/stream_copy.aic"
  "examples/io/error_handling.aic"
  "examples/io/cli_file_pipeline.aic"
  "examples/io/process_pipeline.aic"
  "examples/io/tcp_echo.aic"
  "examples/io/tcp_echo_client.aic"
  "examples/io/async_net_event_loop.aic"
  "examples/io/async_await_submit_bridge.aic"
  "examples/io/http_server_hello.aic"
  "examples/io/http_router.aic"
  "examples/io/retry_with_jitter.aic"
  "examples/io/worker_pool.aic"
  "examples/io/structured_concurrency.aic"
  "examples/io/file_processor.aic"
  "examples/io/log_tee.aic"
  "examples/io/env_config.aic"
  "examples/io/subprocess_pipeline.aic"
  "examples/data/bitwise_protocol.aic"
  "examples/data/log_parse_regex.aic"
  "examples/data/join_module_qualification.aic"
  "examples/data/map_headers.aic"
  "examples/data/deque_workloads.aic"
  "examples/data/float_ops.aic"
  "examples/data/config_json.aic"
  "examples/data/serde_models.aic"
  "examples/data/serde_negative_cases.aic"
  "examples/data/string_ops.aic"
  "examples/data/char_ops.aic"
  "examples/data/template_literals.aic"
  "examples/data/http_types.aic"
  "examples/data/audit_timestamps.aic"
  "examples/data/ingest_transform_emit.aic"
  "examples/data/data_stack_negative_cases.aic"
  "examples/data/url_http_negative_cases.aic"
  "examples/e6/deps_checksum.aic"
  "examples/e6/pkg_app"
  "examples/e7/cli_smoke.aic"
  "examples/vscode/snippets_showcase.aic"
  "examples/vscode/inlay_hints_demo.aic"
  "examples/vscode/semantic_highlighting_showcase.aic"
  "examples/vscode/status_bar_demo.aic"
  "examples/vscode/symbol_outline.aic"
  "examples/vscode/marketplace_packaging_demo.aic"
  "examples/vscode/folding_selection_showcase.aic"
  "examples/vscode/auto_import_workspace"
  "examples/vscode/call_hierarchy_showcase.aic"
  "examples/vscode/extension_test_suite_demo.aic"
  "examples/agent/lsp_workspace"
  "examples/agent/incremental_demo/app"
  "examples/e8/conformance_pack/codegen/enum_codegen.aic"
  "examples/e8/roundtrip_random_seed.aic"
  "examples/e8/matrix_program.aic"
  "examples/e8/large_project_bench/bench03_effects_contracts.aic"
  "examples/e9/sandbox_smoke.aic"
  "examples/ops/observability_demo/main.aic"
  "examples/core/async_ping.aic"
  "examples/core/trait_sort.aic"
  "examples/core/trait_methods.aic"
  "examples/core/borrow_checker_completeness.aic"
  "examples/core/result_propagation.aic"
  "examples/core/mut_vec.aic"
  "examples/core/vec_capacity.aic"
  "examples/core/opt_levels_demo.aic"
  "examples/core/loop_control.aic"
  "examples/core/closure_fn_values.aic"
  "examples/core/leak_check_closure_capture.aic"
  "examples/core/tuple_types.aic"
  "examples/core/named_function_arguments.aic"
  "examples/core/struct_methods.aic"
  "examples/core/struct_defaults.aic"
  "examples/core/visibility_modifiers_demo"
  "examples/core/enum_methods_option_result.aic"
  "examples/core/cross_compile_targets.aic"
  "examples/core/pattern_or.aic"
  "examples/verify/range_proofs.aic"
  "examples/verify/qv_contract_proof_fixed.aic"
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

expect_run_exit_code() {
  local file="$1"
  local expected="$2"
  local binary="$ARTIFACT_DIR/$(basename "$file" .aic).exit_check"
  "${AIC[@]}" build "$file" -o "$binary" >/tmp/aic-example.out
  set +e
  "$binary" >/tmp/aic-example.out 2>/tmp/aic-example.err
  local got=$?
  set -e
  if [[ "$got" != "$expected" ]]; then
    echo "unexpected exit code for $file: expected '$expected' got '$got'" >&2
    cat /tmp/aic-example.out >&2 || true
    cat /tmp/aic-example.err >&2 || true
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

wasm_target_unavailable() {
  local err_file="$1"
  local stderr_lower
  stderr_lower="$(tr '[:upper:]' '[:lower:]' <"$err_file")"
  if [[ "$stderr_lower" != *"wasm32-unknown-unknown"* ]]; then
    return 1
  fi
  if [[ "$stderr_lower" == *"no available targets"* ]] || [[ "$stderr_lower" == *"unknown target"* ]] || [[ "$stderr_lower" == *"unable to create target"* ]] || [[ "$stderr_lower" == *"is not a valid target"* ]] || [[ "$stderr_lower" == *"unsupported option"* ]]; then
    return 0
  fi
  return 1
}

expect_wasm_magic() {
  local path="$1"
  python3 - "$path" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1])
b = p.read_bytes()
if len(b) < 4 or b[:4] != b"\x00asm":
    raise SystemExit(f"missing wasm magic bytes: {p}")
PY
}

expect_wasm_contains() {
  local path="$1"
  local symbol="$2"
  python3 - "$path" "$symbol" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1])
needle = sys.argv[2].encode("utf-8")
if needle not in p.read_bytes():
    raise SystemExit(f"expected symbol '{sys.argv[2]}' in {p}")
PY
}

validate_wasm_manifest() {
  local manifest="$1"
  local expected_output="$2"
  python3 - "$manifest" "$expected_output" <<'PY'
import json, pathlib, sys
data = json.loads(pathlib.Path(sys.argv[1]).read_text())
if data.get("target") != "wasm32":
    raise SystemExit(f"manifest target mismatch: {data.get('target')!r}")
if data.get("artifact_kind") != "exe":
    raise SystemExit(f"manifest artifact kind mismatch: {data.get('artifact_kind')!r}")
actual = pathlib.Path(data.get("output_path", "")).resolve()
expected = pathlib.Path(sys.argv[2]).resolve()
if actual != expected:
    raise SystemExit(f"manifest output path mismatch: {actual!s} != {expected!s}")
PY
}

run_wasm_with_node() {
  local wasm="$1"
  local harness="$ARTIFACT_DIR/wasm-node-runner.js"
  cat >"$harness" <<'JS'
const fs = require("fs");

async function main() {
  const bytes = fs.readFileSync(process.argv[2]);
  let printed = 0;
  const env = new Proxy(
    {
      aic_rt_print_str: () => { printed += 1; },
      aic_rt_print_int: () => { printed += 1; },
      aic_rt_env_set_args: () => {},
    },
    {
      get(target, prop) {
        if (!(prop in target)) {
          target[prop] = () => 0;
        }
        return target[prop];
      },
    },
  );
  const { instance } = await WebAssembly.instantiate(bytes, { env });
  const fn = instance.exports.aic_main || instance.exports.main;
  if (typeof fn !== "function") {
    throw new Error("missing exported entry function (aic_main/main)");
  }
  const result = fn();
  if (result !== 0 && result !== 0n) {
    throw new Error(`unexpected wasm return value: ${result}`);
  }
  if (printed === 0) {
    throw new Error("expected print import to be invoked");
  }
}

main().catch((err) => {
  console.error(err && err.stack ? err.stack : String(err));
  process.exit(1);
});
JS
  node "$harness" "$wasm" >/tmp/aic-example.out 2>/tmp/aic-example.err
}

run_pure_wasm_with_wasmtime() {
  local source="$ARTIFACT_DIR/wasm_pure_probe.aic"
  local wasm="$ARTIFACT_DIR/wasm_pure_probe.wasm"
  cat >"$source" <<'AIC'
fn main() -> Int {
    0
}
AIC
  if ! "${AIC[@]}" build "$source" --target wasm32 -o "$wasm" >/tmp/aic-example.out 2>/tmp/aic-example.err; then
    return 1
  fi
  wasmtime --invoke aic_main "$wasm" >/tmp/aic-example.out 2>/tmp/aic-example.err
}

validate_wasm_example() {
  local file="examples/interop/wasm_hello_world.aic"
  local wasm="$ARTIFACT_DIR/wasm_hello_world.wasm"
  local manifest="$ARTIFACT_DIR/wasm_hello_world.build.json"
  if ! "${AIC[@]}" build "$file" --target wasm32 -o "$wasm" --manifest "$manifest" >/tmp/aic-example.out 2>/tmp/aic-example.err; then
    if wasm_target_unavailable /tmp/aic-example.err; then
      echo "note: skipping wasm example validation (toolchain lacks wasm32 target support)" >&2
      return 0
    fi
    echo "wasm example build failed: $file" >&2
    cat /tmp/aic-example.out >&2 || true
    cat /tmp/aic-example.err >&2 || true
    exit 1
  fi

  expect_file_exists "$wasm"
  expect_file_exists "$manifest"
  expect_wasm_magic "$wasm"
  expect_wasm_contains "$wasm" "aic_rt_print_str"
  validate_wasm_manifest "$manifest" "$wasm"

  if command -v node >/dev/null 2>&1; then
    if run_wasm_with_node "$wasm"; then
      return 0
    fi
    echo "node wasm runtime validation failed for $file" >&2
    cat /tmp/aic-example.out >&2 || true
    cat /tmp/aic-example.err >&2 || true
    exit 1
  fi

  if command -v wasmtime >/dev/null 2>&1; then
    if run_pure_wasm_with_wasmtime; then
      return 0
    fi
    echo "wasmtime wasm runtime validation failed" >&2
    cat /tmp/aic-example.out >&2 || true
    cat /tmp/aic-example.err >&2 || true
    exit 1
  fi

  echo "note: wasm runtime engine unavailable; validated artifact bytes and imports only" >&2
}

case "$MODE" in
  check)
    for f in "${check_pass[@]}"; do
      "${AIC[@]}" check "$f" >/dev/null
    done
    for f in "${check_fail[@]}"; do
      expect_check_fail "$f"
    done
    validate_wasm_example
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
    expect_run_value "examples/io/fs_backup.aic" "42"
    expect_run_value "examples/io/fs_all_ops.aic" "42"
    expect_run_value "examples/io/stream_copy.aic" "42"
    expect_run_value "examples/io/error_handling.aic" "42"
    expect_run_value "examples/io/cli_file_pipeline.aic" "42"
    expect_run_exit_code "examples/io/process_pipeline.aic" "42"
    expect_run_value "examples/io/async_net_event_loop.aic" "42"
    expect_run_value "examples/io/async_await_submit_bridge.aic" "42"
    expect_run_value "examples/io/file_processor.aic" "42"
    expect_run_value "examples/io/log_tee.aic" "42"
    expect_run_value "examples/io/env_config.aic" "42"
    expect_run_value "examples/io/subprocess_pipeline.aic" "42"
    expect_run_value "examples/io/tcp_echo.aic" "42"
    expect_run_value "examples/io/tcp_echo_client.aic" "42"
    expect_run_value "examples/io/http_server_hello.aic" "42"
    expect_run_value "examples/io/http_router.aic" "42"
    expect_run_value "examples/io/retry_with_jitter.aic" "42"
    expect_run_value "examples/io/worker_pool.aic" "42"
    expect_run_value "examples/data/log_parse_regex.aic" "42"
    expect_run_value "examples/data/join_module_qualification.aic" "42"
    expect_run_value "examples/data/map_headers.aic" "42"
    expect_run_value "examples/data/deque_workloads.aic" "42"
    expect_run_value "examples/data/float_ops.aic" "42"
    expect_run_value "examples/data/config_json.aic" "42"
    expect_run_value "examples/data/serde_models.aic" "42"
    expect_run_value "examples/data/serde_negative_cases.aic" "42"
    expect_run_value "examples/data/string_ops.aic" "42"
    expect_run_value "examples/data/template_literals.aic" "42"
    expect_run_value "examples/data/http_types.aic" "42"
    expect_run_value "examples/data/audit_timestamps.aic" "42"
    expect_run_value "examples/data/ingest_transform_emit.aic" "42"
    expect_run_value "examples/data/data_stack_negative_cases.aic" "42"
    expect_run_value "examples/data/url_http_negative_cases.aic" "42"
    expect_run_value "examples/e6/deps_checksum.aic" "42"
    expect_run_value "examples/e6/pkg_app" "42"
    expect_run_value "examples/e7/cli_smoke.aic" "42"
    expect_run_value "examples/vscode/snippets_showcase.aic" "42"
    expect_run_value "examples/vscode/inlay_hints_demo.aic" "42"
    expect_run_value "examples/vscode/semantic_highlighting_showcase.aic" "42"
    expect_run_value "examples/vscode/status_bar_demo.aic" "42"
    expect_run_value "examples/vscode/symbol_outline.aic" "42"
    expect_run_value "examples/vscode/marketplace_packaging_demo.aic" "42"
    expect_run_value "examples/vscode/folding_selection_showcase.aic" "42"
    expect_run_value "examples/vscode/auto_import_workspace" "42"
    expect_run_value "examples/vscode/call_hierarchy_showcase.aic" "42"
    expect_run_value "examples/vscode/extension_test_suite_demo.aic" "42"
    expect_run_value "examples/agent/incremental_demo/app" "42"
    expect_run_value "examples/e8/conformance_pack/codegen/enum_codegen.aic" "42"
    expect_run_value "examples/e8/roundtrip_random_seed.aic" "42"
    expect_run_value "examples/e8/matrix_program.aic" "42"
    expect_run_value "examples/e8/large_project_bench/bench03_effects_contracts.aic" "42"
    expect_run_value "examples/e9/sandbox_smoke.aic" "42"
    expect_run_value "examples/ops/observability_demo/main.aic" "42"
    expect_run_value "examples/core/async_ping.aic" "42"
    expect_run_value "examples/core/trait_sort.aic" "42"
    expect_run_value "examples/core/trait_methods.aic" "42"
    expect_run_value "examples/core/borrow_checker_completeness.aic" "2"
    expect_run_value "examples/core/result_propagation.aic" "42"
    expect_run_value "examples/core/mut_vec.aic" "2"
    expect_run_value "examples/core/vec_capacity.aic" "42"
    expect_run_value "examples/core/opt_levels_demo.aic" "42"
    expect_run_value "examples/core/loop_control.aic" "42"
    expect_run_value "examples/core/closure_fn_values.aic" "23"
    expect_run_value "examples/core/leak_check_closure_capture.aic" "42"
    expect_run_value "examples/core/struct_defaults.aic" "9094"
    expect_run_value "examples/core/visibility_modifiers_demo" "42"
    expect_run_value "examples/core/tuple_types.aic" "42"
    expect_run_value "examples/core/named_function_arguments.aic" "42"
    expect_run_value "examples/core/struct_methods.aic" "42"
    expect_run_value "examples/core/enum_methods_option_result.aic" "42"
    expect_run_value "examples/core/cross_compile_targets.aic" "42"
    expect_run_value "examples/core/pattern_or.aic" "42"
    expect_run_value "examples/verify/range_proofs.aic" "9"
    expect_run_value "examples/verify/qv_contract_proof_fixed.aic" "7"
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
    MIGRATION_DIR="$ARTIFACT_DIR/migration_v1_to_v2"
    cp -R "examples/ops/migration_v1_to_v2" "$MIGRATION_DIR"
    "${AIC[@]}" migrate "$MIGRATION_DIR" --dry-run --json >"$ARTIFACT_DIR/migration_dry_run.json"
    python3 -m json.tool "$ARTIFACT_DIR/migration_dry_run.json" >/dev/null
    "${AIC[@]}" migrate "$MIGRATION_DIR" --report "$ARTIFACT_DIR/migration_report.json" >/dev/null
    python3 -m json.tool "$ARTIFACT_DIR/migration_report.json" >/dev/null
    "${AIC[@]}" check "$MIGRATION_DIR/src/main.aic" >/dev/null
    "${AIC[@]}" release manifest --root . --output "$ARTIFACT_DIR/repro-manifest.json" --source-date-epoch 1700000000 >/dev/null
    "${AIC[@]}" release sbom --root . --output "$ARTIFACT_DIR/sbom.json" --source-date-epoch 1700000000 >/dev/null
    "${AIC[@]}" release policy --check >/dev/null
    "${AIC[@]}" release lts --check >/dev/null
    "${AIC[@]}" release security-audit --json >"$ARTIFACT_DIR/security_audit.json"
    python3 -m json.tool "$ARTIFACT_DIR/security_audit.json" >/dev/null
    expect_build_artifact "examples/e5/object_link_main.aic" "obj" "$ARTIFACT_DIR/object_link_main.o"
    expect_build_artifact "examples/e5/object_link_main.aic" "lib" "$ARTIFACT_DIR/libobject_link_main.a"
    validate_wasm_example
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
