# Regex Runtime (`std.regex`)

This document defines the `std.regex` API contract implemented for `[DT-T1] std regex engine integration`.

For the consolidated DT stack guide (regex/json/serde/url/http/datetime + cookbook), see:

- `docs/data-text/README.md`
- `docs/data-text/cookbook.md`

## Module surface

```aic
module std.regex;

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
```

Public APIs:

- `compile(pattern: String) -> Result[Regex, RegexError]`
- `compile_with_flags(pattern: String, flags: Int) -> Result[Regex, RegexError]`
- `is_match(regex: Regex, text: String) -> Result[Bool, RegexError]`
- `find(regex: Regex, text: String) -> Result[String, RegexError]`
- `replace(regex: Regex, text: String, replacement: String) -> Result[String, RegexError]`

## Flags

`flags` is a deterministic bitmask:

- `no_flags() -> Int`: `0`
- `flag_case_insensitive() -> Int`: `1`
- `flag_multiline() -> Int`: `2`
- `flag_dot_matches_newline() -> Int`: `4`

Behavior notes:

- Unknown bits produce `Err(InvalidInput)`.
- `flag_multiline() + flag_dot_matches_newline()` is currently unsupported and returns `Err(UnsupportedFeature)`.
- Matching uses POSIX ERE semantics in the runtime.

## Error model

- `InvalidPattern`: regex compile failure (`regcomp` style syntax errors).
- `InvalidInput`: invalid flags or invalid ABI string input.
- `NoMatch`: `find` has no match.
- `UnsupportedFeature`: unsupported flag combinations or unsupported host runtime.
- `TooComplex`: runtime allocation/engine complexity boundary.
- `Internal`: unexpected runtime failure.

## Runtime behavior

- `compile*` validates pattern + flags and returns a `Regex` value if successful.
- `is_match` returns `Ok(true|false)`; no-match is not an error.
- `find` returns the first matched substring or `Err(NoMatch)`.
- `replace` replaces only the first match; if no match, original text is returned.

## Example

See `/Users/kasunranasinghe/Projects/Rust/aicore/examples/data/log_parse_regex.aic`.

## Serde Derive Lite (`std.json`) - DT-T3

This section documents the derive-like ADT JSON support added for `[DT-T3] Serde derive lite for ADTs`.

### Public APIs

```aic
fn encode[T](value: T) -> Result[JsonValue, JsonError]
fn decode_with[T](value: JsonValue, marker: Option[T]) -> Result[T, JsonError]
fn schema[T](marker: Option[T]) -> Result[String, JsonError]
```

Notes:

- `decode_with` and `schema` use `Option[T]` markers to disambiguate target types in the current IR/codegen model.
- Marker value is typically `None()` with an explicit marker type annotation.

### Derived wire format

- Structs are encoded as JSON objects keyed by field names.
- Enums use deterministic indexed tagging:
  - object key `"tag"` stores variant index (`Int`)
  - object key `"value"` stores payload JSON (`null` for no-payload variants)

### Deterministic ordering contract

- Struct field schema entries are emitted in lexicographic field-name order.
- Enum tag assignment is declaration-order index (`0..n-1`).
- Runtime object writes remain deterministic (object keys canonically sorted by runtime object builder).
- Schema strings are deterministic across runs for identical sources.

### Versioning diagnostics

`decode_with` emits stable `JsonError` values for common wire/schema drift:

- `MissingField` when required struct fields or enum keys are absent.
- `InvalidType` for unknown enum tags or incompatible payload kinds.

### Example

See `/Users/kasunranasinghe/Projects/Rust/aicore/examples/data/serde_models.aic`.

## URL + HTTP Types (`std.url`, `std.http`) - DT-T4

This section documents `[DT-T4] URL and HTTP types and parsers`.

### URL module (`std.url`)

Core types:

- `UrlError`: `InvalidUrl`, `InvalidScheme`, `InvalidHost`, `InvalidPort`, `InvalidPath`, `InvalidInput`, `Internal`
- `Url`: `{ scheme, host, port, path, query, fragment }`

Public APIs:

- `parse(text: String) -> Result[Url, UrlError]`
- `normalize(url: Url) -> Result[String, UrlError]`
- `net_addr(url: Url) -> Result[String, UrlError]`
- `has_explicit_port(url: Url) -> Bool`

Behavior:

- `parse` validates an RFC-style subset: `scheme://authority/path?query#fragment`.
- Host and scheme normalization is deterministic (`normalize` lowercases both).
- Default ports are elided in normalized output (`http:80`, `https:443`).
- `net_addr` returns `host:port`, filling default ports for `http`/`https` when absent.

### HTTP module (`std.http`)

Core types:

- `HttpError`: `InvalidMethod`, `InvalidStatus`, `InvalidHeaderName`, `InvalidHeaderValue`, `InvalidTarget`, `InvalidInput`, `Internal`
- `HttpMethod`: `Get|Head|Post|Put|Patch|Delete|Options`
- `HttpHeader`, `HttpRequest`, `HttpResponse`

Public APIs:

- `parse_method(text: String) -> Result[HttpMethod, HttpError]`
- `method_name(method: HttpMethod) -> Result[String, HttpError]`
- `status_reason(status: Int) -> Result[String, HttpError]`
- `validate_header(name: String, value: String) -> Result[Bool, HttpError]`
- `validate_target(target: String) -> Result[Bool, HttpError]`
- `header(name: String, value: String) -> Result[HttpHeader, HttpError]`
- `request(method, target, headers, body) -> Result[HttpRequest, HttpError]`
- `response(status, headers, body) -> Result[HttpResponse, HttpError]`

Behavior:

- Method parsing is strict and deterministic (`TRACE` returns `InvalidMethod`).
- Status validation enforces `100..599`; known status phrases are stable.
- Header validation enforces token-safe names and control-free values.
- Request-target validation accepts origin-form (`/path`) and absolute-form URLs.

### Runtime Integration

- URL parsing/normalization and HTTP validators are runtime-backed intrinsics in `src/codegen.rs`.
- `std.url.net_addr` is designed for direct use with `std.net` APIs (for example `tcp_listen`/`tcp_connect` address input).

### Example

See `/Users/kasunranasinghe/Projects/Rust/aicore/examples/data/http_types.aic`.

## Date Time Formatting and Parsing (`std.time`) - DT-T5

Core types:

- `TimeError`: `InvalidFormat`, `InvalidDate`, `InvalidTime`, `InvalidOffset`, `InvalidInput`, `Internal`
- `DateTime`: `{ year, month, day, hour, minute, second, millisecond, offset_minutes }`

Public APIs:

- `parse_rfc3339(text: String) -> Result[DateTime, TimeError]`
- `parse_iso8601(text: String) -> Result[DateTime, TimeError]`
- `format_rfc3339(value: DateTime) -> Result[String, TimeError]`
- `format_iso8601(value: DateTime) -> Result[String, TimeError]`

Behavior:

- RFC3339 parsing requires timezone (`Z` or `+/-HH:MM`) and `T` separator.
- ISO8601 parser accepts date-only and timezone-optional forms.
- Formatter output is deterministic with millisecond precision.

Example:

- `/Users/kasunranasinghe/Projects/Rust/aicore/examples/data/audit_timestamps.aic`
