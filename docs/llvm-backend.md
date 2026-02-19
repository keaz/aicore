# LLVM Backend Overview (MVP)

Backend flow:

1. Lowered IR (contracts inserted as asserts)
2. LLVM IR text emission
3. `clang main.ll runtime.c -o <bin>`

Runtime C shim provides:

- `aic_rt_print_int`
- `aic_rt_print_str`
- `aic_rt_strlen`
- `aic_rt_panic`

Current backend supports core scalar codegen and Option-based branching, enough for executable MVP examples.
