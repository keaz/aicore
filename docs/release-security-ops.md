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
7. LTS branch support windows and compatibility matrix enforcement

## CLI Commands

### Reproducibility manifest

Generate deterministic source manifest:

```bash
aic release manifest --root . --output target/release/repro-manifest.json --source-date-epoch 1700000000
```

The manifest records source inputs only. Local/generated directories such as `target`, `target-linux`, `.aic`, `.aic-cache`, `.aic-replay`, `.ci-local-bin`, `.vscode-test`, `dist`, and `node_modules` are excluded so release verification is not affected by self-host bootstrap artifacts, editor test downloads, or local build output. Self-host native reproducibility uses platform artifact normalization outside this source manifest: Linux uses `strip --strip-all`, and macOS uses `strip -S -x` after Mach-O outputs are ad-hoc signed. The self-host bootstrap report also records manifest-backed resource budgets and observed duration, artifact-size, reproducibility-duration, and child peak-RSS values; release readiness requires `performance.ok` to be `true` with no CI/release budget overrides.

Self-host release provenance is generated after the bootstrap gate:

```bash
make selfhost-release-provenance
```

This writes `target/selfhost-release/provenance.json`, `target/selfhost-release/selfhost-release-checksums.sha256`, and the canonical `target/selfhost-release/aicore-selfhost-compiler-<platform>-<arch>` artifact. The provenance format is `aicore-selfhost-release-provenance-v1`; it records source commit, toolchain versions, stage0/stage1/stage2 raw and normalized digests, bootstrap/parity/stage-matrix/performance reports, and the reproducibility result.

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

python3 scripts/selfhost/release_provenance.py verify \
  --provenance target/selfhost-release/provenance.json
```

### Artifact checksum verification

Validate packaged release archives against `.sha256` files:

```bash
aic release verify-checksum \
  --artifact aicore-vX.Y.Z-linux-x64.tar.gz \
  --checksum aicore-vX.Y.Z-linux-x64.tar.gz.sha256
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

### LTS policy and compatibility matrix

Show LTS policy JSON:

```bash
aic release lts --json
```

Check required LTS docs, matrix entries, and CI gates:

```bash
aic release lts --check
```

### Guided migration

Run deterministic migration analysis:

```bash
aic migrate examples/ops/migration_v1_to_v2 --dry-run --json
```

Apply known migrations and persist a report:

```bash
aic migrate examples/ops/migration_v1_to_v2 --report target/ops/migration-report.json
```

## Sandbox Profiles

`aic run` supports sandbox profiles:

```bash
aic run examples/option_match.aic --sandbox none
aic run examples/option_match.aic --sandbox ci
aic run examples/option_match.aic --sandbox strict
aic run examples/ops/sandbox_profiles/fs_blocked_demo.aic --sandbox-config examples/ops/sandbox_profiles/profiles/strict.json
```

Profile policy:

- `none`: no additional resource limits
- `ci`: moderate limits (CPU, memory, file size, open files, process count)
- `strict`: tighter limits for untrusted samples

Linux implementation uses `prlimit`.

Custom profile format is JSON with `profile`, `permissions`, and optional `limits`:

```json
{
  "profile": "ops-test",
  "permissions": { "fs": false, "net": false, "proc": false, "time": false },
  "limits": {
    "profile": "ops-test",
    "cpu_seconds": 10,
    "memory_bytes": 536870912,
    "file_bytes": 33554432,
    "max_open_files": 128,
    "max_processes": 32
  }
}
```

Disallowed runtime operations emit machine-readable diagnostics on stderr:

```json
{"code":"sandbox_policy_violation","trace_id":"ops-trace-001","profile":"ops-test","domain":"fs","operation":"read_text"}
```

## Observability Telemetry

Enable structured telemetry events for compiler/runtime phases:

```bash
AIC_TRACE_ID=ops-trace-001 \
AIC_TELEMETRY_PATH=target/ops/telemetry.jsonl \
aic check examples/ops/observability_demo/main.aic
```

Telemetry schema and runbook:

- `docs/security-ops/telemetry.schema.json`
- `docs/security-ops/telemetry.md`
- `docs/security-ops/migration.md`
- `docs/security-ops/release-runbook.md`
- `docs/security-ops/sandbox-operations.md`
- `docs/security-ops/incident-response.md`

## Local CI Integration

Use these targets before publishing:

```bash
make security-audit
make repro-check
make test-e9
make selfhost-bootstrap
make selfhost-release-provenance
make release-preflight
aic release lts --check
```

`make ci` also runs E9 checks. `make release-preflight` is the local release-readiness target and includes the supported self-host bootstrap gate plus self-host release provenance for the current host.

## GitHub Actions

- `.github/workflows/ci.yml` runs `make test-e9`, `make security-audit`, and `make repro-check`.
- `.github/workflows/ci.yml` also runs `Self-Host Bootstrap (${{ matrix.os }})` on Linux and macOS, runs `make selfhost-release-provenance`, and uploads `target/selfhost-bootstrap/report.json`, `performance-report.json`, `performance-trend.json`, parity, stage-matrix artifacts, and `target/selfhost-release/**`.
- `.github/workflows/release.yml` builds release artifacts and publishes checksums + metadata, with `release policy`, `release lts`, `Release Self-Host Bootstrap (${{ matrix.os }})`, and self-host release provenance Linux/macOS gates.
- `.github/workflows/security.yml` runs scheduled and on-demand security audit checks, and the LTS policy gate.
- `docs/release/matrix.md` documents the cross-platform release matrix and verification workflow.
- `docs/release/lts-policy.md` and `docs/release/compatibility-matrix.json` define branch support windows and SLA compatibility expectations.
