# Task CLI Tooling Demo

This project is a small AIC CLI application that stores tasks in a newline-delimited text file.

Commands:
- `help`
- `list`
- `stats`
- `add <title>`
- `done <id>`

Environment:
- `AIC_TASK_CLI_FILE` overrides the storage file path.

Feature workflow against this project:

```bash
aic query --project examples/e7/task_cli_tooling --kind function --name 'validate*' --has-contract --json
aic symbols --project examples/e7/task_cli_tooling --json
aic context --project examples/e7/task_cli_tooling --for function mark_done --depth 2 --limit 5 --json
aic scaffold struct Task --field id:Int --field done:Bool --field title:String --with-invariant 'id >= 0'
aic synthesize --from spec validate_task_index --project examples/e7/task_cli_tooling --json
aic testgen --strategy boundary --for function validate_task_index --project examples/e7/task_cli_tooling --emit-dir tests/generated --json
aic checkpoint create --project examples/e7/task_cli_tooling --json
aic patch --preview examples/e7/task_cli_tooling/patches/add_storage_hint.json --project examples/e7/task_cli_tooling --json
```

Example run:

```bash
AIC_TASK_CLI_FILE=/tmp/task-cli.txt aic run examples/e7/task_cli_tooling -- add "write release notes"
AIC_TASK_CLI_FILE=/tmp/task-cli.txt aic run examples/e7/task_cli_tooling -- list
AIC_TASK_CLI_FILE=/tmp/task-cli.txt aic run examples/e7/task_cli_tooling -- done 0
AIC_TASK_CLI_FILE=/tmp/task-cli.txt aic run examples/e7/task_cli_tooling -- stats
```
