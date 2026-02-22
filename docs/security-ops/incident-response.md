# LTS and Incident Response Playbook

This runbook defines LTS lifecycle, patching expectations, and operational incident handling.

## LTS Lifecycle

Source of truth:

- `docs/release/lts-policy.md`
- `docs/release/compatibility-matrix.json`

Current channels:

- `main` (`active`, 12 months support window)
- `release/0.1` (`lts`, 18 months support window)

Policy gate:

```bash
aic release lts --check
```

## Security Patching SLA

- critical: patch within 2 days
- high: patch within 7 days
- medium: patch within 30 days

## Incident Classes

- release integrity incident (checksum/provenance mismatch)
- policy drift incident (missing compatibility/LTS assets)
- sandbox escape or policy bypass incident
- migration regression incident

## Response Workflow

1. Detect and classify severity.
2. Capture telemetry and trace ids (`AIC_TRACE_ID`, `AIC_TELEMETRY_PATH`).
3. Freeze affected release channel and stop artifact promotion.
4. Reproduce with deterministic commands:
   - `aic release security-audit --json`
   - `aic release policy --check --json`
   - `aic release lts --check --json`
5. Mitigate and validate fix in CI (`make ci`, `make test-e9`).
6. Publish incident note with root cause and prevention action.

## Migration Rollback Trigger

If migration output introduces production regression:

1. restore from source control baseline
2. rerun migration in dry-run mode for analysis
3. isolate high-risk edit classes from report (`high_risk_edits`)
4. roll forward with reviewed manual patch instead of blind apply
