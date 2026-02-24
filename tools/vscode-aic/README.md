# AICore VS Code Extension (Prototype)

This extension starts the AICore language server using:

```bash
aic lsp
```

## Features

- Syntax highlighting for `.aic` files
- Autocomplete (language server completions + editor suggestions)
- Code snippets for common AICore patterns (`fn`, `struct`, `match`, contracts, effects)
- Document outline (`textDocument/documentSymbol`) and workspace symbol search (`workspace/symbol`)
- Inlay hints for inferred types and effectful call sites
- Diagnostics (matches `aic check` diagnostics)
- Hover
- Go-to-definition
- Formatting

## Settings

- `aic.server.path` (default: `aic`)
- `aic.server.args` (default: `["lsp"]`)
- `aic.trace.server` (`off` | `messages` | `verbose`)

## Development

```bash
cd tools/vscode-aic
npm install
npm run build
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
