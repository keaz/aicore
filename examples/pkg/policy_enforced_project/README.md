# Policy Enforced Project (PKG-T4)

This example demonstrates package trust policy configuration for signed-package installs.

## Configure trusted verification key

```bash
export AIC_TRUSTED_CORP_KEY="<shared-hmac-key>"
```

## Install from a registry with trust policy

```bash
aic pkg install corp/signed_pkg@^1.0.0 --path examples/pkg/policy_enforced_project --json
```

Install output includes an `audit` section with deterministic trust decisions per resolved package.
