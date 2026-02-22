# Migration v1 to v2 Demo

This directory contains intentionally legacy inputs used by `aic migrate`.

## Contents

- `src/main.aic`: uses deprecated `std.time.now()` and legacy `null`.
- `legacy_ir.json`: legacy IR JSON without `schema_version`.

## Dry-run report

```bash
aic migrate examples/ops/migration_v1_to_v2 --dry-run --json
```

## Apply migration and write report

```bash
aic migrate examples/ops/migration_v1_to_v2 --report target/ops/migration-report.json
```

## Verify migrated source

```bash
aic check examples/ops/migration_v1_to_v2/src/main.aic
```
