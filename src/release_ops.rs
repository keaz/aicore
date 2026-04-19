use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::cli_contract::CLI_CONTRACT_VERSION;
use crate::ir::CURRENT_IR_SCHEMA_VERSION;

pub const REPRO_MANIFEST_VERSION: u32 = 1;
pub const SBOM_FORMAT: &str = "aicore-sbom-v1";
pub const PROVENANCE_FORMAT: &str = "aicore-provenance-v1";
pub const COMPATIBILITY_POLICY_VERSION: &str = "1.0";
pub const LTS_POLICY_VERSION: &str = "1.0";
pub const SELFHOST_MODE_FORMAT: &str = "aicore-selfhost-compiler-mode-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReproManifest {
    pub version: u32,
    pub source_date_epoch: u64,
    pub root: String,
    pub file_count: usize,
    pub total_bytes: usize,
    pub digest: String,
    pub files: Vec<ReproFileEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReproFileEntry {
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SbomDocument {
    pub format: String,
    pub source_date_epoch: u64,
    pub root_package: SbomPackage,
    pub dependencies: Vec<SbomPackage>,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SbomPackage {
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceStatement {
    pub format: String,
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub artifact_bytes: u64,
    pub sbom_path: String,
    pub sbom_sha256: String,
    pub sbom_bytes: u64,
    pub manifest_path: Option<String>,
    pub manifest_sha256: Option<String>,
    pub key_id: Option<String>,
    pub algorithm: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityPolicy {
    pub version: String,
    pub ir_schema_version: u32,
    pub cli_contract_version: String,
    pub migration_commands: Vec<String>,
    pub required_docs: Vec<String>,
    pub required_workflows: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LtsPolicy {
    pub version: String,
    pub branches: Vec<LtsBranchPolicy>,
    pub compatibility_gates: Vec<String>,
    pub required_docs: Vec<String>,
    pub required_workflows: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LtsBranchPolicy {
    pub name: String,
    pub channel: String,
    pub support_window_months: u32,
    pub security_sla_days: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityAuditReport {
    pub ok: bool,
    pub checks: Vec<SecurityAuditCheck>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityAuditCheck {
    pub name: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfHostModeReport {
    pub format: String,
    pub requested_mode: String,
    pub active_compiler: String,
    pub default_enabled: bool,
    pub fallback_available: bool,
    pub default_approval: bool,
    pub ok: bool,
    pub problems: Vec<String>,
    pub required_gates: Vec<String>,
    pub evidence: SelfHostModeEvidence,
    pub rust_reference_retirement: RustReferenceRetirementPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfHostModeEvidence {
    pub bootstrap_report: String,
    pub provenance: String,
    pub bootstrap_ready: bool,
    pub bootstrap_status: Option<String>,
    pub reproducibility_ok: bool,
    pub parity_ok: bool,
    pub stage_matrix_ok: bool,
    pub performance_ok: bool,
    pub budget_overrides: Vec<String>,
    pub canonical_artifact: Option<String>,
    pub canonical_artifact_sha256: Option<String>,
    pub source_commit: Option<String>,
    pub worktree_dirty: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RustReferenceRetirementPolicy {
    pub allowed_in_this_issue: bool,
    pub requirement: String,
}

pub fn effective_source_date_epoch(explicit: Option<u64>) -> u64 {
    if let Some(value) = explicit {
        return value;
    }
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}

pub fn generate_repro_manifest(
    root: &Path,
    source_date_epoch: u64,
) -> anyhow::Result<ReproManifest> {
    let mut files = collect_files(root)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let mut digest = Sha256::new();
    let total_bytes = files.iter().map(|f| f.bytes).sum::<usize>();

    for file in &files {
        digest.update(file.path.as_bytes());
        digest.update([0]);
        digest.update(file.sha256.as_bytes());
        digest.update([0]);
        digest.update(file.bytes.to_string().as_bytes());
        digest.update([0]);
    }

    Ok(ReproManifest {
        version: REPRO_MANIFEST_VERSION,
        source_date_epoch,
        root: ".".to_string(),
        file_count: files.len(),
        total_bytes,
        digest: format!("{:x}", digest.finalize()),
        files,
    })
}

pub fn write_repro_manifest(path: &Path, manifest: &ReproManifest) -> anyhow::Result<()> {
    write_json(path, manifest)
}

pub fn read_repro_manifest(path: &Path) -> anyhow::Result<ReproManifest> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<ReproManifest>(&raw)?)
}

pub fn verify_repro_manifest(root: &Path, expected: &ReproManifest) -> anyhow::Result<Vec<String>> {
    let current = generate_repro_manifest(root, expected.source_date_epoch)?;
    let mut mismatches = Vec::new();

    if current.version != expected.version {
        mismatches.push(format!(
            "manifest version mismatch: expected {} got {}",
            expected.version, current.version
        ));
    }

    if current.digest != expected.digest {
        mismatches.push(format!(
            "manifest digest mismatch: expected {} got {}",
            expected.digest, current.digest
        ));
    }

    if current.file_count != expected.file_count {
        mismatches.push(format!(
            "file_count mismatch: expected {} got {}",
            expected.file_count, current.file_count
        ));
    }

    if current.total_bytes != expected.total_bytes {
        mismatches.push(format!(
            "total_bytes mismatch: expected {} got {}",
            expected.total_bytes, current.total_bytes
        ));
    }

    let expected_map = expected
        .files
        .iter()
        .map(|f| (f.path.clone(), f))
        .collect::<BTreeMap<_, _>>();
    let current_map = current
        .files
        .iter()
        .map(|f| (f.path.clone(), f))
        .collect::<BTreeMap<_, _>>();

    for (path, exp) in &expected_map {
        match current_map.get(path) {
            None => mismatches.push(format!("missing file: {}", path)),
            Some(cur) => {
                if exp.sha256 != cur.sha256 {
                    mismatches.push(format!(
                        "content hash mismatch for {}: expected {} got {}",
                        path, exp.sha256, cur.sha256
                    ));
                }
                if exp.bytes != cur.bytes {
                    mismatches.push(format!(
                        "size mismatch for {}: expected {} got {}",
                        path, exp.bytes, cur.bytes
                    ));
                }
            }
        }
    }

    for path in current_map.keys() {
        if !expected_map.contains_key(path) {
            mismatches.push(format!("unexpected file: {}", path));
        }
    }

    mismatches.sort();
    mismatches.dedup();
    Ok(mismatches)
}

pub fn generate_sbom(root: &Path, source_date_epoch: u64) -> anyhow::Result<SbomDocument> {
    let root_package = read_root_package(&root.join("Cargo.toml"))?;
    let mut dependencies = read_lockfile_packages(&root.join("Cargo.lock"))?;
    dependencies.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.version.cmp(&b.version))
            .then(a.source.cmp(&b.source))
            .then(a.checksum.cmp(&b.checksum))
    });

    let mut digest = Sha256::new();
    digest.update(root_package.name.as_bytes());
    digest.update([0]);
    digest.update(root_package.version.as_bytes());
    digest.update([0]);

    for dep in &dependencies {
        digest.update(dep.name.as_bytes());
        digest.update([0]);
        digest.update(dep.version.as_bytes());
        digest.update([0]);
        if let Some(source) = &dep.source {
            digest.update(source.as_bytes());
        }
        digest.update([0]);
        if let Some(checksum) = &dep.checksum {
            digest.update(checksum.as_bytes());
        }
        digest.update([0]);
    }

    Ok(SbomDocument {
        format: SBOM_FORMAT.to_string(),
        source_date_epoch,
        root_package,
        dependencies,
        digest: format!("{:x}", digest.finalize()),
    })
}

pub fn write_sbom(path: &Path, sbom: &SbomDocument) -> anyhow::Result<()> {
    write_json(path, sbom)
}

pub fn read_sbom(path: &Path) -> anyhow::Result<SbomDocument> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<SbomDocument>(&raw)?)
}

pub fn generate_provenance(
    artifact: &Path,
    sbom: &Path,
    manifest: Option<&Path>,
    key: &str,
    key_id: Option<String>,
) -> anyhow::Result<ProvenanceStatement> {
    let artifact_bytes = fs::read(artifact)?;
    let sbom_bytes = fs::read(sbom)?;

    let manifest_bytes = match manifest {
        Some(path) => Some(fs::read(path)?),
        None => None,
    };

    let artifact_sha256 = sha256_hex(&artifact_bytes);
    let sbom_sha256 = sha256_hex(&sbom_bytes);
    let manifest_sha256 = manifest_bytes.as_ref().map(|b| sha256_hex(b));

    let artifact_path = io_path_string(artifact);
    let sbom_path = io_path_string(sbom);
    let manifest_path = manifest.map(io_path_string);

    let payload = provenance_signing_payload(
        &artifact_path,
        &artifact_sha256,
        artifact_bytes.len() as u64,
        &sbom_path,
        &sbom_sha256,
        sbom_bytes.len() as u64,
        manifest_path.as_deref(),
        manifest_sha256.as_deref(),
        key_id.as_deref(),
    );

    let signature = hmac_sha256_hex(key.as_bytes(), payload.as_bytes());

    Ok(ProvenanceStatement {
        format: PROVENANCE_FORMAT.to_string(),
        artifact_path,
        artifact_sha256,
        artifact_bytes: artifact_bytes.len() as u64,
        sbom_path,
        sbom_sha256,
        sbom_bytes: sbom_bytes.len() as u64,
        manifest_path,
        manifest_sha256,
        key_id,
        algorithm: "hmac-sha256".to_string(),
        signature,
    })
}

pub fn write_provenance(path: &Path, provenance: &ProvenanceStatement) -> anyhow::Result<()> {
    write_json(path, provenance)
}

pub fn read_provenance(path: &Path) -> anyhow::Result<ProvenanceStatement> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<ProvenanceStatement>(&raw)?)
}

pub fn verify_provenance(
    provenance: &ProvenanceStatement,
    key: &str,
) -> anyhow::Result<Vec<String>> {
    let mut errors = Vec::new();

    if provenance.algorithm != "hmac-sha256" {
        errors.push(format!(
            "unsupported signature algorithm: {}",
            provenance.algorithm
        ));
        return Ok(errors);
    }

    let artifact_bytes = fs::read(&provenance.artifact_path)?;
    let sbom_bytes = fs::read(&provenance.sbom_path)?;

    let artifact_sha = sha256_hex(&artifact_bytes);
    if artifact_sha != provenance.artifact_sha256 {
        errors.push(format!(
            "artifact hash mismatch: expected {} got {}",
            provenance.artifact_sha256, artifact_sha
        ));
    }

    let sbom_sha = sha256_hex(&sbom_bytes);
    if sbom_sha != provenance.sbom_sha256 {
        errors.push(format!(
            "sbom hash mismatch: expected {} got {}",
            provenance.sbom_sha256, sbom_sha
        ));
    }

    let manifest_sha_actual = match &provenance.manifest_path {
        Some(path) => Some(sha256_hex(&fs::read(path)?)),
        None => None,
    };

    if manifest_sha_actual != provenance.manifest_sha256 {
        errors.push(format!(
            "manifest hash mismatch: expected {:?} got {:?}",
            provenance.manifest_sha256, manifest_sha_actual
        ));
    }

    let payload = provenance_signing_payload(
        &provenance.artifact_path,
        &provenance.artifact_sha256,
        provenance.artifact_bytes,
        &provenance.sbom_path,
        &provenance.sbom_sha256,
        provenance.sbom_bytes,
        provenance.manifest_path.as_deref(),
        provenance.manifest_sha256.as_deref(),
        provenance.key_id.as_deref(),
    );
    let expected_sig = hmac_sha256_hex(key.as_bytes(), payload.as_bytes());
    if expected_sig != provenance.signature {
        errors.push("signature mismatch".to_string());
    }

    Ok(errors)
}

pub fn verify_checksum_file(artifact: &Path, checksum_file: &Path) -> anyhow::Result<Vec<String>> {
    let (expected_hash, expected_name) = parse_checksum_entry(checksum_file)?;
    let artifact_bytes = fs::read(artifact)?;
    let actual_hash = sha256_hex(&artifact_bytes);
    let artifact_name = artifact
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut errors = Vec::new();
    if expected_hash != actual_hash {
        errors.push(format!(
            "checksum mismatch: expected {} got {}",
            expected_hash, actual_hash
        ));
    }
    if !expected_name.is_empty() && expected_name != artifact_name {
        errors.push(format!(
            "artifact name mismatch: checksum entry `{}` does not match `{}`",
            expected_name, artifact_name
        ));
    }
    Ok(errors)
}

fn parse_checksum_entry(path: &Path) -> anyhow::Result<(String, String)> {
    let raw = fs::read_to_string(path)?;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            anyhow::bail!("invalid checksum entry in {}", path.display());
        };

        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!("invalid checksum hash in {}", path.display());
        }
        let normalized_name = name.trim_start_matches('*').to_string();
        return Ok((hash.to_ascii_lowercase(), normalized_name));
    }

    anyhow::bail!("checksum file has no entries: {}", path.display())
}

pub fn compatibility_policy() -> CompatibilityPolicy {
    CompatibilityPolicy {
        version: COMPATIBILITY_POLICY_VERSION.to_string(),
        ir_schema_version: CURRENT_IR_SCHEMA_VERSION,
        cli_contract_version: CLI_CONTRACT_VERSION.to_string(),
        migration_commands: vec![
            "aic migrate <path> --dry-run --json".to_string(),
            "aic ir-migrate <legacy-ir.json>".to_string(),
            "aic std-compat --check --baseline docs/std-api-baseline.json".to_string(),
            "aic release policy --check".to_string(),
            "aic release lts --check".to_string(),
        ],
        required_docs: vec![
            "docs/spec.md".to_string(),
            "docs/compatibility-migration-policy.md".to_string(),
            "docs/security-ops/migration.md".to_string(),
            "docs/security-threat-model.md".to_string(),
            "docs/release-security-ops.md".to_string(),
            "docs/release/matrix.md".to_string(),
            "docs/release/lts-policy.md".to_string(),
            "docs/release/compatibility-matrix.json".to_string(),
        ],
        required_workflows: vec![
            ".github/workflows/ci.yml".to_string(),
            ".github/workflows/release.yml".to_string(),
            ".github/workflows/security.yml".to_string(),
        ],
    }
}

pub fn check_compatibility_policy(root: &Path, policy: &CompatibilityPolicy) -> Vec<String> {
    let mut problems = Vec::new();

    for doc in &policy.required_docs {
        if !root.join(doc).is_file() {
            problems.push(format!("missing required doc: {}", doc));
        }
    }

    for workflow in &policy.required_workflows {
        if !root.join(workflow).is_file() {
            problems.push(format!("missing required workflow: {}", workflow));
        }
    }

    if policy.ir_schema_version == 0 {
        problems.push("invalid ir_schema_version: 0".to_string());
    }

    if policy.cli_contract_version.trim().is_empty() {
        problems.push("empty cli_contract_version".to_string());
    }

    problems.sort();
    problems.dedup();
    problems
}

pub fn lts_policy() -> LtsPolicy {
    let main_sla = BTreeMap::from([
        ("critical".to_string(), 2),
        ("high".to_string(), 7),
        ("medium".to_string(), 30),
    ]);
    let stable_sla = BTreeMap::from([
        ("critical".to_string(), 2),
        ("high".to_string(), 7),
        ("medium".to_string(), 30),
    ]);

    LtsPolicy {
        version: LTS_POLICY_VERSION.to_string(),
        branches: vec![
            LtsBranchPolicy {
                name: "main".to_string(),
                channel: "active".to_string(),
                support_window_months: 12,
                security_sla_days: main_sla,
            },
            LtsBranchPolicy {
                name: "release/0.1".to_string(),
                channel: "lts".to_string(),
                support_window_months: 18,
                security_sla_days: stable_sla,
            },
        ],
        compatibility_gates: vec![
            "cargo run --quiet --bin aic -- release policy --check".to_string(),
            "cargo run --quiet --bin aic -- release lts --check".to_string(),
            "cargo run --quiet --bin aic -- std-compat --check --baseline docs/std-api-baseline.json"
                .to_string(),
        ],
        required_docs: vec![
            "docs/release/lts-policy.md".to_string(),
            "docs/release/compatibility-matrix.json".to_string(),
        ],
        required_workflows: vec![
            ".github/workflows/ci.yml".to_string(),
            ".github/workflows/release.yml".to_string(),
            ".github/workflows/security.yml".to_string(),
        ],
    }
}

pub fn check_lts_policy(root: &Path, policy: &LtsPolicy) -> Vec<String> {
    let mut problems = Vec::new();

    if policy.version.trim().is_empty() {
        problems.push("empty lts policy version".to_string());
    }

    if policy.branches.is_empty() {
        problems.push("lts policy has no branches".to_string());
    }

    for branch in &policy.branches {
        if branch.name.trim().is_empty() {
            problems.push("lts branch with empty name".to_string());
        }
        if branch.channel.trim().is_empty() {
            problems.push(format!("lts branch `{}` has empty channel", branch.name));
        }
        if branch.support_window_months == 0 {
            problems.push(format!(
                "lts branch `{}` has zero support_window_months",
                branch.name
            ));
        }

        let critical = branch
            .security_sla_days
            .get("critical")
            .copied()
            .unwrap_or(0);
        let high = branch.security_sla_days.get("high").copied().unwrap_or(0);
        if critical == 0 {
            problems.push(format!(
                "lts branch `{}` missing critical security SLA days",
                branch.name
            ));
        }
        if high == 0 {
            problems.push(format!(
                "lts branch `{}` missing high security SLA days",
                branch.name
            ));
        }
        if critical > 7 {
            problems.push(format!(
                "lts branch `{}` critical security SLA exceeds 7 days ({})",
                branch.name, critical
            ));
        }
        if high > 14 {
            problems.push(format!(
                "lts branch `{}` high security SLA exceeds 14 days ({})",
                branch.name, high
            ));
        }
    }

    for doc in &policy.required_docs {
        if !root.join(doc).is_file() {
            problems.push(format!("missing required lts doc: {}", doc));
        }
    }

    for workflow in &policy.required_workflows {
        if !root.join(workflow).is_file() {
            problems.push(format!("missing required lts workflow: {}", workflow));
        }
    }

    let matrix_path = root.join("docs/release/compatibility-matrix.json");
    if matrix_path.is_file() {
        match fs::read_to_string(&matrix_path) {
            Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(parsed) => {
                    if parsed
                        .get("schema_version")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                        != 1
                    {
                        problems.push("compatibility matrix schema_version must be 1".to_string());
                    }
                    let names = parsed
                        .get("branches")
                        .and_then(|v| v.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|item| {
                                    item.get("name")
                                        .and_then(|value| value.as_str())
                                        .map(|value| value.to_string())
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    for expected in &policy.branches {
                        if !names.contains(&expected.name) {
                            problems.push(format!(
                                "compatibility matrix missing lts branch `{}`",
                                expected.name
                            ));
                        }
                    }
                }
                Err(err) => problems.push(format!(
                    "compatibility matrix is invalid JSON ({}): {}",
                    matrix_path.display(),
                    err
                )),
            },
            Err(err) => problems.push(format!(
                "failed to read compatibility matrix {}: {}",
                matrix_path.display(),
                err
            )),
        }
    }

    for workflow in [
        ".github/workflows/ci.yml",
        ".github/workflows/release.yml",
        ".github/workflows/security.yml",
    ] {
        let path = root.join(workflow);
        if !path.is_file() {
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(raw) => {
                if !raw.contains("release lts --check") {
                    problems.push(format!(
                        "workflow {} missing `release lts --check` gate",
                        workflow
                    ));
                }
                if workflow.ends_with("security.yml") && !raw.contains("cron:") {
                    problems.push(
                        "security workflow missing scheduled cadence for SLA enforcement"
                            .to_string(),
                    );
                }
            }
            Err(err) => {
                problems.push(format!("failed to read workflow {}: {}", workflow, err));
            }
        }
    }

    problems.sort();
    problems.dedup();
    problems
}

pub fn evaluate_selfhost_compiler_mode(
    root: &Path,
    requested_mode: &str,
    bootstrap_report: &Path,
    provenance: &Path,
    default_approval: bool,
) -> SelfHostModeReport {
    let normalized_mode = normalize_selfhost_mode(requested_mode);
    let mut problems = Vec::new();
    let mut evidence = empty_selfhost_evidence(bootstrap_report, provenance);
    let evidence_required = matches!(normalized_mode.as_deref(), Some("supported" | "default"));

    if normalized_mode.is_none() {
        problems.push(format!(
            "unsupported compiler mode `{}`; expected reference, experimental, supported, default, or fallback",
            requested_mode
        ));
    }

    if evidence_required {
        evidence = load_selfhost_mode_evidence(root, bootstrap_report, provenance, &mut problems);
        require_supported_selfhost_evidence(&evidence, &mut problems);
    } else if normalized_mode.as_deref() == Some("experimental") {
        evidence = load_optional_selfhost_mode_evidence(root, bootstrap_report, provenance);
    }

    if normalized_mode.as_deref() == Some("default") && !default_approval {
        problems.push(
            "default self-host mode requires explicit approval after all production gates pass"
                .to_string(),
        );
    }

    problems.sort();
    problems.dedup();

    let mode = normalized_mode.unwrap_or_else(|| requested_mode.to_string());
    let active_compiler = match mode.as_str() {
        "experimental" | "supported" | "default" => "aic-selfhost",
        _ => "rust-reference",
    };

    SelfHostModeReport {
        format: SELFHOST_MODE_FORMAT.to_string(),
        requested_mode: mode.clone(),
        active_compiler: active_compiler.to_string(),
        default_enabled: mode == "default" && default_approval && problems.is_empty(),
        fallback_available: true,
        default_approval,
        ok: problems.is_empty(),
        problems,
        required_gates: vec![
            "cargo build --locked".to_string(),
            "cargo test --locked".to_string(),
            "make selfhost-parity-candidate".to_string(),
            "make selfhost-bootstrap".to_string(),
            "make selfhost-stage-matrix".to_string(),
            "make selfhost-release-provenance".to_string(),
            "make release-preflight".to_string(),
            "make examples-check".to_string(),
            "make examples-run".to_string(),
            "make docs-check".to_string(),
            "make ci".to_string(),
        ],
        evidence,
        rust_reference_retirement: RustReferenceRetirementPolicy {
            allowed_in_this_issue: false,
            requirement: "keep the Rust reference compiler available until a separate retirement issue is opened, approved, implemented, and verified".to_string(),
        },
    }
}

pub fn normalize_selfhost_mode(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" => Some("reference".to_string()),
        "reference" | "rust" | "rust-reference" => Some("reference".to_string()),
        "experimental" | "experiment" => Some("experimental".to_string()),
        "selfhost" | "self-host" | "supported" => Some("supported".to_string()),
        "default" => Some("default".to_string()),
        "fallback" | "rust-fallback" => Some("fallback".to_string()),
        _ => None,
    }
}

fn empty_selfhost_evidence(bootstrap_report: &Path, provenance: &Path) -> SelfHostModeEvidence {
    SelfHostModeEvidence {
        bootstrap_report: bootstrap_report.display().to_string(),
        provenance: provenance.display().to_string(),
        bootstrap_ready: false,
        bootstrap_status: None,
        reproducibility_ok: false,
        parity_ok: false,
        stage_matrix_ok: false,
        performance_ok: false,
        budget_overrides: Vec::new(),
        canonical_artifact: None,
        canonical_artifact_sha256: None,
        source_commit: None,
        worktree_dirty: None,
    }
}

fn load_optional_selfhost_mode_evidence(
    root: &Path,
    bootstrap_report: &Path,
    provenance: &Path,
) -> SelfHostModeEvidence {
    let mut ignored = Vec::new();
    load_selfhost_mode_evidence(root, bootstrap_report, provenance, &mut ignored)
}

fn load_selfhost_mode_evidence(
    root: &Path,
    bootstrap_report: &Path,
    provenance: &Path,
    problems: &mut Vec<String>,
) -> SelfHostModeEvidence {
    let bootstrap_path = resolve_release_path(root, bootstrap_report);
    let provenance_path = resolve_release_path(root, provenance);
    let mut evidence = empty_selfhost_evidence(bootstrap_report, provenance);

    match read_json_value(&bootstrap_path) {
        Ok(report) => {
            if report.get("format").and_then(Value::as_str) != Some("aicore-selfhost-bootstrap-v1")
            {
                problems.push(format!(
                    "bootstrap report has unexpected format: {}",
                    bootstrap_report.display()
                ));
            }
            evidence.bootstrap_ready = report
                .get("ready")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            evidence.bootstrap_status = report
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string);
            evidence.reproducibility_ok = report
                .get("reproducibility")
                .and_then(|value| value.get("matches"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            evidence.performance_ok = report
                .get("performance")
                .and_then(|value| value.get("ok"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            evidence.budget_overrides = object_keys(
                report
                    .get("performance")
                    .and_then(|value| value.get("budget_source"))
                    .and_then(|value| value.get("overrides")),
            );
        }
        Err(err) => problems.push(format!(
            "missing or invalid self-host bootstrap report {}: {}",
            bootstrap_report.display(),
            err
        )),
    }

    match read_json_value(&provenance_path) {
        Ok(report) => {
            if report.get("format").and_then(Value::as_str)
                != Some("aicore-selfhost-release-provenance-v1")
            {
                problems.push(format!(
                    "self-host provenance has unexpected format: {}",
                    provenance.display()
                ));
            }
            let validation = report.get("validation");
            evidence.parity_ok = validation
                .and_then(|value| value.get("parity_ok"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            evidence.stage_matrix_ok = validation
                .and_then(|value| value.get("stage_matrix_ok"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let provenance_performance_ok = validation
                .and_then(|value| value.get("performance_ok"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            evidence.performance_ok = evidence.performance_ok && provenance_performance_ok;
            evidence.budget_overrides.extend(object_keys(
                validation.and_then(|value| value.get("budget_overrides")),
            ));
            evidence.budget_overrides.sort();
            evidence.budget_overrides.dedup();
            evidence.canonical_artifact = report
                .get("canonical_artifact")
                .and_then(|value| value.get("path"))
                .and_then(Value::as_str)
                .map(str::to_string);
            evidence.canonical_artifact_sha256 = report
                .get("canonical_artifact")
                .and_then(|value| value.get("sha256"))
                .and_then(Value::as_str)
                .map(str::to_string);
            evidence.source_commit = report
                .get("source")
                .and_then(|value| value.get("commit"))
                .and_then(Value::as_str)
                .map(str::to_string);
            evidence.worktree_dirty = report
                .get("source")
                .and_then(|value| value.get("worktree_dirty"))
                .and_then(Value::as_bool);
        }
        Err(err) => problems.push(format!(
            "missing or invalid self-host release provenance {}: {}",
            provenance.display(),
            err
        )),
    }

    evidence
}

fn require_supported_selfhost_evidence(
    evidence: &SelfHostModeEvidence,
    problems: &mut Vec<String>,
) {
    if !evidence.bootstrap_ready || evidence.bootstrap_status.as_deref() != Some("supported-ready")
    {
        problems.push("self-host bootstrap report is not supported-ready".to_string());
    }
    if !evidence.reproducibility_ok {
        problems.push("self-host stage reproducibility did not pass".to_string());
    }
    if !evidence.parity_ok {
        problems.push("self-host parity evidence did not pass".to_string());
    }
    if !evidence.stage_matrix_ok {
        problems.push("self-host package/workspace matrix evidence did not pass".to_string());
    }
    if !evidence.performance_ok {
        problems.push("self-host performance evidence did not pass".to_string());
    }
    if !evidence.budget_overrides.is_empty() {
        problems.push(format!(
            "self-host performance budget overrides are not allowed for supported/default mode: {}",
            evidence.budget_overrides.join(",")
        ));
    }
    if evidence.canonical_artifact.is_none() || evidence.canonical_artifact_sha256.is_none() {
        problems.push(
            "self-host release provenance is missing canonical artifact evidence".to_string(),
        );
    }
    if evidence.worktree_dirty.unwrap_or(true) {
        problems.push(
            "self-host release provenance was generated from a dirty tracked worktree".to_string(),
        );
    }
}

fn resolve_release_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn read_json_value(path: &Path) -> anyhow::Result<Value> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<Value>(&raw)?)
}

fn object_keys(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_object)
        .map(|items| items.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

pub fn run_security_audit(root: &Path) -> anyhow::Result<SecurityAuditReport> {
    let mut checks = Vec::new();
    let mut issues = Vec::new();

    let threat_model = root.join("docs/security-threat-model.md");
    let threat_model_check = if threat_model.is_file() {
        let text = fs::read_to_string(&threat_model)?;
        let required_sections = [
            "## Scope",
            "## Assets",
            "## Trust Boundaries",
            "## Threat Scenarios",
            "## Mitigations",
            "## Residual Risk",
        ];
        let missing = required_sections
            .iter()
            .filter(|s| !text.contains(**s))
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        if missing.is_empty() {
            SecurityAuditCheck {
                name: "threat_model_documented".to_string(),
                passed: true,
                details: "threat model present with required sections".to_string(),
            }
        } else {
            issues.push(format!(
                "threat model missing sections: {}",
                missing.join(", ")
            ));
            SecurityAuditCheck {
                name: "threat_model_documented".to_string(),
                passed: false,
                details: format!("missing sections: {}", missing.join(", ")),
            }
        }
    } else {
        issues.push("threat model file missing: docs/security-threat-model.md".to_string());
        SecurityAuditCheck {
            name: "threat_model_documented".to_string(),
            passed: false,
            details: "missing docs/security-threat-model.md".to_string(),
        }
    };
    checks.push(threat_model_check);

    let unsafe_tokens = scan_for_unsafe_keyword(&root.join("src"))?;
    if unsafe_tokens.is_empty() {
        checks.push(SecurityAuditCheck {
            name: "unsafe_rust_scan".to_string(),
            passed: true,
            details: "no `unsafe` token found in src/".to_string(),
        });
    } else {
        let locations = unsafe_tokens
            .iter()
            .map(|p| stable_path_key(p))
            .collect::<Vec<_>>();
        issues.push(format!(
            "unsafe token found in source files: {}",
            locations.join(", ")
        ));
        checks.push(SecurityAuditCheck {
            name: "unsafe_rust_scan".to_string(),
            passed: false,
            details: format!("unsafe token present in {} files", locations.len()),
        });
    }

    let workflow_check = audit_workflows(root)?;
    if !workflow_check.passed {
        issues.push(workflow_check.details.clone());
    }
    checks.push(workflow_check);

    Ok(SecurityAuditReport {
        ok: issues.is_empty(),
        checks,
        issues,
    })
}

fn audit_workflows(root: &Path) -> anyhow::Result<SecurityAuditCheck> {
    let workflows_dir = root.join(".github/workflows");
    if !workflows_dir.is_dir() {
        return Ok(SecurityAuditCheck {
            name: "workflow_pinning".to_string(),
            passed: false,
            details: "missing .github/workflows".to_string(),
        });
    }

    let mut offenders = Vec::new();
    let mut release_has_locked = false;
    let mut release_has_permissions = false;
    let mut release_has_concurrency = false;

    for entry in fs::read_dir(&workflows_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("yml") {
            continue;
        }

        let text = fs::read_to_string(&path)?;
        if path.file_name().and_then(|s| s.to_str()) == Some("release.yml") {
            release_has_locked = text.contains("--locked");
            release_has_permissions = text.contains("permissions:");
            release_has_concurrency = text.contains("concurrency:");
        }

        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("uses:") {
                continue;
            }
            let uses_ref = trimmed.trim_start_matches("uses:").trim();
            if uses_ref.contains("@main") || uses_ref.contains("@master") {
                offenders.push(format!(
                    "{} uses floating ref {}",
                    stable_path_key(&path),
                    uses_ref
                ));
            }
        }
    }

    if !release_has_locked {
        offenders.push("release workflow missing --locked builds".to_string());
    }
    if !release_has_permissions {
        offenders.push("release workflow missing permissions section".to_string());
    }
    if !release_has_concurrency {
        offenders.push("release workflow missing concurrency section".to_string());
    }

    if offenders.is_empty() {
        Ok(SecurityAuditCheck {
            name: "workflow_pinning".to_string(),
            passed: true,
            details: "workflow action refs and release hardening checks passed".to_string(),
        })
    } else {
        Ok(SecurityAuditCheck {
            name: "workflow_pinning".to_string(),
            passed: false,
            details: offenders.join("; "),
        })
    }
}

fn provenance_signing_payload(
    artifact_path: &str,
    artifact_sha256: &str,
    artifact_bytes: u64,
    sbom_path: &str,
    sbom_sha256: &str,
    sbom_bytes: u64,
    manifest_path: Option<&str>,
    manifest_sha256: Option<&str>,
    key_id: Option<&str>,
) -> String {
    format!(
        "format={format}\nartifact_path={artifact_path}\nartifact_sha256={artifact_sha256}\nartifact_bytes={artifact_bytes}\nsbom_path={sbom_path}\nsbom_sha256={sbom_sha256}\nsbom_bytes={sbom_bytes}\nmanifest_path={manifest_path}\nmanifest_sha256={manifest_sha256}\nkey_id={key_id}\n",
        format = PROVENANCE_FORMAT,
        artifact_path = artifact_path,
        artifact_sha256 = artifact_sha256,
        artifact_bytes = artifact_bytes,
        sbom_path = sbom_path,
        sbom_sha256 = sbom_sha256,
        sbom_bytes = sbom_bytes,
        manifest_path = manifest_path.unwrap_or(""),
        manifest_sha256 = manifest_sha256.unwrap_or(""),
        key_id = key_id.unwrap_or(""),
    )
}

fn collect_files(root: &Path) -> anyhow::Result<Vec<ReproFileEntry>> {
    let mut out = Vec::new();

    if root.is_file() {
        let bytes = fs::read(root)?;
        out.push(ReproFileEntry {
            path: root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("<unknown>")
                .to_string(),
            sha256: sha256_hex(&bytes),
            bytes: bytes.len(),
        });
        return Ok(out);
    }

    collect_files_from(root, root, &mut out)?;
    Ok(out)
}

fn collect_files_from(
    base: &Path,
    dir: &Path,
    out: &mut Vec<ReproFileEntry>,
) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if should_skip_path(&path) {
            continue;
        }
        if path.is_dir() {
            collect_files_from(base, &path, out)?;
            continue;
        }

        let content = fs::read(&path)?;
        let rel = path.strip_prefix(base).unwrap_or(path.as_path());
        let path_key = stable_path_key(rel);
        out.push(ReproFileEntry {
            path: path_key,
            sha256: sha256_hex(&content),
            bytes: content.len(),
        });
    }

    Ok(())
}

fn should_skip_path(path: &Path) -> bool {
    let skip_names = [
        ".aic",
        ".aic-cache",
        ".aic-replay",
        ".ci-local-bin",
        ".git",
        ".idea",
        ".vscode",
        ".vscode-test",
        "dist",
        "node_modules",
        "target",
        "target-linux",
    ];
    path.components().any(|c| match c {
        Component::Normal(segment) => {
            let seg = segment.to_string_lossy();
            skip_names.iter().any(|skip| seg == *skip)
        }
        _ => false,
    })
}

fn read_root_package(path: &Path) -> anyhow::Result<SbomPackage> {
    let text = fs::read_to_string(path)?;
    let mut in_package = false;
    let mut name = None;
    let mut version = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if line.starts_with("name") {
            name = parse_toml_string_value(line);
        } else if line.starts_with("version") {
            version = parse_toml_string_value(line);
        }
    }

    let name = name.ok_or_else(|| anyhow::anyhow!("missing package.name in Cargo.toml"))?;
    let version =
        version.ok_or_else(|| anyhow::anyhow!("missing package.version in Cargo.toml"))?;

    Ok(SbomPackage {
        name,
        version,
        source: Some("path".to_string()),
        checksum: None,
    })
}

fn read_lockfile_packages(path: &Path) -> anyhow::Result<Vec<SbomPackage>> {
    let text = fs::read_to_string(path)?;
    let mut packages = Vec::new();
    let mut current: Option<SbomPackage> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line == "[[package]]" {
            if let Some(pkg) = current.take() {
                if !pkg.name.is_empty() && !pkg.version.is_empty() {
                    packages.push(pkg);
                }
            }
            current = Some(SbomPackage {
                name: String::new(),
                version: String::new(),
                source: None,
                checksum: None,
            });
            continue;
        }

        let Some(pkg) = current.as_mut() else {
            continue;
        };

        if line.starts_with("name") {
            if let Some(value) = parse_toml_string_value(line) {
                pkg.name = value;
            }
        } else if line.starts_with("version") {
            if let Some(value) = parse_toml_string_value(line) {
                pkg.version = value;
            }
        } else if line.starts_with("source") {
            pkg.source = parse_toml_string_value(line);
        } else if line.starts_with("checksum") {
            pkg.checksum = parse_toml_string_value(line);
        }
    }

    if let Some(pkg) = current.take() {
        if !pkg.name.is_empty() && !pkg.version.is_empty() {
            packages.push(pkg);
        }
    }

    Ok(packages)
}

fn parse_toml_string_value(line: &str) -> Option<String> {
    let (_, value) = line.split_once('=')?;
    let value = value.trim();
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    Some(value.to_string())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    fs::write(path, json)?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn stable_path_key(path: &Path) -> String {
    let mut key = String::new();
    for component in path.components() {
        let part = match component {
            Component::RootDir => continue,
            Component::Prefix(prefix) => prefix.as_os_str().to_string_lossy().into_owned(),
            Component::CurDir => ".".to_string(),
            Component::ParentDir => "..".to_string(),
            Component::Normal(segment) => segment.to_string_lossy().into_owned(),
        };

        if !key.is_empty() {
            key.push('/');
        }
        key.push_str(&part);
    }
    key
}

fn io_path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn scan_for_unsafe_keyword(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_rs_files(root, &mut files)?;
    let mut out = Vec::new();

    for file in files {
        let text = fs::read_to_string(&file)?;
        // Security audit focuses on production source. Ignore cfg(test) sections so
        // test fixtures containing `unsafe` tokens do not fail release checks.
        let production = text.split("#[cfg(test)]").next().unwrap_or(&text);
        if has_unsafe_keyword(production) {
            out.push(file);
        }
    }

    out.sort();
    out.dedup();
    Ok(out)
}

fn has_unsafe_keyword(source: &str) -> bool {
    let mut token = String::new();
    let mut chars = source.chars().peekable();
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment_depth = 0u32;
    let mut pending_unsafe = false;

    let finalize_token = |token: &mut String, pending_unsafe: &mut bool| -> bool {
        if token.is_empty() {
            return false;
        }
        if *pending_unsafe {
            if matches!(token.as_str(), "fn" | "impl" | "trait" | "extern") {
                return true;
            }
            *pending_unsafe = false;
        }
        if token == "unsafe" {
            *pending_unsafe = true;
        }
        token.clear();
        false
    };

    while let Some(ch) = chars.next() {
        if line_comment {
            if ch == '\n' {
                line_comment = false;
            }
            continue;
        }

        if block_comment_depth > 0 {
            if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                block_comment_depth += 1;
                continue;
            }
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                block_comment_depth -= 1;
            }
            continue;
        }

        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        if in_char {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '\'' => in_char = false,
                _ => {}
            }
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            if finalize_token(&mut token, &mut pending_unsafe) {
                return true;
            }
            line_comment = true;
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            if finalize_token(&mut token, &mut pending_unsafe) {
                return true;
            }
            block_comment_depth = 1;
            continue;
        }
        if ch == '"' {
            if finalize_token(&mut token, &mut pending_unsafe) {
                return true;
            }
            pending_unsafe = false;
            in_string = true;
            continue;
        }
        if ch == '\'' {
            if finalize_token(&mut token, &mut pending_unsafe) {
                return true;
            }
            pending_unsafe = false;
            in_char = true;
            continue;
        }

        if ch == 'b' && chars.peek() == Some(&'r') {
            let mut probe = chars.clone();
            probe.next();
            let mut hash_count = 0usize;
            while probe.peek() == Some(&'#') {
                probe.next();
                hash_count += 1;
            }
            if probe.peek() == Some(&'"') {
                if finalize_token(&mut token, &mut pending_unsafe) {
                    return true;
                }
                pending_unsafe = false;
                chars.next();
                for _ in 0..hash_count {
                    chars.next();
                }
                chars.next();
                consume_raw_string(&mut chars, hash_count);
                continue;
            }
        }
        if ch == 'r' {
            let mut probe = chars.clone();
            let mut hash_count = 0usize;
            while probe.peek() == Some(&'#') {
                probe.next();
                hash_count += 1;
            }
            if probe.peek() == Some(&'"') {
                if finalize_token(&mut token, &mut pending_unsafe) {
                    return true;
                }
                pending_unsafe = false;
                for _ in 0..hash_count {
                    chars.next();
                }
                chars.next();
                consume_raw_string(&mut chars, hash_count);
                continue;
            }
        }

        if ch.is_ascii_alphanumeric() || ch == '_' {
            token.push(ch);
            continue;
        }

        if finalize_token(&mut token, &mut pending_unsafe) {
            return true;
        }

        if pending_unsafe {
            if ch == '{' {
                return true;
            }
            if !ch.is_whitespace() {
                pending_unsafe = false;
            }
        }
    }

    if finalize_token(&mut token, &mut pending_unsafe) {
        return true;
    }

    false
}

fn consume_raw_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, hash_count: usize) {
    while let Some(ch) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut probe = chars.clone();
        let mut matched = true;
        for _ in 0..hash_count {
            if probe.next() != Some('#') {
                matched = false;
                break;
            }
        }
        if matched {
            for _ in 0..hash_count {
                chars.next();
            }
            return;
        }
    }
}

fn collect_rs_files(root: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out)?;
            continue;
        }

        if path.extension().and_then(|x| x.to_str()) == Some("rs") {
            out.push(path);
        }
    }

    Ok(())
}

fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    // RFC 2104 HMAC-SHA256 with block size 64.
    let mut normalized_key = vec![0u8; 64];
    if key.len() > 64 {
        let mut hashed = Sha256::new();
        hashed.update(key);
        let digest = hashed.finalize();
        normalized_key[..digest.len()].copy_from_slice(&digest);
    } else {
        normalized_key[..key.len()].copy_from_slice(key);
    }

    let mut o_key_pad = vec![0u8; 64];
    let mut i_key_pad = vec![0u8; 64];
    for (idx, key_byte) in normalized_key.iter().copied().enumerate() {
        o_key_pad[idx] = key_byte ^ 0x5c;
        i_key_pad[idx] = key_byte ^ 0x36;
    }

    let mut inner = Sha256::new();
    inner.update(&i_key_pad);
    inner.update(data);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(&o_key_pad);
    outer.update(inner_digest);
    format!("{:x}", outer.finalize())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        check_compatibility_policy, check_lts_policy, compatibility_policy, generate_provenance,
        generate_repro_manifest, has_unsafe_keyword, lts_policy, read_lockfile_packages,
        verify_checksum_file, verify_provenance, write_provenance,
    };

    #[test]
    fn repro_manifest_is_deterministic_for_same_tree() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        fs::create_dir_all(root.join("src")).expect("mkdir");
        fs::create_dir_all(root.join("target-linux/tmp")).expect("mkdir target-linux");
        fs::create_dir_all(root.join(".aic-cache/generated")).expect("mkdir cache");
        fs::create_dir_all(root.join("tools/vscode-aic/.vscode-test")).expect("mkdir vscode test");
        fs::write(root.join("src/main.aic"), "fn main() -> Int { 0 }\n").expect("write");
        fs::write(root.join("README.md"), "hello\n").expect("write");
        fs::write(root.join("target-linux/tmp/stage2"), b"generated").expect("write target");
        fs::write(root.join(".aic-cache/generated/cache.bin"), b"cache").expect("write cache");
        fs::write(
            root.join("tools/vscode-aic/.vscode-test/electron"),
            b"generated",
        )
        .expect("write vscode test");

        let a = generate_repro_manifest(root, 42).expect("manifest");
        let b = generate_repro_manifest(root, 42).expect("manifest");
        assert_eq!(a, b);
        assert_eq!(a.file_count, 2);
        assert!(a.files.iter().any(|file| file.path == "README.md"));
        assert!(a.files.iter().any(|file| file.path == "src/main.aic"));
        assert!(!a
            .files
            .iter()
            .any(|file| file.path.contains("target-linux")));
        assert!(!a.files.iter().any(|file| file.path.contains(".aic-cache")));
        assert!(!a
            .files
            .iter()
            .any(|file| file.path.contains(".vscode-test")));
    }

    #[test]
    fn lockfile_parser_extracts_packages() {
        let lock = r#"
[[package]]
name = "anyhow"
version = "1.0.0"
source = "registry+https://example.invalid"
checksum = "abc"

[[package]]
name = "serde"
version = "1.0.0"
"#;

        let dir = tempdir().expect("tempdir");
        let lock_path = dir.path().join("Cargo.lock");
        fs::write(&lock_path, lock).expect("write lock");

        let pkgs = read_lockfile_packages(&lock_path).expect("parse lock");
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, "anyhow");
        assert_eq!(pkgs[1].name, "serde");
    }

    #[test]
    fn provenance_sign_and_verify_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        let artifact = root.join("artifact.bin");
        let sbom = root.join("sbom.json");
        fs::write(&artifact, b"artifact-bytes").expect("artifact");
        fs::write(&sbom, b"{\"format\":\"x\"}").expect("sbom");

        let statement = generate_provenance(&artifact, &sbom, None, "secret", Some("k1".into()))
            .expect("provenance");
        let path = root.join("provenance.json");
        write_provenance(&path, &statement).expect("write provenance");

        let errors = verify_provenance(&statement, "secret").expect("verify");
        assert!(errors.is_empty(), "errors={errors:#?}");

        fs::write(&artifact, b"tampered").expect("tamper artifact");
        let errors = verify_provenance(&statement, "secret").expect("verify");
        assert!(!errors.is_empty());
    }

    #[test]
    fn compatibility_policy_references_required_assets() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        fs::create_dir_all(root.join("docs")).expect("docs");
        fs::create_dir_all(root.join("docs/security-ops")).expect("security ops docs");
        fs::create_dir_all(root.join(".github/workflows")).expect("workflows");
        fs::write(root.join("docs/spec.md"), "# spec\n").expect("spec");
        fs::write(
            root.join("docs/compatibility-migration-policy.md"),
            "# compat\n",
        )
        .expect("compat");
        fs::write(root.join("docs/security-threat-model.md"), "# threat\n").expect("threat");
        fs::write(root.join("docs/release-security-ops.md"), "# ops\n").expect("ops");
        fs::write(root.join("docs/security-ops/migration.md"), "# migrate\n").expect("migrate");
        fs::create_dir_all(root.join("docs/release")).expect("release docs");
        fs::write(root.join("docs/release/matrix.md"), "# matrix\n").expect("matrix");
        fs::write(root.join("docs/release/lts-policy.md"), "# lts\n").expect("lts");
        fs::write(
            root.join("docs/release/compatibility-matrix.json"),
            r#"{"schema_version":1,"branches":[{"name":"main"},{"name":"release/0.1"}]}"#,
        )
        .expect("compat matrix");
        fs::write(root.join(".github/workflows/ci.yml"), "name: CI\n").expect("ci");
        fs::write(
            root.join(".github/workflows/release.yml"),
            "name: Release\n",
        )
        .expect("release");
        fs::write(
            root.join(".github/workflows/security.yml"),
            "name: Security\n",
        )
        .expect("security");

        let policy = compatibility_policy();
        let problems = check_compatibility_policy(root, &policy);
        assert!(problems.is_empty(), "problems={problems:#?}");
    }

    #[test]
    fn lts_policy_references_required_assets_and_gates() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        fs::create_dir_all(root.join("docs/release")).expect("release docs");
        fs::create_dir_all(root.join(".github/workflows")).expect("workflows");
        fs::write(root.join("docs/release/lts-policy.md"), "# lts\n").expect("lts");
        fs::write(
            root.join("docs/release/compatibility-matrix.json"),
            r#"{
  "schema_version": 1,
  "branches": [
    { "name": "main", "channel": "active" },
    { "name": "release/0.1", "channel": "lts" }
  ]
}"#,
        )
        .expect("compat matrix");

        let gate = "cargo run --quiet --bin aic -- release lts --check";
        fs::write(
            root.join(".github/workflows/ci.yml"),
            format!("steps:\n  - run: {gate}\n"),
        )
        .expect("ci");
        fs::write(
            root.join(".github/workflows/release.yml"),
            format!("steps:\n  - run: {gate}\n"),
        )
        .expect("release");
        fs::write(
            root.join(".github/workflows/security.yml"),
            format!("schedule:\n  - cron: \"0 6 * * 1\"\nsteps:\n  - run: {gate}\n"),
        )
        .expect("security");

        let policy = lts_policy();
        let problems = check_lts_policy(root, &policy);
        assert!(problems.is_empty(), "problems={problems:#?}");
    }

    #[test]
    fn checksum_verification_detects_tampering() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        let artifact = root.join("artifact.tar.gz");
        let checksum = root.join("artifact.sha256");
        fs::write(&artifact, b"artifact-bytes").expect("artifact");
        let digest = super::sha256_hex(b"artifact-bytes");
        fs::write(&checksum, format!("{digest}  artifact.tar.gz\n")).expect("checksum");

        let ok = verify_checksum_file(&artifact, &checksum).expect("verify");
        assert!(ok.is_empty(), "errors={ok:#?}");

        fs::write(&artifact, b"tampered").expect("tamper");
        let errors = verify_checksum_file(&artifact, &checksum).expect("verify");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("checksum mismatch"));
    }

    #[test]
    fn unsafe_keyword_scan_ignores_strings_and_raw_strings() {
        assert!(!has_unsafe_keyword(r#"let s = "unsafe fn f() {}";"#));
        assert!(!has_unsafe_keyword("\"unsafe\" => TokenKind::KwUnsafe,"));
        assert!(!has_unsafe_keyword("let s = r#\"unsafe fn f() {}\"#;"));
        assert!(!has_unsafe_keyword("let s = br##\"unsafe { block }\"##;"));
        assert!(!has_unsafe_keyword(
            "let src = r#\"extern \\\"C\\\" fn c_abs(x: Int) -> Int; unsafe fn wrap(x: Int) -> Int { unsafe { c_abs(x) } }\"#;"
        ));
        assert!(has_unsafe_keyword("unsafe fn real() {}"));
    }

    #[test]
    fn unsafe_keyword_scan_ignores_lexer_keyword_table() {
        let lexer = fs::read_to_string("src/lexer.rs").expect("read lexer");
        let production = lexer.split("#[cfg(test)]").next().unwrap_or(&lexer);
        assert!(!has_unsafe_keyword(production));
    }
}
