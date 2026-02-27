# Secure Postgres TLS/SCRAM Loop

This recipe is the canonical AGX5 loop for implementing and validating a Postgres-like secure client flow.
It maps implementation steps directly to deterministic checks and CI evidence.

## Protocol Example

- Canonical example: `examples/io/postgres_tls_scram_reference.aic`
- Deterministic replay contract: `docs/security-ops/postgres-tls-scram-replay.v1.json`
- Unified typed errors: `std.secure_errors` (`TLS_CERT_INVALID`, `PG_AUTH_FAILED`, `PG_TIMEOUT`, `POOL_MAX_SIZE_REACHED`)

Implementation-to-check mapping:

1. Frame startup/auth payloads with `std.buffer`.
Command check: `aic check examples/io/postgres_tls_scram_reference.aic`
2. Derive SCRAM proof with `std.crypto` (`pbkdf2_sha256`, `hmac_sha256_raw`).
Command check: `aic run examples/io/postgres_tls_scram_reference.aic -- success`
3. Enforce TLS secure default and explicit unsafe audit path.
Command check: `aic run examples/io/postgres_tls_scram_reference.aic -- bad-cert`
4. Validate timeout and pool-capacity negative semantics.
Command checks:
- `aic run examples/io/postgres_tls_scram_reference.aic -- timeout`
- `aic run examples/io/postgres_tls_scram_reference.aic -- pool-exhausted`
5. Validate deterministic replay suite.
Command check: `aic run examples/io/postgres_tls_scram_reference.aic`

## Fallback Behavior

If runtime environment cannot complete a live handshake, keep this replay contract as the source of truth:

- Use deterministic scenario args from `docs/security-ops/postgres-tls-scram-replay.v1.json`.
- Branch on typed causes (`code`, `category`, `retryable`) from `std.secure_errors`.
- Do not replace typed failures with generic `Io`/string-only errors.

## Platform Caveats

- Windows TLS backend limitations still apply (`TlsError::ProtocolError` paths).
- Replay scenarios are deterministic and do not depend on external network services.
- Pool semantics in this recipe are contract-level (`PoolErrorContract`) until full pool runtime APIs are widened.

## Docs Test Commands

<!-- docs-test:start -->
aic check examples/io/postgres_tls_scram_reference.aic
aic run examples/io/postgres_tls_scram_reference.aic
aic run examples/io/postgres_tls_scram_reference.aic -- bad-cert
aic run examples/io/postgres_tls_scram_reference.aic -- auth-failure
aic run examples/io/postgres_tls_scram_reference.aic -- timeout
aic run examples/io/postgres_tls_scram_reference.aic -- pool-exhausted
<!-- docs-test:end -->
