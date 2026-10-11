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
`aic build` and `aic run` use the inferred types, so a program with resolved holes builds and runs.
A hole whose type cannot be inferred (for example a parameter of a function that is never called) is error `E1296`, because code generation needs a concrete type.
