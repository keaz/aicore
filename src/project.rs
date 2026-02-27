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

fn main() -> Int effects { io } capabilities { io } {
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

fn print_float(x: Float) -> () effects { io } {
    ()
}

fn panic(message: String) -> () effects { io } {
    ()
}
"#,
    )?;

    fs::write(
        path.join("std/string.aic"),
        include_str!("../std/string.aic"),
    )?;

    fs::write(path.join("std/char.aic"), include_str!("../std/char.aic"))?;

    fs::write(path.join("std/bytes.aic"), include_str!("../std/bytes.aic"))?;

    fs::write(
        path.join("std/buffer.aic"),
        include_str!("../std/buffer.aic"),
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

    fs::write(path.join("std/deque.aic"), include_str!("../std/deque.aic"))?;

    fs::write(path.join("std/fs.aic"), include_str!("../std/fs.aic"))?;

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

    fs::write(path.join("std/net.aic"), include_str!("../std/net.aic"))?;

    fs::write(
        path.join("std/crypto.aic"),
        include_str!("../std/crypto.aic"),
    )?;

    fs::write(path.join("std/time.aic"), include_str!("../std/time.aic"))?;

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
        path.join("std/retry.aic"),
        r#"module std.retry;

import std.rand;
import std.result;
import std.time;

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

fn default_retry_config() -> RetryConfig {
    RetryConfig {
        max_attempts: 4,
        initial_backoff_ms: 25,
        backoff_multiplier: 2,
        max_backoff_ms: 1000,
        jitter_enabled: true,
        jitter_ms: 10,
    }
}

fn normalize_non_negative(value: Int) -> Int {
    if value < 0 {
        0
    } else {
        value
    }
}

fn normalize_attempts(value: Int) -> Int {
    if value <= 0 {
        1
    } else {
        value
    }
}

fn normalize_multiplier(value: Int) -> Int {
    if value < 1 {
        1
    } else {
        value
    }
}

fn cap_delay(delay_ms: Int, max_backoff_ms: Int) -> Int {
    let bounded_delay = normalize_non_negative(delay_ms);
    let bounded_cap = normalize_non_negative(max_backoff_ms);
    if bounded_delay > bounded_cap {
        bounded_cap
    } else {
        bounded_delay
    }
}

fn normalize_config(config: RetryConfig) -> RetryConfig {
    let bounded_cap = normalize_non_negative(config.max_backoff_ms);
    let bounded_initial = cap_delay(config.initial_backoff_ms, bounded_cap);
    RetryConfig {
        max_attempts: normalize_attempts(config.max_attempts),
        initial_backoff_ms: bounded_initial,
        backoff_multiplier: normalize_multiplier(config.backoff_multiplier),
        max_backoff_ms: bounded_cap,
        jitter_enabled: config.jitter_enabled,
        jitter_ms: normalize_non_negative(config.jitter_ms),
    }
}

fn next_backoff_ms(current_ms: Int, multiplier: Int, max_backoff_ms: Int) -> Int {
    let current = cap_delay(current_ms, max_backoff_ms);
    let bounded_cap = normalize_non_negative(max_backoff_ms);
    let bounded_multiplier = normalize_multiplier(multiplier);
    if current == 0 || bounded_cap == 0 {
        0
    } else if current >= bounded_cap {
        bounded_cap
    } else if bounded_multiplier == 1 {
        current
    } else {
        let threshold = bounded_cap / bounded_multiplier;
        if current > threshold {
            bounded_cap
        } else {
            current * bounded_multiplier
        }
    }
}

fn delay_with_optional_jitter(delay_ms: Int, config: RetryConfig) -> Int effects { rand } {
    let base = cap_delay(delay_ms, config.max_backoff_ms);
    if !config.jitter_enabled || config.jitter_ms <= 0 {
        base
    } else {
        let jitter = random_range(0, config.jitter_ms + 1);
        cap_delay(base + jitter, config.max_backoff_ms)
    }
}

fn retry[T](config: RetryConfig, operation: Fn() -> Result[T, String]) -> RetryResult[T] effects { time, rand } {
    let normalized = normalize_config(config);
    let started = monotonic_ms();
    let mut attempts = 0;
    let mut delay_ms = normalized.initial_backoff_ms;
    let mut last_error = "retry failed";
    let mut completed = false;
    let mut out_result: Result[T, String] = Err("retry failed");
    let mut out_elapsed = 0;

    while attempts < normalized.max_attempts && !completed {
        attempts = attempts + 1;
        let attempt_result: Result[T, String] = match operation() {
            Ok(value) => Ok(value),
            Err(err) => Err(err),
        };
        let step = RetryResult {
            result: attempt_result,
            attempts: attempts,
            elapsed_ms: monotonic_ms() - started,
        };
        let succeeded = match step.result {
            Ok(_) => true,
            Err(_) => false,
        };
        if succeeded {
            out_result = step.result;
            out_elapsed = step.elapsed_ms;
            completed = true;
        } else {
            last_error = match step.result {
                Ok(_) => "retry failed",
                Err(err) => err,
            };
            if attempts < normalized.max_attempts {
                let wait_ms = delay_with_optional_jitter(delay_ms, normalized);
                if wait_ms > 0 {
                    sleep_ms(wait_ms);
                } else {
                    ()
                };
                delay_ms = next_backoff_ms(
                    delay_ms,
                    normalized.backoff_multiplier,
                    normalized.max_backoff_ms,
                );
            } else {
                ()
            }
        };
    };

    if completed {
        RetryResult {
            result: out_result,
            attempts: attempts,
            elapsed_ms: out_elapsed,
        }
    } else {
        let failure: Result[T, String] = Err(last_error);
        RetryResult {
            result: failure,
            attempts: attempts,
            elapsed_ms: monotonic_ms() - started,
        }
    }
}

fn with_timeout[T](timeout_ms: Int, operation: Fn() -> T) -> Result[T, String] effects { time } {
    if timeout_ms <= 0 {
        Err("deadline exceeded")
    } else {
        let deadline = deadline_after_ms(timeout_ms);
        if timeout_expired(deadline) {
            Err("deadline exceeded")
        } else {
            let value = operation();
            if timeout_expired(deadline) {
                Err("deadline exceeded")
            } else {
                Ok(value)
            }
        }
    }
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

fn aic_json_encode_float_intrinsic(value: Float) -> JsonValue {
    JsonValue {
        raw: "0.0",
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

fn aic_json_decode_float_intrinsic(value: JsonValue) -> Result[Float, JsonError] {
    let out: Result[Float, JsonError] = Ok(0.0);
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

fn encode_float(value: Float) -> JsonValue {
    aic_json_encode_float_intrinsic(value)
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

fn decode_float(value: JsonValue) -> Result[Float, JsonError] {
    aic_json_decode_float_intrinsic(value)
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
        path.join("std/http_server.aic"),
        r#"module std.http_server;

import std.map;
import std.result;

enum ServerError {
    InvalidRequest,
    InvalidMethod,
    InvalidHeader,
    InvalidTarget,
    Timeout,
    ConnectionClosed,
    BodyTooLarge,
    Net,
    Internal,
}

struct Request {
    method: String,
    path: String,
    query: Map[String, String],
    headers: Map[String, String],
    body: String,
}

struct Response {
    status: Int,
    headers: Map[String, String],
    body: String,
}

fn aic_http_server_listen_intrinsic(addr: String) -> Result[Int, ServerError] effects { net } {
    let _addr = addr;
    let out: Result[Int, ServerError] = Err(Net());
    out
}

fn aic_http_server_accept_intrinsic(listener: Int, timeout_ms: Int) -> Result[Int, ServerError] effects { net } {
    let _listener = listener;
    let _timeout_ms = timeout_ms;
    let out: Result[Int, ServerError] = Err(Net());
    out
}

fn aic_http_server_read_request_intrinsic(conn: Int, max_bytes: Int, timeout_ms: Int) -> Result[Request, ServerError] effects { net } {
    let _conn = conn;
    let _max_bytes = max_bytes;
    let _timeout_ms = timeout_ms;
    let out: Result[Request, ServerError] = Err(InvalidRequest());
    out
}

fn aic_http_server_write_response_intrinsic(conn: Int, response: Response) -> Result[Int, ServerError] effects { net } {
    let _conn = conn;
    let _response = response;
    let out: Result[Int, ServerError] = Err(Net());
    out
}

fn aic_http_server_close_intrinsic(handle: Int) -> Result[Bool, ServerError] effects { net } {
    let _handle = handle;
    let out: Result[Bool, ServerError] = Err(Net());
    out
}

fn aic_http_server_text_response_intrinsic(status: Int, body: String) -> Response {
    let headers0: Map[String, String] = aic_map_new_intrinsic();
    let headers = aic_map_insert_intrinsic(headers0, "content-type", "text/plain; charset=utf-8");
    Response {
        status: status,
        headers: headers,
        body: body,
    }
}

fn aic_http_server_json_response_intrinsic(status: Int, body: String) -> Response {
    let headers0: Map[String, String] = aic_map_new_intrinsic();
    let headers = aic_map_insert_intrinsic(headers0, "content-type", "application/json");
    Response {
        status: status,
        headers: headers,
        body: body,
    }
}

fn aic_http_server_header_intrinsic(resp: Response, name: String) -> Option[String] {
    aic_map_get_intrinsic(resp.headers, name)
}

fn listen(addr: String) -> Result[Int, ServerError] effects { net } {
    aic_http_server_listen_intrinsic(addr)
}

fn accept(listener: Int, timeout_ms: Int) -> Result[Int, ServerError] effects { net } {
    aic_http_server_accept_intrinsic(listener, timeout_ms)
}

fn read_request(conn: Int, max_bytes: Int, timeout_ms: Int) -> Result[Request, ServerError] effects { net } {
    aic_http_server_read_request_intrinsic(conn, max_bytes, timeout_ms)
}

fn write_response(conn: Int, response: Response) -> Result[Int, ServerError] effects { net } {
    aic_http_server_write_response_intrinsic(conn, response)
}

fn close(handle: Int) -> Result[Bool, ServerError] effects { net } {
    aic_http_server_close_intrinsic(handle)
}

fn response(status: Int, headers: Map[String, String], body: String) -> Response {
    Response {
        status: status,
        headers: headers,
        body: body,
    }
}

fn text_response(status: Int, body: String) -> Response {
    aic_http_server_text_response_intrinsic(status, body)
}

fn json_response(status: Int, body: String) -> Response {
    aic_http_server_json_response_intrinsic(status, body)
}

fn error_response(status: Int, message: String) -> Response {
    text_response(status, message)
}

fn header(resp: Response, name: String) -> Option[String] {
    aic_http_server_header_intrinsic(resp, name)
}
"#,
    )?;

    fs::write(
        path.join("std/router.aic"),
        r#"module std.router;

import std.map;
import std.result;

enum RouterError {
    InvalidPattern,
    InvalidMethod,
    Capacity,
    Internal,
}

struct Router {
    handle: Int,
}

struct RouteMatch {
    route_id: Int,
    params: Map[String, String],
}

fn aic_router_new_intrinsic() -> Result[Router, RouterError] {
    let out: Result[Router, RouterError] = Err(Capacity());
    out
}

fn aic_router_add_intrinsic(router: Router, method: String, pattern: String, route_id: Int) -> Result[Router, RouterError] {
    let _router = router;
    let _method = method;
    let _pattern = pattern;
    let _route_id = route_id;
    let out: Result[Router, RouterError] = Err(Capacity());
    out
}

fn aic_router_match_intrinsic(router: Router, method: String, path: String) -> Result[Option[RouteMatch], RouterError] {
    let _router = router;
    let _method = method;
    let _path = path;
    let out: Result[Option[RouteMatch], RouterError] = Err(Capacity());
    out
}

fn new_router() -> Result[Router, RouterError] {
    aic_router_new_intrinsic()
}

fn add(router: Router, method: String, pattern: String, route_id: Int) -> Result[Router, RouterError] {
    aic_router_add_intrinsic(router, method, pattern, route_id)
}

fn match_route(router: Router, method: String, path: String) -> Result[Option[RouteMatch], RouterError] {
    aic_router_match_intrinsic(router, method, path)
}
"#,
    )?;

    fs::write(
        path.join("std/regex.aic"),
        r#"module std.regex;

import std.option;
import std.result;
import std.string;
import std.vec;

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

struct RegexMatch {
    full: String,
    groups: Vec[String],
    start: Int,
    end: Int,
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

fn aic_regex_captures_intrinsic(regex: Regex, text: String) -> Result[Option[RegexMatch], RegexError] {
    let out: Result[Option[RegexMatch], RegexError] = Ok(None());
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

fn captures(regex: Regex, text: String) -> Result[Option[RegexMatch], RegexError] {
    aic_regex_captures_intrinsic(regex, text)
}

fn find_all_step(regex: Regex, text: String, cursor: Int, acc: Vec[RegexMatch], m: RegexMatch) -> Result[Vec[RegexMatch], RegexError] {
    let next_cursor = if m.end > m.start {
        cursor + m.end
    } else {
        cursor + 1
    };
    let next_acc = vec.push(acc, RegexMatch {
        full: m.full,
        groups: m.groups,
        start: m.start + cursor,
        end: m.end + cursor,
    });
    find_all_collect(regex, text, next_cursor, next_acc)
}

fn find_all_collect(regex: Regex, text: String, cursor: Int, acc: Vec[RegexMatch]) -> Result[Vec[RegexMatch], RegexError] {
    let text_len = string.len(text);
    if cursor > text_len {
        Ok(acc)
    } else {
        let window = string.substring(text, cursor, text_len);
        match captures(regex, window) {
            Ok(found) => match found {
                Some(m) => find_all_step(regex, text, cursor, acc, m),
                None => Ok(acc),
            },
            Err(err) => Err(err),
        }
    }
}

fn find_all(regex: Regex, text: String) -> Result[Vec[RegexMatch], RegexError] {
    let empty: Vec[RegexMatch] = vec.new_vec();
    find_all_collect(regex, text, 0, empty)
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
