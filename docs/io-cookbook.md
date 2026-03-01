# IO Cookbook

This cookbook provides deterministic implementation patterns for the current IO runtime.
Use these patterns when writing examples, agent workflows, or production-oriented AIC programs.

## 1. Interactive Input With Typed Fallbacks

Use `std.io` read APIs and branch on `IoError` instead of panicking.

```aic
import std.io;

fn read_name() -> String effects { io } {
    match prompt("Name: ") {
        Ok(value) => value,
        Err(EndOfInput) => "guest",
        Err(InvalidInput) => "guest",
        Err(Io) => "guest",
    }
}
```

Reference: `examples/io/interactive_greeter.aic`.

## 2. File Processing With Handle APIs

Prefer handle APIs for multi-line workflows.

```aic
import std.fs;

fn unwrap_handle(v: Result[FileHandle, FsError]) -> FileHandle {
    match v {
        Ok(handle) => handle,
        Err(_) => FileHandle { handle: 0 },
    }
}

fn process() -> Int effects { fs } {
    let reader = unwrap_handle(open_read("in.txt"));
    let first = file_read_line(reader);
    let _ = first;
    let _ = file_close(reader);
    0
}
```

References:

- `examples/io/file_processor.aic`
- `examples/io/line_reader.aic`

## 3. Tee-Style Logging (stdout + stderr + file)

Use stdout for user-facing status, stderr for operator diagnostics, and `std.fs` for persistent logs.

```aic
import std.io;
import std.fs;

fn log_once(path: String) -> Int effects { io, fs } {
    let _ = append_text(path, "INFO startup\n");
    println_str("INFO startup");
    eprint_str("WARN startup");
    flush_stdout();
    flush_stderr();
    0
}
```

Reference: `examples/io/log_tee.aic`.

## 4. Environment-Driven Config

Use `std.env.get` for optional values and keep defaults local.

```aic
import std.env;

fn config_token() -> String effects { env } {
    match get("AIC_TOKEN") {
        Ok(value) => value,
        Err(NotFound) => "default-token",
        Err(_) => "default-token",
    }
}
```

Reference: `examples/io/env_config.aic`.

## 5. Subprocess Pipelines

Use `run`/`pipe` for short commands, and `run_with` when cwd/env/stdin must be controlled.

```aic
import std.proc;
import std.vec;

fn pipeline_demo() -> Result[ProcOutput, ProcError] effects { proc, env } {
    let mut stages: Vec[String] = vec.vec_of("printf 'hello'");
    stages = vec.push(stages, "cat");
    pipe_chain(stages)
}
```

Reference: `examples/io/subprocess_pipeline.aic`.

## 6. Retry + Timeout Utilities

Use `std.retry` for exponential backoff with optional jitter, and `with_timeout` for deadline checks around operations.

```aic
import std.retry;

fn default_policy() -> RetryConfig {
    default_retry_config()
}
```

References:

- `examples/io/retry_with_jitter.aic`
- `docs/examples/retry-workflow.md`

## 7. Binary Framing With `std.buffer`

Use `ByteBuffer` for protocol layouts that require endian-aware integers, null-terminated strings, and backpatching.

```aic
import std.buffer;

fn frame() -> Result[ByteBuffer, BufferError] {
    match new_growable_buffer(32, 512) {
        Err(err) => Err(err),
        Ok(buf) => {
            let write_len = buf_write_i32_be(buf, 0);
            let write_kind = buf_write_u8(buf, 1);
            let write_tag = buf_write_cstring(buf, "msg");
            let end = buf_position(buf);
            let patch_len = buf_patch_u32_be(buf, 0, end);
            let _a = write_len;
            let _b = write_kind;
            let _c = write_tag;
            let _d = patch_len;
            Ok(buf)
        }
    }
}
```

Reference: `examples/data/binary_protocol.aic`.

Use `buf_close` when a frame buffer is no longer needed in long-lived workers to release memory eagerly.

## 8. Crypto Patterns (Hashes, HMAC, PBKDF2)

Use byte-level checks for digest validation and prefer typed decode/derive error handling.

```aic
import std.crypto;
import std.bytes;

fn bytes_or_empty(v: Result[Bytes, CryptoError]) -> Bytes {
    match v {
        Ok(value) => value,
        Err(_) => bytes.empty(),
    }
}

fn scram_seed(password: String) -> Bytes {
    bytes_or_empty(pbkdf2_sha256(password, bytes.from_string("salt"), 4096, 32))
}
```

Reference: `examples/crypto/pg_scram_auth.aic`.

## 9. Platform Caveat Patterns

When cross-platform behavior differs, branch on typed errors.

```aic
import std.net;

fn connect_or_skip(addr: String) -> Int effects { net } {
    match tcp_connect(addr, 500) {
        Ok(_) => 1,
        Err(Io) => 0,
        Err(_) => 0,
    }
}
```

Current runtime caveats to account for:

- Windows `std.net`: currently returns `NetError::Io`.
- Windows `std.proc`: `spawn/run_with/run_timeout/pipe_chain` can return `ProcError::Io`; `wait/kill/is_running` return `ProcError::UnknownProcess`.

## 10. TLS Client Handshake + Typed Fallback

Use `std.tls` for encrypted transport and branch on `TlsError` for deterministic behavior in environments that do not provide TLS backend support.

```aic
import std.tls;
import std.bytes;

fn tls_connected(stream: TlsStream) -> Int effects { net } {
    let sent = tls_send_bytes(stream, bytes.from_string("HEAD / HTTP/1.0\n\n"));
    let recv = tls_recv_bytes(stream, 256, 3000);
    let closed = tls_close(stream);
    let _a = sent;
    let _b = recv;
    let _c = closed;
    1
}

fn tls_probe(addr: String, host: String) -> Int effects { net } {
    let cfg = unsafe_insecure_tls_config(Some(host));
    match tls_connect_addr(addr, cfg, 3000) {
        Ok(stream) => tls_connected(stream),
        Err(ProtocolError) => 0,
        Err(Io) => 0,
        Err(_) => 0,
    }
}
```

Reference: `examples/io/tls_connect.aic`.
Policy: `docs/security-ops/tls-policy.v1.json`.

## 11. Unified Secure Error Contract

Normalize module-specific errors to machine-readable secure error metadata for deterministic branching.

```aic
import std.secure_errors;
import std.buffer;

fn protocol_category_or_unknown(v: Result[Int, BufferError]) -> String {
    match v {
        Ok(_) => "ok",
        Err(err) => secure_errors.buffer_error_info(err).category,
    }
}
```

Reference: `examples/io/secure_error_contract.aic`.
Contract: `docs/errors/secure-networking-error-contract.v1.json`.

## 12. Postgres TLS/SCRAM Canonical Replay

Use the canonical replay example when implementing production-style secure protocol clients with deterministic failure semantics.

```aic
import std.secure_errors;

fn classify(info: SecureErrorInfo) -> String {
    info.code
}
```

What this flow covers:

- startup/binary framing (`std.buffer`)
- SCRAM proof derivation (`std.crypto`)
- TLS secure-default + unsafe audit path (`std.tls`)
- retry and timeout behavior (`std.retry`)
- pool-capacity contract semantics (`PoolErrorContract`)

Reference example: `examples/io/postgres_tls_scram_reference.aic`.
Replay artifact: `docs/security-ops/postgres-tls-scram-replay.v1.json`.
