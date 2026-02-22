# Compatibility and Migration Policy

This policy defines what AICore treats as compatibility guarantees and how migrations are handled.

## Stability Domains

1. CLI contract (`aic contract --json`)
2. Canonical IR schema version (`schema_version`)
3. Diagnostic code namespace and semantics
4. Standard library API baseline (`docs/std-api-baseline.json`)
5. Release metadata formats (repro manifest, SBOM, provenance)
6. LTS branch support matrix (`docs/release/lts-policy.md`, `docs/release/compatibility-matrix.json`)

## Compatibility Rules

### CLI

- Command removals/renames are breaking.
- Flag behavior changes are breaking if existing automation can change outcome.
- Breaking CLI changes require:
  - contract version bump
  - migration notes
  - compatibility tests updated

### IR

- `schema_version` increments on incompatible structural changes.
- `aic ir-migrate` must support migrating from previous public schema.
- `aic migrate` must provide deterministic dry-run/apply reports for known source + IR breakages.

### Diagnostics

- Diagnostic codes are stable machine identifiers.
- Existing codes cannot be repurposed.
- New codes must be registered in `src/diagnostic_codes.rs` and documented.

### Std API

- Removals and signature changes are breaking.
- Deprecate first; remove only in a planned window.
- `aic std-compat --check --baseline docs/std-api-baseline.json` is required in CI.

## Migration Workflow

1. Propose breaking change with migration impact section.
2. Add/adjust migration tooling (`aic migrate`, `aic ir-migrate`, std compatibility checks, docs).
3. Add tests for old-to-new behavior.
4. Update docs and examples.
5. Pass `aic release policy --check` and `aic release lts --check` in CI.

## Policy Validation

Run:

```bash
aic release policy --check
aic release lts --check
aic migrate examples/ops/migration_v1_to_v2 --dry-run --json
```

JSON output for agents:

```bash
aic release policy --check --json
aic release lts --check --json
```

The check verifies required docs/workflows and policy metadata consistency.
