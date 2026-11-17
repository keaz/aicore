# Channel Slot Reclamation

Status: resolved by #500 (CORR-T12). This page records the design and the
evidence. User-facing rules are in
[`docs/io-concurrency-runtime.md#channel-slot-lifetime`](../io-concurrency-runtime.md#channel-slot-lifetime).

## Problem (before the fix)

The C runtime allocates concurrency channels from a fixed table,
`aic_rt_conc_channels[AIC_RT_CONC_CHANNEL_CAP]` in
`src/codegen/runtime/part03.c`, with 128 slots (`AIC_RT_LIMIT_CONC_CHANNELS` can
only lower that number). No code set a slot back to inactive: closing a channel
only set `closed = 1`. A program that created a `buffered_channel(1)` and closed
it in a loop got a zero handle on call 129, and `std.pool` stopped working after
about 125 `acquire(...)`/`pool_stats(...)` calls in one process.

## Design

### Slot release

A slot is released when nothing can use it again:

- the channel is closed,
- its buffer is empty, and
- no `send`/`recv` is blocked on it (`waiters == 0`; each blocking wait increments
  the counter around `pthread_cond_timedwait`).

Every channel operation calls `aic_rt_conc_channel_release_if_idle_locked` before
it unlocks the slot, so the release happens in whichever operation makes the
condition true: the close itself, the receive that takes the last buffered value,
or the blocked operation that wakes up after the close. Release frees the value
buffer and advances the slot's generation.

The slot mutex and condition variables are initialised once per slot (through
`pthread_once`) and never destroyed. A thread that read a stale handle can always
lock the slot safely and then sees that the generation no longer matches.
Allocation takes `aic_rt_conc_channel_table_mutex`, then each candidate slot's
mutex (lock order: table, then slot). Free slots are picked round-robin, starting
after the last allocated slot.

### Generation-tagged handles

`handle = generation * 128 + slot_index + 1`. The first channel in each slot has
generation 0, so the first 128 handles are `1..128` as before.

`aic_rt_conc_channel_lock` classifies a handle:

| Case | Result |
| --- | --- |
| generation matches and slot active | live; slot locked |
| generation older than the slot (or the slot's generation has wrapped) | released: behaves like a closed, empty channel |
| `handle <= 0`, out of range, generation not issued yet, or slot inactive at that generation | not found (unchanged codes) |

On a released handle `send_int`, `try_send_int`, `recv_int` and `try_recv_int`
return `Closed` (6) and `close_channel` returns success, so `close_sender` and
`close_receiver` on the same channel stay safe in either order.

Generations wrap at `2^23` (`AIC_RT_CONC_CHANNEL_GEN_LIMIT`), so every handle is
at most `2^30`. That keeps handles inside the `*_u32` helpers and inside the
`std.pool` handle encoding (`id_base()` is now `2^31` and `timeout_field_base()`
`2^39`). A stale handle could only alias a new channel if the same slot were
reused 8,388,608 times while the stale copy was still in use.

### Buffered payloads

Generic channels carry payload ids into `aic_rt_conc_payloads`. The runtime
cannot tell payload ids from plain `Int` values, so it never drops values itself:
it only releases a slot whose buffer is empty. `close_receiver` (std) closes the
channel and then receives and drops every buffered payload, which empties the
buffer and releases the slot. `close_sender` and `close_channel` keep buffered
values receivable; the slot is released when the last one is taken. A channel
closed that way and never drained keeps its slot.

### Exhaustion

When all slots are open, channel creation returns `Io` (7). `channel_int(...)`
and the new `try_buffered_channel(...)` surface it as `Err(Io)`.
`channel()`/`buffered_channel(...)` keep their tuple signature and return handle
`0`; `std.pool` `acquire(...)` now checks for that handle and returns `Closed`
without sending a request the manager could not answer.

### Windows

The change uses only the pthread shim already used by the runtime
(`pthread_once`, `pthread_mutex_*`, `pthread_cond_*`), so the Windows contract in
`docs/io-concurrency-runtime.md` is unchanged.

## Evidence

Execution tests in `tests/execution_tests.rs`:

- `exec_channel_slots_are_reclaimed_after_close`: 1,000 empty channels closed by
  `close_receiver`, 5,000 channels closed with a buffered `String` payload (more
  than the 4,096-slot payload table), 300 channels drained to `Closed` after
  `close_sender`, and 300 `IntChannel`s closed with `close_channel`.
- `exec_channel_stale_handle_after_slot_reuse_fails_closed`: after a new channel
  reuses a released slot, `send`, `try_send`, `recv_timeout` and `send_int` on the
  old handle fail with `Closed`, closing it again returns `Ok`, the new channel
  stays empty and still works, and a never-issued handle returns `NotFound`.
- `exec_channel_exhaustion_reports_error_and_recovers`: 128 open channels fill the
  table; the next `try_buffered_channel` fails with `Io`, `buffered_channel`
  returns handle `0`, capacity `0` fails with `InvalidInput`, and creation works
  again after one channel is closed.
- `exec_channel_blocked_receiver_releases_slot_after_close`: 150 rounds of a task
  blocked in `recv_timeout` woken by `close_sender`.
- `exec_pool_many_acquire_release_and_stats_calls_do_not_leak`: 250
  acquire/release cycles and 250 `pool_stats(...)` calls in one process, ending
  with `in_use == 0` and `total <= max_size`.

Example: `examples/io/channel_slot_reuse.aic` (prints `42`).
