# IDE Integration Guide (E7)

AICore exposes an LSP server via:

```bash
aic lsp
```

Implemented LSP capabilities:

- diagnostics (`textDocument/publishDiagnostics`)
- hover (`textDocument/hover`)
- go-to-definition (`textDocument/definition`)
- formatting (`textDocument/formatting`)

## VS Code setup

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

Use `examples/e7/lsp_project/` to validate hover/definition/formatting behavior.
