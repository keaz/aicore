# `aic test` attribute workflow (`#[test]`, `#[should_panic]`)

Use attribute-based tests for package-local test functions.

## Example

```aic
#[test]
fn test_addition() -> () {
    assert_eq(1 + 1, 2);
    assert(true);
}

#[test]
#[should_panic]
fn test_division_by_zero() -> () {
    assert_eq(1, 2);
}
```

## Run all tests

```bash
cargo run --quiet --bin aic -- test examples/e7/test_framework --json
```

## Run filtered subset

```bash
cargo run --quiet --bin aic -- test examples/e7/test_framework --filter addition --json
```

## CI contract

- JSON mode prints a machine-readable report to stdout.
- Attribute-test runs also write `test_results.json` to the selected test root.
- CI reference example: `scripts/ci/examples.sh`.
