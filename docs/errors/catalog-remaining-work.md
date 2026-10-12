# Diagnostic Catalog: Remaining Work

Hand-off notes from the catalog rewrite (`Give every diagnostic code a verified trigger and fix in
the catalog`). Read [`catalog.md`](catalog.md) and the "Diagnostic catalog contract" section of
[`../ai-agent-implementation.md`](../ai-agent-implementation.md) first. Each item below is
independent; pick one, follow `AGENTS.md`, and remove the item from this file when it is done.

Run the catalog test after any compiler change:

```bash
cargo test --locked --test diagnostic_catalog_tests
```

## State at hand-off

- Done: one row per registered code (314), with a Mode column: 234 `check`, 3 `warn-unused`,
  13 `build`, 1 `cli`, 19 `package`, 14 `selfhost`, 24 `internal`, 5 `reserved`, 1 `retired`.
  Every runnable trigger reports its code and every fix is error-free (verified by
  `tests/diagnostic_catalog_tests.rs`, registered in `make test-unit`).
- Verification: the full `make ci` passed on the commit before the rebase onto #449/#450. After the
  rebase, the catalog test, `cargo test --lib` and the first half of `make ci` (`fmt-check` through
  `std-doc-check`) passed; `examples-check` through `repro-check` were not re-run. Run `make ci`
  before relying on the merge.
- Not yet re-verified against `lang-prod-roadmap` commits after #450 (for example #451, bare unit
  variants as values). Rebasing onto them may change which code some triggers report; the catalog
  test shows exactly which rows.

## 1. Close the check/build parity gaps (compiler bugs)

Twelve codes have `build` rows whose triggers pass `aic check` and then fail in code generation.
Policy (`docs/diagnostic-codes.md`, "Check/build parity") says only `E5037` may do that. The list,
with one program per gap, is in "Known parity gaps" in `docs/diagnostic-codes.md`.

- Clear checker gaps: `E5002`, `E5004`, `E5006`, `E5007`, `E5012`, `E5019`, `E5023`, `E5031`, `E5036`.
- Edge cases that need a policy decision first:
  - `E5010`, `E5011`: reachable only from a user file that declares `module std.<name>;`. Decide
    whether user files may claim `std.*` module names; if not, reject that and make these `internal`.
  - `E5020`: a user-declared `intrinsic fn` without a runtime lowering. Decide whether user code may
    declare intrinsics; if yes, add a checker rule for unknown intrinsics.
- For each fix: add the checker rule (or make code generation handle the program), move the
  catalog row to the checker code (the `E5xxx` row becomes `internal` and names that code), delete
  the line from "Known parity gaps", and add unit tests. The catalog test fails until the row moves.

## 2. Generic mangling collision (`E1255`)

`f[B, Int]` and `f_B[Int]` both mangle to `fn_f_B_Int` because `_` is both an identifier character
and the separator, so a valid program is rejected (the `E1255` catalog trigger). Make mangling
injective, keep `E1255` as a backstop, and change the `E1255` row to `internal` (or another genuine
trigger).

## 3. Self-host compiler reuses codes with other meanings

The "never reuse a code" policy is broken between the Rust compiler and `compiler/aic`:

- `compiler/aic/libs/ir` emits `E5001`-`E5004` and `E5010`-`E5013` with meanings unrelated to the
  Rust code generation codes of the same number (for example `E5001` = const initializer cannot be
  lowered to IR).
- `compiler/aic/libs/parser` emits `E1042` (`if` without `else`, `pub(` scope), `E1060` (invalid
  type-reference token) and `E1071` (missing `:` after a parameter name). The Rust parser no longer
  emits these, or never reaches them, with those meanings.

The catalog rows describe the Rust meaning where one exists. Decide whether to renumber the
self-host codes (the self-host compiler is frozen, see `docs/selfhost/scope-decision.md`) or to
document the self-host meanings as aliases.

## 4. Package rows without a covering test

These `package` rows name the command that produces the code, but no test asserts the code:
`E2101`, `E2106`, `E2108`, `E2110`, `E2111`, `E2113`, `E2115`. `E2125` reaches `aic lock` only as plain
`error[E2125]: ...` text, not a structured diagnostic. Add CLI or unit tests (see
`tests/e7_cli_tests.rs` and `src/package_registry.rs` tests for the pattern) and cite them in the rows.

## 5. Unverified non-snippet claims

- `E5101`, `E5102`, `E5104` (`selfhost`): the rows describe how the self-host backend produces them,
  but the self-host tools were not run. `E5103`, `E5105`, `E5205` need backend options or library
  calls that `aic_selfhost` never makes.
- `E5034` (`internal`): the claim that `E1303` pre-empts it comes from a code comment and was not
  probed; `E1282` and `E1283` were.
- `E1042` (`selfhost`): the `pub(` trigger in the row was read from the self-host parser source,
  not run.

## 6. `aic explain` text is still range-generic

`src/diagnostic_explain.rs` returns one title/summary per code range (for example every `E10xx`
gets "lexing/parsing front-end diagnostic"). The catalog now has a precise description per code;
consider generating `aic explain` summaries from it, or adding per-code text.

## 7. Smaller compiler findings

Found while writing triggers; none blocks the catalog.

- `E1212` is also reported for an unknown trait in a `dyn` type (`src/typecheck.rs`, around the
  `dyn` resolution); that is a name-resolution failure and probably belongs to `E1228`.
- `E1214` is also reported for every `dyn` object-safety violation, which is not an argument type
  mismatch; consider a dedicated code.
- `E1209`: the "callee path cannot be empty" branch looks unreachable; `x(2)` on a non-function
  local reports `E1218`.
- `E1054` at `src/parser.rs` (first of its two sites, the `expect ';'` right after checking for
  `;`) is a dead branch.
- `E1092` is reported twice for one argument (parser and `src/typecheck.rs`), plus `E1213`.
- `E1253` only fires for `Null`/`NULL`; lowercase `null` is rejected by the parser as `E1051`.
- Reading an element by index in an expression (`let cur = v[0];`) does not parse (`E1033`/`E1041`),
  although `v[0] = x` assigns. Check whether index reads are meant to be supported.
