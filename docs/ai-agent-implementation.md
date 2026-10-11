# AI Agent Implementation Guide (E4 + E5 + E6 + E7 + E8 + E9)

This document is implementation-oriented and intended for autonomous contributors working on compiler/runtime/package changes.

Wave 5 numeric adoption planning artifacts:

- `docs/numeric-api-adoption-wave5.md`
- `docs/numeric-api-adoption-wave5.json`

## Wave 5D Migration Markers (`#333`)

- Agent-facing Wave 5D example markers: `wave5_migration_buffer_u32`, `wave5_numeric_end_to_end`.
- Canonical verification snippet style should keep explicit test-suite selection (`--test`) to avoid command-shape drift.
- Numeric drift + command policy guard reference: `tests/unit_tests.rs` (`unit_wave5d_docs_examples_and_command_policy_markers_are_present`).
- Docs/examples CI wiring guard reference: `tests/agent_recipe_tests.rs` (`wave5d_numeric_docs_examples_and_ci_policy_are_consistent`).

```bash
cargo run --quiet --bin aic -- check examples/data/wave5_migration_buffer_u32.aic
cargo run --quiet --bin aic -- run examples/data/wave5_migration_buffer_u32.aic
cargo run --quiet --bin aic -- check examples/data/wave5_numeric_end_to_end.aic
cargo run --quiet --bin aic -- run examples/data/wave5_numeric_end_to_end.aic
cargo test --locked --test unit_tests unit_wave5d_docs_examples_and_command_policy_markers_are_present
cargo test --locked --test agent_recipe_tests wave5d_numeric_docs_examples_and_ci_policy_are_consistent
```

Core language-specific deep dive:

- `docs/core-language-1.0.md`
- `docs/ai-agent-rest-guide.md`

## Pipeline Touch Points

- Frontend orchestration: `src/driver.rs`
- Package loading and imports: `src/package_loader.rs`
- Manifest/lock/checksum/cache: `src/package_workflow.rs`
- Effects normalization + validation: `compiler/aic/libs/typecheck/src/main.aic`
- Type/effect checking + generic instantiation recording: `compiler/aic/libs/typecheck/src/main.aic`
- Contract static verification + runtime lowering: `compiler/aic/libs/typecheck/src/main.aic`
- LLVM backend + runtime ABI: `compiler/aic/libs/backend_llvm/src/main.aic`
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
- Source formatter (`aic fmt`, LSP formatting): `src/formatter.rs`

## Source Formatter Trivia (`aic fmt`)

- `lexer::lex_with_comments` returns `Comment { text, span }` for every `//` comment; `lexer::lex` drops them, so the parser is unaffected.
- `formatter::format_source(source, file)` is the only entry point for formatting a file a person wrote. It prints from the IR and threads a `SourceContext` (source text, tokens, comment cursor) through the printer:
  - comments are flushed before each item, attribute, statement, block tail, struct field, enum variant, trait/impl method, match arm, and closing `}`; a comment on the same line after an element is appended to that line;
  - comments inside an element that is printed on one line are flushed above it (`first_nested_region` finds where the element's own comment slots start);
  - literal tokens are re-emitted from source by exact span (`0xFF`, escapes, template prefix); `self` receivers come from source tokens because the IR drops `&`/`&mut`;
  - lowered sugar is re-sugared: `for` (range and iterator protocol), `else if`, tuple `let` (`__tuple_let_N` temporaries), and the synthesized `Name::default` function for defaulted structs is skipped.
- `format_source` re-parses its output and compares comment texts plus the span-free, canonicalized AST (`verify_formatted`). Any mismatch returns `FormatError::Verification`, and `aic fmt` exits `3` without writing.
- `formatter::format_program(&ir)` remains the trivia-free printer for generated code (synthesize, scaffold, differential, fuzzing).
- When adding syntax that the parser lowers, extend the printer to re-sugar it; `tests/formatter_tests.rs` formats every checked-in `.aic` file and fails on a guard trip, a changed comment, or a non-idempotent result.
- User guide: [`docs/agent-tooling/commands/aic-fmt.md`](agent-tooling/commands/aic-fmt.md).

## Named Function Arguments (`[ERGO-T6]`)

- Parser contract: call sites support `name: value` in argument lists and carry labels via `arg_names: Vec<Option<String>>` on AST/IR call nodes.
- Typechecker contract (`compiler/aic/libs/typecheck/src/main.aic`):
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
- [ ] Protocol schemas are current: `docs/agent-tooling/schemas/parse-response.schema.json`, `docs/agent-tooling/schemas/ast-response.schema.json`, `docs/agent-tooling/schemas/check-response.schema.json`, `docs/agent-tooling/schemas/build-response.schema.json`, `docs/agent-tooling/schemas/fix-response.schema.json`, `docs/agent-tooling/schemas/testgen-response.schema.json`, `docs/agent-tooling/schemas/session-response.schema.json`, `docs/agent-tooling/schemas/patch-response.schema.json`, `docs/agent-tooling/schemas/patch-request.schema.json`, `docs/agent-tooling/schemas/validate-call-response.schema.json`, `docs/agent-tooling/schemas/validate-type-response.schema.json`, `docs/agent-tooling/schemas/suggest-response.schema.json`, `docs/agent-tooling/schemas/context-response.schema.json`, `docs/agent-tooling/schemas/query-response.schema.json`, `docs/agent-tooling/schemas/symbols-response.schema.json`
- [ ] Incremental daemon docs are current: `docs/agent-tooling/incremental-daemon.md`
- [ ] LSP example reflects the implemented workflow: `examples/agent/lsp_workflow.json`
- [ ] Agent recipe docs are current and aligned: `docs/agent-recipes/`
- [ ] Test gate passes: `make test-e7`
- [ ] Relevant test files pass: `tests/agent_protocol_tests.rs`, `tests/agent_recipe_tests.rs`, `tests/lsp_smoke_tests.rs`, `tests/e7_cli_tests.rs`
- [ ] Epic closure evidence is posted: commit hash, `make test-e7` result, and touched docs/examples/tests

## E4 Summary (Effects + Contracts)

### Effects

- Canonical pass: `normalize_effect_declarations(program, file)` in `compiler/aic/libs/typecheck/src/main.aic`
- Known taxonomy: `io`, `fs`, `net`, `time`, `rand`, `env`, `proc`, `concurrency`
- Diagnostics:
  - unknown effect: `E2003`
  - duplicate declaration: `E2004`
  - direct undeclared use: `E2001`
  - transitive missing effect path: `E2005`

### Contracts

- Static verifier: `verify_static(program, file)` in `compiler/aic/libs/typecheck/src/main.aic`
- Runtime lowering: `lower_runtime_asserts(program)`
- Guarantees:
  - `requires` checks at function entry
  - `ensures` checks on every explicit return and implicit tail return
  - struct invariants via synthesized helper constructors (`__aic_invariant_ctor_*`)

## E5 Summary (LLVM Backend + Runtime ABI)

### Toolchain contract

In `compiler/aic/libs/backend_llvm/src/main.aic`:

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
- string runtime intrinsics surface deterministic panic payloads for invalid input/alloc/overflow:
  - `AIC_RT_STRING_ERROR|api=<name>|code=<CODE>|detail=<detail>`
  - valid empty-string results are preserved and are no longer used as failure sentinels
- `aic build --debug-info` emits debug metadata and source-mapped panic locations
- `aic build --opt-level <LEVEL>` accepts `0..3` (`O0..O3`)
- `aic build --release` defaults optimization to `O2` unless overridden with `--opt-level`/`-O`
- Wave 5C net endpoint/port ABI guarantee:
  - runtime net intrinsics remain `addr: String`-based at the boundary
  - typed endpoint/port wrappers are additive source-level helpers
  - wrapper coercions (`Int`/`UInt16`, `String`/endpoint) are deterministic checked conversions and map invalid values to typed errors without changing runtime ABI layout

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
  - `Option`: `is_some`, `is_none`, `unwrap_or`, `unwrap_or_else`, `map`, `map_or`, `and_then`, `or`, `or_else`, `filter`, `zip`, `ok_or`, `ok_or_else`
  - `Result`: `is_ok`, `is_err`, `ok`, `err`, `unwrap_or`, `unwrap_or_else`, `map`, `map_err`, `map_or`, `and_then`, `or_else`
  - `?` works on both (`check_option_try` / `gen_option_try` for Option); there is no panicking `unwrap`/`expect` until the `panic` effect (VERIFY-T2). See `docs/std-api/option-result.md` (SYNTAX-T6, #453).
  - method chains use standard static method dispatch (`value.map(...).and_then(...).unwrap_or(...)`)
- Current backend support for set/map key paths includes `String`, `Bytes`, `Int`, `UInt64`, and `Bool`; unsupported key kinds emit deterministic backend diagnostic `E5011` with explicit supported-key guidance.
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
- Lockfile entries include dependency name, resolved path, checksum, resolved version (when available), and source provenance (when available).
- Build/check/run consume lockfile through package loader when present.

Diagnostics:

- `E2106`: lockfile drift (`aic.toml` vs `aic.lock`).

### Native dependency bridge (PKG-T3)

`src/package_workflow.rs`, `compiler/aic/libs/typecheck/src/main.aic`, `compiler/aic/libs/backend_llvm/src/main.aic`, `src/main.rs`, and `src/driver.rs` now form the FFI bridge pipeline.

- Frontend syntax:
  - `extern "C" fn ...;`
  - `unsafe fn ...`
  - expression `unsafe { ... }`
- Typechecker/diagnostic contract:
  - `E2120`: missing/unsupported extern ABI
  - `E2121`: extern signature must be plain (no async/generics/effects/contracts)
  - `E2122`: calling `extern` or `unsafe fn` requires an explicit unsafe boundary
  - `E2123`: unsupported C-ABI signature type (MVP support: `Int`, `Int8`, `Int16`, `Int32`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `Bool`, `Float`, `Char`, `()`)
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

`compiler/aic/libs/lexer/src/main.aic`, `compiler/aic/libs/parser/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `canonical formatting implementation`, and `compiler/aic/libs/typecheck/src/main.aic` model intrinsic runtime bindings directly.

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
- verifies every intrinsic has a known backend lowering: a runtime binding (`INTRINSIC_BINDING_EXPECTATIONS`) or a builtin lowering by name (`BUILTIN_INTRINSIC_LOWERINGS`)
- verifies runtime-binding declaration signatures against accepted backend shapes and runtime symbol mapping
- for `std`, every backend-lowered function is a signature-only `intrinsic fn` (no placeholder bodies); a missing lowering fails compilation with `E5020`

Deterministic JSON fields:

- `schema_version`, `input`, `files_scanned`, `intrinsic_declarations`, `verified_bindings`, `builtin_lowerings`, `issue_count`, `ok`
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
- Scaffold usage guide with exact command/output pairs: `docs/agent-tooling/scaffold-guide.md`
- Patch authoring guide: `docs/agent-tooling/patch-authoring.md`
- Agent protocol schemas:
  - `docs/agent-tooling/schemas/parse-response.schema.json`
  - `docs/agent-tooling/schemas/ast-response.schema.json`
- `docs/agent-tooling/schemas/check-response.schema.json`
- `docs/agent-tooling/schemas/build-response.schema.json`
- `docs/agent-tooling/schemas/fix-response.schema.json`
- `docs/agent-tooling/schemas/testgen-response.schema.json`
- `docs/agent-tooling/schemas/session-response.schema.json`
- `docs/agent-tooling/schemas/patch-response.schema.json`
- `docs/agent-tooling/schemas/patch-request.schema.json`
- `docs/agent-tooling/schemas/validate-call-response.schema.json`
- `docs/agent-tooling/schemas/validate-type-response.schema.json`
- `docs/agent-tooling/schemas/suggest-response.schema.json`
- `docs/agent-tooling/schemas/context-response.schema.json`
- `docs/agent-tooling/schemas/query-response.schema.json`
- `docs/agent-tooling/schemas/symbols-response.schema.json`
- Protocol guide:
  - `docs/agent-tooling/protocol-v1.md`
- Protocol fixtures:
  - `examples/agent/protocol_parse.json`
  - `examples/agent/protocol_ast.md`
- `examples/agent/protocol_check.json`
- `examples/agent/protocol_build.json`
- `examples/agent/protocol_fix.json`
- `examples/agent/protocol_testgen.json`
- `examples/agent/protocol_session.json`
- `examples/agent/protocol_patch.json`
- `examples/agent/protocol_validate_call.json`
- `examples/agent/protocol_query.json`
- `examples/agent/protocol_symbols.json`
- `examples/agent/protocol_validate_type.json`
- `examples/agent/protocol_suggest.json`
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

### Diagnostic catalog contract

`docs/errors/catalog.md` has exactly one row per code in `REGISTERED_DIAGNOSTIC_CODES`:

```text
| `E1234` | Mode | Description | Trigger | Fix |
```

- Runnable modes hold single-file programs (one line, `|` written as `\|`):
  - `check`: `aic check` reports the code for the trigger.
  - `warn-unused`: `aic check --warn-unused` reports it (unused-analysis warnings).
  - `build`: the trigger passes the checker and code generation reports the code.
  - The fix must not report the code and must have no error diagnostics at all.
- Described modes are not run; their trigger/fix cells say how the code is produced and resolved:
  `cli` (a CLI command), `package` (manifest, lockfile, registry or workspace fixture),
  `selfhost` (self-host tools under `compiler/aic`), `internal` (defensive backstop the checker
  always pre-empts; the row names the code users see instead), `reserved` (never emitted),
  `retired` (no longer emitted).
- `tests/diagnostic_catalog_tests.rs` (run by `make test-unit`) parses the table, runs every runnable
  trigger and fix through `driver::run_frontend` (plus `unused_analysis` for `warn-unused`, plus
  `codegen` for `build`), and fails when:
  - a registered code has no row, a row names an unregistered code, or a code has two rows;
  - a row is malformed (wrong cell count, unknown mode, trigger/fix not one code span);
  - two rows share a trigger program, or two non-`reserved` rows share a description (the placeholder
    pattern this test replaced);
  - a trigger does not report its code, or a fix reports it or any error.
- `build` rows other than `E5037` are known check/build parity gaps (listed in `docs/diagnostic-codes.md`).
  Closing one makes the checker reject the trigger first, so the test fails until the row moves to the
  checker code.
- When you add a diagnostic code: register it, add its row with a verified trigger and fix, and update
  the `(N codes)` count in the catalog introduction.

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
  - JSON diagnostic transports may include optional `reasoning` metadata for supported high-frequency codes; absence is the stable fallback for unsupported families.
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
  - `examples/agent/protocol_validate_call.json`
  - `examples/agent/protocol_validate_type.json`
  - `examples/agent/protocol_suggest.json`
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

## Impl Methods as Function Values (#445 parity)

- Language surface: `C::get` names an impl method (inherent or trait impl) as a value. The `self` receiver is the first parameter, so `fn get(self) -> Int` gives `Fn(C) -> Int`; `fn scaled(&self, k: Int) -> Int` gives `Fn(C, Int) -> Int`.
- Checker (`src/typecheck.rs`, `ExprKind::Var` value path): the same rules as named functions apply (`E1282` generic, `E1283` effectful).
- Codegen (`src/codegen/generator_core.rs`): `resolve_function_value_symbol` considers impl methods as well as top-level functions, skipping generic ones; `ensure_fn_value_adapter` builds the usual env-ignoring adapter that forwards to the method's LLVM symbol.
- `&mut self` methods (SYNTAX-T1, #448) take the receiver by pointer, so they are rejected as values (`E1303`) and skipped by `resolve_function_value_symbol`. A `&mut self` method value that reaches codegen without the checker reports `E5034` instead of getting a by-value adapter.
- Verification: `src/typecheck.rs` tests `method_values_take_the_receiver_as_first_parameter` (types, and `E1303` for arguments, struct fields and trait impl methods) and `mutable_self_receivers_are_checked`, `src/codegen/tests.rs` tests `impl_methods_lower_as_function_values` and `mutable_receiver_methods_are_not_lowered_as_function_values`, `tests/execution_tests.rs` test `exec_first_class_fn_value_from_impl_method`, the parity gate `tests/check_build_parity_tests.rs`, and `examples/core/method_values.aic`.

## Place Assignment and `&mut self` (SYNTAX-T1, #448)

- Language surface: `p.x = v`, `p.a.b = v`, `t.0 = v`, `t.1.0 = v`, `v[i] = x`, `items[i].qty = x`, and `self.n = v` / `self = v` in `&mut self` methods. Rules and diagnostics are in `docs/reference/statements.md`.
- Data model:
  - `ast::Stmt::Assign` / `ir::Stmt::Assign` carry `path: Vec<PlaceProjection>` after the root `target`.
  - The IR builder binds each index expression to a hidden `$place_index<N>` local (`ir_builder::PLACE_INDEX_BINDING_PREFIX`) emitted just before the assignment; `ir::PlaceProjection::Index` names that binding. The formatter prints the expression back inside the brackets.
  - `Param.mutable_receiver` marks a `&mut self` receiver; the resolver copies it to `FunctionInfo.mutable_receiver`.
- Checker (`src/typecheck.rs`): `place_type` types the path (`E1297`, `E1298`); the borrow pass checks the root's mutability, borrows and moves; `mutable_receiver_calls` records `&mut self` calls so `check_mutable_receiver_call` and the closure capture check can validate the receiver.
- Codegen (`src/codegen/generator_place.rs`): a place assignment rebuilds each level (`insertvalue` for structs and tuples, `aic_rt_vec_set` for `Vec`), re-runs invariant helpers, and stores the new root. A `&mut self` function takes its receiver as `T*`; callers pass the receiver's storage address (`place_address`) or spill a temporary.
- Verification: `tests/execution_tests.rs` (`exec_place_assignment_*`, `exec_mutable_self_methods_update_the_callers_value`, `exec_field_assignment_rechecks_struct_invariants`, `exec_vec_index_assignment_out_of_bounds_panics`), `src/typecheck.rs` tests (`place_assignment_targets_are_type_checked`, `mutable_self_receivers_are_checked`, `trait_impl_receiver_mode_must_match`), `src/parser.rs` tests, and `examples/core/place_assignment.aic`.

## String Ergonomics (SYNTAX-T12, #463)

- Language surface: `String + String`, string ordering, raw (`r"..."`, `r#"..."#`) and multiline (`"""..."""`) literals, interpolation of numbers, `Bool` and `Char`; `std.string` gains `byte_count`, `char_count` and `compare` (`byte_len`/`concat` would clash with `std.bytes` names in modules that import both). Rules: `docs/reference/expressions.md`, `docs/reference/syntax.md`.
- Lexer: `lex_raw_string` (`raw_string_hashes`) and `lex_multiline_string` (`dedent_multiline`) produce ordinary `String` tokens.
- Checker: `check_binary` types `String + String` as `String` and accepts string ordering. An interpolation argument of a scalar type is wrapped in the `$display` builtin (`ast::DISPLAY_BUILTIN`, recorded in `expr_rewrites`).
- Codegen/runtime: `gen_string_concat` calls `aic_rt_string_concat` (explicit lengths, so embedded NUL bytes survive); ordering compares the result of `aic_rt_string_compare`; `gen_display` widens integers to `i128` for `aic_rt_string_i128_to_string` (signed or unsigned) and uses `aic_rt_string_float_to_string`, `aic_rt_string_bool_to_string` and `aic_rt_string_from_char` (UTF-8).
- Not covered: interpolating user types, which depends on the core `Display` trait (TRAIT-T5, #481).
- Verification: `exec_string_concatenation_ordering_literals_and_interpolation`, `string_concatenation_ordering_and_interpolation_are_checked`, `unit_typecheck_orders_strings_and_rejects_mixed_comparisons`, `unit_template_literal_shows_scalars_and_rejects_other_values`, `lexes_raw_and_multiline_strings`, formatter test `raw_and_multiline_strings_round_trip`, golden `case37_strings.aic`, `examples/core/string_ergonomics.aic`.

## Tuple Structs and Unit Structs (SYNTAX-T16, #462)

- Language surface: `struct Meters(Int);`, `struct Origin;`, construction `Meters(5)` / `Origin`, `.0` access, patterns `Meters(x)` / `Origin`, `self.0` in invariants. Rules: `docs/reference/types.md`.
- AST/IR: `StructDef.shape: StructShape { Named, Tuple, Unit }` (omitted from JSON when `Named`); tuple fields are named `0`, `1`, ... (`parse_tuple_or_unit_struct`). `resolver::StructInfo.shape` carries it to the checker.
- Checker: `check_tuple_struct_ctor` checks `Meters(5)` as the struct literal `Meters { 0: 5 }` and records it in `expr_rewrites`; a unit struct name used as a value becomes `Origin {}`. `tuple_struct_pattern` turns `Meters(x)` / `Origin` patterns into struct patterns (`pattern_rewrites`), also for exhaustiveness, and takes apart the `Pair((a, b))` form the parser produces for multi-field patterns.
- Invariants: `ir::resolve_invariant_self` rewrites `self.f` to the field name; the checker applies it to a copy, the driver applies it to the IR before contract lowering. `ir::visit_expr_mut` is the expression-level walker it uses.
- Formatter: `struct_def` prints the tuple and unit forms.
- Verification: `exec_tuple_and_unit_structs`, `exec_tuple_struct_invariant_is_enforced`, `tuple_and_unit_structs_are_checked`, `parses_tuple_and_unit_structs`, formatter test `tuple_and_unit_structs_round_trip`, golden `case36_tuple_structs.aic`, `examples/core/tuple_structs.aic`.

## Explicit Generic Type Arguments (SYNTAX-T15, #461)

- Language surface: `callee::[T, U](args)` for functions (also module-qualified), inherent methods and enum variant constructors including `Some`/`Ok`/`Err`. Rules: `docs/reference/generics.md`.
- Parser: `parse_postfix` reads `::[..]` and attaches it to the following call (`ExprKind::Call.type_args`, also in the IR as `TypeId`s, omitted from JSON when empty); an identifier path stops at `::[`. E1312 for an empty list or no call.
- Checker: `Checker::explicit_type_args` turns them into initial generic bindings for the function path, user enum variant constructors and the builtin `Some`/`Ok`/`Err`; inherent method calls pass them on to the `Type::method` call they become. Other method paths report E1312. The call's result type goes into `expr_type_hints`, which is how code generation picks the instance (`make::[Int]()` has no argument to infer from).
- Formatter: prints `::[..]` after the callee.
- Verification: `exec_explicit_type_arguments_select_instances`, `explicit_type_arguments_bind_generics`, `parses_explicit_type_arguments_on_calls`, formatter test `explicit_type_arguments_round_trip`, golden `case35_type_arguments.aic`, `examples/core/explicit_type_arguments.aic`.

## Closure Inference and Callee Expressions (SYNTAX-T14, #460)

- Language surface: `|x| x + 1`, `|x| { .. }` (return type inferred), parameter types from the expected `Fn` type (including generic calls and the enclosing function's own type variables), calls on any `Fn`-typed expression, and calls of `Fn`-typed struct fields. Rules: `docs/reference/expressions.md`.
- Parser/AST: `ExprKind::Closure.ret_type` is `Option<TypeExpr>`; an expression body becomes a block with that tail. The IR keeps a `TypeId`; an omitted return type lowers to the hole type `<?>`.
- Checker: the closure arm takes the expected return when it is fixed (`Checker::is_fixed_type`, which accepts the current function's generics recorded in `current_function_generics`), otherwise infers it from the body and re-checks a body with `return` against it. Every closure's parameter and return types are recorded in `TypecheckOutput::closure_signatures`; `driver::apply_closure_signatures` writes them into closures that omit them. `check_field_closure_call` turns `s.f(x)` into a call of `{ s.f }` (`expr_rewrites`) when `f` is an `Fn` field and no method exists.
- Codegen: closure capture analysis now captures an outer `Fn` value used as a callee (`|x| g(f(x))` inside a generic function failed with `E5012 unknown function 'f'`). Names seen only as callees are marked (`CALLEE_CAPTURE_MARK`); `gen_closure_value` captures them when they are locals and otherwise leaves them to normal call resolution (generic and std functions are not in `fn_sigs` when the closure is generated).
- Formatter: closures without `-> T` print as `|x| expr` or `|x| { .. }` (parenthesized as an operand), and `Fn` types print in source form `Fn(A, B) -> R` instead of the internal `Fn[A, B, R]`.
- Verification: `exec_closures_infer_types_and_call_any_callee`, `closure_return_and_parameter_types_are_inferred`, `parses_closures_with_and_without_return_annotations`, formatter test `closures_without_return_annotations_round_trip`, golden `case34_closures.aic`, `examples/core/closure_inference.aic`.

## Block Comments, Binary/Octal Literals and Digit Separators (SYNTAX-T13, #459)

- Lexer: `lex_block_comment` records nested `/* */` comments in the comment list that `lex_with_comments` returns, so `aic fmt` keeps them like line comments (E0002 when unclosed). `lex_number` reads a radix prefix (`0x`, `0b`, `0o`) and `lex_digit_run` digits with `_` separators (E0003 for a digit outside the base or no digits). Tokens keep the source text in `raw_text`.
- `lexer::parse_int_literal_text` turns that text into its value for every radix and separator; the checker's and the code generator's `parse_raw_int_literal_magnitude` use it.
- Verification: `lexes_binary_octal_literals_and_digit_separators`, `lexes_nested_block_comments_as_comments`, formatter test `block_comments_and_new_literals_round_trip`, `exec_binary_octal_and_separated_literals`, `examples/core/number_literals.aic`.

## Ranges, Range Patterns and `@` Bindings (SYNTAX-T11, #458)

- Language surface: range values `a..b` / `a..=b` (`std.range::Range[T]`), slices `v[a..b]` (and the open forms), range patterns `lo..=hi` / `lo..hi` / `lo..` / `..=hi`, negative and float literal patterns, `name @ pattern`. Rules: `docs/reference/expressions.md`, `docs/reference/pattern-matching.md`.
- Lexer: `..=` (`DotDotEq`) and `@` (`At`).
- Parser: `parse_range` sits above `||` and produces builtin calls (`ast::RANGE_BUILTIN`, `RANGE_INCLUSIVE_BUILTIN`, `RANGE_FROM_BUILTIN`, `RANGE_TO_BUILTIN`, `RANGE_TO_INCLUSIVE_BUILTIN`, `RANGE_FULL_BUILTIN`). `for i in a..b` / `a..=b` is detected with `split_range_call` and desugared by `desugar_for_range` (the inclusive form adds a `done` flag so it never computes `end + 1`); the formatter's `extract_range_for` recognizes both shapes. Patterns gain `PatternKind::Float`, `Range { start, end, inclusive }` (`ast::RangeBound`) and `Binding { name, pattern }`; a top-level `@` in a plain `let` binds the whole value, then destructures it.
- Checker: `check_range_value` rewrites `a..b` to `range.new_range(a, b, false)`; `slice_lookup` rewrites `v[range]` to `range.slice_vec(v, start, end, inclusive)` (or `slice_*_by` for a `Range` value) and `finish_index_lookup` wraps it in the panicking match that indexing uses. `check_range_pattern` validates bounds (E1234/E1245/E1309).
- Exhaustiveness: integer and `Char` patterns are `match_usefulness::Ctor::Range(lo, hi)` over `PatternTypes::int_domain`; columns are split at range boundaries (`range_boundaries`, `split_range`, `domain_pieces`), missing ranges are merged. `UInt128` values use an order-preserving `i128` encoding (`encode_pattern_int`).
- Codegen: `gen_pattern_test` emits signed/unsigned `icmp` range checks, `fcmp oeq` for floats and stores `@` bindings before testing the inner pattern; the tuple fast path never sees these patterns (`pattern_is_flat` is false).
- Std: `std/range.aic` (`Range`, `new_range`, `range_contains`, `range_len`, `range_is_empty`, `range_next`, the `Iterator` impl for `Range[Int]`, `slice_bounds`, `slice_vec`/`slice_string`/`slice_bytes` and their `_by` forms). The std API baseline is regenerated with `AIC_STD_ROOT=$PWD/std aic std-compat`.
- Verification: `exec_range_negative_float_and_binding_patterns`, `exec_range_values_iteration_and_slices`, `exec_out_of_range_slices_panic`, `range_patterns_are_checked_and_split_for_exhaustiveness`, `open_ranges_and_non_integer_ranges_are_rejected_as_values`, `parses_range_float_negative_and_binding_patterns`, `parses_range_expressions_with_lowest_precedence`, `lexes_range_operators_and_at`, formatter test `ranges_and_new_patterns_round_trip`, golden `case33_ranges_and_patterns.aic`, `examples/core/ranges_and_patterns.aic`.

## Loop Labels (SYNTAX-T10, #457)

- Language surface: `'name: while|loop|for`, `break 'name [value]`, `continue 'name`. Rules: `docs/reference/expressions.md`.
- Lexer: a quote followed by a name and no closing quote is `TokenKind::Label(name)`; `'a'` stays a `Char`.
- AST/IR: `While`, `Loop`, `Break` and `Continue` carry `label: Option<String>` (omitted from JSON when absent). `parse_labeled_loop` parses the loop and `set_loop_label` puts the label on the loop node, including the `loop` inside the `if true { ..; loop { .. } }` a `for` desugars to. Passes that rebuild expressions (`ir_builder`, `contracts.rs`, `template_lowering.rs`) copy the label.
- Checker: `LoopContext.label`; `loop_target` resolves the frame a `break`/`continue` targets (E1307), `check_new_loop_label` rejects reused labels (E1308). A labeled break with a value types the target loop.
- Codegen: `LoopFrame.source_label`; `loop_frame_index` picks the frame, and the break/continue runs drops down to that frame's scope depth before branching.
- Formatter: prints `'name: ` before `while`, `while let`, `loop` and `for` (`for_loop_label`), and the label after `break`/`continue`.
- Verification: `exec_labeled_break_and_continue_target_outer_loops`, `loop_labels_are_resolved_and_typed`, `parses_loop_labels_on_while_loop_and_for`, `lexes_loop_labels_apart_from_char_literals`, formatter test `loop_labels_round_trip`, golden `case32_loop_labels.aic`, `examples/core/loop_labels.aic`.

## Numeric Casts and Conversion Methods (SYNTAX-T9, #456)

- Language surface: `x as T` (lossless numeric conversions only) and `checked_to_<t>()` (`Option[T]`), `saturating_to_<t>()`, `wrapping_to_<t>()`, `to_f32()`, `to_f64()` on every integer and float type. Rules: `docs/reference/expressions.md`.
- Parser: `parse_cast` sits between `parse_factor` and `parse_unary` (`as` is contextual) and produces the builtin call `$as:<Type>(x)` (`ast::CAST_BUILTIN_PREFIX`); the formatter prints it back as `x as T`.
- Checker: `check_cast` accepts a pair only when `NumericKind::converts_exactly_to` holds, otherwise `E1305` with a help naming the method. `check_numeric_conversion_method` handles the methods (receiver types are normalized, so aliases such as `type Port = UInt16` work) and records `$convert:<mode>:<Target>(x)` in `expr_rewrites`; `checked` types as `Option[Target]`.
- Codegen: `src/codegen/generator_convert.rs` (`gen_numeric_conversion`); `gen_call` dispatches both prefixes first. LLVM intrinsics it needs are declared through `Generator::llvm_intrinsic_decls`.
- Node ids: checker rewrites carry synthetic node ids counting down from `u32::MAX`, and std now contains such rewrites in every program. Passes that allocate fresh ids after checking (`contracts.rs`, `template_lowering.rs`) start above the largest parsed id via `ir::NodeId::allocation_id`, which counts ids at or above `NodeId::SYNTHETIC_FLOOR` as 0.
- Std: the counting/bit loops that converted between `Int` and fixed-width integers in `std.buffer`, `std.concurrent`, `std.env`, `std.http`, `std.http_server`, `std.net`, `std.proc`, `std.tls` and `std.url` now use `as` and the conversion methods; signatures are unchanged.
- Verification: `exec_integer_conversions_match_two_complement_semantics` and `exec_float_conversions_round_truncate_and_saturate` (generated matrices checked against a Rust reference, including MIN/MAX, NaN and infinities), `numeric_casts_and_conversion_methods_are_checked`, `parses_casts_between_unary_and_binary_operators`, formatter test `casts_round_trip`, golden `case31_numeric_casts.aic`, `examples/core/numeric_conversions.aic`.

## Indexing, List Literals and Std Method Syntax (SYNTAX-T8, #455)

- Language surface: `v[i]`, `m[k]`, `s[i]`, `b[i]`, `[a, b, c]`, and `value.f(args)` on std collections. Rules: `docs/reference/expressions.md`.
- Parser: `base[index]` → `$index(base, index)` and `[a, b]` → `$vec_literal(a, b)` (`ast::INDEX_BUILTIN`, `VEC_LITERAL_BUILTIN`), so every `Call` walker already handles them; the formatter prints them back.
- Checker: `check_index_expr`, `check_vec_literal` and `check_std_method_call` type-check an equivalent std expression built from synthetic nodes (`match vec.get(v, i) { Some(x) => x, None => $index_panic() }`, `vec.push(vec.vec_of(a), b)`, `vec.vec_len(v)`) and record it in `TypecheckOutput::expr_rewrites`; the driver (`apply_expr_rewrites`) substitutes them before codegen, so codegen only adds `$index_panic` (`ast::INDEX_PANIC_BUILTIN`, a structured `index_out_of_bounds` panic).
- Verification: `exec_index_expressions_list_literals_and_std_methods`, `exec_index_out_of_bounds_and_missing_key_panic`, `index_expressions_require_indexable_types_and_imports`, `parses_index_expressions_and_list_literals`, formatter test `index_expressions_and_list_literals_round_trip`, golden `case30_indexing.aic`, `examples/core/indexing.aic`.

## Multi-field and Struct-like Enum Variants (SYNTAX-T4, #454)

- Language surface: `Rect(Int, Int)` and `Move { x: Int, y: Int }` variants, constructed as `Shape::Rect(2, 3)` / `Shape::Move { x: 1, y: 2 }` and matched as `Rect(w, h)` / `Move { x, .. }`. Rules: `docs/reference/pattern-matching.md`.
- Data model (no new IR): a multi-field variant's payload is the tuple of its fields; the parser rewrites `V(a, b)` patterns to `V((a, b))`, and the checker and codegen (`gen_variant_constructor`) accept one argument per tuple element. A struct-like variant's fields become the hidden struct `Enum__Variant` (`ast::struct_variant_payload_name`, generic over the enum's parameters) that the parser emits after the enum (`Parser::pending_items`).
- Checker: `struct_variant_for` resolves `Variant { .. }` literals and `struct_variant_pattern` resolves `Variant { .. }` patterns against the payload struct; both are recorded (`TypecheckOutput::variant_struct_inits`, `pattern_rewrites`) and the driver (`apply_struct_variants`) rewrites them to `Variant(Enum__Variant { .. })` before codegen. `user_struct_name` shows `Shape::Move` in field diagnostics; `match_usefulness::render_variant` prints witnesses as `Rect(_, _)` and `Move { .. }`.
- Formatter: prints the variants back in source form and skips the payload structs.
- Verification: `exec_multi_field_and_struct_like_variants` (including nested patterns, generics and JSON round trip), `multi_field_and_struct_like_variants_are_checked`, `parses_multi_field_and_struct_like_variants`, golden `case29_variant_fields.aic`, `examples/core/variant_fields.aic`.

## `if let`, `while let`, `let ... else` and Destructuring `let` (SYNTAX-T5, #452)

- Language surface and rules: `docs/reference/expressions.md`, `docs/reference/statements.md`.
- Parser desugaring (no new IR): `if let P = e {A} else {B}` → `match e { P => {A}, _$if_let_else => {B} }`; `while let P = e {A}` → `loop { match e { P => {A}, _$while_let_end => break } }`; `let P = e else {B};` → a match whose catch-all arm binds `_$let_else` (one binding: `let x = match ..`; several: a `_$let_else_value_N` temporary plus projections); struct patterns in `let` → a `__struct_let_N__Struct` temporary plus field projections. The marker names are `ast::IF_LET_ELSE_BINDING`, `WHILE_LET_END_BINDING`, `LET_ELSE_BINDING` and `LET_ELSE_TMP_PREFIX` (`_$` cannot be written in source, and the leading `_` keeps unused-variable analysis quiet).
- Checker: `desugared_arm_kind` recognizes the marker arms to require a diverging let-else block (`E1302`) and to explain an always-matching pattern (`E1251`) or a missing `else` (`E1220`); `check_struct_let_pattern` matches the struct temporary against the value's type (`E1245`). The parser rejects refutable plain `let` (`E1110`).
- Formatter: `if_let_parts`, `while_let_parts`, `let_else_group` and the generalized `tuple_let_group` print the sugar back from IR (with or without source text).
- Verification: `exec_if_let_while_let_let_else_and_struct_destructuring`, `pattern_lets_are_checked`, parser tests `desugars_if_let_while_let_and_let_else` and `destructures_structs_in_let_and_rejects_refutable_patterns`, formatter test `pattern_lets_keep_their_sugar`, golden `case28_pattern_lets.aic`, `examples/core/pattern_lets.aic`.

## Bare and Qualified Unit Variants (SYNTAX-T7, #451)

- Language surface: unit variants as values (`Red`, `Color::Red`, `None`, `Option::None`), qualified constructor calls (`Color::Blue(3)`, `Option::Some(x)`, `Result::Ok(v)`), and qualified patterns (`Color::Red =>`, `Option::Some(x) =>`).
- Checker: `unit_variant_callee` resolves a value name the scope does not bind; the `Var` is checked as a zero-argument constructor call and recorded in `TypecheckOutput::variant_callees`. Qualified calls filter `find_variants` by enum (`split_variant_path`); `builtin_variant_constructor` maps `Option::`/`Result::` names to the builtins. Qualified patterns must name the scrutinee's enum (`E1245`).
- Driver: `apply_variant_callees` (using `ir::visit_exprs_mut`) rewrites those `Var`s into constructor calls before codegen; `gen_variant_constructor` filters candidates by the qualifier.
- Formatter: prints `None()` as `None`, and its verifier treats the two as the same program.
- Verification: `exec_bare_and_qualified_unit_variants`, `bare_and_qualified_unit_variants_resolve`, `parses_qualified_variant_patterns_and_values`, golden `case27_unit_variants.aic`, `examples/core/unit_variants.aic`, and the README examples.

## Block Expressions and `if` Without `else` (SYNTAX-T3, #450)

- Language surface: `{ stmts; tail }` as an expression, `if cond { ... }` without `else`, block bodies in match arms (the arm comma is then optional), and block-like statements without `;`. Rules: `docs/reference/expressions.md`, `docs/reference/statements.md`.
- Data model: `ast::ExprKind::Block` / `ir::ExprKind::Block { block }`; every walker that visits `UnsafeBlock` visits `Block` the same way, without the unsafe context. `if` without `else` is an `If` whose else block is empty and zero-width (`else_block.span.start == else_block.span.end`); the checker uses that to explain `E1220`, and `aic fmt` omits the `else`.
- Parser: `parse_primary` accepts `{`; `is_block_like` decides when a statement may omit `;` and a match arm its comma.
- Verification: `tests/execution_tests.rs` (`exec_block_expressions_and_if_without_else`), `src/parser.rs` and `src/typecheck.rs` unit tests, golden `tests/golden/case26_block_expressions.aic`, `examples/core/block_expressions.aic`, and `examples/io/signal_shutdown.aic` in `make examples-check`.

## Compound Assignment (SYNTAX-T2, #449)

- Language surface: `+=`, `-=`, `*=`, `/=`, `%=` alongside the bitwise `&=`, `|=`, `^=`, `<<=`, `>>=`, `>>>=`, on any assignment place (`x`, `p.a.b`, `t.0`, `v[i]`, `items[i].qty`).
- Data model: the parser no longer desugars compound assignment. `ast::Stmt::Assign.op` / `ir::Stmt::Assign.op` hold the operator and `expr` is the right operand, so `aic fmt` prints the compound form and indexed places need no expression for the element read.
- Checker: the stored value is typed with `check_binary(op, place, rhs)`; a compound assignment reads the root, so it is checked for use after move.
- Codegen (`gen_place_assign` in `src/codegen/generator_place.rs`): evaluates the right-hand side, reads the place (`read_place`, which panics on an out-of-range `Vec` index), applies `gen_binary`, and rebuilds the place.
- Verification: `tests/execution_tests.rs` (`exec_compound_assignment_*`), `src/typecheck.rs` (`compound_assignment_is_type_checked`), `src/parser.rs` (`parses_every_compound_assignment_operator`), golden `tests/golden/case25_place_assignment.aic`, REPL test in `tests/e7_cli_tests.rs`.

## Connectivity Primitive: Bitwise Operators (CONN-T1)

- Language surface:
  - Binary: `&`, `|`, `^`, `<<`, `>>`, `>>>`
  - Unary: `~`
  - Compound assignment: `&=`, `|=`, `^=`, `<<=`, `>>=`, `>>>=` (arithmetic `+=` and friends: see SYNTAX-T2 below)
  - Hex literals: `0x...` (`Int`)
- Type rules:
  - All bitwise/shift operators require `Int` operands and produce `Int`.
  - `>>` is arithmetic right shift; `>>>` is logical right shift.
  - Bool misuse diagnostics suggest `&&` / `||`.
- Implementation files:
  - `compiler/aic/libs/lexer/src/main.aic`
  - `compiler/aic/libs/parser/src/main.aic`
  - `compiler/aic/libs/typecheck/src/main.aic`
  - `compiler/aic/libs/backend_llvm/src/main.aic`
- Verification:
  - `compiler/aic/libs/lexer/src/main.aic` parser/lexer unit tests include hex + bitwise token coverage.
  - `compiler/aic/libs/parser/src/main.aic` tests cover precedence and compound assignment desugaring.
  - `compiler/aic/libs/typecheck/src/main.aic` tests cover accepted `Int` paths and rejected `Bool` misuse.
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
  - `aic_rt_net_dns_lookup_all`
  - `aic_rt_net_dns_reverse`
- Error model:
  - `errno`/`getaddrinfo` mapping to `NetError` is deterministic and shared by sync/async paths.
  - Timeouts are explicit (`Timeout`), bind conflicts map to `AddressInUse`, and refused connects map to `Refused`.
- Verification:
  - `tests/execution_tests.rs` includes TCP loopback, UDP/DNS helpers, deterministic multi-address DNS lookup coverage, timeout/invalid-input diagnostics, and refused/address-in-use stability coverage.
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
- Parser/IR preserve template literals as explicit expression nodes.
- A dedicated template-lowering compiler phase rewrites template literals to `aic_string_format_intrinsic(template, args)` using compiler-synthesized `Vec[String]` assembly calls before backend codegen.
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
  - `tests/diagnostic_catalog_tests.rs` (every catalog trigger/fix program)
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

Examples are integrated into `scripts/ci/examples.sh`. The script writes all
scratch output (including captured stdout/stderr of each example) under its
per-run `ARTIFACT_DIR` (`mktemp -d`, removed by an `EXIT` trap); use
`"$EXAMPLE_OUT"`, `"$EXAMPLE_ERR"`, or a path under `"$ARTIFACT_DIR"` for new
checks, never a fixed `/tmp/...` path, so concurrent runs from different
worktrees stay isolated.

## Std Types Need Their Module (E1313)

- `Vec`, `Map`, `Set`, `Deque`, `Bytes`, `ByteBuffer`, and `JsonValue` are known to the checker by name, but they are not prelude types. Code generation takes their layout from the struct in `std.vec`, `std.map`, `std.set`, `std.deque`, `std.bytes`, `std.buffer`, or `std.json`.
- `check_type_name_known` in `src/typecheck.rs` calls `check_std_module_type_loaded`, which reports `E1313` when one of these names is used in a type position and no struct, enum, alias, or generic parameter of that name is visible. The help line names the `import` to add (mapping in `std_module_for_type`).
- A module loaded through another import (for example `std.vec` through `std.string`) satisfies the check, because code generation then has the layout. A user declaration with the same name shadows the std type.
- Without this check, such programs passed `aic check` and failed `aic build` with `E5019`, which broke the check/build parity gate (`tests/check_build_parity_tests.rs`).
- Examples: `examples/core/std_type_without_import_invalid.aic` (check-fail) and `examples/core/std_type_with_import.aic` (run, prints `2`).

## Safe Extension Checklist

1. Keep lockfile schema deterministic; sort dependencies and outputs.
2. Do not bypass checksum validation when lockfile is present.
3. Keep offline mode conservative: fail fast on missing/corrupted cache.
4. When changing std API surface, update baseline intentionally and review deprecations.
5. Keep new diagnostics registered in `diagnostic registry`.
6. Run `make ci` before commit.
7. For E8 changes, update corpus/budget docs and ensure `make test-e8` remains deterministic.
8. For E9 changes, run `make test-e9`, `make security-audit`, and `make repro-check`.
