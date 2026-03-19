# Incremental Daemon Protocol (AG-T4)

`aic daemon` exposes a line-delimited JSON-RPC 2.0 interface over stdio for long-lived check/build and collaboration-session workflows.

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
- `session.create`
- `session.list`
- `session.lock.acquire`
- `session.lock.release`
- `session.conflicts`
- `session.merge`
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

## `session.create` request

```json
{"jsonrpc":"2.0","id":10,"method":"session.create","params":{"project":"examples/e7/session_protocol","label":"alpha","now_ms":100}}
```

Creates a deterministic session id rooted at the supplied project path.

## `session.lock.acquire` request

```json
{"jsonrpc":"2.0","id":11,"method":"session.lock.acquire","params":{"project":"examples/e7/session_protocol","session_id":"sess-0002","target":["function","handle_result"],"operation_id":"op-valid-modify","lease_ms":30000,"now_ms":1000}}
```

Contention is modeled as a normal `result` payload with `ok: false`, `denied_by`, and an optional current `lock`, rather than a JSON-RPC transport error.

## `session.conflicts` request

```json
{"jsonrpc":"2.0","id":12,"method":"session.conflicts","params":{"project":"examples/e7/session_protocol","plan":"examples/e7/session_protocol/plans/valid_plan.json"}}
```

Returns deterministic `operations[]` plus structured `conflicts[]` for unknown sessions, unresolved symbols, overlapping edits, and lock ownership problems.

## `session.merge` request

```json
{"jsonrpc":"2.0","id":13,"method":"session.merge","params":{"project":"examples/e7/session_protocol","plan":"examples/e7/session_protocol/plans/valid_plan.json","offline":false,"now_ms":1000}}
```

Runs validation-only merge inside an isolated temp workspace and returns `valid`, `merged_files[]`, and any frontend `diagnostics[]`.

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

Session state includes:

- deterministic session registry under `.aic-sessions/state.json`
- project-relative symbol keys (`kind`, module/name, file, span start) for lock identity
- lease expiry timestamps (`expires_ms`) for reclaim decisions
- lock metadata under `.aic-sessions/.state.lock` (`pid`, `host`, `created_ms`, `process_hint`)
- stale lock self-healing for crashed owners (dead PID on same host), while preserving live owners

Any dependency source change causes fingerprint changes and forces a cache miss.

## Example fixture

- `examples/agent/incremental_demo/`
- Request script: `examples/agent/incremental_demo/requests/check_build_shutdown.jsonl`
- Error taxonomy script: `examples/agent/incremental_demo/requests/error_taxonomy.jsonl`
- Stale-lock recovery script: `examples/agent/incremental_demo/requests/session_lock_recovery.jsonl`

Run:

```bash
aic daemon < examples/agent/incremental_demo/requests/check_build_shutdown.jsonl
```

For stale-lock recovery workflows, first seed a stale `.aic-sessions/.state.lock`, then run:

```bash
aic daemon < examples/agent/incremental_demo/requests/session_lock_recovery.jsonl
```

## Failure handling

- Every JSON-RPC error response includes `error.data` with this stable shape:

```json
{
  "kind": "invalid_param",
  "retryable": false,
  "param": "input",
  "details": {
    "method": "check"
  }
}
```

- Stable `error.data.kind` values:
  - `parse_error`: request payload was not valid JSON.
  - `invalid_request`: required JSON-RPC envelope fields were missing.
  - `method_not_found`: unknown method name.
  - `invalid_param`: request envelope was valid, but parameters were missing/invalid.
  - `file_not_found`: referenced source/plan/file path could not be resolved.
  - `frontend_failed`: frontend/build pipeline failed for a valid request.
  - `session_lock_conflict`: session lock/session ownership prerequisites were not met.
  - `internal`: fallback for unknown/unclassified daemon failures.

- `retryable` indicates whether immediate remediation/retry is expected to be useful.
- `param` is present when a specific parameter is invalid.
- `details` is optional method-scoped context for deterministic agent remediation.

- Invalid JSON payloads return parse errors (`code = -32700`, `kind = "parse_error"`).
- Missing/invalid parameters return request errors (`code = -32602`, `kind = "invalid_param"`).
- Unknown methods return method errors (`code = -32601`, `kind = "method_not_found"`).
- Session state-lock timeouts include the latest observed lock metadata (`pid`, `host`, `age/alive` status, or malformed metadata summary) plus explicit remediation guidance.
- Session/lock/merge business conflicts stay in the `result` payload with `ok: false`; they do not use JSON-RPC `error`.
