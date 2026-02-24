# Typed Holes Workflow

Use typed holes (`_`) while iterating on function signatures and local annotations.

Example source:

- `examples/e7/typed_holes.aic`

Inspect inferred hole types:

```bash
aic check examples/e7/typed_holes.aic --show-holes
```

Output shape:

```json
{
  "holes": [
    {
      "line": 6,
      "inferred": "Int",
      "context": "parameter 'x' in function 'plus_one'"
    }
  ]
}
```

Typed holes produce warning `E6003` and do not fail `aic check` unless other errors are present.
