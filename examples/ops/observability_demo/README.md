# Observability Demo

This example demonstrates telemetry capture and trace correlation (`OPS-T3`).

## Run with telemetry enabled

```bash
AIC_TRACE_ID=demo-trace-001 \
AIC_TELEMETRY_PATH=target/ops/telemetry-demo.jsonl \
aic run examples/ops/observability_demo/main.aic --sandbox none
```

## Capture frontend telemetry on check

```bash
AIC_TRACE_ID=demo-trace-002 \
AIC_TELEMETRY_PATH=target/ops/telemetry-check.jsonl \
aic check examples/ops/observability_demo/main.aic
```

## Inspect events

Telemetry is newline-delimited JSON (`jsonl`) and follows schema `docs/security-ops/telemetry.schema.json`.
