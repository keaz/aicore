# AICore VS Code Extension

This extension starts the AICore language server using:

```bash
aic lsp
```

## Features

- Syntax highlighting for `.aic` files
- Autocomplete (language server completions + editor suggestions)
- Doc-aware hover/completion (`///` summary in completion detail + full markdown docs in hover/completion docs)
- Auto-import suggestions for unimported module symbols (completion inserts `import ...;`)
- Code snippets for common AICore patterns (`fn`, `struct`, `match`, contracts, effects)
- Status bar language server health indicator (starting/running/error/stopped + diagnostics count)
- Inline error lens diagnostics with severity colors (error/warning/info)
- Semantic highlighting for mutable/readonly variables, effectful calls, and deprecated APIs
- Document outline (`textDocument/documentSymbol`) and workspace symbol search (`workspace/symbol`)
- Call hierarchy (`textDocument/prepareCallHierarchy`, incoming/outgoing calls)
- Folding ranges (`textDocument/foldingRange`) and semantic selection expansion (`textDocument/selectionRange`)
- Inlay hints for inferred types and effectful call sites
- Diagnostics (matches `aic check` diagnostics)
- Automated extension integration tests (activation, LSP lifecycle, diagnostics, completion, restart command, grammar contract)
- Hover
- Go-to-definition
- Formatting
- Debugger integration (`type: "aic"`) backed by `aic debug dap` and LLDB DAP backends

## Screenshots

![Completion and auto-import](./assets/screenshots/completion-auto-import.png)
![Diagnostics and status bar](./assets/screenshots/diagnostics-status-bar.png)
![Semantic tokens and inlay hints](./assets/screenshots/semantic-inlay.png)

## Installation

### VS Code Marketplace

1. Open Extensions (`Ctrl/Cmd + Shift + X`)
2. Search for `AICore Language Tools`
3. Click Install

### Manual VSIX install

```bash
cd tools/vscode-aic
npx -y @vscode/vsce package
code --install-extension aic-language-tools-*.vsix
```

## Settings

- `aic.server.path` (default: `aic`)
- `aic.server.args` (default: `["lsp"]`)
- `aic.trace.server` (`off` | `messages` | `verbose`)
- `aic.debug.adapterPath` (optional absolute path to `lldb-dap`/`lldb-vscode`)
- `aic.errorLens.enabled` (default: `true`)
- `aic.errorLens.showOnlyFirstPerLine` (default: `true`)
- `aic.inlayHints.typeAnnotations` (default: `true`)
- `aic.inlayHints.effectAnnotations` (default: `true`)
- `aic.inlayHints.contractAnnotations` (default: `false`)

## Development

```bash
cd tools/vscode-aic
npm install
npm run build
npm test
```

Press `F5` in VS Code to launch an Extension Development Host.

Use `examples/e7/lsp_project/` as a sample workspace.

## Debugging (DAP)

Create a launch template:

```bash
Command Palette -> "AICore: Create launch.json"
```

Generated configuration:

```json
{
  "type": "aic",
  "request": "launch",
  "name": "Debug AICore",
  "program": "${workspaceFolder}/src/main.aic",
  "args": [],
  "cwd": "${workspaceFolder}",
  "breakOnContractViolation": false
}
```

Runtime behavior:

- If `program` points to a `.aic` file, the extension runs `aic build <program> --debug-info` before launch.
- Debug adapter process is `aic debug dap`, which delegates to `lldb-dap` or `lldb-vscode`.
- Set `breakOnContractViolation` to `true` to inject a startup breakpoint at `aic_rt_panic`.

## Packaging

Package without `--no-dependencies`, otherwise runtime modules (including
`vscode-languageclient`) are excluded from the VSIX and activation fails.

```bash
cd tools/vscode-aic
npx -y @vscode/vsce package
```

## Release

Publishing is automated by GitHub Actions on release tags via
`.github/workflows/vscode-extension-publish.yml` and requires `VSCE_PAT`.
