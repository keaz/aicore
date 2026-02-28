# Core Language Safety Coverage (PROD-T3, PROD-T7, PROD-T9)

This document maps the core language safety tickets to concrete implementation evidence, tests, and runnable examples.

## PROD-T3: Drop Trait and RAII Cleanup

Acceptance coverage:
- Scope-exit and early-return cleanup: `tests/execution_tests.rs` (`exec_raii_file_handle_cleanup_on_scope_exit_and_early_return`)
- `?` propagation cleanup path: `tests/execution_tests.rs` (`exec_raii_file_handle_cleanup_on_question_mark_error_return`)
- Move-out/drop transfer behavior: `tests/execution_tests.rs` (`exec_raii_file_handle_move_out_preserves_transferred_ownership`)
- Drop dispatch ordering + mixed paths: `tests/execution_tests.rs` (`exec_drop_trait_dispatch_lifo_question_mark_and_move_paths`)

Examples (CI wired):
- `examples/io/raii_file_cleanup.aic` (expected output `42`)
- `examples/io/drop_trait_cleanup.aic` (expected output `42`)

## PROD-T7: First-Class Tuple Types

Acceptance coverage:
- Tuple destructure + field access + match usage: `tests/execution_tests.rs` (`exec_tuple_types_destructure_match_and_field_access`)
- Tuple typing and destructure at typecheck layer: `tests/unit_tests.rs` (`unit_tuple_types_destructure_and_match_typecheck`)

Examples (CI wired):
- `examples/core/tuple_types.aic` (expected output `42`)

## PROD-T9: Borrow Checker Completeness

Acceptance coverage:
- Use-after-move: `tests/unit_tests.rs` (`unit_use_after_move_reports_e1270`)
- Mutable borrow exclusivity: `tests/unit_tests.rs` (`unit_conflicting_mutable_borrow_reports_e1263`, `unit_shared_borrow_while_mutable_borrow_active_reports_e1264`)
- Assignment while borrowed: `tests/unit_tests.rs` (`unit_assignment_while_borrowed_reports_e1265`)
- Move while borrowed: `tests/unit_tests.rs` (`unit_move_while_borrowed_reports_e1271`)
- Field-level borrow conflicts: `tests/unit_tests.rs` (`unit_field_borrow_conflict_reports_e1263`)
- Runtime-safe borrow flow after reinitialization: `tests/execution_tests.rs` (`exec_borrow_checker_reinitialize_after_move`)

Examples (CI wired):
- `examples/core/borrow_checker_completeness.aic` (expected output `2`)

## Quick Validation Commands

```bash
cargo test --locked --test execution_tests exec_drop_trait_dispatch_lifo_question_mark_and_move_paths
cargo test --locked --test execution_tests exec_tuple_types_destructure_match_and_field_access
cargo test --locked --test unit_tests unit_use_after_move_reports_e1270
cargo test --locked --test e7_cli_tests prod_t3_t7_t9_examples_are_ci_wired_and_run_with_expected_outputs
cargo run --quiet --bin aic -- run examples/io/raii_file_cleanup.aic
cargo run --quiet --bin aic -- run examples/io/drop_trait_cleanup.aic
cargo run --quiet --bin aic -- run examples/core/tuple_types.aic
cargo run --quiet --bin aic -- run examples/core/borrow_checker_completeness.aic
```
