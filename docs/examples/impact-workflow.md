# `aic impact` blast-radius workflow

Use `aic impact` to inspect who depends on a function and how risky a change is.

## Run from a file path

```bash
cargo run --quiet --bin aic -- impact normalize examples/e7/impact_demo.aic
```

## Run from a package root

```bash
cargo run --quiet --bin aic -- impact normalize
```

## JSON fields

- `function`: analyzed function (module-qualified when unambiguous)
- `direct_callers`: functions that call `function` directly
- `transitive_callers`: indirect callers reachable through call chains
- `affected_tests`: impacted test-like functions (`test_*`/`*_test` or test modules)
- `affected_contracts`: impacted functions with `requires`/`ensures`
- `blast_radius`: `small` | `medium` | `large`

When `affected_tests` is empty but callers exist, treat it as an untested impact zone.

