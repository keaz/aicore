# Config Loading API (`std.config`) (Issue #165)

This document defines the contract for loading application configuration from JSON files and environment variables.

## Overview

`std.config` is a composition layer over:

- `std.fs` for file reads
- `std.json` for JSON parsing/decoding
- `std.env` for environment-variable discovery
- `std.map` for deterministic key/value storage

It targets a common workflow: load base config from file, then apply env overrides.

## Types

```aic
enum ConfigError {
    FileNotFound,
    PermissionDenied,
    InvalidInput,
    Io,
    InvalidJson,
    InvalidValue,
    MissingKey,
}
```

## API Surface

```aic
fn load_json(path: String) -> Result[Map[String, String], ConfigError] effects { fs }
fn load_env_prefix(prefix: String) -> Map[String, String] effects { env }
fn get_or_default(config: Map[String, String], key: String, fallback: String) -> String
fn require(config: Map[String, String], key: String) -> Result[String, ConfigError]
```

## Behavior

- `load_json`:
  - reads the file via `std.fs.read_text`
  - parses JSON via `std.json.parse`
  - decodes as `Map[String, String]` via `std.json.decode_with`
  - maps filesystem/JSON failures into `ConfigError`
- `load_env_prefix`:
  - iterates `std.env.all_vars()`
  - includes keys that start with `prefix`
  - strips the matched prefix from each resulting key
- `get_or_default`:
  - returns the value when key exists
  - otherwise returns caller-provided fallback
- `require`:
  - returns `Ok(value)` when key exists
  - returns `Err(MissingKey)` when key is absent

## Env prefix semantics

Prefix stripping is direct and case-sensitive.

- Input env key: `APP_PORT`
- Prefix: `APP_`
- Output map key: `PORT`

## Example workflow

Reference example: `examples/io/config_loading.aic`

The example demonstrates:

- JSON file config load
- prefixed env overlay (`APP_...`)
- deterministic override merge (env values replace file values on same key)
- required/missing key handling

## Current limitations

- `load_json` currently expects a JSON object whose values decode to `String`.
- Nested JSON objects/arrays are not flattened automatically.
- No built-in precedence policy beyond what callers implement (example uses file then env merge).

## CLI manifest thresholds (`aic.toml`)

The `aic metrics` command also loads deterministic threshold configuration from the nearest
`aic.toml` file using a `[metrics]` section:

```toml
[metrics]
max_cyclomatic = 15
max_cognitive = 25
max_lines = 120
max_params = 6
max_nesting_depth = 4
```

Behavior:

- All threshold keys are optional.
- `aic metrics --check` uses configured thresholds.
- `--max-cyclomatic` overrides configured `max_cyclomatic` for that invocation.
