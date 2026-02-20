# Release Security and Operations (E9)

This document defines the production release controls for AICore.

## Scope

E9 covers:

1. Reproducible release inputs and deterministic manifests
2. CI/CD release automation and artifact publishing
3. SBOM generation plus signed provenance
4. Security audit checks and threat-model enforcement
5. Sandboxed runtime execution profiles
6. Compatibility and migration policy checks

## CLI Commands

### Reproducibility manifest

Generate deterministic source manifest:

```bash
aic release manifest --root . --output target/release/repro-manifest.json --source-date-epoch 1700000000
```

Verify against checked-in/output manifest:

```bash
aic release verify-manifest --root . --manifest target/release/repro-manifest.json
```

### SBOM

Generate SBOM from `Cargo.toml` + `Cargo.lock`:

```bash
aic release sbom --root . --output target/release/sbom.json --source-date-epoch 1700000000
```

### Provenance signing and verification

Create signed provenance statement (HMAC-SHA256):

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

Verify the provenance file:

```bash
aic release verify-provenance \
  --provenance target/release/provenance.json \
  --key-env AIC_SIGNING_KEY
```

### Security audit

```bash
aic release security-audit --json
```

Checks include:

- threat model document exists and has required sections
- no `unsafe` token in `src/`
- workflow action refs are pinned (no `@main`/`@master`)
- release workflow has `permissions`, `concurrency`, and `--locked` build usage

### Compatibility policy

Show policy JSON:

```bash
aic release policy --json
```

Check required compatibility assets:

```bash
aic release policy --check
```

## Sandbox Profiles

`aic run` supports sandbox profiles:

```bash
aic run examples/option_match.aic --sandbox none
aic run examples/option_match.aic --sandbox ci
aic run examples/option_match.aic --sandbox strict
```

Profile policy:

- `none`: no additional resource limits
- `ci`: moderate limits (CPU, memory, file size, open files, process count)
- `strict`: tighter limits for untrusted samples

Linux implementation uses `prlimit`.

## Local CI Integration

Use these targets before publishing:

```bash
make security-audit
make repro-check
make test-e9
make release-preflight
```

`make ci` also runs E9 checks.

## GitHub Actions

- `.github/workflows/ci.yml` runs `make test-e9`, `make security-audit`, and `make repro-check`.
- `.github/workflows/release.yml` builds release artifacts and publishes checksums + metadata.
- `.github/workflows/security.yml` runs scheduled and on-demand security audit checks.

