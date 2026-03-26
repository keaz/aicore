# AICore

AICore is an **agent-native, IR-first programming language** designed for **human + AI agent collaboration** on real software. It features deterministic formatting, structured diagnostics, a type + effect checker, design-by-contract support, and an LLVM native backend.

**Inspired by Rust**, AICore inherits many of the principles that make Rust a reliable systems language — strong static typing with no implicit coercions, an ownership and borrow discipline for memory safety, algebraic data types (`enum`/`struct`), exhaustive pattern matching, explicit error handling via `Result[T, E]` and `Option[T]` (no null), and trait-based generics. AICore extends these foundations with an explicit effect system, design-by-contract (`requires`/`ensures`/`invariant`), an IR-first architecture for deterministic tooling, and structured machine-readable diagnostics — all purpose-built for AI agent workflows at scale.

The canonical source of truth is **IR** (`aic ir --emit json`), while text syntax is a deterministic view (`aic fmt`).

Project note: AICore was written mainly using **ChatGPT Codex**, with human review and validation.

---

## Start Here

- New to the project: start with [Local machine setup](docs/local-machine-setup.md), then open the [docs index](docs/index.md).
- Writing your first program: jump to [Stdlib + IO quick start](#stdlib--io-quick-start) and [Hello World](#hello-world).
- Exploring the language: use [Language Overview](#language-overview) and [Standard Library](#standard-library).
- Looking for runnable material: open [Examples](examples/README.md) and [Tests](tests/README.md).
- Working on agent/tooling flows: use [Agent tooling docs](docs/agent-tooling/README.md).

## Stdlib + IO Quick Start

- [Stdlib API index](docs/std-api/index.md)
- [IO API reference](docs/io-api-reference.md)
- [IO runtime guide](docs/io-runtime/README.md)
- [IO cookbook](docs/io-cookbook.md)
- [Data/Text stack guide](docs/data-text/README.md)
- [Config loading guide](docs/config-loading.md)
- [Examples index](examples/README.md)
- [Tests index](tests/README.md)

---

## Table of Contents

- [Start Here](#start-here)
- [Stdlib + IO Quick Start](#stdlib--io-quick-start)
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
- [Common Commands](#common-commands)
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

fn greet(name: String) -> () effects { io } capabilities { io } {
    println_str(f"Hello, {name}!")
}
```

AICore also supports `async fn`, `intrinsic fn` (runtime-bound FFI stubs), `unsafe fn`, and `extern fn` declarations:

```aic
async fn fetch_data(url: String) -> String effects { net } capabilities { net } {
    // async body
    "data"
}

intrinsic fn aic_fs_read_to_string_intrinsic(path: String) -> String effects { fs };

extern "C" fn c_sqrt(x: Float) -> Float;
```

### Types and Data Structures

AICore has a **strong, static type system** with **no implicit coercions** and **no null**.

#### Primitive Types

| Family | Types | Notes |
|--------|-------|-------|
| Signed integers | `Int`, `Int8`, `Int16`, `Int32`, `Int64`, `Int128` | `Int` is the general integer type and currently uses signed 64-bit range. |
| Unsigned integers | `UInt8`, `UInt16`, `UInt32`, `UInt64`, `UInt128` | Fixed-width unsigned numeric primitives. |
| Size-family integers | `ISize`, `USize`, `UInt` | `ISize`/`USize` use deterministic 64-bit semantics; `UInt` is a compatibility alias for `USize`. |
| Floating point | `Float32`, `Float64`, `Float` | `Float` is a compatibility alias for `Float64`. |
| Other scalar/text primitives | `Bool`, `Char`, `String`, `Bytes`, `()` | `Char` is a Unicode scalar value; `String` is UTF-8 text; `Bytes` is the binary payload type used by filesystem and networking APIs. |

Literal forms follow the same surface:

- Integer suffixes: `i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`
- Float suffixes: `f32`, `f64`
- Unsuffixed integer literals default to `Int`
- Unsuffixed float literals default to `Float` (`Float64`) unless context narrows them to `Float32`

Additional first-class type forms supported by the checker:

- Tuple types and literals such as `(Int, String)` and `(1, "x")`, with field access via `.0`, `.1`, ...
- Reference wrappers `Ref[T]` and `RefMut[T]`, produced by `&x` and `&mut x`
- Async and callable wrappers such as `Async[T]` and `Fn(Int, String) -> Bool`
- Runtime-dispatch trait objects via `dyn Trait`

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
fn greet() -> () effects { io } capabilities { io } {
    println_str("hello")
}

// Multiple effects
fn fetch_and_log(url: String) -> String effects { net, io } capabilities { net, io } {
    let data = http_get(url);
    println_str(data);
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
async fn fetch_user(id: Int) -> String effects { net } capabilities { net } {
    http_get(f"/users/{int_to_string(id)}")
}

async fn main() -> () effects { net, io } capabilities { net, io } {
    let user = await fetch_user(42);
    println_str(user);
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

fn load_config(path: String) -> Result[Config, FsError] effects { fs } capabilities { fs } {
    match read_text(path) {
        Ok(text) => match parse_port(text) {
            Ok(port) => Ok(Config { port: port }),
            Err(_) => Err(InvalidInput()),
        },
        Err(err) => Err(err),
    }
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

AICore's standard library is organized into focused modules for IO/filesystem, networking and protocols, collections and text, runtime support, and process/environment access. The public surface is compatibility-checked against `docs/std-api-baseline.json` and summarized in [docs/std-api/index.md](docs/std-api/index.md).

- IO and filesystem: `std.io`, `std.fs`, `std.path`, `std.bytes`, `std.buffer`
- Networking and protocols: `std.net`, `std.http`, `std.http_server`, `std.tls`, `std.router`
- Collections and text: `std.string`, `std.regex`, `std.json`, `std.vec`, `std.map`, `std.set`, `std.deque`
- Runtime and utility: `std.concurrent`, `std.time`, `std.rand`, `std.env`, `std.proc`, `std.log`, `std.config`, `std.signal`, `std.retry`, `std.option`, `std.result`, `std.math`, `std.numeric`, `std.iterator`, `std.pool`, `std.char`, `std.url`, `std.error_context`
- Security and integrity: `std.crypto`, `std.secure_errors`

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

fn io_fn() -> () effects { io } capabilities { io } {
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

fn main() -> Int effects { io } capabilities { io } {
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
| Type checker (rich scalar types, tuples, functions, refs, `dyn Trait`, enums, structs) | ✅ Implemented |
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
| Standard library (focused module set) | ✅ Implemented |
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

fn main() -> Int effects { io } capabilities { io } {
    println_str("Hello, AICore!");
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

## Common Commands

For the full surface, use `aic --help` and `aic <command> --help`. The commands below are the common entry points, not an exhaustive command index.

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
aic pkg <publish|search|install> ...  # Package management
aic std-compat --check      # Std library compatibility/deprecation lint
aic verify-intrinsics       # Verify intrinsic bindings
aic diff --semantic <old> <new> # Semantic API/change diff in JSON
aic contract --json         # Emit CLI contract for tool negotiation
aic release manifest        # Reproducibility manifest
aic release sbom            # Generate SBOM
aic release policy --check  # Enforce release/LTS policy gates
aic run <file> --sandbox <none|ci|strict>  # Sandboxed execution
aic check <file> --sarif    # SARIF diagnostics export
```

---

## AI-Agent Documentation

For agent-first usage guidance (feature selection, command strategy, and workflow playbooks):

- [Docs index](docs/index.md)
- [Agent tooling docs index](docs/agent-tooling/README.md)
- [Examples docs index](docs/examples/README.md)
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
├── std/          # Standard library modules
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

Use the test families below to pick the smallest stable check that covers your change:

- Core language and stdlib behavior: `tests/unit_tests.rs`
- Parser, formatter, and snapshot stability: `tests/golden_tests.rs`
- Runtime and backend execution: `tests/execution_tests.rs`
- CLI contract and workflow checks: `tests/e7_cli_tests.rs`, `tests/suggest_contracts_cli_tests.rs`
- LSP/editor smoke coverage: `tests/lsp_smoke_tests.rs`
- Verification, fuzz, and perf gates: `tests/e8_*`, `tests/fuzz/`

Common commands:

- `make ci` for the full local gate
- `make test-e7` for CLI and docs-as-tests coverage
- `make test-e8` for verification gates
- `cargo test --locked --test execution_tests` for runtime changes
- `cargo test --locked --test unit_tests` for language or stdlib changes

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
