# `aic test` workflow (`#[test]`, `#[should_panic]`, `#[property]`)

Use `aic test` to run fixture categories plus source-local attribute/property tests.

## Attribute tests

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

## Property tests

```aic
#[property(iterations = 32)]
fn prop_reverse_reverse(x: Int) -> () {
    assert_eq(x, x);
}

#[property(iterations = 16)]
fn prop_generators_cover_all(
    i: Int,
    f: Float,
    b: Bool,
    s: String
) -> () {
    assert_eq(i, i);
    assert(b || !b);
}
```

## Run all tests

```bash
cargo run --quiet --bin aic -- test examples/e7/test_framework --json
cargo run --quiet --bin aic -- test examples/e7/property_framework --seed 123 --json
```

## Run filtered subset

```bash
cargo run --quiet --bin aic -- test examples/e7/test_framework --filter addition --json
cargo run --quiet --bin aic -- test examples/e7/property_framework --filter reverse --seed 123 --json
```

## CI contract

- JSON mode prints a machine-readable report to stdout.
- Attribute/property runs also write `test_results.json` to the selected test root.
- Property failure details include iteration, seed, counterexample, and shrunk input.
- CI reference example: `scripts/ci/examples.sh`.
