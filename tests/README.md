# Tests Index

This directory groups the repository's executable checks by stable category rather than by count.

## Test families

- `unit_tests.rs` - parser, type checker, stdlib helpers, and low-level compiler behavior
- `golden_tests.rs` - formatting and snapshot stability
- `execution_tests.rs` - runtime and backend execution coverage
- `e7_cli_tests.rs` and `suggest_contracts_cli_tests.rs` - CLI contract and workflow coverage
- `lsp_smoke_tests.rs` - LSP request/response smoke coverage
- `e8_*` - verification, fuzz, and performance gates
- `fuzz/` - regression corpus and fuzz seeds

## Run the right slice

- `cargo test --locked --test unit_tests`
- `cargo test --locked --test golden_tests`
- `cargo test --locked --test execution_tests`
- `cargo test --locked --test e7_cli_tests`
- `cargo test --locked --test lsp_smoke_tests`
- `cargo test --locked --test e8_conformance_tests`
- `make test-e7`
- `make test-e8`
- `make ci` for the full local gate

## Related docs

- Verification runbooks: [../docs/verification-quality/README.md](../docs/verification-quality/README.md)
