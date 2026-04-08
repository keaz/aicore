use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

use aicore::cli_contract::{EXIT_DIAGNOSTIC_ERROR, EXIT_OK};
use aicore::codegen::{
    compile_with_clang_artifact_with_options, emit_llvm_with_resolution_and_options, ArtifactKind,
    CodegenOptions, CompileOptions, OptimizationLevel,
};
use aicore::contracts::lower_runtime_asserts;
use aicore::driver::{diagnostics_pretty, has_errors, run_frontend_with_options, FrontendOptions};
use aicore::sandbox::{run_with_policy, SandboxPolicy};
use aicore::telemetry;
use serde::{Deserialize, Serialize};

pub const PROFILE_REPORT_SCHEMA_VERSION: &str = "1.0";
pub const PROFILE_TOP_FUNCTION_LIMIT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileFunctionTiming {
    pub function: String,
    pub self_time_ms: f64,
    pub total_time_ms: f64,
    pub calls: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileReport {
    pub phase: String,
    pub schema_version: String,
    pub input: String,
    pub output: String,
    pub offline: bool,
    pub sandbox_profile: String,
    pub total_time_ms: f64,
    pub top_functions: Vec<ProfileFunctionTiming>,
}

pub struct RunProfileOptions<'a> {
    pub input: &'a Path,
    pub offline: bool,
    pub args: &'a [String],
    pub policy: &'a SandboxPolicy,
    pub output_path: &'a Path,
}

pub struct RunProfileOutcome {
    pub exit_code: i32,
}

pub fn run_profiled(options: RunProfileOptions<'_>) -> anyhow::Result<RunProfileOutcome> {
    let project_root = crate::resolve_project_root(options.input);
    let link = crate::resolve_native_link_options(&project_root)?;
    let work = crate::fresh_work_dir("run-profile");
    let executable = work.join("aicore_run_bin");

    let front = run_frontend_with_options(
        options.input,
        FrontendOptions {
            offline: options.offline,
        },
    )?;
    if has_errors(&front.diagnostics) {
        print!("{}", diagnostics_pretty(&front.diagnostics));
        return Ok(RunProfileOutcome {
            exit_code: EXIT_DIAGNOSTIC_ERROR,
        });
    }

    let mut samples = vec![
        phase_timing("frontend.load", front.timings.load_ms),
        phase_timing("frontend.ir_build", front.timings.ir_build_ms),
        phase_timing(
            "frontend.effect_normalize",
            front.timings.effect_normalize_ms,
        ),
        phase_timing("frontend.resolve", front.timings.resolve_ms),
        phase_timing("frontend.typecheck", front.timings.typecheck_ms),
        phase_timing("frontend.verify", front.timings.verify_ms),
    ];

    let lower_started = Instant::now();
    let lowered = lower_runtime_asserts(&front.ir);
    samples.push(phase_timing(
        "contracts.lower_runtime_asserts",
        duration_ms(lower_started.elapsed()),
    ));

    let llvm_started = Instant::now();
    let llvm = match emit_llvm_with_resolution_and_options(
        &lowered,
        Some(&front.resolution),
        &options.input.to_string_lossy(),
        CodegenOptions::default(),
    ) {
        Ok(output) => output,
        Err(diags) => {
            print!("{}", diagnostics_pretty(&diags));
            return Ok(RunProfileOutcome {
                exit_code: EXIT_DIAGNOSTIC_ERROR,
            });
        }
    };
    samples.push(phase_timing(
        "codegen.llvm_emit",
        duration_ms(llvm_started.elapsed()),
    ));

    let clang_started = Instant::now();
    compile_with_clang_artifact_with_options(
        &llvm.llvm_ir,
        &executable,
        &work,
        ArtifactKind::Exe,
        CompileOptions {
            debug_info: false,
            opt_level: OptimizationLevel::O0,
            target_triple: None,
            static_link: false,
            link,
        },
    )?;
    samples.push(phase_timing(
        "codegen.clang_compile",
        duration_ms(clang_started.elapsed()),
    ));

    let trace_id = telemetry::current_trace_id();
    let execute_started = Instant::now();
    let status = run_with_policy(&executable, options.args, options.policy, Some(&trace_id))?;
    let execute_elapsed = execute_started.elapsed();
    let execute_ms = duration_ms(execute_elapsed);
    samples.push(phase_timing("run.execute", execute_ms));

    let attrs = BTreeMap::from([
        (
            "input".to_string(),
            serde_json::json!(options.input.display().to_string()),
        ),
        (
            "profile".to_string(),
            serde_json::json!(options.policy.profile.clone()),
        ),
    ]);
    telemetry::emit_phase(
        "run",
        "execute",
        if status.success() { "ok" } else { "error" },
        execute_elapsed,
        attrs.clone(),
    );
    telemetry::emit_metric(
        "run",
        "exit_code",
        status.code().unwrap_or(-1) as f64,
        attrs,
    );

    let report = build_report(&options, samples);
    write_report(options.output_path, &report)?;

    Ok(RunProfileOutcome {
        exit_code: if status.success() {
            EXIT_OK
        } else {
            EXIT_DIAGNOSTIC_ERROR
        },
    })
}

fn build_report(
    options: &RunProfileOptions<'_>,
    mut samples: Vec<ProfileFunctionTiming>,
) -> ProfileReport {
    samples.sort_by(|a, b| {
        b.total_time_ms
            .partial_cmp(&a.total_time_ms)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.function.cmp(&b.function))
    });

    let total_time_ms = round_three(samples.iter().map(|sample| sample.total_time_ms).sum());
    let top_functions = samples
        .into_iter()
        .take(PROFILE_TOP_FUNCTION_LIMIT)
        .collect::<Vec<_>>();

    ProfileReport {
        phase: "profile".to_string(),
        schema_version: PROFILE_REPORT_SCHEMA_VERSION.to_string(),
        input: options.input.display().to_string(),
        output: options.output_path.display().to_string(),
        offline: options.offline,
        sandbox_profile: options.policy.profile.clone(),
        total_time_ms,
        top_functions,
    }
}

fn write_report(path: &Path, report: &ProfileReport) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let encoded = serde_json::to_string_pretty(report)?;
    fs::write(path, encoded)?;
    Ok(())
}

fn phase_timing(name: &str, ms: f64) -> ProfileFunctionTiming {
    let rounded = round_three(ms.max(0.0));
    ProfileFunctionTiming {
        function: name.to_string(),
        self_time_ms: rounded,
        total_time_ms: rounded,
        calls: 1,
    }
}

fn duration_ms(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn round_three(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
