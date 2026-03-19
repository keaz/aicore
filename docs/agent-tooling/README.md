# Agent Tooling Docs (AG-T6)

Machine-first reference for agent-native, IR-first compiler, diagnostic, and editor integration.

Development note: this project has been developed mainly using **GPT-5.3-Codex** as the primary implementation agent.

## Versioned protocol

- Protocol contract: [protocol-v1.md](protocol-v1.md)
- Schemas:
  - [parse-response.schema.json](schemas/parse-response.schema.json)
  - [ast-response.schema.json](schemas/ast-response.schema.json)
- [check-response.schema.json](schemas/check-response.schema.json)
- [build-response.schema.json](schemas/build-response.schema.json)
- [fix-response.schema.json](schemas/fix-response.schema.json)
- [testgen-response.schema.json](schemas/testgen-response.schema.json)
- [session-response.schema.json](schemas/session-response.schema.json)
- [patch-response.schema.json](schemas/patch-response.schema.json)
- [validate-call-response.schema.json](schemas/validate-call-response.schema.json)
- [validate-type-response.schema.json](schemas/validate-type-response.schema.json)
- [suggest-response.schema.json](schemas/suggest-response.schema.json)
- [context-response.schema.json](schemas/context-response.schema.json)
- [query-response.schema.json](schemas/query-response.schema.json)
- [symbols-response.schema.json](schemas/symbols-response.schema.json)
- Patch authoring schema: [patch-request.schema.json](schemas/patch-request.schema.json)
- [`docs/diagnostics.schema.json`](../diagnostics.schema.json) (shared raw `aic check --json` / `aic diag --json` diagnostics array)

Canonical surface note:

- `check-response.schema.json` and `build-response.schema.json` apply to daemon JSON-RPC `check`/`build` `result` payloads.
- CLI `aic check --json` / `aic diag --json` use `docs/diagnostics.schema.json`.

Diagnostic transport note:

- `diagnostics[*].reasoning` is optional and versioned by `reasoning.schema_version`.
- When the field is absent, treat that as the stable fallback for unsupported diagnostic families.

## Tooling workflows

- LSP capabilities and examples: [`examples/agent/lsp_workflow.json`](../../examples/agent/lsp_workflow.json)
- Incremental daemon behavior/troubleshooting: [incremental-daemon.md](incremental-daemon.md)
- Agent cookbook end-to-end loops: [`docs/agent-recipes/`](../agent-recipes/)

## Agent-first playbooks

- Language feature guidance (when/how to use each implemented feature):
  - [language-feature-playbook.md](language-feature-playbook.md)
- Full CLI command decision playbook:
  - [aic-command-playbook.md](aic-command-playbook.md)
- Scaffold command guide with exact command/output pairs:
  - [scaffold-guide.md](scaffold-guide.md)
- Patch authoring guide:
  - [patch-authoring.md](patch-authoring.md)
- Deep command guides:
  - [commands/aic-init.md](commands/aic-init.md)
  - [commands/aic-lsp.md](commands/aic-lsp.md)
  - [commands/aic-diff.md](commands/aic-diff.md)

## Core commands

- `aic contract --json`
- `aic ast --json <path>`
- `aic check <path> --json`
- `aic context --for function <name> --depth <n> --limit <n> --project examples/e7/context_query --json`
- `aic query --kind function --name 'validate*' --module demo.search --has-contract --project examples/e7/symbol_query --json`
- `aic symbols --project examples/e7/symbol_query --json`
- `aic scaffold fn process_user --param u:User --return 'Result[Int, AppError]' --effect io --capability io --requires 'u.age >= 0' --ensures 'true'`
- `aic validate-call <target> --arg <type> --project .`
- `aic validate-type <type_expr> --project .`
- `aic suggest --partial <text> --project . --limit <n>`
- `aic synthesize --from spec <name> --project . --json`
- `aic testgen --strategy boundary --for function <name> --project . --json`
- `aic checkpoint diff <checkpoint> [--to <checkpoint>] --project . --json`
- `aic session merge plans/valid_plan.json --project examples/e7/session_protocol --json`
- `aic patch --preview patches/valid_patch.json --project examples/e7/patch_protocol --json`
- `aic diag apply-fixes <path> --dry-run --json`
- `aic lsp`
- `aic daemon`

Fast-path budget for hallucination-prevention commands:

- `aic validate-call`, `aic validate-type`, and `aic suggest --partial` are front-end-only checks.
- They may parse, resolve, consult the symbol index, and rank candidates.
- They must not trigger codegen, execution, artifact writes, or daemon/session mutation.

## Validation gates

- Schema and fixture validation: `tests/agent_protocol_tests.rs`
- Recipe docs-as-tests: `tests/agent_recipe_tests.rs`
- LSP/autofix/daemon integration tests: `tests/lsp_smoke_tests.rs`, `tests/e7_cli_tests.rs`

## Docs Validation Checklist

Before merging command/feature documentation updates:

1. Confirm command surface against `src/main.rs` and `docs/cli-contract.md`.
2. Validate diagnostic references against `docs/diagnostic-codes.md`.
3. Ensure README and `docs/agent-tooling/README.md` link to any new agent docs.
4. Verify examples/command snippets use current flag shapes (`aic <command> --help`).
5. Keep guaranteed behavior in `docs/reference/*`; keep forward-looking items in `docs/reference/open-issue-contracts.md`.

## Epic #62 proof-of-completion checklist (open)

Use this checklist when preparing closure evidence for epic `#62`. Keep the epic open until every item below is complete and evidenced.

- [ ] Protocol docs + schemas match implemented behavior: `docs/agent-tooling/protocol-v1.md`, `docs/agent-tooling/schemas/parse-response.schema.json`, `docs/agent-tooling/schemas/ast-response.schema.json`, `docs/agent-tooling/schemas/check-response.schema.json`, `docs/agent-tooling/schemas/build-response.schema.json`, `docs/agent-tooling/schemas/fix-response.schema.json`, `docs/agent-tooling/schemas/testgen-response.schema.json`, `docs/agent-tooling/schemas/session-response.schema.json`, `docs/agent-tooling/schemas/patch-response.schema.json`, `docs/agent-tooling/schemas/patch-request.schema.json`, `docs/agent-tooling/schemas/validate-call-response.schema.json`, `docs/agent-tooling/schemas/validate-type-response.schema.json`, `docs/agent-tooling/schemas/suggest-response.schema.json`, `docs/agent-tooling/schemas/context-response.schema.json`, `docs/agent-tooling/schemas/query-response.schema.json`, `docs/agent-tooling/schemas/symbols-response.schema.json`
- [ ] Daemon docs reflect current incremental behavior and troubleshooting: `docs/agent-tooling/incremental-daemon.md`
- [ ] LSP workflow example is current and runnable: `examples/agent/lsp_workflow.json`
- [ ] Agent recipes are current for end-to-end loops: `docs/agent-recipes/`
- [ ] Test gate run: `make test-e7`
- [ ] Relevant test files are green: `tests/agent_protocol_tests.rs`, `tests/agent_recipe_tests.rs`, `tests/lsp_smoke_tests.rs`, `tests/e7_cli_tests.rs`
- [ ] Epic closure comment contains evidence: commit hash, commands run (`make test-e7`), and touched docs/examples/tests
