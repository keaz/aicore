# Service and P2P Capability Boundary

This document records the current assessment of AICore's suitability for real-world software such as database-backed REST services and P2P communication systems.

The goal is to keep AICore focused on low-level language/runtime capabilities while leaving protocol stacks, frameworks, and service integrations to external libraries.

## Summary

Current repository evidence indicates that AICore already has enough low-level runtime surface on Linux/macOS to support:

- TCP and UDP transport
- DNS lookup and reverse lookup
- TLS transport upgrade and async TLS I/O
- async submit/wait/cancel/poll/wait-many lifecycle
- filesystem, process, environment, time, retry, and randomness APIs
- generic concurrency primitives and generic connection pools
- byte-oriented parsing/building and cryptographic helpers
- explicit `extern "C"` FFI for native interop

That means the language/runtime is close to the right abstraction boundary for:

- database client libraries
- HTTP and REST libraries/frameworks
- protocol client/server libraries
- custom TCP/UDP/TLS application protocols
- retry, pooling, and resilience middleware

The remaining language/runtime work should stay focused on foundational substrate, not on shipping high-level product-specific frameworks inside core AICore.

## What Must Stay In The Language / Runtime

These are low-level capabilities that should be available in the language/runtime because external libraries cannot cleanly replace them.

### Core execution model

- deterministic async/await lowering
- scalable async I/O runtime
- bounded concurrency primitives
- typed effect and capability boundaries for `net`, `fs`, `proc`, `env`, `time`, `rand`, and `concurrency`

### Core transport and binary substrate

- TCP sockets
- UDP sockets
- DNS lookup/reverse lookup
- TLS transport and certificate metadata access
- binary-safe `Bytes` and `ByteBuffer` style APIs
- exact/deadline/framed byte reads and timeout-bounded writes

### Service-operability primitives

- process execution
- environment and config loading primitives
- monotonic and wall-clock time
- retry/backoff primitives
- connection/resource pooling primitives
- runtime pressure and backpressure introspection

### Interop substrate

- explicit, typed, low-level FFI
- package/native link configuration

## What Should Stay In Libraries

These are important for real-world software, but they should not become built-in language/runtime responsibilities.

### REST and web stack

- routing frameworks with middleware chains
- request extraction and binding layers
- JSON schema validation, OpenAPI, and endpoint generation
- auth/session middleware
- tracing/logging middleware
- higher-level HTTP clients and servers with framework ergonomics
- HTTP/2, WebSocket, SSE, gRPC, and framework-specific protocols

### Database ecosystem

- PostgreSQL client driver
- MySQL client driver
- SQLite wrapper or pure-language adapter
- Redis and Kafka libraries
- ORM or query builders
- migrations and schema tooling
- transaction helpers and repository abstractions

### P2P and advanced networking

- STUN/TURN/ICE logic
- NAT traversal strategies
- application-level peer discovery
- WebRTC/QUIC/SCTP stacks
- gossip, replication, and overlay-network protocols
- message framing, encryption, and protocol codecs above raw transport

### Platform/product integrations

- cloud SDKs
- observability exporters
- service-framework conventions
- deployment-specific runtime adapters

## Current Findings

### Strong enough today for low-level library construction

Repository evidence shows the language/runtime already has the low-level pieces required for external libraries to be built on top:

- `std.net`, `std.tls`, `std.concurrent`, `std.pool`, `std.buffer`, `std.crypto`, `std.retry`, `std.proc`, and `std.fs`
- runtime-backed async TCP/TLS/fs submit-wait lifecycle
- byte-stream and framed-read APIs suitable for protocol implementations
- generic connection pooling suitable for DB/session/channel reuse
- explicit FFI path for native interop when pure-AICore libraries are not enough

### Important distinction: in-tree examples are not the same as shipping libraries

The repository already includes deterministic protocol references such as the PostgreSQL TLS/SCRAM replay example, but those examples are evidence that the substrate is present, not proof that a production database driver ecosystem exists.

That distinction should remain explicit:

- protocol examples and replay contracts can live in-repo as proofs of capability
- production protocol/client frameworks should live as external libraries

### HTTP capability is ahead of the top-level summary

The runtime and tests already cover more than just simple `Content-Length` request handling:

- chunked request decoding exists
- request-body streaming APIs exist
- fixed-length streaming response APIs exist

This is mainly a documentation/support-matrix alignment issue, not a request for more core-language surface.

### Windows support is still a substrate-quality concern

The current documentation is not fully aligned on Windows `std.net` support.

Some docs describe shared backend support and smoke coverage, while other docs still describe `std.net` as returning `NetError::Io` on Windows. That is a core runtime support-contract problem, not a library concern.

## Language / Runtime Gaps That Still Need Work

Only the items below should be tracked as core language/runtime work from this assessment.

### 1. Cross-platform runtime support contract for `std.net` service paths

Windows runtime behavior and documentation are still mixed enough that service authors cannot treat the support contract as settled.

This belongs in the language/runtime because it concerns:

- runtime implementation parity
- typed error guarantees
- support-matrix truthfulness
- CI-backed validation of actual low-level transport behavior

## Items Explicitly Not Requested As Core-Language Work

This assessment does not recommend core-language issues for:

- ORM support
- database query DSLs
- REST frameworks
- WebSocket/HTTP2/gRPC built-ins
- Kafka/Redis/Postgres drivers
- P2P overlay/discovery protocols
- NAT traversal stacks
- service middleware

Those should be built as libraries on top of the current and future low-level runtime substrate.

## Related Existing Open Issues

These already cover adjacent runtime work and should not be duplicated:

- `#385` `[ASYNC-READY-T5] Complete TLS async runtime parity and production diagnostics`
- `#391` `[ASYNC-READY-T7] Add Windows coverage for native async REST server paths`

## New Issues To Track From This Assessment

The remaining open issue created from this document should stay limited to:

- `#392` `[ASYNC-READY-T8] Scale async runtime beyond the single event-loop worker thread` has been implemented in the core runtime and should move out of the open gap list once the repo state is published.
- `#393` `[IO-READY-T1] Reconcile and validate the Windows std.net support contract for service libraries`
