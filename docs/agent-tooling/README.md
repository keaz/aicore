# Agent Tooling Docs (AG-T6)

Machine-first reference for autonomous compiler/diagnostic/editor integration.

Development note: this project has been developed mainly using **GPT-5.3-Codex** as the primary implementation agent.

## Versioned protocol

- Protocol contract: `docs/agent-tooling/protocol-v1.md`
- Schemas:
  - `docs/agent-tooling/schemas/parse-response.schema.json`
  - `docs/agent-tooling/schemas/ast-response.schema.json`
- `docs/agent-tooling/schemas/check-response.schema.json`
- `docs/agent-tooling/schemas/build-response.schema.json`
- `docs/agent-tooling/schemas/fix-response.schema.json`
- `docs/agent-tooling/schemas/testgen-response.schema.json`

## Tooling workflows

- LSP capabilities and examples: `examples/agent/lsp_workflow.json`
- Incremental daemon behavior/troubleshooting: `docs/agent-tooling/incremental-daemon.md`
- Agent cookbook end-to-end loops: `docs/agent-recipes/`

## Agent-first playbooks

- Language feature guidance (when/how to use each implemented feature):
  - `docs/agent-tooling/language-feature-playbook.md`
- Full CLI command decision playbook:
  - `docs/agent-tooling/aic-command-playbook.md`
- Deep command guides:
  - `docs/agent-tooling/commands/aic-init.md`
  - `docs/agent-tooling/commands/aic-lsp.md`
  - `docs/agent-tooling/commands/aic-diff.md`

## Core commands

- `aic contract --json`
- `aic ast --json <path>`
- `aic check <path> --json`
- `aic context --for function <name> --depth <n> --json`
- `aic synthesize --from spec <name> --project . --json`
- `aic testgen --strategy boundary --for function <name> --project . --json`
- `aic checkpoint diff <checkpoint> [--to <checkpoint>] --project . --json`
- `aic diag apply-fixes <path> --dry-run --json`
- `aic lsp`
- `aic daemon`

## Validation gates

- Schema and fixture validation: `tests/agent_protocol_tests.rs`
- Recipe docs-as-tests: `tests/agent_recipe_tests.rs`
- LSP/autofix/daemon integration tests: `tests/lsp_smoke_tests.rs`, `tests/e7_cli_tests.rs`

## Docs maintenance checklist

Before merging command/feature documentation updates:

1. Confirm command surface against `src/main.rs` and `docs/cli-contract.md`.
2. Validate diagnostic references against `docs/diagnostic-codes.md`.
3. Ensure README and `docs/agent-tooling/README.md` link to any new agent docs.
4. Verify examples/command snippets use current flag shapes (`aic <command> --help`).
5. Keep guaranteed behavior in `docs/reference/*`; keep forward-looking items in `docs/reference/open-issue-contracts.md`.

## Epic #62 proof-of-completion checklist (open)

Use this checklist when preparing closure evidence for epic `#62`. Keep the epic open until every item below is complete and evidenced.

- [ ] Protocol docs + schemas match implemented behavior: `docs/agent-tooling/protocol-v1.md`, `docs/agent-tooling/schemas/parse-response.schema.json`, `docs/agent-tooling/schemas/ast-response.schema.json`, `docs/agent-tooling/schemas/check-response.schema.json`, `docs/agent-tooling/schemas/build-response.schema.json`, `docs/agent-tooling/schemas/fix-response.schema.json`, `docs/agent-tooling/schemas/testgen-response.schema.json`
- [ ] Daemon docs reflect current incremental behavior and troubleshooting: `docs/agent-tooling/incremental-daemon.md`
- [ ] LSP workflow example is current and runnable: `examples/agent/lsp_workflow.json`
- [ ] Agent recipes are current for end-to-end loops: `docs/agent-recipes/`
- [ ] Test gate run: `make test-e7`
- [ ] Relevant test files are green: `tests/agent_protocol_tests.rs`, `tests/agent_recipe_tests.rs`, `tests/lsp_smoke_tests.rs`, `tests/e7_cli_tests.rs`
- [ ] Epic closure comment contains evidence: commit hash, commands run (`make test-e7`), and touched docs/examples/tests
