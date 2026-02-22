# Example Debug Session (`aic build --debug-info`)

This is a concrete session you can replay locally.

## Build

```bash
aic build examples/e5/panic_line_map.aic --debug-info -o target/debug/panic_line_map_dbg
```

Output:

```text
built target/debug/panic_line_map_dbg
```

## LLDB session

```bash
lldb target/debug/panic_line_map_dbg
```

```text
(lldb) breakpoint set --name aic_rt_panic
(lldb) run
Process ... stopped
* thread #1, stop reason = breakpoint ... aic_rt_panic
(lldb) bt
(lldb) frame select 1
(lldb) source list
```

Expected runtime message when not stopping first:

```text
AICore panic at 4:11: panic_line_map example
```

## GDB session

```bash
gdb target/debug/panic_line_map_dbg
```

```text
(gdb) break aic_rt_panic
(gdb) run
Breakpoint 1, aic_rt_panic (...)
(gdb) bt
(gdb) frame 1
(gdb) list
```

## What this verifies

- `--debug-info` produced debugger-visible symbols and line locations.
- Panic sites map back to `.aic` source coordinates (`4:11` in this example).
