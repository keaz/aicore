use serde::Serialize;

pub const CLI_CONTRACT_VERSION: &str = "1.0";
pub const EXIT_OK: i32 = 0;
pub const EXIT_DIAGNOSTIC_ERROR: i32 = 1;
pub const EXIT_USAGE_ERROR: i32 = 2;
pub const EXIT_INTERNAL_ERROR: i32 = 3;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommandContract {
    pub name: &'static str,
    pub description: &'static str,
    pub stable_flags: &'static [&'static str],
    pub output_modes: &'static [&'static str],
}

pub static COMMAND_CONTRACTS: &[CommandContract] = &[
    CommandContract {
        name: "init",
        description: "Initialize a new AICore project scaffold",
        stable_flags: &[],
        output_modes: &["text"],
    },
    CommandContract {
        name: "check",
        description: "Type/effect/contracts checks without compilation",
        stable_flags: &["--json", "--sarif", "--offline"],
        output_modes: &["text", "json", "sarif"],
    },
    CommandContract {
        name: "diag",
        description: "Alias of check focused on diagnostics output",
        stable_flags: &["--json", "--sarif", "--offline"],
        output_modes: &["text", "json", "sarif"],
    },
    CommandContract {
        name: "fmt",
        description: "Deterministic formatter",
        stable_flags: &["--check"],
        output_modes: &["text"],
    },
    CommandContract {
        name: "ir",
        description: "Emit current IR",
        stable_flags: &["--emit", "--offline"],
        output_modes: &["json", "text"],
    },
    CommandContract {
        name: "ir-migrate",
        description: "Migrate legacy IR JSON to current schema",
        stable_flags: &[],
        output_modes: &["json"],
    },
    CommandContract {
        name: "lock",
        description: "Generate deterministic package lockfile",
        stable_flags: &[],
        output_modes: &["text"],
    },
    CommandContract {
        name: "pkg",
        description: "Publish/install/search package registry workflows",
        stable_flags: &[
            "subcommands",
            "--json",
            "--registry",
            "--registry-config",
            "--token",
        ],
        output_modes: &["text", "json"],
    },
    CommandContract {
        name: "build",
        description: "Build executable/object/static-library artifacts",
        stable_flags: &["--artifact", "--debug-info", "--offline"],
        output_modes: &["text"],
    },
    CommandContract {
        name: "doc",
        description: "Generate package API docs",
        stable_flags: &["--output", "--offline"],
        output_modes: &["text"],
    },
    CommandContract {
        name: "std-compat",
        description: "Std API compatibility check",
        stable_flags: &["--check", "--baseline"],
        output_modes: &["text", "json"],
    },
    CommandContract {
        name: "run",
        description: "Build and execute entry program",
        stable_flags: &["--offline", "--sandbox"],
        output_modes: &["text"],
    },
    CommandContract {
        name: "explain",
        description: "Explain a diagnostic code with remediation guidance",
        stable_flags: &["--json"],
        output_modes: &["text", "json"],
    },
    CommandContract {
        name: "lsp",
        description: "Run Language Server Protocol server on stdio",
        stable_flags: &[],
        output_modes: &["json-rpc"],
    },
    CommandContract {
        name: "test",
        description: "Run AIC fixture harness categories",
        stable_flags: &["--mode", "--json"],
        output_modes: &["text", "json"],
    },
    CommandContract {
        name: "release",
        description: "Release security and operations workflows",
        stable_flags: &["subcommands"],
        output_modes: &["text", "json"],
    },
];

pub fn contract_json() -> serde_json::Value {
    serde_json::json!({
        "version": CLI_CONTRACT_VERSION,
        "exit_codes": {
            EXIT_OK.to_string(): "success",
            EXIT_DIAGNOSTIC_ERROR.to_string(): "diagnostic or runtime failure",
            EXIT_USAGE_ERROR.to_string(): "command-line usage error",
            EXIT_INTERNAL_ERROR.to_string(): "internal/tooling failure"
        },
        "commands": COMMAND_CONTRACTS,
        "policy": "breaking CLI changes require a versioned migration process"
    })
}

#[cfg(test)]
mod tests {
    use super::{contract_json, COMMAND_CONTRACTS};

    #[test]
    fn contracts_are_sorted_and_unique() {
        let mut names = COMMAND_CONTRACTS.iter().map(|c| c.name).collect::<Vec<_>>();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        names.sort();
        assert_eq!(names, sorted, "command contract names must be unique");
    }

    #[test]
    fn contract_json_contains_expected_shape() {
        let value = contract_json();
        assert_eq!(value["version"], "1.0");
        assert!(value["commands"].is_array());
        assert!(value["exit_codes"].is_object());
    }
}
