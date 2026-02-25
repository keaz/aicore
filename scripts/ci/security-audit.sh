#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

if [[ -x "$ROOT_DIR/target/debug/aic" ]]; then
  AIC=("$ROOT_DIR/target/debug/aic")
else
  AIC=(cargo run --quiet --bin aic --)
fi
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aic-security.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

REPORT="$TMP_DIR/security-audit.json"
"${AIC[@]}" release security-audit --root "$ROOT_DIR" --json >"$REPORT"
python3 -m json.tool "$REPORT" >/dev/null

if ! python3 - <<'PY' "$REPORT"; then
import json, sys
path = sys.argv[1]
with open(path, 'r', encoding='utf-8') as fh:
    data = json.load(fh)
if not data.get('ok'):
    print('security audit failed:')
    for issue in data.get('issues', []):
        print(f'  - {issue}')
    raise SystemExit(1)
print('security audit: ok')
PY
  exit 1
fi
