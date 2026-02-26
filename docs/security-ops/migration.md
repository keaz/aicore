# Migration Tooling (`aic migrate`)

`aic migrate` provides deterministic upgrade assistance across source and IR artifacts.

## Command modes

Dry-run JSON report:

```bash
aic migrate <path> --dry-run --json
```

Apply changes and write an artifact report:

```bash
aic migrate <path> --report target/ops/migration-report.json
```

## Current automated rules

- `MIG001` (low risk): replace deprecated `std.time.now(...)` calls with `std.time.now_ms(...)`.
- `MIG002` (high risk): replace legacy `null` with `None()`.
- `MIG003` (medium risk): migrate legacy IR JSON to current `schema_version`.

## Concurrency Migration (AGX3)

For legacy `std.concurrent` int-only APIs (`channel_int`, `send_int`, `recv_int`, etc.),
use deterministic deprecation diagnostics as the migration driver:

```bash
aic check <path> --json
```

Every deprecated call emits `E6001` with a machine-readable replacement hint, which can be scripted by agents to stage incremental rewrites to generic `Sender[T]` / `Receiver[T]` APIs.

## Compatibility strategy

- apply source and IR migrations before release branch cut
- keep migration operations deterministic (`--dry-run --json` reports are stable)
- combine migration checks with:
  - `aic release policy --check`
  - `aic release lts --check`
- treat high-risk edits as mandatory human review items

## Report schema

Reports contain:

- `schema_version`: migration report schema version
- `files_scanned`, `files_changed`, `edits_planned`
- `high_risk_edits` summary
- per-file edits with rule id, risk level, line/column, and before/after snippets

`high_risk_edits > 0` means manual review is required before release.

## Rollback plan

1. keep pre-migration commit/tag for rollback.
2. run dry-run report and store it as release evidence.
3. if post-migration checks fail, restore baseline commit.
4. re-apply only reviewed edits manually, then re-run `make ci`.
5. attach migration report and rollback notes to incident log.

## Example workflow

```bash
aic migrate examples/ops/migration_v1_to_v2 --dry-run --json
aic migrate examples/ops/migration_v1_to_v2 --report target/ops/migration-report.json
aic check examples/ops/migration_v1_to_v2/src/main.aic
```
