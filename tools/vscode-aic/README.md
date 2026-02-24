# AICore VS Code Extension

This extension starts the AICore language server using:

```bash
aic lsp
```

## Features

- Syntax highlighting for `.aic` files
- Autocomplete (language server completions + editor suggestions)
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
