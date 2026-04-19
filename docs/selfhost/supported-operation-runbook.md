# Supported Self-Host Operation Runbook

This runbook is the maintainer procedure for operating the AICore self-host compiler as a supported compiler path. It covers only the core compiler, bootstrap artifacts, parity gates, release provenance, and the evidence needed before closing self-hosting work.

Runtime libraries, protocol adapters, editor extensions, service integrations, and application packages are separate work streams. They can use self-hosted compiler artifacts after the compiler gate passes, but they are not prerequisites for declaring the core compiler self-host path supported.

## Operating Modes

| Mode | Meaning | Allowed use | Required gate |
|---|---|---|---|
| `experimental` | Stage0 or later self-host compiler artifacts can be built for investigation, but readiness gaps are still allowed. | Local diagnosis, report generation, and issue investigation. | `make selfhost-bootstrap-report` |
| `supported` | The self-host compiler is a supported compiler path on the current Linux or macOS host. | Maintainer validation, CI release evidence, and release artifact review. | `make selfhost-bootstrap` and `make selfhost-release-provenance` |
| `default` | The self-host compiler is selected as the normal compiler path. | Only after the explicit default-mode cutover issue is complete. | Supported gate plus the cutover issue acceptance criteria |
| `fallback` | Maintainers intentionally use the Rust reference compiler because a self-host gate failed or a platform is outside the supported matrix. | Triage, release blocking, or keeping existing Rust-reference behavior available. | Failing self-host evidence is attached to the issue or release review |
| `rollback` | A previous self-host promotion is backed out to the last known passing compiler path. | Production incident response or release candidate rejection. | Rollback evidence, failing report links, and a new follow-up issue |

Do not describe self-hosting as the default compiler path until the default-mode cutover issue is implemented, verified, committed, pushed, and closed with evidence. A supported Linux/macOS gate means the self-host compiler can be validated and shipped as a supported artifact; it does not by itself change default compiler selection.

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
make release-preflight
make ci
```

For long CI-equivalent local runs, keep the production budget defaults and only extend the process timeout:

```bash
AIC_SELFHOST_BOOTSTRAP_TIMEOUT=3600 make selfhost-bootstrap
```

Do not set resource-budget override variables for release evidence. Supported readiness requires the checked-in production defaults from `docs/selfhost/bootstrap-budgets.v1.json`.

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

Only use `default` when the explicit default-mode cutover issue is complete. For all other self-hosting issues, set `Default compiler status changed` to `no`.

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
