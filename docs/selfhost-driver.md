# Self-Hosted Driver

`compiler/aic/libs/driver` is the self-hosted compiler orchestration layer. It runs the implemented AIC compiler packages in process:

- parse source text with `compiler.parser`
- resolve modules and symbols with `compiler.frontend`
- analyze semantic metadata with `compiler.semantics`
- run type/effect/capability/borrow/resource checks with `compiler.typecheck`
- lower checked AST to self-host IR with `compiler.ir`
- emit deterministic LLVM text artifacts with `compiler.backend_llvm`

`compiler/aic/tools/aic_selfhost` is the executable candidate for this driver. It is built by the existing compiler toolchain, but command handling and compiler decisions are performed by the AIC driver package rather than by delegating to `cargo run --bin aic`.

## Commands

Supported command shapes:

```bash
aic_selfhost check <path>
aic_selfhost ir <path> --emit json
aic_selfhost ir-json <path>
aic_selfhost build <path> -o <artifact>
aic_selfhost run <path>
```

`<path>` may be a `.aic` file or a package directory. Package directories read `aic.toml` and use `main = "..."` when present, otherwise `src/main.aic`.

`build` emits deterministic self-host LLVM text through `compiler.backend_llvm`, writes it to a temporary artifact, and invokes `clang` to materialize the requested native executable. The self-host driver links the runtime C source parts directly, uses portable `-pthread -lm` POSIX link flags, applies the macOS stack-size linker flag when `uname -s` reports `Darwin`, ad-hoc signs macOS Mach-O outputs with `${AIC_CODESIGN:-/usr/bin/codesign}`, applies the ELF stack-size linker flag when `uname -s` reports `Linux`, and relies on the runtime startup stack guard to raise Linux `RLIMIT_STACK` when the host permits it. `run` uses the same materialization path and then executes the generated program, returning its exit status for backend-covered programs.

## Diagnostics

The driver emits deterministic diagnostics:

- `E5200` for unsupported command shapes
- `E5201` for missing parsed entry programs
- `E5202` for unreadable source inputs
- `E5203` for artifact write failures
- `E5204` for native materialization failures
- `E5205` for direct library-level run requests that bypass the executable materialization path

Frontend, IR, and backend diagnostics are preserved instead of being converted into success paths.

## Validation

Required local checks for driver changes:

```bash
cargo run --quiet --bin aic -- check compiler/aic/libs/driver --max-errors 240
cargo run --quiet --bin aic -- check compiler/aic/tools/aic_selfhost --max-errors 240
cargo run --quiet --bin aic -- build compiler/aic/tools/aic_selfhost -o target/aic_selfhost_t12
target/aic_selfhost_t12 check target/selfhost-driver-smoke/ok.aic
target/aic_selfhost_t12 ir target/selfhost-driver-smoke/ok.aic --emit json
target/aic_selfhost_t12 ir-json target/selfhost-driver-smoke/ok.aic
target/aic_selfhost_t12 build target/selfhost-driver-smoke/ok.aic -o target/selfhost-driver-smoke/ok
target/aic_selfhost_t12 run target/selfhost-driver-smoke/ok.aic
target/aic_selfhost_t12 check target/selfhost-driver-smoke/bad.aic
SELFHOST_PARITY_MANIFEST=tests/selfhost/aic_selfhost_driver_manifest.json \
SELFHOST_CANDIDATE=target/aic_selfhost_t12 \
make selfhost-parity
```
