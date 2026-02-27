SHELL := /usr/bin/env bash

CARGO ?= cargo
AIC ?= cargo run --quiet --bin aic --

.DEFAULT_GOAL := help

.PHONY: help init hooks-install hooks-uninstall ci ci-fast check fmt-check lint build test test-unit test-golden test-exec test-e7 test-e8 test-e8-concurrency-stress test-e8-nightly-fuzz test-e9 intrinsic-placeholder-guard verify-intrinsics examples-check examples-run cli-smoke docs-check no-null-lint repro-check security-audit release-preflight

help:
	@echo "AICore developer commands"
	@echo "  make init          Install git hooks (pre-commit + pre-push)"
	@echo "  make ci            Full local CI (matches Linux CI job)"
	@echo "  make ci-fast       Fast local CI for quick iteration"
	@echo "  make check         Full validation except format/lint"
	@echo "  make fmt-check     Verify canonical formatting"
	@echo "  make lint          Run clippy"
	@echo "  make build         Build compiler"
	@echo "  make test-unit     Run unit tests"
	@echo "  make test-golden   Run parser/formatter golden tests"
	@echo "  make test-exec     Run LLVM execution tests"
	@echo "  make test-e7       Run E7 CLI + LSP integration tests"
	@echo "  make test-e8       Run E8 verification/fuzz/diff/matrix/perf tests"
	@echo "  make test-e8-concurrency-stress Run deterministic concurrency stress/replay gate"
	@echo "  make test-e8-nightly-fuzz Run long-running E8 fuzz stress tests"
	@echo "  make test-e9       Run E9 release/security operations tests"
	@echo "  make intrinsic-placeholder-guard Enforce AGX1 intrinsic declaration policy"
	@echo "  make verify-intrinsics Validate runtime intrinsic bindings"
	@echo "  make examples-check Validate example compile/check behavior"
	@echo "  make examples-run  Run executable example validations"
	@echo "  make no-null-lint  Ensure .aic sources do not use null semantics"
	@echo "  make cli-smoke     End-to-end CLI smoke test"
	@echo "  make docs-check    Validate docs and schema artifacts"
	@echo "  make repro-check   Verify deterministic reproducibility manifest pipeline"
	@echo "  make security-audit Run release security audit checks"
	@echo "  make release-preflight Run all release readiness checks"

init: hooks-install
	@echo "hooks installed"

hooks-install:
	@mkdir -p .git/hooks
	@install -m 0755 scripts/hooks/pre-commit .git/hooks/pre-commit
	@install -m 0755 scripts/hooks/pre-push .git/hooks/pre-push
	@echo "installed .git/hooks/pre-commit"
	@echo "installed .git/hooks/pre-push"

hooks-uninstall:
	@rm -f .git/hooks/pre-commit .git/hooks/pre-push
	@echo "removed local git hooks"

ci: fmt-check lint check

ci-fast: fmt-check build test-unit test-golden

check: build test-unit test-golden test-exec test-e7 test-e8 test-e9 intrinsic-placeholder-guard verify-intrinsics examples-check examples-run no-null-lint cli-smoke docs-check security-audit repro-check

fmt-check:
	$(CARGO) fmt --all -- --check

lint:
	$(CARGO) clippy --all-features --lib
	$(CARGO) clippy --all-features --bins

build:
	$(CARGO) build --locked

test:
	$(CARGO) test --locked

test-unit:
	$(CARGO) test --locked --lib
	$(CARGO) test --locked --test unit_tests

test-golden:
	$(CARGO) test --locked --test golden_tests

test-exec:
	$(CARGO) test --locked --test execution_tests

test-e7:
	$(CARGO) test --locked --test e7_cli_tests
	$(CARGO) test --locked --test lsp_smoke_tests
	$(CARGO) test --locked --test agent_protocol_tests
	$(CARGO) test --locked --test agent_recipe_tests

test-e8:
	$(CARGO) test --locked --test e8_conformance_tests
	$(CARGO) test --locked --test e8_fuzz_tests
	$(CARGO) test --locked --test e8_differential_tests
	$(CARGO) test --locked --test e8_matrix_tests
	$(CARGO) test --locked --test e8_concurrency_stress_tests
	$(CARGO) test --locked --test e8_perf_tests

test-e8-concurrency-stress:
	$(CARGO) test --locked --test e8_concurrency_stress_tests

test-e8-nightly-fuzz:
	$(CARGO) test --locked --test e8_fuzz_tests -- --ignored

test-e9:
	$(CARGO) test --locked --test e9_release_ops_tests

intrinsic-placeholder-guard:
	python3 scripts/ci/intrinsic_placeholder_guard.py

verify-intrinsics:
	./target/debug/aic verify-intrinsics std --json >/tmp/aic-verify-intrinsics.json
	python3 -m json.tool /tmp/aic-verify-intrinsics.json >/dev/null

examples-check:
	./scripts/ci/examples.sh check

examples-run:
	./scripts/ci/examples.sh run

cli-smoke:
	./scripts/ci/cli-smoke.sh

repro-check:
	./scripts/ci/repro-build-check.sh

security-audit:
	./scripts/ci/security-audit.sh

release-preflight: ci repro-check security-audit

docs-check:
	@test -f docs/spec.md
	@test -f docs/syntax.md
	@test -f docs/ir-schema.md
	@test -f docs/id-allocation.md
	@test -f docs/type-system.md
	@test -f docs/effect-system.md
	@test -f docs/capability-protocols.md
	@test -f docs/contracts.md
	@test -f docs/diagnostic-codes.md
	@test -f docs/cli-contract.md
	@test -f docs/sarif.md
	@test -f docs/ide-integration.md
	@test -f docs/llvm-backend.md
	@test -f docs/agent-tooling/README.md
	@test -f docs/agent-tooling/protocol-v1.md
	@test -f docs/agent-tooling/incremental-daemon.md
	@test -f docs/agent-recipes/README.md
	@test -f docs/agent-recipes/feature-loop.md
	@test -f docs/agent-recipes/bugfix-loop.md
	@test -f docs/agent-recipes/refactor-loop.md
	@test -f docs/agent-recipes/diagnostics-loop.md
	@test -f docs/agent-recipes/secure-postgres-tls-scram-loop.md
	@test -f docs/agent-tooling/schemas/parse-response.schema.json
	@test -f docs/agent-tooling/schemas/check-response.schema.json
	@test -f docs/agent-tooling/schemas/build-response.schema.json
	@test -f docs/agent-tooling/schemas/fix-response.schema.json
	@test -f docs/package-workflow.md
	@test -f docs/ai-agent-rest-guide.md
	@test -f docs/package-ecosystem/README.md
	@test -f docs/package-ecosystem/publish-consume.md
	@test -f docs/package-ecosystem/workspaces-and-locks.md
	@test -f docs/package-ecosystem/ffi-and-supply-chain.md
	@test -f docs/package-ecosystem/failure-playbooks.md
	@test -f docs/io-filesystem.md
	@test -f docs/io-process-env-path.md
	@test -f docs/io-concurrency-runtime.md
	@test -f docs/io-api-reference.md
	@test -f docs/io-cookbook.md
	@test -f docs/io-agent-guide.md
	@test -f docs/io-migration.md
	@test -f docs/intrinsics-runtime-bindings.md
	@test -f docs/io-runtime/README.md
	@test -f docs/io-runtime/net-time-rand.md
	@test -f docs/io-runtime/error-model.md
	@test -f docs/io-runtime/lifecycle-playbook.md
	@test -f docs/data-regex.md
	@test -f docs/std-compatibility.md
	@test -f docs/e8-verification-gates.md
	@test -f docs/verification-quality/README.md
	@test -f docs/verification-quality/contracts-proof-obligations.md
	@test -f docs/verification-quality/effect-protocols.md
	@test -f docs/verification-quality/fuzz-differential-runbook.md
	@test -f docs/verification-quality/concurrency-stress-replay.md
	@test -f docs/verification-quality/perf-sla-playbook.md
	@test -f docs/verification-quality/incident-reproduction.md
	@test -f docs/release-security-ops.md
	@test -f docs/security-ops/README.md
	@test -f docs/security-ops/release-runbook.md
	@test -f docs/security-ops/sandbox-operations.md
	@test -f docs/security-ops/telemetry.md
	@test -f docs/security-ops/telemetry.schema.json
	@test -f docs/security-ops/tls-policy.v1.json
	@test -f docs/security-ops/postgres-tls-scram-replay.v1.json
	@test -f docs/release/lts-policy.md
	@test -f docs/release/compatibility-matrix.json
	@test -f docs/security-ops/migration.md
	@test -f docs/security-ops/incident-response.md
	@test -f docs/security-threat-model.md
	@test -f docs/compatibility-migration-policy.md
	@test -f docs/errors/secure-networking-error-contract.v1.json
	@test -f docs/std-api-baseline.json
	@python3 -m json.tool docs/diagnostics.schema.json >/dev/null
	@python3 -m json.tool docs/agent-tooling/schemas/parse-response.schema.json >/dev/null
	@python3 -m json.tool docs/agent-tooling/schemas/check-response.schema.json >/dev/null
	@python3 -m json.tool docs/agent-tooling/schemas/build-response.schema.json >/dev/null
	@python3 -m json.tool docs/agent-tooling/schemas/fix-response.schema.json >/dev/null
	@python3 -m json.tool docs/release/compatibility-matrix.json >/dev/null
	@python3 -m json.tool docs/security-ops/tls-policy.v1.json >/dev/null
	@python3 -m json.tool docs/security-ops/postgres-tls-scram-replay.v1.json >/dev/null
	@python3 -m json.tool docs/errors/secure-networking-error-contract.v1.json >/dev/null
	@grep -q "aic init" README.md
	@grep -q "aic check" README.md
	@grep -q "aic fmt" README.md
	@grep -q "aic ir" README.md
	@grep -q "aic ir-migrate" README.md
	@grep -q "aic migrate" README.md
	@grep -q "aic build" README.md
	@grep -q "aic lock" README.md
	@grep -q "aic doc" README.md
	@grep -q "aic explain" README.md
	@grep -q "aic lsp" README.md
	@grep -q "aic daemon" README.md
	@grep -q "aic test" README.md
	@grep -q "aic run" README.md
	@grep -q "aic release" README.md
	@grep -q "aic contract" README.md
	@grep -q "docs/ai-agent-rest-guide.md" README.md
	@$(CARGO) test --locked --test agent_recipe_tests tutorial_chapters_and_agent_steps_contract_is_deterministic -- --exact
	@$(CARGO) test --locked --test agent_recipe_tests std_api_docs_explain_human_and_machine_readable_outputs -- --exact
	@$(CARGO) test --locked --test agent_recipe_tests std_api_docs_test_commands_generate_expected_files_for_module_and_std_inputs -- --exact
	@./target/debug/aic std-compat --check --baseline docs/std-api-baseline.json >/dev/null
	@./target/debug/aic release policy --check >/dev/null
	@./target/debug/aic release lts --check >/dev/null

no-null-lint:
	@if rg -n --glob '*.aic' --glob '!examples/ops/migration_v1_to_v2/**' '\bnull\b' examples std tests/golden >/tmp/aic-no-null-lint.out; then \
		echo "forbidden 'null' token found in AIC source files:" >&2; \
		cat /tmp/aic-no-null-lint.out >&2; \
		exit 1; \
	fi
