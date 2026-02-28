# Std API Documentation

This page defines how AICore standard-library API documentation is generated and maintained.

## Source of truth

- `std/*.aic` (including `std/bytes.aic`): canonical API declarations.
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

`aic doc <input> --output <dir>` creates `<dir>` when missing. With the default `--format all`, it emits:

- `index.html`: human-readable browsable API reference with client-side search.
- `index.md`: human-readable markdown API reference suitable for repository docs.
- `api.json`: machine-readable module/item payload (`schema_version = 1`).

Use these commands to validate module and std documentation generation behavior:

<!-- std-api:docgen:start -->
aic doc examples/e4/verified_abs.aic --output target/docs-contract/module-docs
aic doc std/fs.aic --output target/docs-contract/std-fs-docs
<!-- std-api:docgen:end -->

Each command above must produce the following files inside its output directory:

<!-- std-api:docgen-files:start -->
index.html
index.md
api.json
<!-- std-api:docgen-files:end -->

## Module coverage

Current baseline snapshot (`schema_version: 1`) covers these modules:

| Module |
|---|
| `std.bytes` |
| `std.concurrent` |
| `std.deque` |
| `std.env` |
| `std.fs` |
| `std.http` |
| `std.http_server` |
| `std.io` |
| `std.json` |
| `std.map` |
| `std.net` |
| `std.option` |
| `std.path` |
| `std.proc` |
| `std.rand` |
| `std.regex` |
| `std.result` |
| `std.retry` |
| `std.router` |
| `std.string` |
| `std.tls` |
| `std.time` |
| `std.url` |
| `std.vec` |

For exact symbol totals and kind distribution, query the baseline directly:

```bash
jq -r '.symbols | length' /Users/kasunranasinghe/Projects/Rust/aicore/docs/std-api-baseline.json
jq -r '.symbols | group_by(.kind)[] | "\(.[0].kind): \(length)"' /Users/kasunranasinghe/Projects/Rust/aicore/docs/std-api-baseline.json
```

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
- call out effect/capability diagnostics where relevant (`E2001-E2009`).

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

- `/Users/kasunranasinghe/Projects/Rust/aicore/docs/data-bytes.md`
- `/Users/kasunranasinghe/Projects/Rust/aicore/docs/std-api/tls.md`
- `/Users/kasunranasinghe/Projects/Rust/aicore/docs/std-api/machine-readable.md`
- `/Users/kasunranasinghe/Projects/Rust/aicore/docs/std-compatibility.md`
- `/Users/kasunranasinghe/Projects/Rust/aicore/docs/io-api-reference.md`
