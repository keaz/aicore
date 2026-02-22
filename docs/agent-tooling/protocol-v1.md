# Agent Compiler Protocol v1.0

This document defines the machine-facing JSON protocol contract for parse/check/build/fix workflows.

## Version negotiation

Query the contract and negotiate a protocol version:

```bash
aic contract --json --accept-version 1.2,1.0
```

Negotiation rules:

- server publishes supported versions in `protocol.supported_versions`
- server selects the highest compatible version where:
  - major version matches
  - server version is less than or equal to requested version
- if no version is compatible, `protocol.compatible` is `false` and command exits with status `1`

## Published schemas

- Parse response: `docs/agent-tooling/schemas/parse-response.schema.json`
- Check response: `docs/agent-tooling/schemas/check-response.schema.json`
- Build response: `docs/agent-tooling/schemas/build-response.schema.json`
- Fix response: `docs/agent-tooling/schemas/fix-response.schema.json`

## Compatibility guarantees

- Schema files are versioned (`...-1.0.schema.json` in `$id`) and backward compatible within major version `1`.
- New optional fields are allowed in minor updates.
- Removing required fields, changing field meaning, or changing required field types requires a major version bump.
- Diagnostics remain stable via code IDs and deterministic payload ordering.

## Reference examples

- Parse: `examples/agent/protocol_parse.json`
- Check: `examples/agent/protocol_check.json`
- Build: `examples/agent/protocol_build.json`
- Fix: `examples/agent/protocol_fix.json`
