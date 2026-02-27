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

This confirms conformance, fuzzing, differential, matrix, concurrency-stress, and performance gates remain green.

## Step 5: Concurrency Stress Replay (when applicable)

If `make test-e8` fails in `e8_concurrency_stress_tests`, use generated replay metadata:

```bash
cat target/e8/concurrency-stress-replay.txt
AIC_CONC_STRESS_REPLAY='<seed>:<round>:<case_id>' cargo test --locked --test e8_concurrency_stress_tests -- --exact concurrency_stress_suite_is_replayable_and_within_budget --nocapture --test-threads=1
```
