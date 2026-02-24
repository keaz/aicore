# Std API Documentation

This page defines how AICore standard-library API documentation is generated and maintained.

## Source of truth

- `std/*.aic`: canonical API declarations.
- `/Users/kasunranasinghe/Projects/Rust/aicore/src/std_policy.rs`: snapshot collector and compatibility comparator.
- `/Users/kasunranasinghe/Projects/Rust/aicore/docs/std-api-baseline.json`: machine-readable baseline consumed by `aic std-compat`.

## Generation workflow

1. Update std modules under `/Users/kasunranasinghe/Projects/Rust/aicore/std`.
2. Regenerate snapshot:

```bash
cargo run --quiet --bin aic -- std-compat > /Users/kasunranasinghe/Projects/Rust/aicore/docs/std-api-baseline.json
```

3. Validate compatibility against baseline:

```bash
cargo run --quiet --bin aic -- std-compat --check --baseline /Users/kasunranasinghe/Projects/Rust/aicore/docs/std-api-baseline.json
```

4. If `--check` reports `E6002`, either:
- restore backward compatibility, or
- explicitly document a migration and intentionally update baseline in the same change.

The check treats removed/changed symbols as breaking and additive symbols as compatible.

## `aic doc` generation contract

`aic doc <input> --output <dir>` creates `<dir>` when missing and always emits:

- `index.md`: human-readable module documentation.
- `api.json`: machine-readable module/item payload (`schema_version = 1`).

Use these commands to validate module and std documentation generation behavior:

<!-- std-api:docgen:start -->
aic doc examples/e4/verified_abs.aic --output target/docs-contract/module-docs
aic doc std/fs.aic --output target/docs-contract/std-fs-docs
<!-- std-api:docgen:end -->

Each command above must produce the following files inside its output directory:

<!-- std-api:docgen-files:start -->
index.md
api.json
<!-- std-api:docgen-files:end -->

## Module coverage

Current baseline snapshot (`schema_version: 1`) contains `521` symbols across these modules.

| Module | Symbol count |
|---|---:|
| `std.concurrent` | 26 |
| `std.env` | 31 |
| `std.fs` | 57 |
| `std.http` | 21 |
| `std.http_server` | 21 |
| `std.io` | 90 |
| `std.json` | 40 |
| `std.map` | 20 |
| `std.net` | 30 |
| `std.option` | 1 |
| `std.path` | 10 |
| `std.proc` | 23 |
| `std.rand` | 7 |
| `std.regex` | 15 |
| `std.result` | 1 |
| `std.retry` | 5 |
| `std.router` | 9 |
| `std.string` | 48 |
| `std.time` | 21 |
| `std.url` | 9 |
| `std.vec` | 36 |

Symbol kind distribution:

- `fn`: 475
- `struct`: 27
- `enum`: 19

## Signature policy

Documented signatures must match snapshot rendering rules from `/Users/kasunranasinghe/Projects/Rust/aicore/src/std_policy.rs`:

- Functions: `name[generics](params) -> Ret effects { ... }`
- Structs: `Name[generics] { field: Type, ... }`
- Enums: `Name[generics] { Variant, Variant(Type), ... }`
- Traits: `TraitName[generics];`
- Impls: `TraitName[TypeArgs];`

Rules:

- Keep parameter names and effect sets exactly as declared in std modules.
- Keep generic bounds in signatures when present.
- Do not normalize or alias module names in rendered signatures.

## Effects policy

Effects recorded in the std baseline currently include:

- `concurrency`
- `env`
- `fs`
- `io`
- `net`
- `proc`
- `rand`
- `time`

For each effectful API in docs:

- include the exact `effects { ... }` list,
- document required effect declarations at call sites,
- call out effect-related diagnostics where relevant (`E2001-E2006`).

## Errors policy

For APIs returning `Result[T, E]`:

- document the error enum (`E`) and expected failure classes,
- separate deterministic validation failures from environment/runtime failures,
- include diagnostics users will most often see when misuse occurs (for example `E2001`, `E212x`, `E50xx`, `E6001`, `E6002`).

## Examples policy

Examples must stay concise and compile-intent aligned with AIC syntax.

Required structure for each std API page section:

1. one minimal success-path snippet,
2. one failure-path snippet (or usage note) that demonstrates error/effect constraints,
3. clear module-qualified names when ambiguity is possible.

Example style:

```aic
module demo.main;

fn main() -> Result[String, FsError] effects { fs } {
  std.fs.read_file("README.md")
}
```

## Related docs

- `/Users/kasunranasinghe/Projects/Rust/aicore/docs/std-api/machine-readable.md`
- `/Users/kasunranasinghe/Projects/Rust/aicore/docs/std-compatibility.md`
- `/Users/kasunranasinghe/Projects/Rust/aicore/docs/io-api-reference.md`
