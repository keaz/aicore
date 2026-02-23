# Hermetic build manifest example

Build with deterministic metadata and optional hash verification:

```bash
aic build examples/e7/cli_smoke.aic \
  -o target/e7/cli_smoke.hermetic \
  --manifest target/e7/build.json \
  --verify-hash <expected-sha256>
```

If `--manifest` is omitted for single-target builds, `aic build` writes `build.json` in the current working directory.

The manifest contains:

- `input_path`
- `output_path`
- `output_sha256`
- `content_addressed_artifact_path`
- `artifact_kind`

`content_addressed_artifact_path` points to a materialized content-addressed copy under:

```text
<output-parent>/.aic/artifacts/<artifact-kind>/<sha256>/<output-filename>
```
