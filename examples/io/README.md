# IO Examples

This directory contains runnable examples for the current IO runtime surface.
Use these as the canonical entrypoint when checking `std.io`, `std.fs`, `std.env`, `std.proc`, `std.net`, `std.tls`, `std.http_server`, `std.router`, and `std.config` behavior.

## Index

- Input/output basics: `interactive_greeter.aic`, `interactive_cli.aic`, `stderr_logging.aic`
- File and byte workflows: `fs_all_ops.aic`, `fs_backup.aic`, `fs_async_await_bridge.aic`, `fs_async_runtime_controls.aic`, `fs_async_tasks.aic`, `async_nested_futures.aic`, `file_processor.aic`, `line_reader.aic`, `binary_file_copy.aic`, `stream_copy.aic`
- Environment and process flows: `env_config.aic`, `env_inspect.aic`, `cli_args.aic`, `process_pipeline.aic`, `subprocess_pipeline.aic`
- Network and server flows: `tcp_echo.aic`, `tcp_echo_client.aic`, `tcp_socket_tuning.aic`, `tls_connect.aic`, `tls_async_submit_wait.aic`, `tls_policy_defaults.aic`, `http_server_hello.aic`, `http_router.aic`
- Runtime control and resilience: `async_await_submit_bridge.aic`, `async_net_event_loop.aic`, `async_net_worker_pool.aic`, `async_runtime_pressure_gating.aic`, `retry_with_jitter.aic`, `signal_shutdown.aic`, `worker_pool.aic`, `connection_pool.aic`
- Testing and diagnostics: `effect_misuse_fs.aic`, `error_context_chain.aic`, `secure_error_contract.aic`, `prod_t1_intrinsics_runtime_smoke.aic`

## Validation

Most examples are runnable with:

```bash
cargo run --quiet --bin aic -- run examples/io/<example>.aic
```

For examples that depend on a live signal or external environment, check the file-level notes before running them in batch.
