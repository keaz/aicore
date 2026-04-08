# Examples Index

This tree mixes onboarding examples, runnable subsystem demos, negative fixtures, and protocol/test inputs. Start with the curated tours, then move into the subsystem directories.

## Start with these

- `core/core_language_tour.aic` - broad syntax and type-system tour
- `core/traits_and_dispatch_tour.aic` - traits, impls, and dispatch patterns
- `core/async_trait_methods.aic` - async trait methods through generic and dyn dispatch
- `core/effects_capabilities_patterns_tour.aic` - effects, capabilities, and pattern matching
- `io/README.md` - IO/runtime examples grouped by subsystem
- `data/README.md` - bytes, JSON, regex, URL/HTTP, and time examples

## Learning examples

- `core/` - focused language examples and the curated tour programs
- `data/` - deterministic data/text/bytes examples
- `io/` - filesystem, process, env, HTTP server, networking, async, and concurrency examples
- `pkg/` - packages, workspaces, registries, provenance, and FFI examples
- `e2/` and `e5/` - tutorial-aligned multi-file and beginner-friendly examples

## Diagnostic and negative fixtures

- `e3/`, `e4/`, and files named `*_invalid_*` or `*_negative_*` are mostly compile-time or verification fixtures
- `verify/` contains proof-oriented examples used by verification docs and gates
- `agent/`, `e7/`, and `test/` contain workflow/protocol fixtures rather than first-run learning material

## Useful commands

- `make examples-check` to type-check the curated example set
- `make examples-run` to execute the runnable example set
- `aic check <file>` for a single example without execution
- `aic run <file>` for a single runnable example

## Related docs

- Example workflow docs: [../docs/examples/README.md](../docs/examples/README.md)
