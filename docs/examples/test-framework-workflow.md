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

## Mock IO tests

```aic
import std.io;
import std.vec;

#[test]
fn test_mock_reader() -> () effects { io } {
    let reader = mock_reader_from_lines(append(vec_of("hello"), vec_of("world")));
    let _ = install_mock_reader(reader);

    let first = match read_line() { Ok(v) => v, Err(_) => "" };
    let second = match read_line() { Ok(v) => v, Err(_) => "" };

    assert_eq(first, "hello");
    assert_eq(second, "world");
}
```

## Run all tests

```bash
cargo run --quiet --bin aic -- test examples/e7/test_framework --json
cargo run --quiet --bin aic -- test examples/e7/property_framework --seed 123 --json
cargo run --quiet --bin aic -- test examples/test --seed 123 --json
```

## Run filtered subset

```bash
cargo run --quiet --bin aic -- test examples/e7/test_framework --filter addition --json
cargo run --quiet --bin aic -- test examples/e7/property_framework --filter reverse --seed 123 --json
cargo run --quiet --bin aic -- test examples/test --filter mock --seed 123 --json
```

## Replay a failing run

`aic test --json` now emits a `replay` object when failures occur.

```bash
! cargo run --quiet --bin aic -- test examples/test/replay_failure.aic --seed 777 --json > /tmp/aic-replay-report.json
python3 - <<'PY'
import json
report = json.load(open("/tmp/aic-replay-report.json"))
print(report["replay"]["replay_id"])
PY
```

Re-run with replay metadata:

```bash
REPLAY_ID="$(python3 - <<'PY'
import json
report = json.load(open("/tmp/aic-replay-report.json"))
print(report["replay"]["replay_id"])
PY
)"
! cargo run --quiet --bin aic -- test examples/test/replay_failure.aic --replay "$REPLAY_ID" --json
```

## Mock isolation violation example

`examples/test/mock_isolation_violation.aic` intentionally attempts real `net`/`proc` operations in test mode. The run must fail with structured `sandbox_policy_violation` diagnostics.

```bash
! cargo run --quiet --bin aic -- test examples/test/mock_isolation_violation.aic --json
```

## CI contract

- JSON mode prints a machine-readable report to stdout.
- Attribute/property runs also write `test_results.json` to the selected test root.
- Property failure details include iteration, seed, counterexample, and shrunk input.
- Failing runs emit replay metadata (`replay_id`, artifact path, seed/time/mock profile/trace id) and persist an artifact under `.aic-replay/`.
- Test runner sets deterministic defaults for tests (`AIC_TEST_SEED`, `AIC_TEST_TIME_MS`) and can run with mocked IO isolation (`AIC_TEST_NO_REAL_IO`, `AIC_TEST_IO_CAPTURE`).
- In mock isolation mode, accidental real `fs`/`net`/`proc` side effects are rejected with structured diagnostics.
- CI reference example: `scripts/ci/examples.sh`.
