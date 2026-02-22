# Package Failure Playbooks

## Resolver conflict (`E2114`)

Symptom:

- install command fails with incompatible requirements for the same package.

Actions:

1. run install with `--json` and capture diagnostics.
2. align requirements to a shared semver range.
3. rerun `aic pkg install ...`.
4. regenerate lockfile (`aic lock <project>`).

## Private registry auth/config (`E2117`, `E2118`)

Symptom:

- scoped/private package install fails before dependency extraction.

Actions:

1. verify selected registry alias and scope mapping in `aic.registry.json`.
2. ensure token source exists (`--token`, `token_env`, or token file path).
3. rerun install with `--json` and confirm resolved registry path.

## Provenance/trust policy failures (`E2119`, `E2124`)

Symptom:

- install denied by allow/deny policy or signature validation.

Actions:

1. inspect `audit` records from `aic pkg install ... --json`.
2. verify `trusted_keys` environment variable value for the signature `key_id`.
3. republish package with correct signing key if signature metadata is stale.
4. rerun install and verify `audit[*].signature_verified == true` when required.

## Lock/cache failures (`E2106`, `E2107`, `E2108`, `E2109`)

Symptom:

- drift, checksum mismatch, or offline cache errors.

Actions:

1. run `aic lock <project-or-workspace>` online.
2. commit `aic.lock` changes.
3. rerun offline check/build.
