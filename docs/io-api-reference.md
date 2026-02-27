# IO API Reference

This file is the agent-facing reference for the current IO runtime surface.
Source of truth is the current repository state in `std/*.aic` and runtime lowering in `src/codegen.rs`.

## Scope

Covered modules:

- `std.io`
- `std.error_context`
- `std.fs`
- `std.env`
- `std.path`
- `std.proc`
- `std.net`
- `std.time`
- `std.signal`
- `std.rand`
- `std.retry`
- `std.set`
- `std.log`
- `std.buffer`

## Effect Taxonomy

Known effects (from `src/effects.rs`):

- `io`
- `fs`
- `net`
- `time`
- `rand`
- `env`
- `proc`
- `concurrency`

Typechecking enforces direct and transitive effect declarations (`E2001`, `E2005`).

## Runtime Error Mapping

The backend maps runtime status codes to typed error enums in `src/codegen.rs`.

| Module | Status-to-variant mapping |
|---|---|
| `IoError` | `1=EndOfInput`, `2=InvalidInput`, `3=Io` |
| `FsError` | `1=NotFound`, `2=PermissionDenied`, `3=AlreadyExists`, `4=InvalidInput`, `5=Io` |
| `EnvError` | `1=NotFound`, `2=PermissionDenied`, `3=InvalidInput`, `4=Io` |
| `ProcError` | `1=NotFound`, `2=PermissionDenied`, `3=InvalidInput`, `4=Io`, `5=UnknownProcess` |
| `NetError` | `1=NotFound`, `2=PermissionDenied`, `3=Refused`, `4=Timeout`, `5=AddressInUse`, `6=InvalidInput`, `7=Io` |
| `TimeError` | `1=InvalidFormat`, `2=InvalidDate`, `3=InvalidTime`, `4=InvalidOffset`, `5=InvalidInput`, `6=Internal` |
| `SignalError` | `1=UnsupportedPlatform`, `2=InvalidSignal`, `3=PermissionDenied`, `4=Internal` |
| `BufferError` | `1=Underflow`, `2=Overflow`, `3=InvalidUtf8`, `4=InvalidInput` |

## `std.io`

```aic
enum IoError {
    EndOfInput,
    InvalidInput,
    Io,
}

type IoErrorContext = ErrorContext[IoError]

fn print_int(x: Int) -> () effects { io }
fn print_str(x: String) -> () effects { io }
fn print_float(x: Float) -> () effects { io }
fn read_line() -> Result[String, IoError] effects { io }
fn read_int() -> Result[Int, IoError] effects { io }
fn read_char() -> Result[String, IoError] effects { io }
fn prompt(message: String) -> Result[String, IoError] effects { io }
fn eprint_str(x: String) -> () effects { io }
fn eprint_int(x: Int) -> () effects { io }
fn println_str(x: String) -> () effects { io }
fn println_int(x: Int) -> () effects { io }
fn print_bool(x: Bool) -> () effects { io }
fn println_bool(x: Bool) -> () effects { io }
fn flush_stdout() -> () effects { io }
fn flush_stderr() -> () effects { io }
fn panic(message: String) -> () effects { io }

fn from_fs_error(err: FsError) -> IoError
fn from_net_error(err: NetError) -> IoError
fn from_proc_error(err: ProcError) -> IoError
fn from_env_error(err: EnvError) -> IoError

fn from_fs_error_with_context(err: FsError, context: String) -> IoErrorContext
fn from_net_error_with_context(err: NetError, context: String) -> IoErrorContext
fn from_proc_error_with_context(err: ProcError, context: String) -> IoErrorContext
fn from_env_error_with_context(err: EnvError, context: String) -> IoErrorContext
fn io_error(ctx: IoErrorContext) -> IoError
```

Notes:

- `prompt` writes the message, flushes stdout, then reads one line.
- `read_char` expects a single UTF-8 scalar value from one input line.
- Existing `Result[..., IoError]` APIs are unchanged; context chaining is opt-in via `from_*_error_with_context(...)`.
- Context chain format is append-only and flattened as text (for example: `open config -> fs.NotFound -> io.EndOfInput -> bootstrap`).

## `std.error_context`

```aic
struct ErrorContext[E] {
    error: E,
    context: String,
    chain: String,
}

fn new_error_context[E](error: E, context: String) -> ErrorContext[E]
fn with_context[E](ctx: ErrorContext[E], context: String) -> ErrorContext[E]
fn with_context_error[E](error: E, context: String) -> ErrorContext[E]
fn with_cause[E](error: E, context: String, cause: String) -> ErrorContext[E]
fn with_cause_context[E](ctx: ErrorContext[E], cause: String) -> ErrorContext[E]
fn error_value[E](ctx: ErrorContext[E]) -> E
fn error_chain[E](ctx: ErrorContext[E]) -> String
```

## `std.fs`

```aic
enum FsError {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    InvalidInput,
    Io,
}

struct FsMetadata {
    is_file: Bool,
    is_dir: Bool,
    size: Int,
}

struct FileHandle {
    handle: Int,
}

fn exists(path: String) -> Bool effects { fs }
fn read_text(path: String) -> Result[String, FsError] effects { fs }
fn write_text(path: String, content: String) -> Result[Bool, FsError] effects { fs }
fn append_text(path: String, content: String) -> Result[Bool, FsError] effects { fs }
fn copy(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs }
fn move(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs }
fn delete(path: String) -> Result[Bool, FsError] effects { fs }
fn metadata(path: String) -> Result[FsMetadata, FsError] effects { fs }
fn walk_dir(path: String) -> Result[Vec[String], FsError] effects { fs }
fn temp_file(prefix: String) -> Result[String, FsError] effects { fs }
fn temp_dir(prefix: String) -> Result[String, FsError] effects { fs }
fn read_bytes(path: String) -> Result[String, FsError] effects { fs }
fn write_bytes(path: String, content: String) -> Result[Bool, FsError] effects { fs }
fn append_bytes(path: String, content: String) -> Result[Bool, FsError] effects { fs }
fn open_read(path: String) -> Result[FileHandle, FsError] effects { fs }
fn open_write(path: String) -> Result[FileHandle, FsError] effects { fs }
fn open_append(path: String) -> Result[FileHandle, FsError] effects { fs }
fn file_read_line(file: FileHandle) -> Result[Option[String], FsError] effects { fs }
fn file_write_str(file: FileHandle, content: String) -> Result[Bool, FsError] effects { fs }
fn file_close(file: FileHandle) -> Result[Bool, FsError] effects { fs }
fn mkdir(path: String) -> Result[Bool, FsError] effects { fs }
fn mkdir_all(path: String) -> Result[Bool, FsError] effects { fs }
fn rmdir(path: String) -> Result[Bool, FsError] effects { fs }
fn list_dir(path: String) -> Result[Vec[String], FsError] effects { fs }
fn create_symlink(target_path: String, link_path: String) -> Result[Bool, FsError] effects { fs }
fn read_symlink(path: String) -> Result[String, FsError] effects { fs }
fn set_readonly(path: String, readonly: Bool) -> Result[Bool, FsError] effects { fs }
```

Notes:

- File-handle table capacity is bounded (`1024` runtime slots).
- `walk_dir` currently exposes count-only `Vec` payload semantics in codegen; use `vec_len(...)` as the stable operation.
- `list_dir` returns concrete directory entry strings.
- Windows caveats:
  - `create_symlink` may fail with privilege-related errors.
  - `read_symlink` currently returns `FsError::Io`.

## `std.env`

```aic
enum EnvError {
    NotFound,
    PermissionDenied,
    InvalidInput,
    Io,
}

struct EnvEntry {
    key: String,
    value: String,
}

fn get(key: String) -> Result[String, EnvError] effects { env }
fn set(key: String, value: String) -> Result[Bool, EnvError] effects { env }
fn remove(key: String) -> Result[Bool, EnvError] effects { env }
fn cwd() -> Result[String, EnvError] effects { env, fs }
fn set_cwd(path: String) -> Result[Bool, EnvError] effects { env, fs }
fn args() -> Vec[String] effects { env }
fn arg_count() -> Int effects { env }
fn arg_at(index: Int) -> Option[String] effects { env }
fn exit(code: Int) -> () effects { env }
fn all_vars() -> Vec[EnvEntry] effects { env }
fn home_dir() -> Result[String, EnvError] effects { env, fs }
fn temp_dir() -> Result[String, EnvError] effects { env, fs }
fn os_name() -> String effects { env }
fn arch() -> String effects { env }
```

Notes:

- Invalid variable names (empty or containing `=`) map to `EnvError::InvalidInput`.
- `args` and `all_vars` return snapshots of process state at call time.

## `std.set`

```aic
struct Set[T] {
    items: Map[T, Int],
}

fn new_set[T]() -> Set[T]
fn add[T](s: Set[T], value: T) -> Set[T]
fn has[T](s: Set[T], value: T) -> Bool
fn discard[T](s: Set[T], value: T) -> Set[T]
fn set_size[T](s: Set[T]) -> Int
fn to_vec[T](s: Set[T]) -> Vec[T]
fn union[T](left: Set[T], right: Set[T]) -> Set[T]
fn intersection[T](left: Set[T], right: Set[T]) -> Set[T]
fn difference[T](left: Set[T], right: Set[T]) -> Set[T]
```

Notes:

- `add`/`has`/`discard` are the supported mutator/query APIs.
- `to_vec` is deterministic and returns members in ascending key order.
- Current backend limitation is deterministic: non-`String` key specializations fail with backend diagnostic `E5011` (`...String key...`) until key support is widened.

## `std.log`

```aic
enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

fn log(level: LogLevel, message: String) -> () effects { io }
fn debug(message: String) -> () effects { io }
fn info(message: String) -> () effects { io }
fn warn(message: String) -> () effects { io }
fn error(message: String) -> () effects { io }
fn set_level(level: LogLevel) -> () effects { io }
fn set_json_output(enabled: Bool) -> () effects { io }
```

Notes:

- Default runtime level is `Info`; `Debug` is filtered until level is lowered.
- `set_json_output(true)` switches stderr output to JSON lines with `level`, `msg`, `ts`, and `trace_id`.
- `AIC_LOG_LEVEL` and `AIC_LOG_JSON` environment variables are read at runtime startup and can be overridden by API calls.

## `std.path`

```aic
fn join(left: String, right: String) -> String
fn basename(path: String) -> String
fn dirname(path: String) -> String
fn extension(path: String) -> String
fn is_abs(path: String) -> Bool
```

Notes:

- Path helpers are pure (no effects).
- `join` returns `right` directly when `right` is absolute.

## `std.proc`

```aic
enum ProcError {
    NotFound,
    PermissionDenied,
    InvalidInput,
    Io,
    UnknownProcess,
}

struct ProcOutput {
    status: Int,
    stdout: String,
    stderr: String,
}

struct RunOptions {
    stdin: String,
    cwd: String,
    env: Vec[String],
    timeout_ms: Int,
}

fn spawn(command: String) -> Result[Int, ProcError] effects { proc, env }
fn wait(handle: Int) -> Result[Int, ProcError] effects { proc }
fn kill(handle: Int) -> Result[Bool, ProcError] effects { proc }
fn run(command: String) -> Result[ProcOutput, ProcError] effects { proc, env }
fn pipe(left: String, right: String) -> Result[ProcOutput, ProcError] effects { proc, env }
fn run_with(command: String, options: RunOptions) -> Result[ProcOutput, ProcError] effects { proc, env }
fn is_running(handle: Int) -> Result[Bool, ProcError] effects { proc }
fn current_pid() -> Result[Int, ProcError] effects { proc }
fn run_timeout(command: String, timeout_ms: Int) -> Result[ProcOutput, ProcError] effects { proc, env }
fn pipe_chain(stages: Vec[String]) -> Result[ProcOutput, ProcError] effects { proc, env }
```

Notes:

- `run`/`pipe` success is about launch/execution plumbing; check `ProcOutput.status` for command exit status.
- Spawned-handle table capacity is bounded (`64` runtime slots).
- Windows caveats:
  - `spawn` returns `ProcError::Io`.
  - `wait`, `kill`, `is_running` return `ProcError::UnknownProcess`.
  - `run_with`, `run_timeout`, `pipe_chain` return `ProcError::Io`.
- `run`, `pipe`, and `current_pid` remain available.

## `std.signal`

```aic
enum Signal {
    SigInt,
    SigTerm,
    SigHup,
}

enum SignalError {
    UnsupportedPlatform,
    InvalidSignal,
    PermissionDenied,
    Internal,
}

fn register(signal: Signal) -> Result[Bool, SignalError] effects { proc }
fn register_shutdown_handlers() -> Result[Bool, SignalError] effects { proc }
fn wait_for_signal() -> Result[Signal, SignalError] effects { proc }
```

Notes:

- Runtime support is implemented for Linux/macOS only and handles `SIGINT`, `SIGTERM`, and `SIGHUP`.
- Windows and other non-Linux/macOS targets return `SignalError::UnsupportedPlatform`.
- `wait_for_signal` blocks until one of the registered signals arrives.

## `std.net`

```aic
enum NetError {
    NotFound,
    PermissionDenied,
    Refused,
    Timeout,
    AddressInUse,
    InvalidInput,
    Io,
}

struct UdpPacket {
    from: String,
    payload: String,
}

fn tcp_listen(addr: String) -> Result[Int, NetError] effects { net }
fn tcp_local_addr(handle: Int) -> Result[String, NetError] effects { net }
fn tcp_accept(listener: Int, timeout_ms: Int) -> Result[Int, NetError] effects { net }
fn tcp_connect(addr: String, timeout_ms: Int) -> Result[Int, NetError] effects { net }
fn tcp_send(handle: Int, payload: String) -> Result[Int, NetError] effects { net }
fn tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[String, NetError] effects { net }
fn tcp_close(handle: Int) -> Result[Bool, NetError] effects { net }
fn udp_bind(addr: String) -> Result[Int, NetError] effects { net }
fn udp_local_addr(handle: Int) -> Result[String, NetError] effects { net }
fn udp_send_to(handle: Int, addr: String, payload: String) -> Result[Int, NetError] effects { net }
fn udp_recv_from(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[UdpPacket, NetError] effects { net }
fn udp_close(handle: Int) -> Result[Bool, NetError] effects { net }
fn dns_lookup(host: String) -> Result[String, NetError] effects { net }
fn dns_reverse(addr: String) -> Result[String, NetError] effects { net }
```

Notes:

- Network-handle table capacity is bounded (`128` runtime slots).
- On Windows, current runtime implementation returns `NetError::Io` for all `std.net` APIs.

## `std.time`

```aic
enum TimeError {
    InvalidFormat,
    InvalidDate,
    InvalidTime,
    InvalidOffset,
    InvalidInput,
    Internal,
}

struct DateTime {
    year: Int,
    month: Int,
    day: Int,
    hour: Int,
    minute: Int,
    second: Int,
    millisecond: Int,
    offset_minutes: Int,
}

fn now_ms() -> Int effects { time }
fn now() -> Int effects { time }
fn monotonic_ms() -> Int effects { time }
fn sleep_ms(ms: Int) -> () effects { time }
fn parse_rfc3339(text: String) -> Result[DateTime, TimeError] effects { time }
fn parse_iso8601(text: String) -> Result[DateTime, TimeError] effects { time }
fn format_rfc3339(value: DateTime) -> Result[String, TimeError] effects { time }
fn format_iso8601(value: DateTime) -> Result[String, TimeError] effects { time }
fn deadline_after_ms(timeout_ms: Int) -> Int effects { time }
fn remaining_ms(deadline_ms: Int) -> Int effects { time }
fn timeout_expired(deadline_ms: Int) -> Bool effects { time }
fn sleep_until(deadline_ms: Int) -> () effects { time }
```

Notes:

- `now()` is compatibility API and is deprecated in policy metadata in favor of `now_ms()`.
- `parse_rfc3339` requires timezone and seconds.
- `parse_iso8601` accepts date-only and timezone-optional forms.

## `std.rand`

```aic
fn seed(seed_value: Int) -> () effects { rand }
fn random_int() -> Int effects { rand }
fn random_bool() -> Bool effects { rand }
fn random_range(min_inclusive: Int, max_exclusive: Int) -> Int effects { rand }
```

Notes:

- `seed(...)` makes sequences deterministic.
- `random_range(a, b)` returns `a` when `b <= a`.

## `std.retry`

```aic
struct RetryConfig {
    max_attempts: Int,
    initial_backoff_ms: Int,
    backoff_multiplier: Int,
    max_backoff_ms: Int,
    jitter_enabled: Bool,
    jitter_ms: Int,
}

struct RetryResult[T] {
    result: Result[T, String],
    attempts: Int,
    elapsed_ms: Int,
}

fn default_retry_config() -> RetryConfig
fn retry[T](config: RetryConfig, operation: Fn() -> Result[T, String]) -> RetryResult[T] effects { time, rand }
fn with_timeout[T](timeout_ms: Int, operation: Fn() -> T) -> Result[T, String] effects { time }
```

Notes:

- `retry` uses exponential backoff (`initial_backoff_ms`, `backoff_multiplier`) capped by `max_backoff_ms`.
- Jitter is optional and controlled by `jitter_enabled` + `jitter_ms`.
- `RetryResult` always reports `attempts` and total `elapsed_ms`.
- `with_timeout` enforces deadline checks before and after operation execution; the wrapped operation is not force-cancelled mid-call.

## `std.buffer`

```aic
enum BufferError {
    Underflow,
    Overflow,
    InvalidUtf8,
    InvalidInput,
}

struct ByteBuffer {
    handle: Int,
}

fn new_buffer(capacity: Int) -> ByteBuffer
fn buffer_from_bytes(data: Bytes) -> ByteBuffer
fn buffer_to_bytes(buf: ByteBuffer) -> Bytes

fn buf_position(buf: ByteBuffer) -> Int
fn buf_remaining(buf: ByteBuffer) -> Int
fn buf_seek(buf: ByteBuffer, position: Int) -> Result[(), BufferError]
fn buf_reset(buf: ByteBuffer) -> ()

fn buf_read_u8(buf: ByteBuffer) -> Result[Int, BufferError]
fn buf_read_i16_be(buf: ByteBuffer) -> Result[Int, BufferError]
fn buf_read_i32_be(buf: ByteBuffer) -> Result[Int, BufferError]
fn buf_read_i64_be(buf: ByteBuffer) -> Result[Int, BufferError]
fn buf_read_i16_le(buf: ByteBuffer) -> Result[Int, BufferError]
fn buf_read_i32_le(buf: ByteBuffer) -> Result[Int, BufferError]
fn buf_read_i64_le(buf: ByteBuffer) -> Result[Int, BufferError]
fn buf_read_bytes(buf: ByteBuffer, count: Int) -> Result[Bytes, BufferError]
fn buf_read_cstring(buf: ByteBuffer) -> Result[String, BufferError]
fn buf_read_length_prefixed(buf: ByteBuffer) -> Result[Bytes, BufferError]

fn buf_write_u8(buf: ByteBuffer, value: Int) -> Result[(), BufferError]
fn buf_write_i16_be(buf: ByteBuffer, value: Int) -> Result[(), BufferError]
fn buf_write_i32_be(buf: ByteBuffer, value: Int) -> Result[(), BufferError]
fn buf_write_i64_be(buf: ByteBuffer, value: Int) -> Result[(), BufferError]
fn buf_write_i16_le(buf: ByteBuffer, value: Int) -> Result[(), BufferError]
fn buf_write_i32_le(buf: ByteBuffer, value: Int) -> Result[(), BufferError]
fn buf_write_i64_le(buf: ByteBuffer, value: Int) -> Result[(), BufferError]
fn buf_write_bytes(buf: ByteBuffer, data: Bytes) -> Result[(), BufferError]
fn buf_write_cstring(buf: ByteBuffer, s: String) -> Result[(), BufferError]
fn buf_write_string_prefixed(buf: ByteBuffer, s: String) -> Result[(), BufferError]
```

Notes:

- APIs are pure (no effect declaration required).
- `new_buffer(capacity)` is fixed-capacity; writes past capacity return `Overflow`.
- Reads past available bytes return `Underflow` (never panic).
- `buf_read_cstring` requires null-terminated valid UTF-8; invalid payload returns `InvalidUtf8`.
- `buf_read_length_prefixed` expects signed big-endian i32 length; negative lengths return `InvalidInput`.
- `buf_seek` validates bounds (`0 <= position <= length`) and returns `InvalidInput` on invalid positions.

## Deterministic Validation Commands

```bash
cargo run --quiet --bin aic -- std-compat
cargo run --quiet --bin aic -- check examples/io/interactive_greeter.aic
cargo run --quiet --bin aic -- explain E2001
```
