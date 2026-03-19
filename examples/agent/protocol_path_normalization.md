# Path Normalization Example

This example demonstrates the canonical machine-path policy for machine-facing JSON.

## Daemon check (relative input -> canonical output)

Request:

```json
{"jsonrpc":"2.0","id":1,"method":"check","params":{"input":"src/main.aic"}}
```

Response excerpt:

```json
{
  "protocol_version": "1.0",
  "phase": "check",
  "input": "/abs/project/src/main.aic",
  "diagnostics": [
    {
      "spans": [
        {
          "file": "/abs/project/src/main.aic"
        }
      ]
    }
  ]
}
```

## Patch preview (canonical path fields)

Response excerpt:

```json
{
  "phase": "patch",
  "mode": "preview",
  "files_changed": ["/abs/project/src/main.aic"],
  "applied_edits": [
    {
      "file": "/abs/project/src/main.aic",
      "start": 120,
      "end": 160
    }
  ]
}
```

## Query/symbols (canonical roots and symbol locations)

Response excerpt:

```json
{
  "project_root": "/abs/project",
  "symbols": [
    {
      "location": {
        "file": "/abs/project/src/main.aic"
      }
    }
  ]
}
```
