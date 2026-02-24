# `aic test` snapshot workflow (`golden` fixtures)

`aic test` supports a golden-file workflow for `golden/*.aic` cases.

## Update snapshots

Write or refresh `*.aic.golden` files from current formatter output:

```bash
cargo run --quiet --bin aic -- test examples/e7/harness --mode golden --update-golden
```

## Check snapshots

Compare current formatter output against existing `*.aic.golden` files:

```bash
cargo run --quiet --bin aic -- test examples/e7/harness --mode golden --check-golden
```

On mismatch, the harness exits non-zero and prints a diff-like block with:

- `--- expected`
- `+++ actual`
- line hunks (`@@ line N @@`, `- ...`, `+ ...`)

## Notes

- Default `aic test` behavior is unchanged when neither flag is provided.
- `--update-golden` and `--check-golden` are mutually exclusive.
