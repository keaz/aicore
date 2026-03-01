# Changelog

All notable changes to the AICore VS Code extension are documented in this file.

## [0.1.3] - 2026-03-01

### Added

- Language-level file icons for `.aic` files in Explorer.
- Integration test coverage that verifies `.aic` file icon contribution assets are packaged.

### Changed

- Extension now auto-restarts the language server when `aic.server.path`, `aic.server.args`, or `aic.trace.server` changes.
- Debug pre-launch build for `.aic` programs now runs asynchronously with a cancellable progress notification to avoid blocking the extension host.

## [0.1.2] - 2026-02-24

### Added

- Auto-import completion edits for unimported module symbols.
- Call hierarchy support in the extension/LSP surface.
- Folding ranges and semantic selection ranges.
- Integration test coverage for extension activation and LSP contracts.
- Marketplace packaging metadata, icon, screenshots, and publish workflow.

## [0.1.1] - 2026-02-24

### Added

- Inlay hints for inferred types/effects/contracts.
- Semantic highlighting scopes and modifiers.
- Status bar diagnostics and inline error lens rendering.

## [0.1.0] - 2026-02-24

### Added

- Initial AICore VS Code extension with language configuration, grammar, and LSP wiring.
