You are an expert programming language engineer and compiler architect. Build an **agent-native, verifiable, general-purpose programming language** with **Option<T> (no nulls)** and an **LLVM** backend. The primary author is **autonomous AI agents**, but humans may review and run the output.

## Mission

Design and implement an MVP of a new language (working name: **AICore**) that is:

* **IR-first**: canonical source-of-truth is a structured IR/AST (stable serialization).
* **Round-trip text view**: a human-friendly syntax that parses to the IR and prints back deterministically (stable formatting).
* **Verifiable**: strong static typing + effect system + contracts (requires/ensures/invariants) with a verification pipeline (compile-time when possible, otherwise runtime checks).
* **Deterministic**: reproducible builds, deterministic name resolution and formatting, machine-readable diagnostics.
* **Compiles to LLVM**: generate LLVM IR and produce a native executable (or object file).

Your job: deliver a working repo with a CLI toolchain, a small standard library, tests, and examples.

---

# 0) Non-negotiable constraints

1. **No null**: represent absence only with `Option<T>`.
2. **Deterministic printing**: formatting is canonical; printing the same IR must always yield identical text.
3. **Canonical IR is the truth**: text is a view; the compiler internally operates on IR.
4. **Structured diagnostics**: every error/warning has an error code + JSON output with spans and fix suggestions.
5. **LLVM target**: use LLVM (via `inkwell`/`llvm-sys`/MLIR optional) to produce executables.
6. **Minimal ambiguity**: no implicit coercions; explicit imports; explicit effects.
7. **General purpose**: core language minimal but capable (functions, ADTs, pattern matching, generics).
8. **Verifiability-first**: implement types + effects before advanced features.

---

# 1) Deliverables

Produce:

* A Git repository with:

  * `aic` CLI compiler: `aic build`, `aic run`, `aic check`, `aic fmt`, `aic ir`, `aic diag --json`
  * `aic init` creates a sample project
  * `std/` minimal standard library (Option, Result, Vec/String minimal, printing)
  * `examples/` showing language features
  * `docs/` design docs: syntax, IR schema, type system, effect system, contracts, LLVM backend overview
  * `tests/` unit + golden tests for parser/printer + type/effect checking + LLVM codegen
* A working MVP that can compile and run small programs.

---

# 2) MVP feature set (scope control)

## 2.1 Language constructs

Implement:

* Modules + explicit imports
* `let` bindings (immutable default)
* Functions (first-order OK for MVP; closures optional if time permits)
* Structs and enums (ADTs)
* Pattern matching with exhaustiveness checking
* Generics (parametric polymorphism) for structs/enums/functions (can be limited initially)
* `Option<T>` and `Result<T, E>` in std
* Control flow: `if`, `match`, `return`
* Integers, booleans, strings (can be minimal), unit `()`

Do NOT implement initially:

* Macros
* Traits/typeclasses (unless you keep them tiny)
* Async/await
* GC (prefer explicit allocation strategy via std wrapper)

## 2.2 Effects (must-have)

Implement an effect system:

* Functions are `pure` by default.
* Effects include at least: `io`, `fs`, `net`, `time`, `rand` (even if only `io` is used in MVP).
* Effects are part of function signatures and checked compositionally.
* Standard library functions declare their effects.
* For MVP: enforce effect checking; capability passing is optional but preferred.

## 2.3 Contracts (must-have, staged)

Implement contracts syntax and enforcement:

* `requires <bool-expr>`
* `ensures <bool-expr>` (can refer to `result`)
* `invariant` for structs (checked on construction and mutation if mutation exists)
  Stages:

1. MVP: compile contracts into runtime checks (panic with structured error).
2. Next: build a verification pass for a restricted subset (pure integer logic) that can discharge some contracts at compile time (SMT optional; if too big, implement simple simplifier + constant folding + range checks).

---

# 3) Architecture (required)

Implement the compiler as a pipeline with clear data structures and stable interfaces.

## 3.1 Components

1. **Parser**: text → AST (lossless enough to round-trip)
2. **Canonical IR builder**: AST → IR (canonicalized, resolved)
3. **Formatter/Printer**: IR → canonical text (golden tests)
4. **Resolver**: name resolution + module imports → symbol IDs
5. **Type checker**: assigns types, reports errors with spans
6. **Effect checker**: verifies effect constraints
7. **Contract lowering**: transforms contracts into (a) runtime checks, (b) optional verification obligations
8. **LLVM codegen**: IR → LLVM IR → object/exe
9. **Driver/CLI**: orchestrates build, caching, diagnostics

## 3.2 Canonical IR requirements

* Use stable IDs (SymbolId/TypeId/NodeId) to support deterministic diffs and future patch operations.
* Serialize IR to a canonical format (JSON for debug; CBOR/MessagePack optional).
* Provide `aic ir --emit json` to inspect the IR.

## 3.3 Diagnostics format (required)

Define a JSON schema for diagnostics:

* `code`: stable string like `E0001`
* `severity`: error|warning|note
* `message`
* `spans`: file, start, end, label
* `help` / `suggested_fixes`: text + optional edit operations
  Implement `aic check --json` returning an array of diagnostics.

---

# 4) Language design decisions (make concrete)

You must write down and implement:

* Exact syntax (minimal and unambiguous)
* Type syntax (including `Option[T]` or `Option<T>`)
* Match syntax + pattern grammar
* Module/import syntax
* Effect annotation syntax, e.g.:

  * `fn read_file(path: String) -> Result<String, IoError> effects { fs, io }`
  * or `fn ... -> ... !{io,fs}`
* Contract syntax, e.g.:

  * `fn abs(x: Int) -> Int requires true ensures result >= 0 { ... }`

Pick one consistent style and enforce it.

---

# 5) LLVM backend requirements

Use LLVM to produce a native executable.

## 5.1 Codegen scope

Support codegen for:

* Int/Bool/Unit
* Structs/enums (enums can be tagged unions)
* Option<T> as tagged union (None/Some)
* Function calls, if, match (lower match to branching)
* Strings can be a pointer+len (minimal) with runtime helpers in std

## 5.2 Runtime

Provide a tiny runtime layer (in Rust or C) for:

* printing strings/ints (IO effect)
* panic/trap with message
* minimal allocation strategy (you can start with libc malloc/free or Rust allocator behind FFI)

---

# 6) Testing strategy (required)

1. **Golden tests** for formatter:

   * parse → IR → print should match expected canonical text
   * print → parse → IR must be equivalent
2. **Type/effect tests**:

   * compile-time errors with expected diagnostic codes and spans
3. **LLVM execution tests**:

   * compile examples and run; assert stdout

---

# 7) Milestones and concrete tasks

Implement in this order (do not skip):

1. Repo scaffold + CLI skeleton + `aic init`
2. Lexer/parser + AST + basic pretty-printer
3. Canonical IR + deterministic printer + golden tests
4. Resolver + symbol table
5. Type checker (Int/Bool/Unit, functions, structs, enums, Option/Result)
6. Pattern match + exhaustiveness checking
7. Effect annotations + checker
8. Contracts lowering to runtime checks
9. LLVM codegen for the supported subset
10. Minimal std + runtime + example programs

At each step, ensure `aic check` works and produces structured diagnostics.

---

# 8) Example programs to include

Create and ensure these compile+run:

## 8.1 Option / match

* A function that returns `Option[Int]` and pattern matches it exhaustively.

## 8.2 Effects

* A pure function and an IO function; calling IO from pure should be rejected with a diagnostic.

## 8.3 Contracts

* `abs(x)` with `ensures result >= 0`; include a failing test that triggers runtime contract violation.

## 8.4 Struct + invariant

* A `NonEmptyString` wrapper with invariant `len > 0`.

---

# 9) Output expectations

When done, provide:

* A brief README with install/build/run steps.
* The full language spec (MVP) in `docs/spec.md`.
* A table of implemented features vs planned.
* At least 30 unit tests + 10 golden tests + 5 execution tests.

---

# 10) Implementation guidance (you choose, but justify)

Choose implementation language for the compiler (prefer Rust).
Choose LLVM binding (e.g., inkwell) and document build prerequisites.
Keep the code modular: `parser`, `ir`, `resolver`, `typecheck`, `effects`, `contracts`, `codegen`, `diagnostics`, `cli`.

---

# 11) Important: agent operating mode

Work incrementally and keep everything runnable at every commit:

* Add a small feature
* Add tests
* Ensure `aic check` and `aic fmt` pass
* Only then proceed

If you must cut scope, keep: IR-first + deterministic printer + type checker + effects + LLVM codegen for a small subset.

Begin by proposing the exact syntax, IR schema, and diagnostics schema, then implement the pipeline.
