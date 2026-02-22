# Private Registry Setup Example

This example shows a reproducible CI-friendly private registry flow.

## 1. Registry Config (`aic.registry.json`)

```json
{
  "default": "public",
  "registries": {
    "public": { "path": "/tmp/aic/public" },
    "private": {
      "path": "/tmp/aic/private",
      "private": true,
      "token_env": "AIC_PRIVATE_TOKEN",
      "token_file": "/tmp/aic/private.token",
      "mirrors": ["/tmp/aic/private-mirror"]
    }
  },
  "scopes": {
    "corp/": "private"
  }
}
```

## 2. Publish

```bash
aic pkg publish path/to/pkg --registry private --registry-config aic.registry.json --token "$AIC_PRIVATE_TOKEN"
```

## 3. Install with Scope Routing

```bash
aic pkg install corp/http_client@^1.2.0 --path path/to/app --registry-config aic.registry.json
```

`corp/` packages resolve to `private`, while non-scoped packages use `public`.

## 4. Mirror Fallback

If the primary private registry path is unavailable or missing package metadata, `mirrors` are attempted in listed order.

## 5. Offline/CI Guidance

- Commit `aic.toml`, `aic.lock`, and `aic.registry.json`.
- Keep secrets out of source control (`token_env` + CI secret injection).
- Use `aic check --offline` / `aic build --offline` after lockfile refresh.
