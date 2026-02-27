# Agent Cookbook (AG-T5)

This directory defines reproducible autonomous workflows for common development loops:

- feature delivery (`feature-loop.md`)
- bugfix/autofix (`bugfix-loop.md`)
- deterministic refactor (`refactor-loop.md`)
- diagnostics triage (`diagnostics-loop.md`)
- secure Postgres TLS/SCRAM replay (`secure-postgres-tls-scram-loop.md`)

Each recipe contains:

- protocol fixture references
- explicit fallback behavior
- executable docs-test commands between:
  - `<!-- docs-test:start -->`
  - `<!-- docs-test:end -->`

Docs-as-tests coverage is enforced by `tests/agent_recipe_tests.rs` and run in CI.
