# Incremental Daemon Demo

This demo contains two packages:

- `dep/`: dependency package (`inc_dep`)
- `app/`: consumer package (`inc_app`) importing `inc_dep`

Run the app without daemon:

```bash
aic run examples/agent/incremental_demo/app
```

Run daemon workflow from repository root:

```bash
aic daemon < examples/agent/incremental_demo/requests/check_build_shutdown.jsonl
```

To observe dependency invalidation:

1. Start daemon and run a `check` request for `app/src/main.aic` twice.
2. Edit `examples/agent/incremental_demo/dep/src/main.aic`.
3. Run the same `check` request again.

The third response should return `cache_hit: false` with a new fingerprint.
