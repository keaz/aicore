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
```
