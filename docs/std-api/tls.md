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
fn tls_recv(stream: TlsStream, max_bytes: Int, timeout_ms: Int) -> Result[String, TlsError] effects { net }
fn tls_recv_bytes(stream: TlsStream, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, TlsError] effects { net }
fn tls_close(stream: TlsStream) -> Result[Bool, TlsError] effects { net }

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
- `TlsStream` participates in resource protocol checking (`E2006`) after `tls_close`.
- `TlsStream` also participates in runtime handle cleanup on scope drop (RAII close path).
