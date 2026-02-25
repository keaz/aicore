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
- LLVM backend + runtime ABI: `src/codegen.rs`
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

In `src/codegen.rs`:

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
- runtime panic ABI: `aic_rt_panic(ptr, len, cap, line, column)`
- `aic build --debug-info` emits debug metadata and source-mapped panic locations

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

- `io`, `fs`, `env`, `config`, `path`, `proc`, `net`, `time`, `rand`, `json`, `url`, `http`, `regex`, `concurrent`, `string`, `vec`, `option`, `result`

Notes:

- Effects are declared on side-effecting std APIs.
- `std.time.now` is compatibility API and intentionally deprecated in policy metadata.
- `std.fs` public APIs delegate to runtime intrinsic wrappers (`aic_fs_*_intrinsic`) and should not be replaced with constant placeholders.
- `std.config` composes file + JSON + env capabilities for app config loading (`load_json`, `load_env_prefix`, `get_or_default`, `require`).
- `std.set` mutator/query APIs are `add`, `has`, and `discard`; `union`/`intersection`/`difference` preserve deterministic ordering via `to_vec`.
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

### Manifest + lockfile workflow (E6-T2)

`src/package_workflow.rs`:

- Parses `aic.toml` package/dependency metadata.
- Generates deterministic `aic.lock` via `aic lock`.
- Lockfile entries include dependency name, resolved path, checksum.
- Build/check/run consume lockfile through package loader when present.

Diagnostics:

- `E2106`: lockfile drift (`aic.toml` vs `aic.lock`).

### Native dependency bridge (PKG-T3)

`src/package_workflow.rs`, `src/typecheck.rs`, `src/codegen.rs`, `src/main.rs`, and `src/driver.rs` now form the FFI bridge pipeline.

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

### Built-in fixture harness (E7-T5)

- Harness module: `src/test_harness.rs`
- Command:
  - `aic test [path] --mode all|run-pass|compile-fail|golden [--json]`
- Fixture categories discovered by directory segment:
  - `run-pass`
  - `compile-fail`
  - `golden`
- Sample fixtures:
  - `examples/e7/harness/`

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

### Verification-quality runbooks (QV-T6)

Agent-grade docs for all QV gates:

- `docs/verification-quality/README.md`
- `docs/verification-quality/contracts-proof-obligations.md`
- `docs/verification-quality/effect-protocols.md`
- `docs/verification-quality/fuzz-differential-runbook.md`
- `docs/verification-quality/perf-sla-playbook.md`
- `docs/verification-quality/incident-reproduction.md`

Verifier-focused examples:

- `examples/verify/qv_contract_proof_fail.aic`
- `examples/verify/qv_contract_proof_fixed.aic`
- `examples/verify/file_protocol.aic`
- `examples/verify/file_protocol_invalid.aic`

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
