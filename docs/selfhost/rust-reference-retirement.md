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

The current blockers are intentional: approval required before Rust reference removal, release bake-in evidence, replacement or retained-role decisions, and rollback validation evidence.

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

The retirement audit classifies tracked Rust and Cargo-owned paths into these classes. Each manifest entry also has a `retirement_decision` object so removal and retention are reviewed with the same evidence contract.

| Class | Current role | Current removal state | Replacement owner |
|---|---|---|---|
| `rust-reference-compiler-core` | Reference parser, resolver, diagnostics, typing, IR, formatter, and semantic checks. | Not allowed | `compiler/aic/libs/{source,diagnostics,syntax,lexer,parser,ast,ir,frontend,semantics,typecheck}` |
| `rust-reference-backend-runtime` | Reference LLVM backend generation and native runtime materialization behavior. | Not allowed | `compiler/aic/libs/backend_llvm` and `compiler/aic/tools/aic_selfhost` native materialization |
| `rust-host-cli-release-packaging` | Host command surface, package workflow, release checks, sandbox, project, and toolchain support. | Not allowed | Post-retirement host tooling or narrowed Rust host crate with documented non-reference role |
| `rust-developer-tooling` | Developer commands, conformance, docs, LSP, profiling, migration, and test generation. | Not allowed | Separate developer-tooling issues or documented retained Rust tooling |
| `rust-test-suites` | Integration and regression tests that define the compatibility contract. | Not allowed | Post-retirement test harness or retained Rust test crate with documented scope |

The audit fails if a tracked Rust or Cargo path is not covered by at least one documented ownership decision. Overlap is allowed only where the path participates in more than one review surface; any such overlap is visible in the generated report.

## Class Decision Evidence

Each `rust_path_classes` entry must declare a `retirement_decision`:

- `intent`: `remove-after-replacement` for Rust reference compiler behavior that must disappear after replacement, or `retain-non-reference` for Rust code that may remain as host tooling, release tooling, bootstrap support, tests, or other non-reference infrastructure.
- `status`: `pending` until the decision has complete evidence, or `approved` only after every required command has verified evidence.
- `non_reference_role`: required when `intent` is `retain-non-reference`.
- `evidence`: one entry per required command, each with `command`, `recorded_at`, `report`, and `report_sha256`.

The audit verifies every class evidence checksum. A class decision cannot be `approved` unless every command listed in `required_replacement_evidence` has a matching evidence entry. For `remove-after-replacement`, `removal_allowed` must also be true before approval is accepted. Until then, `python3 scripts/selfhost/retirement_audit.py --require-approved` reports the class as a blocker.

## Evidence Collection Helper

Use `scripts/selfhost/retirement_evidence.py` to create checksum-bearing evidence entries after the required commands have actually run. The helper writes JSON entries for later review; it does not approve retirement unless the caller explicitly asks for an assembled candidate manifest with an approver.

Create a bake-in entry from a successful platform run:

```bash
python3 scripts/selfhost/retirement_evidence.py bake-in-entry \
  --platform macos \
  --source-commit <commit> \
  --recorded-at <timestamp> \
  --bootstrap-report target/selfhost-bootstrap/report.json \
  --release-provenance target/selfhost-release/provenance.json \
  --default-build-artifact target/selfhost-default/aic_selfhost \
  --out target/selfhost-retirement/bake-in-macos.json
```

Create rollback and class decision entries:

```bash
python3 scripts/selfhost/retirement_evidence.py rollback-entry \
  --source-ref <last-rust-reference-tag> \
  --source-commit <commit> \
  --recorded-at <timestamp> \
  --cargo-build-log target/selfhost-retirement/rollback-cargo-build.log \
  --retirement-audit-report target/selfhost-retirement/rollback-audit.json \
  --marker-scan-report target/selfhost-retirement/rollback-marker-scan.txt \
  --out target/selfhost-retirement/rollback-entry.json

python3 scripts/selfhost/retirement_evidence.py class-entry \
  --command "make selfhost-bootstrap" \
  --recorded-at <timestamp> \
  --report target/selfhost-bootstrap/report.json \
  --out target/selfhost-retirement/class-bootstrap.json
```

Assemble a candidate manifest for final review only after all required evidence files exist:

```bash
python3 scripts/selfhost/retirement_evidence.py assemble-manifest \
  --manifest docs/selfhost/rust-reference-retirement.v1.json \
  --bake-in-entry target/selfhost-retirement/bake-in-macos.json \
  --rollback-entry target/selfhost-retirement/rollback-entry.json \
  --class-entry rust-reference-compiler-core=target/selfhost-retirement/class-bootstrap.json \
  --out target/selfhost-retirement/approved-manifest.json
```

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

## Rollback Validation Evidence

Rollback validation evidence in `docs/selfhost/rust-reference-retirement.v1.json` must prove that the recorded source can restore the Rust reference implementation and that the restored checkout still passes the retirement audit consistency gate. The manifest keeps this in `rollback.validation_evidence`.

A valid entry must include:

- `source_ref`: the tag or branch used as the restore source
- `source_commit`: the exact commit resolved from that restore source
- `recorded_at`: the review timestamp or CI run timestamp
- `commands`: including `git fetch --tags origin`, a `git checkout <source_ref> -- ...` command that restores every `rollback.restore_paths` entry, `cargo build --locked`, and `make selfhost-retirement-audit`
- `cargo_build_log` and `cargo_build_sha256`
- `retirement_audit_report` and `retirement_audit_sha256`
- `marker_scan_report` and `marker_scan_sha256`

The audit verifies each evidence checksum and verifies that the retirement audit report has format `aicore-rust-reference-retirement-audit-v1` with no consistency problems. `rollback.validated` must remain `false` until at least one valid restore evidence entry is recorded, and `python3 scripts/selfhost/retirement_audit.py --require-approved` must remain blocked while rollback validation is missing.

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
