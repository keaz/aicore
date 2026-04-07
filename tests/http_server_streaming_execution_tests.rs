use std::fs;
use std::process::{Command, Stdio};

use aicore::codegen::{compile_with_clang, emit_llvm};
use aicore::contracts::lower_runtime_asserts;
use aicore::driver::{has_errors, run_frontend};
use tempfile::tempdir;

fn compile_and_run(source: &str) -> (i32, String, String) {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("main.aic");
    fs::write(&src, source).expect("write source");

    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &src.to_string_lossy()).expect("emit llvm");

    let exe = dir.path().join("app");
    compile_with_clang(&llvm.llvm_ir, &exe, dir.path()).expect("clang build");

    let output = Command::new(exe)
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run exe");

    (
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_streaming_request_body_iteration_round_trips() {
    let src = r#"
import std.bytes;
import std.http_server;
import std.io;
import std.map;
import std.net;
import std.string;

fn empty_reader() -> RequestBodyReader {
    RequestBodyReader {
        head: RequestHead {
            method: "",
            path: "",
            query: map.new_map(),
            headers: map.new_map(),
            content_length: 0,
            chunked: false,
        },
        body: bytes.empty(),
        cursor: 0,
        chunk_bytes: 1,
    }
}

fn main() -> Int effects { io, net } capabilities { io, net } {
    let listener = match listen("127.0.0.1:0") {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(value) => value,
        Err(_) => "",
    };
    let client = match tcp_connect(addr, 1000) {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let server = match accept(listener, 1000) {
        Ok(handle) => handle,
        Err(_) => 0,
    };

    let request_wire = "POST /stream HTTP/1.1\r\nHost: localhost\r\nContent-Length: 11\r\n\r\nstream-body";
    let sent = match tcp_send(client, bytes.from_string(request_wire)) {
        Ok(value) => value,
        Err(_) => 0,
    };

    let reader = match read_request_stream(server, 4096, 1000, 4) {
        Ok(value) => value,
        Err(_) => empty_reader(),
    };
    let head = request_stream_head(reader);
    let head_ok = if len(head.method) == 4 &&
        len(head.path) == 7 &&
        head.content_length == 11 &&
        head.chunked == false {
        1
    } else {
        0
    };

    let mut current = reader;
    let mut collected = bytes.empty();
    let mut chunk_count = 0;
    let mut done = false;
    while !done {
        let step = match request_stream_read_next(current) {
            Ok(value) => value,
            Err(_) => RequestBodyStep {
                reader: empty_reader(),
                chunk: None(),
            },
        };
        current = step.reader;
        let chunk = match step.chunk {
            Some(value) => value,
            None => bytes.empty(),
        };
        let chunk_len = bytes.byte_len(chunk);
        if chunk_len > 0 {
            collected = bytes.concat(collected, chunk);
            chunk_count = chunk_count + 1;
        } else {
            done = true;
        };
    };

    let body_text = match bytes.to_string(collected) {
        Ok(value) => value,
        Err(_) => "",
    };
    let done_ok = if request_stream_done(current) { 1 } else { 0 };

    let closed_client = match tcp_close(client) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_server = match close(server) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_listener = match close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if sent > 0 &&
        head_ok == 1 &&
        chunk_count == 3 &&
        done_ok == 1 &&
        len(body_text) == 11 &&
        string.contains(body_text, "stream-body") &&
        closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_streaming_response_chunk_writes_round_trip() {
    let src = r#"
import std.bytes;
import std.http_server;
import std.io;
import std.map;
import std.net;
import std.string;

fn empty_reader() -> RequestBodyReader {
    RequestBodyReader {
        head: RequestHead {
            method: "",
            path: "",
            query: map.new_map(),
            headers: map.new_map(),
            content_length: 0,
            chunked: false,
        },
        body: bytes.empty(),
        cursor: 0,
        chunk_bytes: 1,
    }
}

fn main() -> Int effects { io, net } capabilities { io, net } {
    let listener = match listen("127.0.0.1:0") {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(value) => value,
        Err(_) => "",
    };
    let client = match tcp_connect(addr, 1000) {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let server = match accept(listener, 1000) {
        Ok(handle) => handle,
        Err(_) => 0,
    };

    let request_wire = "POST /chunked HTTP/1.1\r\nHost: localhost\r\nContent-Length: 11\r\n\r\nhello world";
    let sent = match tcp_send(client, bytes.from_string(request_wire)) {
        Ok(value) => value,
        Err(_) => 0,
    };

    let reader = match read_request_stream(server, 4096, 1000, 5) {
        Ok(value) => value,
        Err(_) => empty_reader(),
    };
    let mut current = reader;
    let mut request_bytes = bytes.empty();
    let mut request_done = false;
    while !request_done {
        let step = match request_stream_read_next(current) {
            Ok(value) => value,
            Err(_) => RequestBodyStep {
                reader: empty_reader(),
                chunk: None(),
            },
        };
        current = step.reader;
        let chunk = match step.chunk {
            Some(value) => value,
            None => bytes.empty(),
        };
        let chunk_len = bytes.byte_len(chunk);
        if chunk_len > 0 {
            request_bytes = bytes.concat(request_bytes, chunk);
        } else {
            request_done = true;
        };
    };

    let mut headers0: Map[String, String] = map.new_map();
    headers0 = map.insert(headers0, "content-type", "text/plain; charset=utf-8");
    let writer0 = match begin_streaming_response(server, 200u16, headers0, 11, 1000) {
        Ok(value) => value,
        Err(_) => ResponseBodyWriter {
            conn: server,
            timeout_ms: 1000,
            expected_bytes: 11,
            sent_bytes: 0,
        },
    };
    let writer1 = match write_streaming_response_chunk(writer0, bytes.from_string("hello ")) {
        Ok(value) => value,
        Err(_) => writer0,
    };
    let writer2 = match write_streaming_response_chunk(writer1, bytes.from_string("world")) {
        Ok(value) => value,
        Err(_) => writer1,
    };
    let finished = match finish_streaming_response(writer2) {
        Ok(value) => value,
        Err(_) => 0,
    };

    let wire = match tcp_recv(client, 4096, 1000) {
        Ok(value) => value,
        Err(_) => bytes.empty(),
    };
    let wire_text = match bytes.to_string(wire) {
        Ok(value) => value,
        Err(_) => "",
    };
    let request_text = match bytes.to_string(request_bytes) {
        Ok(value) => value,
        Err(_) => "",
    };
    let request_ok = if len(request_text) == 11 && string.contains(request_text, "hello world") {
        1
    } else {
        0
    };
    let response_ok = if string.contains(wire_text, "HTTP/1.1 200 OK") &&
        string.contains(wire_text, "content-length: 11") &&
        string.contains(wire_text, "hello world") {
        1
    } else {
        0
    };

    let closed_client = match tcp_close(client) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_server = match close(server) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_listener = match close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if sent > 0 &&
        request_done &&
        request_ok == 1 &&
        finished == 11 &&
        response_ok == 1 &&
        closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_streaming_example_smoke() {
    let src =
        fs::read_to_string("examples/io/http_server_streaming_api.aic").expect("read example");
    let (code, stdout, stderr) = compile_and_run(&src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}
