use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, bail, Context};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cli_contract::CLI_CONTRACT_VERSION;
use crate::semantic_diff::{self, SemanticDiffReport};

const CHECKPOINT_SCHEMA_VERSION: u32 = 1;
const CHECKPOINT_PHASE: &str = "checkpoint";
const CHECKPOINTS_DIR_NAME: &str = ".aic-checkpoints";
const CHECKPOINT_FILES_DIR_NAME: &str = "files";
const CHECKPOINT_METADATA_NAME: &str = "metadata.json";

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CheckpointSummary {
    pub id: String,
    pub file_count: usize,
    pub total_bytes: usize,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CheckpointCreateResponse {
    pub protocol_version: String,
    pub phase: String,
    pub command: String,
    pub checkpoint: CheckpointSummary,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CheckpointListResponse {
    pub protocol_version: String,
    pub phase: String,
    pub command: String,
    pub checkpoints: Vec<CheckpointSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CheckpointRestoreResponse {
    pub protocol_version: String,
    pub phase: String,
    pub command: String,
    pub checkpoint: CheckpointSummary,
    pub restored_files: usize,
    pub restored_paths: Vec<String>,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct CheckpointDiffSummary {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub unchanged: usize,
    pub semantic_breaking: usize,
    pub semantic_non_breaking: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CheckpointFileDiff {
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic: Option<SemanticDiffReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CheckpointDiffResponse {
    pub protocol_version: String,
    pub phase: String,
    pub command: String,
    pub from: String,
    pub to: String,
    pub summary: CheckpointDiffSummary,
    pub files: Vec<CheckpointFileDiff>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct CheckpointMetadata {
    #[serde(default = "default_checkpoint_schema_version")]
    schema_version: u32,
    id: String,
    file_count: usize,
    total_bytes: usize,
    digest: String,
    files: Vec<CheckpointFileEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct CheckpointFileEntry {
    path: String,
    sha256: String,
    bytes: usize,
    snapshot_path: String,
}

#[derive(Debug, Clone)]
struct LoadedCheckpoint {
    metadata: CheckpointMetadata,
    files: BTreeMap<String, FileVersion>,
}

#[derive(Debug, Clone)]
struct FileVersion {
    sha256: String,
    actual_path: PathBuf,
}

#[derive(Debug)]
struct RestorePayload {
    rel_path: String,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct RestoreAction {
    rel_path: String,
    target_path: PathBuf,
    temp_path: PathBuf,
    backup_path: Option<PathBuf>,
}

pub fn create_checkpoint(project_root: &Path) -> anyhow::Result<CheckpointCreateResponse> {
    let files = collect_checkpointable_paths(project_root)?;
    if files.is_empty() {
        bail!(
            "no checkpointable files found under {}",
            project_root.display()
        );
    }

    let checkpoints_root = checkpoints_root(project_root);
    fs::create_dir_all(&checkpoints_root)
        .with_context(|| format!("failed to create {}", checkpoints_root.display()))?;

    let id = next_checkpoint_id(project_root)?;
    let staging_dir = checkpoints_root.join(format!(".tmp-create-{id}-{}", temp_suffix()));
    let final_dir = checkpoint_dir(project_root, &id);
    if final_dir.exists() {
        bail!("checkpoint `{id}` already exists");
    }
    fs::create_dir_all(staging_dir.join(CHECKPOINT_FILES_DIR_NAME)).with_context(|| {
        format!(
            "failed to initialize checkpoint staging directory {}",
            staging_dir.display()
        )
    })?;

    let mut entries = Vec::new();
    let mut total_bytes = 0usize;
    for rel_path in files {
        let source_path = project_root.join(&rel_path);
        let bytes = fs::read(&source_path)
            .with_context(|| format!("failed to read {}", source_path.display()))?;
        let sha256 = sha256_hex(&bytes);
        total_bytes += bytes.len();

        let snapshot_rel = format!("{CHECKPOINT_FILES_DIR_NAME}/{rel_path}");
        let snapshot_path = staging_dir.join(&snapshot_rel);
        if let Some(parent) = snapshot_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&snapshot_path, &bytes)
            .with_context(|| format!("failed to write {}", snapshot_path.display()))?;

        entries.push(CheckpointFileEntry {
            path: rel_path,
            sha256,
            bytes: bytes.len(),
            snapshot_path: snapshot_rel,
        });
    }

    entries.sort_by(|lhs, rhs| lhs.path.cmp(&rhs.path));
    let metadata = CheckpointMetadata {
        schema_version: CHECKPOINT_SCHEMA_VERSION,
        id: id.clone(),
        file_count: entries.len(),
        total_bytes,
        digest: checkpoint_digest(&entries),
        files: entries,
    };
    write_json(&staging_dir.join(CHECKPOINT_METADATA_NAME), &metadata)?;
    fs::rename(&staging_dir, &final_dir).with_context(|| {
        format!(
            "failed to move checkpoint staging directory {} into {}",
            staging_dir.display(),
            final_dir.display()
        )
    })?;

    Ok(CheckpointCreateResponse {
        protocol_version: CLI_CONTRACT_VERSION.to_string(),
        phase: CHECKPOINT_PHASE.to_string(),
        command: "create".to_string(),
        checkpoint: checkpoint_summary(&metadata),
    })
}

pub fn list_checkpoints(project_root: &Path) -> anyhow::Result<CheckpointListResponse> {
    let checkpoints = load_all_metadata(project_root)?
        .into_iter()
        .map(|metadata| checkpoint_summary(&metadata))
        .collect();

    Ok(CheckpointListResponse {
        protocol_version: CLI_CONTRACT_VERSION.to_string(),
        phase: CHECKPOINT_PHASE.to_string(),
        command: "list".to_string(),
        checkpoints,
    })
}

pub fn restore_checkpoint(
    project_root: &Path,
    id: &str,
) -> anyhow::Result<CheckpointRestoreResponse> {
    let checkpoint = load_checkpoint(project_root, id)?;
    let payloads = checkpoint
        .metadata
        .files
        .iter()
        .map(|entry| {
            let snapshot_path = checkpoint_dir(project_root, id).join(&entry.snapshot_path);
            let bytes = fs::read(&snapshot_path).with_context(|| {
                format!(
                    "checkpoint `{id}` snapshot missing file `{}` at {}",
                    entry.path,
                    snapshot_path.display()
                )
            })?;
            let actual_sha256 = sha256_hex(&bytes);
            if actual_sha256 != entry.sha256 {
                bail!(
                    "checkpoint `{id}` snapshot hash mismatch for {}: expected {} got {}",
                    entry.path,
                    entry.sha256,
                    actual_sha256
                );
            }
            if bytes.len() != entry.bytes {
                bail!(
                    "checkpoint `{id}` snapshot size mismatch for {}: expected {} got {}",
                    entry.path,
                    entry.bytes,
                    bytes.len()
                );
            }
            Ok(RestorePayload {
                rel_path: entry.path.clone(),
                bytes,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let backup_root =
        checkpoints_root(project_root).join(format!(".tmp-restore-backup-{id}-{}", temp_suffix()));
    let mut actions = Vec::with_capacity(payloads.len());

    for payload in &payloads {
        let rel_path = safe_relative_path(&payload.rel_path)?;
        let target_path = project_root.join(&rel_path);
        let target_parent = target_path
            .parent()
            .ok_or_else(|| anyhow!("failed to determine parent for {}", target_path.display()))?;
        fs::create_dir_all(target_parent)
            .with_context(|| format!("failed to create {}", target_parent.display()))?;
        let temp_path = temp_sibling_path(&target_path);
        fs::write(&temp_path, &payload.bytes)
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        let backup_path = if target_path.exists() {
            let backup_path = backup_root.join(&rel_path);
            if let Some(parent) = backup_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            Some(backup_path)
        } else {
            None
        };
        actions.push(RestoreAction {
            rel_path: payload.rel_path.clone(),
            target_path,
            temp_path,
            backup_path,
        });
    }

    for action in &actions {
        if let Some(backup_path) = &action.backup_path {
            if let Err(err) = fs::rename(&action.target_path, backup_path) {
                rollback_restore(&actions);
                return Err(err).with_context(|| {
                    format!(
                        "failed to stage existing file {} for restore",
                        action.target_path.display()
                    )
                });
            }
        }
    }

    let mut committed = 0usize;
    for action in &actions {
        if let Err(err) = fs::rename(&action.temp_path, &action.target_path) {
            rollback_restore(&actions);
            return Err(err).with_context(|| {
                format!(
                    "failed to commit restored file {}",
                    action.target_path.display()
                )
            });
        }
        committed += 1;
    }

    debug_assert_eq!(committed, actions.len());
    let _ = fs::remove_dir_all(&backup_root);

    Ok(CheckpointRestoreResponse {
        protocol_version: CLI_CONTRACT_VERSION.to_string(),
        phase: CHECKPOINT_PHASE.to_string(),
        command: "restore".to_string(),
        checkpoint: checkpoint_summary(&checkpoint.metadata),
        restored_files: actions.len(),
        restored_paths: actions.into_iter().map(|action| action.rel_path).collect(),
        verified: true,
    })
}

pub fn diff_checkpoint(
    project_root: &Path,
    from_id: &str,
    to_id: Option<&str>,
) -> anyhow::Result<CheckpointDiffResponse> {
    let from = load_checkpoint(project_root, from_id)?;
    let (to_label, to_files) = if let Some(other_id) = to_id {
        let checkpoint = load_checkpoint(project_root, other_id)?;
        (format!("checkpoint:{other_id}"), checkpoint.files)
    } else {
        ("workspace".to_string(), load_workspace_files(project_root)?)
    };

    let mut paths = BTreeSet::new();
    paths.extend(from.files.keys().cloned());
    paths.extend(to_files.keys().cloned());

    let mut summary = CheckpointDiffSummary::default();
    let mut files = Vec::new();

    for path in paths {
        let old = from.files.get(&path);
        let new = to_files.get(&path);
        let mut semantic = None;
        let mut semantic_error = None;

        let status = match (old, new) {
            (None, Some(_)) => {
                summary.added += 1;
                "added"
            }
            (Some(_), None) => {
                summary.removed += 1;
                "removed"
            }
            (Some(old_file), Some(new_file)) if old_file.sha256 == new_file.sha256 => {
                summary.unchanged += 1;
                "unchanged"
            }
            (Some(old_file), Some(new_file)) => {
                summary.modified += 1;
                if path.ends_with(".aic") {
                    match semantic_diff::diff_files(&old_file.actual_path, &new_file.actual_path) {
                        Ok(report) => {
                            summary.semantic_breaking += report.summary.breaking;
                            summary.semantic_non_breaking += report.summary.non_breaking;
                            semantic = Some(report);
                        }
                        Err(err) => {
                            semantic_error = Some(err.to_string());
                        }
                    }
                }
                "modified"
            }
            (None, None) => unreachable!("path union produced empty entry"),
        };

        files.push(CheckpointFileDiff {
            path,
            status: status.to_string(),
            old_sha256: old.map(|file| file.sha256.clone()),
            new_sha256: new.map(|file| file.sha256.clone()),
            semantic,
            semantic_error,
        });
    }

    Ok(CheckpointDiffResponse {
        protocol_version: CLI_CONTRACT_VERSION.to_string(),
        phase: CHECKPOINT_PHASE.to_string(),
        command: "diff".to_string(),
        from: format!("checkpoint:{from_id}"),
        to: to_label,
        summary,
        files,
    })
}

pub fn format_create_text(response: &CheckpointCreateResponse) -> String {
    format!(
        "checkpoint created {} ({} files, {} bytes, digest {})",
        response.checkpoint.id,
        response.checkpoint.file_count,
        response.checkpoint.total_bytes,
        response.checkpoint.digest
    )
}

pub fn format_list_text(response: &CheckpointListResponse) -> String {
    let mut lines = Vec::new();
    if response.checkpoints.is_empty() {
        lines.push("checkpoints: none".to_string());
        return lines.join("\n");
    }

    lines.push(format!("checkpoints ({}):", response.checkpoints.len()));
    for checkpoint in &response.checkpoints {
        lines.push(format!(
            "  {} -> {} files, {} bytes, digest {}",
            checkpoint.id, checkpoint.file_count, checkpoint.total_bytes, checkpoint.digest
        ));
    }
    lines.join("\n")
}

pub fn format_restore_text(response: &CheckpointRestoreResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "checkpoint restored {} ({} files)",
        response.checkpoint.id, response.restored_files
    ));
    for path in &response.restored_paths {
        lines.push(format!("  restored {path}"));
    }
    lines.join("\n")
}

pub fn format_diff_text(response: &CheckpointDiffResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "checkpoint diff {} -> {}",
        response.from, response.to
    ));
    lines.push(format!(
        "summary: added {} removed {} modified {} unchanged {} semantic-breaking {} semantic-non-breaking {}",
        response.summary.added,
        response.summary.removed,
        response.summary.modified,
        response.summary.unchanged,
        response.summary.semantic_breaking,
        response.summary.semantic_non_breaking
    ));
    if response.files.is_empty() {
        lines.push("files: none".to_string());
        return lines.join("\n");
    }
    lines.push(format!("files ({}):", response.files.len()));
    for file in &response.files {
        let mut line = format!("  [{}] {}", file.status, file.path);
        if let Some(semantic) = &file.semantic {
            line.push_str(&format!(
                " (semantic breaking={}, non-breaking={})",
                semantic.summary.breaking, semantic.summary.non_breaking
            ));
        } else if let Some(error) = &file.semantic_error {
            line.push_str(&format!(" (semantic unavailable: {error})"));
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn load_all_metadata(project_root: &Path) -> anyhow::Result<Vec<CheckpointMetadata>> {
    let root = checkpoints_root(project_root);
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(&root)
        .with_context(|| format!("failed to read {}", root.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    let mut checkpoints = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        checkpoints.push(read_checkpoint_metadata(
            &path.join(CHECKPOINT_METADATA_NAME),
        )?);
    }
    checkpoints.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    Ok(checkpoints)
}

fn load_checkpoint(project_root: &Path, id: &str) -> anyhow::Result<LoadedCheckpoint> {
    let dir = checkpoint_dir(project_root, id);
    if !dir.exists() {
        bail!("unknown checkpoint `{id}`");
    }

    let metadata = read_checkpoint_metadata(&dir.join(CHECKPOINT_METADATA_NAME))?;
    if metadata.id != id {
        bail!(
            "checkpoint directory `{}` does not match metadata id `{}`",
            id,
            metadata.id
        );
    }

    let mut files = BTreeMap::new();
    for entry in &metadata.files {
        let rel_path = safe_relative_path(&entry.path)?;
        let snapshot_rel = safe_relative_path(&entry.snapshot_path)?;
        if !snapshot_rel.starts_with(CHECKPOINT_FILES_DIR_NAME) {
            bail!(
                "checkpoint `{id}` snapshot path for {} must live under {}",
                entry.path,
                CHECKPOINT_FILES_DIR_NAME
            );
        }
        let snapshot_path = dir.join(&snapshot_rel);
        let bytes = fs::read(&snapshot_path).with_context(|| {
            format!(
                "checkpoint `{id}` snapshot missing file `{}` at {}",
                entry.path,
                snapshot_path.display()
            )
        })?;
        let actual_sha256 = sha256_hex(&bytes);
        if actual_sha256 != entry.sha256 {
            bail!(
                "checkpoint `{id}` snapshot hash mismatch for {}: expected {} got {}",
                entry.path,
                entry.sha256,
                actual_sha256
            );
        }
        if bytes.len() != entry.bytes {
            bail!(
                "checkpoint `{id}` snapshot size mismatch for {}: expected {} got {}",
                entry.path,
                entry.bytes,
                bytes.len()
            );
        }
        files.insert(
            stable_path_key(&rel_path),
            FileVersion {
                sha256: entry.sha256.clone(),
                actual_path: snapshot_path,
            },
        );
    }

    Ok(LoadedCheckpoint { metadata, files })
}

fn load_workspace_files(project_root: &Path) -> anyhow::Result<BTreeMap<String, FileVersion>> {
    let mut files = BTreeMap::new();
    for rel_path in collect_checkpointable_paths(project_root)? {
        let actual_path = project_root.join(&rel_path);
        let bytes = fs::read(&actual_path)
            .with_context(|| format!("failed to read {}", actual_path.display()))?;
        let rel_path_buf = safe_relative_path(&rel_path)?;
        files.insert(
            rel_path,
            FileVersion {
                sha256: sha256_hex(&bytes),
                actual_path: project_root.join(rel_path_buf),
            },
        );
    }
    Ok(files)
}

fn read_checkpoint_metadata(path: &Path) -> anyhow::Result<CheckpointMetadata> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read checkpoint metadata {}", path.display()))?;
    let mut metadata: CheckpointMetadata = serde_json::from_str(&raw)
        .map_err(|err| anyhow!("invalid checkpoint metadata '{}': {}", path.display(), err))?;

    match metadata.schema_version {
        CHECKPOINT_SCHEMA_VERSION => {}
        other => {
            bail!(
                "unsupported checkpoint schema version {} in {}",
                other,
                path.display()
            )
        }
    }

    if metadata.id.trim().is_empty() {
        bail!(
            "invalid checkpoint metadata '{}': missing id",
            path.display()
        );
    }

    metadata.files.sort_by(|lhs, rhs| lhs.path.cmp(&rhs.path));
    let mut previous = None::<String>;
    for entry in &metadata.files {
        validate_file_entry(entry, path)?;
        if previous.as_deref() == Some(entry.path.as_str()) {
            bail!(
                "invalid checkpoint metadata '{}': duplicate file entry {}",
                path.display(),
                entry.path
            );
        }
        previous = Some(entry.path.clone());
    }

    if metadata.file_count != metadata.files.len() {
        bail!(
            "checkpoint metadata {} file_count mismatch: expected {} got {}",
            path.display(),
            metadata.file_count,
            metadata.files.len()
        );
    }
    let actual_total_bytes = metadata
        .files
        .iter()
        .map(|entry| entry.bytes)
        .sum::<usize>();
    if metadata.total_bytes != actual_total_bytes {
        bail!(
            "checkpoint metadata {} total_bytes mismatch: expected {} got {}",
            path.display(),
            metadata.total_bytes,
            actual_total_bytes
        );
    }
    let actual_digest = checkpoint_digest(&metadata.files);
    if metadata.digest != actual_digest {
        bail!(
            "checkpoint metadata {} digest mismatch: expected {} got {}",
            path.display(),
            metadata.digest,
            actual_digest
        );
    }

    Ok(metadata)
}

fn validate_file_entry(entry: &CheckpointFileEntry, metadata_path: &Path) -> anyhow::Result<()> {
    if entry.path.trim().is_empty() {
        bail!(
            "invalid checkpoint metadata '{}': file path is empty",
            metadata_path.display()
        );
    }
    let _ = safe_relative_path(&entry.path)?;
    let snapshot_rel = safe_relative_path(&entry.snapshot_path)?;
    if !snapshot_rel.starts_with(CHECKPOINT_FILES_DIR_NAME) {
        bail!(
            "invalid checkpoint metadata '{}': snapshot path {} must live under {}",
            metadata_path.display(),
            entry.snapshot_path,
            CHECKPOINT_FILES_DIR_NAME
        );
    }
    if !is_valid_sha256_hex(&entry.sha256) {
        bail!(
            "invalid checkpoint metadata '{}': sha256 for {} is not valid hex",
            metadata_path.display(),
            entry.path
        );
    }
    Ok(())
}

fn collect_checkpointable_paths(project_root: &Path) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::new();
    collect_checkpointable_paths_from(project_root, project_root, &mut out)?;
    out.sort();
    out.dedup();
    Ok(out)
}

fn collect_checkpointable_paths_from(
    root: &Path,
    dir: &Path,
    out: &mut Vec<String>,
) -> anyhow::Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();

        if path.is_dir() {
            if should_skip_dir(name) {
                continue;
            }
            collect_checkpointable_paths_from(root, &path, out)?;
            continue;
        }

        if should_capture_file(&path) {
            let rel = path.strip_prefix(root).unwrap_or(path.as_path());
            out.push(stable_path_key(rel));
        }
    }
    Ok(())
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | ".aic-cache" | CHECKPOINTS_DIR_NAME
    )
}

fn should_capture_file(path: &Path) -> bool {
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    name == "aic.toml"
        || name == "aic.lock"
        || path.extension().and_then(OsStr::to_str) == Some("aic")
}

fn next_checkpoint_id(project_root: &Path) -> anyhow::Result<String> {
    let root = checkpoints_root(project_root);
    if !root.exists() {
        return Ok("ckpt-0001".to_string());
    }

    let mut next = 1u32;
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        let Some(raw) = name.strip_prefix("ckpt-") else {
            continue;
        };
        if let Ok(value) = raw.parse::<u32>() {
            next = next.max(value.saturating_add(1));
        }
    }
    Ok(format!("ckpt-{next:04}"))
}

fn checkpoint_summary(metadata: &CheckpointMetadata) -> CheckpointSummary {
    CheckpointSummary {
        id: metadata.id.clone(),
        file_count: metadata.file_count,
        total_bytes: metadata.total_bytes,
        digest: metadata.digest.clone(),
    }
}

fn checkpoints_root(project_root: &Path) -> PathBuf {
    project_root.join(CHECKPOINTS_DIR_NAME)
}

fn checkpoint_dir(project_root: &Path, id: &str) -> PathBuf {
    checkpoints_root(project_root).join(id)
}

fn checkpoint_digest(entries: &[CheckpointFileEntry]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.path.as_bytes());
        hasher.update([0]);
        hasher.update(entry.sha256.as_bytes());
        hasher.update([0]);
        hasher.update(entry.bytes.to_string().as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(value)?;
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_relative_path(raw: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        bail!("checkpoint paths must be relative: {raw}");
    }
    for component in path.components() {
        match component {
            Component::CurDir | Component::Normal(_) => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("checkpoint paths must stay within the project root: {raw}")
            }
        }
    }
    Ok(path)
}

fn stable_path_key(path: &Path) -> String {
    let mut out = String::new();
    for component in path.components() {
        let piece = match component {
            Component::RootDir => continue,
            Component::Prefix(prefix) => prefix.as_os_str().to_string_lossy().into_owned(),
            Component::CurDir => ".".to_string(),
            Component::ParentDir => "..".to_string(),
            Component::Normal(segment) => segment.to_string_lossy().into_owned(),
        };
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&piece);
    }
    out
}

fn temp_sibling_path(target_path: &Path) -> PathBuf {
    let file_name = target_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("checkpoint");
    let suffix = temp_suffix();
    target_path.with_file_name(format!(".{file_name}.aic-checkpoint-{suffix}.tmp"))
}

fn temp_suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn rollback_restore(actions: &[RestoreAction]) {
    for action in actions.iter().rev() {
        if action.target_path.exists() {
            let _ = fs::remove_file(&action.target_path);
        }
        if action.temp_path.exists() {
            let _ = fs::remove_file(&action.temp_path);
        }
        if let Some(backup_path) = &action.backup_path {
            if backup_path.exists() {
                if let Some(parent) = action.target_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::rename(backup_path, &action.target_path);
            }
        }
    }
}

fn default_checkpoint_schema_version() -> u32 {
    CHECKPOINT_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::{
        collect_checkpointable_paths, create_checkpoint, diff_checkpoint, read_checkpoint_metadata,
        restore_checkpoint,
    };
    use std::fs;

    use serde_json::Value;
    use tempfile::tempdir;

    #[test]
    fn checkpoint_file_collection_skips_internal_state_and_sorts_paths() {
        let project = tempdir().expect("tempdir");
        fs::create_dir_all(project.path().join("src/nested")).expect("mkdir src/nested");
        fs::create_dir_all(project.path().join(".aic-cache/tmp")).expect("mkdir cache");
        fs::create_dir_all(project.path().join(".aic-checkpoints/ckpt-0001")).expect("mkdir cp");
        fs::write(
            project.path().join("aic.toml"),
            "[package]\nname=\"demo\"\n",
        )
        .expect("write aic.toml");
        fs::write(
            project.path().join("src/main.aic"),
            "fn main() -> Int { 0 }\n",
        )
        .expect("write src/main.aic");
        fs::write(
            project.path().join("src/nested/lib.aic"),
            "fn lib() -> Int { 1 }\n",
        )
        .expect("write src/nested/lib.aic");
        fs::write(project.path().join(".aic-cache/tmp/ignored.aic"), "bad")
            .expect("write ignored file");

        let files = collect_checkpointable_paths(project.path()).expect("collect paths");
        assert_eq!(
            files,
            vec!["aic.toml", "src/main.aic", "src/nested/lib.aic"]
        );
    }

    #[test]
    fn checkpoint_round_trip_diff_and_restore_work() {
        let project = tempdir().expect("tempdir");
        fs::create_dir_all(project.path().join("src")).expect("mkdir src");
        fs::write(
            project.path().join("aic.toml"),
            "[package]\nname = \"checkpoint_demo\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write aic.toml");
        fs::write(
            project.path().join("aic.lock"),
            "{\n  \"schema_version\": 2\n}\n",
        )
        .expect("write aic.lock");
        let initial = concat!(
            "module demo.checkpoint;\n",
            "fn main() -> Int {\n",
            "    0\n",
            "}\n",
        );
        fs::write(project.path().join("src/main.aic"), initial).expect("write initial source");

        let created = create_checkpoint(project.path()).expect("create checkpoint");
        assert_eq!(created.checkpoint.id, "ckpt-0001");

        fs::write(
            project.path().join("src/main.aic"),
            concat!(
                "module demo.checkpoint;\n",
                "fn helper() -> Int {\n",
                "    1\n",
                "}\n",
                "fn main() -> Int {\n",
                "    helper()\n",
                "}\n",
            ),
        )
        .expect("rewrite source");
        fs::write(
            project.path().join("aic.lock"),
            "{\n  \"schema_version\": 3\n}\n",
        )
        .expect("rewrite lockfile");

        let diff = diff_checkpoint(project.path(), "ckpt-0001", None).expect("diff checkpoint");
        assert_eq!(diff.summary.modified, 2);
        assert_eq!(diff.summary.semantic_non_breaking, 1);
        let payload = serde_json::to_value(&diff).expect("serialize diff");
        assert_eq!(payload["phase"], Value::String("checkpoint".to_string()));

        let restored = restore_checkpoint(project.path(), "ckpt-0001").expect("restore");
        assert_eq!(restored.restored_files, 3);
        assert_eq!(
            fs::read_to_string(project.path().join("src/main.aic")).expect("read restored source"),
            initial
        );
        assert_eq!(
            fs::read_to_string(project.path().join("aic.lock")).expect("read restored lockfile"),
            "{\n  \"schema_version\": 2\n}\n"
        );
    }

    #[test]
    fn checkpoint_metadata_reader_rejects_tampered_digest() {
        let project = tempdir().expect("tempdir");
        fs::create_dir_all(project.path().join("src")).expect("mkdir src");
        fs::write(
            project.path().join("aic.toml"),
            "[package]\nname = \"checkpoint_demo\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write aic.toml");
        fs::write(
            project.path().join("src/main.aic"),
            "module demo;\nfn main() -> Int { 0 }\n",
        )
        .expect("write source");

        let created = create_checkpoint(project.path()).expect("create checkpoint");
        let metadata_path = project
            .path()
            .join(".aic-checkpoints")
            .join(&created.checkpoint.id)
            .join("metadata.json");
        let mut metadata: Value =
            serde_json::from_str(&fs::read_to_string(&metadata_path).expect("read metadata"))
                .expect("parse metadata");
        metadata["digest"] = Value::String("deadbeef".repeat(8));
        fs::write(
            &metadata_path,
            serde_json::to_string_pretty(&metadata).expect("serialize metadata"),
        )
        .expect("rewrite metadata");

        let err =
            read_checkpoint_metadata(&metadata_path).expect_err("tampered metadata must fail");
        assert!(
            err.to_string().contains("digest mismatch"),
            "unexpected error: {err:#}"
        );
    }
}
