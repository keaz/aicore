# REST Conformance Scenario Matrix (Issue #364)

This runbook defines the deterministic end-to-end REST conformance scenario matrix used by `tests/e8_conformance_tests.rs`.

Machine-readable matrix: `tests/integration/rest-conformance.matrix.json`

## Validation Command

```bash
cargo test --locked --test e8_conformance_tests
```

`rest_conformance_harness_runs_matrix_scenarios` executes every scenario in the matrix table-driven by scenario ID.

## Scenario IDs

| Scenario ID | Coverage focus | Expected deterministic outcome |
|---|---|---|
| `REST-CONF-001-HTTP-VALID-E2E` | Integrated parse -> route -> JSON -> response path | Process exits `0`; stdout is `42\n` |
| `REST-CONF-002-HTTP-MALFORMED-METHOD` | Malformed HTTP method parse failure | Process exits `0`; stdout is `42\n` (typed `ServerError::InvalidMethod`) |
| `REST-CONF-003-ROUTER-PRECEDENCE-PARAMS` | Router precedence contract and deterministic param extraction | Process exits `0`; stdout is `42\n` |
| `REST-CONF-004-JSON-MALFORMED-PAYLOAD` | Malformed JSON payload after valid HTTP parse | Process exits `0`; stdout is `42\n` (typed `JsonError::InvalidJson`) |
| `REST-CONF-005-ASYNC-LIFECYCLE` | Async submit/wait timeout + shutdown lifecycle | Process exits `0`; stdout is `42\n` |
| `REST-CONF-006-TYPED-ERROR-MAPPING` | Deterministic typed error mapping across HTTP/router/JSON/async | Process exits `0`; stdout is `42\n` |

## Contract Notes

- All scenarios must keep stable IDs; changing IDs requires updating both this doc and `tests/integration/rest-conformance.matrix.json`.
- Matrix schema is pinned to `schema_version = 1`.
- Negative malformed HTTP and malformed JSON cases are mandatory and must remain deterministic.
