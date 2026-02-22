# LTS Policy and Compatibility Matrix

This document defines long-term support expectations for release branches and how they are enforced in CI.

## Branch Channels

- `main`: active development channel
- `release/0.1`: LTS maintenance channel

## LTS Support Windows

| Branch | Channel | Support Window |
|---|---|---|
| `main` | `active` | 12 months rolling |
| `release/0.1` | `lts` | 18 months from branch cut |

Support windows are encoded in `docs/release/compatibility-matrix.json` and validated by `aic release lts --check`.

## Security Patch SLA

All supported channels follow the same maximum patch response windows:

- `critical`: 2 days
- `high`: 7 days
- `medium`: 30 days

The policy gate rejects configurations where:

- `critical` exceeds 7 days
- `high` exceeds 14 days

## CI Enforcement

Use these commands to validate policy and matrix consistency:

```bash
aic release lts --check
aic release lts --check --json
```

Mandatory workflow gates:

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `.github/workflows/security.yml`

Each workflow must include `release lts --check`.

## Update Process

1. Update branch support/SLA values in this document.
2. Apply matching updates to `docs/release/compatibility-matrix.json`.
3. Run `aic release lts --check`.
4. Run `make ci` before merge.
