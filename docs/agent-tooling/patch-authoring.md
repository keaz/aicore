# Patch Authoring Guide

Use `aic patch` for deterministic symbol-aware edits that can be previewed before they are applied.

## Contracts

- Request schema: `docs/agent-tooling/schemas/patch-request.schema.json`
- Response schema: `docs/agent-tooling/schemas/patch-response.schema.json`
- Example project: `examples/e7/patch_protocol`

## Workflow

1. Author a patch document that satisfies `patch-request.schema.json`.
2. Run preview first:

```bash
aic patch --preview examples/e7/patch_protocol/patches/valid_patch.json --project examples/e7/patch_protocol --json
```

3. Apply only after preview is clean:

```bash
aic patch --apply examples/e7/patch_protocol/patches/valid_patch.json --project examples/e7/patch_protocol --json
```

4. Re-run `aic check <project>` after apply if the patch is part of a larger edit batch.

## Supported operations

### `add_function`

Required fields:

- `kind: "add_function"`
- `function.name`
- `function.return_type`
- `function.body`

Optional fields:

- `target_file`
- `after_symbol`
- `function.params[]`
- `function.effects[]`
- `function.capabilities[]`
- `function.requires`
- `function.ensures`

Example:

```json
{
  "kind": "add_function",
  "target_file": "src/main.aic",
  "after_symbol": "handle_result",
  "function": {
    "name": "validate_port",
    "params": [{ "name": "c", "ty": "Config" }],
    "return_type": "Bool",
    "body": "c.port >= 0"
  }
}
```

### `modify_match_arm`

Required fields:

- `kind: "modify_match_arm"`
- `target_function`
- `match_index`
- `arm_pattern`
- `new_body`

Optional fields:

- `target_file`

Example:

```json
{
  "kind": "modify_match_arm",
  "target_file": "src/main.aic",
  "target_function": "handle_result",
  "match_index": 0,
  "arm_pattern": "Err(e)",
  "new_body": "0 - e"
}
```

### `add_field`

Required fields:

- `kind: "add_field"`
- `target_struct`
- `field.name`
- `field.ty`

Optional fields:

- `target_file`

Example:

```json
{
  "kind": "add_field",
  "target_file": "src/main.aic",
  "target_struct": "Config",
  "field": {
    "name": "timeout",
    "ty": "Int"
  }
}
```

## Authoring rules

- Keep `target_file` project-relative whenever possible.
- Do not queue two operations against the same semantic target in one document. Overlaps are rejected deterministically.
- Keep operation ordering intentional. `operation_index` in conflicts maps directly to array position.
- Treat preview output as the authoritative diff plan. Do not assume source spans yourself.
- Unknown fields are rejected by the parser.

## Response interpretation

Successful responses contain:

- `files_changed[]`
- `applied_edits[]`
- `previews[]`

Conflict responses contain:

- `conflicts[].operation_index`
- `conflicts[].kind`
- `conflicts[].message`
- optional `conflicts[].file`

Common conflict kinds:

- `document`: patch JSON could not be read or parsed
- `resolve_target`: target file or symbol could not be resolved
- `plan`: requested symbol/arm/field could not be planned
- `overlap`: two operations target the same semantic location
- `validate`: patched source no longer parses
- `validate_semantics`: patched source parses but fails frontend type/effect validation
- `write`: apply mode hit an IO failure; earlier writes were rolled back

## Determinism and safety

- `preview` never mutates the workspace.
- `apply` writes only when every operation is valid.
- Multi-file apply is transactional: if a later file write fails, earlier patched files are restored.
