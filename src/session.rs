use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::cli_contract::CLI_CONTRACT_VERSION;
use crate::diagnostics::Diagnostic;
use crate::driver::{has_errors, run_frontend_with_options, FrontendOptions};
use crate::machine_paths;
use crate::package_workflow::read_manifest;
use crate::patch_protocol::{self, PatchDocument, PatchOperation};
use crate::symbol_query::{self, SymbolKind, SymbolRecord};
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

const SESSION_SCHEMA_VERSION: u32 = 1;
const SESSION_PHASE: &str = "session";
const SESSIONS_DIR_NAME: &str = ".aic-sessions";
const STATE_FILE_NAME: &str = "state.json";
const STATE_LOCK_NAME: &str = ".state.lock";
const STATE_LOCK_SCHEMA_VERSION: u32 = 1;
const LOCK_WAIT_RETRIES: usize = 200;
const LOCK_WAIT_INTERVAL_MS: u64 = 5;
const DEFAULT_STALE_LOCK_TTL_MS: u64 = 120_000;
const DEFAULT_LEASE_MS: u64 = 30_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub created_ms: u64,
    pub active_locks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSymbol {
    pub key: String,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_end: Option<usize>,
    #[serde(default)]
    pub synthetic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionLockView {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    pub acquired_ms: u64,
    pub expires_ms: u64,
    #[serde(default)]
    pub expired: bool,
    pub target: SessionSymbol,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionCreateResponse {
    pub protocol_version: String,
    pub phase: String,
    pub command: String,
    pub session: SessionSummary,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionListResponse {
    pub protocol_version: String,
    pub phase: String,
    pub command: String,
    pub sessions: Vec<SessionSummary>,
    pub locks: Vec<SessionLockView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionLockResponse {
    pub protocol_version: String,
    pub phase: String,
    pub command: String,
    pub action: String,
    pub ok: bool,
    pub session_id: String,
    pub target: SessionSymbol,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock: Option<SessionLockView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denied_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reclaimed_from: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionOperationSummary {
    pub session_id: String,
    pub operation_id: String,
    pub patch: String,
    pub symbols: Vec<SessionSymbol>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionConflictEntry {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SessionSymbol>,
    pub sessions: Vec<String>,
    pub operation_ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub patches: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionConflictResponse {
    pub protocol_version: String,
    pub phase: String,
    pub command: String,
    pub plan: String,
    pub ok: bool,
    pub operations: Vec<SessionOperationSummary>,
    pub conflicts: Vec<SessionConflictEntry>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionMergeResponse {
    pub protocol_version: String,
    pub phase: String,
    pub command: String,
    pub plan: String,
    pub ok: bool,
    pub valid: bool,
    pub entry: String,
    pub merged_files: Vec<String>,
    pub operations: Vec<SessionOperationSummary>,
    pub conflicts: Vec<SessionConflictEntry>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionPlanDocument {
    operations: Vec<SessionPlanOperation>,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionPlanOperation {
    session_id: String,
    operation_id: String,
    patch: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct SessionState {
    #[serde(default = "default_session_schema_version")]
    schema_version: u32,
    #[serde(default = "default_next_session_seq")]
    next_session_seq: u64,
    sessions: Vec<StoredSession>,
    locks: Vec<StoredLock>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct StoredSession {
    id: String,
    label: Option<String>,
    created_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct StoredLock {
    session_id: String,
    operation_id: Option<String>,
    acquired_ms: u64,
    expires_ms: u64,
    target: SessionSymbol,
}

#[derive(Debug, Clone)]
struct TargetSelector {
    kind: Option<SymbolKind>,
    name: String,
    module: Option<String>,
}

#[derive(Debug, Clone)]
struct PlannedOperation {
    session_id: String,
    operation_id: String,
    patch_display: String,
    document: PatchDocument,
    symbols: Vec<SessionSymbol>,
}

#[derive(Debug)]
struct StateLockGuard {
    path: PathBuf,
}

#[derive(Debug)]
struct TempWorkspace {
    path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StateLockMetadata {
    schema_version: u32,
    pid: u32,
    host: String,
    created_ms: u64,
    process_hint: String,
}

#[derive(Debug, Clone)]
struct LockObservation {
    summary: String,
}

impl StateLockMetadata {
    fn current() -> Self {
        let pid = std::process::id();
        let host = current_host_identifier();
        let created_ms = current_time_ms();
        Self {
            schema_version: STATE_LOCK_SCHEMA_VERSION,
            pid,
            host: host.clone(),
            created_ms,
            process_hint: format!("pid={pid}@{host}"),
        }
    }
}

impl Drop for StateLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn default_lease_ms() -> u64 {
    DEFAULT_LEASE_MS
}

pub fn create_session(
    project_root: &Path,
    label: Option<&str>,
    now_ms: Option<u64>,
) -> anyhow::Result<SessionCreateResponse> {
    let project_root = machine_paths::canonical_machine_path_buf(project_root);
    let label = label.map(str::trim).filter(|value| !value.is_empty());
    let now_ms = now_ms.unwrap_or_else(current_time_ms);
    with_state_mut(&project_root, |state| {
        let id = format!("sess-{:04}", state.next_session_seq);
        state.next_session_seq += 1;
        state.sessions.push(StoredSession {
            id: id.clone(),
            label: label.map(ToString::to_string),
            created_ms: now_ms,
        });
        state.sessions.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));

        Ok(SessionCreateResponse {
            protocol_version: CLI_CONTRACT_VERSION.to_string(),
            phase: SESSION_PHASE.to_string(),
            command: "create".to_string(),
            session: SessionSummary {
                id,
                label: label.map(ToString::to_string),
                created_ms: now_ms,
                active_locks: 0,
            },
        })
    })
}

pub fn list_sessions(
    project_root: &Path,
    now_ms: Option<u64>,
) -> anyhow::Result<SessionListResponse> {
    let project_root = machine_paths::canonical_machine_path_buf(project_root);
    let state = load_state(&project_root)?;
    let now_ms = now_ms.unwrap_or_else(current_time_ms);
    let sessions = state
        .sessions
        .iter()
        .map(|session| SessionSummary {
            id: session.id.clone(),
            label: session.label.clone(),
            created_ms: session.created_ms,
            active_locks: state
                .locks
                .iter()
                .filter(|lock| lock.session_id == session.id && !lock_is_expired(lock, now_ms))
                .count(),
        })
        .collect::<Vec<_>>();
    let mut locks = state
        .locks
        .iter()
        .map(|lock| lock_view(lock, now_ms))
        .collect::<Vec<_>>();
    locks.sort_by(|lhs, rhs| {
        lhs.target
            .key
            .cmp(&rhs.target.key)
            .then(lhs.session_id.cmp(&rhs.session_id))
            .then(lhs.operation_id.cmp(&rhs.operation_id))
    });
    Ok(SessionListResponse {
        protocol_version: CLI_CONTRACT_VERSION.to_string(),
        phase: SESSION_PHASE.to_string(),
        command: "list".to_string(),
        sessions,
        locks,
    })
}

pub fn acquire_lock(
    project_root: &Path,
    session_id: &str,
    target_tokens: &[String],
    lease_ms: u64,
    operation_id: Option<&str>,
    now_ms: Option<u64>,
) -> anyhow::Result<SessionLockResponse> {
    let project_root = machine_paths::canonical_machine_path_buf(project_root);
    if lease_ms == 0 {
        bail!("--lease-ms must be greater than 0");
    }
    let session_id = session_id.trim();
    if session_id.is_empty() {
        bail!("session id must not be empty");
    }

    let target = resolve_cli_target(&project_root, target_tokens)?;
    let now_ms = now_ms.unwrap_or_else(current_time_ms);
    let operation_id = operation_id
        .map(str::trim)
        .filter(|value| !value.is_empty());

    with_state_mut(&project_root, |state| {
        ensure_session_exists(state, session_id)?;
        let mut reclaimed_from = None;
        if let Some(index) = state
            .locks
            .iter()
            .position(|lock| lock.target.key == target.key)
        {
            let existing = &state.locks[index];
            if !lock_is_expired(existing, now_ms) && existing.session_id != session_id {
                return Ok(SessionLockResponse {
                    protocol_version: CLI_CONTRACT_VERSION.to_string(),
                    phase: SESSION_PHASE.to_string(),
                    command: "lock".to_string(),
                    action: "acquire".to_string(),
                    ok: false,
                    session_id: session_id.to_string(),
                    target: target.clone(),
                    lock: Some(lock_view(existing, now_ms)),
                    denied_by: Some(existing.session_id.clone()),
                    reclaimed_from: None,
                    message: format!(
                        "lock denied: {} is already held by {} until {}",
                        target.key, existing.session_id, existing.expires_ms
                    ),
                });
            }
            if existing.session_id != session_id {
                reclaimed_from = Some(existing.session_id.clone());
            }
            state.locks.remove(index);
        }

        let stored = StoredLock {
            session_id: session_id.to_string(),
            operation_id: operation_id.map(ToString::to_string),
            acquired_ms: now_ms,
            expires_ms: now_ms + lease_ms,
            target: target.clone(),
        };
        state.locks.push(stored.clone());
        sort_locks(&mut state.locks);

        Ok(SessionLockResponse {
            protocol_version: CLI_CONTRACT_VERSION.to_string(),
            phase: SESSION_PHASE.to_string(),
            command: "lock".to_string(),
            action: "acquire".to_string(),
            ok: true,
            session_id: session_id.to_string(),
            target,
            lock: Some(lock_view(&stored, now_ms)),
            denied_by: None,
            reclaimed_from: reclaimed_from.clone(),
            message: if let Some(previous) = &reclaimed_from {
                format!("lock acquired after reclaiming expired lease from {previous}")
            } else {
                "lock acquired".to_string()
            },
        })
    })
}

pub fn release_lock(
    project_root: &Path,
    session_id: &str,
    target_tokens: &[String],
    now_ms: Option<u64>,
) -> anyhow::Result<SessionLockResponse> {
    let project_root = machine_paths::canonical_machine_path_buf(project_root);
    let session_id = session_id.trim();
    if session_id.is_empty() {
        bail!("session id must not be empty");
    }

    let target = resolve_cli_target(&project_root, target_tokens)?;
    let now_ms = now_ms.unwrap_or_else(current_time_ms);

    with_state_mut(&project_root, |state| {
        ensure_session_exists(state, session_id)?;
        let Some(index) = state
            .locks
            .iter()
            .position(|lock| lock.target.key == target.key)
        else {
            return Ok(SessionLockResponse {
                protocol_version: CLI_CONTRACT_VERSION.to_string(),
                phase: SESSION_PHASE.to_string(),
                command: "lock".to_string(),
                action: "release".to_string(),
                ok: false,
                session_id: session_id.to_string(),
                target: target.clone(),
                lock: None,
                denied_by: None,
                reclaimed_from: None,
                message: format!("no lock is recorded for {}", target.key),
            });
        };

        let existing = &state.locks[index];
        if existing.session_id != session_id {
            return Ok(SessionLockResponse {
                protocol_version: CLI_CONTRACT_VERSION.to_string(),
                phase: SESSION_PHASE.to_string(),
                command: "lock".to_string(),
                action: "release".to_string(),
                ok: false,
                session_id: session_id.to_string(),
                target: target.clone(),
                lock: Some(lock_view(existing, now_ms)),
                denied_by: Some(existing.session_id.clone()),
                reclaimed_from: None,
                message: format!(
                    "lock release denied: {} is owned by {}",
                    target.key, existing.session_id
                ),
            });
        }

        let released = state.locks.remove(index);
        Ok(SessionLockResponse {
            protocol_version: CLI_CONTRACT_VERSION.to_string(),
            phase: SESSION_PHASE.to_string(),
            command: "lock".to_string(),
            action: "release".to_string(),
            ok: true,
            session_id: session_id.to_string(),
            target,
            lock: Some(lock_view(&released, now_ms)),
            denied_by: None,
            reclaimed_from: None,
            message: "lock released".to_string(),
        })
    })
}

pub fn detect_conflicts(
    project_root: &Path,
    plan_path: &Path,
) -> anyhow::Result<SessionConflictResponse> {
    let project_root = machine_paths::canonical_machine_path_buf(project_root);
    let plan = analyze_plan(&project_root, plan_path, false, None)?;
    Ok(SessionConflictResponse {
        protocol_version: CLI_CONTRACT_VERSION.to_string(),
        phase: SESSION_PHASE.to_string(),
        command: "conflicts".to_string(),
        plan: display_path(plan_path),
        ok: plan.conflicts.is_empty(),
        operations: summarize_operations(&plan.operations),
        conflicts: plan.conflicts,
    })
}

pub fn validate_merge(
    project_root: &Path,
    plan_path: &Path,
    offline: bool,
    now_ms: Option<u64>,
) -> anyhow::Result<SessionMergeResponse> {
    let project_root = machine_paths::canonical_machine_path_buf(project_root);
    let plan = analyze_plan(&project_root, plan_path, true, now_ms)?;
    let operations = summarize_operations(&plan.operations);
    let mut conflicts = plan.conflicts;
    if !conflicts.is_empty() {
        return Ok(SessionMergeResponse {
            protocol_version: CLI_CONTRACT_VERSION.to_string(),
            phase: SESSION_PHASE.to_string(),
            command: "merge".to_string(),
            plan: display_path(plan_path),
            ok: false,
            valid: false,
            entry: String::new(),
            merged_files: Vec::new(),
            operations,
            conflicts,
            diagnostics: Vec::new(),
        });
    }

    let temp = copy_project_to_temp(&project_root)?;
    let temp_root = temp.path.as_path();
    let mut merged_files = BTreeSet::new();
    for operation in &plan.operations {
        let response = patch_protocol::apply_patch_document(
            temp_root,
            &operation.document,
            patch_protocol::PatchMode::Apply,
        )?;
        if !response.ok {
            conflicts.push(SessionConflictEntry {
                kind: "patch_conflict".to_string(),
                symbol: operation.symbols.first().cloned(),
                sessions: vec![operation.session_id.clone()],
                operation_ids: vec![operation.operation_id.clone()],
                patches: vec![operation.patch_display.clone()],
                message: response
                    .conflicts
                    .iter()
                    .map(|conflict| conflict.message.clone())
                    .collect::<Vec<_>>()
                    .join("; "),
            });
            continue;
        }
        for file in response.files_changed {
            merged_files.insert(relativize_path(temp_root, Path::new(&file)));
        }
    }

    if !conflicts.is_empty() {
        return Ok(SessionMergeResponse {
            protocol_version: CLI_CONTRACT_VERSION.to_string(),
            phase: SESSION_PHASE.to_string(),
            command: "merge".to_string(),
            plan: display_path(plan_path),
            ok: false,
            valid: false,
            entry: String::new(),
            merged_files: merged_files.into_iter().collect(),
            operations,
            conflicts,
            diagnostics: Vec::new(),
        });
    }

    let entry = resolve_merge_entry(temp_root)?;
    let output = run_frontend_with_options(&entry, FrontendOptions { offline })?;
    let diagnostics = output.diagnostics;
    let valid = !has_errors(&diagnostics);
    Ok(SessionMergeResponse {
        protocol_version: CLI_CONTRACT_VERSION.to_string(),
        phase: SESSION_PHASE.to_string(),
        command: "merge".to_string(),
        plan: display_path(plan_path),
        ok: valid,
        valid,
        entry: relativize_path(temp_root, &entry),
        merged_files: merged_files.into_iter().collect(),
        operations,
        conflicts,
        diagnostics,
    })
}

pub fn format_create_text(response: &SessionCreateResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!("session create: {}", response.session.id));
    if let Some(label) = &response.session.label {
        lines.push(format!("label: {label}"));
    }
    lines.push(format!("created_ms: {}", response.session.created_ms));
    lines.join("\n")
}

pub fn format_list_text(response: &SessionListResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "session list: {} session(s)",
        response.sessions.len()
    ));
    for session in &response.sessions {
        let mut line = format!("  {}", session.id);
        if let Some(label) = &session.label {
            line.push_str(&format!(" ({label})"));
        }
        line.push_str(&format!(
            " locks={} created_ms={}",
            session.active_locks, session.created_ms
        ));
        lines.push(line);
    }
    if response.locks.is_empty() {
        lines.push("locks: none".to_string());
    } else {
        lines.push(format!("locks ({}):", response.locks.len()));
        for lock in &response.locks {
            lines.push(format!(
                "  {} -> {} expires={}{}",
                lock.target.key,
                lock.session_id,
                lock.expires_ms,
                if lock.expired { " [expired]" } else { "" }
            ));
        }
    }
    lines.join("\n")
}

pub fn format_lock_text(response: &SessionLockResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "session lock {}: {}",
        response.action,
        if response.ok { "ok" } else { "denied" }
    ));
    lines.push(format!("session: {}", response.session_id));
    lines.push(format!("target: {}", response.target.key));
    lines.push(format!("message: {}", response.message));
    if let Some(lock) = &response.lock {
        lines.push(format!(
            "lease: owner={} acquired={} expires={}{}",
            lock.session_id,
            lock.acquired_ms,
            lock.expires_ms,
            if lock.expired { " [expired]" } else { "" }
        ));
    }
    lines.join("\n")
}

pub fn format_conflicts_text(response: &SessionConflictResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "session conflicts: {}",
        if response.ok { "none" } else { "detected" }
    ));
    lines.push(format!("operations: {}", response.operations.len()));
    if response.conflicts.is_empty() {
        lines.push("conflicts: none".to_string());
    } else {
        lines.push(format!("conflicts ({}):", response.conflicts.len()));
        for conflict in &response.conflicts {
            let symbol = conflict
                .symbol
                .as_ref()
                .map(|symbol| symbol.key.as_str())
                .unwrap_or("<none>");
            lines.push(format!(
                "  [{}] {} sessions={} ops={}",
                conflict.kind,
                symbol,
                conflict.sessions.join(","),
                conflict.operation_ids.join(",")
            ));
        }
    }
    lines.join("\n")
}

pub fn format_merge_text(response: &SessionMergeResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "session merge: {}",
        if response.valid { "valid" } else { "rejected" }
    ));
    if !response.entry.is_empty() {
        lines.push(format!("entry: {}", response.entry));
    }
    if !response.merged_files.is_empty() {
        lines.push(format!("files: {}", response.merged_files.join(", ")));
    }
    if !response.conflicts.is_empty() {
        lines.push(format!("conflicts: {}", response.conflicts.len()));
    }
    if !response.diagnostics.is_empty() {
        lines.push(format!("diagnostics: {}", response.diagnostics.len()));
    }
    lines.join("\n")
}

#[derive(Debug, Clone)]
struct PlanAnalysis {
    operations: Vec<PlannedOperation>,
    conflicts: Vec<SessionConflictEntry>,
}

fn analyze_plan(
    project_root: &Path,
    plan_path: &Path,
    enforce_locks: bool,
    now_ms: Option<u64>,
) -> anyhow::Result<PlanAnalysis> {
    let state = load_state(project_root)?;
    let session_ids = state
        .sessions
        .iter()
        .map(|session| session.id.clone())
        .collect::<BTreeSet<_>>();
    let symbols = symbol_query::list_symbols(project_root)?;
    let plan = read_plan_document(plan_path)?;
    let mut operations = Vec::new();
    let mut conflicts = Vec::new();

    for item in plan.operations {
        let session_id = item.session_id.trim().to_string();
        let operation_id = item.operation_id.trim().to_string();
        let patch_path = resolve_support_path(project_root, &item.patch);
        let patch_display = display_path(&patch_path);
        if session_id.is_empty() || operation_id.is_empty() {
            conflicts.push(SessionConflictEntry {
                kind: "invalid_operation".to_string(),
                symbol: None,
                sessions: vec![session_id.clone()],
                operation_ids: vec![operation_id.clone()],
                patches: vec![patch_display],
                message: "session plan entries require non-empty session_id and operation_id"
                    .to_string(),
            });
            continue;
        }
        if !session_ids.contains(&session_id) {
            conflicts.push(SessionConflictEntry {
                kind: "unknown_session".to_string(),
                symbol: None,
                sessions: vec![session_id.clone()],
                operation_ids: vec![operation_id.clone()],
                patches: vec![patch_display],
                message: format!("session `{session_id}` does not exist"),
            });
            continue;
        }

        let document = match patch_protocol::read_patch_document(&patch_path) {
            Ok(document) => document,
            Err(err) => {
                conflicts.push(SessionConflictEntry {
                    kind: "patch_document".to_string(),
                    symbol: None,
                    sessions: vec![session_id.clone()],
                    operation_ids: vec![operation_id.clone()],
                    patches: vec![patch_display],
                    message: err.to_string(),
                });
                continue;
            }
        };

        let symbols_for_operation =
            match touched_symbols_for_document(project_root, &symbols, &document) {
                Ok(symbols_for_operation) => symbols_for_operation,
                Err(err) => {
                    conflicts.push(SessionConflictEntry {
                        kind: "symbol_resolution".to_string(),
                        symbol: None,
                        sessions: vec![session_id.clone()],
                        operation_ids: vec![operation_id.clone()],
                        patches: vec![patch_display],
                        message: err,
                    });
                    continue;
                }
            };

        operations.push(PlannedOperation {
            session_id,
            operation_id,
            patch_display,
            document,
            symbols: symbols_for_operation,
        });
    }

    operations.sort_by(|lhs, rhs| {
        lhs.session_id
            .cmp(&rhs.session_id)
            .then(lhs.operation_id.cmp(&rhs.operation_id))
            .then(lhs.patch_display.cmp(&rhs.patch_display))
    });

    let mut by_symbol = BTreeMap::<String, Vec<&PlannedOperation>>::new();
    for operation in &operations {
        for key in operation
            .symbols
            .iter()
            .map(|symbol| symbol.key.clone())
            .collect::<BTreeSet<_>>()
        {
            by_symbol.entry(key).or_default().push(operation);
        }
    }

    for (symbol_key, operations_for_symbol) in &by_symbol {
        let distinct_sessions = operations_for_symbol
            .iter()
            .map(|operation| operation.session_id.clone())
            .collect::<BTreeSet<_>>();
        if distinct_sessions.len() <= 1 {
            continue;
        }
        let symbol = operations_for_symbol.iter().find_map(|operation| {
            operation
                .symbols
                .iter()
                .find(|symbol| symbol.key == *symbol_key)
                .cloned()
        });
        conflicts.push(SessionConflictEntry {
            kind: "symbol_overlap".to_string(),
            symbol,
            sessions: operations_for_symbol
                .iter()
                .map(|operation| operation.session_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            operation_ids: operations_for_symbol
                .iter()
                .map(|operation| operation.operation_id.clone())
                .collect(),
            patches: operations_for_symbol
                .iter()
                .map(|operation| operation.patch_display.clone())
                .collect(),
            message: format!("multiple sessions modify the same symbol {}", symbol_key),
        });
    }

    if enforce_locks {
        let now_ms = now_ms.unwrap_or_else(current_time_ms);
        for operation in &operations {
            for symbol in &operation.symbols {
                if symbol.synthetic {
                    continue;
                }
                let maybe_lock = state
                    .locks
                    .iter()
                    .find(|lock| lock.target.key == symbol.key && !lock_is_expired(lock, now_ms));
                match maybe_lock {
                    Some(lock) if lock.session_id == operation.session_id => {}
                    Some(lock) => conflicts.push(SessionConflictEntry {
                        kind: "lock_ownership".to_string(),
                        symbol: Some(symbol.clone()),
                        sessions: vec![operation.session_id.clone(), lock.session_id.clone()],
                        operation_ids: vec![operation.operation_id.clone()],
                        patches: vec![operation.patch_display.clone()],
                        message: format!(
                            "session `{}` cannot merge changes for {} while lock is owned by `{}`",
                            operation.session_id, symbol.key, lock.session_id
                        ),
                    }),
                    None => conflicts.push(SessionConflictEntry {
                        kind: "lock_missing".to_string(),
                        symbol: Some(symbol.clone()),
                        sessions: vec![operation.session_id.clone()],
                        operation_ids: vec![operation.operation_id.clone()],
                        patches: vec![operation.patch_display.clone()],
                        message: format!(
                            "session `{}` does not hold an active lock for {}",
                            operation.session_id, symbol.key
                        ),
                    }),
                }
            }
        }
    }

    conflicts.sort_by(|lhs, rhs| {
        lhs.kind
            .cmp(&rhs.kind)
            .then(lhs.sessions.cmp(&rhs.sessions))
            .then(lhs.operation_ids.cmp(&rhs.operation_ids))
            .then(
                lhs.symbol
                    .as_ref()
                    .map(|symbol| &symbol.key)
                    .cmp(&rhs.symbol.as_ref().map(|symbol| &symbol.key)),
            )
    });

    Ok(PlanAnalysis {
        operations,
        conflicts,
    })
}

fn touched_symbols_for_document(
    project_root: &Path,
    symbols: &[SymbolRecord],
    document: &PatchDocument,
) -> Result<Vec<SessionSymbol>, String> {
    let mut touched = Vec::new();
    for operation in &document.operations {
        match operation {
            PatchOperation::ModifyMatchArm {
                target_file,
                target_function,
                ..
            } => touched.push(resolve_named_symbol(
                project_root,
                symbols,
                SymbolKind::Function,
                target_function,
                target_file.as_deref(),
            )?),
            PatchOperation::AddField {
                target_file,
                target_struct,
                ..
            } => touched.push(resolve_named_symbol(
                project_root,
                symbols,
                SymbolKind::Struct,
                target_struct,
                target_file.as_deref(),
            )?),
            PatchOperation::AddFunction {
                target_file,
                after_symbol,
                function,
            } => touched.push(synthetic_add_function_symbol(
                project_root,
                symbols,
                function.name.trim(),
                target_file.as_deref(),
                after_symbol.as_deref(),
            )?),
        }
    }
    touched.sort_by(|lhs, rhs| lhs.key.cmp(&rhs.key));
    touched.dedup_by(|lhs, rhs| lhs.key == rhs.key);
    Ok(touched)
}

fn resolve_named_symbol(
    project_root: &Path,
    symbols: &[SymbolRecord],
    kind: SymbolKind,
    name: &str,
    target_file: Option<&str>,
) -> Result<SessionSymbol, String> {
    let target_name = name.trim();
    if target_name.is_empty() {
        return Err(format!("{} target name must not be empty", kind.as_str()));
    }
    let expected_file = target_file
        .map(|value| relativize_path(project_root, &resolve_support_path(project_root, value)));
    let mut candidates = symbols
        .iter()
        .filter(|symbol| symbol.kind == kind && symbol.name == target_name)
        .filter(|symbol| {
            expected_file.as_ref().is_none_or(|file| {
                relativize_path(project_root, Path::new(&symbol.location.file)) == file.as_str()
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|lhs, rhs| {
        lhs.module
            .cmp(&rhs.module)
            .then(lhs.location.file.cmp(&rhs.location.file))
            .then(lhs.location.span_start.cmp(&rhs.location.span_start))
    });
    match candidates.as_slice() {
        [] => Err(format!(
            "unable to resolve {} `{}`{}",
            kind.as_str(),
            target_name,
            expected_file
                .map(|file| format!(" in {}", file))
                .unwrap_or_default()
        )),
        [symbol] => Ok(symbol_to_session_symbol(project_root, symbol)),
        many => Err(format!(
            "ambiguous {} `{}` across {}",
            kind.as_str(),
            target_name,
            many.iter()
                .map(|symbol| normalize_symbol_file(symbol))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn synthetic_add_function_symbol(
    project_root: &Path,
    symbols: &[SymbolRecord],
    function_name: &str,
    target_file: Option<&str>,
    after_symbol: Option<&str>,
) -> Result<SessionSymbol, String> {
    let function_name = function_name.trim();
    if function_name.is_empty() {
        return Err("add_function requires non-empty function.name".to_string());
    }
    let file = if let Some(target_file) = target_file {
        relativize_path(
            project_root,
            &resolve_support_path(project_root, target_file),
        )
    } else if let Some(after_symbol) = after_symbol {
        resolve_named_symbol(
            project_root,
            symbols,
            SymbolKind::Function,
            after_symbol,
            None,
        )?
        .file
        .ok_or_else(|| format!("unable to resolve file for after_symbol `{after_symbol}`"))?
    } else {
        relativize_path(project_root, &project_root.join("src/main.aic"))
    };

    Ok(SessionSymbol {
        key: format!("pending:function:{}:{}", file, function_name),
        kind: "function".to_string(),
        name: function_name.to_string(),
        module: None,
        file: Some(file),
        span_start: None,
        span_end: None,
        synthetic: true,
    })
}

fn resolve_cli_target(
    project_root: &Path,
    target_tokens: &[String],
) -> anyhow::Result<SessionSymbol> {
    let selector = parse_target_selector(target_tokens)?;
    let symbols = symbol_query::list_symbols(project_root)?;
    select_cli_target(project_root, &symbols, &selector)
}

fn select_cli_target(
    project_root: &Path,
    symbols: &[SymbolRecord],
    selector: &TargetSelector,
) -> anyhow::Result<SessionSymbol> {
    let mut candidates = symbols
        .iter()
        .filter(|symbol| symbol.name == selector.name)
        .filter(|symbol| selector.kind.is_none_or(|kind| symbol.kind == kind))
        .filter(|symbol| {
            selector
                .module
                .as_ref()
                .is_none_or(|module| symbol.module.as_ref() == Some(module))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|lhs, rhs| {
        lhs.kind
            .as_str()
            .cmp(rhs.kind.as_str())
            .then(lhs.module.cmp(&rhs.module))
            .then(lhs.location.file.cmp(&rhs.location.file))
            .then(lhs.location.span_start.cmp(&rhs.location.span_start))
    });

    match candidates.as_slice() {
        [] => bail!("unknown session target `{}`", selector.name),
        [symbol] => Ok(symbol_to_session_symbol(project_root, symbol)),
        many => {
            let choices = many
                .iter()
                .map(|symbol| {
                    let module = symbol
                        .module
                        .clone()
                        .unwrap_or_else(|| "<root>".to_string());
                    format!("{} {} ({module})", symbol.kind.as_str(), symbol.name)
                })
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "ambiguous session target `{}`; choose one of: {}",
                selector.name,
                choices
            )
        }
    }
}

fn parse_target_selector(tokens: &[String]) -> anyhow::Result<TargetSelector> {
    if tokens.is_empty() {
        bail!("--for requires a target selector");
    }
    let joined = tokens.join(" ");
    let parts = joined
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        bail!("--for requires a non-empty target selector");
    }

    let (kind, raw_name) = if let Some(kind) = parse_kind_label(&parts[0]) {
        if parts.len() < 2 {
            bail!("--for requires a symbol name after `{}`", parts[0]);
        }
        (Some(kind), parts[1..].join(" "))
    } else {
        (None, parts.join(" "))
    };

    let raw_name = raw_name.trim();
    if raw_name.is_empty() {
        bail!("--for requires a non-empty symbol name");
    }
    let (module, name) = split_module_and_name(raw_name);
    Ok(TargetSelector { kind, name, module })
}

fn parse_kind_label(raw: &str) -> Option<SymbolKind> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "function" | "fn" => Some(SymbolKind::Function),
        "struct" => Some(SymbolKind::Struct),
        "enum" => Some(SymbolKind::Enum),
        "variant" => Some(SymbolKind::Variant),
        "trait" => Some(SymbolKind::Trait),
        "impl" => Some(SymbolKind::Impl),
        "module" => Some(SymbolKind::Module),
        _ => None,
    }
}

fn split_module_and_name(raw: &str) -> (Option<String>, String) {
    if let Some((module, name)) = raw.rsplit_once('.') {
        if !module.trim().is_empty() && !name.trim().is_empty() {
            return (Some(module.trim().to_string()), name.trim().to_string());
        }
    }
    (None, raw.trim().to_string())
}

fn ensure_session_exists(state: &SessionState, session_id: &str) -> anyhow::Result<()> {
    if state
        .sessions
        .iter()
        .any(|session| session.id == session_id)
    {
        Ok(())
    } else {
        bail!("unknown session `{session_id}`")
    }
}

fn lock_view(lock: &StoredLock, now_ms: u64) -> SessionLockView {
    SessionLockView {
        session_id: lock.session_id.clone(),
        operation_id: lock.operation_id.clone(),
        acquired_ms: lock.acquired_ms,
        expires_ms: lock.expires_ms,
        expired: lock_is_expired(lock, now_ms),
        target: lock.target.clone(),
    }
}

fn lock_is_expired(lock: &StoredLock, now_ms: u64) -> bool {
    now_ms >= lock.expires_ms
}

fn symbol_to_session_symbol(project_root: &Path, symbol: &SymbolRecord) -> SessionSymbol {
    let file = relativize_path(project_root, Path::new(&symbol.location.file));
    let module = symbol.module.clone();
    let module_key = module.clone().unwrap_or_else(|| "<root>".to_string());
    SessionSymbol {
        key: format!(
            "{}:{}:{}:{}:{}",
            symbol.kind.as_str(),
            module_key,
            symbol.name,
            file,
            symbol.location.span_start
        ),
        kind: symbol.kind.as_str().to_string(),
        name: symbol.name.clone(),
        module,
        file: Some(file),
        span_start: Some(symbol.location.span_start),
        span_end: Some(symbol.location.span_end),
        synthetic: false,
    }
}

fn normalize_symbol_file(symbol: &SymbolRecord) -> String {
    display_path(Path::new(&symbol.location.file))
}

fn summarize_operations(operations: &[PlannedOperation]) -> Vec<SessionOperationSummary> {
    operations
        .iter()
        .map(|operation| SessionOperationSummary {
            session_id: operation.session_id.clone(),
            operation_id: operation.operation_id.clone(),
            patch: operation.patch_display.clone(),
            symbols: operation.symbols.clone(),
        })
        .collect()
}

fn read_plan_document(path: &Path) -> anyhow::Result<SessionPlanDocument> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read session plan {}", path.display()))?;
    let plan = serde_json::from_str::<SessionPlanDocument>(&raw)
        .with_context(|| format!("failed to parse session plan {}", path.display()))?;
    if plan.operations.is_empty() {
        bail!("session plan must contain at least one operation");
    }
    Ok(plan)
}

fn load_state(project_root: &Path) -> anyhow::Result<SessionState> {
    let path = state_path(project_root);
    if !path.exists() {
        return Ok(SessionState {
            schema_version: SESSION_SCHEMA_VERSION,
            next_session_seq: default_next_session_seq(),
            sessions: Vec::new(),
            locks: Vec::new(),
        });
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut state = serde_json::from_str::<SessionState>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if state.schema_version != SESSION_SCHEMA_VERSION {
        bail!(
            "unsupported session state schema version {} in {}",
            state.schema_version,
            path.display()
        );
    }
    state.sessions.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    sort_locks(&mut state.locks);
    Ok(state)
}

fn with_state_mut<T>(
    project_root: &Path,
    op: impl FnOnce(&mut SessionState) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    fs::create_dir_all(sessions_root(project_root))
        .with_context(|| format!("failed to create {}", sessions_root(project_root).display()))?;
    let _guard = acquire_state_lock(project_root)?;
    let mut state = load_state(project_root)?;
    let output = op(&mut state)?;
    write_state(project_root, &state)?;
    Ok(output)
}

fn acquire_state_lock(project_root: &Path) -> anyhow::Result<StateLockGuard> {
    acquire_state_lock_with_options(
        project_root,
        LOCK_WAIT_RETRIES,
        LOCK_WAIT_INTERVAL_MS,
        stale_lock_ttl_ms(),
    )
}

fn acquire_state_lock_with_options(
    project_root: &Path,
    retries: usize,
    wait_interval_ms: u64,
    stale_lock_ttl_ms: u64,
) -> anyhow::Result<StateLockGuard> {
    let path = sessions_root(project_root).join(STATE_LOCK_NAME);
    let mut last_observation = None;
    for _ in 0..retries {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let metadata = StateLockMetadata::current();
                write_lock_metadata(&mut file, &metadata)?;
                return Ok(StateLockGuard { path });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if let Some(observation) = inspect_and_reclaim_state_lock(&path, stale_lock_ttl_ms)
                {
                    if observation.summary.starts_with("reclaimed stale lock") {
                        eprintln!("aic session: {} ({})", observation.summary, path.display());
                        last_observation = Some(observation.summary);
                        continue;
                    }
                    last_observation = Some(observation.summary);
                }
                thread::sleep(Duration::from_millis(wait_interval_ms));
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to acquire state lock {}", path.display()));
            }
        }
    }
    let observation = last_observation.unwrap_or_else(|| "lock metadata unavailable".to_string());
    bail!(
        "timed out waiting for session state lock {} ({observation}); if owner is no longer running, remove the lock file and retry",
        path.display(),
    )
}

fn write_lock_metadata(file: &mut fs::File, metadata: &StateLockMetadata) -> anyhow::Result<()> {
    let encoded = serde_json::to_vec(metadata).context("failed to encode lock metadata")?;
    file.write_all(&encoded)
        .context("failed to write lock metadata")?;
    file.flush().context("failed to flush lock metadata")?;
    Ok(())
}

fn inspect_and_reclaim_state_lock(path: &Path, stale_lock_ttl_ms: u64) -> Option<LockObservation> {
    let now_ms = current_time_ms();
    match read_lock_metadata(path) {
        Ok(metadata) => {
            let age_ms = now_ms.saturating_sub(metadata.created_ms);
            let same_host = metadata.host == current_host_identifier();
            let alive = if same_host {
                process_is_alive(metadata.pid)
            } else {
                None
            };
            let stale_by_pid = same_host && matches!(alive, Some(false));
            let stale_by_age =
                same_host && age_ms > stale_lock_ttl_ms && !matches!(alive, Some(true));
            if stale_by_pid || stale_by_age {
                if reclaim_state_lock_file(path) {
                    let reason = if stale_by_pid {
                        "owner process is no longer alive"
                    } else {
                        "lock age exceeded stale TTL"
                    };
                    return Some(LockObservation {
                        summary: format!(
                            "reclaimed stale lock pid={} host={} age_ms={} reason={reason}",
                            metadata.pid, metadata.host, age_ms
                        ),
                    });
                }
            }
            let alive_status = match alive {
                Some(true) => "alive",
                Some(false) => "dead",
                None => "unknown",
            };
            Some(LockObservation {
                summary: format!(
                    "lock owner pid={} host={} age_ms={} alive={alive_status}",
                    metadata.pid, metadata.host, age_ms
                ),
            })
        }
        Err(reason) => Some(LockObservation {
            summary: format!("malformed lock metadata: {reason}"),
        }),
    }
}

fn reclaim_state_lock_file(path: &Path) -> bool {
    let reclaimed = path.with_file_name(format!(
        "{}.reclaimed-{}-{}",
        STATE_LOCK_NAME,
        std::process::id(),
        current_time_ms()
    ));
    match fs::rename(path, &reclaimed) {
        Ok(()) => {
            let _ = fs::remove_file(reclaimed);
            true
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => false,
    }
}

fn read_lock_metadata(path: &Path) -> Result<StateLockMetadata, String> {
    let raw = fs::read_to_string(path).map_err(|err| err.to_string())?;
    match serde_json::from_str::<StateLockMetadata>(&raw) {
        Ok(metadata) => Ok(metadata),
        Err(json_err) => {
            let trimmed = raw.trim();
            if let Ok(pid) = trimmed.parse::<u32>() {
                return Ok(StateLockMetadata {
                    schema_version: 0,
                    pid,
                    host: current_host_identifier(),
                    created_ms: 0,
                    process_hint: "legacy_pid_only".to_string(),
                });
            }
            Err(format!("{} (raw: {})", json_err, trimmed))
        }
    }
}

fn stale_lock_ttl_ms() -> u64 {
    std::env::var("AIC_SESSION_LOCK_STALE_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_STALE_LOCK_TTL_MS)
}

fn current_host_identifier() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "unknown-host".to_string())
}

fn process_is_alive(pid: u32) -> Option<bool> {
    if pid == std::process::id() {
        return Some(true);
    }
    #[cfg(unix)]
    {
        return Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()
            .map(|status| status.success());
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn write_state(project_root: &Path, state: &SessionState) -> anyhow::Result<()> {
    let root = sessions_root(project_root);
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    let path = state_path(project_root);
    let temp_path = root.join(format!(".state-{}.tmp", std::process::id()));
    let encoded = serde_json::to_vec_pretty(state)?;
    fs::write(&temp_path, encoded)
        .with_context(|| format!("failed to write {}", temp_path.display()))?;
    fs::rename(&temp_path, &path)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

fn sessions_root(project_root: &Path) -> PathBuf {
    project_root.join(SESSIONS_DIR_NAME)
}

fn state_path(project_root: &Path) -> PathBuf {
    sessions_root(project_root).join(STATE_FILE_NAME)
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn fresh_temp_workspace(tag: &str) -> PathBuf {
    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let seq = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("aicore-{tag}-{pid}-{nanos}-{seq}"))
}

fn default_session_schema_version() -> u32 {
    SESSION_SCHEMA_VERSION
}

fn default_next_session_seq() -> u64 {
    1
}

fn sort_locks(locks: &mut [StoredLock]) {
    locks.sort_by(|lhs, rhs| {
        lhs.target
            .key
            .cmp(&rhs.target.key)
            .then(lhs.session_id.cmp(&rhs.session_id))
            .then(lhs.operation_id.cmp(&rhs.operation_id))
    });
}

fn resolve_merge_entry(project_root: &Path) -> anyhow::Result<PathBuf> {
    if let Some(manifest) = read_manifest(project_root)? {
        return Ok(project_root.join(manifest.main));
    }
    let fallback = project_root.join("src/main.aic");
    if fallback.exists() {
        Ok(fallback)
    } else {
        bail!(
            "unable to resolve merge entrypoint under {}",
            project_root.display()
        )
    }
}

fn copy_project_to_temp(project_root: &Path) -> anyhow::Result<TempWorkspace> {
    let temp = TempWorkspace {
        path: fresh_temp_workspace("session-merge"),
    };
    fs::create_dir_all(&temp.path)
        .with_context(|| format!("failed to create {}", temp.path.display()))?;
    copy_tree(project_root, &temp.path)?;
    Ok(temp)
}

fn copy_tree(source_root: &Path, destination_root: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(destination_root)
        .with_context(|| format!("failed to create {}", destination_root.display()))?;
    let mut entries = fs::read_dir(source_root)
        .with_context(|| format!("failed to read {}", source_root.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let source_path = entry.path();
        let name = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if matches!(
            name,
            ".git" | "target" | ".aic-checkpoints" | ".aic-sessions"
        ) {
            continue;
        }
        let destination_path = destination_root.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if ty.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "failed to copy {} into {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn resolve_support_path(project_root: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    }
}

fn relativize_path(project_root: &Path, path: &Path) -> String {
    let canonical_root = machine_paths::canonical_machine_path_buf(project_root);
    let canonical_path = machine_paths::canonical_machine_path_buf(path);
    if let Ok(relative) = canonical_path.strip_prefix(&canonical_root) {
        return machine_paths::normalize_separators_path(relative);
    }
    machine_paths::canonical_machine_path(&canonical_path)
}

fn display_path(path: &Path) -> String {
    machine_paths::canonical_machine_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn write_session_fixture(root: &Path) {
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(
            root.join("aic.toml"),
            "[package]\nname = \"session_fixture\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write manifest");
        fs::write(
            root.join("src/main.aic"),
            concat!(
                "module demo.session;\n",
                "struct Config {\n",
                "    port: Int\n",
                "}\n",
                "fn helper_status(x: Int) -> Int {\n",
                "    x\n",
                "}\n",
                "fn handle_result(x: Result[Int, Int]) -> Int {\n",
                "    match x {\n",
                "        Ok(v) => v,\n",
                "        Err(e) => helper_status(e),\n",
                "    }\n",
                "}\n",
                "fn main() -> Int {\n",
                "    handle_result(Ok(1))\n",
                "}\n",
            ),
        )
        .expect("write source");
    }

    fn write_state_lock_metadata(root: &Path, metadata: &StateLockMetadata) {
        let sessions_root = root.join(SESSIONS_DIR_NAME);
        fs::create_dir_all(&sessions_root).expect("create sessions dir");
        let lock_path = sessions_root.join(STATE_LOCK_NAME);
        let encoded = serde_json::to_vec(metadata).expect("encode lock metadata");
        fs::write(&lock_path, encoded).expect("write lock metadata");
    }

    #[test]
    fn orphan_state_lock_file_is_reclaimed_automatically() {
        let dir = tempdir().expect("tempdir");
        write_session_fixture(dir.path());
        write_state_lock_metadata(
            dir.path(),
            &StateLockMetadata {
                schema_version: STATE_LOCK_SCHEMA_VERSION,
                pid: u32::MAX,
                host: current_host_identifier(),
                created_ms: current_time_ms(),
                process_hint: "orphan-test".to_string(),
            },
        );

        let created = create_session(dir.path(), Some("auto-recover"), Some(100))
            .expect("session create should reclaim stale lock");
        assert_eq!(created.session.id, "sess-0001");
        assert!(!sessions_root(dir.path()).join(STATE_LOCK_NAME).exists());
    }

    #[test]
    fn live_state_lock_is_not_reclaimed_and_timeout_contains_metadata() {
        let dir = tempdir().expect("tempdir");
        write_session_fixture(dir.path());
        let pid = std::process::id();
        write_state_lock_metadata(
            dir.path(),
            &StateLockMetadata {
                schema_version: STATE_LOCK_SCHEMA_VERSION,
                pid,
                host: current_host_identifier(),
                created_ms: 0,
                process_hint: "live-lock-test".to_string(),
            },
        );

        let err = acquire_state_lock_with_options(dir.path(), 3, 1, 1)
            .expect_err("live owner lock should not be reclaimed");
        let text = err.to_string();
        assert!(text.contains("timed out waiting for session state lock"));
        assert!(text.contains("pid="));
        assert!(text.contains("alive=alive"));
        assert!(text.contains("remove the lock file and retry"));
        assert!(sessions_root(dir.path()).join(STATE_LOCK_NAME).exists());
    }

    #[test]
    fn malformed_lock_metadata_falls_back_to_retry_with_guidance() {
        let dir = tempdir().expect("tempdir");
        write_session_fixture(dir.path());
        let sessions_root = sessions_root(dir.path());
        fs::create_dir_all(&sessions_root).expect("create sessions dir");
        fs::write(sessions_root.join(STATE_LOCK_NAME), b"not-json").expect("write malformed lock");

        let err = acquire_state_lock_with_options(dir.path(), 3, 1, 1)
            .expect_err("malformed lock metadata should not be reclaimed automatically");
        let text = err.to_string();
        assert!(text.contains("malformed lock metadata"));
        assert!(text.contains("remove the lock file and retry"));
        assert!(sessions_root.join(STATE_LOCK_NAME).exists());
    }

    #[test]
    fn unit_lock_reclaim_is_deterministic() {
        let dir = tempdir().expect("tempdir");
        write_session_fixture(dir.path());
        let first = create_session(dir.path(), Some("alpha"), Some(100)).expect("create alpha");
        let second = create_session(dir.path(), Some("beta"), Some(101)).expect("create beta");

        let acquire = acquire_lock(
            dir.path(),
            &first.session.id,
            &["function".to_string(), "handle_result".to_string()],
            10,
            Some("op-a"),
            Some(1_000),
        )
        .expect("acquire first");
        assert!(acquire.ok);

        let denied = acquire_lock(
            dir.path(),
            &second.session.id,
            &["function".to_string(), "handle_result".to_string()],
            10,
            Some("op-b"),
            Some(1_005),
        )
        .expect("deny second");
        assert!(!denied.ok);
        assert_eq!(denied.denied_by.as_deref(), Some(first.session.id.as_str()));

        let reclaimed = acquire_lock(
            dir.path(),
            &second.session.id,
            &["function".to_string(), "handle_result".to_string()],
            10,
            Some("op-b"),
            Some(1_020),
        )
        .expect("reclaim second");
        assert!(reclaimed.ok);
        assert_eq!(
            reclaimed.reclaimed_from.as_deref(),
            Some(first.session.id.as_str())
        );
    }

    #[test]
    fn unit_conflict_analysis_reports_symbol_overlap_and_missing_locks() {
        let dir = tempdir().expect("tempdir");
        write_session_fixture(dir.path());
        let first = create_session(dir.path(), None, Some(10)).expect("create first");
        let second = create_session(dir.path(), None, Some(11)).expect("create second");

        let left_patch = dir.path().join("left.json");
        let right_patch = dir.path().join("right.json");
        let plan = dir.path().join("plan.json");
        fs::write(
            &left_patch,
            serde_json::to_vec_pretty(&json!({
                "operations": [
                    {
                        "kind": "modify_match_arm",
                        "target_file": "src/main.aic",
                        "target_function": "handle_result",
                        "match_index": 0,
                        "arm_pattern": "Err(e)",
                        "new_body": "helper_status(0 - e)"
                    }
                ]
            }))
            .expect("encode left patch"),
        )
        .expect("write left patch");
        fs::write(
            &right_patch,
            serde_json::to_vec_pretty(&json!({
                "operations": [
                    {
                        "kind": "modify_match_arm",
                        "target_file": "src/main.aic",
                        "target_function": "handle_result",
                        "match_index": 0,
                        "arm_pattern": "Err(e)",
                        "new_body": "0"
                    }
                ]
            }))
            .expect("encode right patch"),
        )
        .expect("write right patch");
        fs::write(
            &plan,
            serde_json::to_vec_pretty(&json!({
                "operations": [
                    {
                        "session_id": first.session.id,
                        "operation_id": "op-left",
                        "patch": left_patch.file_name().and_then(|value| value.to_str()).expect("left file name")
                    },
                    {
                        "session_id": second.session.id,
                        "operation_id": "op-right",
                        "patch": right_patch.file_name().and_then(|value| value.to_str()).expect("right file name")
                    }
                ]
            }))
            .expect("encode plan"),
        )
        .expect("write plan");

        let response =
            validate_merge(dir.path(), &plan, false, Some(100)).expect("merge validation");
        assert!(!response.valid);
        assert!(response
            .conflicts
            .iter()
            .any(|conflict| conflict.kind == "symbol_overlap"));
    }
}
