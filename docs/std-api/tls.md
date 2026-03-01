# `std.tls`

`std.tls` provides TLS-encrypted transport wrappers over `std.net` TCP handles.

## Types

```aic
enum TlsError {
    HandshakeFailed,
    CertificateInvalid,
    CertificateExpired,
    HostnameMismatch,
    ProtocolError,
    ConnectionClosed,
    Io,
}

enum TlsVersion {
    Tls12,
    Tls13,
}

struct TlsConfig {
    verify_server: Bool,
    ca_cert_path: Option[String],
    client_cert_path: Option[String],
    client_key_path: Option[String],
    server_name: Option[String],
}

struct TlsStream {
    handle: Int,
}

enum ByteStream {
    Tcp(TcpStream),
    Tls(TlsStream),
}

enum ByteStreamError {
    Net(NetError),
    Tls(TlsError),
}
```

## API

```aic
fn default_tls_config() -> TlsConfig
fn unsafe_insecure_tls_config(server_name: Option[String]) -> TlsConfig

fn tls_connect_with_config(tcp_fd: Int, config: TlsConfig) -> Result[TlsStream, TlsError] effects { net }
fn tls_connect(tcp_fd: Int, hostname: String, config: TlsConfig) -> Result[TlsStream, TlsError] effects { net }
fn tls_upgrade(tcp_fd: Int, hostname: String, config: TlsConfig) -> Result[TlsStream, TlsError] effects { net }
fn tls_connect_addr(addr: String, config: TlsConfig, timeout_ms: Int) -> Result[TlsStream, TlsError] effects { net }

fn tls_accept_timeout(listener_handle: Int, config: TlsConfig, timeout_ms: Int) -> Result[TlsStream, TlsError] effects { net }
fn tls_accept(listener_handle: Int, config: TlsConfig) -> Result[TlsStream, TlsError] effects { net }

fn tls_send(stream: TlsStream, payload: String) -> Result[Int, TlsError] effects { net }
fn tls_send_bytes(stream: TlsStream, data: Bytes) -> Result[Int, TlsError] effects { net }
fn tls_send_timeout(stream: TlsStream, payload: String, timeout_ms: Int) -> Result[Int, TlsError] effects { net }
fn tls_send_bytes_timeout(stream: TlsStream, data: Bytes, timeout_ms: Int) -> Result[Int, TlsError] effects { net }
fn tls_recv(stream: TlsStream, max_bytes: Int, timeout_ms: Int) -> Result[String, TlsError] effects { net }
fn tls_recv_bytes(stream: TlsStream, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, TlsError] effects { net }
fn tls_async_send_submit(stream: TlsStream, data: Bytes, timeout_ms: Int) -> Result[AsyncIntOp, TlsError] effects { net, concurrency }
fn tls_async_recv_submit(stream: TlsStream, max_bytes: Int, timeout_ms: Int) -> Result[AsyncStringOp, TlsError] effects { net, concurrency }
fn tls_async_wait_int(op: AsyncIntOp, timeout_ms: Int) -> Result[Int, TlsError] effects { net, concurrency }
fn tls_async_wait_string(op: AsyncStringOp, timeout_ms: Int) -> Result[Bytes, TlsError] effects { net, concurrency }
fn tls_async_send(stream: TlsStream, data: Bytes, timeout_ms: Int) -> Result[Int, TlsError] effects { net, concurrency }
fn tls_async_recv(stream: TlsStream, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, TlsError] effects { net, concurrency }
fn tls_async_shutdown() -> Result[Bool, TlsError] effects { net, concurrency }
fn tls_recv_exact_deadline(stream: TlsStream, expected_bytes: Int, deadline_ms: Int) -> Result[Bytes, TlsError] effects { net, time }
fn tls_recv_exact(stream: TlsStream, expected_bytes: Int, timeout_ms: Int) -> Result[Bytes, TlsError] effects { net, time }
fn tls_recv_framed_deadline(stream: TlsStream, max_frame_bytes: Int, deadline_ms: Int) -> Result[Bytes, TlsError] effects { net, time }
fn tls_recv_framed(stream: TlsStream, max_frame_bytes: Int, timeout_ms: Int) -> Result[Bytes, TlsError] effects { net, time }
fn tls_close(stream: TlsStream) -> Result[Bool, TlsError] effects { net }

fn byte_stream_from_tcp(handle: Int) -> ByteStream
fn byte_stream_from_tcp_stream(stream: TcpStream) -> ByteStream
fn byte_stream_from_tls(stream: TlsStream) -> ByteStream
fn byte_stream_send(stream: ByteStream, payload: Bytes) -> Result[Int, ByteStreamError] effects { net }
fn byte_stream_send_timeout(stream: ByteStream, payload: Bytes, timeout_ms: Int) -> Result[Int, ByteStreamError] effects { net }
fn byte_stream_recv(stream: ByteStream, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, ByteStreamError] effects { net }
fn byte_stream_recv_exact_deadline(stream: ByteStream, expected_bytes: Int, deadline_ms: Int) -> Result[Bytes, ByteStreamError] effects { net, time }
fn byte_stream_recv_exact(stream: ByteStream, expected_bytes: Int, timeout_ms: Int) -> Result[Bytes, ByteStreamError] effects { net, time }
fn byte_stream_recv_framed_deadline(stream: ByteStream, max_frame_bytes: Int, deadline_ms: Int) -> Result[Bytes, ByteStreamError] effects { net, time }
fn byte_stream_recv_framed(stream: ByteStream, max_frame_bytes: Int, timeout_ms: Int) -> Result[Bytes, ByteStreamError] effects { net, time }
fn byte_stream_close(stream: ByteStream) -> Result[Bool, ByteStreamError] effects { net }

fn tls_peer_subject(stream: TlsStream) -> Result[String, TlsError] effects { net }
fn tls_peer_cn(stream: TlsStream) -> Result[String, TlsError] effects { net }
fn tls_version(stream: TlsStream) -> Result[TlsVersion, TlsError] effects { net }
```

## Client Example

```aic
module docs.std_api.tls_client;

import std.tls;
import std.bytes;

fn main() -> Int effects { net } capabilities { net } {
    let cfg = default_tls_config();
    match tls_connect_addr("example.com:443", cfg, 3000) {
        Ok(stream) => if true {
            let _sent = tls_send_bytes(stream, bytes.from_string("GET / HTTP/1.0\nHost: example.com\n\n"));
            let _recv = tls_recv_bytes(stream, 1024, 3000);
            let _close = tls_close(stream);
            0
        } else {
            1
        },
        Err(_) => 1,
    }
}
```

## ByteStream Adapter Example

```aic
module docs.std_api.tls_byte_stream;

import std.net;
import std.tls;
import std.bytes;

fn main() -> Int effects { net } capabilities { net } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(v) => v,
        Err(_) => "",
    };
    let client = match tcp_connect(addr, 3000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match tcp_accept(listener, 3000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let _sent = byte_stream_send(byte_stream_from_tcp(client), bytes.from_string("ping"));
    let _recv = byte_stream_recv(byte_stream_from_tcp_stream(tcp_stream(server)), 256, 3000);
    let _client_close = byte_stream_close(byte_stream_from_tcp(client));
    let _server_close = byte_stream_close(byte_stream_from_tcp(server));
    let _listener_close = tcp_close(listener);
    0
}
```

## Upgrade Example (StartTLS-style)

```aic
module docs.std_api.tls_upgrade;

import std.net;
import std.tls;

fn main() -> Int effects { net } capabilities { net } {
    let cfg = default_tls_config();
    match tcp_connect("example.com:443", 3000) {
        Ok(tcp) => match tls_upgrade(tcp, "example.com", cfg) {
            Ok(stream) => match tls_close(stream) {
                Ok(_) => 0,
                Err(_) => 1,
            },
            Err(_) => 1,
        },
        Err(_) => 1,
    }
}
```

## Server Example (accept wrapper)

```aic
module docs.std_api.tls_accept;

import std.net;
import std.tls;

fn none_string() -> Option[String] { None() }

fn main() -> Int effects { net } capabilities { net } {
    let listener = match tcp_listen("127.0.0.1:9443") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let cfg = TlsConfig {
        verify_server: false,
        ca_cert_path: none_string(),
        client_cert_path: Some("server_cert.pem"),
        client_key_path: Some("server_key.pem"),
        server_name: none_string(),
    };
    match tls_accept_timeout(listener, cfg, 1000) {
        Ok(stream) => match tls_close(stream) {
            Ok(_) => 0,
            Err(_) => 1,
        },
        Err(_) => 1,
    }
}
```

## Operational Notes

- `default_tls_config()` is secure-by-default (`verify_server: true`).
- `unsafe_insecure_tls_config(...)` must only be used in explicitly audited scenarios.
- `tls_recv` / `tls_recv_bytes` surface peer EOF/close as `TlsError::ConnectionClosed`; timeout remains the non-close path (`TlsError::Io`).
- `TlsStream` participates in resource protocol checking (`E2006`) after `tls_close`.
- `TlsStream` also participates in runtime handle cleanup on scope drop (RAII close path).
- `ByteStream` provides protocol-agnostic byte I/O by adapting `TcpStream` and `TlsStream`.
- `tls_send_timeout`/`tls_send_bytes_timeout` enforce timeout-bounded TLS writes.
- TLS write timeout expiry maps to `TlsError::Io` because `TlsError` has no `Timeout` variant.
- TLS async submit/wait wrappers are bytes-first (`tls_async_*`) and require `effects { net, concurrency }`.
- `tls_async_wait_int` / `tls_async_wait_string` timeout returns `TlsError::Io` while keeping the operation pending for retry.
- Re-waiting a consumed TLS async op returns `TlsError::ProtocolError`.
- Runnable async submit/wait usage example: `examples/io/tls_async_submit_wait.aic`.
- Exact read APIs (`*_recv_exact*`) keep reading until `expected_bytes` is satisfied or the deadline budget is exhausted.
- Framed read APIs (`*_recv_framed*`) decode a 4-byte big-endian length prefix, enforce `max_frame_bytes`, then read the exact payload.
