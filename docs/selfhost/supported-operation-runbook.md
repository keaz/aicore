# Supported Self-Host Operation Runbook

This runbook is the maintainer procedure for operating the AICore self-host compiler as a supported compiler path. It covers only the core compiler, bootstrap artifacts, parity gates, release provenance, and the evidence needed before closing self-hosting work.

Runtime libraries, protocol adapters, editor extensions, service integrations, and application packages are separate work streams. They can use self-hosted compiler artifacts after the compiler gate passes, but they are not prerequisites for declaring the core compiler self-host path supported.

## Operating Modes

| Mode | Meaning | Allowed use | Required gate |
|---|---|---|---|
| `experimental` | Stage0 or later self-host compiler artifacts can be built for investigation, but readiness gaps are still allowed. | Local diagnosis, report generation, and issue investigation. | `make selfhost-bootstrap-report` |
| `supported` | The self-host compiler is a supported compiler path on the current Linux or macOS host. | Maintainer validation, CI release evidence, and release artifact review. | `make selfhost-bootstrap` and `make selfhost-release-provenance` |
| `default` | The self-host compiler is selected as the normal compiler path for the controlled AICore compiler source build. | Building `compiler/aic/tools/aic_selfhost` after default-mode cutover evidence exists. | Supported gate plus the cutover issue acceptance criteria |
| `fallback` | Maintainers intentionally use the Rust reference compiler because a self-host gate failed or a platform is outside the supported matrix. | Triage, release blocking, or keeping existing Rust-reference behavior available. | Failing self-host evidence is attached to the issue or release review |
| `rollback` | A previous self-host promotion is backed out to the last known passing compiler path. | Production incident response or release candidate rejection. | Rollback evidence, failing report links, and a new follow-up issue |

Self-hosting is the controlled default for `aic build compiler/aic/tools/aic_selfhost -o <artifact>` after the default-mode cutover evidence is present. That implicit default is intentionally limited to the unmodified executable build shape; compiler-source builds with target, artifact, link, debug, release, optimization, offline, verify-hash, or manifest modifiers keep the documented reference behavior unless maintainers pass an explicit compiler mode. Other `.aic` inputs also keep the documented reference behavior unless maintainers pass an explicit compiler mode. A supported Linux/macOS gate means the self-host compiler can be validated and shipped as a supported artifact; the default build gate proves the cutover path separately.

## Clean Checkout Readiness Path

From a clean checkout on Linux or macOS, run these commands in order:

```bash
cargo build --locked
cargo test --locked --test selfhost_parity_tests
make docs-check
make examples-check
make examples-run
make selfhost-parity-candidate
make selfhost-bootstrap
make selfhost-release-provenance
make selfhost-default-mode-check
make selfhost-default-build-check
make selfhost-retirement-audit
make release-preflight
make ci
```

For long CI-equivalent local runs, keep the production budget defaults and only extend the process timeout:

```bash
AIC_SELFHOST_BOOTSTRAP_TIMEOUT=3600 make selfhost-bootstrap
```

Do not set resource-budget override variables for release evidence. Supported readiness requires the checked-in production defaults from `docs/selfhost/bootstrap-budgets.v1.json`.

Check the active compiler mode policy with:

```bash
aic release selfhost-mode --mode reference --check --json
aic release selfhost-mode --mode supported --check
aic release selfhost-mode --mode default --check --approve-default
```

`reference` and `fallback` use the Rust reference compiler path and do not require self-host evidence. `supported` requires the supported bootstrap report, parity report, package/workspace matrix report, performance evidence, and release provenance to pass. `default` requires the same evidence plus explicit default approval. The release and preflight gates use `--approve-default` only after the controlled default cutover is approved.

Use the compiler-mode selector only when the build path needs an explicit implementation choice:

```bash
AIC_COMPILER_MODE=fallback aic build examples/e5/hello_int.aic -o target/fallback-hello
AIC_SELFHOST_COMPILER=target/selfhost-release/aicore-selfhost-compiler-<platform>-<arch> \
  aic build examples/e5/hello_int.aic -o target/selfhost-hello --compiler-mode supported
aic build compiler/aic/tools/aic_selfhost -o target/selfhost-default/aic_selfhost
```

The `experimental` mode can route through a local self-host compiler for investigation without claiming supported or default readiness. The absent mode selector intentionally defaults to self-host only for the unmodified `compiler/aic/tools/aic_selfhost` executable build after default evidence exists. Fallback validation must use `reference` or `fallback` mode.

Use this scanner before closing a self-hosting issue. It intentionally builds the pattern from adjacent shell strings so the runbook does not match itself:

```bash
AIC_MARKER_PATTERN='TO''DO|du''mmy|st''ub|un''implemented|panic\("to''do|FIX''ME'
rg -n "$AIC_MARKER_PATTERN" <touched paths>
```

The scan must produce no output for the touched documentation, scripts, tests, compiler packages, and workflow files.

## Host Setup

Linux prerequisites:

```bash
command -v cargo
command -v clang
command -v strip
```

On Debian or Ubuntu runners, install the native toolchain with:

```bash
sudo apt-get update
sudo apt-get install -y build-essential clang binutils
```

Linux normalization uses:

```bash
strip --strip-all
```

macOS prerequisites:

```bash
xcode-select --install
command -v cargo
xcrun --find clang
command -v strip
command -v codesign
codesign --version
```

macOS normalization uses:

```bash
strip -S -x
```

The self-host materializer ad-hoc signs Mach-O outputs with `codesign --force --sign -` after `clang` links them. If a locally produced executable hangs before it reaches AICore argument handling and stack traces show `_dyld_start`, treat it as a macOS host approval or signing problem first:

```bash
spctl developer-mode enable-terminal
codesign --force --sign - target/selfhost-bootstrap/stage2/aic_selfhost
target/selfhost-bootstrap/stage2/aic_selfhost --help
```

Then approve the terminal application in System Settings > Privacy & Security > Developer Tools, restart the terminal or Codex session, and rerun `make selfhost-bootstrap`. If `target/debug/aic --help` also hangs in `_dyld_start`, verify Xcode Command Line Tools, host approval, and quarantine attributes before debugging compiler logic.

## Report Artifacts

Supported bootstrap writes the primary report and companion evidence under `target/selfhost-bootstrap/`:

```text
target/selfhost-bootstrap/report.json
target/selfhost-bootstrap/parity-report.json
target/selfhost-bootstrap/stage-matrix-report.json
target/selfhost-bootstrap/performance-report.json
target/selfhost-bootstrap/performance-trend.json
target/selfhost-bootstrap/stage0/aic_selfhost
target/selfhost-bootstrap/stage1/aic_selfhost
target/selfhost-bootstrap/stage2/aic_selfhost
```

Release provenance writes the release evidence under `target/selfhost-release/`:

```text
target/selfhost-release/provenance.json
target/selfhost-release/provenance.json.sha256
target/selfhost-release/selfhost-release-checksums.sha256
target/selfhost-release/aicore-selfhost-compiler-<platform>-<arch>
target/selfhost-release/aicore-selfhost-compiler-<platform>-<arch>.sha256
```

Use these inspection commands during review:

```bash
jq '{status, ready, host, reproducibility, performance}' target/selfhost-bootstrap/report.json
jq '.comparisons[] | select(.status != "match")' target/selfhost-bootstrap/parity-report.json
jq '.cases[] | select(.ok == false)' target/selfhost-bootstrap/stage-matrix-report.json
jq '{validation, canonical_artifact, stage_artifacts}' target/selfhost-release/provenance.json
python3 scripts/selfhost/release_provenance.py verify --provenance target/selfhost-release/provenance.json
shasum -a 256 -c target/selfhost-release/selfhost-release-checksums.sha256
```

On Linux, `sha256sum -c target/selfhost-release/selfhost-release-checksums.sha256` is equivalent to the `shasum` command.

## Failure Triage

Start with `target/selfhost-bootstrap/report.json`. The `host-preflight` step records missing host tools and platform details. Each build, parity, stage-matrix, performance, and reproducibility step records command lines, exit codes, timeouts, stdout/stderr, artifact paths, artifact sizes, SHA-256 values, and child peak RSS where available.

Use this triage order:

1. Host preflight failure: install or approve `cargo`, `clang`, `strip`, and macOS `codesign`; rerun `make selfhost-bootstrap`.
2. Stage0 failure: run `cargo run --quiet --bin aic -- check compiler/aic/tools/aic_selfhost --max-errors 240`.
3. Stage1 or stage2 failure: run the previous stage compiler directly against `compiler/aic/tools/aic_selfhost` and inspect the recorded stdout/stderr in `report.json`.
4. Parity failure: run `make selfhost-parity-candidate`, then inspect `target/selfhost-parity-candidate/report.json` for mismatched actions, diagnostic codes, JSON fingerprints, or artifact paths.
5. Stage matrix failure: run `make selfhost-stage-matrix`, then inspect `target/selfhost-stage-matrix/report.json` and the failing package path.
6. Performance failure: inspect `target/selfhost-bootstrap/performance-trend.json` for the violated metric, host baseline, active budget, and local overrides.
7. Reproducibility failure: compare the raw and normalized stage1/stage2 digests in `report.json`; on Linux verify `strip --strip-all`, and on macOS verify `strip -S -x` plus ad-hoc signing.
8. Provenance failure: rerun `make selfhost-release-provenance`, then verify `target/selfhost-release/provenance.json` against all stage artifacts and reports.

Do not replace a failing stage artifact with a Rust-reference artifact, skip a failing report, or treat a missing report as success. That creates release evidence that cannot be reproduced.

## Fallback And Rollback

Fallback is the expected response when the supported self-host gate fails:

```bash
cargo run --quiet --bin aic -- check <path>
cargo run --quiet --bin aic -- build <path> -o <artifact>
```

Keep the failing self-host issue open, attach the failing report paths or CI artifact links, and state that Rust-reference behavior remains the active production fallback.

Rollback is required when a promoted self-host artifact or workflow gate is found to be unsafe after promotion:

1. Stop promotion of the affected release artifact.
2. Revert the default selection or workflow promotion in a focused change.
3. Preserve the failing `target/selfhost-bootstrap/**` and `target/selfhost-release/**` artifacts.
4. Open a follow-up issue with the failing report digest, platform, toolchain versions, and rollback commit.
5. Rerun `make ci` and the supported bootstrap gate on the fallback path before restoring release promotion.

Fallback validation command:

```bash
AIC_COMPILER_MODE=fallback aic build examples/e5/hello_int.aic -o target/selfhost-fallback-check
aic release selfhost-mode --mode fallback --check --json
```

Default compiler source validation command:

```bash
aic release selfhost-mode --mode default --check --approve-default
aic build compiler/aic/tools/aic_selfhost -o target/selfhost-default/aic_selfhost
```

## Rust Reference Retirement Audit

Rust-reference retirement is a separate governance step after default-mode operation. The supported/default self-host gates do not approve Rust source deletion by themselves.

Run the retirement inventory audit before making or reviewing issue `#419` changes:

```bash
make selfhost-retirement-audit
```

The report is written to:

```text
target/selfhost-retirement/report.json
```

The audit passes when the manifest, decision record, docs, rollback commands, rollback validation schema, active paths, tracked Rust/Cargo path classifications, and every `retirement_decision` entry are internally consistent. It intentionally reports `removal_allowed=false` until approval, bake-in evidence, rollback validation evidence, and every replacement or retained-role decision is complete.

Passing bake-in evidence must include `make release-preflight`, `make ci`, the source commit, a supported bootstrap report with a matching `sha256:` digest, release provenance with a matching `sha256:` digest, and the controlled default compiler-source build artifact with a matching `sha256:` digest. Failed evidence can be recorded for history, but it does not count toward Linux/macOS bake-in.

Rollback evidence must be recorded under `rollback.validation_evidence`. A valid entry records the tag or branch and commit used to restore the Rust reference, the exact checkout command covering every `rollback.restore_paths` entry, `cargo build --locked`, `make selfhost-retirement-audit`, and matching `sha256:` digests for the cargo build log, retirement audit report, and marker scan report.

Class decision evidence is recorded under each `rust_path_classes[*].retirement_decision`. Reference compiler classes use `intent=remove-after-replacement`; retained host/tooling/test classes use `intent=retain-non-reference` with a named non-reference role. A class decision stays blocked until every command listed in `required_replacement_evidence` has a matching report and checksum.

Use `scripts/selfhost/retirement_evidence.py` after the real commands have run to generate checksum-bearing bake-in, rollback, and class decision entries. The helper can assemble a candidate manifest under `target/selfhost-retirement/` for review, but the approved manifest must still pass `python3 scripts/selfhost/retirement_audit.py --require-approved` before issue `#419` can close.

Use this command only when validating the final retirement decision:

```bash
python3 scripts/selfhost/retirement_audit.py --require-approved
```

That command must fail while the decision is deferred. Do not delete Rust reference source, remove fallback behavior, or close issue `#419` until it succeeds with an approved manifest and validated rollback evidence.

## Issue Closure Policy

Self-hosting issues remain open when any acceptance criterion is not fully implemented, when a placeholder path remains, when a scaffold-only implementation stands in for real behavior, when a fake success path is present, when a readiness case is skipped, or when production behavior is unsupported on the claimed platform.

Before closing a self-hosting issue:

1. Confirm the issue Definition of Done and acceptance criteria are implemented in code or documentation.
2. Build the changed components.
3. Add or update tests for the changed behavior, including failure paths when code changes are involved.
4. Run targeted tests and `make ci`.
5. Run `make docs-check`, `make examples-check`, and `make examples-run` when docs or examples changed.
6. Run `make selfhost-bootstrap` or reference the latest validated report when the documented command contract is unchanged.
7. Run `make selfhost-release-provenance` when release evidence or artifact provenance is affected.
8. Run the marker scanner from this runbook over touched paths.
9. Commit and push only the related changes.
10. Post evidence on the GitHub issue, then close it.

## Evidence Comment Template

Use this template before closing self-hosting work:

```markdown
Implementation evidence:

- Commit: `<commit-sha>`
- Docs updated: `<paths>`
- Code/tests updated: `<paths or n/a>`
- Local build: `<command> -> passed`
- Targeted tests: `<commands> -> passed`
- Docs/examples: `make docs-check`, `make examples-check`, `make examples-run` -> passed
- Self-host bootstrap: `<make selfhost-bootstrap or latest validated report path and digest>` -> passed
- Release provenance: `<make selfhost-release-provenance or n/a>` -> passed
- CI gate: `make ci` -> passed
- Marker scan: `<scanner command and touched paths>` -> no output
- Reports: `<report paths, artifact names, or CI artifact links>`

Readiness decision:

- Mode: `<experimental | supported | default | fallback | rollback>`
- Default compiler status changed: `<yes/no>`
- Remaining work: `<none, or linked follow-up issues>`
```

Use `default` only for the controlled AICore compiler source cutover and include the default build command evidence. For unrelated self-hosting issues, set `Default compiler status changed` to `no`.

Rust-reference retirement is not part of default-mode operation. Removal requires a separate issue with its own approval, tests, rollback plan, release notes, and verification evidence.

## CI And Release Evidence

CI uploads self-host evidence from `Self-Host Bootstrap (${{ matrix.os }})` as:

```text
selfhost-bootstrap-ubuntu-latest
selfhost-bootstrap-macos-latest
```

Release workflow uploads self-host evidence from `Release Self-Host Bootstrap (${{ matrix.os }})` as:

```text
release-selfhost-bootstrap-ubuntu-latest
release-selfhost-bootstrap-macos-latest
```

Review those artifacts before closing release-readiness issues. The release evidence is complete only when the supported bootstrap report is ready, the parity report passes, the stage matrix passes, performance budgets pass without overrides, reproducibility passes exactly or through the documented strip-normalization path, and release provenance verifies the canonical artifact and all required report checksums.
