# AICore Threat Model

## Scope

In scope:

- Compiler CLI execution (`aic`)
- Frontend parsing/type/effect/contract analysis
- LLVM codegen and local native artifact creation
- Release metadata generation (manifest, SBOM, provenance)
- CI/CD automation workflows

Out of scope:

- Third-party registry or package trust decisions outside AICore controls
- Host OS/kernel hardening controls not configured by this repository

## Assets

Primary assets:

- Source integrity (`src/`, `std/`, `docs/`, workflows)
- Release binaries and checksums
- SBOM and provenance artifacts
- Diagnostic outputs consumed by automation agents
- Signing keys used for provenance generation

## Trust Boundaries

1. Developer workstation to repository boundary
2. CI runner to GitHub release boundary
3. Local untrusted program execution boundary (`aic run` sandbox profiles)
4. Secret storage boundary for signing keys in CI

## Threat Scenarios

1. Source tampering between review and release
2. Supply-chain drift from lockfile changes or floating workflow action refs
3. Release artifact substitution after build
4. Running untrusted AIC programs without resource isolation
5. Silent compatibility break that bypasses migration policy

## Mitigations

- Deterministic reproducibility manifest and verification (`aic release manifest` / `verify-manifest`)
- `--locked` builds in CI/release workflows
- SBOM generation from lockfile and signed provenance statements
- Workflow pinning checks and mandatory release hardening checks
- Sandbox profiles for runtime execution (`--sandbox ci|strict` on Linux)
- Compatibility policy checks for docs/workflows/migration command surfaces
- Local and CI gates: `make ci`, `make security-audit`, `make repro-check`

## Residual Risk

- HMAC key compromise invalidates trust in generated provenance signatures.
- Linux-only enforcement for hard runtime limits (`prlimit`) leaves non-Linux profiles non-enforcing.
- Manual release operations can still bypass policy checks if workflows are not used.

Residual risks are accepted for current stage and should be revisited before a 1.0 public security SLA.

