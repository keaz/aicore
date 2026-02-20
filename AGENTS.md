# AGENTS.md

This file defines mandatory execution rules for autonomous agents working in this repository.

## Scope

- Applies to the entire repository.
- Applies to all epics/issues and all GitHub projects, including Project 6: `AICore Complete IO Runtime`.

## Non-Negotiable Rule: No Placeholder Completions

- Never mark an issue as Done if any part is still a stub, dummy, placeholder, TODO, or partial workaround.
- Never ship "fake" success paths (constant return values, no-op implementations, unreachable code pretending to be implemented).
- If implementation is partial, keep the issue open and explicitly report remaining work.

## Definition Of Done (Required Before Closing Any Issue)

An issue can be closed and moved to Done only when all of the following are true:

1. The full issue scope and acceptance criteria are implemented in code (not just scaffolding).
2. No placeholder implementation remains in touched paths.
3. Tests are added/updated to prove behavior, including negative/failure paths where applicable.
4. Examples are added/updated (when applicable) and compile/run as expected.
5. Documentation is updated (user-facing docs + AI-agent implementation docs when relevant).
6. `make ci` passes locally with zero failures.
7. Changes are committed and pushed.
8. GitHub issue comment includes implementation evidence (commit hash, tests run, docs/examples added).
9. Only after steps 1-8: close issue and set project status to Done.

## Mandatory Verification Before Issue Closure

Run these checks at minimum:

- `make ci`
- Targeted tests for the changed subsystem(s)
- Example validation for changed behavior (`make examples-check` and/or `make examples-run` when applicable)

And verify no obvious placeholder markers were introduced in touched files:

- `rg -n "TODO|dummy|stub|unimplemented|panic\(\"todo|FIXME" <touched paths>`

## Project 6 / IO-Specific Guardrails

For IO-runtime issues (including `[IO-T1] Filesystem API completion`):

- All public API methods in `std/fs.aic` must have real behavior, not placeholders.
- Cross-platform limitations must be explicit and documented.
- Error paths must be intentional and test-covered.
- Do not mark IO issues Done if any exported API remains dummy-implemented.

## Status Update Policy

- "In Progress": active implementation, incomplete acceptance criteria.
- "Done": only after full DoD completion above.
- If partially implemented, add a progress comment and keep issue open.

## Quality Bar

- Prefer complete, correct implementations over broad but shallow changes.
- Do not optimize for board movement; optimize for production readiness.
