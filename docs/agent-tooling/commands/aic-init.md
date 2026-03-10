# `aic init` Agent Guide

Related docs:
- [Agent-First aic Command Playbook](../aic-command-playbook.md)
- [`docs/cli-contract.md`](../../cli-contract.md)
- [`src/project.rs`](../../src/project.rs)

## What it does

`aic init [path]` scaffolds a project with:

- `aic.toml`
- `src/main.aic`
- `examples/`
- `docs/`
- `tests/`

Implementation source: [`src/project.rs`](../../src/project.rs).

Starter program intent in `src/main.aic`:
- module declaration (`module sample.main;`)
- std import (`import std.io;`)
- `Option[Int]` + exhaustive `match`
- explicit effect/capability declaration on `main`

## When to use

Use `aic init` when an agent needs a clean AICore project root and no existing package/workspace structure exists.

## When not to use

- Do not run in a mature repository root that already has curated `aic.toml`/`src/main.aic`.
- Do not use it for adding a new package to an existing workspace; scaffold manually to preserve workspace wiring.

## Overwrite and idempotency caveats

- `aic init` creates directories if missing.
- It writes `aic.toml` and `src/main.aic` directly.
- Existing files at those paths are overwritten.

## Common failure modes

- Running `aic init` in a populated directory where `aic.toml`/`src/main.aic` should be preserved.
- Filesystem permission/path errors when creating directories or writing starter files.
- Immediate std import failures on fresh machines when global std toolchain has not been installed.

## Deterministic follow-up commands

```bash
aic init my_service
cd my_service

aic setup                # optional if std is not installed yet
aic check src/main.aic
aic run src/main.aic
aic lock .
```

## Agent checklist

1. Confirm target directory is safe to initialize.
2. Run `aic init <path>`.
3. Run `aic check src/main.aic --json`.
4. If std imports fail, run `aic setup` and retry check.
5. Capture deterministic baseline by running `aic lock .`.
