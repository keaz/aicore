# Wave 5A Numeric API Adoption Matrix (`#330`)

This document is the human-readable Wave 5A numeric typing matrix for std APIs.

## Canonical Artifacts

- Human-readable matrix: `docs/numeric-api-adoption-wave5.md` (this file)
- Machine-readable matrix: `docs/numeric-api-adoption-wave5.json`
- Prior scalar taxonomy baseline: `docs/io-fixed-width-taxonomy-wave2.md`

## Policy Categories

### 1. Counts, Lengths, Capacity, and Index Domains

- Use `USize` for in-process count/length/capacity/index domains.
- Use explicit unsigned fixed-width integers (`UInt16`/`UInt32`/`UInt64`) when an external protocol/ABI sets width.
- Keep legacy `Int` APIs as compatibility surfaces until wrappers and call sites converge.

### 2. Protocol Fields, Frame Lengths, Ports, and Codes

- Use fixed-width unsigned domains for protocol-facing fields.
- Standard selections in Wave 5A:
  - frame lengths and protocol byte counts: `UInt32`
  - network ports and compact protocol codes: `UInt16`
  - one-byte wire codes and tags: `UInt8`
- Keep compatibility wrappers when runtime intrinsics still expose `Int`.

### 3. Float Math, Serde, and Formatting

- `Float` is a compatibility alias of `Float64`.
- Use `Float64` as the default float domain for math, formatting, and serde APIs unless a width-constrained ABI requires `Float32`.
- Mixed-width float operations are rejected by type policy; APIs should keep explicit, stable width boundaries.

## Wave 5A Adoption Matrix

| Row | API / Domain | Policy Category | Current Surface | Chosen Domain | Wave 5A Action | Follow-up |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | `std.buffer.new_buffer` / `new_growable_buffer` capacities | counts/lengths/capacity/index | `Int` capacity arguments | `USize` for capacity domain | Keep current `Int` APIs, add typed wrappers and conversion guards | `#331` |
| C2 | `std.buffer.buf_position` / `buf_remaining` / `buf_size` / `buf_seek` | counts/lengths/capacity/index | `Int` index/length surfaces | `USize` for cursor/size/index domain | Add additive `*_usize` getters/setters while preserving legacy `Int` compatibility | `#331` |
| C3 | `std.net.tcp_recv` / `tcp_stream_recv` byte-count bounds | counts/lengths/capacity/index | `Int` + additive `UInt32` wrappers | `USize` for runtime-local bounds, `UInt32` for protocol-width wrappers | Preserve existing `*_u32` APIs and add explicit `USize` migration targets | `#331` |
| P1 | `std.buffer` frame length prefix (`buf_read_length_prefixed`) | protocol fields/frame lengths/ports/codes | signed `i32` length decode path | `UInt32` frame length | Add explicit unsigned frame-length wrappers and keep signed compatibility entrypoint | `#332` |
| P2 | `std.net` port domain (endpoint parse/format and policy wrappers) | protocol fields/frame lengths/ports/codes | port embedded in `addr: String` surfaces | `UInt16` port domain | Add typed parse/format helpers that round-trip `UInt16` ports without changing existing connect/listen signatures | `#332` |
| P3 | `std.net.tcp_stream_frame_len_be_u32` / `std.tls.tls_frame_len_be_u32` | protocol fields/frame lengths/ports/codes | additive `UInt32` wrappers already present | `UInt32` frame length | Keep wrappers as canonical protocol-width surfaces | `#332` |
| P4 | `std.tls.tls_version_code` negotiated code domain | protocol fields/frame lengths/ports/codes | bounded integer code contracts | `UInt16`/`UInt8` bounded unsigned code policy | Keep bounded-code contract explicit in wrappers and docs | `#332` |
| F1 | `std.io.print_float` formatting surface | float math/serde/format | `Float` parameter | `Float64` (`Float` alias) | Keep compatibility alias while documenting canonical `Float64` domain | `#333` |
| F2 | `std.string.parse_float` / `float_to_string` conversion/format | float math/serde/format | compatibility `Float` surfaces | `Float64` default with explicit width policy | Document `Float64` default and require explicit width wrappers for `Float32` interop | `#333` |
| F3 | JSON float encode/decode boundaries | float math/serde/format | compatibility float surfaces | explicit `Float32`/`Float64` policy, default `Float64` | Keep default `Float64` serde path; add explicit `Float32` wrappers where protocol schemas require it | `#333` |

## Required Example Rows (Narrative)

### `std.buffer` Frame Length Domain

- Policy row: `P1`.
- Rationale: frame length prefixes are non-negative protocol fields and should be represented as `UInt32`.
- Compatibility: keep signed legacy entrypoints for existing callers, but expose unsigned wrappers as the canonical protocol-safe path.

### `std.net` Port Domain

- Policy row: `P2`.
- Rationale: network ports are bounded `0..=65535`, so `UInt16` is the canonical domain.
- Compatibility: existing `addr: String` connect/listen APIs remain stable; typed helpers should parse/format/validate ports in `UInt16`.

## Follow-up Linkage

### `#331` Counts/Lengths/Capacity/Index Rollout

- Rows: `C1`, `C2`, `C3`.
- Scope: additive `USize` wrappers, deterministic conversion boundaries, and compatibility retention for existing `Int` signatures.

### `#332` Protocol Fields/Frame Lengths/Ports/Codes Rollout

- Rows: `P1`, `P2`, `P3`, `P4`.
- Scope: unsigned fixed-width protocol domains with explicit wrapper-first migration paths.

### `#333` Float Math/Serde/Format Rollout

- Rows: `F1`, `F2`, `F3`.
- Scope: `Float64` default policy, `Float` alias compatibility, and explicit-width interop points for `Float32`.

#### Wave 5D Command-Style Policy (`#333`)

- Canonical targeted test shape for rollout validation: `cargo test --locked --test <target> ...`
- Canonical exact test shape when filtering: `cargo test --locked --test <target> -- --exact <case_name>`
- Canonical ignored test shape: `cargo test --locked --test <target> -- --ignored`
- Command-style guard references: use `#329` guard checks to detect ambiguous/non-canonical filtered invocations while keeping `#329` issue state unchanged.
- Anti-pattern (ambiguous filtered invocation): `cargo test --locked wave5_numeric`

Wave 5D examples:

- `examples/data/wave5_numeric_end_to_end.aic`
- `examples/data/wave5_migration_buffer_u32.aic`
