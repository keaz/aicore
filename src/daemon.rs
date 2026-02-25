use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::codegen::{
    compile_with_clang_artifact_with_options, emit_llvm_with_options, ArtifactKind, CodegenOptions,
    CompileOptions, LinkOptions,
};
use crate::contracts::lower_runtime_asserts;
use crate::diagnostics::Diagnostic;
use crate::driver::{has_errors, run_frontend_with_options, FrontendOptions, FrontendOutput};
use crate::package_workflow::{
    compute_package_checksum_for_path, native_link_config, resolve_dependency_context,
    NativeLinkConfig, PackageOptions,
};

#[derive(Default)]
struct DaemonState {
    frontend_cache: BTreeMap<String, FrontendCacheEntry>,
    build_cache: BTreeMap<String, BuildCacheEntry>,
    stats: DaemonStats,
}

#[derive(Clone)]
struct FrontendCacheEntry {
    fingerprint: String,
    output: Arc<FrontendOutput>,
}

#[derive(Clone)]
struct BuildCacheEntry {
    fingerprint: String,
    output: PathBuf,
    output_sha256: String,
}

#[derive(Debug, Clone, Serialize, Default)]
struct DaemonStats {
    requests_total: u64,
    check_requests: u64,
    build_requests: u64,
    stats_requests: u64,
    frontend_cache_hits: u64,
    frontend_cache_misses: u64,
    build_cache_hits: u64,
    build_cache_misses: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildKind {
    Exe,
    Obj,
    Lib,
}

impl BuildKind {
    fn parse(raw: Option<&str>) -> anyhow::Result<Self> {
        match raw.unwrap_or("exe") {
            "exe" => Ok(Self::Exe),
            "obj" => Ok(Self::Obj),
            "lib" => Ok(Self::Lib),
            other => anyhow::bail!("unsupported artifact '{other}', expected one of: exe|obj|lib"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Exe => "exe",
            Self::Obj => "obj",
            Self::Lib => "lib",
        }
    }

    fn to_codegen(self) -> ArtifactKind {
        match self {
            Self::Exe => ArtifactKind::Exe,
            Self::Obj => ArtifactKind::Obj,
            Self::Lib => ArtifactKind::Lib,
        }
    }
}

pub fn run_stdio() -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    let mut state = DaemonState::default();
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let message: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(err) => {
                write_response(
                    &mut writer,
                    &rpc_error(Value::Null, -32700, format!("invalid JSON payload: {err}")),
                )?;
                continue;
            }
        };

        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let method = match message.get("method").and_then(Value::as_str) {
            Some(v) => v,
            None => {
                write_response(
                    &mut writer,
                    &rpc_error(id, -32600, "invalid request: missing method"),
                )?;
                continue;
            }
        };
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

        state.stats.requests_total += 1;
        let result = match method {
            "check" => {
                state.stats.check_requests += 1;
                state.handle_check(&params)
            }
            "build" => {
                state.stats.build_requests += 1;
                state.handle_build(&params)
            }
            "stats" => {
                state.stats.stats_requests += 1;
                Ok(state.stats_response())
            }
            "shutdown" => {
                write_response(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "ok": true,
                            "stats": state.stats_response()
                        }
                    }),
                )?;
                break;
            }
            _ => Err(anyhow::anyhow!("method not found: {method}")),
        };

        match result {
            Ok(payload) => {
                write_response(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": payload,
                    }),
                )?;
            }
            Err(err) => {
                let code = if method == "check" || method == "build" {
                    -32602
                } else {
                    -32601
                };
                write_response(&mut writer, &rpc_error(id, code, err.to_string()))?;
            }
        }
    }

    writer.flush()?;
    Ok(())
}

impl DaemonState {
    fn handle_check(&mut self, params: &Value) -> anyhow::Result<Value> {
        let started = Instant::now();
        let input = request_path(params, "input")?;
        let offline = params
            .get("offline")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let fingerprint = compute_input_fingerprint(&input, offline)?;

        let (front, cache_hit) = self.frontend_output(&input, offline, &fingerprint)?;
        let diagnostics = front.diagnostics.clone();

        Ok(json!({
            "input": normalize_path(&canonical_or_self(input)),
            "cache_hit": cache_hit,
            "fingerprint": fingerprint,
            "has_errors": has_errors(&diagnostics),
            "diagnostics": diagnostics,
            "duration_ms": started.elapsed().as_millis(),
        }))
    }

    fn handle_build(&mut self, params: &Value) -> anyhow::Result<Value> {
        let started = Instant::now();
        let input = request_path(params, "input")?;
        let offline = params
            .get("offline")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let debug_info = params
            .get("debug_info")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let artifact = BuildKind::parse(params.get("artifact").and_then(Value::as_str))?;

        let output = if let Some(path) = params.get("output").and_then(Value::as_str) {
            PathBuf::from(path)
        } else {
            default_build_output_name(&input, artifact)
        };

        let output_fingerprint = compute_input_fingerprint(&input, offline)?;
        let fingerprint = format!(
            "{output_fingerprint}\nartifact={}\ndebug_info={debug_info}\noutput={}",
            artifact.as_str(),
            normalize_path(&output)
        );

        let build_key = format!(
            "{}|offline={offline}|artifact={}|debug_info={debug_info}|output={}",
            normalize_path(&canonical_or_self(input.clone())),
            artifact.as_str(),
            normalize_path(&output),
        );

        if let Some(existing) = self.build_cache.get(&build_key) {
            if existing.fingerprint == fingerprint && existing.output.exists() {
                self.stats.build_cache_hits += 1;
                return Ok(json!({
                    "input": normalize_path(&canonical_or_self(input)),
                    "output": normalize_path(&existing.output),
                    "artifact": artifact.as_str(),
                    "cache_hit": true,
                    "frontend_cache_hit": true,
                    "fingerprint": fingerprint,
                    "has_errors": false,
                    "diagnostics": [],
                    "output_sha256": existing.output_sha256,
                    "duration_ms": started.elapsed().as_millis(),
                }));
            }
        }
        self.stats.build_cache_misses += 1;

        let (front, frontend_cache_hit) =
            self.frontend_output(&input, offline, &output_fingerprint)?;
        if has_errors(&front.diagnostics) {
            return Ok(json!({
                "input": normalize_path(&canonical_or_self(input)),
                "output": normalize_path(&output),
                "artifact": artifact.as_str(),
                "cache_hit": false,
                "frontend_cache_hit": frontend_cache_hit,
                "fingerprint": fingerprint,
                "has_errors": true,
                "diagnostics": front.diagnostics,
                "output_sha256": Value::Null,
                "duration_ms": started.elapsed().as_millis(),
            }));
        }

        let lowered = lower_runtime_asserts(&front.ir);
        let llvm = match emit_llvm_with_options(
            &lowered,
            &input.to_string_lossy(),
            CodegenOptions { debug_info },
        ) {
            Ok(v) => v,
            Err(diags) => {
                return Ok(json!({
                    "input": normalize_path(&canonical_or_self(input)),
                    "output": normalize_path(&output),
                    "artifact": artifact.as_str(),
                    "cache_hit": false,
                    "frontend_cache_hit": frontend_cache_hit,
                    "fingerprint": fingerprint,
                    "has_errors": true,
                    "diagnostics": diags,
                    "output_sha256": Value::Null,
                    "duration_ms": started.elapsed().as_millis(),
                }));
            }
        };

        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create output directory '{}'",
                    parent.to_string_lossy()
                )
            })?;
        }

        let project_root = resolve_project_root(&input);
        let link = resolve_native_link_options(&project_root)?;
        let work = fresh_work_dir("daemon-build");
        compile_with_clang_artifact_with_options(
            &llvm.llvm_ir,
            &output,
            &work,
            artifact.to_codegen(),
            CompileOptions {
                debug_info,
                target_triple: None,
                static_link: false,
                link,
            },
        )?;

        let output_sha256 = sha256_file(&output)?;
        self.build_cache.insert(
            build_key,
            BuildCacheEntry {
                fingerprint: fingerprint.clone(),
                output: output.clone(),
                output_sha256: output_sha256.clone(),
            },
        );

        Ok(json!({
            "input": normalize_path(&canonical_or_self(input)),
            "output": normalize_path(&output),
            "artifact": artifact.as_str(),
            "cache_hit": false,
            "frontend_cache_hit": frontend_cache_hit,
            "fingerprint": fingerprint,
            "has_errors": false,
            "diagnostics": Vec::<Diagnostic>::new(),
            "output_sha256": output_sha256,
            "duration_ms": started.elapsed().as_millis(),
        }))
    }

    fn frontend_output(
        &mut self,
        input: &Path,
        offline: bool,
        fingerprint: &str,
    ) -> anyhow::Result<(Arc<FrontendOutput>, bool)> {
        let key = format!(
            "{}|offline={offline}",
            normalize_path(&canonical_or_self(input.to_path_buf()))
        );
        if let Some(existing) = self.frontend_cache.get(&key) {
            if existing.fingerprint == fingerprint {
                self.stats.frontend_cache_hits += 1;
                return Ok((Arc::clone(&existing.output), true));
            }
        }

        self.stats.frontend_cache_misses += 1;
        let output = Arc::new(run_frontend_with_options(
            input,
            FrontendOptions { offline },
        )?);
        self.frontend_cache.insert(
            key,
            FrontendCacheEntry {
                fingerprint: fingerprint.to_string(),
                output: Arc::clone(&output),
            },
        );
        Ok((output, false))
    }

    fn stats_response(&self) -> Value {
        json!({
            "requests_total": self.stats.requests_total,
            "check_requests": self.stats.check_requests,
            "build_requests": self.stats.build_requests,
            "stats_requests": self.stats.stats_requests,
            "frontend_cache_hits": self.stats.frontend_cache_hits,
            "frontend_cache_misses": self.stats.frontend_cache_misses,
            "build_cache_hits": self.stats.build_cache_hits,
            "build_cache_misses": self.stats.build_cache_misses,
            "frontend_cache_entries": self.frontend_cache.len(),
            "build_cache_entries": self.build_cache.len(),
        })
    }
}

fn request_path(params: &Value, key: &str) -> anyhow::Result<PathBuf> {
    let raw = params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing string parameter: {key}"))?;
    if raw.trim().is_empty() {
        anyhow::bail!("parameter '{key}' must not be empty");
    }
    Ok(PathBuf::from(raw))
}

fn compute_input_fingerprint(input: &Path, offline: bool) -> anyhow::Result<String> {
    let canonical_input = canonical_or_self(input.to_path_buf());
    let project_root = resolve_project_root(&canonical_input);
    let self_checksum = compute_package_checksum_for_path(&project_root)
        .with_context(|| format!("failed to checksum package '{}'", project_root.display()))?;

    let dep_context = resolve_dependency_context(&project_root, PackageOptions { offline })?;
    let mut lines = vec![
        format!("input={}", normalize_path(&canonical_input)),
        format!("project_root={}", normalize_path(&project_root)),
        format!("offline={offline}"),
        format!("self={self_checksum}"),
        format!("lockfile_used={}", dep_context.lockfile_used),
    ];

    let mut dep_lines = Vec::new();
    for root in dep_context.source_roots {
        let canonical = canonical_or_self(root);
        if canonical == project_root || !canonical.exists() {
            continue;
        }
        let checksum = compute_package_checksum_for_path(&canonical)
            .with_context(|| format!("failed to checksum dependency '{}'", canonical.display()))?;
        dep_lines.push(format!("dep:{}={checksum}", normalize_path(&canonical)));
    }
    dep_lines.sort();
    lines.extend(dep_lines);

    let mut diag_lines = dep_context
        .diagnostics
        .iter()
        .map(|diag| format!("ctx_diag:{}:{}", diag.code, diag.message))
        .collect::<Vec<_>>();
    diag_lines.sort();
    lines.extend(diag_lines);

    Ok(lines.join("\n"))
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read output artifact '{}'", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn resolve_native_link_options(project_root: &Path) -> anyhow::Result<LinkOptions> {
    let native = native_link_config(project_root)?;
    Ok(native_to_link_options(project_root, &native))
}

fn native_to_link_options(project_root: &Path, native: &NativeLinkConfig) -> LinkOptions {
    LinkOptions {
        search_paths: native
            .search_paths
            .iter()
            .map(|path| resolve_native_path(project_root, path))
            .collect(),
        libs: native.libs.clone(),
        objects: native
            .objects
            .iter()
            .map(|path| resolve_native_path(project_root, path))
            .collect(),
    }
}

fn resolve_native_path(project_root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    }
}

fn default_build_output_name(input: &Path, artifact: BuildKind) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("a.out");
    match artifact {
        BuildKind::Exe => PathBuf::from(stem),
        BuildKind::Obj => PathBuf::from(format!("{stem}.o")),
        BuildKind::Lib => PathBuf::from(format!("lib{stem}.a")),
    }
}

fn fresh_work_dir(tag: &str) -> PathBuf {
    static WORK_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let seq = WORK_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("aicore-{tag}-{pid}-{nanos}-{seq}"))
}

fn resolve_project_root(path: &Path) -> PathBuf {
    let fallback = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    let mut dir = fallback.clone();

    loop {
        if dir.join("aic.toml").exists() {
            return dir;
        }
        let Some(parent) = dir.parent() else {
            return fallback;
        };
        dir = parent.to_path_buf();
    }
}

fn canonical_or_self(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn write_response(writer: &mut impl Write, value: &Value) -> anyhow::Result<()> {
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn rpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::BuildKind;

    #[test]
    fn parses_supported_build_kind_values() {
        assert_eq!(BuildKind::parse(Some("exe")).expect("exe"), BuildKind::Exe);
        assert_eq!(BuildKind::parse(Some("obj")).expect("obj"), BuildKind::Obj);
        assert_eq!(BuildKind::parse(Some("lib")).expect("lib"), BuildKind::Lib);
    }

    #[test]
    fn rejects_unknown_build_kind() {
        let err = BuildKind::parse(Some("dll")).expect_err("unknown kind should fail");
        assert!(err.to_string().contains("unsupported artifact"));
    }
}
