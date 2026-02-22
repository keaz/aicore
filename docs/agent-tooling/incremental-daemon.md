# Incremental Daemon Protocol (AG-T4)

`aic daemon` exposes a line-delimited JSON-RPC 2.0 interface over stdio for long-lived check/build workflows.

Goals:

- Reuse parse/check/build work across requests.
- Invalidate cache entries deterministically using content hashes.
- Keep warm and cold outputs equivalent.

## Start daemon

```bash
aic daemon
```

Each request is a single JSON line. Each response is a single JSON line.

## Supported methods

- `check`
- `build`
- `stats`
- `shutdown`

## `check` request

```json
{"jsonrpc":"2.0","id":1,"method":"check","params":{"input":"examples/agent/incremental_demo/app/src/main.aic","offline":false}}
```

`check` response includes:

- `cache_hit`: whether frontend output came from daemon cache
- `fingerprint`: deterministic content-hash key (project + dependency checksums)
- `diagnostics`: frontend diagnostics
- `duration_ms`: wall time for the request

## `build` request

```json
{"jsonrpc":"2.0","id":2,"method":"build","params":{"input":"examples/agent/incremental_demo/app/src/main.aic","artifact":"exe","output":"target/incremental-demo","debug_info":false,"offline":false}}
```

`build` response includes:

- `cache_hit`: whether artifact build was reused
- `frontend_cache_hit`: whether frontend output was reused
- `output_sha256`: artifact digest for parity verification
- `diagnostics`: build/codegen diagnostics (if any)
- `duration_ms`: wall time for the request

## `stats` request

```json
{"jsonrpc":"2.0","id":3,"method":"stats","params":{}}
```

Returns request counters and cache hit/miss counters.

## `shutdown` request

```json
{"jsonrpc":"2.0","id":4,"method":"shutdown","params":{}}
```

Returns final `stats` and exits cleanly.

## Invalidation rules

Cache fingerprints include:

- canonical input path
- project root checksum (`aic.toml` package source tree)
- each resolved dependency source root checksum
- `offline` mode
- dependency-context diagnostics + lockfile usage markers
- build parameters (`artifact`, `debug_info`, `output`) for build cache entries

Any dependency source change causes fingerprint changes and forces a cache miss.

## Example fixture

- `examples/agent/incremental_demo/`
- Request script: `examples/agent/incremental_demo/requests/check_build_shutdown.jsonl`

Run:

```bash
aic daemon < examples/agent/incremental_demo/requests/check_build_shutdown.jsonl
```

## Failure handling

- Invalid JSON payloads return JSON-RPC parse errors (`code = -32700`).
- Missing/invalid parameters return request errors (`code = -32602`).
- Unknown methods return method errors (`code = -32601`).
