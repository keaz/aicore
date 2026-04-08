# REST Microservice Readiness — GitHub Issues

> These issues track the missing language capabilities required to build REST microservices in AICore.
> **Priority order**: REST-T1 → REST-T2 → REST-T3 → REST-T4 → REST-T5 → REST-T6 → REST-T7 → REST-T8

---

## [REST-T1] Add `while` and `loop` control flow constructs

**Severity**: 🔴 Critical  
**Estimate**: XL  
**Dependencies**: None (core language primitive)

### Background

AICore has no looping construct. The only iteration mechanism is recursion + `if`/`match`. This is a **critical blocker** for any server application — you cannot write a TCP accept loop, iterate over collections, or implement retry/polling patterns.

### Proposed Syntax

```aic
// while loop
fn countdown(n: Int) -> () effects { io } {
    let mut i = n;
    while i > 0 {
        print_int(i);
        i = i - 1;
    }
}

// infinite loop with break
fn server_loop(listener: Int) -> () effects { net, io } {
    loop {
        let conn = tcp_accept(listener, 5000);
        match conn {
            Ok(handle) => handle_request(handle),
            Err(_) => break,
        }
    }
}

// while with break value
fn find_first(items: Vec[Int], target: Int) -> Option[Int] {
    let mut i = 0;
    while i < len(items) {
        if get(items, i) == target {
            break Some(i)
        }
        i = i + 1;
    }
}
```

### Definition of Done

- [ ] Lexer: Add `KwWhile`, `KwLoop`, `KwBreak`, `KwContinue` tokens
- [ ] Parser: Parse `while <expr> { <block> }` and `loop { <block> }` as expressions
- [ ] Parser: Parse `break`, `break <expr>`, and `continue` inside loops
- [ ] AST/IR: Add `While`, `Loop`, `Break`, `Continue` IR nodes
- [ ] IR Builder: Lower while/loop AST to IR
- [ ] Formatter: Canonical printing of while/loop expressions
- [ ] Type Checker: Validate while condition is `Bool`, break expression matches loop type
- [ ] Type Checker: Reject break/continue outside of loop context (new diagnostic code)
- [ ] Effect Checker: Propagate effects through loop bodies
- [ ] Contract Checker: Handle loops in contract lowering
- [ ] LLVM Codegen: Lower while/loop to LLVM branch/phi instructions
- [ ] Golden tests for parse/format roundtrip
- [ ] Compile-fail tests for break-outside-loop, non-Bool condition
- [ ] Execution tests for countdown, server loop, break-with-value patterns

### Acceptance Criteria

- `while cond { body }` compiles and executes correctly
- `loop { ... break ... }` compiles and executes correctly
- `break` and `continue` only valid inside loops (deterministic diagnostic code)
- Loops interact correctly with effects, contracts, and borrow checker
- Deterministic formatting via `aic fmt`
- All existing tests continue to pass

---

## [REST-T2] Complete string manipulation standard library

**Severity**: 🔴 Critical  
**Estimate**: L  
**Dependencies**: REST-T1 (loops needed for internal implementations)

### Background

`std.string` currently has only `len` and `concat`. Building a REST microservice requires splitting HTTP request lines (`GET /api/users HTTP/1.1`), extracting paths, parsing headers (`Content-Type: application/json`), trimming whitespace, and comparing substrings.

### Proposed API

```aic
module std.string;

// Searching
fn contains(haystack: String, needle: String) -> Bool
fn starts_with(s: String, prefix: String) -> Bool
fn ends_with(s: String, suffix: String) -> Bool
fn index_of(s: String, needle: String) -> Option[Int]
fn last_index_of(s: String, needle: String) -> Option[Int]

// Extraction
fn substring(s: String, start: Int, end: Int) -> String
fn char_at(s: String, index: Int) -> Option[String]
fn split(s: String, delimiter: String) -> Vec[String]
fn split_first(s: String, delimiter: String) -> Option[Vec[String]]

// Transformation
fn trim(s: String) -> String
fn trim_start(s: String) -> String
fn trim_end(s: String) -> String
fn to_upper(s: String) -> String
fn to_lower(s: String) -> String
fn replace(s: String, from: String, to: String) -> String
fn repeat(s: String, count: Int) -> String

// Conversion
fn parse_int(s: String) -> Result[Int, String]
fn int_to_string(n: Int) -> String
fn bool_to_string(b: Bool) -> String

// Joining
fn join(parts: Vec[String], separator: String) -> String
```

### Definition of Done

- [ ] Define all function signatures in `std/string.aic` with effect annotations (all pure)
- [ ] Implement C runtime intrinsics for each function in `codegen.rs`
- [ ] Add LLVM declarations for all `aic_rt_string_*` symbols
- [ ] Implement codegen lowering for all string intrinsic calls
- [ ] Unit tests: empty strings, Unicode edge cases, out-of-bounds indices
- [ ] Execution tests: HTTP-relevant usage (split request line, parse headers)
- [ ] Update `docs/std-api-baseline.json`
- [ ] Add example: `examples/data/string_ops.aic`

### Acceptance Criteria

- All listed string functions compile and produce correct results
- `split("GET /api/users HTTP/1.1", " ")` returns `["GET", "/api/users", "HTTP/1.1"]`
- `trim`, `to_upper`, `to_lower` handle ASCII correctly
- `parse_int` returns `Result[Int, String]` with `Err` for non-numeric input
- `index_of` returns `Option[Int]` — `None` when not found
- All functions are pure (no effect annotation required)
- Memory safety: no buffer overflows in C runtime

---

## [REST-T3] Add `Map[K, V]` dictionary data structure

**Severity**: 🟠 Major  
**Estimate**: XL  
**Dependencies**: REST-T2 (string operations needed for key handling)

### Background

AICore has only `Vec[T]` as a collection type. Without `Map[K, V]`, there is no efficient way to store HTTP headers by name, parse query parameters, build route tables, or maintain any index/cache.

### Proposed API

```aic
module std.map;

import std.option;
import std.vec;

struct Map[K, V] {
    handle: Int,
}

struct MapEntry[K, V] {
    key: K,
    value: V,
}

fn new_map[K, V]() -> Map[K, V]
fn insert[K, V](m: Map[K, V], key: K, value: V) -> Map[K, V]
fn get[K, V](m: Map[K, V], key: K) -> Option[V]
fn contains_key[K, V](m: Map[K, V], key: K) -> Bool
fn remove[K, V](m: Map[K, V], key: K) -> Map[K, V]
fn size[K, V](m: Map[K, V]) -> Int
fn keys[K, V](m: Map[K, V]) -> Vec[K]
fn values[K, V](m: Map[K, V]) -> Vec[V]
fn entries[K, V](m: Map[K, V]) -> Vec[MapEntry[K, V]]
```

### Definition of Done

- [ ] Define `Map[K, V]` and `MapEntry[K, V]` types in `std/map.aic`
- [ ] Implement C runtime hash map for string keys (MVP: `Map[String, V]`)
- [ ] Add LLVM declarations for `aic_rt_map_*` runtime symbols
- [ ] Implement codegen lowering for map operations with generic monomorphization
- [ ] Handle memory management: maps own their entries, cleanup on scope exit
- [ ] Type checker support for Map generic instantiation
- [ ] Execution tests: insert/get/remove/contains_key/keys/values
- [ ] Example: `examples/data/map_headers.aic`
- [ ] Update `docs/std-api-baseline.json`

### Acceptance Criteria

- `Map[String, String]` works for HTTP header storage
- `Map[String, Int]` works for counters/config
- `get` returns `Option[V]` — `None` when key not found (no null)
- `insert` with existing key updates the value
- `keys` and `values` return deterministic sorted order
- All map operations are pure (no effect needed)
- Generic monomorphization produces correct code for multiple `K, V` combinations

---

## [REST-T4] HTTP server library with request parsing and response building

**Severity**: 🔴 Critical  
**Estimate**: XL  
**Dependencies**: REST-T1, REST-T2, REST-T3

### Background

AICore has TCP sockets (`std.net`) and HTTP types (`std.http`), but no code to parse raw TCP bytes into `HttpRequest`, serialize `HttpResponse` to wire format, or run a server accept loop.

### Proposed API

```aic
module std.http_server;

import std.http;
import std.net;
import std.map;

struct HttpServer {
    listener: Int,
    addr: String,
}

enum ServerError {
    BindFailed(NetError),
    AcceptFailed(NetError),
    ParseFailed(String),
    SendFailed(NetError),
    Closed,
}

// Lifecycle
fn listen(addr: String) -> Result[HttpServer, ServerError] effects { net }
fn accept_request(server: HttpServer, timeout_ms: Int) -> Result[HttpRequest, ServerError] effects { net }
fn send_response(conn: Int, response: HttpResponse) -> Result[Bool, ServerError] effects { net }
fn close_server(server: HttpServer) -> Result[Bool, ServerError] effects { net }

// Convenience builders
fn text_response(status: Int, body: String) -> HttpResponse
fn json_response(status: Int, body: String) -> HttpResponse
fn error_response(status: Int, message: String) -> HttpResponse

// Request helpers
fn request_path(req: HttpRequest) -> String
fn request_method(req: HttpRequest) -> HttpMethod
fn request_header(req: HttpRequest, name: String) -> Option[String]
fn request_body(req: HttpRequest) -> String
fn query_params(req: HttpRequest) -> Map[String, String]
```

### Definition of Done

- [ ] Create `std/http_server.aic` with all function signatures
- [ ] Implement C runtime for HTTP/1.1 request parsing from TCP stream
  - Parse request line: `METHOD PATH HTTP/1.1\r\n`
  - Parse headers: `Name: Value\r\n` until `\r\n\r\n`
  - Read body based on `Content-Length`
- [ ] Implement C runtime for HTTP/1.1 response serialization
- [ ] Add LLVM declarations for all `aic_rt_http_server_*` symbols
- [ ] Add codegen lowering for server intrinsic calls
- [ ] Execution test: echo server that accepts one request and responds
- [ ] Example: `examples/io/http_server_hello.aic`
- [ ] Effect declarations: all server ops require `effects { net }`

### Acceptance Criteria

- A minimal HTTP server can listen on a port and accept requests
- Incoming HTTP/1.1 requests are parsed into `HttpRequest` with method, path, headers, body
- `HttpResponse` is serialized correctly to HTTP/1.1 wire format
- `text_response(200, "Hello")` produces valid HTTP response
- `json_response(200, body)` sets `Content-Type: application/json`
- Server errors are typed as `ServerError` enum — no panics on malformed input
- Query parameters extracted into `Map[String, String]`

---

## [REST-T5] Route handler dispatch / router library

**Severity**: 🔴 Critical  
**Estimate**: L  
**Dependencies**: REST-T3, REST-T4, REST-T7 (closures for handler functions)

### Background

Without a routing mechanism, building a multi-endpoint REST API requires manual string matching on `request.target` inside a giant `match` or `if-else` chain. A proper router maps paths + methods to handler functions.

### Proposed API

```aic
module std.router;

import std.http;
import std.http_server;

struct Router {
    handle: Int,
}

struct RouteMatch {
    handler_id: Int,
    path_params: Map[String, String],
}

fn new_router() -> Router
fn add_route(r: Router, method: HttpMethod, path: String, handler_id: Int) -> Router
fn match_route(r: Router, method: HttpMethod, path: String) -> Option[RouteMatch]

// Path parameter extraction: "/users/:id" matches "/users/42" with id="42"
// Wildcard paths: "/static/*" matches "/static/css/main.css"
```

### Definition of Done

- [ ] Create `std/router.aic` with route registration and matching API
- [ ] Implement C runtime for path-pattern matching with `:param` and `*` wildcard support
- [ ] Support exact match, parameterized paths, and wildcard routes
- [ ] Add LLVM declarations and codegen lowering
- [ ] Method-based dispatch: `GET /users` vs `POST /users` are different routes
- [ ] Execution tests: register routes, match requests, extract path params
- [ ] Example: `examples/io/http_router.aic`

### Acceptance Criteria

- Routes can be registered with method + path pattern
- `match_route(r, Get, "/users/42")` matches route `/users/:id` and returns `path_params = { "id": "42" }`
- Method mismatch returns `None`
- Unmatched paths return `None`
- Routes with overlaps resolved in registration order (first match wins)
- Path params extracted into `Map[String, String]`

---

## [REST-T6] Add `Float` primitive type

**Severity**: 🟡 Moderate  
**Estimate**: L  
**Dependencies**: None

### Background

AICore has only `Int` and `Bool` as primitive types. REST APIs commonly deal with decimal numbers (prices, coordinates, measurements). JSON numbers with decimals can't be represented, breaking JSON round-trip for most real-world payloads.

### Proposed Changes

```aic
// New primitive type: Float (64-bit IEEE 754 double)
let price: Float = 19.99;
let ratio: Float = 3.14;

// Arithmetic
let total: Float = price * 1.08;  // tax calculation

// Conversion
fn int_to_float(n: Int) -> Float
fn float_to_int(f: Float) -> Int      // truncates
fn parse_float(s: String) -> Result[Float, String]
fn float_to_string(f: Float) -> String

// JSON integration
fn encode_float(value: Float) -> JsonValue    // in std.json
fn decode_float(value: JsonValue) -> Result[Float, JsonError]
```

### Definition of Done

- [ ] Lexer: Parse float literals (`3.14`, `0.5`, `1e10`, `2.5e-3`)
- [ ] Parser: Float literal expression node
- [ ] AST/IR: `Float` type in type system
- [ ] Type Checker: Float arithmetic, comparisons, no implicit Int↔Float coercion
- [ ] LLVM Codegen: Float as `double`, arithmetic as `fadd`/`fsub`/`fmul`/`fdiv`
- [ ] Runtime: Float formatting in `aic_rt_print_float`
- [ ] std.string: `parse_float`, `float_to_string`
- [ ] std.json: `encode_float`, `decode_float`
- [ ] Execution tests: arithmetic, comparisons, JSON round-trip
- [ ] Update spec.md and std-api-baseline

### Acceptance Criteria

- `let x: Float = 3.14;` compiles and works
- Float arithmetic produces IEEE 754 compliant results
- No implicit coercion between Int and Float (explicit conversion required)
- JSON encode/decode preserves Float precision
- Comparison operators work: `<`, `>`, `<=`, `>=`, `==`, `!=`
- NaN and infinity handling is deterministic

---

## [REST-T7] Add closures / first-class functions

**Severity**: 🟡 Moderate  
**Estimate**: XL  
**Dependencies**: None

### Background

AICore cannot pass functions as values. This prevents key patterns for REST services: registering handler functions with a router, implementing middleware chains, using callbacks for async operations, and functional transformations on collections (map/filter/reduce).

### Proposed Syntax

```aic
// Closure syntax
let add_one = |x: Int| -> Int { x + 1 };
let greet = |name: String| -> String { concat("Hello, ", name) };

// Function type annotation
fn apply(f: Fn(Int) -> Int, x: Int) -> Int {
    f(x)
}

// Closures capture environment
fn make_adder(n: Int) -> Fn(Int) -> Int {
    |x: Int| -> Int { x + n }
}

// Usage with router (future)
fn setup_routes(router: Router) -> Router {
    add_route(router, Get, "/health", |req: HttpRequest| -> HttpResponse {
        text_response(200, "ok")
    })
}

// Collection operations
fn map_vec[T, U](items: Vec[T], f: Fn(T) -> U) -> Vec[U]
fn filter_vec[T](items: Vec[T], pred: Fn(T) -> Bool) -> Vec[T]
fn fold_vec[T, U](items: Vec[T], init: U, f: Fn(U, T) -> U) -> U
```

### Definition of Done

- [ ] Lexer: Parse `|params| -> ReturnType { body }` closure syntax
- [ ] Parser: Closure expression node with parameter list, return type, body
- [ ] AST/IR: `Closure` IR node, `FnType` type constructor
- [ ] Type Checker: Closure type inference, capture analysis, Fn type matching
- [ ] Effect Checker: Closure effects are union of body effects + captured variable effects
- [ ] Borrow Checker: Captured variable lifetime/mutability rules
- [ ] LLVM Codegen: Closure as `{fn_ptr, env_ptr}` fat pointer with environment struct
- [ ] std.vec: Add `map_vec`, `filter_vec`, `fold_vec` using Fn types
- [ ] Execution tests: basic closures, captures, higher-order functions
- [ ] Formatter: Canonical closure formatting

### Acceptance Criteria

- `|x: Int| -> Int { x + 1 }` creates a closure value
- Closures capture immutable variables from enclosing scope
- `Fn(T) -> U` type can be used in function parameters and return types
- Closures compose: `apply(|x| -> Int { x * 2 }, 21)` returns `42`
- Effect system tracks closure effects correctly
- Closures interact correctly with generics

---

## [REST-T8] Historical async runtime design record

Historical note as of 2026-04-08:

- The reactor-backed async runtime described by this epic now exists in the main repo; see `docs/async-event-loop.md` for current behavior and evidence anchors.
- `std.net` async submit/wait/cancel/poll/wait-many/shutdown and the `await` submit bridge are implemented.
- `std.http_server` async accept/read/write/serve wrappers are also present and exercised by the current example/CI matrix.
- The backlog text below is retained as design history and should not be read as the current support contract.

**Severity**: 🟡 Moderate  
**Estimate**: XL  
**Dependencies**: REST-T1, REST-T4, REST-T7

### Original Background

AICore has `async fn` / `await` syntax that type-checks, and `std.concurrent` provides thread-based task spawning. However, there is no non-blocking I/O event loop. For a production REST microservice, blocking one OS thread per connection doesn't scale — you need an event loop that multiplexes many connections on few threads.

### Original Proposed Design

```aic
// async handler
async fn handle_user(req: HttpRequest) -> HttpResponse effects { net, io } {
    let user_id = request_path_param(req, "id");
    let data = await fetch_user(user_id);
    json_response(200, encode(data))
}

// Event loop runtime
fn run_server(addr: String, handler: Fn(HttpRequest) -> Async[HttpResponse]) -> Result[(), ServerError] 
  effects { net, io, concurrency }
{
    let server = listen(addr)?;
    loop {
        let req = await async_accept(server);
        spawn_handler(handler, req);  // non-blocking dispatch
    }
}
```

### Original Definition of Done

- [ ] Design event loop architecture: epoll (Linux) / kqueue (macOS) based
- [ ] Implement non-blocking TCP accept/read/write in C runtime
- [ ] Implement task scheduler with work-stealing or single-threaded event loop
- [ ] Lower `async fn` to state machines (stackless coroutines) in codegen
- [ ] Lower `await` to yield points in the state machine
- [ ] Implement `Async[T]` as a future that integrates with the event loop
- [ ] Add `async_accept`, `async_tcp_recv`, `async_tcp_send` to std.net
- [ ] Execution test: async echo server handling multiple concurrent connections
- [ ] Performance test: compare with thread-per-connection baseline

### Original Acceptance Criteria

- `async fn` compiles to a state machine, not a thread spawn
- `await` suspends the current task without blocking an OS thread
- Event loop can handle 100+ concurrent connections on a single thread
- Async functions compose with effects and contracts
- Backpressure: bounded task queue prevents unbounded memory growth
- Graceful shutdown: pending tasks complete before server exits

---

## Dependency Graph

```
REST-T1 (loops) ──────────────┐
                                ├── REST-T4 (HTTP server) ── REST-T5 (router)
REST-T2 (strings) ────────────┤                                     │
                                │                                     │
REST-T3 (Map) ─────────────────┘                                     │
                                                                      │
REST-T6 (Float) ── standalone                                        │
                                                                      │
REST-T7 (closures) ───────────────────────────────────────────────────┘
                                        │
REST-T8 (async event loop) ────────────┘
```

## Implementation Order

| Phase | Issues | Outcome |
|-------|--------|---------|
| **Phase 1** — Core Primitives | REST-T1, REST-T2, REST-T6 | Loops, strings, floats |
| **Phase 2** — Collections | REST-T3 | Map data structure |
| **Phase 3** — HTTP Stack | REST-T4, REST-T5 | Server + router |
| **Phase 4** — Advanced | REST-T7, REST-T8 | Closures, async runtime |

After Phase 3, AICore can build a basic **synchronous REST microservice**.  
After Phase 4, AICore can build a **production-grade async REST microservice**.
