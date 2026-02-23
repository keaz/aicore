# Projects #13-#23 Execution Plan (Multi-Agent)

Date: 2026-02-23  
Planning owner: Backlog orchestration  
Scope: newly added 11 projects (`#13` to `#23`) with explicit coverage for 52 tracked issues.

## 1) Backlog accounting and explicit coverage

| Project | Title | Issue range | Count | Current state |
| --- | --- | --- | --- | --- |
| #13 | Production-Ready IO Module | #115-#123 | 9 | Closed baseline |
| #14 | Core Language Features | #125-#130 | 6 | Closed baseline |
| #15 | Standard Library Completeness | #131-#135 | 5 | Closed baseline |
| #16 | Type System and Safety | #136-#139 | 4 | Closed baseline |
| #17 | Tooling and Developer Experience | #140-#145 | 6 | Closed baseline |
| #18 | Documentation and Learning | #146-#150 | 5 | Closed baseline |
| #19 | Concurrency and Async Runtime | #151-#153 | 3 | Open execution queue |
| #20 | Error Handling and Diagnostics | #154-#156 | 3 | Open execution queue |
| #21 | Memory Management and Safety | #157-#159 | 3 | Open execution queue |
| #22 | Ecosystem and Interop | #160-#162 | 3 | Open execution queue |
| #23 | Production Hardening | #163-#166 | 4 | Open execution queue |

Tracked totals:
- Project issues: 51 total (`35` closed, `16` open).
- Carry-over dependency context: `#124` (must remain in closeout evidence).
- Plan coverage target: `52/52` tracked issues accounted for.

## 2) Multi-agent operating model and parallelism limits

Execution model:
- One issue owner agent at a time, one active issue per agent.
- Maximum global parallelism: `4` active implementation issues.
- Maximum high-contention parallelism: `2` active issues touching shared runtime/compiler core paths at the same time.
- No parallel edits in overlapping path ownership zones unless a path split is declared before coding.
- Issue status must remain `In Progress` until all AGENTS.md completion gates pass with evidence.

Path ownership zones for conflict control:
- Runtime core lane: async runtime, scheduler, execution internals.
- Error/diagnostics lane: panic, error catalog, diagnostics surfaces.
- Memory/type safety lane: ownership, drop/RAII, related type behavior.
- Ecosystem/hardening lane: interop, package/runtime security hardening.

## 3) Dependency-driven batching and wave plan

Dependency anchors:
- Primitive blockers: `#151`, `#154`, `#157`.
- Key dependency edges: `#152<-#151`, `#153<-#151,#152`, `#155<-#154`, `#156<-#154,#155`, `#158<-#157`, `#159<-#157`, `#160<-#157,#155`, `#161<-#151,#160`, `#162<-#160`, `#163<-#154,#155`, `#164<-#151,#157,#163`, `#165<-#155`, `#166<-#151,#162,#163`.

| Wave | Batch goal | Issue set | Max concurrent agents | Entry criteria | Exit criteria |
| --- | --- | --- | --- | --- | --- |
| Wave 0 | Ownership and lock planning | Open queue setup (`#151-#166`) | 0 coding, planning only | Start criteria in Section 6 met | Owners assigned, path zones declared, dependency map confirmed |
| Wave 1 | Primitive blockers | `#151`, `#154`, `#157` | 3 | Wave 0 complete | All 3 issues pass gates and evidence checklist |
| Wave 2A | First-order dependents | `#152`, `#155`, `#158`, `#159` | 4 | Wave 1 complete | All 4 issues pass gates and evidence checklist |
| Wave 2B | Second-order dependents | `#153`, `#156`, `#163`, `#165` | 4 | Required 2A parents complete (`#152`, `#155`) | All 4 issues pass gates and evidence checklist |
| Wave 3A | Integration prerequisites | `#160`, `#164` | 2 | `#155`, `#157`, `#163` complete | Both issues pass gates and evidence checklist |
| Wave 3B | Ecosystem follow-through | `#161`, `#162` | 2 | `#160` complete | Both issues pass gates and evidence checklist |
| Wave 3C | Final hardening | `#166` | 1 | `#162` and `#163` complete | Issue passes gates and evidence checklist |
| Wave 4 | Done-state closure sweep | All open issues (`#151-#166`) | 2 reviewers | All implementation waves complete | All issue comments contain required evidence; only then move to `Done` |

Batching notes:
- If a wave finishes early, do not pull work from later waves unless all listed dependencies are complete.
- If an issue fails gates, it stays in the same wave and blocks dependent work.
- Closed baseline projects (`#13-#18`) remain immutable inputs unless a blocking regression is discovered.

## 4) AGENTS.md required validation gates before `Done`

Each issue must satisfy all gates before closure:

1. Full acceptance criteria implemented in code, no partial/scaffold-only completion.
2. Placeholder scan in touched paths:
   - `rg -n "TODO|dummy|stub|unimplemented|panic\\(\"todo|FIXME" <touched paths>`
3. Targeted subsystem tests pass, including failure-path/negative-path coverage where applicable.
4. Example validation passes when behavior is user-facing:
   - `make examples-check`
   - `make examples-run`
5. Documentation updates are present:
   - user-facing docs
   - AI-agent implementation docs when relevant
6. Full CI passes with zero failures:
   - `make ci`
7. Changes are committed and pushed.
8. Issue comment includes evidence:
   - commit hash
   - commands/tests run
   - examples added/updated
   - docs added/updated
9. Only after gates 1-8: close issue and move project card to `Done`.

## 5) Validation command matrix by issue family

| Issue family | Minimum targeted tests | Minimum docs touchpoints |
| --- | --- | --- |
| #115-#123 | `make test-unit` + `make test-exec` + `make test-e7` | `docs/io-filesystem.md`, `docs/io-api-reference.md`, `docs/io-agent-guide.md` |
| #124 (carry-over evidence context) | `make test-e7` + `make docs-check` | `docs/ai-agent-rest-guide.md` |
| #125-#130 | `make test-unit` + `make test-golden` + `make test-exec` | `docs/spec.md`, `docs/syntax.md` |
| #131-#135 | `make test-unit` + `make test-exec` | `docs/std-compatibility.md`, std module docs touched by scope |
| #136-#139 | `make test-unit` + `make test-golden` + `make test-exec` | `docs/type-system.md`, `docs/contracts.md` |
| #140-#145 | `make test-e7` + `make test-e8` | `docs/ide-integration.md`, `docs/agent-tooling/README.md` |
| #146-#150 | `make docs-check` + `make test-e7` | Topic-specific `docs/` pages touched by scope |
| #151-#153 | `make test-unit` + `make test-exec` + `make test-e7` | `docs/io-concurrency-runtime.md`, `docs/io-runtime/README.md` |
| #154-#156 | `make test-unit` + `make test-e7` + `make test-exec` | `docs/diagnostic-codes.md`, `docs/errors/catalog.md` |
| #157-#159 | `make test-unit` + `make test-exec` + `make test-e8` | `docs/type-system.md`, runtime memory-safety sections |
| #160-#162 | `make test-exec` + `make test-e9` | `docs/package-ecosystem/ffi-and-supply-chain.md`, interop target docs |
| #163-#166 | `make test-e8` + `make test-e9` + `make test-exec` | `docs/release-security-ops.md`, `docs/security-ops/README.md` |

Global rule:
- `make examples-check`, `make examples-run`, and `make ci` are required for every issue before `Done`.
- If any command is truly not applicable for strict scope reasons, the issue comment must explicitly justify the exception with evidence.

## 6) Start after current work done (go/no-go readiness criteria)

This execution plan starts only after all current active work outside this backlog handoff is cleared.

Readiness checklist:
1. No unresolved in-progress implementation branches that overlap the open queue paths (`#151-#166` impact zones).
2. Mainline is green and stable (latest CI run passed with no required check failures).
3. Wave 1 staffing is confirmed for `#151`, `#154`, `#157`, with one backup owner for each.
4. Dependency labels and acceptance criteria are present on each open issue before assignment.
5. Path ownership zones are published for conflict prevention before coding starts.
6. Command environment is ready for mandatory gates: targeted tests, examples validation, docs updates, and `make ci`.
7. Done-evidence template is prepared so no issue can be closed without commit/tests/docs/examples proof.

Start condition:
- Begin Wave 0 immediately once all readiness items are true.
- Begin Wave 1 on the next working block after Wave 0 ownership lock completes.
