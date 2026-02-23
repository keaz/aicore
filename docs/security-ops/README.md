# Security and Operations Runbooks (OPS-T6)

This directory is the agent-grade operations manual for release, security, migration, and incident handling.

## Capability Map (OPS-T1..OPS-T5)

| Story | Capability | Runbook |
|---|---|---|
| OPS-T1 | cross-platform release matrix and artifact verification | `release-runbook.md` |
| OPS-T2 | sandbox policy design and enforcement semantics | `sandbox-operations.md` |
| OPS-T3 | observability logs/metrics/traces and correlation | `telemetry.md` |
| OPS-T4 | compatibility migration workflows and rollback planning | `migration.md` |
| OPS-T5 | LTS support windows, patching SLA, incident response | `incident-response.md` |

## Canonical Examples

- release and migration sample: `examples/ops/migration_v1_to_v2/`
- telemetry sample: `examples/ops/observability_demo/`
- sandbox policy samples: `examples/ops/sandbox_profiles/`

## Fast Validation Commands

```bash
aic release policy --check
aic release lts --check
aic release security-audit --json
aic migrate examples/ops/migration_v1_to_v2 --dry-run --json
```

For full release readiness:

```bash
make test-e9
make security-audit
make repro-check
make release-preflight
```

## Enforcement Map for Issue #106 (Epic #64)

| Command | Enforced by | What it validates |
|---|---|---|
| `make test-e9` | `.github/workflows/ci.yml` (`E9 release and security tests`) | Runs `tests/e9_release_ops_tests.rs` to validate deterministic release/migration outputs, release policy + LTS + security-audit command contracts, checksum/provenance tamper detection, and OPS runbook/workflow gate wiring. |
| `make security-audit` | `.github/workflows/ci.yml`, `.github/workflows/security.yml` | Enforces the release security audit gate on PR/mainline CI and on the scheduled security workflow. |
| `make repro-check` | `.github/workflows/ci.yml`, `.github/workflows/security.yml` | Enforces reproducibility manifest checks in CI and in the security workflow. |
| `make release-preflight` | `.github/workflows/release.yml` (`release-preflight` job) | Mirrors release gating locally before tagging, aligned with release workflow checks (`make ci`, release policy/LTS gates, and security-audit gate). |
