use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cli_contract::CLI_CONTRACT_VERSION;
use crate::ir::CURRENT_IR_SCHEMA_VERSION;

pub const REPRO_MANIFEST_VERSION: u32 = 1;
pub const SBOM_FORMAT: &str = "aicore-sbom-v1";
pub const PROVENANCE_FORMAT: &str = "aicore-provenance-v1";
pub const COMPATIBILITY_POLICY_VERSION: &str = "1.0";

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

pub fn compatibility_policy() -> CompatibilityPolicy {
    CompatibilityPolicy {
        version: COMPATIBILITY_POLICY_VERSION.to_string(),
        ir_schema_version: CURRENT_IR_SCHEMA_VERSION,
        cli_contract_version: CLI_CONTRACT_VERSION.to_string(),
        migration_commands: vec![
            "aic ir-migrate <legacy-ir.json>".to_string(),
            "aic std-compat --check --baseline docs/std-api-baseline.json".to_string(),
            "aic release policy --check".to_string(),
        ],
        required_docs: vec![
            "docs/spec.md".to_string(),
            "docs/compatibility-migration-policy.md".to_string(),
            "docs/security-threat-model.md".to_string(),
            "docs/release-security-ops.md".to_string(),
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
    let skip_names = [".git", "target", "dist", "node_modules", ".idea", ".vscode"];
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
        if has_unsafe_keyword(&text) {
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
            if token == "unsafe" {
                return true;
            }
            token.clear();
            line_comment = true;
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            if token == "unsafe" {
                return true;
            }
            token.clear();
            block_comment_depth = 1;
            continue;
        }
        if ch == '"' {
            if token == "unsafe" {
                return true;
            }
            token.clear();
            in_string = true;
            continue;
        }
        if ch == '\'' {
            if token == "unsafe" {
                return true;
            }
            token.clear();
            in_char = true;
            continue;
        }

        if ch.is_ascii_alphanumeric() || ch == '_' {
            token.push(ch);
        } else {
            if token == "unsafe" {
                return true;
            }
            token.clear();
        }
    }

    token == "unsafe"
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
        check_compatibility_policy, compatibility_policy, generate_provenance,
        generate_repro_manifest, read_lockfile_packages, verify_provenance, write_provenance,
    };

    #[test]
    fn repro_manifest_is_deterministic_for_same_tree() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        fs::create_dir_all(root.join("src")).expect("mkdir");
        fs::write(root.join("src/main.aic"), "fn main() -> Int { 0 }\n").expect("write");
        fs::write(root.join("README.md"), "hello\n").expect("write");

        let a = generate_repro_manifest(root, 42).expect("manifest");
        let b = generate_repro_manifest(root, 42).expect("manifest");
        assert_eq!(a, b);
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
        fs::create_dir_all(root.join(".github/workflows")).expect("workflows");
        fs::write(root.join("docs/spec.md"), "# spec\n").expect("spec");
        fs::write(
            root.join("docs/compatibility-migration-policy.md"),
            "# compat\n",
        )
        .expect("compat");
        fs::write(root.join("docs/security-threat-model.md"), "# threat\n").expect("threat");
        fs::write(root.join("docs/release-security-ops.md"), "# ops\n").expect("ops");
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
}
