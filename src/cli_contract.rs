use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const CLI_CONTRACT_VERSION: &str = "1.0";
pub const CLI_JSON_PROTOCOL_NAME: &str = "aic-compiler-json";
pub const CLI_JSON_PROTOCOL_CURRENT_VERSION: &str = "1.0";
pub const CLI_JSON_PROTOCOL_SUPPORTED_VERSIONS: &[&str] = &[CLI_JSON_PROTOCOL_CURRENT_VERSION];
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PhaseSchemaContract {
    pub phase: &'static str,
    pub schema_path: &'static str,
    pub example_path: &'static str,
    pub description: &'static str,
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
        stable_flags: &[
            "--json",
            "--sarif",
            "--offline",
            "subcommands:apply-fixes",
            "apply-fixes --dry-run",
        ],
        output_modes: &["text", "json", "sarif", "fix-json"],
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
        name: "daemon",
        description: "Run deterministic incremental check/build daemon on stdio",
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

pub static PHASE_SCHEMA_CONTRACTS: &[PhaseSchemaContract] = &[
    PhaseSchemaContract {
        phase: "parse",
        schema_path: "docs/agent-tooling/schemas/parse-response.schema.json",
        example_path: "examples/agent/protocol_parse.json",
        description: "Parser-only protocol envelope and diagnostics.",
    },
    PhaseSchemaContract {
        phase: "check",
        schema_path: "docs/agent-tooling/schemas/check-response.schema.json",
        example_path: "examples/agent/protocol_check.json",
        description: "Type/effect/contracts check protocol envelope and diagnostics.",
    },
    PhaseSchemaContract {
        phase: "build",
        schema_path: "docs/agent-tooling/schemas/build-response.schema.json",
        example_path: "examples/agent/protocol_build.json",
        description: "Artifact build protocol envelope and diagnostics.",
    },
    PhaseSchemaContract {
        phase: "fix",
        schema_path: "docs/agent-tooling/schemas/fix-response.schema.json",
        example_path: "examples/agent/protocol_fix.json",
        description: "Deterministic autofix planning/application protocol envelope.",
    },
];

pub fn contract_json(requested_versions: &[String]) -> Value {
    let requested_versions = requested_versions
        .iter()
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let (selected_version, invalid_requested_versions) =
        negotiate_protocol_version(&requested_versions);

    let mut schemas = BTreeMap::new();
    let mut examples = BTreeMap::new();
    for schema in PHASE_SCHEMA_CONTRACTS {
        schemas.insert(
            schema.phase,
            serde_json::json!({
                "path": schema.schema_path,
                "description": schema.description
            }),
        );
        examples.insert(schema.phase, schema.example_path);
    }

    serde_json::json!({
        "version": CLI_CONTRACT_VERSION,
        "exit_codes": {
            EXIT_OK.to_string(): "success",
            EXIT_DIAGNOSTIC_ERROR.to_string(): "diagnostic or runtime failure",
            EXIT_USAGE_ERROR.to_string(): "command-line usage error",
            EXIT_INTERNAL_ERROR.to_string(): "internal/tooling failure"
        },
        "protocol": {
            "name": CLI_JSON_PROTOCOL_NAME,
            "current_version": CLI_JSON_PROTOCOL_CURRENT_VERSION,
            "supported_versions": CLI_JSON_PROTOCOL_SUPPORTED_VERSIONS,
            "requested_versions": requested_versions,
            "selected_version": selected_version,
            "compatible": selected_version.is_some(),
            "invalid_requested_versions": invalid_requested_versions,
            "compatibility": {
                "rule": "same-major-and-server-version<=requested-version",
                "guarantee": "schema contracts are backward compatible within the same major protocol version"
            }
        },
        "schemas": schemas,
        "examples": examples,
        "commands": COMMAND_CONTRACTS,
        "policy": "breaking CLI changes require a versioned migration process"
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ProtocolVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

fn parse_protocol_version(raw: &str) -> Option<ProtocolVersion> {
    let pieces = raw.split('.').collect::<Vec<_>>();
    if pieces.len() < 2 || pieces.len() > 3 {
        return None;
    }
    let major = pieces[0].parse::<u32>().ok()?;
    let minor = pieces[1].parse::<u32>().ok()?;
    let patch = if pieces.len() == 3 {
        pieces[2].parse::<u32>().ok()?
    } else {
        0
    };
    Some(ProtocolVersion {
        major,
        minor,
        patch,
    })
}

pub fn negotiate_protocol_version(requested: &[String]) -> (Option<&'static str>, Vec<String>) {
    if requested.is_empty() {
        return (Some(CLI_JSON_PROTOCOL_CURRENT_VERSION), Vec::new());
    }

    let supported = CLI_JSON_PROTOCOL_SUPPORTED_VERSIONS
        .iter()
        .filter_map(|raw| parse_protocol_version(raw).map(|v| (*raw, v)))
        .collect::<Vec<_>>();

    let mut best: Option<(&'static str, ProtocolVersion)> = None;
    let mut invalid = Vec::new();
    for raw in requested {
        let Some(requested_version) = parse_protocol_version(raw) else {
            invalid.push(raw.clone());
            continue;
        };

        for (supported_raw, supported_version) in &supported {
            if supported_version.major != requested_version.major {
                continue;
            }
            if *supported_version > requested_version {
                continue;
            }
            match best {
                Some((_, current)) if current >= *supported_version => {}
                _ => best = Some((*supported_raw, *supported_version)),
            }
        }
    }

    (best.map(|(raw, _)| raw), invalid)
}

#[cfg(test)]
mod tests {
    use super::{
        contract_json, negotiate_protocol_version, COMMAND_CONTRACTS, PHASE_SCHEMA_CONTRACTS,
    };

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
        let value = contract_json(&[]);
        assert_eq!(value["version"], "1.0");
        assert!(value["commands"].is_array());
        assert!(value["exit_codes"].is_object());
        assert_eq!(value["protocol"]["name"], "aic-compiler-json");
        assert_eq!(value["protocol"]["selected_version"], "1.0");
        assert!(value["schemas"].is_object());
        assert_eq!(
            value["schemas"]["check"]["path"],
            PHASE_SCHEMA_CONTRACTS[1].schema_path
        );
        assert_eq!(
            value["examples"]["check"],
            PHASE_SCHEMA_CONTRACTS[1].example_path
        );
    }

    #[test]
    fn negotiation_accepts_compatible_minor() {
        let requested = vec!["1.4".to_string(), "1.2.3".to_string()];
        let (selected, invalid) = negotiate_protocol_version(&requested);
        assert_eq!(selected, Some("1.0"));
        assert!(invalid.is_empty());
    }

    #[test]
    fn negotiation_rejects_incompatible_major() {
        let requested = vec!["2.0".to_string()];
        let (selected, invalid) = negotiate_protocol_version(&requested);
        assert_eq!(selected, None);
        assert!(invalid.is_empty());
    }

    #[test]
    fn negotiation_reports_invalid_versions() {
        let requested = vec!["abc".to_string(), "1".to_string(), "1.0".to_string()];
        let (selected, invalid) = negotiate_protocol_version(&requested);
        assert_eq!(selected, Some("1.0"));
        assert_eq!(invalid, vec!["abc".to_string(), "1".to_string()]);
    }
}
