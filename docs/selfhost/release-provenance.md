# Self-Host Release Provenance

This document defines the release-grade artifact and provenance gate for the AICore self-host compiler. It covers only compiler bootstrap artifacts and reports. Library packages, services, editor tooling, and application packages use their own release processes.

## Canonical Artifact

`make selfhost-release-provenance` reads the supported bootstrap report from:

```bash
target/selfhost-bootstrap/report.json
```

The gate copies the supported stage2 compiler into a platform-specific release artifact under `target/selfhost-release/`:

```text
target/selfhost-release/aicore-selfhost-compiler-linux-x64
target/selfhost-release/aicore-selfhost-compiler-linux-arm64
target/selfhost-release/aicore-selfhost-compiler-macos-x64
target/selfhost-release/aicore-selfhost-compiler-macos-arm64
```

The exact suffix is derived from the bootstrap host platform and machine. Supported release platforms are Linux and macOS. Unsupported platforms fail the gate instead of producing partial provenance.

## Generated Files

The release provenance gate writes:

```text
target/selfhost-release/provenance.json
target/selfhost-release/provenance.json.sha256
target/selfhost-release/selfhost-release-checksums.sha256
target/selfhost-release/aicore-selfhost-compiler-<platform>-<arch>
target/selfhost-release/aicore-selfhost-compiler-<platform>-<arch>.sha256
```

`provenance.json` uses format `aicore-selfhost-release-provenance-v1` and schema version `1`. It records:

- source commit and tracked-worktree dirty state
- host platform and machine
- cargo, rustc, clang, strip, and macOS codesign version evidence
- canonical release artifact path, size, and SHA-256
- stage0, stage1, and stage2 artifact paths, raw SHA-256, normalized SHA-256, sizes, and normalization command
- bootstrap, parity, stage-matrix, performance, and performance-trend report paths and SHA-256 values
- stage1/stage2 reproducibility result from the bootstrap report
- validation booleans for bootstrap readiness, parity, stage matrix, performance budgets, and budget overrides
- checksum manifest path and entries

The checksum files use standard sha256sum-compatible lines:

```text
<hex-sha256>  <repo-relative-path>
```

## Release Gate

Run the full local release preflight:

```bash
make release-preflight
```

That target runs:

```bash
make ci
make selfhost-bootstrap
make selfhost-release-provenance
make repro-check
make security-audit
```

The self-host release provenance gate fails if:

- the bootstrap report is missing or not `supported-ready`
- stage0, stage1, or stage2 artifacts are missing
- a stage artifact checksum differs from the bootstrap report
- stage1/stage2 reproducibility did not pass
- parity, stage-matrix, performance, or performance-trend reports are missing or failing
- performance budget overrides were used
- the platform is not Linux or macOS
- the platform normalization command is not the checked-in command for the host

Linux normalization is:

```bash
strip --strip-all
```

macOS normalization is:

```bash
strip -S -x
```

## Verification

After generation, verify the release provenance with:

```bash
python3 scripts/selfhost/release_provenance.py verify \
  --provenance target/selfhost-release/provenance.json
```

The verifier recomputes checksums for the canonical artifact, stage artifacts, and required reports. It also re-runs the recorded normalization command for each stage artifact and compares the normalized digest with the provenance record.

To inspect checksums manually:

```bash
python3 scripts/selfhost/release_provenance.py generate
python3 scripts/selfhost/release_provenance.py verify
shasum -a 256 -c target/selfhost-release/selfhost-release-checksums.sha256
```

On Linux, `sha256sum -c target/selfhost-release/selfhost-release-checksums.sha256` is equivalent.

## Release Review

Release reviewers should compare:

- `target/selfhost-bootstrap/report.json`: stage commands, raw stage checksums, readiness, and reproducibility
- `target/selfhost-bootstrap/performance-report.json`: active budget source and observed resource metrics
- `target/selfhost-bootstrap/performance-trend.json`: baseline comparison metrics
- `target/selfhost-bootstrap/parity-report.json`: Rust reference versus self-host compiler parity
- `target/selfhost-bootstrap/stage-matrix-report.json`: package, package-member, workspace-probe, and core-example validation
- `target/selfhost-release/provenance.json`: source commit, toolchain, artifact checksums, report checksums, and validation summary
- `target/selfhost-release/selfhost-release-checksums.sha256`: independent checksum list for release evidence

Do not approve a self-host compiler artifact if any required report is absent, failing, generated with budget overrides, or disconnected from the source commit under review.
