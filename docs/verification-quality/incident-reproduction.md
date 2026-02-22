# Verification Incident Reproduction

This runbook reproduces a full verification incident from failure to validated fix.

## Scenario

Static `ensures` proof failure (`E4002`) followed by verifier-friendly fix.

## Step 1: Reproduce Failure

```bash
aic check examples/verify/qv_contract_proof_fail.aic --json
```

Expected signal:

- exit code `1`
- diagnostic code `E4002`

## Step 2: Apply Fix

Use the corrected contract formulation:

```bash
aic check examples/verify/qv_contract_proof_fixed.aic --json
```

Expected signal:

- exit code `0`
- no `E4002`

## Step 3: Runtime Sanity

```bash
aic run examples/verify/qv_contract_proof_fixed.aic
```

Expected output: `7`

## Step 4: Gate Verification

```bash
make test-e8
```

This confirms conformance, fuzzing, differential, matrix, and performance gates remain green.
