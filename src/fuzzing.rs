use std::any::Any;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ir_builder;
use crate::lexer;
use crate::parser;
use crate::resolver;
use crate::typecheck;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FuzzTarget {
    Lexer,
    Parser,
    Typecheck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzConfig {
    pub iterations: usize,
    pub max_len: usize,
    pub seed: u64,
}

impl Default for FuzzConfig {
    fn default() -> Self {
        Self {
            iterations: 256,
            max_len: 512,
            seed: 0xA1C0_5EED,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzCrash {
    pub target: FuzzTarget,
    pub iteration: usize,
    pub seed: u64,
    pub input: String,
    pub panic: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzRunReport {
    pub target: FuzzTarget,
    pub iterations: usize,
    pub corpus_cases: usize,
    pub total_crashes: usize,
    pub crashes: Vec<FuzzCrash>,
    pub triage: Vec<FuzzTriageCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzTriageCase {
    pub id: String,
    pub target: FuzzTarget,
    pub panic: String,
    pub minimized_input: String,
    pub first_iteration: usize,
    pub seed: u64,
    pub occurrences: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzRegressionResult {
    pub target: FuzzTarget,
    pub file: String,
    pub passed: bool,
    pub details: String,
}

pub fn load_corpus(root: &Path, target: FuzzTarget) -> anyhow::Result<Vec<String>> {
    let dir = root.join(target_dir_name(target));
    let mut files = collect_files(&dir)?;
    files.sort();

    let mut corpus = Vec::new();
    for file in files {
        corpus.push(fs::read_to_string(file)?);
    }
    Ok(corpus)
}

pub fn run_seeded_fuzz(target: FuzzTarget, corpus: &[String], config: FuzzConfig) -> FuzzRunReport {
    let mut rng = Lcg::new(config.seed);
    let mut raw_crashes = Vec::new();

    for (idx, source) in corpus.iter().enumerate() {
        if let Some(panic) = run_target_catch(target, source) {
            raw_crashes.push(FuzzCrash {
                target,
                iteration: idx,
                seed: config.seed,
                input: source.clone(),
                panic,
            });
        }
    }

    for iteration in 0..config.iterations {
        let input = mutate_from_corpus(corpus, &mut rng, config.max_len);
        if let Some(panic) = run_target_catch(target, &input) {
            raw_crashes.push(FuzzCrash {
                target,
                iteration,
                seed: config.seed,
                input,
                panic,
            });
        }
    }

    let (crashes, triage) = dedup_and_minimize_crashes(target, &raw_crashes);

    FuzzRunReport {
        target,
        iterations: config.iterations,
        corpus_cases: corpus.len(),
        total_crashes: raw_crashes.len(),
        crashes,
        triage,
    }
}

pub fn replay_regressions(
    root: &Path,
    target: FuzzTarget,
) -> anyhow::Result<Vec<FuzzRegressionResult>> {
    let dir = root.join(target_dir_name(target));
    let mut files = collect_files(&dir)?;
    files.sort();

    let mut out = Vec::new();
    for file in files {
        let source = fs::read_to_string(&file)?;
        let panic = run_target_catch(target, &source);
        let passed = panic.is_none();
        out.push(FuzzRegressionResult {
            target,
            file: file.to_string_lossy().to_string(),
            passed,
            details: panic.unwrap_or_else(|| "ok".to_string()),
        });
    }
    Ok(out)
}

pub fn release_gate_ok(reports: &[FuzzRunReport]) -> bool {
    reports.iter().all(|report| report.triage.is_empty())
}

pub fn write_crash_repro_artifacts(
    report: &FuzzRunReport,
    out_root: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    let target_dir = out_root.join(target_dir_name(report.target));
    fs::create_dir_all(&target_dir)?;

    let mut files = Vec::new();
    for case in &report.triage {
        let path = target_dir.join(format!("{}.aic", case.id));
        fs::write(&path, &case.minimized_input)?;
        files.push(path);
    }
    Ok(files)
}

fn run_target_catch(target: FuzzTarget, source: &str) -> Option<String> {
    let result = std::panic::catch_unwind(|| run_target(target, source));
    match result {
        Ok(()) => None,
        Err(payload) => Some(panic_payload_to_string(payload)),
    }
}

fn run_target(target: FuzzTarget, source: &str) {
    match target {
        FuzzTarget::Lexer => {
            let _ = lexer::lex(source, "fuzz_lexer.aic");
        }
        FuzzTarget::Parser => {
            let _ = parser::parse(source, "fuzz_parser.aic");
        }
        FuzzTarget::Typecheck => {
            let (program, _parse_diags) = parser::parse(source, "fuzz_typecheck.aic");
            if let Some(program) = program {
                let ir = ir_builder::build(&program);
                let (resolution, _resolve_diags) = resolver::resolve(&ir, "fuzz_typecheck.aic");
                let _ = typecheck::check(&ir, &resolution, "fuzz_typecheck.aic");
            }
        }
    }
}

fn dedup_and_minimize_crashes(
    target: FuzzTarget,
    raw: &[FuzzCrash],
) -> (Vec<FuzzCrash>, Vec<FuzzTriageCase>) {
    if raw.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut groups = BTreeMap::<String, Vec<&FuzzCrash>>::new();
    for crash in raw {
        groups.entry(crash.panic.clone()).or_default().push(crash);
    }

    let mut deduped = Vec::new();
    let mut triage = Vec::new();

    for (panic, mut group) in groups {
        group.sort_by(|a, b| a.iteration.cmp(&b.iteration).then(a.seed.cmp(&b.seed)));
        let first = group[0];
        let minimized = minimize_input(&first.input, |candidate| {
            run_target_catch(target, candidate).as_deref() == Some(panic.as_str())
        });
        let crash_id = crash_id(target, &panic, &minimized);

        deduped.push(FuzzCrash {
            target,
            iteration: first.iteration,
            seed: first.seed,
            input: minimized.clone(),
            panic: panic.clone(),
        });
        triage.push(FuzzTriageCase {
            id: crash_id,
            target,
            panic: panic.clone(),
            minimized_input: minimized,
            first_iteration: first.iteration,
            seed: first.seed,
            occurrences: group.len(),
        });
    }

    deduped.sort_by(|a, b| a.iteration.cmp(&b.iteration).then(a.panic.cmp(&b.panic)));
    triage.sort_by(|a, b| {
        a.first_iteration
            .cmp(&b.first_iteration)
            .then(a.panic.cmp(&b.panic))
    });

    (deduped, triage)
}

fn minimize_input<F>(input: &str, mut predicate: F) -> String
where
    F: FnMut(&str) -> bool,
{
    if input.is_empty() || !predicate(input) {
        return input.to_string();
    }

    let mut bytes = input.as_bytes().to_vec();
    let mut granularity = (bytes.len() / 2).max(1);

    while granularity > 0 {
        let mut reduced = false;
        let mut index = 0usize;
        while index < bytes.len() {
            let end = (index + granularity).min(bytes.len());
            if end <= index {
                break;
            }
            let mut candidate = bytes.clone();
            candidate.drain(index..end);
            let candidate_text = String::from_utf8_lossy(&candidate).to_string();
            if predicate(&candidate_text) {
                bytes = candidate;
                reduced = true;
            } else {
                index += granularity;
            }
        }

        if !reduced {
            if granularity == 1 {
                break;
            }
            granularity /= 2;
        }
    }

    String::from_utf8_lossy(&bytes).to_string()
}

fn crash_id(target: FuzzTarget, panic: &str, minimized_input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{target:?}").as_bytes());
    hasher.update([0]);
    hasher.update(panic.as_bytes());
    hasher.update([0]);
    hasher.update(minimized_input.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    digest[..12].to_string()
}

fn mutate_from_corpus(corpus: &[String], rng: &mut Lcg, max_len: usize) -> String {
    if corpus.is_empty() {
        return String::new();
    }

    let seed_idx = rng.next_usize(corpus.len());
    let mut bytes = corpus[seed_idx].as_bytes().to_vec();
    let operations = 1 + rng.next_usize(4);

    for _ in 0..operations {
        let op = rng.next_usize(4);
        match op {
            0 => {
                if !bytes.is_empty() {
                    let idx = rng.next_usize(bytes.len());
                    bytes[idx] = random_byte(rng);
                }
            }
            1 => {
                if bytes.len() < max_len {
                    let idx = if bytes.is_empty() {
                        0
                    } else {
                        rng.next_usize(bytes.len() + 1)
                    };
                    bytes.insert(idx, random_byte(rng));
                }
            }
            2 => {
                if !bytes.is_empty() {
                    let idx = rng.next_usize(bytes.len());
                    bytes.remove(idx);
                }
            }
            _ => {
                if !bytes.is_empty() && bytes.len() < max_len {
                    let start = rng.next_usize(bytes.len());
                    let len = 1 + rng.next_usize((bytes.len() - start).max(1));
                    let end = (start + len).min(bytes.len());
                    let chunk = bytes[start..end].to_vec();
                    let insert_at = rng.next_usize(bytes.len() + 1);
                    for (offset, byte) in chunk.into_iter().enumerate() {
                        if bytes.len() >= max_len {
                            break;
                        }
                        bytes.insert(insert_at + offset, byte);
                    }
                }
            }
        }
    }

    if bytes.len() > max_len {
        bytes.truncate(max_len);
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn random_byte(rng: &mut Lcg) -> u8 {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_{}[]()<>+-*/%:;,.\" \\n\\t";
    ALPHABET[rng.next_usize(ALPHABET.len())]
}

fn panic_payload_to_string(payload: Box<dyn Any + Send>) -> String {
    if let Some(msg) = payload.downcast_ref::<String>() {
        return msg.clone();
    }
    if let Some(msg) = payload.downcast_ref::<&str>() {
        return (*msg).to_string();
    }
    "non-string panic payload".to_string()
}

fn target_dir_name(target: FuzzTarget) -> &'static str {
    match target {
        FuzzTarget::Lexer => "lexer",
        FuzzTarget::Parser => "parser",
        FuzzTarget::Typecheck => "typecheck",
    }
}

fn collect_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }

    let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_files(&path)?);
            continue;
        }
        files.push(path);
    }

    Ok(files)
}

#[derive(Debug, Clone, Copy)]
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }

    fn next_usize(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive == 0 {
            return 0;
        }
        (self.next_u64() as usize) % upper_exclusive
    }
}

#[cfg(test)]
mod tests {
    use super::{
        crash_id, minimize_input, release_gate_ok, FuzzConfig, FuzzRunReport, FuzzTarget,
        FuzzTriageCase, Lcg,
    };

    #[test]
    fn lcg_is_deterministic_for_same_seed() {
        let mut a = Lcg::new(7);
        let mut b = Lcg::new(7);
        let seq_a = (0..8).map(|_| a.next_u64()).collect::<Vec<_>>();
        let seq_b = (0..8).map(|_| b.next_u64()).collect::<Vec<_>>();
        assert_eq!(seq_a, seq_b);
    }

    #[test]
    fn default_fuzz_config_has_non_zero_iterations() {
        let cfg = FuzzConfig::default();
        assert!(cfg.iterations > 0);
        assert!(cfg.max_len > 0);
    }

    #[test]
    fn minimizes_input_deterministically() {
        let minimized = minimize_input("xxpanicxx", |candidate| candidate.contains("panic"));
        assert_eq!(minimized, "panic");
    }

    #[test]
    fn crash_id_is_stable_for_same_payload() {
        let a = crash_id(FuzzTarget::Parser, "boom", "abc");
        let b = crash_id(FuzzTarget::Parser, "boom", "abc");
        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
    }

    #[test]
    fn release_gate_blocks_reports_with_unresolved_crashes() {
        let clean = FuzzRunReport {
            target: FuzzTarget::Lexer,
            iterations: 1,
            corpus_cases: 1,
            total_crashes: 0,
            crashes: Vec::new(),
            triage: Vec::new(),
        };
        assert!(release_gate_ok(std::slice::from_ref(&clean)));

        let failing = FuzzRunReport {
            target: FuzzTarget::Lexer,
            iterations: 1,
            corpus_cases: 1,
            total_crashes: 1,
            crashes: Vec::new(),
            triage: vec![FuzzTriageCase {
                id: "deadbeefcafe".to_string(),
                target: FuzzTarget::Lexer,
                panic: "panic".to_string(),
                minimized_input: "x".to_string(),
                first_iteration: 0,
                seed: 7,
                occurrences: 1,
            }],
        };
        assert!(!release_gate_ok(&[clean, failing]));
    }
}
