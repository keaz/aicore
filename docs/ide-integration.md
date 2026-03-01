# IDE Integration Guide (E7)

AICore exposes an LSP server via:

```bash
aic lsp
```

For interactive expression evaluation, AICore also exposes:

```bash
aic repl
```

Use `aic repl --json` for machine-consumable line-delimited JSON events (`ready`, `result`, `type`, `effects`, `error`, `bye`), which is useful for AI-agent tool integration.

For deterministic tooling artifacts:

- `aic coverage <input> --report <path>` emits a JSON coverage summary (and supports `--check --min <pct>` threshold gates).
- `aic run <input> --profile --profile-output <path>` writes a deterministic profile JSON report with top functions and `self_time_ms`/`total_time_ms`.

Implemented LSP capabilities:

- diagnostics (`textDocument/publishDiagnostics`)
- hover (`textDocument/hover`, including `///` markdown docs with fenced `aic` examples)
- go-to-definition (`textDocument/definition`)
- formatting (`textDocument/formatting`)
- completion (`textDocument/completion`, including `///` summary + full docs)
- rename (`textDocument/rename`)
- code actions (`textDocument/codeAction`)
- semantic tokens (`textDocument/semanticTokens/full`)

Autofix code actions are sourced from diagnostic `suggested_fixes` payloads and returned as deterministic quick-fix edits.

## REPL commands

Inside `aic repl`, supported commands are:

- `:type <expr>`
- `:effects <fn>`
- `:history` (print numbered history)
- `!!` (re-run previous entry)
- `!<n>` (re-run history entry `n`, 1-based)
- `:quit`

REPL state persists across entries for bindings/evaluated values during the session.
In non-JSON mode, control characters are applied as line edits before evaluation (`Backspace`/`Delete`, `Ctrl-U`, `Ctrl-W`).

Example history flow:

```text
$ aic repl
aic repl ready (:type <expr>, :effects <fn>, :history, :quit)
let x = 7
x = 7 : Int
!!
x = 7 : Int
:history
1: let x = 7
2: let x = 7
```

## VS Code setup

Prototype extension source is included in-repo:

- `tools/vscode-aic/`

### Option A: `coc.nvim`/`coc`-style language server config

Use any client that accepts a stdio server command:

- command: `aic`
- args: `lsp`
- language id: `aic`
- file extension: `.aic`

### Option B: `package.json` contribution (custom extension)

`contributes.languages` entry:

- id: `aic`
- extensions: [`.aic`]

`contributes.configuration` should point format-on-save to the AIC language server.

Build extension prototype:

```bash
cd tools/vscode-aic
npm install
npm run build
```

Create debugger launch template (command palette):

```text
AICore: Create launch.json
```

Debugger runtime path:

- VSCode extension starts DAP with `aic debug dap`.
- `aic debug dap` resolves `lldb-dap`/`lldb-vscode` (or `--adapter <path>`).
- For `.aic` launch targets, extension performs `aic build --debug-info` before launching.

## Neovim setup (`nvim-lspconfig`)

```lua
require('lspconfig').aic = {
  default_config = {
    cmd = { 'aic', 'lsp' },
    filetypes = { 'aic' },
    root_dir = function(fname)
      return require('lspconfig.util').root_pattern('aic.toml', '.git')(fname)
    end,
    single_file_support = true,
  },
}

require('lspconfig').aic.setup({})
```

## Sample workspace

Use:

- `examples/e7/lsp_project/` for hover/definition/formatting smoke
- `examples/agent/lsp_workspace/` for completion/rename/semantic token/code-action workflow checks
- `examples/vscode/doc_hover_completion_showcase.aic` for doc-aware hover/completion behavior
- `examples/vscode/debugger_launch_demo.aic` for debugger launch/step variable inspection workflow
