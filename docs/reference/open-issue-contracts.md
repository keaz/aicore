# Open Issue Contracts Reference

See also: [Spec](../spec.md), [Type System](../type-system.md), [Architecture](../architecture.md), [Contributing](../contributing.md), [Syntax](./syntax.md), [Types](./types.md), [Generics](./generics.md), [Memory](./memory.md), [Effects](./effects.md)

This page is an AI-agent-facing contract for open language issues:

- `#128` `[LANG-T4] Tuple types`
- `#130` `[LANG-T6] Struct methods and method call syntax`
- `#136` `[TYPE-T1] Trait method declarations and dynamic dispatch`
- `#137` `[TYPE-T2] Borrow checker completeness`
- `#138` `[TYPE-T3] Generic type constraints and where clauses`
- `#139` `[TYPE-T4] Improved type inference`

The sections below intentionally separate `Current behavior` from `Target behavior`. `Target behavior` is a design and implementation contract, not a claim that features are already implemented.

## Issue #128: Tuple types

### Syntax form

- Current behavior:
  - Type grammar accepts `()` (unit) and named/generic types; `(T, U)` is not accepted.
  - Expression grammar treats `(expr)` as grouping and `()` as unit only; tuple literals are not accepted.
  - `let` and `match` patterns do not support tuple destructuring.
  - Postfix field syntax requires identifier tokens, so tuple index projection like `.0` is rejected.
- Target behavior:
  - Add tuple type syntax `(T1, T2, ..., Tn)` with `n >= 2`.
  - Add tuple literal syntax `(e1, e2, ..., en)` with `n >= 2`.
  - Add tuple pattern syntax in `let` and `match`: `(p1, p2, ..., pn)`.
  - Add tuple projection syntax `expr.<index>` with zero-based indices.
  - Preserve existing grouping `(<expr>)` and unit `()` behavior unchanged.

### Type/effect/borrow behavior

- Current behavior:
  - No tuple type constructor in AST/IR/type checker.
- Target behavior:
  - Tuple literal type is the pointwise product of element types.
  - Tuple literal evaluation is left-to-right; effect usage is the union of element effects.
  - Borrow rules apply both to tuple values and projected elements (projection must not bypass alias checks).
  - Generic substitution must work inside tuple elements.

### Expected diagnostics

- Current behavior:
  - Tuple-like syntax fails with existing parser diagnostics (for example `E1025`, `E1031`, `E1036`, `E1037`, `E1046`).
- Target behavior:
  - Add dedicated diagnostics for:
    - invalid tuple arity (`n < 2`) in tuple type/literal/pattern contexts
    - tuple projection index out of bounds
    - tuple projection on non-tuple values
    - tuple-pattern arity mismatch against scrutinee type
  - Register all new codes before feature rollout (`src/diagnostic_codes.rs` and `docs/diagnostic-codes.md`).

### Minimal test matrix template

| Case | Source sketch | Expected |
| --- | --- | --- |
| Parse tuple type | `fn f() -> (Int, String) { ... }` | Parses and lowers deterministically |
| Parse tuple literal | `let t = (1, "x");` | Literal accepted with correct arity |
| Projection typing | `t.0` / `t.1` | Correct element type |
| Destructure in `let` | `let (a, b) = t;` | Pattern binds correctly |
| Destructure in `match` | `match t { (x, y) => ... }` | Pattern typing + coverage checks |
| Negative index/type | `t.2`, `x.0` | Deterministic error diagnostics |

### Agent implementation checklist

- [ ] Extend lexer/parser grammar for tuple types, literals, patterns, and numeric projections.
- [ ] Add tuple nodes in AST, IR, formatter, and JSON schema compatibility paths.
- [ ] Implement tuple typing rules, projection typing, and pattern typing/exhaustiveness interactions.
- [ ] Integrate tuple borrow semantics with existing alias/mutability checks.
- [ ] Add parser/typecheck/compile-fail and execution tests covering positive and negative paths.
- [ ] Update reference docs (`syntax`, `types`, `expressions`, `pattern-matching`) and diagnostics registry docs.

## Issue #130: Struct methods and method-call syntax

### Syntax form

- Current behavior:
  - `impl` supports marker trait implementations (`impl Trait[Type];`) only.
  - Inherent impl blocks (`impl Type { ... }`) are not parsed.
  - Method-call surface syntax (`value.method(...)`) is parsed as field-access + call and then fails during resolution/type checking.
  - Associated function syntax (`Type::method(...)`) is not available in expression grammar.
- Target behavior:
  - Add inherent impl blocks: `impl Type { fn ... }`.
  - Add receiver forms in method declarations: `self`, `&self`, `&mut self`.
  - Add method-call resolution: `value.method(args)` desugars to a function-style call with an explicit receiver.
  - Add associated function call syntax: `Type::name(args)`.
  - Allow multiple impl blocks per type (deterministic merge rules).

### Type/effect/borrow behavior

- Current behavior:
  - No method receiver model in resolver/type checker.
- Target behavior:
  - Receiver typing contract:
    - `self`: consumes receiver value
    - `&self`: shared borrow
    - `&mut self`: mutable borrow with exclusivity checks
  - Method call effect accounting follows normal function effect rules; no implicit effect exemptions.
  - Method dispatch is static in MVP (resolved at compile time from receiver type + method name).

### Expected diagnostics

- Current behavior:
  - Inherent impl/method declarations fail with existing parser errors.
  - `value.method(...)` often fails as module-qualifier/callable resolution error.
- Target behavior:
  - Add dedicated diagnostics for:
    - unknown method on receiver type
    - receiver mutability mismatch (`&mut self` method on immutable receiver)
    - method arity/type mismatch after receiver binding
    - invalid associated call form (instance-only vs associated-only misuse)
  - Keep diagnostics deterministic and include receiver type in messages.

### Minimal test matrix template

| Case | Source sketch | Expected |
| --- | --- | --- |
| Inherent method parse | `impl User { fn is_adult(&self) -> Bool { ... } }` | Parses and lowers |
| Associated call | `User::new("A", 1)` | Resolves to associated function |
| Instance method call | `u.is_adult()` | Resolves with receiver binding |
| Mutable receiver | `u.bump_age()` with `&mut self` | Borrow checks enforce mutability |
| Effect propagation | `u.greet()` where method is `effects { io }` | Caller must declare required effects |
| Negative resolution | missing method / wrong args | Deterministic method diagnostics |

### Agent implementation checklist

- [ ] Add syntax and AST/IR support for inherent impls, method signatures, and `Type::name(...)`.
- [ ] Extend resolver symbol tables for type-owned methods and associated functions.
- [ ] Implement receiver-aware method resolution and call desugaring in type checker.
- [ ] Integrate receiver borrowing/consumption semantics with existing borrow checker.
- [ ] Update formatter + codegen lowering path for method/associated calls.
- [ ] Add parser/resolver/typecheck/execution tests and update docs/reference pages.

## Issue #136: Trait method declarations and dynamic dispatch

### Syntax form

- Current behavior:
  - Traits are marker-only declarations (`trait Name[T];`).
  - Trait impls are marker-only (`impl Trait[Type];`), with no method bodies.
  - Trait-bound generic method invocation (`x.method()`) through trait signatures is unavailable.
- Target behavior:
  - Extend trait declarations to include method signatures.
  - Extend trait impl declarations to provide required method implementations.
  - Resolve trait method calls via generic bounds (static dispatch MVP).
  - Dynamic dispatch via `dyn Trait` is optional and gated as stretch behavior after static dispatch is complete.

### Type/effect/borrow behavior

- Current behavior:
  - Trait bounds only act as marker constraints.
- Target behavior:
  - Trait impl method signatures must match trait declarations exactly (name, receiver shape, params, return type, effects).
  - Generic calls constrained by trait bounds resolve method symbols using the concrete type substitution.
  - Receiver borrow/ownership semantics are enforced consistently with inherent methods.
  - If dynamic dispatch is implemented, object-safety constraints and vtable layout rules must be explicit and test-covered.

### Expected diagnostics

- Current behavior:
  - Trait bodies and trait impl bodies fail parse with current trait/impl declaration diagnostics.
- Target behavior:
  - Add dedicated diagnostics for:
    - missing required trait methods in impl
    - trait method signature mismatch in impl
    - method call on type missing required trait bound
    - invalid dyn/object-safe usage (if dyn dispatch is enabled)
  - Keep existing bound-failure diagnostic behavior consistent for marker and method traits.

### Minimal test matrix template

| Case | Source sketch | Expected |
| --- | --- | --- |
| Trait with method signatures | `trait Display { fn to_string(self) -> String; }` | Parses and lowers |
| Valid trait impl | `impl Display for User { fn to_string(self) -> String { ... } }` | Signature checks pass |
| Generic bound call | `fn show[T: Display](x: T) -> String { x.to_string() }` | Resolves and type-checks |
| Signature mismatch | wrong return/effects/receiver | Deterministic trait mismatch diagnostic |
| Missing method | impl omits required method | Deterministic missing-method diagnostic |
| Optional dyn dispatch | `dyn Display` call path | Works or emits explicit object-safety diagnostic |

### Agent implementation checklist

- [ ] Extend AST/IR data models for trait method signatures and trait impl method bodies.
- [ ] Update parser/formatter for trait and trait-impl method syntax.
- [ ] Implement resolver/type checker conformance checks between trait methods and impl methods.
- [ ] Implement trait-bound method resolution in generic calls (static dispatch MVP).
- [ ] Gate dynamic dispatch work behind explicit feature scope and diagnostics.
- [ ] Add complete positive/negative tests and update diagnostics/reference docs.

## Issue #137: Borrow checker completeness

### Syntax form

- Current behavior:
  - Surface borrow syntax supports `&x` and `&mut x`.
- Target behavior:
  - No new syntax required for MVP completion; improvements are semantic.

### Type/effect/borrow behavior

- Current behavior:
  - Borrow checks are local and lexical with deterministic diagnostics (`E1263`-`E1269`).
  - Checks cover mutable-vs-shared aliasing and assignment-while-borrowed for local bindings.
  - Move semantics, use-after-move checks, and cross-function borrow lifetime reasoning are incomplete.
- Target behavior:
  - Add move tracking and reject use-after-move.
  - Extend borrow reasoning through function calls, return paths, and struct-field projections.
  - Reject aliased mutable borrows and illegal shared/mutable overlap across control-flow joins.
  - Define and enforce deterministic drop/destructor ordering guarantees for checked ownership transitions.
  - Preserve current diagnostics for existing cases and add new diagnostics for newly enforced violations.

### Expected diagnostics

- Current behavior:
  - `E1263`-`E1269` cover current alias/mutability failures.
- Target behavior:
  - Keep existing code meanings stable.
  - Add dedicated diagnostics for:
    - use-after-move
    - moving out of borrowed content
    - escaping borrowed references beyond valid lifetime/scope
    - invalid borrow across function boundary or return
  - New codes must be registered before release.

### Minimal test matrix template

| Case | Source sketch | Expected |
| --- | --- | --- |
| Existing alias checks | overlapping `&` and `&mut` | Existing diagnostics preserved |
| Use-after-move | move then reuse value | Compile-time error |
| Borrow through call | borrowed arg + conflicting mutation | Compile-time error |
| Struct field borrow | borrow field then mutate owner | Compile-time error where required |
| Branch/loop join | borrow state merges across CFG edges | Deterministic acceptance/rejection |
| Drop ordering | ownership transfer and scope exit | Deterministic behavior + tests |

### Agent implementation checklist

- [ ] Introduce move-state tracking in semantic analysis.
- [ ] Extend borrow state model across call boundaries and CFG merges.
- [ ] Add field-level/aggregate ownership interaction checks.
- [ ] Preserve existing error-code behavior and add registered codes for new violations.
- [ ] Expand compile-fail fixtures to cover every violation class.
- [ ] Add run-pass cases proving accepted ownership patterns remain valid.

## Issue #138: Generic constraints and `where` clauses

### Syntax form

- Current behavior:
  - Inline trait bounds on generics are available, including multi-bounds (`T: A + B`).
  - `where` clauses are not part of grammar.
- Target behavior:
  - Add `where` clause grammar on generic declarations, at minimum for functions:
    - `fn f[T, U](...) -> R where T: A, U: B { ... }`
  - Normalize inline bounds and `where` bounds into one internal constraint set.
  - Keep existing inline bound syntax fully supported.

### Type/effect/borrow behavior

- Current behavior:
  - Bound checks rely on explicit trait impl lookup; missing bound yields deterministic error.
- Target behavior:
  - Bound satisfaction must be equivalent regardless of declaration location (inline vs `where`).
  - Multiple bounds are conjunctive (`T: A + B` means both required).
  - No special effect or borrow exceptions are introduced by bound syntax.
  - If associated-type constraints are included in scope, they must participate in the same normalization and satisfaction checks.

### Expected diagnostics

- Current behavior:
  - Invalid/missing trait bounds use existing trait-bound diagnostics.
  - `where` usage currently fails parse/block expectations.
- Target behavior:
  - Add deterministic diagnostics for malformed `where` clauses and duplicate/conflicting bounds.
  - Reuse existing bound-failure diagnostics where semantics are unchanged.
  - Include help text that shows equivalent inline/`where` rewrites.

### Minimal test matrix template

| Case | Source sketch | Expected |
| --- | --- | --- |
| Inline multi-bound | `T: A + B` | Both bounds enforced |
| `where` equivalent | same constraint expressed in `where` | Same type-check result as inline |
| Mixed constraints | inline + `where` together | Combined normalized constraints |
| Missing impl | call with non-conforming type | Bound-failure diagnostic |
| Malformed `where` | bad clause shape | Deterministic parser/type diagnostics |
| Generic + effects path | constrained generic calls effectful callee | Effect checks unchanged |

### Agent implementation checklist

- [ ] Add lexer/parser support for `where` clause syntax.
- [ ] Extend AST/IR to carry normalized constraint sets.
- [ ] Merge inline and `where` bounds in resolver/type checker.
- [ ] Ensure existing trait-bound diagnostics remain stable where semantics match.
- [ ] Add parser/typecheck tests for positive and negative bound scenarios.
- [ ] Update generics/type reference docs with canonical examples.

## Issue #139: Improved type inference

### Syntax form

- Current behavior:
  - Function signatures and most closure signatures require explicit types.
- Target behavior:
  - Keep function signatures explicit by design.
  - Expand local inference in expression/block scopes without changing signature explicitness.

### Type/effect/borrow behavior

- Current behavior:
  - Local `let` inference and generic-argument inference from direct call arguments are supported.
  - Inference failures produce deterministic errors (`E1204`, `E1212`); closure params without explicit types currently fail (`E1280`).
  - Inference does not use later usage chains such as container operations (`new_vec(); push(...)`) to complete unknowns.
- Target behavior:
  - Infer closure parameter types from expected function context when available.
  - Infer generic type arguments from constrained local usage within bounded local scope.
  - Improve `Option`/`Result` contextual inference in match/branch contexts.
  - Never guess on ambiguity: unresolved or conflicting constraints must emit clear diagnostics.
  - Borrow/effect checks run on finalized inferred types and retain existing safety guarantees.

### Expected diagnostics

- Current behavior:
  - `E1204`, `E1212`, `E1280` are common inference-failure outcomes.
- Target behavior:
  - Preserve deterministic failure on ambiguity.
  - Add/adjust diagnostics to distinguish:
    - insufficient type context
    - conflicting inferred constraints
    - inference cycle/recursion limits (if applicable)
  - Keep messages actionable with concrete annotation suggestions.

### Minimal test matrix template

| Case | Source sketch | Expected |
| --- | --- | --- |
| Closure context inference | closure passed to typed `Fn(...) -> ...` callee | Parameter types inferred |
| Generic usage inference | `let v = new_vec(); push(v, 42);` | Concrete `Vec[Int]` inferred |
| Option/Result context | pattern/match-driven inference | Correct variant typing |
| Ambiguous case | insufficient constraints | Explicit deterministic diagnostic |
| Conflicting constraints | incompatible later uses | Conflict diagnostic |
| Signature boundary | omitted function signature types | Still rejected by explicit-signature policy |

### Agent implementation checklist

- [ ] Define inference boundary rules (what is local, what is out-of-scope).
- [ ] Extend constraint collection/solving for closure and usage-driven inference.
- [ ] Preserve deterministic ordering in solver decisions and diagnostics.
- [ ] Validate inferred types before effect/borrow analysis; reject unresolved outputs.
- [ ] Add focused unit tests for each inference class and ambiguity path.
- [ ] Update docs/examples to show explicit-signature policy remains intact.
