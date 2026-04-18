# Self-Host Stage Compiler Matrix

The stage compiler matrix validates the latest self-hosted `aic_selfhost` executable through the command surfaces used by packages, examples, and release gates. It is a production readiness gate for compiler/tooling integration only.

## Scope

The matrix is intentionally limited to core compiler and core-language inputs:

- single-file core-language examples
- package directories with `aic.toml` main metadata
- package members inside the compiler workspace when the self-host driver can read them as package directories
- negative core-language diagnostics with required diagnostic codes
- explicit unsupported workspace-root probes that are recorded as non-readiness results

It does not include services, editor tooling, network protocols, or non-core libraries. Those areas are validated by their own project gates.

## Commands

Run the matrix against the stage compiler produced by `make selfhost-bootstrap`:

```bash
make selfhost-stage-matrix
```

Override the compiler path when validating a local candidate:

```bash
SELFHOST_STAGE_COMPILER=target/aic_selfhost_candidate make selfhost-stage-matrix
```

The default manifest is `tests/selfhost/stage_matrix_manifest.json`. The report is written to `target/selfhost-stage-matrix/report.json`, with per-action stdout/stderr files and build artifacts isolated under `target/selfhost-stage-matrix/`.

`make selfhost-bootstrap` runs the same matrix as a required `stage-matrix` step after building the latest available stage compiler. Supported self-hosting readiness requires this step to pass.

## Report Format

The report format marker is `aicore-selfhost-stage-matrix-v1`. The report records:

- manifest name, schema version, and case count
- stage compiler command
- artifact directory
- aggregate summary by status, action, and input kind
- per-action command, exit code, timeout state, duration, stdout/stderr paths, output excerpts, SHA-256 digests, diagnostic codes, artifact path, artifact digest, and artifact size

Result statuses have strict meanings:

- `passed`: a readiness action matched its expected positive or negative behavior.
- `failed`: a readiness action regressed, timed out, missed a required diagnostic, emitted invalid IR JSON, or failed to materialize a required build artifact.
- `unsupported`: an explicitly configured non-readiness case produced the expected unsupported diagnostic. These results are evidence, but they are not counted as passing readiness coverage.

## Adding Cases

Add cases to `tests/selfhost/stage_matrix_manifest.json` only when the input is part of the core language or compiler package surface.

Each case must include:

- `name`: stable identifier used for artifact paths
- `kind`: `single-file`, `package`, `package-member`, or `workspace`
- `path`: repository-relative input path
- `expected`: `pass`, `fail`, or `unsupported`
- `actions`: one or more of `check`, `ir-json`, `build`, and `run`

Use `diagnostic_codes` for negative and unsupported cases. A negative case should require the primary code that proves the intended failure path, for example:

```json
{
  "name": "trait_bound_negative_diagnostic",
  "kind": "single-file",
  "path": "tests/selfhost/cases/trait_bound_invalid.aic",
  "expected": "fail",
  "actions": ["check"],
  "diagnostic_codes": {
    "check": ["E1258"]
  }
}
```

Use `readiness: false` only for `expected: "unsupported"` probes. Unsupported cases must still execute the compiler and must require a diagnostic code; they cannot be counted as passing readiness coverage.

Before closing an implementation issue that changes this matrix, run:

```bash
python3 scripts/selfhost/stage_matrix.py --manifest tests/selfhost/stage_matrix_manifest.json --list
make selfhost-stage-matrix
make selfhost-bootstrap
make examples-check
make examples-run
make docs-check
```
