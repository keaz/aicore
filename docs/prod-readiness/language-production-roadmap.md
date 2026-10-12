# AICore Language Production-Readiness Roadmap

Tracking epic: [#420](https://github.com/keaz/aicore/issues/420). Label: `lang-prod-readiness`.

This roadmap comes from an audit of commit `2a11587` (2026-10). The audit ran about 200 probe programs through a fresh `cargo build --release` of the Rust reference compiler (the default `aic` front end and backend). It focuses on language features, data types and constructs that help AI agents write correct code.

## Current state summary

Strong today:

- Effect tracking (direct and transitive, with call-path diagnostics), no null, no implicit coercions.
- Generic structs/enums, trait bounds, `dyn Trait`, tuples, closures, `for` loops, iterator chains, `?` on Result, async/await, `const`, type aliases, module privacy.
- `#[test]` and `#[property]` (with shrinking), structured JSON diagnostics, and the agent tooling commands (`query`, `context`, `patch`, `testgen`, `checkpoint`, `session`, `validate-call`).

Blocking production use:

- Silent wrong results: Vec copies alias a shared buffer, contracts on impl methods are ignored, closure writes to captured `mut` locals are lost, integer division by zero is undefined behaviour, integer overflow wraps silently.
- `aic check` passing does not imply `aic build` succeeds (E5016, E5017, E5006, E5011, E5019); exhaustiveness is incorrect for nested patterns.
- String, Vec and struct heap memory is never freed; there are no recursive data types; panics terminate the process.
- Missing everyday constructs: field/index/compound assignment, block expressions, early `return` in nested blocks, multi-field enum variants, `if let`, casts, indexing, String concatenation.
- Thin trait system: no default methods, associated types, supertraits, generic impls, core traits or derive.
- Verification is shallow: interval-only verifier, no call-site `requires` checking, no effect polymorphism, capabilities duplicate effects.
- CI on `main` is red (Windows runtime build), docs overstate features, no tagged release.

## Phases

Phases run in order. Items in the Continuous phase start at the position shown in the global order and run alongside later phases.

- **Phase 0 - Unblock: green CI and no compiler hangs**
- **Phase 1 - Soundness: no silent wrong results, `check` agrees with `build`**
- **Phase 2 - Core statement and expression constructs**
- **Phase 3 - Memory management and ownership**
- **Phase 4 - Traits, derive, and generic data**
- **Phase 5 - Agent-grade verification (contracts, effects, refinement, typestate)**
- **Continuous - Diagnostics, docs, release (run in parallel with every phase)**

## Epics

| Epic | Issue | Phase | Child issues |
|---|---|---|---|
| [EPIC CORR] Silent wrong results and soundness fixes | [#421](https://github.com/keaz/aicore/issues/421) | Phase 1 | 10 |
| [EPIC PARITY] Pattern matching correctness and check/build parity | [#422](https://github.com/keaz/aicore/issues/422) | Phase 1 | 4 |
| [EPIC SYNTAX] Core statement and expression constructs | [#423](https://github.com/keaz/aicore/issues/423) | Phase 2 | 16 |
| [EPIC MEM] Memory management and ownership | [#424](https://github.com/keaz/aicore/issues/424) | Phase 3 | 6 |
| [EPIC TRAIT] Traits, derive, and generic data | [#425](https://github.com/keaz/aicore/issues/425) | Phase 4 | 9 |
| [EPIC VERIFY] Agent-grade verification: contracts, effects, refinement, typestate | [#426](https://github.com/keaz/aicore/issues/426) | Phase 5 | 12 |
| [EPIC DIAG] Diagnostics that let agents fix code in one iteration | [#427](https://github.com/keaz/aicore/issues/427) | Continuous | 6 |
| [EPIC REL] Release health: CI, docs accuracy, examples, hygiene | [#428](https://github.com/keaz/aicore/issues/428) | Continuous | 7 |

## Global implementation order

| # | Issue | Key | Title | Effort | Depends on |
|---|---|---|---|---|---|
| 1 | [#429](https://github.com/keaz/aicore/issues/429) | `REL-T1` | Fix Windows runtime build (`pthread_once_t`) and restore green CI on main | S-M | - |
| 2 | [#430](https://github.com/keaz/aicore/issues/430) | `CORR-T1` | Parser loops forever on trait `type`/`const` items and default method bodies | S | - |
| 3 | [#431](https://github.com/keaz/aicore/issues/431) | `DIAG-T6` | `aic check` takes 40-140 s on a one-line file depending on path form and cwd | S | - |
| 4 | [#432](https://github.com/keaz/aicore/issues/432) | `DIAG-T3` | Line/column in JSON spans; diagnostics printed twice | S | - |
| 5 | [#433](https://github.com/keaz/aicore/issues/433) | `CORR-T2` | Contracts on inherent `impl` methods are silently ignored | S | [#429](https://github.com/keaz/aicore/issues/429) |
| 6 | [#434](https://github.com/keaz/aicore/issues/434) | `CORR-T5` | Integer division and modulo by zero are undefined behaviour | S | - |
| 7 | [#435](https://github.com/keaz/aicore/issues/435) | `CORR-T6` | `return` inside if/while/loop/match is rejected; `return`/`break`/`continue` are not typed as diverging | M | [#429](https://github.com/keaz/aicore/issues/429) |
| 8 | [#436](https://github.com/keaz/aicore/issues/436) | `CORR-T7` | Unknown type names pass `aic check` and only fail in codegen (E5019) | S | - |
| 9 | [#437](https://github.com/keaz/aicore/issues/437) | `CORR-T3` | Closure writes to captured `mut` locals are silently lost | M | - |
| 10 | [#438](https://github.com/keaz/aicore/issues/438) | `CORR-T4` | Vec copies alias a shared buffer; functional-style API mutates other values | M | - |
| 11 | [#439](https://github.com/keaz/aicore/issues/439) | `CORR-T8` | 223 std `*_intrinsic` functions have dummy bodies that `verify-intrinsics` does not check | S | - |
| 12 | [#440](https://github.com/keaz/aicore/issues/440) | `CORR-T9` | User symbols collide with std names (`count`, `size`, `len`, method `get`) | M | - |
| 13 | [#441](https://github.com/keaz/aicore/issues/441) | `CORR-T10` | Generic struct literal passed directly to a generic function fails inference (E1226) | S | - |
| 14 | [#442](https://github.com/keaz/aicore/issues/442) | `PARITY-T1` | Exhaustiveness and reachability are wrong for nested, tuple and literal patterns | M | - |
| 15 | [#443](https://github.com/keaz/aicore/issues/443) | `PARITY-T2` | Lower `match` on Int, String, Char and struct patterns (E5016) | M | [#442](https://github.com/keaz/aicore/issues/442) |
| 16 | [#444](https://github.com/keaz/aicore/issues/444) | `PARITY-T3` | Lower nested and literal enum payload patterns (E5017) | M | [#443](https://github.com/keaz/aicore/issues/443) |
| 17 | [#445](https://github.com/keaz/aicore/issues/445) | `PARITY-T4` | Enforce check/build parity: `check` must reject anything codegen cannot lower | M | [#443](https://github.com/keaz/aicore/issues/443), [#444](https://github.com/keaz/aicore/issues/444) |
| 18 | [#446](https://github.com/keaz/aicore/issues/446) | `REL-T4` | Repository hygiene: remove stray files and extend `.gitignore` | S | - |
| 19 | [#447](https://github.com/keaz/aicore/issues/447) | `REL-T5` | Decide self-host compiler scope: freeze or commit to full parity | M | - |
| 20 | [#448](https://github.com/keaz/aicore/issues/448) | `SYNTAX-T1` | Place-expression assignment: fields, nested fields, indexes, tuple elements, `&mut self` | M | - |
| 21 | [#449](https://github.com/keaz/aicore/issues/449) | `SYNTAX-T2` | Compound assignment operators `+=`, `-=`, `*=`, `/=`, `%=` | S | [#448](https://github.com/keaz/aicore/issues/448) |
| 22 | [#450](https://github.com/keaz/aicore/issues/450) | `SYNTAX-T3` | Block expressions, multi-statement match arms, and `if` without `else` | M | - |
| 23 | [#451](https://github.com/keaz/aicore/issues/451) | `SYNTAX-T7` | Bare unit enum variants in expressions and qualified variant paths | S | - |
| 24 | [#452](https://github.com/keaz/aicore/issues/452) | `SYNTAX-T5` | `if let`, `while let`, let-else, and struct destructuring in `let` | M | [#435](https://github.com/keaz/aicore/issues/435), [#442](https://github.com/keaz/aicore/issues/442) |
| 25 | [#453](https://github.com/keaz/aicore/issues/453) | `SYNTAX-T6` | `?` on Option and a complete Option/Result API | S | - |
| 26 | [#454](https://github.com/keaz/aicore/issues/454) | `SYNTAX-T4` | Multi-field and struct-like enum variants | M | [#442](https://github.com/keaz/aicore/issues/442) |
| 27 | [#455](https://github.com/keaz/aicore/issues/455) | `SYNTAX-T8` | Indexing `v[i]`, array literals, and method-call syntax for std collections | M | - |
| 28 | [#456](https://github.com/keaz/aicore/issues/456) | `SYNTAX-T9` | Explicit numeric casts (`as` or conversion functions) with defined semantics | M | - |
| 29 | [#457](https://github.com/keaz/aicore/issues/457) | `SYNTAX-T10` | Labeled break/continue, `break` value typing, and loops without trailing `;` | S | - |
| 30 | [#458](https://github.com/keaz/aicore/issues/458) | `SYNTAX-T11` | Ranges as values, range patterns, negative/float literal patterns, `@` bindings | M | - |
| 31 | [#459](https://github.com/keaz/aicore/issues/459) | `SYNTAX-T13` | Lexer gaps: block comments, binary literals, digit separators | S | - |
| 32 | [#460](https://github.com/keaz/aicore/issues/460) | `SYNTAX-T14` | Closure ergonomics: optional return annotation, parameter inference, field-closure calls, generic closure returns | M | - |
| 33 | [#461](https://github.com/keaz/aicore/issues/461) | `SYNTAX-T15` | Explicit generic type arguments at call sites | S | - |
| 34 | [#462](https://github.com/keaz/aicore/issues/462) | `SYNTAX-T16` | Tuple structs and unit structs | S | - |
| 35 | [#463](https://github.com/keaz/aicore/issues/463) | `SYNTAX-T12` | String ergonomics: concatenation, ordering, raw/multiline literals, interpolation of non-String values | M | [#481](https://github.com/keaz/aicore/issues/481) |
| 36 | [#464](https://github.com/keaz/aicore/issues/464) | `DIAG-T1` | 'Did you mean' suggestions for symbols, fields, methods and std functions | S | - |
| 37 | [#465](https://github.com/keaz/aicore/issues/465) | `DIAG-T2` | Machine-applicable fixes for missing imports and non-exhaustive matches; `unreachable(msg)` builtin | S | [#442](https://github.com/keaz/aicore/issues/442) |
| 38 | [#466](https://github.com/keaz/aicore/issues/466) | `DIAG-T5` | Misleading parse errors for common mistakes | S | - |
| 39 | [#467](https://github.com/keaz/aicore/issues/467) | `DIAG-T4` | Specific `aic explain` text for every diagnostic code | S | - |
| 40 | [#468](https://github.com/keaz/aicore/issues/468) | `REL-T3` | All examples must pass `check` and `build`; gate in CI | S | [#450](https://github.com/keaz/aicore/issues/450), [#443](https://github.com/keaz/aicore/issues/443) |
| 41 | [#469](https://github.com/keaz/aicore/issues/469) | `MEM-T1` | Memory model design record: ownership-driven drop vs reference counting | M | - |
| 42 | [#470](https://github.com/keaz/aicore/issues/470) | `MEM-T2` | Free String, Vec, struct and enum heap memory; add real leak gates | L | [#469](https://github.com/keaz/aicore/issues/469), [#438](https://github.com/keaz/aicore/issues/438) |
| 43 | [#471](https://github.com/keaz/aicore/issues/471) | `MEM-T3` | Enforce moves through calls, fields and control-flow joins; partial moves | L | [#469](https://github.com/keaz/aicore/issues/469) |
| 44 | [#472](https://github.com/keaz/aicore/issues/472) | `MEM-T4` | Dereference, `RefMut` write-through, flow-sensitive borrows, and no dangling references | L | [#471](https://github.com/keaz/aicore/issues/471) |
| 45 | [#473](https://github.com/keaz/aicore/issues/473) | `MEM-T5` | Recursive data types via `Box[T]` (lists, trees, ASTs) | L | [#469](https://github.com/keaz/aicore/issues/469), [#470](https://github.com/keaz/aicore/issues/470) |
| 46 | [#474](https://github.com/keaz/aicore/issues/474) | `MEM-T6` | Panic recovery boundary and drop-on-panic | M | [#470](https://github.com/keaz/aicore/issues/470) |
| 47 | [#475](https://github.com/keaz/aicore/issues/475) | `REL-T2` | Make docs match the implementation: grammar, reference pages, README status, memory model | M | - |
| 48 | [#476](https://github.com/keaz/aicore/issues/476) | `REL-T6` | First tagged release with binaries and an executable-building smoke test | M | [#429](https://github.com/keaz/aicore/issues/429), [#468](https://github.com/keaz/aicore/issues/468) |
| 49 | [#477](https://github.com/keaz/aicore/issues/477) | `TRAIT-T1` | Default trait method bodies | M | [#430](https://github.com/keaz/aicore/issues/430) |
| 50 | [#478](https://github.com/keaz/aicore/issues/478) | `TRAIT-T2` | Associated types and associated constants in traits | L | [#430](https://github.com/keaz/aicore/issues/430) |
| 51 | [#479](https://github.com/keaz/aicore/issues/479) | `TRAIT-T3` | Supertraits | M | - |
| 52 | [#480](https://github.com/keaz/aicore/issues/480) | `TRAIT-T4` | Generic inherent impls and blanket trait impls (`impl[T] Box[T]`) | L | - |
| 53 | [#481](https://github.com/keaz/aicore/issues/481) | `TRAIT-T5` | Core traits `Eq`, `Ord`, `Hash`, `Display`, `Debug`, `Clone` and structural equality | M | - |
| 54 | [#482](https://github.com/keaz/aicore/issues/482) | `TRAIT-T6` | `#[derive(...)]` for core traits, serialization, and property-test generators | M | [#481](https://github.com/keaz/aicore/issues/481) |
| 55 | [#483](https://github.com/keaz/aicore/issues/483) | `TRAIT-T7` | Generic Map/Set keys and values via `Hash` + `Eq` | M | [#481](https://github.com/keaz/aicore/issues/481) |
| 56 | [#484](https://github.com/keaz/aicore/issues/484) | `TRAIT-T8` | Native generic concurrency payloads (no JSON) and fallible constructors | M | [#470](https://github.com/keaz/aicore/issues/470) |
| 57 | [#485](https://github.com/keaz/aicore/issues/485) | `TRAIT-T9` | Explicit error conversion for `?` via a `From`-style trait | S | - |
| 58 | [#486](https://github.com/keaz/aicore/issues/486) | `VERIFY-T1` | Checked integer arithmetic by default; explicit wrapping/checked/saturating operations | M | - |
| 59 | [#487](https://github.com/keaz/aicore/issues/487) | `VERIFY-T2` | `panic` and `diverge` effects, or a `total fn` modifier | M | [#435](https://github.com/keaz/aicore/issues/435) |
| 60 | [#488](https://github.com/keaz/aicore/issues/488) | `VERIFY-T7` | Close the FFI effect hole for `extern` calls | S | - |
| 61 | [#489](https://github.com/keaz/aicore/issues/489) | `VERIFY-T3` | SMT-backed contract verification with call-site checking and counterexamples | L | [#433](https://github.com/keaz/aicore/issues/433) |
| 62 | [#490](https://github.com/keaz/aicore/issues/490) | `VERIFY-T4` | Contract language: `old()`, quantifiers, loop invariants and `decreases` | M | [#489](https://github.com/keaz/aicore/issues/489) |
| 63 | [#491](https://github.com/keaz/aicore/issues/491) | `VERIFY-T5` | Contracts on trait methods, inherited by impls | M | [#433](https://github.com/keaz/aicore/issues/433) |
| 64 | [#492](https://github.com/keaz/aicore/issues/492) | `VERIFY-T6` | Effect polymorphism: effects in `Fn` types and effect variables on generics | L | - |
| 65 | [#493](https://github.com/keaz/aicore/issues/493) | `VERIFY-T8` | User-declared effects | M | - |
| 66 | [#494](https://github.com/keaz/aicore/issues/494) | `VERIFY-T9` | Refinement types and opaque newtypes | M | [#462](https://github.com/keaz/aicore/issues/462), [#489](https://github.com/keaz/aicore/issues/489) |
| 67 | [#495](https://github.com/keaz/aicore/issues/495) | `VERIFY-T10` | User-definable typestate / linear resource protocols; unforgeable handles | L | [#471](https://github.com/keaz/aicore/issues/471) |
| 68 | [#496](https://github.com/keaz/aicore/issues/496) | `VERIFY-T12` | `aic testgen` emits placeholder `assert(true)` tests | S | [#489](https://github.com/keaz/aicore/issues/489) |
| 69 | [#497](https://github.com/keaz/aicore/issues/497) | `VERIFY-T11` | Capabilities as values (object-capability model) | L | - |
| 70 | [#498](https://github.com/keaz/aicore/issues/498) | `REL-T7` | Std essentials: CSPRNG, CLI argument parsing, test assertions | M | - |

## Exit criteria

The language is considered production-ready when:

- No known silent-wrong-result bugs remain, and `aic check` passing implies `aic build` succeeds.
- Heap memory is reclaimed and long-running services run in bounded memory.
- The core constructs listed in the SYNTAX, MEM and TRAIT epics are available.
- Contracts are checked at call sites, arithmetic is checked, and effects are polymorphic.
- CI is green on Linux, macOS and Windows, docs match the implementation, and a tagged release exists.

## Process

Every child issue follows the workflow and Definition of Done in `AGENTS.md`. Self-host compiler work per issue follows the scope decision recorded in REL-T5 ([#447](https://github.com/keaz/aicore/issues/447)): the self-host compiler is frozen, roadmap issues change the Rust compiler only, and `make selfhost-frozen-check` must stay green. See `docs/selfhost/scope-decision.md`.
