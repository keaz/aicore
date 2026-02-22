# Coverage Report Example

Generate deterministic coverage JSON for a file and persist it:

```bash
aic coverage examples/e7/cli_smoke.aic --report target/tooling/coverage.json
```

Gate with a minimum threshold:

```bash
aic coverage examples/e7/cli_smoke.aic --check --min 80 --report target/tooling/coverage-check.json
```

Report shape:

```json
{
  "phase": "coverage",
  "schema_version": "1.0",
  "summary": {
    "files_total": 1,
    "files_covered": 1,
    "functions_total": 1,
    "functions_covered": 1,
    "coverage_pct": 100.0
  },
  "check": {
    "min_pct": 80.0,
    "passed": true
  },
  "files": [
    {
      "path": "examples/e7/cli_smoke.aic",
      "functions_total": 1,
      "functions_covered": 1,
      "error_count": 0
    }
  ]
}
```
