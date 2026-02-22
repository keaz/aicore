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

## Local Dry-Run Commands

```bash
aic release manifest --root . --output target/release/repro-manifest.json --source-date-epoch 1700000000
aic release sbom --root . --output target/release/sbom.json --source-date-epoch 1700000000
aic release policy --check
aic release lts --check
aic release security-audit --json
```

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
```

## Failure Response

- checksum mismatch:
  - rerun packaging and regenerate checksum file
  - verify archive path and SHA file pair
- provenance verification failure:
  - validate `AIC_SIGNING_KEY` and `key_id`
  - regenerate provenance from current artifact + manifest + sbom
- policy gate failure:
  - run `aic release policy --check --json` and `aic release lts --check --json`
  - fix missing docs/workflow gates before retry
