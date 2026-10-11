# Channel Slot Reclamation (Open Follow-Up)

Status: not started. This page is the brief for starting the work.

## Problem

The C runtime allocates concurrency channels from a fixed table,
`aic_rt_conc_channels[AIC_RT_CONC_CHANNEL_CAP]` in
`src/codegen/runtime/part03.c`. The table has 128 slots, and
`AIC_RT_LIMIT_CONC_CHANNELS` can only lower that number.

`aic_rt_conc_channel_int` takes the first slot that is not `active`. No code
sets a channel slot back to inactive: `aic_rt_conc_close_channel` only sets
`closed = 1`. A closed channel keeps its slot for the rest of the process.

Task slots behave differently. The join path frees the task slot, so tasks do
not have this problem.

## Evidence

A program that creates a `buffered_channel(1)` and closes it in a loop gets a
zero handle on call 129, although every earlier channel is closed:

```aic
import std.concurrent;
import std.io;

fn main() -> Int effects { io, concurrency } capabilities { io, concurrency } {
    let mut i = 0;
    let mut first_bad = 0;
    while i < 200 && first_bad == 0 {
        let ch: (Sender[Int], Receiver[Int]) = buffered_channel(1);
        if ch.0.handle <= 0 {
            first_bad = i + 1;
        } else {
            let _c = close_receiver(ch.1);
            ()
        };
        i = i + 1;
    };
    print_int(first_bad); // prints 129
    0
}
```

## Impact

- `std.pool` opens one reply channel for every `acquire(...)` round and every
  `pool_stats(...)` call, and `new_pool(...)` uses one request channel. A
  process can make only about 125 of these calls in total. After that, channel
  creation fails and `acquire(...)` returns `Closed`.
- Any code that opens a channel per request, such as request/reply patterns
  over `std.concurrent`, has the same lifetime ceiling.
- `examples/io/connection_pool.aic` bounds its polling loops to stay under the
  limit.

## Requirements

- Free a channel slot when it can never be used again: the channel is closed,
  its buffer is drained or dropped, and no operation is waiting on it.
- Make stale handles safe. Handles are plain `Int` values that AIC code copies
  freely (`Sender[T]` and `Receiver[T]` wrap `handle: Int`). A late operation on
  an old handle must fail with `Closed` or `NotFound`. It must never reach a new
  channel that reuses the slot. The `std.pool` manager depends on this: it sends
  late replies to channels whose caller already gave up, and it expects the send
  to fail so that it can reclaim the lease. Generation-tagged handles with a
  free-list are one way to meet this requirement.
- Drop the payloads (`aic_rt_conc_payloads`, 4096 slots) still buffered in a
  channel when its slot is freed, so that the payload table does not leak.
- Keep the Windows contract in `docs/io-concurrency-runtime.md` unchanged.

## Acceptance Criteria

- Add an execution test in `tests/execution_tests.rs` that creates and closes
  far more than 128 channels in one process.
- Add an execution test that runs more than 200 `std.pool` acquire/release
  cycles and `pool_stats(...)` calls in one process with no leaks
  (`in_use == 0`, `total` within `max_size`).
- Add an execution test that sends on a stale handle after its slot is reused
  and shows that the send fails and the new channel receives nothing.
- Existing pool tests pass, including
  `exec_pool_abandoned_acquire_returns_lease_to_pool`.
- Remove or update the known-limitation notes:
  - `docs/io-runtime/connection-pool.md`, "Known Limitations"
  - `docs/io-concurrency-runtime.md`, "Platform Limitations" > "All platforms"
  - `docs/io-api-reference.md`, the channel-slot note in the runtime limits
    list and the `std.pool` notes
  - the channel-limit comment in `examples/io/connection_pool.aic`, if it no
    longer applies
- Follow `AGENTS.md`: `make ci` passes, then commit.

## Starting the Task

1. Branch from `main`. Commits `979e240` and `032c259` hold the pool fix and
   the limitation notes this work builds on.
2. Read `aic_rt_conc_channel_int`, `aic_rt_conc_close_channel`, the send and
   receive paths, and `aic_rt_conc_get_channel` in
   `src/codegen/runtime/part03.c`. Also read the payload store and drop paths in
   `src/codegen/runtime/part04.c`.
3. Write the failing tests from "Acceptance Criteria" first.
4. Implement the reclamation and handle scheme, then update the docs listed
   above.
5. Do not run `scripts/ci/examples.sh` at the same time as another checkout's
   example run, unless your branch has the per-run temp directory change.
   Older copies of the script write to the shared `/tmp/aic-example.out`.
