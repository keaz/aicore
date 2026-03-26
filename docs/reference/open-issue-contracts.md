# Open Issue Contracts Reference

See also: [Spec](../spec.md), [Type System](../type-system.md), [Architecture](../architecture.md), [Contributing](../contributing.md), [Syntax](./syntax.md), [Types](./types.md), [Generics](./generics.md), [Memory](./memory.md), [Effects](./effects.md)

This page is an AI-agent-facing contract for language issues that are still open or only partially completed:

- `#128` `[LANG-T4] Tuple types`
- `#130` `[LANG-T6] Struct methods and method call syntax`
- `#136` `[TYPE-T1] Trait method declarations and dynamic dispatch`
- `#137` `[TYPE-T2] Borrow checker completeness`
- `#138` `[TYPE-T3] Generic type constraints and where clauses`
- `#139` `[TYPE-T4] Improved type inference`

The sections below intentionally separate `Current behavior` from `Target behavior`. `Target behavior` captures remaining work, not necessarily full feature creation from zero.

## Issue #128: Tuple types

### Syntax form

- Current behavior:
  - Tuple type syntax, tuple literals, tuple destructuring in `let`/`match`, and numeric projection (`.0`, `.1`, ...) are implemented.
  - Grouping `(<expr>)` and unit `()` behavior remains unchanged.
  - Parser currently rejects tuple-wide mutable destructuring (`let mut (a, b) = ...`) and requires mutability at binding level.
- Target behavior:
  - Complete tuple-pattern parity in all lowering paths where tuple branches are still limited.
  - Clarify long-term surface policy for tuple-wide mutable destructuring.

### Type/effect/borrow behavior

- Current behavior:
  - Tuple constructor/projection typing and tuple-pattern arity checks are implemented in the type checker.
  - Tuple values and projected elements participate in existing borrow/mutability checks.
- Target behavior:
  - Extend tuple codegen coverage for remaining tuple-pattern edge cases without regressing deterministic typing/borrow behavior.

### Expected diagnostics

- Current behavior:
  - Tuple index out-of-range, non-tuple projection, and tuple-pattern arity mismatch produce deterministic diagnostics.
  - Invalid tuple syntax continues to use deterministic parser diagnostics.
- Target behavior:
  - Preserve current diagnostics while closing remaining tuple-pattern lowering gaps.

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

- [ ] Add/extend execution coverage for tuple-pattern branches that still have codegen limits.
- [ ] Confirm and document tuple-wide mutable-destructuring policy (`let mut (...)` vs per-binding `mut`).
- [ ] Keep tuple diagnostics and reference pages aligned with runtime behavior.

## Issue #130: Struct methods and method-call syntax

### Syntax form

- Current behavior:
  - Inherent impl blocks (`impl Type { ... }`) and impl methods are parsed and lowered.
  - Method call syntax (`value.method(...)`) and associated call syntax (`Type::name(...)`) resolve in type checking/codegen.
  - Receiver spellings `self`, `self: Type`, `&self`, and `&mut self` are accepted in method signatures.
- Target behavior:
  - Preserve deterministic method/associated-call resolution while tightening remaining receiver-form semantics.
  - Keep impl merge/lookup behavior deterministic across multiple impl blocks.

### Type/effect/borrow behavior

- Current behavior:
  - Method dispatch is static and receiver-aware (`receiver type + method name`) with deterministic unknown-method handling.
  - Method effects participate in normal effect propagation checks.
  - Receiver reference forms currently normalize to `Self` in parser/type paths; distinct `&self` vs `&mut self` borrow semantics are not yet fully differentiated.
- Target behavior:
  - If receiver-reference semantics are split, enforce explicit mutability/alias checks for `&self` and `&mut self` end-to-end.
  - Preserve existing static dispatch behavior and deterministic effect accounting.

### Expected diagnostics

- Current behavior:
  - Unknown method and arity/type mismatch paths produce deterministic method diagnostics.
  - Invalid receiver signatures are diagnosed during parse/typecheck stages.
- Target behavior:
  - If receiver-reference semantics diverge, add dedicated mutability-mismatch diagnostics without changing existing deterministic failure behavior.

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

- [ ] Add explicit tests that distinguish `self`/`&self`/`&mut self` semantics once borrow modeling is separated.
- [ ] Expand negative-path coverage for associated-vs-instance misuse and receiver-shape mismatches.
- [ ] Keep method-call docs and diagnostics examples synchronized with implementation.

## Issue #136: Trait method declarations and dynamic dispatch

### Syntax form

- Current behavior:
  - Traits can declare methods, and trait impl blocks can provide method bodies.
  - Trait-bound generic method invocation (`x.method()`) is supported for static dispatch.
  - `dyn Trait` syntax is supported for object-safe trait method dispatch.
- Target behavior:
  - Expand dyn dispatch coverage where needed without weakening current object-safety guarantees.
  - Keep marker-trait and method-trait behavior coherent under one deterministic resolution model.

### Type/effect/borrow behavior

- Current behavior:
  - Trait impls are checked for required method presence and signature conformance.
  - Generic calls constrained by trait bounds resolve methods deterministically after substitution.
  - Dyn dispatch enforces object-safety restrictions (for example, no trait generics and no invalid `Self` usage in method signatures).
- Target behavior:
  - Extend object-safety and dyn dispatch capabilities only with explicit runtime/typecheck contracts and matching tests.

### Expected diagnostics

- Current behavior:
  - Missing required trait methods and trait signature mismatches emit deterministic diagnostics.
  - Object-safety failures for invalid dyn usage emit deterministic diagnostics (including `E1214` paths).
- Target behavior:
  - Preserve current diagnostics while extending dyn coverage; avoid changing stable bound-failure behavior.

### Minimal test matrix template

| Case | Source sketch | Expected |
| --- | --- | --- |
| Trait with method signatures | `trait Display { fn to_string(self) -> String; }` | Parses and lowers |
| Valid trait impl | `impl Display[User] { fn to_string(self: User) -> String { ... } }` | Signature checks pass |
| Generic bound call | `fn show[T: Display](x: T) -> String { x.to_string() }` | Resolves and type-checks |
| Signature mismatch | wrong return/effects/receiver | Deterministic trait mismatch diagnostic |
| Missing method | impl omits required method | Deterministic missing-method diagnostic |
| Optional dyn dispatch | `dyn Display` call path | Works or emits explicit object-safety diagnostic |

### Agent implementation checklist

- [ ] Add integration coverage for additional dyn-dispatch shapes only after object-safety rules are specified.
- [ ] Keep trait-method/dyn diagnostics examples in sync with parser/typechecker behavior.
- [ ] Expand docs to capture any newly supported object-safe forms.

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
  - Function and trait-method `where` clauses are implemented (including multi-bound forms).
  - Inline and `where` bounds can be mixed on the same declaration.
- Target behavior:
  - Extend `where`-clause support to additional declaration forms only if/when that scope is explicitly adopted.
  - Keep inline and `where` syntaxes semantically equivalent.

### Type/effect/borrow behavior

- Current behavior:
  - Bound checks rely on explicit trait impl lookup; missing bound yields deterministic errors (for example `E1258`).
  - Multiple bounds are conjunctive (`T: A + B` requires both).
  - Constraint syntax location (inline vs `where`) does not change effect or borrow semantics.
- Target behavior:
  - Preserve deterministic bound satisfaction if `where` support is expanded to new surfaces.

### Expected diagnostics

- Current behavior:
  - Invalid/missing trait bounds use existing deterministic trait-bound diagnostics.
  - Malformed `where` syntax fails deterministically in parser/type phases.
- Target behavior:
  - If new `where` forms are added, keep diagnostics deterministic and consistent with existing bound-failure messages.

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

- [ ] Decide and document whether `where` clauses should expand beyond function declarations.
- [ ] Maintain parser/typecheck tests for mixed inline+`where` bounds and malformed-clause paths.
- [ ] Keep generics reference docs aligned with the implemented `where` surface.

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
