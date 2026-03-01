# AI Agent Implementation Guide (E4 + E5 + E6 + E7 + E8 + E9)

This document is implementation-oriented and intended for autonomous contributors working on compiler/runtime/package changes.

Core language-specific deep dive:

- `docs/core-language-1.0.md`
- `docs/ai-agent-rest-guide.md`

## Pipeline Touch Points

- Frontend orchestration: `src/driver.rs`
- Package loading and imports: `src/package_loader.rs`
- Manifest/lock/checksum/cache: `src/package_workflow.rs`
- Effects normalization + validation: `src/effects.rs`
- Type/effect checking + generic instantiation recording: `src/typecheck.rs`
- Contract static verification + runtime lowering: `src/contracts.rs`
- LLVM backend + runtime ABI: `src/codegen/mod.rs`
- API doc generation: `src/docgen.rs`
- Std compatibility/deprecation policy: `src/std_policy.rs`
- SARIF diagnostics export: `src/sarif.rs`
- Diagnostic explain metadata: `src/diagnostic_explain.rs`
- LSP server: `src/lsp.rs`
- Built-in fixture harness: `src/test_harness.rs`
- Conformance suite runner: `src/conformance.rs`
- Fuzzing engine + corpus replay: `src/fuzzing.rs`
- Differential roundtrip engine: `src/differential.rs`
- Cross-target execution matrix runner: `src/execution_matrix.rs`
- Performance budget runner: `src/perf_gate.rs`
- CLI command surface: `src/main.rs`

## Named Function Arguments (`[ERGO-T6]`)

- Parser contract: call sites support `name: value` in argument lists and carry labels via `arg_names: Vec<Option<String>>` on AST/IR call nodes.
- Typechecker contract (`src/typecheck.rs`):
  - validates named arguments against declared parameter names
  - enforces positional-then-named ordering (`E1092` on violations)
  - reports unknown/duplicate/missing named parameters with `E1213` and nearest-name suggestions
- Lowering contract: `src/driver.rs` applies `call_arg_orders` after typecheck so downstream passes/codegen receive parameter-order arguments.
- Current limitation: named arguments are rejected for method syntax and first-class `Fn(...)` values because parameter names are not preserved in those call paths.

```aic
fn connect(host: Int, port: Int, timeout_ms: Int, retry: Bool) -> Int {
    if retry { host + port + timeout_ms } else { 0 }
}

fn main() -> Int {
    connect(timeout_ms: 30, retry: true, host: 10, port: 2)
}
```

## Epic #62 proof-of-completion checklist (open)

Use this checklist to gate closure of epic `#62`. Keep epic status as In Progress until all items are complete with evidence.

- [ ] Protocol contract docs are current: `docs/agent-tooling/protocol-v1.md`
- [ ] Protocol schemas are current: `docs/agent-tooling/schemas/parse-response.schema.json`, `docs/agent-tooling/schemas/ast-response.schema.json`, `docs/agent-tooling/schemas/check-response.schema.json`, `docs/agent-tooling/schemas/build-response.schema.json`, `docs/agent-tooling/schemas/fix-response.schema.json`
- [ ] Incremental daemon docs are current: `docs/agent-tooling/incremental-daemon.md`
- [ ] LSP example reflects the implemented workflow: `examples/agent/lsp_workflow.json`
- [ ] Agent recipe docs are current and aligned: `docs/agent-recipes/`
- [ ] Test gate passes: `make test-e7`
- [ ] Relevant test files pass: `tests/agent_protocol_tests.rs`, `tests/agent_recipe_tests.rs`, `tests/lsp_smoke_tests.rs`, `tests/e7_cli_tests.rs`
- [ ] Epic closure evidence is posted: commit hash, `make test-e7` result, and touched docs/examples/tests

## E4 Summary (Effects + Contracts)

### Effects

- Canonical pass: `normalize_effect_declarations(program, file)` in `src/effects.rs`
- Known taxonomy: `io`, `fs`, `net`, `time`, `rand`, `env`, `proc`, `concurrency`
- Diagnostics:
  - unknown effect: `E2003`
  - duplicate declaration: `E2004`
  - direct undeclared use: `E2001`
  - transitive missing effect path: `E2005`

### Contracts

- Static verifier: `verify_static(program, file)` in `src/contracts.rs`
- Runtime lowering: `lower_runtime_asserts(program)`
- Guarantees:
  - `requires` checks at function entry
  - `ensures` checks on every explicit return and implicit tail return
  - struct invariants via synthesized helper constructors (`__aic_invariant_ctor_*`)

## E5 Summary (LLVM Backend + Runtime ABI)

### Toolchain contract

In `src/codegen/mod.rs`:

- `probe_toolchain()` inspects `clang --version`
- `MIN_SUPPORTED_LLVM_MAJOR = 14`
- optional pin `AIC_LLVM_PIN_MAJOR`
- deterministic actionable errors for missing/unsupported/mismatched toolchains

### ADT lowering and generics

- Struct: LLVM aggregate in field order
- Enum: `{ i32 tag, payload-slots... }`
- Built-in enum templates: `Option[T]`, `Result[T, E]`
- Generic monomorphization emitted from frontend instantiation metadata with stable symbols

### Runtime ABI and debug mapping

- `String` lowered as `{ i8*, i64, i64 }` (ptr-len-cap)
- Runtime map entry storage applies small-string optimization (SSO) for map string fields:
  - inline threshold: `<= 23` bytes
  - larger strings remain heap-backed
  - rollout scope is runtime map storage (`AicMapEntryStorage`) without language-level API changes
  - benchmark toggle: `AIC_RT_DISABLE_MAP_SSO=1` forces heap-only storage path
- runtime panic ABI: `aic_rt_panic(ptr, len, cap, line, column)`
- `aic build --debug-info` emits debug metadata and source-mapped panic locations
- `aic build --opt-level <LEVEL>` accepts `0..3` (`O0..O3`)
- `aic build --release` defaults optimization to `O2` unless overridden with `--opt-level`/`-O`

### Artifact modes

- `aic build --artifact exe|obj|lib`

### WebAssembly target (`[INTEROP-T2]`)

- CLI target: `aic build --target wasm32 <input.aic>`
- clang triple: `wasm32-unknown-unknown`
- output naming: default executable artifact uses `.wasm` extension
- current wasm constraints:
  - wasm target supports `--artifact exe` only
  - workspace builds with `--target wasm32` are rejected
  - `--static-link` is rejected for wasm target
- backend/linker behavior:
  - no runtime C shim or libc requirement (`-nostdlib`)
  - runtime/IO calls are left unresolved intentionally and bound as host imports (`--allow-undefined`)
  - exported functions include `main` and `aic_main`
  - wasm entry wrapper removes argv/env initialization bridge so pure programs do not force `aic_rt_env_set_args` imports
- validation references:
  - `tests/e7_build_hermetic_tests.rs`
  - `examples/interop/wasm_hello_world.aic`
  - `scripts/ci/examples.sh` (node/wasmtime validation when available, byte/import fallback otherwise)

## E6 Summary (Std + Package Ecosystem)

### Standard library modules (E6-T1)

Current std set under `std/`:

- `io`, `fs`, `env`, `config`, `path`, `proc`, `net`, `time`, `rand`, `crypto`, `json`, `url`, `http`, `regex`, `concurrent`, `string`, `vec`, `option`, `result`

Notes:

- Effects are declared on side-effecting std APIs.
- `std.time.now` is compatibility API and intentionally deprecated in policy metadata.
- `std.fs` public APIs delegate to runtime intrinsic wrappers (`aic_fs_*_intrinsic`) and should not be replaced with constant placeholders.
- `std.config` composes file + JSON + env capabilities for app config loading (`load_json`, `load_env_prefix`, `get_or_default`, `require`).
- `std.set` mutator/query APIs are `add`, `has`, and `discard`; `union`/`intersection`/`difference` preserve deterministic ordering via `to_vec`.
- `std.vec` capacity APIs are production-facing: `new_vec_with_capacity`, `reserve`, `shrink_to_fit`; runtime growth remains 2x and capacity behavior is exercised in `tests/execution_tests.rs` and `examples/core/vec_capacity.aic`.
- `std.crypto` provides runtime-backed primitives for MD5/SHA-256/HMAC/PBKDF2 plus hex/base64 encoding, secure random bytes, and constant-time byte comparison.
- `std.crypto` runtime error mapping is stable (`InvalidInput`, `UnsupportedAlgorithm`, `Internal`) and exercised by positive + negative execution tests.
- `std.option` and `std.result` expose inherent enum methods:
  - `Option.unwrap_or`, `Option.map`, `Option.and_then`
  - `Result.unwrap_or`, `Result.map`, `Result.and_then`
  - method chains use standard static method dispatch (`value.map(...).and_then(...).unwrap_or(...)`)
- Current backend support remains `String`-key specialized for set/map key paths; non-`String` set key usage currently emits deterministic backend diagnostic `E5011` (`...String key...`).
- `std.fs` now exposes production-facing APIs with typed failures:
  - `read_text`, `write_text`, `append_text`, `copy`, `move`, `delete`
  - `metadata`, `walk_dir`, `temp_file`, `temp_dir`
  - stable error enum `FsError` and metadata struct `FsMetadata`
- Filesystem API contract and examples:
  - `docs/io-filesystem.md`
  - `examples/io/fs_backup.aic`
  - `examples/io/fs_all_ops.aic`
- Process/env/path API contract and examples:
  - `docs/io-process-env-path.md`
  - `examples/io/process_pipeline.aic`
- Config loading contract and example:
  - `docs/config-loading.md`
  - `examples/io/config_loading.aic`
- Concurrency runtime contract and examples:
  - `docs/io-concurrency-runtime.md`
  - `examples/io/worker_pool.aic`
  - `examples/io/channel_migration_compat.aic`
  - `examples/io/generic_channel_types.aic`
  - `examples/io/structured_concurrency.aic`
  - structured APIs: `spawn_group(Vec[Int], Int)`, `timeout_task(Task, Int)`, `select_first(Vec[Task], Int)`
- Complete IO agent playbooks:
  - `docs/io-runtime/README.md`
  - `docs/io-runtime/net-time-rand.md`
  - `docs/io-runtime/error-model.md`
  - `docs/io-runtime/lifecycle-playbook.md`
- Issue #123 IO reference set:
  - `docs/io-api-reference.md`
  - `docs/io-cookbook.md`
  - `docs/io-agent-guide.md`
  - `docs/io-migration.md`
  - `examples/io/interactive_greeter.aic`
  - `examples/io/file_processor.aic`
  - `examples/io/log_tee.aic`
  - `examples/io/env_config.aic`
  - `examples/io/subprocess_pipeline.aic`
- Data/text/regex/url/http/datetime contracts and examples:
  - `docs/data-text/README.md`
  - `docs/data-text/cookbook.md`
  - `docs/data-regex.md`
  - `examples/data/log_parse_regex.aic`
  - `examples/data/config_json.aic`
  - `examples/data/serde_models.aic`
  - `examples/data/serde_negative_cases.aic`
  - `examples/data/http_types.aic`
  - `examples/data/audit_timestamps.aic`
  - `examples/data/ingest_transform_emit.aic`
  - `examples/data/data_stack_negative_cases.aic`
  - `examples/data/url_http_negative_cases.aic`
- Crypto contract and example:
  - `docs/io-api-reference.md` (`std.crypto` section)
  - `docs/io-cookbook.md` (`Crypto Patterns`)
  - `examples/crypto/pg_scram_auth.aic`

### Enum method chaining example (Option/Result)

```aic
import std.option;
import std.result;

fn add_one(x: Int) -> Int { x + 1 }
fn keep_even_option(x: Int) -> Option[Int] { if x % 2 == 0 { Some(x) } else { None() } }
fn keep_even_result(x: Int) -> Result[Int, Int] { if x % 2 == 0 { Ok(x) } else { Err(0 - x) } }

fn demo() -> Int {
    let opt = Some(41).map(add_one).and_then(keep_even_option).unwrap_or(0);
    let res = Ok(3).map(add_one).and_then(keep_even_result).unwrap_or(0);
    opt + res
}
```

### Manifest + lockfile workflow (E6-T2)

`src/package_workflow.rs`:

- Parses `aic.toml` package/dependency metadata.
- Generates deterministic `aic.lock` via `aic lock`.
- Lockfile entries include dependency name, resolved path, checksum.
- Build/check/run consume lockfile through package loader when present.

Diagnostics:

- `E2106`: lockfile drift (`aic.toml` vs `aic.lock`).

### Native dependency bridge (PKG-T3)

`src/package_workflow.rs`, `src/typecheck.rs`, `src/codegen/mod.rs`, `src/main.rs`, and `src/driver.rs` now form the FFI bridge pipeline.

- Frontend syntax:
  - `extern "C" fn ...;`
  - `unsafe fn ...`
  - expression `unsafe { ... }`
- Typechecker/diagnostic contract:
  - `E2120`: missing/unsupported extern ABI
  - `E2121`: extern signature must be plain (no async/generics/effects/contracts)
  - `E2122`: calling `extern` or `unsafe fn` requires an explicit unsafe boundary
  - `E2123`: unsupported C-ABI signature type (MVP support: `Int`, `Bool`, `()`)
- Backend contract:
  - extern declarations lower to wrapper functions + raw native symbol declarations
  - backend emits `E5024` for unsupported extern ABI/lowering mismatches
- Manifest native linkage:
  - `aic.toml` `[native]` fields: `libs`, `search_paths` (or `search`), `objects`
  - CLI build/run resolves relative paths from project root and passes `-L`, `-l`, and object paths to the linker

Implementation and test references:

- `tests/unit_tests.rs` (ABI/unsafe diagnostics)
- `tests/e7_cli_tests.rs` (`build_links_native_c_library_from_manifest_native_section`)
- `examples/pkg/ffi_zlib.aic`


### Intrinsic declarations (AGX1-T1)

`src/lexer.rs`, `src/parser.rs`, `src/ir.rs`, `src/formatter.rs`, and `src/typecheck.rs` model intrinsic runtime bindings directly.

- Surface syntax: `intrinsic fn ... -> ... effects { ... };`
- Intrinsic declarations are signature-only; parser rejects bodies/contracts/generics with `E1093`.
- Canonical IR/JSON carries explicit metadata: `is_intrinsic` and `intrinsic_abi`.

Example source:

```aic
intrinsic fn aic_fs_exists_intrinsic(path: String) -> Bool effects { fs };

fn exists(path: String) -> Bool effects { fs } {
    aic_fs_exists_intrinsic(path)
}
```

Canonical IR/JSON excerpt:

```json
{
  "name": "aic_fs_exists_intrinsic",
  "is_intrinsic": true,
  "intrinsic_abi": "runtime"
}
```

Additional implementation guidance, troubleshooting, and runnable examples:

- `docs/intrinsics-runtime-bindings.md`
- `examples/core/intrinsic_declaration_demo.aic`
- `examples/core/intrinsic_declaration_invalid_body.aic`

### Intrinsic binding verifier (AGX1-T3)

`aic verify-intrinsics [INPUT] --json` validates intrinsic declarations against backend lowering expectations before release.

- verifies runtime ABI metadata (`intrinsic_abi = runtime`)
- verifies every intrinsic has a known backend lowering mapping
- verifies declaration signatures against accepted backend shapes and runtime symbol mapping

Deterministic JSON fields:

- `schema_version`, `input`, `files_scanned`, `intrinsic_declarations`, `verified_bindings`, `issue_count`, `ok`
- `issues[]` with stable kinds: `parse_diagnostic`, `unsupported_abi`, `missing_lowering`, `signature_mismatch`, `missing_runtime_symbol`

Examples:

```bash
aic verify-intrinsics std --json
aic verify-intrinsics examples/verify/intrinsics/invalid_bindings.aic --json
make intrinsic-placeholder-guard
```

`make intrinsic-placeholder-guard` enforces that AGX1 runtime-bound std intrinsics remain declaration-only in policy modules (`std/concurrent.aic`, `std/net.aic`, `std/proc.aic`).

### Registry provenance and trust policy (PKG-T4)

`src/package_registry.rs` extends package install security with signature metadata and trust-policy gating.

- Registry release metadata now supports optional:
  - `signature`
  - `signature_alg`
  - `signature_key_id`
- Publish signing:
  - if `AIC_PKG_SIGNING_KEY` is set, publish emits deterministic HMAC-SHA256 signatures over `package/version/checksum`.
  - key id uses `AIC_PKG_SIGNING_KEY_ID` (default `default`).
- Install trust policy:
  - policy config is per registry entry (`aic.registry.json -> registries.<alias>.trust`).
  - supports:
    - `default`: `allow` or `deny`
    - `allow`: package patterns (`prefix*` or exact)
    - `deny`: package patterns (`prefix*` or exact)
    - `require_signed`: global signature requirement
    - `require_signed_for`: package-pattern signature requirement
    - `trusted_keys`: `key_id -> env var name` mapping for verification keys
- Security diagnostics:
  - `E2119`: trust policy denied install.
  - `E2124`: signature verification or trusted-key configuration failure.
- Auditability:
  - `InstallResult` JSON now includes `audit` records (`decision`, reason, checksum/signature verification, key id).

Implementation and test references:

- `src/package_registry.rs`
- `tests/e7_cli_tests.rs` (`pkg_trust_policy_enforces_signatures_and_emits_audit_records`)
- `examples/pkg/policy_enforced_project/`

### Monorepo workspace support (PKG-T5)

`src/package_workflow.rs` now models workspace package graphs, shared lockfiles, and deterministic build ordering.

- Workspace root manifest:
  - `aic.workspace.toml` with `[workspace].members = [ ... ]`
- Deterministic graph/order:
  - workspace package dependencies are inferred from member manifests
  - build order is topological with lexical tie-breaks for determinism
- Shared lockfile:
  - `aic lock` on a workspace root (or a member package) writes one shared `aic.lock` at workspace root
  - lockfile includes per-member dependency path metadata for scoped resolution
- Workspace diagnostics:
  - `E2125`: invalid workspace manifest/member metadata
  - `E2126`: workspace package dependency cycle
- CLI integration:
  - `aic check <workspace-root>` runs all members in deterministic order
  - `aic build <workspace-root>` emits artifacts to `target/workspace/<package>/`
  - workspace build fingerprints skip unchanged members (`up-to-date`)

Implementation and test references:

- `src/package_workflow.rs`
- `src/main.rs`
- `tests/e7_cli_tests.rs` (`workspace_check_and_build_execute_in_deterministic_order`, `workspace_cycle_is_reported_as_diagnostic`, `workspace_build_is_incremental_for_unchanged_members`)
- `examples/pkg/workspace_demo/`

### Agent-grade package ecosystem documentation (PKG-T6)

`docs/package-ecosystem/` now provides end-to-end machine-first runbooks for PKG-T1..PKG-T5.

- `README.md`:
  - capability map and deterministic contract summary
  - canonical runnable examples list
- `publish-consume.md`:
  - public publish/search/install flow
  - private registry scopes/auth/mirror flow
  - expected deterministic failure codes (`E2114`, `E2117`, `E2118`)
- `workspaces-and-locks.md`:
  - workspace manifest model
  - shared lockfile semantics and offline behavior
  - workspace diagnostics (`E2126`, `E2108`, `E2109`)
- `ffi-and-supply-chain.md`:
  - extern/native linking safety rules and diagnostics
  - trust/signature verification workflow (`E2119`, `E2124`)
- `failure-playbooks.md`:
  - resolver conflict, auth/config, provenance, lock/cache incident response steps

Validation hooks:

- `Makefile` `docs-check` now requires the package ecosystem doc set files.
- `tests/e7_cli_tests.rs` + `scripts/ci/examples.sh` exercise the documented package examples and flows.

### Checksum verification + offline cache (E6-T3)

`src/package_workflow.rs`:

- Computes deterministic package checksum from `aic.toml` + `.aic` files.
- Verifies dependency checksums before compilation.
- Maintains `.aic-cache/` copies for offline builds.
- Supports offline mode via CLI flag `--offline` on check/build/run/ir/diag.

Diagnostics:

- `E2107`: dependency checksum/source mismatch.
- `E2108`: missing offline lock/cache prerequisites.
- `E2109`: corrupted offline cache entry.

### API doc command (E6-T4)

`src/docgen.rs` + CLI `aic doc`:

- Emits deterministic docs to output directory (default `docs/api`).
- Produces:
  - `index.html`
  - `index.md`
  - `api.json`
- Includes signatures, effects, contracts, invariants, and deprecation metadata.

### Compatibility + deprecation policy (E6-T5)

`src/std_policy.rs`:

- Deprecated APIs declared in static policy table.
- Typecheck emits warning diagnostics for deprecated API usage.
  - `E6001` warning with replacement guidance.
- Compatibility baseline:
  - `docs/std-api-baseline.json`
- Policy lint command:
  - `aic std-compat --check --baseline docs/std-api-baseline.json`

Diagnostics:

- `E6002`: std compatibility check failure (CLI policy lint output).

## E7 Summary (CLI + Diagnostics + IDE Tooling)

### CLI contract and deterministic exits (E7-T1)

- Command/flag/exit contract metadata: `src/cli_contract.rs`
- CLI contract command: `aic contract --json [--accept-version <v1,v2,...>]`
- Agent protocol schemas:
  - `docs/agent-tooling/schemas/parse-response.schema.json`
  - `docs/agent-tooling/schemas/ast-response.schema.json`
  - `docs/agent-tooling/schemas/check-response.schema.json`
  - `docs/agent-tooling/schemas/build-response.schema.json`
  - `docs/agent-tooling/schemas/fix-response.schema.json`
- Protocol guide:
  - `docs/agent-tooling/protocol-v1.md`
- Protocol fixtures:
  - `examples/agent/protocol_parse.json`
  - `examples/agent/protocol_ast.md`
  - `examples/agent/protocol_check.json`
  - `examples/agent/protocol_build.json`
  - `examples/agent/protocol_fix.json`
- Autofix engine:
  - planner/applicator in `src/diag_fixes.rs`
  - command: `aic diag apply-fixes <path> [--dry-run] [--json] [--offline]`
- Effect suggestion engine:
  - analyzer in `src/suggest_effects.rs`
  - command: `aic suggest-effects <path> [--offline]`
- Exit code mapping:
  - `0`: success
  - `1`: diagnostic/runtime failure
  - `2`: command-line usage error
  - `3`: internal/tooling failure
- Contract doc: `docs/cli-contract.md`

### SARIF export (E7-T2)

- SARIF emitter: `diagnostics_to_sarif(diags, tool_name, tool_version)` in `src/sarif.rs`
- CLI support:
  - `aic check --sarif`
  - `aic diag --sarif`
- SARIF docs:
  - `docs/sarif.md`

### Explain command (E7-T3)

- Explanation engine: `src/diagnostic_explain.rs`
- Commands:
  - `aic explain E####`
  - `aic explain E#### --json`
- Coverage guarantee:
  - `registry_explain_coverage()` test ensures all registered codes have explain metadata/range mapping.

### LSP support and IDE integration (E7-T4)

- Server entrypoint: `aic lsp` (`src/lsp.rs`)
- Implemented methods:
  - `initialize`, `shutdown`
  - `textDocument/didOpen`, `textDocument/didChange`, `textDocument/didSave`
  - `textDocument/hover`, `textDocument/definition`, `textDocument/formatting`
  - `textDocument/completion`, `textDocument/rename`
  - `textDocument/codeAction`, `textDocument/semanticTokens/full`
- Doc comment pipeline (`[DOC-T2]`):
  - symbol indexing extracts contiguous `///` blocks above declarations
  - hover renders fenced `aic` signatures and markdown docs
  - completion emits summary in `detail` and full markdown in `documentation`
  - coverage includes functions, structs, enums, traits, and enum variants
- Diagnostics parity:
  - LSP diagnostics are built from frontend diagnostics and filtered by file.
- Autofix integration:
  - code actions are emitted from diagnostic `suggested_fixes` with deterministic quick-fix edits
  - missing effect declaration diagnostics (`E2001`, `E2005`) carry deterministic effect-clause edits
- IDE docs:
  - `docs/ide-integration.md`
- Sample workspace:
  - `examples/e7/lsp_project/`
  - `examples/agent/lsp_workspace/`
  - `examples/vscode/doc_hover_completion_showcase.aic`

### Debug adapter bridge (VSCODE-T9)

- CLI bridge command: `aic debug dap`
  - resolves backend in order: `--adapter`, `AIC_DEBUG_ADAPTER`, `lldb-dap`, `lldb-vscode`
  - forwards stdio directly to backend for DAP launch/step/breakpoint/variables/stack operations
- VSCode extension integration:
  - debug type contribution: `aic`
  - command: `aic.debug.createLaunchJson`
  - launch resolution auto-builds `.aic` targets with `aic build --debug-info`
  - optional `breakOnContractViolation` injects startup breakpoint at `aic_rt_panic`
- Example:
  - `examples/vscode/debugger_launch_demo.aic`

### Built-in fixture harness (E7-T5)

- Harness modules:
  - `src/test_harness.rs`
  - `src/attr_test_runner.rs`
  - `src/property_test_runner.rs`
- Command:
  - `aic test [path] --mode all|run-pass|compile-fail|golden [--filter <pattern>] [--seed <n>] [--replay <id|artifact>] [--json]`
- Fixture categories discovered by directory segment:
  - `run-pass`
  - `compile-fail`
  - `golden`
- Attribute-based test discovery (mode `all`):
  - `#[test]` marks runnable tests
  - `#[should_panic]` marks expected-failure tests
  - helper assertions available in test files: `assert(...)`, `assert_eq(...)`, `assert_ne(...)`
- Property-based test discovery (mode `all`):
  - `#[property]` runs with default deterministic iterations
  - `#[property(iterations = N)]` overrides iteration count per property
  - supported generated input types: `Int`, `Float`, `Bool`, `String`, `Vec[T]`, `Option[T]`
  - failing runs report seed + counterexample and perform shrinking for smaller repro inputs
- Effect mocking support (mode `all`):
  - `std.io` exposes `MockReader` / `MockWriter` helpers (`mock_reader_from_lines`, `install_mock_reader`, `mock_writer_take`)
  - runtime IO calls are interceptable in tests via mock APIs and deterministic env controls
  - attribute/property test subprocesses default to deterministic/isolated IO via `AIC_TEST_NO_REAL_IO=1` and `AIC_TEST_IO_CAPTURE=1` (overridable by caller env)
  - when `AIC_TEST_NO_REAL_IO=1`, accidental real `fs`/`net`/`proc` calls are rejected with structured `sandbox_policy_violation` diagnostics
- Replay metadata and deterministic rerun:
  - failing `aic test --json` runs emit `replay` metadata (`replay_id`, artifact path, seed/time/mock/trace context)
  - replay artifacts persist under `.aic-replay/`
  - `aic test --replay <id|artifact>` replays with captured deterministic context
- CI/automation output:
  - JSON mode emits machine-readable report to stdout
  - attribute/property runs persist `test_results.json` at the selected test root
- Sample fixtures:
  - `examples/e7/harness/`
  - `examples/e7/test_framework/`
  - `examples/e7/property_framework/`
  - `examples/test/mock_io.aic`
  - `examples/test/replay_failure.aic`
  - `examples/test/mock_isolation_violation.aic`

### Deterministic incremental daemon (AG-T4)

- Server entrypoint: `aic daemon` (`src/daemon.rs`)
- Transport: line-delimited JSON-RPC 2.0 over stdio.
- Implemented methods:
  - `check`
  - `build`
  - `stats`
  - `shutdown`
- Cache strategy:
  - frontend cache keyed by canonical input + `offline` + content fingerprint
  - build cache keyed by canonical input + output + artifact + `debug_info` + content fingerprint
- Invalidation:
  - fingerprints include package checksum + resolved dependency checksums + dependency-context markers
  - dependency edits force deterministic cache misses
- Determinism verification:
  - build responses include `output_sha256`
  - warm/cold parity tests compare artifact digests
- Docs:
  - `docs/agent-tooling/incremental-daemon.md`
- Example:
  - `examples/agent/incremental_demo/`

### Agent cookbook and task recipes (AG-T5)

- Recipe docs:
  - `docs/agent-recipes/README.md`
  - `docs/agent-recipes/feature-loop.md`
  - `docs/agent-recipes/bugfix-loop.md`
  - `docs/agent-recipes/refactor-loop.md`
  - `docs/agent-recipes/diagnostics-loop.md`
- Coverage:
  - feature delivery loop
  - bugfix/autofix loop
  - deterministic refactor loop
  - diagnostics triage loop
- Recipe quality gates:
  - each recipe includes protocol fixture references and fallback behavior
  - docs-as-tests execute command blocks between recipe markers
- Validation tests:
  - `tests/agent_recipe_tests.rs`
  - `make test-e7` includes recipe docs-as-tests execution

### Agent-grade tooling documentation (AG-T6)

- Tooling index:
  - `docs/agent-tooling/README.md`
- Versioned protocol reference:
  - `docs/agent-tooling/protocol-v1.md`
- Positive fixtures:
  - `examples/agent/protocol_parse.json`
  - `examples/agent/protocol_check.json`
  - `examples/agent/protocol_build.json`
  - `examples/agent/protocol_fix.json`
- Negative fixtures:
  - `examples/agent/protocol_parse_error.json`
  - `examples/agent/protocol_build_error.json`
  - `examples/agent/protocol_fix_conflict.json`
- LSP request/response workflow examples:
  - `examples/agent/lsp_workflow.json`
- Validation gates:
  - `tests/agent_protocol_tests.rs` validates schemas, fixtures, and docs references
  - `make test-e7` includes agent protocol + recipe docs-as-tests suites

## E8 Summary (Verification + Fuzzing + Performance Gates)

### Conformance suites (E8-T1)

- Conformance catalog (expected behavior source of truth):
  - `examples/e8/conformance_pack/catalog.json`
- Categories:
  - `syntax`
  - `typing`
  - `diagnostics`
  - `codegen`
- Runner API:
  - `load_catalog(path)` and `run_catalog(root, catalog)` in `src/conformance.rs`
- Test:
  - `tests/e8_conformance_tests.rs`

### Lexer/parser/typechecker fuzzing (E8-T2)

- Engine:
  - `src/fuzzing.rs`
- Targets:
  - `FuzzTarget::Lexer`
  - `FuzzTarget::Parser`
  - `FuzzTarget::Typecheck`
- Seed corpus:
  - `tests/fuzz/corpus/*`
- Regression replay corpus:
  - `tests/fuzz/regressions/*`
- Nightly stress gate:
  - ignored test in `tests/e8_fuzz_tests.rs`
  - workflow `.github/workflows/nightly-fuzz.yml`

### Differential roundtrip validation (E8-T3)

- Engine:
  - `src/differential.rs`
- Flow:
  - parse -> IR build -> format -> parse -> IR build -> semantic snapshot compare
- Report includes first divergence line when mismatch exists.
- Tests:
  - `tests/e8_differential_tests.rs`
- Reference file:
  - `examples/e8/roundtrip_random_seed.aic`

### Execution matrix across targets (E8-T4)

- Matrix definition:
  - `examples/e8/execution-matrix.json`
- Runner:
  - `run_host_matrix(root, matrix)` in `src/execution_matrix.rs`
- Modes:
  - `debug`
  - `release`
- CI:
  - dedicated `execution-matrix` job in `.github/workflows/ci.yml` (Linux + macOS).
- Platform policy:
  - Windows target is explicitly marked build-only (`execute=false`) with a documented note.

### Performance budgets (E8-T5)

- Budget policy:
  - `benchmarks/service_baseline/budget.v1.json`
- Target baselines:
  - `benchmarks/service_baseline/baselines.v1.json`
- Dataset stability lock:
  - `benchmarks/service_baseline/dataset-fingerprint.txt`
- Runner:
  - `run_perf_gate(...)` and `build_trend_report(...)` in `src/perf_gate.rs`
- Report artifact:
  - `target/e8/perf-report.json`
  - `target/e8/perf-report-<target>.json`
  - `target/e8/perf-trend-<target>.json`
- CI:
  - `make test-e8` in Linux full validation job.
  - per-target perf gate run in `execution-matrix` job (Linux + macOS) with perf artifacts uploaded.

### Deterministic concurrency stress/replay (AGX3-T3)

- Plan:
  - `examples/e8/concurrency-stress-plan.json`
- Gate:
  - `tests/e8_concurrency_stress_tests.rs`
- Coverage:
  - `examples/io/worker_pool.aic`
  - `examples/io/structured_concurrency.aic`
  - `examples/io/generic_channel_types.aic`
- Artifacts:
  - `target/e8/concurrency-stress-report.json`
  - `target/e8/concurrency-stress-schedule.json`
  - `target/e8/concurrency-stress-replay.txt`
- Replay control:
  - `AIC_CONC_STRESS_REPLAY=<seed>:<round>:<case_id>`

### Verification-quality runbooks (QV-T6)

Agent-grade docs for all QV gates:

- `docs/verification-quality/README.md`
- `docs/verification-quality/contracts-proof-obligations.md`
- `docs/verification-quality/effect-protocols.md`
- `docs/capability-protocols.md`
- `docs/verification-quality/fuzz-differential-runbook.md`
- `docs/verification-quality/concurrency-stress-replay.md`
- `docs/verification-quality/perf-sla-playbook.md`
- `docs/verification-quality/incident-reproduction.md`

Verifier-focused examples:

- `examples/verify/qv_contract_proof_fail.aic`
- `examples/verify/qv_contract_proof_fixed.aic`
- `examples/verify/capability_protocol_ok.aic`
- `examples/verify/capability_missing_invalid.aic`
- `examples/verify/fs_protocol_ok.aic`
- `examples/verify/fs_protocol_invalid.aic`
- `examples/verify/file_protocol.aic`
- `examples/verify/file_protocol_invalid.aic`
- `examples/verify/net_proc_protocol_ok.aic`
- `examples/verify/net_proc_protocol_invalid.aic`

## E9 Summary (Release Security + Operations)

### Reproducibility pipeline (E9-T1)

- Core module:
  - `src/release_ops.rs`
- Commands:
  - `aic release manifest`
  - `aic release verify-manifest`
- Local CI:
  - `scripts/ci/repro-build-check.sh`
  - `make repro-check`

### Release automation (E9-T2)

- Workflow:
  - `.github/workflows/release.yml`
- CI integration:
  - `.github/workflows/ci.yml` includes E9 checks in Linux full validation.
- Local preflight:
  - `make release-preflight`

### SBOM and provenance (E9-T3)

- Commands:
  - `aic release sbom`
  - `aic release provenance`
  - `aic release verify-provenance`
- Data model:
  - `SbomDocument` and `ProvenanceStatement` in `src/release_ops.rs`

### Security audit and threat model (E9-T4)

- Threat model:
  - `docs/security-threat-model.md`
- Audit command:
  - `aic release security-audit --json`
- Scripted gate:
  - `scripts/ci/security-audit.sh`
  - `make security-audit`

### Sandboxed resource limits (E9-T5)

- Runtime limits module:
  - `src/sandbox.rs`
- CLI:
  - `aic run ... --sandbox none|ci|strict`
- Linux implementation:
  - `prlimit` wrapper in `run_with_limits(...)`

### Compatibility and migration policy (E9-T6)

- Policy doc:
  - `docs/compatibility-migration-policy.md`
- Migration runbook:
  - `docs/security-ops/migration.md`
- LTS docs:
  - `docs/release/lts-policy.md`
  - `docs/release/compatibility-matrix.json`
- OPS runbooks:
  - `docs/security-ops/README.md`
  - `docs/security-ops/release-runbook.md`
  - `docs/security-ops/sandbox-operations.md`
  - `docs/security-ops/telemetry.md`
  - `docs/security-ops/migration.md`
  - `docs/security-ops/incident-response.md`
- CLI:
  - `aic migrate <path> --dry-run --json`
  - `aic migrate <path> --report <file>`
  - `aic release policy --check`
  - `aic release policy --check --json`
  - `aic release lts --check`
  - `aic release lts --check --json`
- Policy model:
  - `CompatibilityPolicy` in `src/release_ops.rs`
  - `LtsPolicy` in `src/release_ops.rs`

## Connectivity Primitive: Bitwise Operators (CONN-T1)

- Language surface:
  - Binary: `&`, `|`, `^`, `<<`, `>>`, `>>>`
  - Unary: `~`
  - Compound assignment: `&=`, `|=`, `^=`, `<<=`, `>>=`, `>>>=`
  - Hex literals: `0x...` (`Int`)
- Type rules:
  - All bitwise/shift operators require `Int` operands and produce `Int`.
  - `>>` is arithmetic right shift; `>>>` is logical right shift.
  - Bool misuse diagnostics suggest `&&` / `||`.
- Implementation files:
  - `src/lexer.rs`
  - `src/parser.rs`
  - `src/typecheck.rs`
  - `src/codegen/mod.rs`
- Verification:
  - `src/lexer.rs` parser/lexer unit tests include hex + bitwise token coverage.
  - `src/parser.rs` tests cover precedence and compound assignment desugaring.
  - `src/typecheck.rs` tests cover accepted `Int` paths and rejected `Bool` misuse.
  - `tests/execution_tests.rs` includes runtime validation of all operators.
- Example:
  - `examples/data/bitwise_protocol.aic`

## Connectivity Runtime: Real TCP/UDP/DNS Intrinsics (CONN-T2)

- Runtime ABI surface (POSIX Linux/macOS):
  - `aic_rt_net_tcp_listen`
  - `aic_rt_net_tcp_local_addr`
  - `aic_rt_net_tcp_accept`
  - `aic_rt_net_tcp_connect`
  - `aic_rt_net_tcp_send`
  - `aic_rt_net_tcp_recv`
  - `aic_rt_net_tcp_close`
  - `aic_rt_net_udp_bind`
  - `aic_rt_net_udp_local_addr`
  - `aic_rt_net_udp_send_to`
  - `aic_rt_net_udp_recv_from`
  - `aic_rt_net_udp_close`
  - `aic_rt_net_dns_lookup`
  - `aic_rt_net_dns_reverse`
- Error model:
  - `errno`/`getaddrinfo` mapping to `NetError` is deterministic and shared by sync/async paths.
  - Timeouts are explicit (`Timeout`), bind conflicts map to `AddressInUse`, and refused connects map to `Refused`.
- Verification:
  - `tests/execution_tests.rs` includes TCP loopback, UDP/DNS helpers, timeout/invalid-input diagnostics, and refused/address-in-use stability coverage.
  - async bridge tests validate `async_accept_submit`, `async_tcp_send_submit`, `async_tcp_recv_submit`, `async_wait_*`, and `async_shutdown`.
- Examples:
  - `examples/io/tcp_echo.aic`
  - `examples/io/tcp_echo_client.aic`
  - `examples/io/net_all_ops.aic`

## Connectivity Binary Protocol Buffer (CONN-T5)

- Module surface:
  - `std/buffer.aic`
  - `BufferError` variants: `Underflow`, `Overflow`, `InvalidUtf8`, `InvalidInput`
  - Cursor APIs: `buf_position`, `buf_remaining`, `buf_seek`, `buf_reset`
  - Lifecycle APIs: `buf_close` and drop-safe cleanup of `ByteBuffer` resources
  - Builder APIs: `new_buffer` (fixed) and `new_growable_buffer(initial_capacity, max_capacity)` (bounded auto-grow)
  - Framing APIs: endian-aware read/write (`u8`, `i16/i32/i64`, `u16/u32/u64` BE/LE), `buf_read_cstring`, `buf_read_length_prefixed`, `buf_write_cstring`, `buf_write_string_prefixed`
  - Backfill APIs: `buf_patch_u16/u32/u64_be` and `buf_patch_u16/u32/u64_le` for cursor-safe offset patching
- Runtime ABI surface:
  - `aic_rt_buffer_new`
  - `aic_rt_buffer_new_growable`
  - `aic_rt_buffer_from_bytes`
  - `aic_rt_buffer_to_bytes`
  - `aic_rt_buffer_close`
  - `aic_rt_buffer_seek` / `aic_rt_buffer_reset`
  - `aic_rt_buffer_read_*` / `aic_rt_buffer_write_*`
- Deterministic failure semantics:
  - Read past available data => `Underflow`
  - Write past capacity => `Overflow`
  - Invalid UTF-8 C-string payload => `InvalidUtf8`
  - Invalid cursor/length/input => `InvalidInput`
- Verification:
  - `tests/execution_tests.rs`:
    - `exec_buffer_binary_protocol_roundtrip`
    - `exec_buffer_negative_paths_are_typed_and_deterministic`
    - `exec_buffer_growable_mode_and_explicit_close_are_deterministic`
  - `make verify-intrinsics`
- Example and CI integration:
  - `examples/data/binary_protocol.aic`
  - `scripts/ci/examples.sh` includes the example in both `check` and `run` gates

## Verification Gate Blocking (EPIC-QV #63)

- Required gate command:
  - `make test-e8`
- Release-blocking chain:
  - `.github/workflows/release.yml` `release-preflight` runs `make ci`.
  - `make ci` runs `check`.
  - `check` includes `test-e8`.
- Continuous fuzz validation:
  - `.github/workflows/nightly-fuzz.yml` runs `make test-e8-nightly-fuzz`.
- Evidence artifacts:
  - `target/e8/perf-report.json`
  - `target/e8/perf-report-*.json`
  - `target/e8/perf-trend-*.json`
  - `target/e8/nightly-fuzz-report.json`
- Verification examples:
  - `examples/verify/qv_contract_proof_fail.aic`
  - `examples/verify/qv_contract_proof_fixed.aic`
  - `examples/verify/file_protocol.aic`
  - `examples/verify/file_protocol_invalid.aic`

## Operations Gate Blocking (EPIC-OPS #64)

- Release preflight workflow:
  - `.github/workflows/release.yml` runs `release-preflight` before build/publish.
  - `release-preflight` runs `make ci` + policy/LTS/security checks.
- Security workflow:
  - `.github/workflows/security.yml` runs `make security-audit`, policy/LTS checks, and `make repro-check`.
- Primary OPS commands:
  - `make test-e9`
  - `make security-audit`
  - `make repro-check`
  - `make release-preflight`
- OPS examples:
  - `examples/ops/migration_v1_to_v2/`
  - `examples/ops/observability_demo/`
  - `examples/ops/sandbox_profiles/`

## Struct Default Fields (ERGO-T5 #171)

- Parser/IR support `field: Type = expr` in struct declarations.
- Typechecker enforces compile-time-evaluable defaults and validates default type compatibility.
- Struct literals may omit fields that define defaults; non-default fields remain required.
- Builder synthesizes `TypeName::default()` for structs where all fields declare defaults.
- LLVM codegen materializes omitted fields from evaluated default expressions.
- Example: `examples/core/struct_defaults.aic`.

## Char Type and std.char (ERGO-T3 #169)

- `Char` is a primitive Unicode scalar value lowered to LLVM `i32`.
- Lexer/parser accept single-quoted literals (for example `'a'`, `'😀'`, and escapes).
- `std/char.aic` exposes runtime-backed APIs:
  - `is_digit(c: Char) -> Bool`
  - `is_alpha(c: Char) -> Bool`
  - `is_whitespace(c: Char) -> Bool`
  - `char_to_int(c: Char) -> Int`
  - `int_to_char(n: Int) -> Option[Char]`
  - `chars(s: String) -> Vec[Char]`
  - `from_chars(cs: Vec[Char]) -> String`
- JSON serde support encodes `Char` as integer codepoints and rejects invalid scalar values on decode.
- Reference example: `examples/data/char_ops.aic` (wired into `scripts/ci/examples.sh` check/run sets).


## Visibility Modifiers and Access Control (ERGO-T1 #167)

- Parser/AST/IR support `pub`, `pub(crate)`, and `priv` on `fn`, `struct`, `enum`, `trait`, and `impl`, plus field-level visibility on structs.
- Default visibility is private, so cross-module access now requires explicit `pub` or `pub(crate)`.
- Resolver stores per-module visibility metadata and exports only non-private symbols for imported-module lookup.
- Typechecker rejects cross-module access to private symbols and emits `E2102` with actionable `pub` guidance.
- User-authored direct intrinsic calls (`aic_*`) are rejected as private runtime implementation details; compiler-generated intrinsic lowering paths remain valid.
- Reference example: `examples/core/visibility_modifiers_demo` (wired into `scripts/ci/examples.sh` check/run sets).

## Template Literals (ERGO-T2 #168)

- Lexer accepts prefixed template strings: `f"..."` and `$"..."`.
- Parser supports interpolation segments with nested expressions: `{expr}`.
- Escaped braces are supported via either `{{`/`}}` or `\{`/`\}`.
- Templates are lowered to `aic_string_format_intrinsic(template, args)` with compiler-synthesized `Vec[String]` argument assembly.
- Typechecking enforces interpolated values are `String`; callers must use explicit conversions such as `int_to_string(...)` for non-string values.
- Reference example: `examples/data/template_literals.aic` (wired into `scripts/ci/examples.sh` check/run sets).

## Validation Inventory

### Tests

- Library/unit tests in:
  - `src/package_workflow.rs`
  - `src/docgen.rs`
  - `src/std_policy.rs`
  - `src/fuzzing.rs`
  - `src/differential.rs`
  - `src/execution_matrix.rs`
  - `src/perf_gate.rs`
  - `tests/unit_tests.rs`
- Runtime/execution tests in:
  - `tests/execution_tests.rs`
  - `tests/e8_matrix_tests.rs`
  - `tests/e8_conformance_tests.rs`
  - `tests/e8_fuzz_tests.rs`
  - `tests/e8_differential_tests.rs`
  - `tests/e8_perf_tests.rs`
  - `tests/e9_release_ops_tests.rs`

### Examples

- `examples/e6/std_smoke.aic`
- `examples/e6/pkg_app/`
- `examples/e6/deps_checksum.aic`
- `examples/e6/doc_sample.aic`
- `examples/e6/deprecated_api_use.aic`
- `examples/e7/cli_smoke.aic`
- `examples/e7/diag_errors.aic`
- `examples/e7/explain_trigger.aic`
- `examples/e7/suggest_effects_demo.aic`
- `examples/e7/lsp_project/`
- `examples/e7/harness/`
- `examples/e8/conformance_pack/`
- `examples/e8/roundtrip_random_seed.aic`
- `examples/e8/matrix_program.aic`
- `examples/e8/large_project_bench/`
- `examples/e9/sandbox_smoke.aic`
- `examples/data/bitwise_protocol.aic`
- `examples/data/char_ops.aic`
- `examples/data/template_literals.aic`
- `examples/io/tcp_echo_client.aic`

Examples are integrated into `scripts/ci/examples.sh`.

## Safe Extension Checklist

1. Keep lockfile schema deterministic; sort dependencies and outputs.
2. Do not bypass checksum validation when lockfile is present.
3. Keep offline mode conservative: fail fast on missing/corrupted cache.
4. When changing std API surface, update baseline intentionally and review deprecations.
5. Keep new diagnostics registered in `src/diagnostic_codes.rs`.
6. Run `make ci` before commit.
7. For E8 changes, update corpus/budget docs and ensure `make test-e8` remains deterministic.
8. For E9 changes, run `make test-e9`, `make security-audit`, and `make repro-check`.
