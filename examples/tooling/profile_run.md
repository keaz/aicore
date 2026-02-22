# Profile Run Example

Run with profiling and write `profile.json`:

```bash
aic run examples/e7/cli_smoke.aic --profile
```

Write to a custom path:

```bash
aic run examples/e7/cli_smoke.aic --profile --profile-output target/tooling/profile.json
```

Report shape:

```json
{
  "phase": "profile",
  "schema_version": "1.0",
  "input": "examples/e7/cli_smoke.aic",
  "output": "target/tooling/profile.json",
  "top_functions": [
    {
      "function": "run.execute",
      "self_time_ms": 1.234,
      "total_time_ms": 1.234,
      "calls": 1
    }
  ]
}
```
