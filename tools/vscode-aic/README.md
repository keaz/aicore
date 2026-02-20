# AICore VS Code Extension (Prototype)

This extension starts the AICore language server using:

```bash
aic lsp
```

## Features

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
