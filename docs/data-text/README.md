# Data/Text Stack Guide (DT-T6)

This is the agent-facing contract for `std.regex`, `std.json` (including serde helpers), `std.url`, `std.http`, and `std.time`.

Use this guide when building log parsers, config loaders, API validators, and timestamp-normalization pipelines.

## Capability Matrix

| Module | Determinism contract | Positive example (CI) | Negative example (CI) |
|---|---|---|---|
| `std.regex` | Stable flag bitmask, stable first-match and capture-group behavior | `examples/data/regex_capture_groups.aic` | `examples/data/data_stack_negative_cases.aic` |
| `std.json` | Stable parse/decode error mapping, deterministic stringify/object ordering | `examples/data/config_json.aic` | `examples/data/data_stack_negative_cases.aic` |
| `std.json` serde (`encode/decode_with/schema`) | Deterministic struct/enum wire format and schema text | `examples/data/serde_models.aic` | `examples/data/serde_negative_cases.aic` |
| `std.url` + `std.http` | Stable parser/validator errors and normalization behavior | `examples/data/http_types.aic` | `examples/data/url_http_negative_cases.aic` |
| `std.time` datetime | Cross-platform stable parse/format output contract | `examples/data/audit_timestamps.aic` | `examples/data/data_stack_negative_cases.aic` |
| End-to-end ingest-transform-emit | Stable composition across modules | `examples/data/ingest_transform_emit.aic` | `examples/data/data_stack_negative_cases.aic` |

## Strict Contracts

### `std.regex`

Core APIs:

- `compile`, `compile_with_flags`
- `is_match`, `find`, `captures`, `find_all`, `replace`

Rules:

- `flags` is a deterministic bitmask (`no_flags`, `flag_case_insensitive`, `flag_multiline`, `flag_dot_matches_newline`).
- `is_match` returns `Ok(Bool)` for both match and no-match conditions.
- `find` returns `Err(NoMatch)` when no substring matches.
- `captures` returns `Ok(None())` when no substring matches.
- `find_all` returns matches in source order and returns an empty vector when no substring matches.
- `replace` replaces the first match only; if no match exists, the original text is returned.
- Unsupported flag combos and malformed patterns map to stable `RegexError` variants.

### `std.json`

Core APIs:

- `parse`, `stringify`
- scalar helpers: `encode_int`, `encode_bool`, `encode_string`, `encode_null`, `decode_int`, `decode_bool`, `decode_string`
- object helpers: `object_empty`, `object_set`, `object_get`, `kind`, `is_null`

Rules:

- `parse` accepts the runtime JSON subset used by AICore and returns typed `JsonError` variants for malformed input.
- `decode_*` validates kind strictly and returns `Err(InvalidType)` on kind mismatch.
- `object_get` returns `Result[Option[JsonValue], JsonError]`.
- `object_get` null handling:
  - `Ok(None())`: key not present
  - `Ok(Some(v))` with `kind(v) == NullValue`: key present with explicit `null`
- `stringify` output is deterministic for a given value.

### `std.json` serde helpers

Core APIs:

- `encode[T](value: T)`
- `decode_with[T](value: JsonValue, marker: Option[T])`
- `schema[T](marker: Option[T])`

Rules:

- Marker must be `Option[T]`; use `None()` with explicit marker type annotations.
- Struct wire format: JSON object keyed by field name.
- Enum wire format: object with deterministic keys:
  - `"tag"`: declaration-order variant index
  - `"value"`: payload value (`null` for no-payload variants)
- Schema generation is deterministic for identical source.
- Backward/forward drift surfaces stable `JsonError` variants (`MissingField`, `InvalidType`, etc.).

### `std.url` and `std.http`

`std.url` core APIs:

- `parse`, `normalize`, `net_addr`, `has_explicit_port`

`std.http` core APIs:

- `parse_method`, `method_name`, `status_reason`
- `validate_header`, `validate_target`
- `header`, `request`, `response`

Rules:

- URL parse/normalize behavior is deterministic for equivalent inputs.
- Default ports are normalized consistently (`http:80`, `https:443`).
- HTTP method/target/header validation returns stable `HttpError` values for malformed inputs.
- `request`/`response` constructors enforce invariants before returning typed values.

### `std.time` datetime APIs (`DT-T5`)

Core APIs:

- `parse_rfc3339`, `parse_iso8601`
- `format_rfc3339`, `format_iso8601`

Accepted parse formats:

- RFC3339 (`parse_rfc3339`):
  - `YYYY-MM-DDTHH:MM:SS[.s|.ss|.sss](Z|+HH:MM|-HH:MM)`
- ISO8601 subset (`parse_iso8601`):
  - `YYYY-MM-DD`
  - `YYYY-MM-DD[T| ]HH:MM`
  - optional `:SS`
  - optional fractional seconds (`.s`, `.ss`, `.sss`) when seconds are present
  - optional timezone: `Z`, `+HH:MM`, `+HHMM`, `+HH` (and negative forms)
  - missing timezone defaults to offset `0`

Datetime validation rules:

- Date range: year `0000..9999`, month/day validated with leap-year handling.
- Time range: `00:00:00.000` through `23:59:59.999`.
- Offset range: `-14:00..+14:00`.
- Leap seconds are rejected (`second=60` -> `InvalidTime`).

Deterministic formatting output:

- `format_rfc3339`: always `YYYY-MM-DDTHH:MM:SS.mmmZ` for zero offset; otherwise `YYYY-MM-DDTHH:MM:SS.mmm+HH:MM`.
- `format_iso8601`: always `YYYY-MM-DDTHH:MM:SS.mmm+HH:MM` (explicit numeric offset).

Stable error mapping (`TimeError`):

- `InvalidFormat`, `InvalidDate`, `InvalidTime`, `InvalidOffset`, `InvalidInput`, `Internal`.

## Wire Format, Determinism, and Versioning Guidance

- Keep struct field names stable in serialized contracts.
- Additive evolution strategy:
  - add new struct fields only when consumers can tolerate `MissingField` on old payloads
  - add new enum variants only with explicit compatibility plans (variant tags are declaration-order)
- Avoid reordering enum variants: it changes wire `tag` values.
- Treat schema output as a deterministic compatibility artifact.
- Prefer explicit migration paths over silent coercion when parse/decode errors occur.

## Cookbook

See `docs/data-text/cookbook.md` for ingest-transform-emit patterns, including timestamp normalization and API payload validation.

## CI Validation Commands

```bash
cargo run --quiet --bin aic -- run examples/data/log_parse_regex.aic
cargo run --quiet --bin aic -- run examples/data/regex_capture_groups.aic
cargo run --quiet --bin aic -- run examples/data/config_json.aic
cargo run --quiet --bin aic -- run examples/data/serde_models.aic
cargo run --quiet --bin aic -- run examples/data/serde_negative_cases.aic
cargo run --quiet --bin aic -- run examples/data/http_types.aic
cargo run --quiet --bin aic -- run examples/data/audit_timestamps.aic
cargo run --quiet --bin aic -- run examples/data/ingest_transform_emit.aic
cargo run --quiet --bin aic -- run examples/data/data_stack_negative_cases.aic
cargo run --quiet --bin aic -- run examples/data/url_http_negative_cases.aic
```

Each command above is expected to print `42` on the last line.
