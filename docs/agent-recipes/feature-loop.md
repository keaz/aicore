# Feature Loop Recipe

## Goal

Ship a feature with deterministic check/build behavior and runnable validation.

## Protocol Example

- Parse baseline: `examples/agent/protocol_parse.json`
- Build envelope: `examples/agent/protocol_build.json`
- Incremental daemon request sample: `examples/agent/incremental_demo/requests/check_build_shutdown.jsonl`

## Workflow

1. Verify baseline health.
2. Build an artifact with explicit output path.
3. Execute a runtime smoke to confirm behavior.

## Fallback Behavior

- If `check` fails: run diagnostics loop recipe first.
- If `build` fails: inspect diagnostics JSON and retry with `--debug-info`.
- If runtime smoke fails: keep artifact and capture stderr for triage before retry.

## Docs Test

<!-- docs-test:start -->
aic check examples/e7/cli_smoke.aic
aic build examples/e7/cli_smoke.aic --artifact obj -o target/agent-recipes/feature-loop.o
aic run examples/e7/cli_smoke.aic
<!-- docs-test:end -->
