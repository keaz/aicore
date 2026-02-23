# Grammar command examples

Use the CLI grammar command to consume the frozen language grammar artifact.

## Print EBNF

```bash
aic grammar --ebnf
```

## Print JSON envelope

```bash
aic grammar --json
```

Expected JSON keys:
- `version`
- `format`
- `grammar`
- `source_path`
- `source_contract_path`
