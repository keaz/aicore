# AST JSON protocol usage

Command:

```bash
aic ast --json examples/e7/cli_smoke.aic
```

Schema:

- `docs/agent-tooling/schemas/ast-response.schema.json`

Required response fields:

- `version`
- `module`
- `ast`
- `ir`
- `resolved_types`
- `generic_instantiations`
- `function_effects`
- `contracts`
- `import_graph`

The response is deterministic:

- map-like data is emitted with stable key ordering
- type and instantiation tables are sorted by stable IDs
- contract extraction preserves source order for reproducible agent consumption
