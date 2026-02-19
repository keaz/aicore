SHELL := /usr/bin/env bash

CARGO ?= cargo
AIC ?= cargo run --quiet --bin aic --

.DEFAULT_GOAL := help

.PHONY: help init hooks-install hooks-uninstall ci ci-fast check fmt-check lint build test test-unit test-golden test-exec examples-check examples-run cli-smoke docs-check

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
	@echo "  make examples-check Validate example compile/check behavior"
	@echo "  make examples-run  Run executable example validations"
	@echo "  make cli-smoke     End-to-end CLI smoke test"
	@echo "  make docs-check    Validate docs and schema artifacts"

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

check: build test-unit test-golden test-exec examples-check examples-run cli-smoke docs-check

fmt-check:
	$(CARGO) fmt --all -- --check

lint:
	$(CARGO) clippy --all-targets --all-features

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

examples-check:
	./scripts/ci/examples.sh check

examples-run:
	./scripts/ci/examples.sh run

cli-smoke:
	./scripts/ci/cli-smoke.sh

docs-check:
	@test -f docs/spec.md
	@test -f docs/syntax.md
	@test -f docs/ir-schema.md
	@test -f docs/id-allocation.md
	@test -f docs/type-system.md
	@test -f docs/effect-system.md
	@test -f docs/contracts.md
	@test -f docs/diagnostic-codes.md
	@test -f docs/llvm-backend.md
	@python3 -m json.tool docs/diagnostics.schema.json >/dev/null
	@grep -q "aic init" README.md
	@grep -q "aic check" README.md
	@grep -q "aic fmt" README.md
	@grep -q "aic ir" README.md
	@grep -q "aic build" README.md
	@grep -q "aic run" README.md
