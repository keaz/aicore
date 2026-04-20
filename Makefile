SHELL := /usr/bin/env bash

CARGO ?= cargo
AIC ?= cargo run --quiet --bin aic --
AIC_SELFHOST_BOOTSTRAP_TIMEOUT ?= 900

.DEFAULT_GOAL := help

.PHONY: help init hooks-install hooks-uninstall ci ci-fast check fmt-check lint build test test-unit test-golden test-exec test-e7 test-e8 test-e8-rest-runtime-soak test-e8-concurrency-stress test-e8-nightly-fuzz test-e9 test-selfhost selfhost-parity selfhost-parity-candidate selfhost-stage-matrix selfhost-bootstrap selfhost-bootstrap-report selfhost-release-provenance selfhost-mode-check selfhost-default-mode-check selfhost-default-build-check selfhost-retirement-audit intrinsic-placeholder-guard test-command-style-guard verify-intrinsics std-doc-check examples-check examples-run integration-harness-offline integration-harness-live cli-smoke docs-check no-null-lint repro-check security-audit release-preflight

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
	@echo "  make test-e8-rest-runtime-soak Run deterministic REST runtime parse/router/json/async soak gate"
	@echo "  make test-e8-concurrency-stress Run deterministic concurrency stress/replay gate"
	@echo "  make test-e8-nightly-fuzz Run long-running E8 fuzz stress tests"
	@echo "  make test-e9       Run E9 release/security operations tests"
	@echo "  make test-selfhost Run self-hosting parity harness tests"
	@echo "  make selfhost-parity Run reference/candidate compiler parity comparisons"
	@echo "  make selfhost-parity-candidate Build aic_selfhost and compare it against the Rust reference"
	@echo "  make selfhost-stage-matrix Validate latest stage compiler on core packages/examples"
	@echo "  make selfhost-bootstrap Run required stage0/stage1/stage2 self-host bootstrap gate"
	@echo "  make selfhost-bootstrap-report Generate bounded bootstrap readiness report without claiming readiness"
	@echo "  make selfhost-release-provenance Generate and verify release-grade self-host provenance"
	@echo "  make selfhost-mode-check Verify supported self-host compiler mode evidence"
	@echo "  make selfhost-default-mode-check Verify approved default self-host compiler mode evidence"
	@echo "  make selfhost-default-build-check Verify default AICore compiler source build uses self-host"
	@echo "  make selfhost-retirement-audit Verify Rust-reference retirement inventory remains blocked until approved"
	@echo "  make intrinsic-placeholder-guard Enforce AGX1 intrinsic declaration policy"
	@echo "  make test-command-style-guard Enforce canonical cargo test snippet style"
	@echo "  make verify-intrinsics Validate runtime intrinsic bindings"
	@echo "  make std-doc-check Verify std modules have required doc comments"
	@echo "  make examples-check Validate example compile/check behavior"
	@echo "  make examples-run  Run executable example validations"
	@echo "  make integration-harness-offline Run offline external protocol harness gate"
	@echo "  make integration-harness-live Run live external protocol harness gate (opt-in)"
	@echo "  make no-null-lint  Ensure .aic sources do not use null semantics"
	@echo "  make cli-smoke     End-to-end CLI smoke test"
	@echo "  make docs-check    Validate docs and schema artifacts"
	@echo "  make repro-check   Verify deterministic reproducibility manifest pipeline"
	@echo "  make security-audit Run release security audit checks"
	@echo "  make release-preflight Run all release readiness checks, including self-host bootstrap"

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

check: build test-unit test-golden test-exec test-e7 test-e8 test-e9 test-selfhost selfhost-retirement-audit intrinsic-placeholder-guard test-command-style-guard verify-intrinsics std-doc-check examples-check examples-run integration-harness-offline no-null-lint cli-smoke docs-check security-audit repro-check

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
	python3 scripts/ci/rest-runtime-soak-gate.py

test-e8-rest-runtime-soak:
	python3 scripts/ci/rest-runtime-soak-gate.py

test-e8-concurrency-stress:
	$(CARGO) test --locked --test e8_concurrency_stress_tests

test-e8-nightly-fuzz:
	$(CARGO) test --locked --test e8_fuzz_tests -- --ignored

test-e9:
	$(CARGO) test --locked --test e9_release_ops_tests

test-selfhost:
	$(CARGO) test --locked --test selfhost_parity_tests

selfhost-parity:
	@args=(--manifest "$${SELFHOST_PARITY_MANIFEST:-tests/selfhost/parity_manifest.json}" --reference "$${SELFHOST_REFERENCE:-cargo run --quiet --bin aic --}" --artifact-dir "$${SELFHOST_ARTIFACT_DIR:-target/selfhost-parity}" --report "$${SELFHOST_PARITY_REPORT:-target/selfhost-parity/report.json}"); \
	if [[ -n "$${SELFHOST_CANDIDATE:-}" ]]; then args+=(--candidate "$$SELFHOST_CANDIDATE"); fi; \
	python3 scripts/selfhost/parity.py "$${args[@]}"

selfhost-parity-candidate:
	$(AIC) build compiler/aic/tools/aic_selfhost -o target/aic_selfhost_candidate --compiler-mode reference
	SELFHOST_PARITY_MANIFEST=tests/selfhost/rust_vs_selfhost_manifest.json SELFHOST_CANDIDATE=target/aic_selfhost_candidate SELFHOST_ARTIFACT_DIR=target/selfhost-parity-candidate SELFHOST_PARITY_REPORT=target/selfhost-parity-candidate/report.json $(MAKE) selfhost-parity

selfhost-stage-matrix:
	@args=(--stage-compiler "$${SELFHOST_STAGE_COMPILER:-target/selfhost-bootstrap/stage2/aic_selfhost}" --manifest "$${SELFHOST_STAGE_MATRIX_MANIFEST:-tests/selfhost/stage_matrix_manifest.json}" --artifact-dir "$${SELFHOST_STAGE_MATRIX_ARTIFACT_DIR:-target/selfhost-stage-matrix}" --report "$${SELFHOST_STAGE_MATRIX_REPORT:-target/selfhost-stage-matrix/report.json}" --timeout "$${SELFHOST_STAGE_MATRIX_TIMEOUT:-90}"); \
	python3 scripts/selfhost/stage_matrix.py "$${args[@]}"

selfhost-bootstrap:
	python3 scripts/selfhost/bootstrap.py --mode supported --timeout "$(AIC_SELFHOST_BOOTSTRAP_TIMEOUT)"

selfhost-bootstrap-report:
	python3 scripts/selfhost/bootstrap.py --mode experimental --allow-incomplete --timeout "$(AIC_SELFHOST_BOOTSTRAP_TIMEOUT)"

selfhost-release-provenance:
	python3 scripts/selfhost/release_provenance.py generate
	python3 scripts/selfhost/release_provenance.py verify

selfhost-mode-check:
	$(AIC) release selfhost-mode --mode supported --check

selfhost-default-mode-check:
	$(AIC) release selfhost-mode --mode default --check --approve-default

selfhost-default-build-check:
	@mkdir -p target/selfhost-default
	$(AIC) build compiler/aic/tools/aic_selfhost -o target/selfhost-default/aic_selfhost

selfhost-retirement-audit:
	python3 scripts/selfhost/retirement_audit.py --check --report target/selfhost-retirement/report.json

intrinsic-placeholder-guard:
	python3 scripts/ci/intrinsic_placeholder_guard.py

test-command-style-guard:
	python3 scripts/ci/test_command_style_guard.py

verify-intrinsics:
	./target/debug/aic verify-intrinsics std --json >/tmp/aic-verify-intrinsics.json
	python3 -m json.tool /tmp/aic-verify-intrinsics.json >/dev/null

std-doc-check:
	python3 scripts/ci/std_doc_coverage.py --check

examples-check:
	./scripts/ci/examples.sh check

examples-run:
	./scripts/ci/examples.sh run

integration-harness-offline:
	python3 scripts/ci/integration-harness.py --mode offline

integration-harness-live:
	python3 scripts/ci/integration-harness.py --mode live

cli-smoke:
	./scripts/ci/cli-smoke.sh

repro-check:
	./scripts/ci/repro-build-check.sh

security-audit:
	./scripts/ci/security-audit.sh

release-preflight: ci selfhost-bootstrap selfhost-release-provenance selfhost-mode-check selfhost-default-mode-check selfhost-default-build-check repro-check security-audit

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
	@test -f docs/backend-llvm.md
	@test -f docs/selfhost-driver.md
	@test -f docs/agent-tooling/README.md
	@test -f docs/agent-tooling/protocol-v1.md
	@test -f docs/agent-tooling/incremental-daemon.md
	@test -f docs/agent-recipes/README.md
	@test -f docs/agent-recipes/feature-loop.md
	@test -f docs/agent-recipes/bugfix-loop.md
	@test -f docs/agent-recipes/refactor-loop.md
	@test -f docs/agent-recipes/diagnostics-loop.md
	@test -f docs/agent-tooling/schemas/parse-response.schema.json
	@test -f docs/agent-tooling/schemas/check-response.schema.json
	@test -f docs/agent-tooling/schemas/build-response.schema.json
	@test -f docs/agent-tooling/schemas/fix-response.schema.json
	@test -f docs/agent-tooling/schemas/testgen-response.schema.json
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
	@test -f docs/async-event-loop.md
	@test -f docs/io-runtime/error-model.md
	@test -f docs/io-runtime/integration-harness.md
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
	@test -f docs/release/lts-policy.md
	@test -f docs/release/compatibility-matrix.json
	@test -f docs/security-ops/migration.md
	@test -f docs/security-ops/incident-response.md
	@test -f docs/security-threat-model.md
	@test -f docs/selfhost/README.md
	@test -f docs/selfhost/stage-matrix.md
	@test -f docs/selfhost/performance.md
	@test -f docs/selfhost/release-provenance.md
	@test -f docs/selfhost/supported-operation-runbook.md
	@test -f docs/selfhost/bootstrap-budgets.v1.json
	@test -f docs/selfhost/rust-reference-retirement.md
	@test -f docs/selfhost/rust-reference-retirement.v1.json
	@test -f scripts/selfhost/retirement_evidence.py
	@test -f docs/compatibility-migration-policy.md
	@test -f docs/errors/secure-networking-error-contract.v1.json
	@test -f docs/std-api-baseline.json
	@python3 -m json.tool tests/selfhost/parity_manifest.json >/dev/null
	@python3 -m json.tool tests/selfhost/stage_matrix_manifest.json >/dev/null
	@python3 -m json.tool docs/selfhost/bootstrap-budgets.v1.json >/dev/null
	@python3 -m json.tool docs/selfhost/rust-reference-retirement.v1.json >/dev/null
	@python3 -m json.tool docs/diagnostics.schema.json >/dev/null
	@python3 -m json.tool docs/agent-tooling/schemas/parse-response.schema.json >/dev/null
	@python3 -m json.tool docs/agent-tooling/schemas/check-response.schema.json >/dev/null
	@python3 -m json.tool docs/agent-tooling/schemas/build-response.schema.json >/dev/null
	@python3 -m json.tool docs/agent-tooling/schemas/fix-response.schema.json >/dev/null
	@python3 -m json.tool docs/agent-tooling/schemas/testgen-response.schema.json >/dev/null
	@python3 -m json.tool docs/release/compatibility-matrix.json >/dev/null
	@python3 -m json.tool docs/security-ops/tls-policy.v1.json >/dev/null
	@python3 -m json.tool docs/errors/secure-networking-error-contract.v1.json >/dev/null
	@grep -Fq "supported-operation-runbook.md" docs/index.md
	@grep -Fq "supported-operation-runbook.md" docs/selfhost/README.md
	@grep -Fq "## Operating Modes" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "## Host Setup" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "## Failure Triage" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "## Fallback And Rollback" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "## Issue Closure Policy" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "## Evidence Comment Template" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "AIC_MARKER_PATTERN" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "_dyld_start" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "core compiler" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "aic release selfhost-mode --mode supported --check" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "aic release selfhost-mode --mode default --check --approve-default" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "AIC_COMPILER_MODE=fallback" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "Rust Reference Retirement Audit" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "target/selfhost-retirement/report.json" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "python3 scripts/selfhost/retirement_audit.py --require-approved" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "bootstrap_report_sha256" docs/selfhost/rust-reference-retirement.md
	@grep -Fq "Rollback Validation Evidence" docs/selfhost/rust-reference-retirement.md
	@grep -Fq "Class Decision Evidence" docs/selfhost/rust-reference-retirement.md
	@grep -Fq "rollback.validation_evidence" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "retirement_decision" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "scripts/selfhost/retirement_evidence.py" docs/selfhost/supported-operation-runbook.md
	@grep -Fq -- "--evidence-root" docs/selfhost/supported-operation-runbook.md
	@grep -Fq "requires_production_budget_defaults" docs/selfhost/rust-reference-retirement.v1.json
	@grep -Fq "selfhost-default-build-check" Makefile
	@grep -Fq "selfhost-default-mode-check" Makefile
	@grep -Fq "selfhost-mode-check" Makefile
	@grep -Fq "selfhost-retirement-audit" Makefile
	@grep -Fq "selfhost-mode" docs/cli-contract.md
	@grep -Fq "fn tcp_send(handle: Int, payload: Bytes) -> Result[Int, NetError] effects { net }" docs/io-api-reference.md
	@grep -Fq "fn tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net }" docs/io-api-reference.md
	@grep -Fq "fn udp_send_to(handle: Int, addr: String, payload: Bytes) -> Result[Int, NetError] effects { net }" docs/io-api-reference.md
	@grep -Fq "fn tcp_send(handle: Int, payload: Bytes) -> Result[Int, NetError] effects { net }" docs/io-runtime/net-time-rand.md
	@grep -Fq "fn tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net }" docs/io-runtime/net-time-rand.md
	@grep -Fq "fn udp_send_to(handle: Int, addr: String, payload: Bytes) -> Result[Int, NetError] effects { net }" docs/io-runtime/net-time-rand.md
	@grep -Fq "async_wait_string(op, timeout_ms) -> Result[Bytes, NetError]" docs/async-event-loop.md
	@grep -Fq "Timeout => Bytes { data: \"\" }" docs/io-runtime/lifecycle-playbook.md
	@if [ "$${AIC_REQUIRE_PROTOCOL_EXAMPLES:-0}" = "1" ]; then \
		test -f docs/agent-recipes/secure-postgres-tls-scram-loop.md; \
		test -f docs/security-ops/postgres-tls-scram-replay.v1.json; \
		python3 -m json.tool docs/security-ops/postgres-tls-scram-replay.v1.json >/dev/null; \
	fi
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
