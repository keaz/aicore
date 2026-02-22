use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const TELEMETRY_SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub schema_version: String,
    pub event_index: u64,
    pub timestamp_ms: u64,
    pub trace_id: String,
    pub command: String,
    pub kind: String,
    pub phase: Option<String>,
    pub status: Option<String>,
    pub duration_ms: Option<f64>,
    pub metric_name: Option<String>,
    pub metric_value: Option<f64>,
    pub attrs: BTreeMap<String, Value>,
}

static TRACE_FALLBACK: OnceLock<String> = OnceLock::new();
static EVENT_INDEX: AtomicU64 = AtomicU64::new(1);
static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub fn telemetry_enabled() -> bool {
    telemetry_path().is_some()
}

pub fn current_trace_id() -> String {
    if let Some(explicit) = std::env::var("AIC_TRACE_ID")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        return explicit;
    }

    TRACE_FALLBACK
        .get_or_init(|| {
            let pid = std::process::id();
            let now = now_ms();
            let mut hasher = Sha256::new();
            hasher.update(format!("aicore-trace-{pid}-{now}"));
            let digest = format!("{:x}", hasher.finalize());
            digest[..16].to_string()
        })
        .clone()
}

pub fn emit_phase(
    command: &str,
    phase: &str,
    status: &str,
    duration: Duration,
    attrs: BTreeMap<String, Value>,
) {
    let Some(path) = telemetry_path() else {
        return;
    };
    let trace_id = current_trace_id();
    let event = TelemetryEvent {
        schema_version: TELEMETRY_SCHEMA_VERSION.to_string(),
        event_index: EVENT_INDEX.fetch_add(1, Ordering::Relaxed),
        timestamp_ms: now_ms(),
        trace_id,
        command: command.to_string(),
        kind: "phase".to_string(),
        phase: Some(phase.to_string()),
        status: Some(status.to_string()),
        duration_ms: Some(duration_to_ms(duration)),
        metric_name: None,
        metric_value: None,
        attrs,
    };
    let _ = append_event(&path, &event);
}

pub fn emit_metric(
    command: &str,
    metric_name: &str,
    metric_value: f64,
    attrs: BTreeMap<String, Value>,
) {
    let Some(path) = telemetry_path() else {
        return;
    };
    let trace_id = current_trace_id();
    let event = TelemetryEvent {
        schema_version: TELEMETRY_SCHEMA_VERSION.to_string(),
        event_index: EVENT_INDEX.fetch_add(1, Ordering::Relaxed),
        timestamp_ms: now_ms(),
        trace_id,
        command: command.to_string(),
        kind: "metric".to_string(),
        phase: None,
        status: None,
        duration_ms: None,
        metric_name: Some(metric_name.to_string()),
        metric_value: Some(metric_value),
        attrs,
    };
    let _ = append_event(&path, &event);
}

pub fn emit_phase_to_path(
    path: &Path,
    trace_id: &str,
    command: &str,
    phase: &str,
    status: &str,
    duration: Duration,
    attrs: BTreeMap<String, Value>,
) -> anyhow::Result<()> {
    let event = TelemetryEvent {
        schema_version: TELEMETRY_SCHEMA_VERSION.to_string(),
        event_index: EVENT_INDEX.fetch_add(1, Ordering::Relaxed),
        timestamp_ms: now_ms(),
        trace_id: trace_id.to_string(),
        command: command.to_string(),
        kind: "phase".to_string(),
        phase: Some(phase.to_string()),
        status: Some(status.to_string()),
        duration_ms: Some(duration_to_ms(duration)),
        metric_name: None,
        metric_value: None,
        attrs,
    };
    append_event(path, &event)
}

pub fn emit_metric_to_path(
    path: &Path,
    trace_id: &str,
    command: &str,
    metric_name: &str,
    metric_value: f64,
    attrs: BTreeMap<String, Value>,
) -> anyhow::Result<()> {
    let event = TelemetryEvent {
        schema_version: TELEMETRY_SCHEMA_VERSION.to_string(),
        event_index: EVENT_INDEX.fetch_add(1, Ordering::Relaxed),
        timestamp_ms: now_ms(),
        trace_id: trace_id.to_string(),
        command: command.to_string(),
        kind: "metric".to_string(),
        phase: None,
        status: None,
        duration_ms: None,
        metric_name: Some(metric_name.to_string()),
        metric_value: Some(metric_value),
        attrs,
    };
    append_event(path, &event)
}

pub fn read_events(path: &Path) -> anyhow::Result<Vec<TelemetryEvent>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        out.push(serde_json::from_str::<TelemetryEvent>(line)?);
    }
    Ok(out)
}

fn append_event(path: &Path, event: &TelemetryEvent) -> anyhow::Result<()> {
    let _guard = WRITE_LOCK.lock().expect("telemetry write mutex poisoned");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn telemetry_path() -> Option<PathBuf> {
    std::env::var("AIC_TELEMETRY_PATH")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn duration_to_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    use tempfile::tempdir;

    use super::{
        emit_metric_to_path, emit_phase, emit_phase_to_path, read_events, TELEMETRY_SCHEMA_VERSION,
    };

    #[test]
    fn schema_roundtrip_is_stable() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("telemetry.jsonl");

        emit_phase_to_path(
            &path,
            "trace-fixed",
            "check",
            "frontend.resolve",
            "ok",
            Duration::from_millis(12),
            BTreeMap::from([("input".to_string(), serde_json::json!("src/main.aic"))]),
        )
        .expect("emit phase");

        emit_metric_to_path(
            &path,
            "trace-fixed",
            "check",
            "diagnostic_count",
            3.0,
            BTreeMap::new(),
        )
        .expect("emit metric");

        let events = read_events(&path).expect("read events");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].schema_version, TELEMETRY_SCHEMA_VERSION);
        assert_eq!(events[0].trace_id, "trace-fixed");
        assert_eq!(events[0].kind, "phase");
        assert_eq!(events[1].kind, "metric");
        assert_eq!(events[1].metric_name.as_deref(), Some("diagnostic_count"));
    }

    #[test]
    fn disabled_fast_path_overhead_stays_bounded() {
        let started = Instant::now();
        for _ in 0..100_000 {
            emit_phase(
                "check",
                "frontend.typecheck",
                "ok",
                Duration::from_millis(1),
                BTreeMap::new(),
            );
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_millis(750),
            "disabled telemetry fast path too slow: {elapsed:?}"
        );
    }
}
