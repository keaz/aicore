# Wave 2 Fixed-Width Scalar Taxonomy (`std.net` / `std.tls` / `std.concurrent`)

This artifact records phase-1 scalar migration choices for issues #311 and #315.

## Migration Policy

- Keep existing runtime/intrinsic signatures stable where lowering currently expects `Int`.
- Add fixed-width wrappers for clearly non-negative domains first (`UInt32` in this phase).
- Preserve timeout/deadline arguments as `Int`; migrate index-position and counter domains only via explicit additive wrappers.

## Scalar Taxonomy Table

| API / Field | Chosen Type | Rationale |
| --- | --- | --- |
| `std.net.ByteCountU32` | `UInt32` | Network byte counts are non-negative and bounded; wrapper-safe over existing `Int` runtime ABI. |
| `std.net.FrameLengthU32` | `UInt32` | 4-byte big-endian frame lengths are naturally unsigned (`0..=2^32-1`). |
| `std.net.tcp_stream_frame_len_be_u32` | `Result[UInt32, NetError]` | Removes signed ambiguity for frame parsing without changing existing `tcp_stream_frame_len_be` contract. |
| `std.net.tcp_recv_u32` / `udp_recv_from_u32` / `async_tcp_recv_submit_u32` | `max_bytes: UInt32` | Safe non-negative receive bound migration while preserving legacy `Int` overloads. |
| `std.net.tcp_set_*_buffer_size_u32` | `size_bytes: UInt32` | Buffer sizes are capacity-like non-negative domains. |
| `std.net.tcp_stream_recv_*_u32` (exact/framed) | `expected/max_frame_bytes: UInt32` | Framed/exact byte goals are non-negative counts. |
| `std.net.AsyncIntSelectionU32.index` / `AsyncStringSelectionU32.index` | `UInt32` | Selection index is a non-negative position. |
| `std.net.AsyncRuntimePressureU32` counters/limits | `UInt32` | Runtime pressure counters/limits are non-negative quantities. |
| `std.tls.TlsByteCountU32` | `UInt32` | TLS receive byte limits are non-negative. |
| `std.tls.TlsFrameLengthU32` | `UInt32` | TLS frame prefix decode is unsigned by definition. |
| `std.tls.tls_frame_len_be_u32` | `Result[UInt32, TlsError]` | Aligns frame-length semantics with unsigned payload sizes. |
| `std.tls.tls_recv_*_u32`, `tls_async_recv*_u32` | `max/expected bytes: UInt32` | Safe wrapper migration for non-negative byte domains. |
| `std.tls.byte_stream_recv*_u32` | `UInt32` byte/count/frame args | Preserves protocol-agnostic adapter while migrating counts to unsigned wrappers. |
| `std.tls.TlsAsync*SelectionU32.index` | `UInt32` | Async selection index is non-negative. |
| `std.tls.tls_async_runtime_pressure_u32` | `Result[AsyncRuntimePressureU32, TlsError]` | Reuses shared unsigned pressure view for TLS async pressure snapshots. |
| `std.concurrent.ConcurrencyCapacityU32` | `UInt32` | Channel capacity is a queue-size domain and cannot be negative. |
| `std.concurrent.ConcurrencyIndexU32` | `UInt32` | Selection/fan-in positions are non-negative domains. |
| `std.concurrent.ConcurrencyHandleU32` | `UInt32` | Runtime handle IDs are non-negative table keys exposed by fixed-width wrappers. |
| `std.concurrent.ConcurrencyPayloadIdU32` | `UInt32` | Concurrency payload-slot IDs are non-negative runtime identifiers. |
| `std.concurrent.ConcurrencyCountU32` | `UInt32` | Refcount/counter surfaces are non-negative quantities. |
| `std.concurrent.buffered_channel_u32` / `buffered_bytes_channel_u32` | `capacity: ConcurrencyCapacityU32` | Generic/channel wrappers expose typed unsigned capacity while keeping legacy APIs. |
| `std.concurrent.channel_int_u32` / `buffered_channel_int_u32` / `channel_int_buffered_u32` | `capacity: ConcurrencyCapacityU32` | Phase-1 migration of explicit channel-capacity APIs. |
| `std.concurrent.IntChannelSelectionU32.channel_index` | `ConcurrencyIndexU32` | Channel index in select results is non-negative. |
| `std.concurrent.IntTaskSelectionU32.task_index` | `ConcurrencyIndexU32` | Task selection index is non-negative. |
| `std.concurrent.select_any_u32` return index | `ConcurrencyIndexU32` | Fan-in receiver index is non-negative. |
| `std.concurrent.arc_strong_count_u32` | `Result[ConcurrencyCountU32, ConcurrencyError]` | Additive fixed-width Arc strong-count view over legacy `arc_strong_count` ABI surface with explicit conversion/error signaling. |

## Compatibility Notes

- Existing `Int` APIs remain available for runtime/backward compatibility.
- New `*_u32` wrappers and alias domains (`ConcurrencyCapacityU32`, `ConcurrencyIndexU32`, `ConcurrencyHandleU32`, `ConcurrencyPayloadIdU32`, `ConcurrencyCountU32`) are deterministic and additive.
- `arc_strong_count_u32` is additive; `arc_strong_count` (`Int`) remains available for legacy callers.
- Current lowering still expects some legacy `Int` payload structs (`AsyncRuntimePressure`, async-op handles), so phase-1 uses wrapper types instead of intrinsic signature replacement.
