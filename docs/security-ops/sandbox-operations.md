# Sandbox Operations

This runbook documents sandbox profile design, enforcement semantics, and policy usage.

## Profile Model

Built-in profiles:

- `none`: no additional restrictions
- `ci`: bounded resources, conservative permissions
- `strict`: tighter limits for untrusted workloads

Custom policy files:

- schema: `profile`, `permissions`, optional `limits`
- examples: `examples/ops/sandbox_profiles/profiles/*.json`

## Enforcement Semantics

Runtime checks map operations to domains:

- `fs`
- `net`
- `proc`
- `time`

Disallowed operations emit machine-readable JSON on stderr:

```json
{"code":"sandbox_policy_violation","trace_id":"ops-trace-001","profile":"strict","domain":"fs","operation":"read_text"}
```

## Example Commands

```bash
aic run examples/ops/sandbox_profiles/fs_blocked_demo.aic --sandbox-config examples/ops/sandbox_profiles/profiles/strict.json
aic run examples/ops/sandbox_profiles/net_blocked_demo.aic --sandbox-config examples/ops/sandbox_profiles/profiles/strict.json
aic run examples/ops/sandbox_profiles/proc_blocked_demo.aic --sandbox-config examples/ops/sandbox_profiles/profiles/strict.json
aic run examples/ops/sandbox_profiles/time_blocked_demo.aic --sandbox-config examples/ops/sandbox_profiles/profiles/strict.json
```

## Operational Guidance

- use `none` only for trusted local development
- use `ci` for regular automation and deterministic checks
- use `strict` for unknown samples and agent-supplied code
- always set `AIC_TRACE_ID` during incident investigations for correlation with telemetry
