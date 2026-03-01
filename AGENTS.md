# AGENTS.md

This file defines mandatory execution rules for autonomous agents working in this repository.

## Scope

- Applies to the entire repository.
- Applies to all epics/issues and all GitHub projects, including Project 6: `AICore Complete IO Runtime`.

## Non-Negotiable Rule: No Placeholder Completions

- Never mark an issue as Done if any part is still a stub, dummy, placeholder, TODO, or partial workaround.
- Never ship "fake" success paths (constant return values, no-op implementations, unreachable code pretending to be implemented).
- If implementation is partial, keep the issue open and explicitly report remaining work.

## Issue Implementation Workflow (Required)

Use this sequence for every issue. Do not mark the issue as Done until all steps are complete:

1. Review the GitHub issue and confirm all Definition of Done (DoD) items and acceptance criteria are understood.
2. Implement the full scope in code so all issue acceptance criteria are actually met.
3. Compile/build the changed code successfully (no compile errors).
4. Add or update unit tests that cover the changed behavior, including edge cases and failure paths.
5. Run tests and confirm both new and existing tests pass.
6. Update documentation; add new documentation files when needed.
7. Add or update a working example for the implemented issue when feasible, and validate it runs.
8. Commit and push only the changes related to the issue.
9. Update the GitHub issue with implementation evidence and then set it to Done.

## Definition Of Done (Required Before Closing Any Issue)

An issue can be closed and moved to Done only when all of the following are true:

1. All issue DoD requirements and acceptance criteria are fully implemented in code (not just scaffolding).
2. No placeholder implementation remains in touched paths.
3. Code compiles/builds successfully for the changed components.
4. New unit tests are added/updated to cover behavior, including edge cases and negative/failure paths.
5. Both new and existing tests pass locally (`make ci` plus targeted tests).
6. Documentation is updated (user-facing docs + AI-agent implementation docs when relevant); new docs are added when required.
7. Examples are added/updated (when applicable) and compile/run as expected.
8. Changes are committed and pushed.
9. GitHub issue is updated with implementation evidence (commit hash, tests run, docs/examples added) and then set to Done.

## Mandatory Verification Before Issue Closure

Run these checks at minimum:

- Build/compile verification for changed components (no compile errors)
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
