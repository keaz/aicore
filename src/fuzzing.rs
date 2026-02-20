use std::any::Any;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
    pub crashes: Vec<FuzzCrash>,
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
    let mut crashes = Vec::new();

    for (idx, source) in corpus.iter().enumerate() {
        if let Some(panic) = run_target_catch(target, source) {
            crashes.push(FuzzCrash {
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
            crashes.push(FuzzCrash {
                target,
                iteration,
                seed: config.seed,
                input,
                panic,
            });
        }
    }

    FuzzRunReport {
        target,
        iterations: config.iterations,
        corpus_cases: corpus.len(),
        crashes,
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
    use super::{FuzzConfig, Lcg};

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
}
