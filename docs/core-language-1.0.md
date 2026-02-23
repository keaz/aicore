# Core Language 1.0 (Agent Guide)

This guide is the implementation-facing contract for Core Language features (`CL-T1` .. `CL-T6`).

## Scope

Core Language 1.0 in AICore includes:

- `async fn` / `await` with `Async[T]` typing
- traits + impls + bounded generics (`T: Trait`)
- Result propagation operator (`expr?`)
- explicit mutability (`let mut`) + assignment + borrow forms (`&x`, `&mut x`)
- pattern matching 1.0:
  - exhaustiveness + unreachable-arm checks
  - pattern-or (`p1 | p2`)
  - guard typing (`pattern if cond => expr`)

No `null` values are allowed; absence is only `Option[T]`.

## Deterministic Authoring Rules (for agents)

- Always emit explicit return types.
- Always use explicit imports.
- Keep effects explicit (`effects { ... }`) on impure functions.
- Prefer canonical formatter output from `aic fmt`.
- Treat diagnostics as stable APIs (`E####`), not free-form text.

## Core Syntax

```aic
import std.io;

trait Order[T];
impl Order[Int];

fn checked_inc(x: Int) -> Result[Int, Int] {
    if x >= 0 { Ok(x + 1) } else { Err(0 - x) }
}

fn pick[T: Order](a: T, b: T) -> T {
    a
}

fn select(x: Option[Int], allow: Bool) -> Int {
    match x {
        None | Some(_) if allow => 1,
        Some(v) => v,
        None => 0,
    }
}

fn main() -> Int effects { io } {
    let mut v = checked_inc(41)?;
    v = v + 1;
    print_int(select(Some(v), true));
    0
}
```

## Pattern Matching 1.0 Details

### Or-patterns

- Syntax: `p1 | p2 | ...`
- Alternatives must bind the same variable names (`E1271` if mismatched).
- Bound variable types must be compatible across alternatives (`E1272`).

Example (valid):

```aic
fn f(x: Bool) -> Int {
    match x {
        true | false => 1,
    }
}
```

Example (invalid bindings):

```aic
fn f(x: Option[Int]) -> Int {
    match x {
        Some(v) | None => 1, // E1271
        _ => 0,
    }
}
```

### Guards

- Syntax: `pattern if <expr> => body`
- Guard expression must be `Bool` (`E1270`).
- Guarded arms do not count for exhaustiveness.

Example:

```aic
fn f(x: Bool, allow: Bool) -> Int {
    match x {
        true if allow => 1,
        false => 0,
        _ => 2,
    }
}
```

Backend note:

- LLVM backend currently rejects guarded match lowering with `E5023`.
- `aic check` supports guard typing/exhaustiveness analysis.

## Diagnostics You Will See Often

- `E1251`: unreachable match arm
- `E1252`: duplicate variable binding in a pattern
- `E1260-E1262`: invalid Result propagation (`?`)
- `E1263-E1269`: borrow/mutability/assignment discipline
- `E1270-E1272`: guard/or-pattern correctness
- `E5023`: guarded match not lowered by backend yet

## Implementation Map

- Parser and grammar: `src/lexer.rs`, `src/parser.rs`, `docs/grammar.ebnf` (frozen source contract: `docs/syntax.md`)
- AST/IR contracts: `src/ast.rs`, `src/ir.rs`, `src/ir_builder.rs`
- Canonical printing: `src/formatter.rs`
- Type and pattern checks: `src/typecheck.rs`
- Contract expression cloning/traversal: `src/contracts.rs`
- LLVM lowering behavior: `src/codegen.rs`

## Validation Workflow

Use this sequence before pushing:

```bash
make lint
make ci
```

Focused checks:

```bash
cargo test --test unit_tests unit_or_pattern_binding_type_mismatch_reports_e1272
cargo test --test execution_tests exec_bool_or_pattern
cargo test --test golden_tests golden_case16_match_or_guard
```

## Examples

- `examples/core/async_ping.aic`
- `examples/core/trait_sort.aic`
- `examples/core/result_propagation.aic`
- `examples/core/mut_vec.aic`
- `examples/core/pattern_or.aic`
- `examples/core/pattern_guard_check.aic`
