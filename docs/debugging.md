# Debugging AIC Programs with LLDB/GDB

This workflow uses compiler-emitted debug metadata from:

```bash
aic build <input.aic> --debug-info -o <output-binary>
```

`--debug-info` enables LLVM debug metadata and compiles with `clang -g`.

## 1. Build a debuggable binary

Example:

```bash
aic build examples/e5/panic_line_map.aic --debug-info -o target/debug/panic_line_map_dbg
```

Expected CLI output format:

```text
built target/debug/panic_line_map_dbg
```

Running the binary directly (outside a debugger) should still show source-mapped panic locations:

```text
AICore panic at 4:11: panic_line_map example
```

Enable runtime stack traces when needed:

```bash
AIC_BACKTRACE=1 target/debug/panic_line_map_dbg
```

`AIC_BACKTRACE` follows `RUST_BACKTRACE`-style semantics: `0/false` disables, any other non-empty value enables.

## 2. LLDB workflow

```bash
lldb target/debug/panic_line_map_dbg
```

Inside LLDB:

```text
(lldb) breakpoint set --name aic_rt_panic
(lldb) run
(lldb) bt
(lldb) frame select 1
(lldb) source list
```

Notes:
- Break on `aic_rt_panic` to stop at runtime panic entry.
- `frame select 1` typically moves from runtime shim into generated program frames.
- `source list` uses DWARF locations emitted by `--debug-info`.

## 3. GDB workflow

```bash
gdb target/debug/panic_line_map_dbg
```

Inside GDB:

```text
(gdb) break aic_rt_panic
(gdb) run
(gdb) bt
(gdb) frame 1
(gdb) list
```

Notes:
- `bt` shows mixed runtime and generated frames.
- `list` should resolve to `.aic` source locations when debug info is present.

## 4. Fast sanity checks

- Confirm build used `--debug-info` (without it, symbol/line mapping is reduced).
- Keep the source tree at the same paths used during build so debugger source lookup works.
- Use a deterministic output path (`-o target/debug/<name>`) so repeated sessions reuse symbols.
