---
name: Language roadmap task
about: A child issue of the language production-readiness roadmap (#420)
title: "[EPIC-Tn] "
labels: []
---

Epic: #
Roadmap: #420

## Implementation Order

- Global roadmap position:
- Within epic:
- Depends on:
- Estimated effort:

## Problem

## Scope

## Acceptance Criteria

- [ ]

## Self-Host Compiler

Not required. The self-host compiler is frozen as a bootstrap demonstrator (#447, `docs/selfhost/scope-decision.md`). Implement this issue in the Rust compiler, std and runtime only, and keep `make selfhost-frozen-check`, `make test-selfhost` and `make selfhost-retirement-audit` green.

## Definition of Done (repository policy, see AGENTS.md)

- [ ] All scope items and acceptance criteria are fully implemented in code (no stub, dummy, TODO, placeholder or fake success path).
- [ ] Changed components compile; `aic check` and `aic build` agree for every new test program.
- [ ] Unit tests cover the change, including edge cases and failure paths; existing tests still pass.
- [ ] `make ci` passes locally, plus targeted tests for the changed subsystem.
- [ ] User-facing docs (`docs/reference/*`, `docs/diagnostic-codes.md`, `README.md` where relevant) and AI-agent docs are updated.
- [ ] An example is added or updated under `examples/` when applicable and passes `make examples-check` / `make examples-run`.
- [ ] `rg -n "TODO|dummy|stub|unimplemented|panic\(\"todo|FIXME" <touched paths>` is clean.
- [ ] Self-host sources still pass `make selfhost-frozen-check`; no roadmap-only syntax is added to files listed in `tests/selfhost/*_manifest.json`.
- [ ] Changes are committed and pushed; this issue is updated with evidence (commit hash, tests run, docs/examples) before it is closed.
