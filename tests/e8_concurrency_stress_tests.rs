use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde::{Deserialize, Serialize};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn plan_path(root: &Path) -> PathBuf {
    root.join("examples/e8/concurrency-stress-plan.json")
}

#[derive(Debug, Deserialize)]
struct StressPlan {
    version: u32,
    seeds: Vec<u64>,
    rounds_per_seed: u32,
    max_runtime_seconds: u64,
    cases: Vec<StressCasePlan>,
}

#[derive(Debug, Deserialize)]
struct StressCasePlan {
    id: String,
    path: String,
    expected_stdout: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ScheduledRun {
    sequence: u64,
    seed: u64,
    round: u32,
    case_id: String,
    case_path: String,
    expected_stdout: String,
    replay_token: String,
    replay_command: String,
}

#[derive(Debug, Clone, Serialize)]
struct StressOutcome {
    sequence: u64,
    seed: u64,
    round: u32,
    case_id: String,
    case_path: String,
    expected_stdout: String,
    exit_code: Option<i32>,
    stdout_tail: String,
    stderr_tail: String,
    duration_ms: u64,
    passed: bool,
    replay_token: String,
    replay_command: String,
    stdout_excerpt: String,
    stderr_excerpt: String,
}

#[derive(Debug, Serialize)]
struct StressReport {
    version: u32,
    plan_path: String,
    total_runs: usize,
    runtime_budget_seconds: u64,
    elapsed_ms: u64,
    flaky_policy: String,
    replay_filter: Option<String>,
    schedule: Vec<ScheduledRun>,
    outcomes: Vec<StressOutcome>,
    failure_count: usize,
}

fn load_plan(root: &Path) -> StressPlan {
    let text = fs::read_to_string(plan_path(root)).expect("read concurrency stress plan");
    serde_json::from_str(&text).expect("parse concurrency stress plan")
}

fn next_lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

fn replay_command(token: &str) -> String {
    format!(
        "AIC_CONC_STRESS_REPLAY='{token}' cargo test --locked --test e8_concurrency_stress_tests -- --exact concurrency_stress_suite_is_replayable_and_within_budget --nocapture --test-threads=1"
    )
}

fn build_schedule(plan: &StressPlan) -> Vec<ScheduledRun> {
    let mut runs = Vec::new();
    let mut sequence = 0u64;
    for &seed in &plan.seeds {
        let mut state = seed;
        for round in 0..plan.rounds_per_seed {
            let mut order = (0..plan.cases.len()).collect::<Vec<_>>();
            for idx in (1..order.len()).rev() {
                let pick = (next_lcg(&mut state) as usize) % (idx + 1);
                order.swap(idx, pick);
            }
            for case_idx in order {
                let case = &plan.cases[case_idx];
                let replay_token = format!("{}:{}:{}", seed, round, case.id);
                runs.push(ScheduledRun {
                    sequence,
                    seed,
                    round,
                    case_id: case.id.clone(),
                    case_path: case.path.clone(),
                    expected_stdout: case.expected_stdout.clone(),
                    replay_command: replay_command(&replay_token),
                    replay_token,
                });
                sequence += 1;
            }
        }
    }
    runs
}

fn last_non_empty_line(text: &str) -> String {
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_default()
}

fn truncate_for_report(text: &str) -> String {
    const LIMIT: usize = 400;
    let normalized = text.trim().replace('\n', "\\n");
    if normalized.len() <= LIMIT {
        normalized
    } else {
        format!("{}...", &normalized[..LIMIT])
    }
}

fn run_case(root: &Path, run: &ScheduledRun) -> StressOutcome {
    let trace_id = format!("e8-conc-{:x}-{}-{}", run.seed, run.round, run.case_id);
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(["run", run.case_path.as_str()])
        .current_dir(root)
        .env("AIC_TEST_MODE", "1")
        .env("AIC_TEST_SEED", run.seed.to_string())
        .env("AIC_TRACE_ID", trace_id)
        .output()
        .expect("run aic stress case");
    let duration_ms = started.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout_tail = last_non_empty_line(&stdout);
    let stderr_tail = last_non_empty_line(&stderr);
    let passed = output.status.success() && stdout_tail == run.expected_stdout;
    StressOutcome {
        sequence: run.sequence,
        seed: run.seed,
        round: run.round,
        case_id: run.case_id.clone(),
        case_path: run.case_path.clone(),
        expected_stdout: run.expected_stdout.clone(),
        exit_code: output.status.code(),
        stdout_tail,
        stderr_tail,
        duration_ms,
        passed,
        replay_token: run.replay_token.clone(),
        replay_command: run.replay_command.clone(),
        stdout_excerpt: truncate_for_report(&stdout),
        stderr_excerpt: truncate_for_report(&stderr),
    }
}

fn filter_schedule(schedule: Vec<ScheduledRun>, replay_filter: Option<&str>) -> Vec<ScheduledRun> {
    let Some(token) = replay_filter else {
        return schedule;
    };
    schedule
        .into_iter()
        .filter(|run| run.replay_token == token)
        .collect()
}

fn write_artifacts(root: &Path, report: &StressReport) {
    let out_dir = root.join("target/e8");
    fs::create_dir_all(&out_dir).expect("mkdir target/e8");
    fs::write(
        out_dir.join("concurrency-stress-report.json"),
        serde_json::to_string_pretty(report).expect("serialize stress report"),
    )
    .expect("write stress report");
    fs::write(
        out_dir.join("concurrency-stress-schedule.json"),
        serde_json::to_string_pretty(&report.schedule).expect("serialize stress schedule"),
    )
    .expect("write stress schedule");

    let mut replay_lines = vec![
        "Deterministic concurrency stress replay commands".to_string(),
        "Use one command at a time with --test-threads=1.".to_string(),
    ];
    if let Some(token) = &report.replay_filter {
        replay_lines.push(format!("active replay token: {token}"));
    }
    let failing = report
        .outcomes
        .iter()
        .filter(|outcome| !outcome.passed)
        .map(|outcome| outcome.replay_command.clone())
        .collect::<Vec<_>>();
    if failing.is_empty() {
        if let Some(first) = report.schedule.first() {
            replay_lines.push(format!("sample replay: {}", first.replay_command));
        }
    } else {
        replay_lines.push("failing runs:".to_string());
        replay_lines.extend(failing);
    }
    fs::write(
        out_dir.join("concurrency-stress-replay.txt"),
        replay_lines.join("\n"),
    )
    .expect("write stress replay");
}

#[test]
fn concurrency_stress_schedule_is_deterministic() {
    let root = repo_root();
    let plan = load_plan(&root);
    let first = build_schedule(&plan);
    let second = build_schedule(&plan);
    assert_eq!(first, second, "stress schedule must be deterministic");
}

#[test]
fn concurrency_stress_suite_is_replayable_and_within_budget() {
    let root = repo_root();
    let plan = load_plan(&root);
    assert!(
        !plan.cases.is_empty(),
        "concurrency stress plan must define at least one case"
    );
    assert!(
        !plan.seeds.is_empty(),
        "concurrency stress plan must define at least one seed"
    );
    let replay_filter = std::env::var("AIC_CONC_STRESS_REPLAY").ok();
    let schedule = filter_schedule(build_schedule(&plan), replay_filter.as_deref());
    assert!(
        !schedule.is_empty(),
        "replay token did not match schedule; token={}",
        replay_filter.unwrap_or_default()
    );

    let started = Instant::now();
    let outcomes = schedule
        .iter()
        .map(|run| run_case(&root, run))
        .collect::<Vec<_>>();
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let failure_count = outcomes.iter().filter(|outcome| !outcome.passed).count();
    let report = StressReport {
        version: plan.version,
        plan_path: plan_path(&root).display().to_string(),
        total_runs: outcomes.len(),
        runtime_budget_seconds: plan.max_runtime_seconds,
        elapsed_ms,
        flaky_policy:
            "No retries in CI. Replay exactly one failing token locally before filing a regression."
                .to_string(),
        replay_filter,
        schedule,
        outcomes: outcomes.clone(),
        failure_count,
    };
    write_artifacts(&root, &report);

    assert!(
        elapsed_ms <= plan.max_runtime_seconds * 1_000,
        "concurrency stress runtime budget exceeded: {}ms > {}ms (report: target/e8/concurrency-stress-report.json)",
        elapsed_ms,
        plan.max_runtime_seconds * 1_000
    );
    if failure_count > 0 {
        let replay = outcomes
            .iter()
            .filter(|outcome| !outcome.passed)
            .map(|outcome| outcome.replay_command.clone())
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "concurrency stress regressions detected ({} failing runs).\n{}\nreport: target/e8/concurrency-stress-report.json\nreplay list: target/e8/concurrency-stress-replay.txt",
            failure_count, replay
        );
    }
}
