# Sandbox Profiles Examples

This directory demonstrates `OPS-T2` sandbox policy behavior.

## Profile Files

- `profiles/dev.json`: unrestricted development profile
- `profiles/ci.json`: CI profile (resource limits + no net/proc)
- `profiles/prod.json`: production profile (resource limits + no net/proc)
- `profiles/strict.json`: fully locked-down profile

## Runtime Policy Demos

- `fs_blocked_demo.aic`
- `net_blocked_demo.aic`
- `proc_blocked_demo.aic`
- `time_blocked_demo.aic`

Run with a policy file:

```bash
aic run examples/ops/sandbox_profiles/fs_blocked_demo.aic \
  --sandbox-config examples/ops/sandbox_profiles/profiles/strict.json
```

When a disallowed operation is invoked, runtime emits machine-readable diagnostics on stderr:

```json
{"code":"sandbox_policy_violation","trace_id":"ops-trace-001","profile":"strict","domain":"fs","operation":"read_text"}
```
