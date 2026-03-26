# Package Ecosystem Agent Guide (PKG-T6)

This guide is machine-first documentation for AICore package workflows.

Use this when an agent must:

- create and publish a package
- consume packages from public/private registries
- enforce trust policy and provenance checks
- use deterministic lockfiles and offline cache
- manage workspace dependency graphs
- integrate native C ABI dependencies safely

## Capability map

- PKG-T1 public publish/search/install: `docs/package-ecosystem/publish-consume.md`
- PKG-T2 private registries/scopes/auth/mirrors: `docs/package-ecosystem/publish-consume.md`
- PKG-T3 FFI bridge + ABI safety: `docs/package-ecosystem/ffi-and-supply-chain.md`
- PKG-T4 trust policy + signature provenance: `docs/package-ecosystem/ffi-and-supply-chain.md`
- PKG-T5 deterministic workspace graph + shared lockfile: `docs/package-ecosystem/workspaces-and-locks.md`
- Failure recovery runbooks: `docs/package-ecosystem/failure-playbooks.md`

## Canonical examples

- Consumer import flow: `examples/pkg/consume_http_client.aic`
- Trust policy flow: `examples/pkg/policy_enforced_project/`
- Workspace flow: `examples/pkg/workspace_demo/`
- FFI declaration pattern: `examples/pkg/ffi_zlib.aic`

The consumer import flow now demonstrates `std.http` request/response construction and header validation without any fake network I/O.

## Determinism contract

- Resolver picks the highest compatible semver version from a sorted candidate list.
- Lockfile generation is deterministic.
- Offline builds consume `.aic-cache/` entries validated against lockfile checksums.
- Workspace builds use deterministic topological order with lexical tie-breaks.
