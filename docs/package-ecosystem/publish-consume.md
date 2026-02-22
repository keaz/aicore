# Publish And Consume Packages

## Create and publish a package (public registry)

```bash
mkdir -p /tmp/http_client/src
cat > /tmp/http_client/aic.toml <<'TOML'
[package]
name = "http_client"
version = "1.2.0"
main = "src/main.aic"
TOML
cat > /tmp/http_client/src/main.aic <<'AIC'
module http_client.main;
fn get() -> Int { 42 }
AIC

aic pkg publish /tmp/http_client --registry /tmp/aic-registry
```

## Search and install into a consumer

```bash
mkdir -p /tmp/consumer/src
cat > /tmp/consumer/aic.toml <<'TOML'
[package]
name = "consumer_app"
version = "0.1.0"
main = "src/main.aic"
TOML
cat > /tmp/consumer/src/main.aic <<'AIC'
module consumer_app.main;
import http_client.main;
fn main() -> Int { http_client.main.get() }
AIC

aic pkg search http --registry /tmp/aic-registry
aic pkg install http_client@^1.0.0 --path /tmp/consumer --registry /tmp/aic-registry
aic check /tmp/consumer
```

## Private registry + scopes + auth

Use `aic.registry.json`:

```json
{
  "default": "public",
  "registries": {
    "public": { "path": "/tmp/public-registry" },
    "private": {
      "path": "/tmp/private-registry",
      "private": true,
      "token_env": "AIC_PRIVATE_TOKEN",
      "mirrors": ["/tmp/private-mirror"]
    }
  },
  "scopes": {
    "corp/": "private"
  }
}
```

Install scoped package:

```bash
AIC_PRIVATE_TOKEN=super-secret \
  aic pkg install corp/http_client@^1.0.0 --path /tmp/consumer --json
```

Expected deterministic failures:

- `E2117`: private registry token missing/invalid
- `E2118`: invalid credential source or config
- `E2114`: requirement conflict across requested versions
