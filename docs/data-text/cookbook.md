# Data/Text Cookbook

This cookbook focuses on high-signal workflows that autonomous agents can implement without guessing runtime behavior.

## Workflow 1: Ingest -> Transform -> Emit

Goal: accept raw text logs, validate shape, normalize timestamp, and emit a structured JSON payload.

Reference implementation:

- `examples/data/ingest_transform_emit.aic`

Pattern:

1. Validate the input line shape using `std.regex` (`compile` + `is_match`).
2. Parse and normalize timestamp with `std.time.parse_rfc3339` + `std.time.format_iso8601`.
3. Build JSON object using `std.json.object_set` and emit with `std.json.stringify`.
4. Validate API request constraints using `std.http` (`parse_method`, `validate_header`, `validate_target`).

## Workflow 2: Config Decode with Explicit Failure Surface

Goal: decode config JSON and preserve deterministic failure modes.

Reference implementations:

- Positive path: `examples/data/config_json.aic`
- Negative path: `examples/data/data_stack_negative_cases.aic`

Pattern:

1. Parse raw config with `std.json.parse`.
2. Pull keys with `object_get` and branch explicitly on:
   - `None()` (missing key)
   - `Some(value)` + `is_null(value)` (explicit null)
3. Decode into scalar types with `decode_*` and fail fast on `InvalidType`.
4. Emit deterministic output with `stringify` only after all fields validate.

## Workflow 3: Typed Model Serialization Contracts

Goal: keep model evolution deterministic and auditable.

Reference implementations:

- Positive ADT roundtrip: `examples/data/serde_models.aic`
- Compatibility negatives: `examples/data/serde_negative_cases.aic`

Pattern:

1. Encode model via `encode[T]`.
2. Persist/send with `stringify`.
3. Decode with explicit marker type: `let marker: Option[T] = None(); decode_with(value, marker)`.
4. Gate changes with `schema[T](marker)` output and preserve variant order.

## Workflow 4: URL + HTTP Payload Gate

Goal: reject malformed endpoints and request metadata early.

Reference implementations:

- Positive URL/HTTP construction: `examples/data/http_types.aic`
- Negative URL/HTTP validation: `examples/data/url_http_negative_cases.aic`

Pattern:

1. Parse and normalize endpoint URLs before network usage.
2. Validate method and status semantics (`parse_method`, `status_reason`).
3. Validate header names/values with `validate_header` or `header` constructor.
4. Validate target with `validate_target` or `request` constructor.

## Workflow 5: Timestamp Normalization

Goal: normalize mixed producer timestamps into one deterministic format.

Reference implementation:

- `examples/data/audit_timestamps.aic`

Pattern:

1. Parse external producer timestamp (`parse_rfc3339` or `parse_iso8601`).
2. Convert to canonical text with `format_iso8601` or `format_rfc3339`.
3. Roundtrip parse the formatted value before persistence to prevent drift.
4. Branch explicitly on `TimeError` variants for observability.
