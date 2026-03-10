# AICore

AICore is an **agent-native, IR-first programming language** designed for **human + AI agent collaboration** on real software. It features deterministic formatting, structured diagnostics, a type + effect checker, design-by-contract support, and an LLVM native backend.

**Inspired by Rust**, AICore inherits many of the principles that make Rust a reliable systems language — strong static typing with no implicit coercions, an ownership and borrow discipline for memory safety, algebraic data types (`enum`/`struct`), exhaustive pattern matching, explicit error handling via `Result[T, E]` and `Option[T]` (no null), and trait-based generics. AICore extends these foundations with an explicit effect system, design-by-contract (`requires`/`ensures`/`invariant`), an IR-first architecture for deterministic tooling, and structured machine-readable diagnostics — all purpose-built for AI agent workflows at scale.

The canonical source of truth is **IR** (`aic ir --emit json`), while text syntax is a deterministic view (`aic fmt`).

Project note: AICore has been developed mainly using **GPT-5.3-Codex** for implementation work, with human review and validation.

---

## Table of Contents

- [Why AICore?](#why-aicore)
- [Language Overview](#language-overview)
  - [Modules and Imports](#modules-and-imports)
  - [Functions](#functions)
  - [Types and Data Structures](#types-and-data-structures)
  - [Control Flow](#control-flow)
  - [Pattern Matching](#pattern-matching)
  - [Generics and Traits](#generics-and-traits)
  - [Effect System](#effect-system)
  - [Design by Contract](#design-by-contract)
  - [Async / Await](#async--await)
  - [Error Handling](#error-handling)
  - [Mutability and Borrow Discipline](#mutability-and-borrow-discipline)
  - [String Interpolation](#string-interpolation)
  - [Visibility and Access Control](#visibility-and-access-control)
- [Standard Library](#standard-library)
- [How AICore Addresses Large-Project Agent Challenges](#how-aicore-addresses-large-project-agent-challenges)
- [Status](#status)
- [Getting Started](#getting-started)
  - [Local Machine Setup Guide](#local-machine-setup-guide)
  - [Prerequisites](#prerequisites)
  - [Build](#build)
  - [Hello World](#hello-world)
- [CLI Reference](#cli-reference)
- [AI-Agent Documentation](#ai-agent-documentation)
- [Local CI with Make](#local-ci-with-make)
- [GitHub Actions](#github-actions)
- [Project Layout](#project-layout)
- [Test Suite](#test-suite)
- [Determinism Guarantees](#determinism-guarantees)
- [Diagnostics](#diagnostics)

---

## Why AICore?

Most existing languages are optimized for human ergonomics first, then toolability as an add-on. AI agents working in large repositories need the opposite balance:

- **Stable, structured outputs** instead of free-form compiler text
- **Deterministic formatting and build behavior** so patches and retries are reproducible
- **Explicit side-effect boundaries** so planning and refactoring are safe
- **Strict compatibility contracts** so automation does not break on minor tool changes

AICore implements these constraints directly in the language semantics and compiler pipeline, making agent workflows a first-class target rather than an afterthought.

---

## Language Overview

### Modules and Imports

Every file can optionally declare a module path. Imports are explicit and deterministic — transitive imports are never implicitly re-exported.

```aic
module app.main;
import std.io;
import std.string;
import app.math;

fn main() -> Int {
    math.add(40, 2)
}
```

- `module` is optional for single-file inputs.
- Qualified calls use `<module_tail>.<symbol>(...)` (e.g. `math.add(...)`).

### Functions

Functions are **pure by default**. Side effects must be explicitly declared.

```aic
fn add(a: Int, b: Int) -> Int {
    a + b
}

fn greet(name: String) -> () effects { io } {
    print(f"Hello, {name}!")
}
```

AICore also supports `async fn`, `intrinsic fn` (runtime-bound FFI stubs), `unsafe fn`, and `extern fn` declarations:

```aic
async fn fetch_data(url: String) -> String effects { net } {
    // async body
    "data"
}

intrinsic fn aic_fs_read_to_string_intrinsic(path: String) -> String effects { fs };

extern "C" fn c_sqrt(x: Float) -> Float;
```

### Types and Data Structures

AICore has a **strong, static type system** with **no implicit coercions** and **no null**.

#### Primitive Types

| Type     | Description                             |
|----------|-----------------------------------------|
| `Int`    | 64-bit signed integer                   |
| `Float`  | 64-bit floating point                   |
| `Bool`   | Boolean (`true` / `false`)              |
| `String` | UTF-8 string                            |
| `Char`   | Unicode scalar value                    |
| `Bytes`  | Binary payload type for IO/networking   |
| `()`     | Unit type (void equivalent)             |

#### Structs

Structs support field defaults and optional `invariant` constraints validated at construction time.

```aic
struct Config {
    host: String = "localhost",
    port: Int = 8080,
    debug: Bool = false,
}

struct NonEmptyString {
    value: String,
} invariant len(value) > 0
```

When all fields have defaults, `TypeName::default()` is automatically synthesized.

#### Enums (Algebraic Data Types)

```aic
enum Option[T] {
    None,
    Some(T),
}

enum Result[T, E] {
    Ok(T),
    Err(E),
}

enum Shape {
    Circle(Float),
    Rectangle(Float),
}
```

Absence of a value is modeled exclusively via `Option[T]` — there is no `null`.

### Control Flow

```aic
// If expressions (they return a value)
let max = if a > b { a } else { b };

// Match expressions
let label = match status {
    0 => "ok",
    1 => "warn",
    _ => "error",
};
```

`if` is an expression, not a statement — both branches must produce the same type.

### Pattern Matching

AICore supports exhaustive pattern matching with or-patterns, guards, wildcard patterns, and destructuring.

```aic
fn describe(opt: Option[Int]) -> String {
    match opt {
        None => "nothing",
        Some(0) => "zero",
        Some(n) if n > 0 => "positive",
        Some(_) => "negative",
    }
}

// Or-patterns
match value {
    None | Some(0) => "empty or zero",
    Some(n) => int_to_string(n),
}
```

**Exhaustiveness is enforced at compile time** for `Bool`, `Option[T]`, `Result[T, E]`, and all user-defined enums. Missing cases are reported as structured diagnostics.

### Generics and Traits

Generic functions support explicit trait bounds. Trait satisfaction is resolved through explicit `impl` declarations — there is no implicit trait inference.

```aic
trait Sortable[T];
trait Printable[T];
impl Sortable[Int];
impl Printable[Int];

fn pick[T: Sortable + Printable](a: T, b: T) -> T {
    a
}
```

- Generic arity is checked statically (e.g. `Option[Int, Int]` is invalid).
- Generic parameters are inferred from call-site arguments.
- Missing `impl` declarations for bound constraints produce deterministic diagnostics.

### Effect System

Every function has an explicit (or empty) effect set. Calling a function with effects requires the caller to declare those effects.

```aic
// Pure function — no effects required
fn add(a: Int, b: Int) -> Int { a + b }

// IO function — must declare `io` effect
fn greet() -> () effects { io } {
    print("hello")
}

// Multiple effects
fn fetch_and_log(url: String) -> String effects { net, io } {
    let data = http_get(url);
    print(data);
    data
}
```

**Standard effects:** `io`, `fs`, `net`, `time`, `rand`, `env`, `proc`, `concurrency`

**Key rules:**
- Callee effects must be a subset of caller declared effects
- Transitive effect analysis catches indirect effect violations with call-path diagnostics
- Effect declarations are canonicalized (sorted, deduplicated) for determinism
- Contracts (`requires`, `ensures`) are checked in pure mode
- `await` does not erase effect obligations

AICore also supports **capability authority** for fine-grained effect provenance:

```aic
fn write_config(path: String) -> () effects { fs } capabilities { fs } {
    // Only runs if fs capability is granted
}
```

**Resource protocol verification** ensures handles (files, sockets, channels, mutexes) are used correctly — use-after-close and double-close are compile-time errors.

### Design by Contract

AICore supports `requires` (preconditions), `ensures` (postconditions), and `invariant` (struct invariants) as first-class language constructs.

```aic
fn divide(a: Int, b: Int) -> Int requires b != 0 {
    a / b
}

fn abs(x: Int) -> Int requires true ensures result >= 0 {
    if x >= 0 { x } else { 0 - x }
}

struct PositiveInt {
    value: Int,
} invariant value > 0
```

- `requires` is checked at function entry
- `ensures` is checked at all function exits (explicit `return` and implicit tail expression)
- `invariant` is validated statically for typing and enforced at runtime on construction
- A restricted static verifier can prove/discharge some integer obligations at compile time

### Async / Await

```aic
async fn fetch_user(id: Int) -> String effects { net } {
    http_get(f"/users/{int_to_string(id)}")
}

async fn main() -> () effects { net, io } {
    let user = await fetch_user(42);
    print(user);
}
```

- `async fn` returns `Async[T]` — consumed only via `await`
- `await` is valid only inside `async fn`
- Async functions participate in the same effect checking as synchronous functions

### Error Handling

AICore uses `Result[T, E]` for error handling with a `?` propagation operator — no exceptions, no implicit conversions.

```aic
fn parse_port(s: String) -> Result[Int, String] {
    let n = string_to_int(s)?;
    if n > 0 { Ok(n) } else { Err("port must be positive") }
}

fn load_config(path: String) -> Result[Config, String] effects { fs } {
    let text = read_to_string(path)?;
    let port = parse_port(text)?;
    Ok(Config { port: port })
}
```

- `expr?` requires `expr: Result[T, E]` and an enclosing return type `Result[U, E]`
- No implicit error conversion — mismatched `E` types are compile-time errors
- Combined with `Option[T]` for total absence of null

### Mutability and Borrow Discipline

Bindings are **immutable by default**. Mutable access requires explicit `let mut`, and the borrow checker enforces aliasing safety.

```aic
fn example() -> Int {
    let mut counter = 0;
    counter = counter + 1;

    let r = &counter;       // immutable borrow
    // counter = 5;         // ERROR: assignment while borrowed (E1265)
    counter
}
```

**Rules:**
- `let mut` is required for reassignment
- `&mut x` requires `x` to be mutable
- Multiple mutable borrows of the same binding are rejected
- Assignments while active borrows exist are rejected
- Borrows are lexically scoped

### String Interpolation

Template strings support expression interpolation:

```aic
let name = "world";
let greeting = f"Hello, {name}!";
let count = 42;
let msg = $"Found {int_to_string(count)} items";
```

- Prefix `f"..."` or `$"..."` activates interpolation
- Interpolated values must be `String` — use explicit converters like `int_to_string()`
- Literal braces are escaped with `\{` and `\}`

### Visibility and Access Control

```aic
pub fn public_api() -> Int { 42 }
pub(crate) fn internal_only() -> Int { 1 }
priv fn private_helper() -> Int { 0 }

pub struct User {
    pub name: String,
    email: String,       // private by default
}
```

- Top-level items default to **private**
- Visibility modifiers: `pub`, `pub(crate)`, `priv`
- Struct fields default to private

---

## Standard Library

AICore ships with a comprehensive standard library covering IO, networking, concurrency, data structures, and more.

| Module           | Description                                                        |
|------------------|--------------------------------------------------------------------|
| `std.io`         | Console IO, formatted output, file reading/writing                 |
| `std.fs`         | Filesystem operations (read, write, copy, move, delete, walk, temp)|
| `std.net`        | TCP/UDP networking, HTTP client                                    |
| `std.tls`        | TLS streams, handshake, and certificate-aware secure transport      |
| `std.http`       | HTTP request/response types                                        |
| `std.http_server`| HTTP server primitives                                             |
| `std.concurrent` | Channels, mutexes, task spawning, async coordination               |
| `std.string`     | String manipulation and conversion                                 |
| `std.vec`        | Dynamic arrays (vectors)                                           |
| `std.map`        | Key-value hash maps                                                |
| `std.set`        | Hash sets                                                          |
| `std.deque`      | Double-ended queues                                                |
| `std.option`     | `Option[T]` type and utilities                                     |
| `std.result`     | `Result[T, E]` type and utilities                                  |
| `std.math`       | Mathematical functions                                             |
| `std.time`       | Time, durations, timestamps, and scheduling                        |
| `std.rand`       | Random number generation                                           |
| `std.regex`      | Regular expression matching, find, capture, replace                 |
| `std.json`       | JSON parsing and serialization                                     |
| `std.env`        | Environment variable access                                        |
| `std.proc`       | Process spawning and management                                    |
| `std.path`       | File path manipulation                                             |
| `std.bytes`      | Binary payload type for IO and networking                          |
| `std.buffer`     | Byte buffer for binary protocol handling                           |
| `std.char`       | Unicode character utilities                                        |
| `std.url`        | URL parsing                                                        |
| `std.log`        | Structured logging                                                 |
| `std.config`     | Configuration loading                                              |
| `std.signal`     | OS signal handling                                                 |
| `std.retry`      | Retry policies with backoff strategies                             |
| `std.router`     | HTTP request routing                                               |
| `std.error_context` | Contextual error wrapping                                       |

---

## How AICore Addresses Large-Project Agent Challenges

| Challenge for AI agents in large codebases | AICore feature | Practical impact |
|---|---|---|
| High diff churn from non-canonical formatting | Canonical IR + deterministic formatter (`aic fmt`) | Stable patches, no reformat-only noise |
| Ambiguous or unstable compiler errors | Structured diagnostics with stable codes/spans/fixes (`--json`, `--sarif`) | Reliable automated triage and fix loops |
| Hidden side effects across deep call graphs | Explicit effect declarations + transitive effect analysis | Safer code generation and refactoring |
| Weak guarantees on generated code behavior | Static typing, exhaustiveness checks, borrow discipline, and contracts | Earlier failure detection |
| Tool/version drift across agents and CI | CLI/protocol contract versioning (`aic contract --json`) | Predictable agent-tool integration |
| Slow iteration on large workspaces | Incremental daemon + deterministic workspace build planning | Faster repeated check/build loops |
| Dependency and API drift over time | Lockfile/checksum/offline workflow + std compatibility checks | Reproducible builds |

### Code Examples

#### Hidden side effects become explicit and machine-checkable

```aic
module examples.effects_reject;
import std.io;

fn io_fn() -> () effects { io } {
    print_int(1)
}

fn pure_fn() -> () {
    io_fn()   // ERROR: calling 'io_fn' requires undeclared effects: io
}
```

The checker emits stable diagnostics (`E2001`, `E2005`) with exact spans and call-path context.

#### Contracts constrain generated code behavior

```aic
module examples.non_empty_string;
import std.string;

struct NonEmptyString {
    value: String,
} invariant len(value) > 0

fn make_non_empty(s: String) -> NonEmptyString requires len(s) > 0 {
    NonEmptyString { value: s }
}
```

Preconditions/invariants give agents executable correctness boundaries during generation and refactoring.

#### No null: explicit absence handling

```aic
fn maybe_even(x: Int) -> Option[Int] {
    if x % 2 == 0 { Some(x) } else { None() }
}

fn main() -> Int effects { io } {
    let out = match maybe_even(42) {
        None => 0,
        Some(n) => n,
    };
    print_int(out);
    0
}
```

Exhaustiveness checks catch missing `Option` / `Result` branches at compile time.

---

## Status

| Area | MVP status |
|---|---|
| IR-first pipeline | ✅ Implemented |
| Deterministic parser/formatter | ✅ Implemented |
| Structured diagnostics JSON (`code`, spans, fixes) | ✅ Implemented |
| Type checker (Int/Bool/Float/String/Unit, functions, enums, structs) | ✅ Implemented |
| Effect checker (`io`, `fs`, `net`, `time`, `rand`, `env`, `proc`, `concurrency`) | ✅ Implemented |
| Contracts (`requires`, `ensures`, `invariant`) | ✅ Implemented |
| Match + exhaustiveness (Bool/Option/Result + enums) | ✅ Implemented |
| Pattern matching (`\|` alternatives + guard typing/coverage) | ✅ Implemented |
| Async/await (`async fn`, `await`, `Async[T]`) | ✅ Implemented |
| Traits + bounded generics (`trait`/`impl` + coherence checks) | ✅ Implemented |
| Result propagation operator (`expr?`) | ✅ Implemented |
| Mutability + borrow discipline (`let mut`, `&`/`&mut`) | ✅ Implemented |
| Generics (deterministic instantiation + codegen) | ✅ Implemented |
| LLVM backend (native via clang) | ✅ Implemented |
| Standard library (30+ modules) | ✅ Implemented |
| Package lock/checksum/offline cache | ✅ Implemented |
| LSP server (diagnostics/hover/definition/format) | ✅ Implemented |
| Incremental daemon | ✅ Implemented |
| Built-in test harness (`aic test`) | ✅ Implemented |
| SARIF diagnostics export | ✅ Implemented |
| API docs generation (`aic doc`) | ✅ Implemented |
| Debug info + panic source mapping | ✅ Implemented |
| SBOM + signed provenance flow | ✅ Implemented |
| Sandboxed run profiles | ✅ Implemented |
| Release reproducibility manifest | ✅ Implemented |

### REST + Async Support Matrix

| Capability | Status | Notes |
|---|---|---|
| `std.http_server` core APIs (`listen`, `accept`, `read_request`, `write_response`, `close`) | Supported | Runtime-backed and execution-tested on Linux/macOS (`exec_http_server_parses_request_and_emits_http11_response`). |
| Async HTTP server APIs in `std.http_server` | Unsupported | No `http_server.async_*` API surface; async primitives are exposed in `std.net`/`std.tls`. |
| HTTP request parsing coverage | Partial | Runtime parser accepts HTTP/1.0 + HTTP/1.1 and known methods (`GET`, `HEAD`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`), with bounded receive-loop body handling driven by `Content-Length`. |
| `std.router` route matching (exact, `:param`, trailing `*`) | Supported | Deterministic first-match order and typed errors are execution-tested (`exec_router_matches_paths_params_and_order`). |
| `std.net` async submit/wait/cancel/poll/wait-many/shutdown/pressure | Supported | Event-loop runtime is covered by execution tests and runnable examples (`examples/io/async_*`). |
| `await` submit bridge (`await Result[Async*Op, NetError]`) | Supported | Lowered to runtime async poll helpers and covered by tests/examples. |
| `std.tls` async submit/wait lifecycle | Partial | API surface is implemented and tested; runtime pressure reports queue metrics as `0` and backend-dependent TLS behavior is handled with typed fallback paths. |
| REST + async runtime on Windows | Unsupported | Network and async runtime paths use deterministic stub errors on Windows; REST/async execution coverage is gated to non-Windows targets. |

---

## Getting Started

### Local Machine Setup Guide

For full local installation and verification steps, see:

- `docs/local-machine-setup.md`

### Prerequisites

- **Rust** (stable) — compiler is written in Rust
- **clang** in `PATH` — used to compile emitted LLVM IR + runtime C shim
- **make** — for local CI orchestration
- **python3** — used by docs/schema checks

### Build

```bash
cargo build
```

### Toolchain Setup

Install the standard library into the global AIC toolchain location (default: `~/.aic/toolchains/<aic-version>/std`):

```bash
cargo run -- setup
```

Overrides:
- `AIC_STD_ROOT`: use an explicit std install directory.
- `AIC_HOME`: change the default base directory used for global toolchains.

### Hello World

Create a file `hello.aic`:

```aic
module hello;
import std.io;

fn main() -> Int effects { io } {
    print("Hello, AICore!");
    0
}
```

Run it:

```bash
cargo run -- run hello.aic
```

Or compile to a native binary:

```bash
cargo run -- build hello.aic -o hello
./hello
```

---

## CLI Reference

```bash
aic init <project>          # Initialize a new project (no local std copy)
aic setup [--std-root <p>]  # Install std into global toolchain location
aic check <file> [--json]   # Type/effect check with structured diagnostics
aic fmt <file>              # Deterministic formatting from IR
aic build <file> -o <out>   # Compile to native binary via LLVM
aic run <file>              # Build and execute
aic test <dir> [--json]     # Run built-in test harness
aic ir <file> --emit json   # Emit canonical IR
aic ir-migrate <ir.json>    # Migrate legacy IR to current schema
aic migrate <path>          # Source + IR migration planning
aic doc <file> -o <dir>     # Generate API documentation
aic lsp                     # Start LSP server
aic daemon                  # Start incremental check/build daemon
aic explain <code>          # Explain diagnostic code (e.g. E2001)
aic lock [path]             # Generate lockfile/checksums for project/workspace
aic pkg publish|search|install  # Package management
aic std-compat --check      # Std library compatibility/deprecation lint
aic verify-intrinsics       # Verify intrinsic bindings
aic diff --semantic <old> <new> # Semantic API/change diff in JSON
aic contract --json         # Emit CLI contract for tool negotiation
aic release manifest        # Reproducibility manifest
aic release sbom            # Generate SBOM
aic release policy --check  # Enforce release/LTS policy gates
aic run <file> --sandbox    # Sandboxed execution (none|ci|strict)
aic check <file> --sarif    # SARIF diagnostics export
```

---

## AI-Agent Documentation

For agent-first usage guidance (feature selection, command strategy, and workflow playbooks):

- [Agent tooling docs index](docs/agent-tooling/README.md)
- [Language feature playbook](docs/agent-tooling/language-feature-playbook.md)
- [CLI command playbook](docs/agent-tooling/aic-command-playbook.md)
- [aic init deep dive](docs/agent-tooling/commands/aic-init.md)
- [aic lsp deep dive](docs/agent-tooling/commands/aic-lsp.md)
- [aic diff --semantic deep dive](docs/agent-tooling/commands/aic-diff.md)

---

## Local CI with Make

Run the same checks as GitHub Actions locally:

```bash
make ci        # Full CI pipeline
make ci-fast   # Quick pre-commit loop
make check     # Full validation except fmt/lint
```

Additional targets:

| Target | Description |
|--------|-------------|
| `make examples-check` | Type-check all examples |
| `make examples-run` | Execute all runnable examples |
| `make cli-smoke` | CLI smoke tests |
| `make docs-check` | Documentation/schema validation |
| `make test-e8` | E8 verification gate tests |
| `make test-e8-nightly-fuzz` | Nightly fuzz stress suite |
| `make intrinsic-placeholder-guard` | Intrinsic stub policy gate |

Install git hooks:

```bash
make init
```

This installs:
- `.git/hooks/pre-commit` → runs `make ci-fast`
- `.git/hooks/pre-push` → runs `make ci`

---

## GitHub Actions

| Workflow | Trigger | Description |
|----------|---------|-------------|
| **CI** | Push/PR | Quality checks, unit/golden/execution/E7/E8 tests, cross-platform matrix (Linux/macOS/Windows) |
| **Nightly Fuzz** | Schedule | Lexer/parser/typechecker fuzz stress suite |
| **Release** | Tags `v*` | Builds release binaries, enforces release + LTS gates, publishes GitHub Release |
| **Security** | Push/PR/Schedule | Security audit, threat-model, workflow hardening checks |

---

## Project Layout

```
├── src/          # Compiler implementation (lexer, parser, type checker, codegen)
├── std/          # Standard library modules (30+ modules)
├── examples/     # Runnable and checker-focused examples
├── docs/         # Language spec, syntax reference, agent recipes, tutorials
│   ├── agent-recipes/        # Executable agent workflow recipes
│   ├── agent-tooling/        # Agent-grade tooling docs
│   ├── tutorial/             # Step-by-step tutorials
│   └── reference/            # Language reference docs
├── tests/        # Golden, unit, execution, CLI, and verification tests
├── benchmarks/   # Performance benchmarks
├── scripts/      # Build and CI helper scripts
└── tools/
    └── vscode-aic/   # VS Code extension (LSP integration)
```

---

## Test Suite

| Category | Count | Location |
|----------|-------|----------|
| Core unit tests | 94 | `src/*` library tests |
| Unit integration tests | 72 | `tests/unit_tests.rs` |
| Golden tests | 16 | `tests/golden_tests.rs` |
| Execution tests | 22 | `tests/execution_tests.rs` |
| CLI contract tests | 5 | `tests/e7_cli_tests.rs` |
| LSP smoke tests | 2 | `tests/lsp_smoke_tests.rs` |
| E8 verification tests | 11 total / 10 active | `tests/e8_*` |

---

## Determinism Guarantees

AICore provides deterministic behavior across every stage of the pipeline:

- **Stable tokenization/parsing** — same input always produces the same token stream and AST
- **Stable IR IDs** — `SymbolId`, `TypeId`, `NodeId` follow a deterministic traversal policy
- **Canonical formatting** — `aic fmt` output is idempotent and IR-driven
- **Deterministic diagnostics** — ordered by span/code/message for reproducible output
- **Reproducible builds** — lockfile, checksums, and release manifests ensure byte-identical outputs

Agent-oriented REST and workflow guide: `docs/ai-agent-rest-guide.md`.

---

## Diagnostics

AICore diagnostics are machine-readable by default, designed for agent consumption.

### JSON Format

```bash
aic check src/main.aic --json
```

```json
[
  {
    "code": "E2001",
    "severity": "error",
    "message": "calling 'io_fn' requires undeclared effects: io",
    "spans": [
      { "file": "src/main.aic", "start": 95, "end": 102, "label": null }
    ],
    "help": ["add `effects { io }` on the enclosing function"],
    "suggested_fixes": []
  }
]
```

### SARIF Export

```bash
aic check src/main.aic --sarif
```

### Diagnostic Explanation

```bash
aic explain E2001
```

Each diagnostic includes a stable code, severity, message, exact source spans, help text, and suggested fixes — enabling fully automated triage and fix loops.

---

## License

See [LICENSE](LICENSE) for details.
