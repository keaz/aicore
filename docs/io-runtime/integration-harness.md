# External Protocol Integration Harness

This document defines the generic container-backed integration harness used to validate external protocol client libraries without embedding protocol-specific logic into AICore core modules.

## Goals

- Keep language/runtime core protocol-agnostic.
- Provide a deterministic harness contract for external libraries (PostgreSQL, Kafka, Redis, and others).
- Separate replay/offline validation from live container integration.

## Harness Entry Points

- Offline replay gate:
  - `make integration-harness-offline`
  - Runs deterministic `offline_cases` from `tests/integration/protocol-harness.matrix.json`.
- Live smoke gate:
  - `AIC_INTEGRATION_LIVE=1 make integration-harness-live`
  - Runs `live_cases` from the same matrix.
  - Requires Docker.

Harness script:

- `scripts/ci/integration-harness.py`
- Defaults `AIC_STD_ROOT` to `<repo>/std` for case commands when unset to keep replay checks pinned to repository std APIs.

Report artifact:

- `target/e8/integration-harness-report.json`

## Deterministic Gating Modes

- `offline`:
  - No container startup required.
  - Used as default CI gate.
  - Intended for replay contracts and deterministic scenario suites.
- `live`:
  - Starts service containers per matrix case.
  - Waits for health checks, runs smoke commands, and tears containers down.
  - Opt-in by setting `AIC_INTEGRATION_LIVE=1`.

## Matrix Contract

Matrix file:

- `tests/integration/protocol-harness.matrix.json`

Top-level fields:

- `schema_version`: pinned to `1`
- `offline_cases`: deterministic replay commands
- `live_cases`: container-backed smoke cases

Case fields (required):

- `id`
- `service`
- `version`
- `auth`
- `security`
- `offline_cases`: `command`
- `live_cases`: `compose_file`, `healthcheck_cmd`, `smoke_cmd`

Optional live fields:

- `up_cmd`, `down_cmd`
- `healthcheck_retries`, `healthcheck_sleep_seconds`
- `env` map

## External Client Library Plug-In Path

External repositories can plug in with minimal boilerplate by adding matrix cases that call their own smoke commands.

Expected pattern:

1. Add Docker compose profile for the target service/version/auth/security combination.
2. Add a matrix case with healthcheck and smoke command.
3. Point smoke command to the external client test entrypoint.

No AICore runtime changes are required for service-specific protocol logic.

## CI Wiring

`tests-linux-full` in `.github/workflows/ci.yml` runs:

- `make integration-harness-offline` (always)
- `make integration-harness-live` (only when `AIC_ENABLE_INTEGRATION_LIVE=1`)

The harness report is uploaded as `integration-harness-linux`.
