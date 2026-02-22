# Cross-Platform Release Matrix

This guide defines the production release matrix for AICore (`OPS-T1`).

## Supported Targets

- `linux-x64` (`ubuntu-latest`)
- `macos-x64` (`macos-latest`)
- `windows-x64` (`windows-latest`)

Each release target builds `aic` in release mode, packages a platform archive, and emits checksum + provenance metadata artifacts.

## Artifact Set Per Target

- Binary archive:
  - `aicore-<tag>-<os>-<arch>.tar.gz` (Linux/macOS)
  - `aicore-<tag>-<os>-<arch>.zip` (Windows)
- Checksum file:
  - `<archive>.sha256`
- Reproducibility manifest:
  - `repro-manifest-<os>-<arch>.json`
- SBOM:
  - `sbom-<os>-<arch>.json`
- Provenance (when signing key is configured):
  - `provenance-<os>-<arch>.json`

## CI Workflow

Implementation: `.github/workflows/release.yml`

Release build jobs run these verification gates per target:

1. Smoke test release binary:
   - `aic --help`
   - `aic check examples/option_match.aic`
2. Checksum integrity validation:
   - `aic release verify-checksum --artifact <archive> --checksum <archive>.sha256`
3. Provenance signature validation (when signing enabled):
   - `aic release verify-provenance --provenance <provenance.json> --key-env AIC_SIGNING_KEY`

## Local Verification Workflow

Use the same commands locally before publishing.

```bash
# Build release binary
cargo build --release --locked --bin aic

# Create and verify checksums
shasum -a 256 aicore-vX.Y.Z-linux-x64.tar.gz > aicore-vX.Y.Z-linux-x64.tar.gz.sha256
./target/release/aic release verify-checksum \
  --artifact aicore-vX.Y.Z-linux-x64.tar.gz \
  --checksum aicore-vX.Y.Z-linux-x64.tar.gz.sha256

# Generate reproducibility metadata
cargo run --quiet --bin aic -- release manifest --root . --output repro-manifest-linux-x64.json --source-date-epoch 1700000000
cargo run --quiet --bin aic -- release sbom --root . --output sbom-linux-x64.json --source-date-epoch 1700000000

# Sign and verify provenance
export AIC_SIGNING_KEY="replace-with-ci-secret"
cargo run --quiet --bin aic -- release provenance \
  --artifact target/release/aic \
  --sbom sbom-linux-x64.json \
  --manifest repro-manifest-linux-x64.json \
  --output provenance-linux-x64.json \
  --key-env AIC_SIGNING_KEY \
  --key-id local-release

cargo run --quiet --bin aic -- release verify-provenance \
  --provenance provenance-linux-x64.json \
  --key-env AIC_SIGNING_KEY
```

## Release Notes Metadata

`release-publish` composes `release-metadata.md` with:

- archive SHA-256 checksum
- manifest digest
- SBOM digest
- provenance signature

This metadata is attached to GitHub release notes (`body_path`) together with generated notes.
