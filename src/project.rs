use std::fs;
use std::path::Path;

pub fn init_project(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path)?;
    fs::create_dir_all(path.join("src"))?;
    fs::create_dir_all(path.join("std"))?;
    fs::create_dir_all(path.join("examples"))?;
    fs::create_dir_all(path.join("docs"))?;
    fs::create_dir_all(path.join("tests"))?;

    fs::write(
        path.join("aic.toml"),
        "[package]\nname = \"sample\"\nmain = \"src/main.aic\"\n",
    )?;

    fs::write(
        path.join("src/main.aic"),
        r#"module sample.main;

import std.io;

fn maybe_even(x: Int) -> Option[Int] {
    if x % 2 == 0 {
    Some(x)
} else {
    None()
}
}

fn main() -> Int effects { io } {
    let v = maybe_even(10);
    let out = match v {
    Some(n) => n,
    None => 0,
};
    print_int(out);
    0
}
"#,
    )?;

    fs::write(
        path.join("std/option.aic"),
        r#"module std.option;

enum Option[T] {
    None,
    Some(T),
}
"#,
    )?;

    fs::write(
        path.join("std/result.aic"),
        r#"module std.result;

enum Result[T, E] {
    Ok(T),
    Err(E),
}
"#,
    )?;

    fs::write(
        path.join("std/io.aic"),
        r#"module std.io;

fn print_int(x: Int) -> () effects { io } {
    ()
}

fn print_str(x: String) -> () effects { io } {
    ()
}

fn panic(message: String) -> () effects { io } {
    ()
}
"#,
    )?;

    fs::write(
        path.join("std/string.aic"),
        r#"module std.string;

fn len(s: String) -> Int {
    0
}

fn is_empty(s: String) -> Bool {
    len(s) == 0
}
"#,
    )?;

    fs::write(
        path.join("std/vec.aic"),
        r#"module std.vec;

struct Vec[T] {
    ptr: Int,
    len: Int,
    cap: Int,
}

fn vec_len[T](v: Vec[T]) -> Int {
    v.len
}

fn vec_cap[T](v: Vec[T]) -> Int {
    v.cap
}
"#,
    )?;

    fs::write(
        path.join("std/fs.aic"),
        r#"module std.fs;

import std.result;
import std.vec;

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

fn aic_fs_exists_intrinsic(path: String) -> Bool effects { fs } {
    false
}

fn aic_fs_read_text_intrinsic(path: String) -> Result[String, FsError] effects { fs } {
    let out: Result[String, FsError] = Ok("");
    out
}

fn aic_fs_write_text_intrinsic(path: String, content: String) -> Result[Bool, FsError] effects { fs } {
    let out: Result[Bool, FsError] = Ok(true);
    out
}

fn aic_fs_append_text_intrinsic(path: String, content: String) -> Result[Bool, FsError] effects { fs } {
    let out: Result[Bool, FsError] = Ok(true);
    out
}

fn aic_fs_copy_intrinsic(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs } {
    let out: Result[Bool, FsError] = Ok(true);
    out
}

fn aic_fs_move_intrinsic(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs } {
    let out: Result[Bool, FsError] = Ok(true);
    out
}

fn aic_fs_delete_intrinsic(path: String) -> Result[Bool, FsError] effects { fs } {
    let out: Result[Bool, FsError] = Ok(true);
    out
}

fn aic_fs_metadata_intrinsic(path: String) -> Result[FsMetadata, FsError] effects { fs } {
    let out: Result[FsMetadata, FsError] = Ok(FsMetadata {
        is_file: false,
        is_dir: false,
        size: 0,
    });
    out
}

fn aic_fs_walk_dir_intrinsic(path: String) -> Result[Vec[String], FsError] effects { fs } {
    let out: Result[Vec[String], FsError] = Ok(Vec {
        ptr: 0,
        len: 0,
        cap: 0,
    });
    out
}

fn aic_fs_temp_file_intrinsic(prefix: String) -> Result[String, FsError] effects { fs } {
    let out: Result[String, FsError] = Ok("");
    out
}

fn aic_fs_temp_dir_intrinsic(prefix: String) -> Result[String, FsError] effects { fs } {
    let out: Result[String, FsError] = Ok("");
    out
}

fn exists(path: String) -> Bool effects { fs } {
    aic_fs_exists_intrinsic(path)
}

fn read_text(path: String) -> Result[String, FsError] effects { fs } {
    aic_fs_read_text_intrinsic(path)
}

fn write_text(path: String, content: String) -> Result[Bool, FsError] effects { fs } {
    aic_fs_write_text_intrinsic(path, content)
}

fn append_text(path: String, content: String) -> Result[Bool, FsError] effects { fs } {
    aic_fs_append_text_intrinsic(path, content)
}

fn copy(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs } {
    aic_fs_copy_intrinsic(from_path, to_path)
}

fn move(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs } {
    aic_fs_move_intrinsic(from_path, to_path)
}

fn delete(path: String) -> Result[Bool, FsError] effects { fs } {
    aic_fs_delete_intrinsic(path)
}

fn metadata(path: String) -> Result[FsMetadata, FsError] effects { fs } {
    aic_fs_metadata_intrinsic(path)
}

fn walk_dir(path: String) -> Result[Vec[String], FsError] effects { fs } {
    aic_fs_walk_dir_intrinsic(path)
}

fn temp_file(prefix: String) -> Result[String, FsError] effects { fs } {
    aic_fs_temp_file_intrinsic(prefix)
}

fn temp_dir(prefix: String) -> Result[String, FsError] effects { fs } {
    aic_fs_temp_dir_intrinsic(prefix)
}
"#,
    )?;

    fs::write(
        path.join("std/env.aic"),
        r#"module std.env;

import std.result;

enum EnvError {
    NotFound,
    PermissionDenied,
    InvalidInput,
    Io,
}

fn aic_env_get_intrinsic(key: String) -> Result[String, EnvError] effects { env } {
    let out: Result[String, EnvError] = Ok("");
    out
}

fn aic_env_set_intrinsic(key: String, value: String) -> Result[Bool, EnvError] effects { env } {
    let out: Result[Bool, EnvError] = Ok(true);
    out
}

fn aic_env_remove_intrinsic(key: String) -> Result[Bool, EnvError] effects { env } {
    let out: Result[Bool, EnvError] = Ok(true);
    out
}

fn aic_env_cwd_intrinsic() -> Result[String, EnvError] effects { env, fs } {
    let out: Result[String, EnvError] = Ok("");
    out
}

fn aic_env_set_cwd_intrinsic(path: String) -> Result[Bool, EnvError] effects { env, fs } {
    let out: Result[Bool, EnvError] = Ok(true);
    out
}

fn get(key: String) -> Result[String, EnvError] effects { env } {
    aic_env_get_intrinsic(key)
}

fn set(key: String, value: String) -> Result[Bool, EnvError] effects { env } {
    aic_env_set_intrinsic(key, value)
}

fn remove(key: String) -> Result[Bool, EnvError] effects { env } {
    aic_env_remove_intrinsic(key)
}

fn cwd() -> Result[String, EnvError] effects { env, fs } {
    aic_env_cwd_intrinsic()
}

fn set_cwd(path: String) -> Result[Bool, EnvError] effects { env, fs } {
    aic_env_set_cwd_intrinsic(path)
}
"#,
    )?;

    fs::write(
        path.join("std/path.aic"),
        r#"module std.path;

fn aic_path_join_intrinsic(left: String, right: String) -> String {
    ""
}

fn aic_path_basename_intrinsic(path: String) -> String {
    ""
}

fn aic_path_dirname_intrinsic(path: String) -> String {
    ""
}

fn aic_path_extension_intrinsic(path: String) -> String {
    ""
}

fn aic_path_is_abs_intrinsic(path: String) -> Bool {
    false
}

fn join(left: String, right: String) -> String {
    aic_path_join_intrinsic(left, right)
}

fn basename(path: String) -> String {
    aic_path_basename_intrinsic(path)
}

fn dirname(path: String) -> String {
    aic_path_dirname_intrinsic(path)
}

fn extension(path: String) -> String {
    aic_path_extension_intrinsic(path)
}

fn is_abs(path: String) -> Bool {
    aic_path_is_abs_intrinsic(path)
}
"#,
    )?;

    fs::write(
        path.join("std/proc.aic"),
        r#"module std.proc;

import std.result;

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

fn aic_proc_spawn_intrinsic(command: String) -> Result[Int, ProcError] effects { proc, env } {
    let out: Result[Int, ProcError] = Ok(0);
    out
}

fn aic_proc_wait_intrinsic(handle: Int) -> Result[Int, ProcError] effects { proc } {
    let out: Result[Int, ProcError] = Ok(0);
    out
}

fn aic_proc_kill_intrinsic(handle: Int) -> Result[Bool, ProcError] effects { proc } {
    let out: Result[Bool, ProcError] = Ok(true);
    out
}

fn aic_proc_run_intrinsic(command: String) -> Result[ProcOutput, ProcError] effects { proc, env } {
    let out: Result[ProcOutput, ProcError] = Ok(ProcOutput {
        status: 0,
        stdout: "",
        stderr: "",
    });
    out
}

fn aic_proc_pipe_intrinsic(left: String, right: String) -> Result[ProcOutput, ProcError] effects { proc, env } {
    let out: Result[ProcOutput, ProcError] = Ok(ProcOutput {
        status: 0,
        stdout: "",
        stderr: "",
    });
    out
}

fn spawn(command: String) -> Result[Int, ProcError] effects { proc, env } {
    aic_proc_spawn_intrinsic(command)
}

fn wait(handle: Int) -> Result[Int, ProcError] effects { proc } {
    aic_proc_wait_intrinsic(handle)
}

fn kill(handle: Int) -> Result[Bool, ProcError] effects { proc } {
    aic_proc_kill_intrinsic(handle)
}

fn run(command: String) -> Result[ProcOutput, ProcError] effects { proc, env } {
    aic_proc_run_intrinsic(command)
}

fn pipe(left: String, right: String) -> Result[ProcOutput, ProcError] effects { proc, env } {
    aic_proc_pipe_intrinsic(left, right)
}
"#,
    )?;

    fs::write(
        path.join("std/net.aic"),
        r#"module std.net;

import std.result;

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

fn aic_net_tcp_listen_intrinsic(addr: String) -> Result[Int, NetError] effects { net } {
    let out: Result[Int, NetError] = Ok(0);
    out
}

fn aic_net_tcp_local_addr_intrinsic(handle: Int) -> Result[String, NetError] effects { net } {
    let out: Result[String, NetError] = Ok("");
    out
}

fn aic_net_tcp_accept_intrinsic(listener: Int, timeout_ms: Int) -> Result[Int, NetError] effects { net } {
    let out: Result[Int, NetError] = Ok(0);
    out
}

fn aic_net_tcp_connect_intrinsic(addr: String, timeout_ms: Int) -> Result[Int, NetError] effects { net } {
    let out: Result[Int, NetError] = Ok(0);
    out
}

fn aic_net_tcp_send_intrinsic(handle: Int, payload: String) -> Result[Int, NetError] effects { net } {
    let out: Result[Int, NetError] = Ok(0);
    out
}

fn aic_net_tcp_recv_intrinsic(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[String, NetError] effects { net } {
    let out: Result[String, NetError] = Ok("");
    out
}

fn aic_net_tcp_close_intrinsic(handle: Int) -> Result[Bool, NetError] effects { net } {
    let out: Result[Bool, NetError] = Ok(true);
    out
}

fn aic_net_udp_bind_intrinsic(addr: String) -> Result[Int, NetError] effects { net } {
    let out: Result[Int, NetError] = Ok(0);
    out
}

fn aic_net_udp_local_addr_intrinsic(handle: Int) -> Result[String, NetError] effects { net } {
    let out: Result[String, NetError] = Ok("");
    out
}

fn aic_net_udp_send_to_intrinsic(handle: Int, addr: String, payload: String) -> Result[Int, NetError] effects { net } {
    let out: Result[Int, NetError] = Ok(0);
    out
}

fn aic_net_udp_recv_from_intrinsic(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[UdpPacket, NetError] effects { net } {
    let out: Result[UdpPacket, NetError] = Ok(UdpPacket {
        from: "",
        payload: "",
    });
    out
}

fn aic_net_udp_close_intrinsic(handle: Int) -> Result[Bool, NetError] effects { net } {
    let out: Result[Bool, NetError] = Ok(true);
    out
}

fn aic_net_dns_lookup_intrinsic(host: String) -> Result[String, NetError] effects { net } {
    let out: Result[String, NetError] = Ok("");
    out
}

fn aic_net_dns_reverse_intrinsic(addr: String) -> Result[String, NetError] effects { net } {
    let out: Result[String, NetError] = Ok("");
    out
}

fn tcp_listen(addr: String) -> Result[Int, NetError] effects { net } {
    aic_net_tcp_listen_intrinsic(addr)
}

fn tcp_local_addr(handle: Int) -> Result[String, NetError] effects { net } {
    aic_net_tcp_local_addr_intrinsic(handle)
}

fn tcp_accept(listener: Int, timeout_ms: Int) -> Result[Int, NetError] effects { net } {
    aic_net_tcp_accept_intrinsic(listener, timeout_ms)
}

fn tcp_connect(addr: String, timeout_ms: Int) -> Result[Int, NetError] effects { net } {
    aic_net_tcp_connect_intrinsic(addr, timeout_ms)
}

fn tcp_send(handle: Int, payload: String) -> Result[Int, NetError] effects { net } {
    aic_net_tcp_send_intrinsic(handle, payload)
}

fn tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[String, NetError] effects { net } {
    aic_net_tcp_recv_intrinsic(handle, max_bytes, timeout_ms)
}

fn tcp_close(handle: Int) -> Result[Bool, NetError] effects { net } {
    aic_net_tcp_close_intrinsic(handle)
}

fn udp_bind(addr: String) -> Result[Int, NetError] effects { net } {
    aic_net_udp_bind_intrinsic(addr)
}

fn udp_local_addr(handle: Int) -> Result[String, NetError] effects { net } {
    aic_net_udp_local_addr_intrinsic(handle)
}

fn udp_send_to(handle: Int, addr: String, payload: String) -> Result[Int, NetError] effects { net } {
    aic_net_udp_send_to_intrinsic(handle, addr, payload)
}

fn udp_recv_from(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[UdpPacket, NetError] effects { net } {
    aic_net_udp_recv_from_intrinsic(handle, max_bytes, timeout_ms)
}

fn udp_close(handle: Int) -> Result[Bool, NetError] effects { net } {
    aic_net_udp_close_intrinsic(handle)
}

fn dns_lookup(host: String) -> Result[String, NetError] effects { net } {
    aic_net_dns_lookup_intrinsic(host)
}

fn dns_reverse(addr: String) -> Result[String, NetError] effects { net } {
    aic_net_dns_reverse_intrinsic(addr)
}
"#,
    )?;

    fs::write(
        path.join("std/time.aic"),
        r#"module std.time;

fn aic_time_now_ms_intrinsic() -> Int effects { time } {
    0
}

fn aic_time_monotonic_ms_intrinsic() -> Int effects { time } {
    0
}

fn aic_time_sleep_ms_intrinsic(ms: Int) -> () effects { time } {
    ()
}

fn now_ms() -> Int effects { time } {
    aic_time_now_ms_intrinsic()
}

fn now() -> Int effects { time } {
    now_ms()
}

fn monotonic_ms() -> Int effects { time } {
    aic_time_monotonic_ms_intrinsic()
}

fn sleep_ms(ms: Int) -> () effects { time } {
    aic_time_sleep_ms_intrinsic(ms)
}

fn deadline_after_ms(timeout_ms: Int) -> Int effects { time } {
    let base = monotonic_ms();
    if timeout_ms <= 0 {
        base
    } else {
        base + timeout_ms
    }
}

fn remaining_ms(deadline_ms: Int) -> Int effects { time } {
    let now = monotonic_ms();
    if deadline_ms <= now {
        0
    } else {
        deadline_ms - now
    }
}

fn timeout_expired(deadline_ms: Int) -> Bool effects { time } {
    monotonic_ms() >= deadline_ms
}

fn sleep_until(deadline_ms: Int) -> () effects { time } {
    sleep_ms(remaining_ms(deadline_ms));
    ()
}
"#,
    )?;

    fs::write(
        path.join("std/rand.aic"),
        r#"module std.rand;

fn aic_rand_seed_intrinsic(seed_value: Int) -> () effects { rand } {
    ()
}

fn aic_rand_int_intrinsic() -> Int effects { rand } {
    0
}

fn aic_rand_range_intrinsic(min_inclusive: Int, max_exclusive: Int) -> Int effects { rand } {
    min_inclusive
}

fn seed(seed_value: Int) -> () effects { rand } {
    aic_rand_seed_intrinsic(seed_value)
}

fn random_int() -> Int effects { rand } {
    aic_rand_int_intrinsic()
}

fn random_bool() -> Bool effects { rand } {
    random_int() % 2 != 0
}

fn random_range(min_inclusive: Int, max_exclusive: Int) -> Int effects { rand } {
    aic_rand_range_intrinsic(min_inclusive, max_exclusive)
}
"#,
    )?;

    fs::write(
        path.join("std/json.aic"),
        r#"module std.json;

import std.result;

enum JsonError {
    InvalidJson,
    InvalidType,
    MissingField,
    InvalidNumber,
    InvalidString,
    InvalidInput,
    Internal,
}

enum JsonKind {
    NullValue,
    BoolValue,
    NumberValue,
    StringValue,
    ArrayValue,
    ObjectValue,
}

struct JsonValue {
    raw: String,
    kind: JsonKind,
}

fn aic_json_parse_intrinsic(text: String) -> Result[JsonValue, JsonError] {
    let out: Result[JsonValue, JsonError] = Ok(JsonValue {
        raw: text,
        kind: StringValue(),
    });
    out
}

fn aic_json_stringify_intrinsic(value: JsonValue) -> Result[String, JsonError] {
    let out: Result[String, JsonError] = Ok(value.raw);
    out
}

fn aic_json_encode_int_intrinsic(value: Int) -> JsonValue {
    JsonValue {
        raw: "0",
        kind: NumberValue(),
    }
}

fn aic_json_encode_bool_intrinsic(value: Bool) -> JsonValue {
    JsonValue {
        raw: "false",
        kind: BoolValue(),
    }
}

fn aic_json_encode_string_intrinsic(value: String) -> JsonValue {
    JsonValue {
        raw: "\"\"",
        kind: StringValue(),
    }
}

fn aic_json_encode_null_intrinsic() -> JsonValue {
    JsonValue {
        raw: "",
        kind: NullValue(),
    }
}

fn aic_json_decode_int_intrinsic(value: JsonValue) -> Result[Int, JsonError] {
    let out: Result[Int, JsonError] = Ok(0);
    out
}

fn aic_json_decode_bool_intrinsic(value: JsonValue) -> Result[Bool, JsonError] {
    let out: Result[Bool, JsonError] = Ok(false);
    out
}

fn aic_json_decode_string_intrinsic(value: JsonValue) -> Result[String, JsonError] {
    let out: Result[String, JsonError] = Ok("");
    out
}

fn aic_json_object_empty_intrinsic() -> JsonValue {
    JsonValue {
        raw: "{}",
        kind: ObjectValue(),
    }
}

fn aic_json_object_set_intrinsic(object: JsonValue, key: String, value: JsonValue) -> Result[JsonValue, JsonError] {
    let out: Result[JsonValue, JsonError] = Ok(object);
    out
}

fn aic_json_object_get_intrinsic(object: JsonValue, key: String) -> Result[Option[JsonValue], JsonError] {
    let out: Result[Option[JsonValue], JsonError] = Ok(None());
    out
}

fn aic_json_kind_intrinsic(value: JsonValue) -> JsonKind {
    value.kind
}

fn aic_json_serde_encode_intrinsic[T](value: T) -> Result[JsonValue, JsonError] {
    let out: Result[JsonValue, JsonError] = Ok(encode_null());
    out
}

fn aic_json_serde_decode_intrinsic[T](value: JsonValue, marker: Option[T]) -> Result[T, JsonError] {
    let out: Result[T, JsonError] = Err(InvalidType());
    out
}

fn aic_json_serde_schema_intrinsic[T](marker: Option[T]) -> Result[String, JsonError] {
    let out: Result[String, JsonError] = Ok("");
    out
}

fn parse(text: String) -> Result[JsonValue, JsonError] {
    aic_json_parse_intrinsic(text)
}

fn stringify(value: JsonValue) -> Result[String, JsonError] {
    aic_json_stringify_intrinsic(value)
}

fn encode_int(value: Int) -> JsonValue {
    aic_json_encode_int_intrinsic(value)
}

fn encode_bool(value: Bool) -> JsonValue {
    aic_json_encode_bool_intrinsic(value)
}

fn encode_string(value: String) -> JsonValue {
    aic_json_encode_string_intrinsic(value)
}

fn encode_null() -> JsonValue {
    aic_json_encode_null_intrinsic()
}

fn decode_int(value: JsonValue) -> Result[Int, JsonError] {
    aic_json_decode_int_intrinsic(value)
}

fn decode_bool(value: JsonValue) -> Result[Bool, JsonError] {
    aic_json_decode_bool_intrinsic(value)
}

fn decode_string(value: JsonValue) -> Result[String, JsonError] {
    aic_json_decode_string_intrinsic(value)
}

fn object_empty() -> JsonValue {
    aic_json_object_empty_intrinsic()
}

fn object_set(object: JsonValue, key: String, value: JsonValue) -> Result[JsonValue, JsonError] {
    aic_json_object_set_intrinsic(object, key, value)
}

fn object_get(object: JsonValue, key: String) -> Result[Option[JsonValue], JsonError] {
    aic_json_object_get_intrinsic(object, key)
}

fn kind(value: JsonValue) -> JsonKind {
    aic_json_kind_intrinsic(value)
}

fn encode[T](value: T) -> Result[JsonValue, JsonError] {
    aic_json_serde_encode_intrinsic(value)
}

fn decode_with[T](value: JsonValue, marker: Option[T]) -> Result[T, JsonError] {
    aic_json_serde_decode_intrinsic(value, marker)
}

fn schema[T](marker: Option[T]) -> Result[String, JsonError] {
    aic_json_serde_schema_intrinsic(marker)
}

fn is_null(value: JsonValue) -> Bool {
    match kind(value) {
        NullValue => true,
        _ => false,
    }
}
"#,
    )?;

    fs::write(
        path.join("std/url.aic"),
        r#"module std.url;

import std.result;

enum UrlError {
    InvalidUrl,
    InvalidScheme,
    InvalidHost,
    InvalidPort,
    InvalidPath,
    InvalidInput,
    Internal,
}

struct Url {
    scheme: String,
    host: String,
    port: Int,
    path: String,
    query: String,
    fragment: String,
}

fn aic_url_parse_intrinsic(text: String) -> Result[Url, UrlError] {
    let out: Result[Url, UrlError] = Err(InvalidUrl());
    out
}

fn aic_url_normalize_intrinsic(url: Url) -> Result[String, UrlError] {
    let out: Result[String, UrlError] = Ok("");
    out
}

fn aic_url_net_addr_intrinsic(url: Url) -> Result[String, UrlError] {
    let out: Result[String, UrlError] = Ok("");
    out
}

fn parse(text: String) -> Result[Url, UrlError] {
    aic_url_parse_intrinsic(text)
}

fn normalize(url: Url) -> Result[String, UrlError] {
    aic_url_normalize_intrinsic(url)
}

fn net_addr(url: Url) -> Result[String, UrlError] {
    aic_url_net_addr_intrinsic(url)
}

fn has_explicit_port(url: Url) -> Bool {
    url.port >= 0
}
"#,
    )?;

    fs::write(
        path.join("std/http.aic"),
        r#"module std.http;

import std.result;
import std.vec;

enum HttpError {
    InvalidMethod,
    InvalidStatus,
    InvalidHeaderName,
    InvalidHeaderValue,
    InvalidTarget,
    InvalidInput,
    Internal,
}

enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

struct HttpHeader {
    name: String,
    value: String,
}

struct HttpRequest {
    method: HttpMethod,
    target: String,
    headers: Vec[HttpHeader],
    body: String,
}

struct HttpResponse {
    status: Int,
    reason: String,
    headers: Vec[HttpHeader],
    body: String,
}

fn aic_http_parse_method_intrinsic(text: String) -> Result[HttpMethod, HttpError] {
    let out: Result[HttpMethod, HttpError] = Err(InvalidMethod());
    out
}

fn aic_http_method_name_intrinsic(method: HttpMethod) -> Result[String, HttpError] {
    let out: Result[String, HttpError] = Ok("");
    out
}

fn aic_http_status_reason_intrinsic(status: Int) -> Result[String, HttpError] {
    let out: Result[String, HttpError] = Ok("");
    out
}

fn aic_http_validate_header_intrinsic(name: String, value: String) -> Result[Bool, HttpError] {
    let out: Result[Bool, HttpError] = Ok(true);
    out
}

fn aic_http_validate_target_intrinsic(target: String) -> Result[Bool, HttpError] {
    let out: Result[Bool, HttpError] = Ok(true);
    out
}

fn aic_http_header_intrinsic(name: String, value: String) -> Result[HttpHeader, HttpError] {
    let out: Result[HttpHeader, HttpError] = Err(InvalidHeaderName());
    out
}

fn aic_http_request_intrinsic(method: HttpMethod, target: String, headers: Vec[HttpHeader], body: String) -> Result[HttpRequest, HttpError] {
    let out: Result[HttpRequest, HttpError] = Err(InvalidTarget());
    out
}

fn aic_http_response_intrinsic(status: Int, headers: Vec[HttpHeader], body: String) -> Result[HttpResponse, HttpError] {
    let out: Result[HttpResponse, HttpError] = Err(InvalidStatus());
    out
}

fn parse_method(text: String) -> Result[HttpMethod, HttpError] {
    aic_http_parse_method_intrinsic(text)
}

fn method_name(method: HttpMethod) -> Result[String, HttpError] {
    aic_http_method_name_intrinsic(method)
}

fn status_reason(status: Int) -> Result[String, HttpError] {
    aic_http_status_reason_intrinsic(status)
}

fn validate_header(name: String, value: String) -> Result[Bool, HttpError] {
    aic_http_validate_header_intrinsic(name, value)
}

fn validate_target(target: String) -> Result[Bool, HttpError] {
    aic_http_validate_target_intrinsic(target)
}

fn header(name: String, value: String) -> Result[HttpHeader, HttpError] {
    aic_http_header_intrinsic(name, value)
}

fn request(method: HttpMethod, target: String, headers: Vec[HttpHeader], body: String) -> Result[HttpRequest, HttpError] {
    aic_http_request_intrinsic(method, target, headers, body)
}

fn response(status: Int, headers: Vec[HttpHeader], body: String) -> Result[HttpResponse, HttpError] {
    aic_http_response_intrinsic(status, headers, body)
}
"#,
    )?;

    fs::write(
        path.join("std/regex.aic"),
        r#"module std.regex;

import std.result;

enum RegexError {
    InvalidPattern,
    InvalidInput,
    NoMatch,
    UnsupportedFeature,
    TooComplex,
    Internal,
}

struct Regex {
    pattern: String,
    flags: Int,
}

fn aic_regex_compile_intrinsic(pattern: String, flags: Int) -> Result[Regex, RegexError] {
    let out: Result[Regex, RegexError] = Ok(Regex {
        pattern: pattern,
        flags: flags,
    });
    out
}

fn aic_regex_is_match_intrinsic(regex: Regex, text: String) -> Result[Bool, RegexError] {
    let out: Result[Bool, RegexError] = Ok(false);
    out
}

fn aic_regex_find_intrinsic(regex: Regex, text: String) -> Result[String, RegexError] {
    let out: Result[String, RegexError] = Ok("");
    out
}

fn aic_regex_replace_intrinsic(regex: Regex, text: String, replacement: String) -> Result[String, RegexError] {
    let out: Result[String, RegexError] = Ok(text);
    out
}

fn no_flags() -> Int {
    0
}

fn flag_case_insensitive() -> Int {
    1
}

fn flag_multiline() -> Int {
    2
}

fn flag_dot_matches_newline() -> Int {
    4
}

fn compile(pattern: String) -> Result[Regex, RegexError] {
    compile_with_flags(pattern, no_flags())
}

fn compile_with_flags(pattern: String, flags: Int) -> Result[Regex, RegexError] {
    aic_regex_compile_intrinsic(pattern, flags)
}

fn is_match(regex: Regex, text: String) -> Result[Bool, RegexError] {
    aic_regex_is_match_intrinsic(regex, text)
}

fn find(regex: Regex, text: String) -> Result[String, RegexError] {
    aic_regex_find_intrinsic(regex, text)
}

fn replace(regex: Regex, text: String, replacement: String) -> Result[String, RegexError] {
    aic_regex_replace_intrinsic(regex, text, replacement)
}
"#,
    )?;

    fs::write(
        path.join("std/concurrent.aic"),
        r#"module std.concurrent;

import std.result;

enum ConcurrencyError {
    NotFound,
    Timeout,
    Cancelled,
    InvalidInput,
    Panic,
    Closed,
    Io,
}

struct Task {
    handle: Int,
}

struct IntChannel {
    handle: Int,
}

struct IntMutex {
    handle: Int,
}

fn aic_conc_spawn_intrinsic(value: Int, delay_ms: Int) -> Result[Task, ConcurrencyError] effects { concurrency } {
    let out: Result[Task, ConcurrencyError] = Ok(Task { handle: 0 });
    out
}

fn aic_conc_join_intrinsic(task: Task) -> Result[Int, ConcurrencyError] effects { concurrency } {
    let out: Result[Int, ConcurrencyError] = Ok(0);
    out
}

fn aic_conc_cancel_intrinsic(task: Task) -> Result[Bool, ConcurrencyError] effects { concurrency } {
    let out: Result[Bool, ConcurrencyError] = Ok(true);
    out
}

fn aic_conc_channel_int_intrinsic(capacity: Int) -> Result[IntChannel, ConcurrencyError] effects { concurrency } {
    let out: Result[IntChannel, ConcurrencyError] = Ok(IntChannel { handle: 0 });
    out
}

fn aic_conc_send_int_intrinsic(ch: IntChannel, value: Int, timeout_ms: Int) -> Result[Bool, ConcurrencyError] effects { concurrency } {
    let out: Result[Bool, ConcurrencyError] = Ok(true);
    out
}

fn aic_conc_recv_int_intrinsic(ch: IntChannel, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency } {
    let out: Result[Int, ConcurrencyError] = Ok(0);
    out
}

fn aic_conc_close_channel_intrinsic(ch: IntChannel) -> Result[Bool, ConcurrencyError] effects { concurrency } {
    let out: Result[Bool, ConcurrencyError] = Ok(true);
    out
}

fn aic_conc_mutex_int_intrinsic(initial: Int) -> Result[IntMutex, ConcurrencyError] effects { concurrency } {
    let out: Result[IntMutex, ConcurrencyError] = Ok(IntMutex { handle: 0 });
    out
}

fn aic_conc_mutex_lock_intrinsic(mutex: IntMutex, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency } {
    let out: Result[Int, ConcurrencyError] = Ok(0);
    out
}

fn aic_conc_mutex_unlock_intrinsic(mutex: IntMutex, value: Int) -> Result[Bool, ConcurrencyError] effects { concurrency } {
    let out: Result[Bool, ConcurrencyError] = Ok(true);
    out
}

fn aic_conc_mutex_close_intrinsic(mutex: IntMutex) -> Result[Bool, ConcurrencyError] effects { concurrency } {
    let out: Result[Bool, ConcurrencyError] = Ok(true);
    out
}

fn spawn_task(value: Int, delay_ms: Int) -> Result[Task, ConcurrencyError] effects { concurrency } {
    aic_conc_spawn_intrinsic(value, delay_ms)
}

fn join_task(task: Task) -> Result[Int, ConcurrencyError] effects { concurrency } {
    aic_conc_join_intrinsic(task)
}

fn cancel_task(task: Task) -> Result[Bool, ConcurrencyError] effects { concurrency } {
    aic_conc_cancel_intrinsic(task)
}

fn channel_int(capacity: Int) -> Result[IntChannel, ConcurrencyError] effects { concurrency } {
    aic_conc_channel_int_intrinsic(capacity)
}

fn send_int(ch: IntChannel, value: Int, timeout_ms: Int) -> Result[Bool, ConcurrencyError] effects { concurrency } {
    aic_conc_send_int_intrinsic(ch, value, timeout_ms)
}

fn recv_int(ch: IntChannel, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency } {
    aic_conc_recv_int_intrinsic(ch, timeout_ms)
}

fn close_channel(ch: IntChannel) -> Result[Bool, ConcurrencyError] effects { concurrency } {
    aic_conc_close_channel_intrinsic(ch)
}

fn mutex_int(initial: Int) -> Result[IntMutex, ConcurrencyError] effects { concurrency } {
    aic_conc_mutex_int_intrinsic(initial)
}

fn lock_int(mutex: IntMutex, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency } {
    aic_conc_mutex_lock_intrinsic(mutex, timeout_ms)
}

fn unlock_int(mutex: IntMutex, value: Int) -> Result[Bool, ConcurrencyError] effects { concurrency } {
    aic_conc_mutex_unlock_intrinsic(mutex, value)
}

fn close_mutex(mutex: IntMutex) -> Result[Bool, ConcurrencyError] effects { concurrency } {
    aic_conc_mutex_close_intrinsic(mutex)
}
"#,
    )?;

    Ok(())
}
