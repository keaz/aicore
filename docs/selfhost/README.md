# Self-Hosting Parity

This document defines the gate for moving AICore toward a compiler written in AICore while preserving the current Rust compiler behavior.

## Scope

The self-hosting path is compiler work only:

- compiler support libraries live outside `std/`
- tool entrypoints import compiler libraries instead of embedding compiler logic
- the Rust compiler remains the reference until parity gates pass
- no new core-language semantics are introduced by the self-hosting work
- runtime, protocol, service, and application helpers stay in separate libraries

Supported operation, failure triage, rollback, and GitHub evidence rules are maintained in `docs/selfhost/supported-operation-runbook.md`. Use that runbook before declaring a self-hosting issue complete.

Suggested package boundaries:

- `compiler/aic/libs/source`
- `compiler/aic/libs/diagnostics`
- `compiler/aic/libs/syntax`
- `compiler/aic/libs/lexer`
- `compiler/aic/libs/parser`
- `compiler/aic/libs/ast`
- `compiler/aic/libs/ir`
- `compiler/aic/libs/frontend`
- `compiler/aic/libs/semantics`
- `compiler/aic/libs/typecheck`
- `compiler/aic/libs/backend_llvm`
- `compiler/aic/libs/driver`
- `compiler/aic/tools/aic_selfhost`
- `compiler/aic/tools/aic_parity`

Current implemented package slice:

- `compiler/aic/libs/source` models source IDs, spans, locations, range operations, and source-span relations.
- `compiler/aic/libs/diagnostics` models diagnostic severity, diagnostic codes, help text, text edits, fixes, and primary diagnostic records.
- `compiler/aic/libs/syntax` models token kinds, token spans, lexemes, and token classification helpers, including the core visibility keywords used by the Rust front end.
- `compiler/aic/libs/lexer` scans the current ASCII lexical surface for identifiers, keywords, numeric/string/char/template literals, comments, whitespace, delimiters, and operator spellings into `compiler.syntax` tokens and EOF-terminated token streams.
- `compiler/aic/libs/parser` models the token-stream cursor, expectation diagnostics, dotted module-path parsing, `module`/`import` declaration parsing and ordering, visibility parsing, top-level item header parsing, structured type-reference parsing for unit, hole, named/path, generic application, tuple, `Fn(...) -> ...`, and `dyn Trait` types, parameter-list parsing, function-signature parsing with structured generic parameters, where-clause bound merging, and optional effects/capabilities lists, expression parsing with precedence for unary, binary, call, field-access, `await`, and `?` forms, structured `for` expressions with iterable and body child nodes, pattern parsing for wildcard, literal, tuple, variant, struct, and or-pattern forms, block parsing with let/assignment/return/expression statements and tail expressions, function-declaration parsing with structured requires/ensures expressions and bodies, struct-declaration parsing with structured field default and invariant expressions, enum-declaration parsing with optional single-type variant payloads, `type`/`const` declaration parsing with structured const initializer expressions, trait-declaration parsing with method signatures and contract rejection parity, impl-declaration parsing with optional generic parameters, structured method signatures, method-level requires/ensures expressions, and method bodies, top-level and nested declaration attribute parsing with framework-attribute validation, item recovery with deterministic diagnostic ordering and max-error truncation, and full program parsing with source-map entries for later front-end stages.
- `compiler/aic/libs/ast` models AST names, module paths, module declarations, import declarations, top-level item headers, attributes and attribute arguments on items, params, fields, variants, and method signatures, function signatures with parameters and generic parameter lists, structured expression, pattern, statement, block, and match-arm nodes, function declarations with contract expressions and bodies, struct declarations with fields/default/invariant expressions, enum declarations with variants, type alias declarations, const declarations with initializer expressions, trait declarations, impl declarations with generic parameters and method bodies, program items with item visibility, full program AST roots, source-map entries, flat structured type descriptors with type-node metadata, and literal descriptors.
- `compiler/aic/libs/ir` models stable IR node, symbol, type, source-map, item, function, block, statement, expression, pattern, struct, enum, trait, impl, alias, const, and generic-instantiation records, lowers checked AST programs into deterministic self-hosted IR, serializes canonical self-host IR JSON/debug/report artifacts for parity comparisons, validates schema/source-map/order metadata before serialization, preserves non-runtime alias/const surfaces without leaking them as runtime functions, and emits stable diagnostics for unsupported const initializer forms, failed semantic/typecheck preconditions, invalid type metadata, missing lowering payloads, and invalid serialization contracts.
- `compiler/aic/libs/frontend` models self-hosted resolver output for modules, imports, symbols, references, type/value/member namespaces, deterministic symbol IDs, duplicate diagnostics, missing import/module diagnostics, trait impl discovery, enum variant discovery, and same-module versus imported visibility checks.
- `compiler/aic/libs/semantics` models self-hosted semantic output for generic parameter environments, trait-bound resolution, generic arity validation, trait and trait-method indexes, inherent and trait impl indexes, conflicting impl metadata, and deterministic semantic diagnostics over resolved AST units.
- `compiler/aic/libs/typecheck` consumes resolver and semantic outputs, checks signatures, constants, local bindings, assignments, expressions, function and variant calls, named arguments, struct literals and field access, tuple expressions and tuple field access, match guards and exhaustiveness, loops with typed break-value stack tracking, numeric width constraints, generic instantiations, trait-bound dispatch, effect/capability declarations, direct and transitive effect usage, capability authority, contract Bool requirements, contract purity, static contract discharge notes, residual runtime contract obligations, local move/use tracking, reinitialization after move, shared and mutable borrow conflicts, assignment while borrowed, mutable borrow of immutable bindings, and terminal resource protocol reuse, and produces typed function, binding, and instantiation metadata for later IR lowering.
- `compiler/aic/libs/backend_llvm` consumes deterministic self-host IR and emits deterministic LLVM-text backend artifacts for the backend-covered corpus. The package models backend options, artifact kind, native-link metadata, backend symbols, artifact naming, feature summaries, and diagnostics. It emits real LLVM functions for primitive executable functions, backend-covered aggregate signatures, struct-literal returns, explicit return statements including non-terminal early returns, branch-local returns in backend-supported block expressions before unreachable unit tails, integer range `for name in start..end { ... }` statements with backend-covered unit bodies, iterator-style `for name in values { ... }` statements over backend-covered `Vec[Int]` values, unit `break;` and `continue;` branches in backend-covered `while` and `for` loop bodies, backend-covered `loop { ... }` unit statements and typed `break value` exits before unreachable unit tails, runtime-backed `String` and filesystem intrinsic result paths, integer arithmetic, direct primitive calls, literal returns, static literal matches, and metadata-backed struct, enum, tuple, generic-definition, closure, async/future, trait/impl, const/global, resource-handle, and native-link surfaces. It rejects unsupported executable statement or return-expression shapes, iterator-style `for` loops over unsupported operand or element types, ABI forms with no deterministic LLVM layout, invalid native-link metadata, invalid IR schema/source/name metadata, missing entry points, and native materialization requests that must be completed by the driver after LLVM IR emission.
- `compiler/aic/libs/driver` orchestrates the self-host compiler phases over source/package inputs. It parses, resolves, analyzes semantics, typechecks, lowers IR, emits backend artifacts, formats diagnostics, reads package `main` metadata, and exposes command-level results for check, IR JSON, build, and guarded direct-library run requests.
- `compiler/aic/tools/aic_selfhost` is the self-host candidate executable. It reads `.aic` files or package directories, invokes `compiler.driver`, materializes backend LLVM through `clang` for `build` and `run`, links the existing runtime C source parts for native executable support, and returns deterministic nonzero diagnostics for unsupported command shapes or native materialization failures.
- `compiler/aic/tools/source_diagnostics_check` imports the implemented libraries and validates the data model through a small executable tool, including typecheck positive and negative cases for generics, structs, enums, traits, impl methods, tuple/closure/async signatures, aliases, constants, numeric widths, named arguments, match guards, exhaustiveness, trait-bound failures, declared/missing/transitive effects, capability authority, invalid effect/capability declarations, function and impl-method contract Bool failures, effectful contract rejection, static contract discharge, residual contract notes, move/use tracking, borrow conflicts, branch-local borrow false-positive prevention, resource use-after-close, terminal resource reuse, positive IR lowering for functions, structs/defaults/invariants, enums, generics, traits/impls, closures, async signatures, loops, matches, aliases, constants, tuple types, effects, and capabilities, negative IR lowering diagnostics for unsupported const initializer forms, unresolved semantic preconditions, invalid type metadata, and missing lowering hooks, positive/negative IR serialization validation for deterministic JSON, debug text, parity artifacts, malformed schema metadata, missing source maps, and unstable ordering, positive/negative backend validation for deterministic LLVM artifacts, symbol naming, feature metadata, native-link metadata, missing lowering hooks, unsupported ABI/type forms, invalid link metadata, invalid IR inputs, empty artifact names, and native materialization requests, plus positive/negative driver validation for command results, manifest main-path metadata, build artifact text, unsupported commands, and guarded library-level run diagnostics.

## Parity Harness

The initial gate is:

```bash
make test-selfhost
make selfhost-parity
```

`make test-selfhost` tests the parity harness with deterministic test compiler scripts. It does not depend on the current `aic` binary.

`make selfhost-parity` compares a reference compiler command against a candidate compiler command using `tests/selfhost/parity_manifest.json`. By default, both sides use the current Rust compiler command:

```bash
make selfhost-parity
```

When an AICore-built compiler exists, configure the candidate and, for the T12 driver slice, use the driver-specific covered manifest:

```bash
SELFHOST_PARITY_MANIFEST=tests/selfhost/aic_selfhost_driver_manifest.json \
SELFHOST_CANDIDATE=target/aic_selfhost_t12 \
make selfhost-parity
```

For the T13 Rust-vs-self-host conformance gate, build the candidate and run the expanded core manifest with:

```bash
make selfhost-parity-candidate
```

That target builds `compiler/aic/tools/aic_selfhost` to `target/aic_selfhost_candidate` and runs `tests/selfhost/rust_vs_selfhost_manifest.json` against the Rust reference compiler.

The report is written to `target/selfhost-parity/report.json`.

The production Rust-vs-self-host conformance manifest is tracked with a coverage map in `tests/selfhost/conformance_coverage.json`. See `docs/selfhost/conformance.md` for the required coverage areas, case policy, and update workflow.

The latest staged compiler is also validated against package, package-member, workspace-probe, and core-example command surfaces by `tests/selfhost/stage_matrix_manifest.json`. See `docs/selfhost/stage-matrix.md` for the matrix scope, report format, and add-case workflow.

For `ir-json` actions, the parity harness parses both compiler outputs as JSON and compares canonical JSON fingerprints. This keeps IR parity stable across harmless whitespace or object-key ordering differences while still failing on malformed IR JSON, schema/contract mismatches, or actual semantic output differences. The report records the comparison kind, raw command metadata, canonical JSON fingerprints, and any JSON parse error for both reference and candidate commands.

The T12/T13 self-host manifests use `selfhost-ir-json` comparison for the self-host IR schema because `aic_selfhost` intentionally exposes the self-host IR contract while the Rust reference still exposes the legacy reference IR schema. They use `artifact-exists` comparison for `build` because the T12/T13 candidate materializes a native executable through the self-host LLVM artifact path, while byte-for-byte native binary parity is reserved for the cutover issue.

Negative conformance cases can use `diagnostic-code` comparison to require matching primary diagnostic codes while still recording the full stdout/stderr fingerprints and diffs. Reports include command lines, exit status, artifact paths for build actions, diagnostic code lists, and unified stdout/stderr diffs for mismatches.

For default `build` actions, the parity harness compares artifact presence and fingerprints. Cases can opt into `artifact-exists` while the self-host driver is validating materialization through a different native codegen path.

## Required Coverage

The parity manifest should grow in lockstep with the AICore compiler port. It must cover:

- pass and fail frontend diagnostics
- lexer and parser recovery
- canonical formatting
- canonical IR JSON
- resolver and visibility errors
- type, effect, borrow, pattern, and contract checks
- LLVM emission
- executable behavior for representative examples
- deterministic output across repeated runs

Each porting issue must add manifest cases for the compiler surface it implements and keep the existing Rust compiler output unchanged.

## Bootstrap Readiness

The final self-hosting gate uses a staged bootstrap report:

```bash
make selfhost-bootstrap-report
```

This bounded report command builds `stage0` with the Rust reference compiler, attempts to build `stage1` with `stage0`, attempts to build `stage2` with `stage1` when `stage1` exists, runs the expanded Rust-vs-self-host parity manifest with the latest available stage compiler, runs the stage compiler matrix, and writes `target/selfhost-bootstrap/report.json`. By default stage0 uses `cargo run --quiet --bin aic --`; set `AIC_SELFHOST_STAGE0` or `AIC` to point at an already-built reference compiler when the host environment runs the bootstrap gate from a prebuilt toolchain.

The report starts with a `host-preflight` step on every host. It records the required toolchain surface before any stage compiler is built: `cargo`, `clang`, `strip`, and, on macOS, `codesign`. On macOS the same step also records Developer Mode state. The self-host materializer ad-hoc signs Mach-O outputs after `clang` links them, so disabled Developer Mode is not an automatic bootstrap failure for AICore-built artifacts. If an externally produced unsigned artifact hangs in `_dyld_start`, enable Terminal as a developer tool with `spctl developer-mode enable-terminal`, approve it in System Settings > Privacy & Security > Developer Tools, restart the terminal/Codex session, and rerun the bootstrap gate.

The report distinguishes compiler modes:

- `experimental`: stage0, parity, and the stage compiler matrix can be exercised, but stage1/stage2 failures, reproducibility failures, stage matrix regressions, or resource-budget violations keep the self-host compiler unsupported.
- `supported`: stage0, stage1, stage2, parity, the stage compiler matrix, stage1/stage2 reproducibility, and resource budgets must pass. Native artifacts may match exactly, or may match after platform strip normalization when the only recorded difference is non-loadable symbol/debug table data. Linux uses `strip --strip-all`; macOS uses `strip -S -x`.
- `default`: the supported gate must pass, followed by explicit release approval.

The supported gate does not by itself make the self-host compiler the default compiler path. Default-mode selection remains blocked until the explicit cutover issue is implemented and closed with evidence.

Compiler implementation selection is explicit except for the controlled AICore compiler source cutover. Use `aic release selfhost-mode --mode supported --check` to verify supported self-host evidence, `aic release selfhost-mode --mode default --check --approve-default` after default approval, `aic build compiler/aic/tools/aic_selfhost -o target/selfhost-default/aic_selfhost` to validate the unmodified default compiler source path, and `AIC_COMPILER_MODE=fallback aic build <input> -o <artifact>` to force the Rust reference fallback. Compiler-source builds with target, release, optimization, artifact, link, debug, offline, verify-hash, or manifest modifiers remain on the reference path unless a self-host compiler mode is explicit.

The bootstrap report includes a `performance` object with total duration, per-step duration, maximum produced artifact size, reproducibility comparison duration, and the maximum child-process peak RSS observed by the gate. Production budgets come from the checked-in manifest at `docs/selfhost/bootstrap-budgets.v1.json`; the report records the manifest path, schema version, platform entry, baseline values, active budgets, and local overrides under `performance.budget_source`.

The gate also writes performance-specific artifacts:

- `target/selfhost-bootstrap/performance-report.json`
- `target/selfhost-bootstrap/performance-trend.json`

The trend report is the review artifact for comparing a run against the Linux/macOS baselines without scraping logs. See `docs/selfhost/performance.md` for metric meanings, current baselines, local override rules, and the budget update process. Set any budget override to `0`, `off`, `none`, or `disabled` only for local investigation; supported readiness requires `performance.ok` to be `true`, and release evidence must use the checked-in production defaults.

The release-blocking command is:

```bash
make selfhost-bootstrap
```

That command exits nonzero until the supported criteria are met. It must not be bypassed by copying stage artifacts, reusing the Rust compiler for later stages, or treating missing stage1/stage2 artifacts as success.

The current Linux/macOS bootstrap status is supported-ready when the gate is run with a working reference compiler, `clang`, `strip`, and macOS `codesign` where applicable. `aic build compiler/aic/tools/aic_selfhost` produces a real stage0 compiler, stage0 emits a runnable stage1 compiler, stage1 emits a runnable stage2 compiler, the latest stage compiler passes the expanded Rust-vs-self-host parity manifest, the latest stage compiler passes the package/workspace/core-example matrix, stage1/stage2 runtime artifacts match exactly or after stripping non-loadable symbol/debug tables, and the resource-budget report passes. The backend-covered executable surface includes primitive functions, backend-covered aggregate signatures, return-position struct literals, lossless fixed-width integer widening at return boundaries, runtime-backed string replacement and `string.join`, escaped string-literal emission, vector construction/push/length/get support for backend-covered values, direct parser-shaped `Some`/`None` match returns over `vec.get`, runtime-backed filesystem result construction for direct `read_text`/`temp_file`/`write_text`/`delete` returns, runtime-backed process execution and argument lookup for the self-host driver, return-position primitive/string conditionals, match/loop lowering for the compiler package graph, field/local value lowering, and stdout/stderr string printing.

## Release Provenance

After `make selfhost-bootstrap` succeeds, release review must generate machine-readable self-host artifact provenance:

```bash
make selfhost-release-provenance
```

This target writes `target/selfhost-release/provenance.json`, verifies it, and records checksums for the canonical stage2 release artifact, stage0/stage1/stage2 compilers, the bootstrap report, the parity report, the stage-matrix report, and performance reports. Canonical release artifacts are named `aicore-selfhost-compiler-<platform>-<arch>` for Linux and macOS. The provenance format is `aicore-selfhost-release-provenance-v1`.

The provenance gate fails when required reports or artifacts are missing, when checksums no longer match the bootstrap report, when stage1/stage2 reproducibility did not pass, when performance budget overrides were used, or when the host platform is unsupported. See `docs/selfhost/release-provenance.md` for schema details and verification commands.

## Rust Reference Retirement

Rust-reference removal is tracked separately from supported/default self-host operation. The decision record is `docs/selfhost/rust-reference-retirement.md`, and the checked inventory is `docs/selfhost/rust-reference-retirement.v1.json`.

Run the consistency audit with:

```bash
make selfhost-retirement-audit
```

That target writes `target/selfhost-retirement/report.json` and passes while the inventory, docs, rollback commands, rollback validation schema, tracked Rust/Cargo path classification, and every `retirement_decision` entry are internally consistent. It does not approve removal. The stronger approval gate remains blocked until the issue `#419` decision, bake-in evidence, rollback validation evidence, and replacement/retention mapping are complete:

```bash
python3 scripts/selfhost/retirement_audit.py --require-approved
```

Passing bake-in entries must be machine-verifiable: each entry records `make release-preflight`, `make ci`, source commit, supported bootstrap report checksum, release provenance checksum, and default compiler-source build artifact checksum. Empty or failed entries do not count toward Linux/macOS bake-in. Rollback evidence is also machine-verifiable through `rollback.validation_evidence`, including the restore source, checkout/build/audit commands, and checksums for the build log, retirement audit report, and marker scan report.

Class decisions are machine-verifiable through `rust_path_classes[*].retirement_decision`. Rust reference compiler classes must use `remove-after-replacement`, while retained Rust host/tooling/test classes must name a non-reference role and provide evidence for each required command before approval. After removal, a `retired` manifest expects approved removal class paths to be absent from the repository.

Generate review entries with `scripts/selfhost/retirement_evidence.py` after the corresponding commands have run. The helper records checksums for bake-in, rollback, and class decision evidence and can assemble a candidate manifest under `target/selfhost-retirement/`.

Use `--path-base <bundle>` when creating evidence entries and `python3 scripts/selfhost/retirement_audit.py --evidence-root <bundle>` when auditing a candidate manifest whose reports, logs, and compiler artifacts are stored in a separate release evidence bundle.

Generate final reference-scan evidence with `scripts/selfhost/retirement_reference_scan.py` after the removal classes are approved in the candidate manifest. The report format is `aicore-rust-reference-retirement-reference-scan-v1`; it must be attached as class evidence for `repository-wide reference scan` and must have no active docs, scripts, tests, workflow, `Makefile`, or `README.md` findings.

## CI and Release Gates

GitHub CI runs the production self-host bootstrap gate in `.github/workflows/ci.yml` as `Self-Host Bootstrap (${{ matrix.os }})` on `ubuntu-latest` and `macos-latest`. The job installs `clang` on Linux, runs the host tool preflight, then runs the same supported-mode command used locally:

```bash
make selfhost-bootstrap
```

CI and release workflows use the checked-in production budgets from `docs/selfhost/bootstrap-budgets.v1.json`. They set only the bootstrap process timeout so long supported runs can finish and report precise manifest budget failures:

```bash
AIC_SELFHOST_BOOTSTRAP_TIMEOUT=3600 make selfhost-bootstrap
```

The release workflow has a separate `Release Self-Host Bootstrap (${{ matrix.os }})` matrix for `ubuntu-latest` and `macos-latest`; release builds depend on that matrix succeeding. Local release validation uses the same host command contract through:

```bash
make release-preflight
```

`make release-preflight` runs the full local CI gate plus the supported self-host bootstrap gate for the current host before reproducibility and security checks.
It also runs `make selfhost-release-provenance` so local release dry runs produce the same self-host artifact metadata that CI uploads.
Release preflight then runs `make selfhost-mode-check`, `make selfhost-default-mode-check`, and `make selfhost-default-build-check`, which report supported/default self-host mode, block unsupported self-host readiness claims, and validate that the controlled AICore compiler source build uses the self-host compiler by default.

Both CI and release workflows upload self-host artifacts even when the gate fails. Inspect these artifact names first:

- `selfhost-bootstrap-ubuntu-latest`
- `selfhost-bootstrap-macos-latest`
- `release-selfhost-bootstrap-ubuntu-latest`
- `release-selfhost-bootstrap-macos-latest`

Each artifact contains `target/selfhost-bootstrap/report.json`, `target/selfhost-bootstrap/performance-report.json`, `target/selfhost-bootstrap/performance-trend.json`, `target/selfhost-bootstrap/parity-report.json`, `target/selfhost-bootstrap/stage-matrix-report.json`, stage compiler outputs, parity artifacts, stage-matrix artifacts, and `target/selfhost-release/**` when those files were produced. The report `host`, `steps`, `reproducibility`, and `performance` fields are the primary evidence for platform details, stage exit codes, strip normalization, resource budgets, and readiness status. The release provenance file ties those reports to the canonical self-host compiler artifact, source commit, toolchain versions, and checksums.

For troubleshooting:

- Inspect `target/selfhost-bootstrap/report.json` for command lines, exit codes, timeouts, stdout/stderr, artifact paths, artifact sizes, SHA-256 digests, peak RSS, and resource-budget violations.
- Inspect `target/selfhost-bootstrap/performance-trend.json` for platform baselines, active budgets, observed top-level metrics, per-step metrics, and local budget overrides.
- Inspect `target/selfhost-bootstrap/stage-matrix-report.json` for package, package-member, workspace-probe, and core-example stage compiler results.
- Inspect `target/selfhost-release/provenance.json` and verify it with `python3 scripts/selfhost/release_provenance.py verify`.
- Run `target/selfhost-bootstrap/stage0/aic_selfhost check <package>` to isolate package-level front-end failures.
- Run `make selfhost-parity-candidate` to validate the currently supported Rust-vs-self-host parity corpus independently of bootstrap.
- Run `make selfhost-stage-matrix` to validate the latest stage compiler independently of bootstrap.
- Keep self-hosting implementation issues open while the report status is `experimental`; unsupported stages are not accepted as done.
