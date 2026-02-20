# Diagnostics SARIF Export (E7)

AICore can export diagnostics as SARIF for code scanning workflows.

## Commands

```bash
aic check <input> --sarif > diagnostics.sarif
aic diag <input> --sarif > diagnostics.sarif
```

## Output guarantees

- SARIF version: `2.1.0`
- Rule IDs: compiler diagnostic code (`E####`)
- Result level: mapped from AIC severity (`error`, `warning`, `note`)
- Location: file + range derived from diagnostic span offsets

## CI usage

GitHub Actions example:

```bash
cargo run --quiet --bin aic -- check examples/e7/diag_errors.aic --sarif > diagnostics.sarif
```

Upload `diagnostics.sarif` using standard SARIF upload actions.
