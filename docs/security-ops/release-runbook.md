# Release Runbook

This runbook describes production release flow with reproducibility, signing, and verification.

## Cross-Platform Pipeline

Release matrix is defined in `.github/workflows/release.yml` and `docs/release/matrix.md`:

- `linux-x64`
- `macos-x64`
- `windows-x64`

Each target produces:

- archive
- SHA-256 checksum
- reproducibility manifest
- SBOM
- provenance statement

Self-host compiler bootstrap artifacts are checked separately from the source reproducibility manifest. Linux bootstrap normalization uses `strip --strip-all`; macOS bootstrap normalization uses `strip -S -x` and requires ad-hoc signed Mach-O outputs from the self-host materializer. The bootstrap report must also pass its resource budgets (`performance.ok == true`) for duration, artifact size, and child peak RSS. Release review additionally requires `target/selfhost-release/provenance.json`, which ties the canonical `aicore-selfhost-compiler-<platform>-<arch>` artifact to the source commit, stage0/stage1/stage2 checksums, parity report, stage-matrix report, performance reports, and reproducibility result.

## Local Dry-Run Commands

```bash
aic release manifest --root . --output target/release/repro-manifest.json --source-date-epoch 1700000000
aic release sbom --root . --output target/release/sbom.json --source-date-epoch 1700000000
aic release policy --check
aic release lts --check
aic release security-audit --json
make selfhost-bootstrap
make selfhost-release-provenance
```

Reproducibility manifests include source inputs and intentionally exclude local/generated paths such as `target`, `target-linux`, `.aic`, `.aic-cache`, `.aic-replay`, `.ci-local-bin`, `.vscode-test`, `dist`, and `node_modules`.

## Signing and Verification

Create signed provenance:

```bash
export AIC_SIGNING_KEY="replace-with-ci-secret"
aic release provenance \
  --artifact target/release/aic \
  --sbom target/release/sbom.json \
  --manifest target/release/repro-manifest.json \
  --output target/release/provenance.json \
  --key-env AIC_SIGNING_KEY \
  --key-id release-ci
```

Verify:

```bash
aic release verify-provenance --provenance target/release/provenance.json --key-env AIC_SIGNING_KEY
python3 scripts/selfhost/release_provenance.py verify --provenance target/selfhost-release/provenance.json
```

## Failure Response

- checksum mismatch:
  - rerun packaging and regenerate checksum file
  - verify archive path and SHA file pair
- provenance verification failure:
  - validate `AIC_SIGNING_KEY` and `key_id`
  - regenerate provenance from current artifact + manifest + sbom
- self-host provenance failure:
  - rerun `make selfhost-bootstrap`
  - verify `performance.ok == true` and budget overrides are empty
  - rerun `make selfhost-release-provenance`
  - compare `target/selfhost-release/selfhost-release-checksums.sha256` with the stage and report paths in `target/selfhost-release/provenance.json`
- policy gate failure:
  - run `aic release policy --check --json` and `aic release lts --check --json`
  - fix missing docs/workflow gates before retry
