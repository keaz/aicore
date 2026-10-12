# Self-Host Compiler Scope Decision

Status: Accepted (2026-10-11)

Issue: `#447` (REL-T5). Roadmap: `#420`. Related: `#419` (Rust reference compiler retirement).

## Decision

The self-host compiler in `compiler/aic/` is frozen as a bootstrap demonstrator with a fixed feature subset. The Rust compiler in `src/` is the only reference implementation and the default compiler for every user program. Language production-readiness roadmap issues (`#429` to `#498`) change the Rust compiler only. They do not require a self-host implementation.

## Context

The language production-readiness audit (2026-10) measured the gap between the two compilers:

- `aic check` passes on 35 of 36 core examples with the Rust compiler and on 13 of 36 with the self-host compiler.
- Self-host backend output references 24 runtime symbols. The Rust backend references 446.
- The self-host compiler recognises none of the 423 std intrinsic names.
- The self-host driver rejects `--release`, `--target` and `-O`.

The roadmap adds features to every compiler phase: memory management, closures, traits, checked arithmetic, effect polymorphism and more. Full parity would require each feature to be implemented twice, in Rust and in AICore, while the self-host compiler still has to close the gap above. That roughly doubles the cost of every roadmap issue and delays the user-facing work. Freezing the self-host compiler keeps the bootstrap story (stage0, stage1 and stage2 builds of `compiler/aic/tools/aic_selfhost`) without making it a second product.

## The Frozen Feature Subset

The frozen subset is the language surface that the self-host gates covered on the decision date:

- the cases in `tests/selfhost/parity_manifest.json`, `tests/selfhost/rust_vs_selfhost_manifest.json` and `tests/selfhost/stage_matrix_manifest.json`;
- the self-host compiler sources themselves, so that `make selfhost-bootstrap` can still build stage0, stage1 and stage2.

Language features added by the roadmap are outside the subset. The self-host compiler may reject them with its existing parse, resolve or typecheck diagnostics. The self-host compiler must not be selected for user programs: the controlled `default` mode stays limited to the unmodified `aic build compiler/aic/tools/aic_selfhost -o <artifact>` shape described in `docs/selfhost/supported-operation-runbook.md`.

## Rules For Each Roadmap Issue

1. Implement the issue in the Rust compiler, std and runtime only. Do not add the feature to `compiler/aic/`.
2. Keep the frozen self-host sources valid for the Rust compiler. `make selfhost-frozen-check` runs `aic check` on every package under `compiler/aic/libs/` and `compiler/aic/tools/`, and `make ci` runs it. When a new checker rule rejects self-host source, fix that source in the same issue with the smallest change that keeps its behaviour.
3. Keep `make test-selfhost` and `make selfhost-retirement-audit` green. Classify every new tracked Rust file in `docs/selfhost/rust-reference-retirement.v1.json`.
4. The self-host manifests point at sources such as `examples/core/*.aic`. Do not put roadmap-only syntax into those files; add a new example instead. When a change intentionally alters the result for a manifest case (for example a new checker rule rejects it), update the case's `expected` field in the same issue and say why in the commit message. Do not delete a manifest case to make a gate pass.
5. State "Self-host: not required (frozen, see `docs/selfhost/scope-decision.md`)" in the issue evidence.

## Consequences

- `#419` stays deferred. The Rust reference compiler cannot be retired while the self-host compiler is frozen, and `make selfhost-retirement-audit` keeps reporting `status=deferred`.
- The self-host compiler falls further behind the language as the roadmap lands. Documentation must not describe it as a general-purpose compiler.
- Self-host bug fixes are accepted only when they keep the bootstrap or an existing manifest case working.

## Revisiting The Decision

Unfreezing the self-host compiler requires a new decision record in this directory that replaces this one. The new record must list parity milestones tied to roadmap issues, the gates that prove each milestone, and the plan for `#419`.
