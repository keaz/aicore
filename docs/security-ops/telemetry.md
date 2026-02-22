# Telemetry Guide

`OPS-T3` telemetry provides structured logs, metrics, and trace correlation IDs for compiler/runtime operations.

## Enable telemetry

Set environment variables before invoking `aic`:

```bash
AIC_TRACE_ID=my-trace-id \
AIC_TELEMETRY_PATH=target/ops/telemetry.jsonl \
aic check examples/ops/observability_demo/main.aic
```

- `AIC_TRACE_ID`: optional explicit correlation identifier.
- `AIC_TELEMETRY_PATH`: output path for newline-delimited JSON events.

## Event model

Each line is a telemetry event matching `docs/security-ops/telemetry.schema.json`.

Core fields:

- `schema_version`: telemetry schema version (`1.0`)
- `event_index`: monotonic event sequence
- `timestamp_ms`: wall-clock timestamp
- `trace_id`: correlation ID shared across command/runtime boundaries
- `command`: high-level component (`frontend`, `codegen`, `run`)
- `kind`: `phase` or `metric`

## Logs, Metrics, and Traces

- logs:
  - each JSON line is an immutable event record
  - use `event_index` for deterministic ordering
- metrics:
  - emitted as `kind = "metric"` events (`diagnostic_count`, `exit_code`, etc.)
  - suitable for SLA dashboards and release quality gates
- traces:
  - `trace_id` correlates compiler stages and runtime diagnostics
  - set `AIC_TRACE_ID` explicitly during incident investigations

Examples:

- phase event: `frontend.resolve`, `codegen.llvm_emit`, `run.execute`
- metric event: `diagnostic_count`, `llvm_emit_diagnostic_count`, `exit_code`

## Correlation behavior

Runtime sandbox violation diagnostics include `trace_id` so operators can correlate stderr failures with telemetry records quickly.

Example runtime diagnostic:

```json
{"code":"sandbox_policy_violation","trace_id":"my-trace-id","profile":"strict","domain":"fs","operation":"read_text"}
```

Correlation practices:

1. set a unique `AIC_TRACE_ID` per CI run or incident replay
2. persist telemetry logs with artifacts (`target/ops/*.jsonl`)
3. join stderr sandbox violations with telemetry by `trace_id`
4. attach the joined timeline to incident reports
