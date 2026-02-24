# `aic metrics` workflow

Use `aic metrics` to emit deterministic per-function complexity data for agent self-assessment.

## Basic run

```bash
cargo run --quiet --bin aic -- metrics examples/metrics/src/main.aic
```

## Gate in CI

```bash
cargo run --quiet --bin aic -- metrics examples/metrics/src/main.aic --check --max-cyclomatic 15
```

## With manifest thresholds

When `examples/metrics/aic.toml` includes:

```toml
[metrics]
max_cyclomatic = 6
max_cognitive = 12
```

you can run:

```bash
cargo run --quiet --bin aic -- metrics examples/metrics/src/main.aic --check
```

and the command will fail with non-zero exit if any function exceeds configured limits.
