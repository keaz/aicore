# Rust Reference Compiler Retirement Decision Record

Removal Status: Deferred

This record covers issue `#419`: deciding whether and when the Rust reference compiler can be deprecated or removed after the controlled self-host default cutover.

## Decision

Rust reference compiler removal is not approved yet. The controlled default self-host path is available for the AICore compiler source build, but the repository still needs release bake-in evidence, explicit approval, and a complete replacement or retention decision for every Rust-owned path before any deletion is allowed.

The machine-readable inventory for this decision is `docs/selfhost/rust-reference-retirement.v1.json`. Validate it with:

```bash
make selfhost-retirement-audit
```

The stronger approval gate must continue to fail until every blocker is cleared:

```bash
python3 scripts/selfhost/retirement_audit.py --require-approved
```

The current blocker is intentional: approval required before Rust reference removal.

## Bake-In Evidence Format

Bake-in evidence entries in `docs/selfhost/rust-reference-retirement.v1.json` must be machine-verifiable. A passing entry must include:

- `platform`: `linux` or `macos`
- `status`: `passed`
- `source_commit`: the exact source commit used for the release preflight
- `recorded_at`: the review timestamp or CI run timestamp
- `release_preflight_command`: `make release-preflight`
- `ci_command`: `make ci`
- `bootstrap_report` and `bootstrap_report_sha256`
- `release_provenance` and `release_provenance_sha256`
- `default_build_artifact` and `default_build_sha256`

The audit verifies that the bootstrap report is `aicore-selfhost-bootstrap-v1`, is `supported-ready`, matches the evidence platform, and uses production budget defaults. It verifies that release provenance is `aicore-selfhost-release-provenance-v1`, has passing validation fields, records a clean worktree, matches the evidence source commit and platform, and points to a canonical artifact with a matching checksum. It also verifies that the default-build artifact exists and matches the recorded checksum.

Failed evidence entries can be recorded for history, but they do not count toward bake-in. They must include a `failure_summary`.

## Scope

This decision is limited to Rust reference compiler retirement. It does not change AICore language semantics, standard library APIs, runtime behavior, editor tooling behavior, or package-manager behavior.

Rust code can remain after reference compiler retirement when it has a documented non-reference role such as host packaging, release orchestration, bootstrap support, native runtime integration, or tests. Any remaining Rust role must be named and verified before this issue can close.

## Inventory Classes

The retirement audit classifies tracked Rust and Cargo-owned paths into these classes:

| Class | Current role | Current removal state | Replacement owner |
|---|---|---|---|
| `rust-reference-compiler-core` | Reference parser, resolver, diagnostics, typing, IR, formatter, and semantic checks. | Not allowed | `compiler/aic/libs/{source,diagnostics,syntax,lexer,parser,ast,ir,frontend,semantics,typecheck}` |
| `rust-reference-backend-runtime` | Reference LLVM backend generation and native runtime materialization behavior. | Not allowed | `compiler/aic/libs/backend_llvm` and `compiler/aic/tools/aic_selfhost` native materialization |
| `rust-host-cli-release-packaging` | Host command surface, package workflow, release checks, sandbox, project, and toolchain support. | Not allowed | Post-retirement host tooling or narrowed Rust host crate with documented non-reference role |
| `rust-developer-tooling` | Developer commands, conformance, docs, LSP, profiling, migration, and test generation. | Not allowed | Separate developer-tooling issues or documented retained Rust tooling |
| `rust-test-suites` | Integration and regression tests that define the compatibility contract. | Not allowed | Post-retirement test harness or retained Rust test crate with documented scope |

The audit fails if a tracked Rust or Cargo path is not covered by at least one documented ownership decision. Overlap is allowed only where the path participates in more than one review surface; any such overlap is visible in the generated report.

## Approval Criteria

Removal can be considered only after all of these are true:

1. The decision manifest status is changed from `deferred` to `approved`.
2. `approval.approved` is true and an approver is recorded.
3. The bake-in evidence records at least two passing release preflight runs across Linux and macOS.
4. Every Rust path class is either marked `removal_allowed=true` with replacement evidence or retained with a documented non-reference role.
5. `cargo build --locked`, `make ci`, `make release-preflight`, `make selfhost-bootstrap`, `make examples-check`, `make examples-run`, and `make docs-check` pass under the post-decision command contract.
6. A repository-wide reference scan confirms active docs, scripts, tests, and workflows no longer point at removed Rust reference paths.
7. Rollback or restore instructions are validated from a tag or branch that contains the last Rust reference implementation.

## Rollback Source

The rollback source is the last tagged release or branch that still contains the Rust reference compiler. Before deletion, release review must record that source in the issue evidence and verify the restore path:

```bash
git fetch --tags origin
git checkout <last-rust-reference-tag> -- Cargo.toml Cargo.lock src tests
cargo build --locked
make selfhost-retirement-audit
```

For runtime incident fallback while Rust remains in the repository, use:

```bash
AIC_COMPILER_MODE=fallback aic build <input> -o <artifact>
```

After deletion, rollback must restore the last approved Rust reference source or roll the release branch back to the tagged artifact set.

## Closure Evidence

Do not close issue `#419` until the evidence comment includes:

- the approved decision manifest and report from `target/selfhost-retirement/report.json`
- the bake-in release preflight runs for Linux and macOS
- the machine-verified bootstrap, release provenance, and default-build artifact checksums for each passing bake-in entry
- the exact Rust paths removed and the exact Rust paths retained
- replacement owners and tests for every removed class
- `cargo build --locked`, `make ci`, `make release-preflight`, `make selfhost-bootstrap`, `make examples-check`, `make examples-run`, and `make docs-check`
- rollback validation from the recorded source
- marker scan output for every touched path
