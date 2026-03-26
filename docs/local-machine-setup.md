# Local Machine Setup

This guide shows how to set up `aic` on a local development machine.

## 1. Prerequisites

Install these tools first:

- Rust (stable toolchain)
- `clang` available in `PATH`
- `make`
- `python3`

Quick checks:

```bash
rustc --version
cargo --version
clang --version
make --version
python3 --version
```

## 2. Build the CLI

From the repository root:

```bash
cargo build --release
```

This produces the binary at:

```text
target/release/aic
```

## 3. Install `aic` for shell usage

Choose one of these options.

### Option A: Cargo install (recommended)

```bash
cargo install --path . --force
```

### Option B: Use the built binary directly

```bash
export PATH="$(pwd)/target/release:$PATH"
```

If you use zsh, add the same `export PATH=...` line to `~/.zshrc` and reload your shell.

## 4. Install the global standard library

Run setup once after installing the CLI:

```bash
aic setup
```

Default global std install path:

```text
~/.aic/toolchains/<aic-version>/std
```

Optional overrides:

- `AIC_HOME`: changes base directory (`<AIC_HOME>/toolchains/<aic-version>/std`)
- `AIC_STD_ROOT`: sets an explicit std directory
- `aic setup --std-root /absolute/path/to/std`

## 5. Verify installation

```bash
aic --version
aic setup --std-root "$(mktemp -d)/std"
```

The `setup` command should print:

```text
installed AICore standard library at <path>
```

## 6. Create and run a project

```bash
aic init hello_aic
cd hello_aic
aic check src/main.aic
aic run src/main.aic
```

`aic init` does not copy `std/` into the project. `std` imports resolve from the global toolchain install.

## Troubleshooting

### `aic: command not found`

- Re-open the terminal after install.
- Confirm `cargo bin` is in `PATH` (typically `~/.cargo/bin`).
- Or add `target/release` to `PATH` if using the local binary directly.

### `aic setup` cannot resolve install location

Set one of these and retry:

```bash
export AIC_HOME="$HOME/.aic"
# or
export AIC_STD_ROOT="$HOME/.aic/toolchains/manual/std"
```

### `std.*` imports fail

- Run `aic setup` again.
- Confirm the std root contains files like `io.aic`.
- If needed, pin explicit location with `AIC_STD_ROOT`.
