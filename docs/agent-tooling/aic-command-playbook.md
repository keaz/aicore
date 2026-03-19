# Agent-First `aic` Command Playbook

Purpose: choose the right `aic` command at the right stage of an autonomous workflow.

Implementation source: `src/main.rs` (command surface), `docs/cli-contract.md` (stability contract).

## Command selection flow

1. Need parse/type/effect feedback: use `aic check --json`.
2. Need fast hallucination-prevention checks before drafting a call/type: use `aic validate-call`, `aic validate-type`, or `aic suggest --partial`.
3. Need deterministic edits from diagnostics: use `aic diag apply-fixes --dry-run --json`.
4. Need focused transitive context around a symbol: use `aic context --for function <name> --depth <n> --limit <n> --json`.
5. Need a spec-first implementation preview: use `aic synthesize --from spec <name> --project . --json`.
6. Need deterministic harness fixtures from contracts/types/effects: use `aic testgen --strategy <strategy> --for <selector> --project . --json`.
7. Need a reversible workspace snapshot before or after edits: use `aic checkpoint create --project . --json`, then `aic checkpoint diff`/`restore`.
8. Need multi-agent lock ownership or merge validation before applying queued patches: use `aic session lock ...`, `aic session conflicts <plan.json> --project . --json`, or `aic session merge <plan.json> --project . --json`.
9. Need structured symbol-aware edits: use `aic patch --preview <patch.json> --json` and author documents against `docs/agent-tooling/patch-authoring.md`.
10. Need IR/source normalization: use `aic fmt` and `aic ir --emit json`.
11. Need executable artifact: use `aic build` (or `aic run` for compile+execute).
12. Need semantic compatibility gate: use `aic diff --semantic --fail-on-breaking`.
13. Need interactive editor integration: use `aic lsp`.

## Decision matrix for autonomous loops

| Loop stage | Primary command | Success signal | Failure handling path |
|---|---|---|---|
| Parse/type/effect validation | `aic check <entry> --json` | Exit `0`, diagnostics array has no errors | Use `aic explain <code>` and `diagnostics[*].reasoning` when present, then apply targeted edits |
| Hallucination preflight | `aic validate-call <target> --arg <type> ... --project .`, `aic validate-type <type_expr> --project .`, `aic suggest --partial <text> --project . --limit <n>` | `ok: true` with resolved callable/type or ranked candidates | Adjust call/type text from `diagnostics[]`/`suggestions[]`, then retry before entering a full compile loop |
| Context-window minimization | `aic context --for function <name> --depth <n> --limit <n> --json` | Target signature + ranked `dependencies[]`/`callers[]` | Narrow selector or reduce depth; resolve ambiguity errors |
| Spec-first skeleton synthesis | `aic synthesize --from spec <name> --project . --json` | Function + attribute-test fixture artifacts emitted deterministically | Fix malformed spec clauses or missing/ambiguous signature types using the reported spec-file span/remediation hints, then re-run |
| Harness fixture generation | `aic testgen --strategy <strategy> --for <selector> --project . --json` | Strategy-specific fixture artifacts emitted deterministically | Adjust selector/strategy pair or simplify unsupported contracts/invariants |
| Checkpoint safety rail | `aic checkpoint create --project . --json` then `aic checkpoint diff <id> [--to <id>] --project . --json` | Stable checkpoint id plus deterministic file/hash/semantic diff summary | Use `aic checkpoint restore <id> --project . --json` to revert checkpointed files; corruption returns non-zero with no partial restore |
| Multi-agent coordination gate | `aic session conflicts <plan.json> --project . --json` then `aic session merge <plan.json> --project . --json` | Conflict-free plan plus merge validation with no frontend errors | Acquire/reclaim the required symbol locks, fix `conflicts[]`/`diagnostics[]`, and re-run |
| Safe autofix planning | `aic diag apply-fixes <entry> --dry-run --json` | `ok: true` with deterministic edit plan | Resolve conflicts manually, re-run dry-run |
| Structured patch planning | `aic patch --preview <patch.json> --json` | `ok: true`, non-empty `applied_edits[]`/`previews[]` | Resolve reported `conflicts[]`, re-run preview; keep request shape aligned with `docs/agent-tooling/patch-authoring.md` |
| Canonical source shape | `aic fmt <entry> --check` | Exit `0` | Run `aic fmt <entry>`, re-check |
| Semantic compatibility gate | `aic diff --semantic <old> <new> --fail-on-breaking` | Exit `0`, `summary.breaking == 0` | Inspect `changes[]`, adjust API/contracts/effects |
| Build artifact production | `aic build <entry> ...` | Exit `0`, artifact emitted | Re-run `aic check --json`; fix frontend/backend diagnostics |
| Runtime confirmation | `aic run <entry> ...` | Exit `0` | Use sandbox/profile flags + `aic check --json` for root cause |
| Editor-assisted refactor loop | `aic lsp` | JSON-RPC initialize + diagnostics stream | Fall back to one-shot `check/fmt` commands |

## Machine-output expectations

These are the command outputs automation should parse directly:

| Command | Machine-oriented output contract |
|---|---|
| `aic check --json` | JSON diagnostics array (`code`, `severity`, `message`, `spans`, `help`, `suggested_fixes`, optional `reasoning`) |
| `aic check --sarif` | SARIF 2.1.0 JSON document |
| `aic check --show-holes` | Typed-hole JSON (`holes[]` with line/inferred/context) |
| `aic validate-call` | Fast-path callable conformance JSON (`resolved`, `suggestions`, `diagnostics`) |
| `aic validate-type` | Fast-path type conformance JSON (`canonical`, `named_types`, `diagnostics`) |
| `aic suggest --partial` | Ranked symbol candidate JSON (`candidate_count`, `candidates[]`) |
| `aic diag apply-fixes --json` | Autofix plan/apply JSON (`ok`, `applied_edits`, `conflicts`) |
| `aic context --json` | Context window JSON (`signature`, `target`, `dependencies`, `callers`, `contracts`, `related_tests`) |
| `aic synthesize --json` | Spec-first artifact JSON (`spec_file`, `artifacts[]`, `notes[]`); failures report original spec-file spans and remediation hints on stderr |
| `aic testgen --json` | Harness-generation JSON (`strategy`, `seed`, `target`, `artifacts[]`, `notes[]`) |
| `aic checkpoint --json` | Checkpoint JSON (`checkpoint`, `checkpoints[]`, `summary`, `files[]`, `restored_paths[]`) |
| `aic session --json` | Session JSON (`session`, `sessions[]`, `locks[]`, `operations[]`, `conflicts[]`, `diagnostics[]`) |
| `aic patch --json` | Structured patch JSON (`ok`, `applied_edits`, `previews`, `conflicts`) |
| `aic ast --json` | AST+IR response including type/effect/import metadata |
| `aic ir --emit json` | Canonical IR JSON |
| `aic impact` | Impact report JSON (`direct_callers`, `transitive_callers`, `affected_tests`, ...) |
| `aic suggest-effects` | Effect/capability suggestion JSON |
| `aic suggest-contracts --json` | Contract suggestion JSON |
| `aic metrics` / `aic coverage` / `aic bench` | Structured report JSON with optional threshold gating |
| `aic diff --semantic` | Semantic change JSON (`changes[]`, `summary.breaking`, `summary.non_breaking`) |
| `aic contract --json` | CLI compatibility contract JSON |
| `aic lsp` / `aic daemon` | JSON-RPC 2.0 over stdio |

Reasoning metadata notes:

- `diagnostics[*].reasoning` is optional; absence means the diagnostic has no published reasoning strategy pack yet.
- When present, `reasoning.schema_version` is currently `1.0` and `hypotheses[]` are ordered by descending confidence.

## Contract-stable command families

### Bootstrap and environment

| Command | Use when | Key output mode |
|---|---|---|
| `aic init [path]` | Creating a fresh project scaffold | Text status |
| `aic check [input] --json` | Core compile-time validation loop | Diagnostics JSON |
| `aic diag [input] --json` | Diagnostics stream without full check wrapper | Diagnostics JSON |
| `aic diag apply-fixes [input] --dry-run --json` | Planning safe automated edits | Planned edit JSON |
| `aic explain <code> [--json]` | Translating error codes into fix intent | Text/JSON explanation |
| `aic fmt [input] [--check]` | Deterministic formatting gate (canonical string escapes: `\\n`, `\\r`, `\\t`, `\\0`, `\\u{...}`) | Exit code + file rewrite/check |
| `aic ir [input] --emit json|text` | Inspecting canonical frontend IR | IR JSON/text |
| `aic impact <function> [input]` | Blast-radius estimation for refactors | JSON impact report |
| `aic suggest-effects <input>` | Inferring missing effects/capabilities | JSON suggestions |
| `aic suggest-contracts <input> [--json]` | Contract proposal generation | Text/JSON suggestions |
| `aic validate-call <target> --arg <type> ... [--project <path>] [--offline]` | Fast-path callable existence and argument compatibility check | JSON validation report |
| `aic validate-type <type_expr> [--project <path>] [--offline]` | Fast-path type-expression parsing and symbol visibility check | JSON validation report |
| `aic suggest --partial <text> [--project <path>] [--limit <n>]` | Ranked symbol suggestion for partial/hallucinated names | JSON candidate report |
| `aic context --for ... [--depth N] [--limit N] [--json]` | Focused transitive symbol context window | Text/JSON context report |
| `aic query [--kind ... --name ... --module ...]` | Semantic symbol retrieval by kind/name/module/effects/contracts/generics | Text/JSON query envelope |
| `aic symbols [--format text|json]` | Full workspace symbol export with contract-aware records | Text/JSON symbols envelope |
| `aic scaffold struct|enum|fn|match|test ...` | Generate compile-clean boilerplate templates for the selected target | Text/JSON scaffold payload |
| `aic synthesize --from spec <name> [--json]` | Spec-first function + test fixture synthesis preview | Text/JSON artifact bundle |
| `aic testgen --strategy <strategy> --for <selector> [--emit-dir <dir>] [--json]` | Deterministic harness fixture generation from contracts/types/effects | Text/JSON artifact bundle |
| `aic checkpoint create/list/restore/diff ...` | Deterministic workspace snapshot, diff, and rollback protocol | Text/JSON checkpoint responses |
| `aic session create/list/lock/conflicts/merge ...` | Collaboration session registry, symbol lock leasing, overlap detection, and validation-only merge protocol | Text/JSON session responses |
| `aic patch --preview|--apply <patch.json>` | Structured add/modify edits by symbol intent; request schema lives in `docs/agent-tooling/patch-authoring.md` | Text/JSON patch response |
| `aic metrics <input> [--check]` | Complexity/perf guardrails in CI | JSON metrics/check status |
| `aic ir-migrate <ir.json>` | Upgrading legacy IR snapshots | Migrated IR JSON |
| `aic migrate [path] [--json]` | Source/IR migration planning | Migration summary/JSON |
| `aic lock [path]` | Deterministic dependency lock materialization | Lockfile generation status |
| `aic pkg publish/search/install ...` | Package registry lifecycle | Text/JSON results |
| `aic build [input] ...` | Producing executable/object/library artifacts | Artifact + optional manifest/hash checks |
| `aic doc [input] --output <dir>` | API doc generation | Generated docs path |
| `aic std-compat --check` | Std API compatibility gate | Text/JSON compatibility report |
| `aic diff --semantic <old> <new>` | Semantic API delta between versions | Deterministic JSON report |
| `aic lsp` | Language-server mode for editors/agents | JSON-RPC over stdio |
| `aic debug dap [--adapter <path>]` | Debug adapter bridge | DAP process bridge |
| `aic daemon` | Incremental check/build JSON-RPC server | JSON-RPC over stdio |
| `aic test [path] [--json]` | Fixture + attribute/property test harness | Text/JSON harness report |
| `aic contract --json` | CLI protocol negotiation/compatibility | Contract JSON |
| `aic release ...` | Manifest/SBOM/provenance/policy/LTS/security ops | Text/JSON release outputs |
| `aic run [input] ...` | Compile and execute with sandbox/runtime options | Program output + exit status |

## Fast-path budget

- `aic validate-call`, `aic validate-type`, and `aic suggest --partial` are budgeted to stay on parser/resolver/typechecker and symbol-index paths.
- Treat any future codegen, execution, artifact writes, or session/daemon mutation in these commands as a regression.
- Keep `aic suggest --partial` candidate ranking deterministic and bounded by `--limit` (default `8`).

## Additional implemented commands (outside stable list)

| Command | Use when | Notes |
|---|---|---|
| `aic setup [--std-root <path>]` | Installing std toolchain files | Required in clean environments before std imports work reliably |
| `aic ast <input> --json` | Returning AST + IR + resolved metadata bundle | Requires `--json` |
| `aic coverage <input> [--check --min <pct>]` | Coverage tracking and thresholds | JSON report, optional file write |
| `aic bench [--budget ...]` | Perf gate execution with trend report | JSON payload + optional compare baseline |
| `aic repl [--json]` | Interactive expression/function effect probing | Session protocol useful for tooling |
| `aic grammar --ebnf|--json` | Grammar artifact export | Contract/EBNF output |

## High-value automation patterns

### Pattern: fast fix loop

```bash
aic check src/main.aic --json
aic diag apply-fixes src/main.aic --dry-run --json
aic fmt src/main.aic --check
aic check src/main.aic --json
```

### Pattern: refactor safety gate

```bash
aic check src/main.aic --json
aic diff --semantic before/main.aic after/main.aic --fail-on-breaking
aic test . --json
```

### Pattern: editor+batch hybrid

```bash
# long-lived process
aic lsp

# CI/batch fallback
aic check src/main.aic --json
aic fmt src/main.aic --check
```

## Deep dives

- [`aic init`](commands/aic-init.md)
- [`aic lsp`](commands/aic-lsp.md)
- [`aic diff --semantic`](commands/aic-diff.md)
