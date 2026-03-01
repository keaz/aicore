#!/usr/bin/env bash
set -euo pipefail

if ! command -v openssl >/dev/null 2>&1; then
  echo "openssl is required to initialize postgres TLS profile" >&2
  exit 1
fi

openssl req \
  -x509 \
  -newkey rsa:2048 \
  -keyout "$PGDATA/server.key" \
  -out "$PGDATA/server.crt" \
  -days 365 \
  -nodes \
  -subj "/CN=localhost"

chmod 600 "$PGDATA/server.key"
chmod 644 "$PGDATA/server.crt"

cat >>"$PGDATA/postgresql.conf" <<'CONF'
ssl = on
ssl_cert_file = 'server.crt'
ssl_key_file = 'server.key'
CONF
