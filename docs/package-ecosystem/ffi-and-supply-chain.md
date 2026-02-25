# FFI And Supply-Chain Safety

## Native C ABI package pattern

AICore supports explicit C ABI declarations and explicit unsafe boundaries.

```aic
extern "C" fn ffi_add42(x: Int) -> Int;

fn wrapped(x: Int) -> Int {
    unsafe { ffi_add42(x) }
}
```

Safety rules:

- only `extern "C"` ABI is supported (`E2120`)
- extern declarations must remain plain (`E2121`)
- extern calls require explicit `unsafe` boundary (`E2122`)
- unsupported ABI types are rejected (`E2123`)

Manifest native link configuration:

```toml
[native]
libs = ["ffiadd"]
search_paths = ["native"]
objects = ["native/add.o"]
```

Reference source: `examples/pkg/ffi_zlib.aic`.

## Provenance and trust policy

Publish with signature metadata:

```bash
AIC_PKG_SIGNING_KEY=secret \
AIC_PKG_SIGNING_KEY_ID=corp \
  aic pkg publish /tmp/pkg --registry /tmp/registry
```

Install with trusted key verification using `aic.registry.json` trust policy:

- `require_signed`
- `require_signed_for`
- `allow` / `deny`
- `trusted_keys` (`key_id -> env var`)

Reference policy project: `examples/pkg/policy_enforced_project/`.

Trust diagnostics:

- `E2119`: policy denied install
- `E2124`: signature verification/trusted-key failure

## WebAssembly host interop (`--target wasm32`)

`aic build --target wasm32` compiles to `wasm32-unknown-unknown` and keeps runtime calls host-bound instead of linking native runtime C shims.

Practical implications:

- pure programs can compile without runtime host dependencies
- IO/runtime paths (for example `print_str`) are imported as host symbols such as `aic_rt_print_str`
- the wasm module exports `main` and `aic_main` for host invocation

Reference example: `examples/interop/wasm_hello_world.aic`.
