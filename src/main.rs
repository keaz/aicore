use std::path::{Path, PathBuf};

use aicore::cli_contract::{
    contract_json, EXIT_DIAGNOSTIC_ERROR, EXIT_INTERNAL_ERROR, EXIT_OK, EXIT_USAGE_ERROR,
};
use aicore::codegen::{
    compile_with_clang, compile_with_clang_artifact_with_options, emit_llvm,
    emit_llvm_with_options, ArtifactKind, CodegenOptions, CompileOptions,
};
use aicore::contracts::lower_runtime_asserts;
use aicore::diagnostic_explain::{explain, explain_text};
use aicore::diagnostics::Severity;
use aicore::docgen::generate_docs;
use aicore::driver::{diagnostics_pretty, has_errors, run_frontend_with_options, FrontendOptions};
use aicore::formatter::format_program;
use aicore::ir::migrate_json_to_current;
use aicore::ir_builder;
use aicore::lsp;
use aicore::package_workflow::generate_and_write_lockfile;
use aicore::parser;
use aicore::project::init_project;
use aicore::release_ops::{
    check_compatibility_policy, compatibility_policy, effective_source_date_epoch,
    generate_provenance, generate_repro_manifest, generate_sbom, read_provenance,
    read_repro_manifest, run_security_audit, verify_provenance, verify_repro_manifest,
    write_provenance, write_repro_manifest, write_sbom,
};
use aicore::sandbox::{run_with_limits, SandboxProfile};
use aicore::sarif::diagnostics_to_sarif;
use aicore::std_policy::{
    collect_std_api_snapshot, compare_snapshots, default_std_root, StdApiSnapshot,
};
use aicore::test_harness::{run_harness, HarnessMode};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "aic", version, about = "AICore compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Check {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long, conflicts_with = "sarif")]
        json: bool,
        #[arg(long, conflicts_with = "json")]
        sarif: bool,
        #[arg(long)]
        offline: bool,
    },
    Diag {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long, conflicts_with = "sarif")]
        json: bool,
        #[arg(long, conflicts_with = "json")]
        sarif: bool,
        #[arg(long)]
        offline: bool,
    },
    Explain {
        code: String,
        #[arg(long)]
        json: bool,
    },
    Fmt {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long)]
        check: bool,
    },
    Ir {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long, value_enum, default_value = "json")]
        emit: EmitKind,
        #[arg(long)]
        offline: bool,
    },
    IrMigrate {
        #[arg(default_value = "ir.json")]
        input: PathBuf,
    },
    Lock {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Build {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "exe")]
        artifact: BuildArtifact,
        #[arg(long)]
        debug_info: bool,
        #[arg(long)]
        offline: bool,
    },
    Doc {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(short, long, default_value = "docs/api")]
        output: PathBuf,
        #[arg(long)]
        offline: bool,
    },
    StdCompat {
        #[arg(long)]
        check: bool,
        #[arg(long, default_value = "docs/std-api-baseline.json")]
        baseline: PathBuf,
    },
    Lsp,
    Test {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum, default_value = "all")]
        mode: TestModeArg,
        #[arg(long)]
        json: bool,
    },
    Contract {
        #[arg(long)]
        json: bool,
    },
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
    },
    Run {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long)]
        offline: bool,
        #[arg(long, value_enum, default_value = "none")]
        sandbox: SandboxProfileArg,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EmitKind {
    Json,
    Text,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BuildArtifact {
    Exe,
    Obj,
    Lib,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TestModeArg {
    All,
    RunPass,
    CompileFail,
    Golden,
}

#[derive(Debug, Clone, Subcommand)]
enum ReleaseCommand {
    Manifest {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(short, long, default_value = "target/release/repro-manifest.json")]
        output: PathBuf,
        #[arg(long)]
        source_date_epoch: Option<u64>,
    },
    VerifyManifest {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value = "target/release/repro-manifest.json")]
        manifest: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Sbom {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(short, long, default_value = "target/release/sbom.json")]
        output: PathBuf,
        #[arg(long)]
        source_date_epoch: Option<u64>,
    },
    Provenance {
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        sbom: PathBuf,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(short, long, default_value = "target/release/provenance.json")]
        output: PathBuf,
        #[arg(long, default_value = "AIC_SIGNING_KEY")]
        key_env: String,
        #[arg(long)]
        key_id: Option<String>,
    },
    VerifyProvenance {
        #[arg(long)]
        provenance: PathBuf,
        #[arg(long, default_value = "AIC_SIGNING_KEY")]
        key_env: String,
        #[arg(long)]
        json: bool,
    },
    SecurityAudit {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Policy {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        check: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SandboxProfileArg {
    None,
    Ci,
    Strict,
}

impl TestModeArg {
    fn to_harness_mode(self) -> HarnessMode {
        match self {
            TestModeArg::All => HarnessMode::All,
            TestModeArg::RunPass => HarnessMode::RunPass,
            TestModeArg::CompileFail => HarnessMode::CompileFail,
            TestModeArg::Golden => HarnessMode::Golden,
        }
    }
}

impl SandboxProfileArg {
    fn to_profile(self) -> SandboxProfile {
        match self {
            SandboxProfileArg::None => SandboxProfile::None,
            SandboxProfileArg::Ci => SandboxProfile::Ci,
            SandboxProfileArg::Strict => SandboxProfile::Strict,
        }
    }
}

fn main() {
    let exit = match run_cli() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("internal error: {err:#}");
            EXIT_INTERNAL_ERROR
        }
    };
    std::process::exit(exit);
}

fn run_cli() -> anyhow::Result<i32> {
    let cli = Cli::parse();

    let exit = match cli.command {
        Command::Init { path } => {
            init_project(&path)?;
            println!("initialized AICore project at {}", path.display());
            EXIT_OK
        }
        Command::Check {
            input,
            json,
            sarif,
            offline,
        }
        | Command::Diag {
            input,
            json,
            sarif,
            offline,
        } => {
            let front = run_frontend_with_options(&input, FrontendOptions { offline })?;
            if sarif {
                let sarif =
                    diagnostics_to_sarif(&front.diagnostics, "aic", env!("CARGO_PKG_VERSION"));
                println!("{}", serde_json::to_string_pretty(&sarif)?);
            } else if json {
                println!("{}", serde_json::to_string_pretty(&front.diagnostics)?);
            } else if front.diagnostics.is_empty() {
                println!("check: ok");
            } else {
                print!("{}", diagnostics_pretty(&front.diagnostics));
            }

            if has_errors(&front.diagnostics) {
                EXIT_DIAGNOSTIC_ERROR
            } else {
                EXIT_OK
            }
        }
        Command::Explain { code, json } => {
            let entry = explain(&code);
            if json {
                println!("{}", serde_json::to_string_pretty(&entry)?);
            } else {
                print!("{}", explain_text(&entry));
            }
            if entry.known {
                EXIT_OK
            } else {
                EXIT_DIAGNOSTIC_ERROR
            }
        }
        Command::Fmt { input, check } => {
            let source = std::fs::read_to_string(&input)?;
            let (ast, diags) = parser::parse(&source, &input.to_string_lossy());
            if diags.iter().any(|d| matches!(d.severity, Severity::Error)) {
                print!("{}", diagnostics_pretty(&diags));
                EXIT_DIAGNOSTIC_ERROR
            } else {
                let Some(ast) = ast else {
                    anyhow::bail!("format failed: parser returned no AST");
                };
                let ir = ir_builder::build(&ast);
                let formatted = format_program(&ir);
                if check {
                    let current = std::fs::read_to_string(&input)?;
                    if current != formatted {
                        eprintln!("format check failed: {}", input.display());
                        EXIT_DIAGNOSTIC_ERROR
                    } else {
                        EXIT_OK
                    }
                } else {
                    std::fs::write(&input, formatted)?;
                    EXIT_OK
                }
            }
        }
        Command::Ir {
            input,
            emit,
            offline,
        } => {
            let front = run_frontend_with_options(&input, FrontendOptions { offline })?;
            if has_errors(&front.diagnostics) {
                print!("{}", diagnostics_pretty(&front.diagnostics));
                EXIT_DIAGNOSTIC_ERROR
            } else {
                match emit {
                    EmitKind::Json => println!("{}", serde_json::to_string_pretty(&front.ir)?),
                    EmitKind::Text => println!("{}", format_program(&front.ir)),
                }
                EXIT_OK
            }
        }
        Command::IrMigrate { input } => {
            let raw = std::fs::read_to_string(&input)?;
            let migrated = migrate_json_to_current(&raw)?;
            println!("{}", serde_json::to_string_pretty(&migrated)?);
            EXIT_OK
        }
        Command::Lock { path } => {
            let root = resolve_project_root(&path);
            let lock_path = generate_and_write_lockfile(&root)?;
            println!("generated {}", lock_path.display());
            EXIT_OK
        }
        Command::Build {
            input,
            output,
            artifact,
            debug_info,
            offline,
        } => {
            let front = run_frontend_with_options(&input, FrontendOptions { offline })?;
            if has_errors(&front.diagnostics) {
                print!("{}", diagnostics_pretty(&front.diagnostics));
                EXIT_DIAGNOSTIC_ERROR
            } else {
                let lowered = lower_runtime_asserts(&front.ir);
                let llvm = match emit_llvm_with_options(
                    &lowered,
                    &input.to_string_lossy(),
                    CodegenOptions { debug_info },
                ) {
                    Ok(v) => v,
                    Err(diags) => {
                        print!("{}", diagnostics_pretty(&diags));
                        return Ok(EXIT_DIAGNOSTIC_ERROR);
                    }
                };

                let out = output.unwrap_or_else(|| default_build_output_name(&input, artifact));
                let work = std::env::temp_dir().join("aicore_build");
                compile_with_clang_artifact_with_options(
                    &llvm.llvm_ir,
                    &out,
                    &work,
                    artifact.to_codegen(),
                    CompileOptions { debug_info },
                )?;
                println!("built {}", out.display());
                EXIT_OK
            }
        }
        Command::Doc {
            input,
            output,
            offline,
        } => {
            let front = run_frontend_with_options(&input, FrontendOptions { offline })?;
            if has_errors(&front.diagnostics) {
                print!("{}", diagnostics_pretty(&front.diagnostics));
                EXIT_DIAGNOSTIC_ERROR
            } else {
                let out = generate_docs(&front, &output)?;
                println!("generated {}", out.index_path.display());
                EXIT_OK
            }
        }
        Command::StdCompat { check, baseline } => {
            let current = collect_std_api_snapshot(&default_std_root())?;
            if check {
                let baseline_text = std::fs::read_to_string(&baseline)?;
                let baseline_snapshot = serde_json::from_str::<StdApiSnapshot>(&baseline_text)?;
                let report = compare_snapshots(&current, &baseline_snapshot);
                if !report.breaking.is_empty() {
                    eprintln!("error[E6002]: std compatibility check failed");
                    for item in &report.breaking {
                        eprintln!(
                            "  removed or changed: {} {} {}",
                            item.module, item.kind, item.signature
                        );
                    }
                    eprintln!("  help: keep APIs compatible or deprecate first before removal");
                    EXIT_DIAGNOSTIC_ERROR
                } else {
                    println!(
                        "std compatibility check: ok ({} additions)",
                        report.additions.len()
                    );
                    EXIT_OK
                }
            } else {
                println!("{}", serde_json::to_string_pretty(&current)?);
                EXIT_OK
            }
        }
        Command::Lsp => {
            lsp::run_stdio()?;
            EXIT_OK
        }
        Command::Test { path, mode, json } => {
            let report = run_harness(&path, mode.to_harness_mode())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_harness_report(&report);
            }
            if report.failed > 0 {
                EXIT_DIAGNOSTIC_ERROR
            } else {
                EXIT_OK
            }
        }
        Command::Contract { json } => {
            let contract = contract_json();
            if json {
                println!("{}", serde_json::to_string_pretty(&contract)?);
            } else {
                println!(
                    "AICore CLI contract v{}",
                    contract["version"].as_str().unwrap_or("1.0")
                );
                println!("exit codes:");
                println!("  {} -> success", EXIT_OK);
                println!("  {} -> diagnostics/runtime failure", EXIT_DIAGNOSTIC_ERROR);
                println!("  {} -> command-line usage error", EXIT_USAGE_ERROR);
                println!("  {} -> internal/tooling failure", EXIT_INTERNAL_ERROR);
            }
            EXIT_OK
        }
        Command::Release { command } => match command {
            ReleaseCommand::Manifest {
                root,
                output,
                source_date_epoch,
            } => {
                let epoch = effective_source_date_epoch(source_date_epoch);
                let manifest = generate_repro_manifest(&root, epoch)?;
                write_repro_manifest(&output, &manifest)?;
                println!(
                    "generated reproducibility manifest {} (digest={})",
                    output.display(),
                    manifest.digest
                );
                EXIT_OK
            }
            ReleaseCommand::VerifyManifest {
                root,
                manifest,
                json,
            } => {
                let expected = read_repro_manifest(&manifest)?;
                let mismatches = verify_repro_manifest(&root, &expected)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&mismatches)?);
                } else if mismatches.is_empty() {
                    println!("reproducibility manifest verification: ok");
                } else {
                    eprintln!("reproducibility manifest verification failed:");
                    for mismatch in &mismatches {
                        eprintln!("  - {}", mismatch);
                    }
                }
                if mismatches.is_empty() {
                    EXIT_OK
                } else {
                    EXIT_DIAGNOSTIC_ERROR
                }
            }
            ReleaseCommand::Sbom {
                root,
                output,
                source_date_epoch,
            } => {
                let epoch = effective_source_date_epoch(source_date_epoch);
                let sbom = generate_sbom(&root, epoch)?;
                write_sbom(&output, &sbom)?;
                println!(
                    "generated SBOM {} (digest={})",
                    output.display(),
                    sbom.digest
                );
                EXIT_OK
            }
            ReleaseCommand::Provenance {
                artifact,
                sbom,
                manifest,
                output,
                key_env,
                key_id,
            } => {
                let key = read_signing_key(&key_env)?;
                let provenance =
                    generate_provenance(&artifact, &sbom, manifest.as_deref(), &key, key_id)?;
                write_provenance(&output, &provenance)?;
                println!(
                    "generated provenance {} (artifact_sha256={})",
                    output.display(),
                    provenance.artifact_sha256
                );
                EXIT_OK
            }
            ReleaseCommand::VerifyProvenance {
                provenance,
                key_env,
                json,
            } => {
                let statement = read_provenance(&provenance)?;
                let key = read_signing_key(&key_env)?;
                let errors = verify_provenance(&statement, &key)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&errors)?);
                } else if errors.is_empty() {
                    println!("provenance verification: ok");
                } else {
                    eprintln!("provenance verification failed:");
                    for error in &errors {
                        eprintln!("  - {}", error);
                    }
                }
                if errors.is_empty() {
                    EXIT_OK
                } else {
                    EXIT_DIAGNOSTIC_ERROR
                }
            }
            ReleaseCommand::SecurityAudit { root, json } => {
                let report = run_security_audit(&root)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else if report.ok {
                    println!("security audit: ok");
                } else {
                    eprintln!("security audit failed:");
                    for issue in &report.issues {
                        eprintln!("  - {}", issue);
                    }
                }
                if report.ok {
                    EXIT_OK
                } else {
                    EXIT_DIAGNOSTIC_ERROR
                }
            }
            ReleaseCommand::Policy { root, json, check } => {
                let policy = compatibility_policy();
                let problems = if check {
                    check_compatibility_policy(&root, &policy)
                } else {
                    Vec::new()
                };

                if json {
                    let payload = serde_json::json!({
                        "policy": policy,
                        "problems": problems,
                    });
                    println!("{}", serde_json::to_string_pretty(&payload)?);
                } else if check {
                    if problems.is_empty() {
                        println!("compatibility policy check: ok");
                    } else {
                        eprintln!("compatibility policy check failed:");
                        for problem in &problems {
                            eprintln!("  - {}", problem);
                        }
                    }
                } else {
                    println!("{}", serde_json::to_string_pretty(&policy)?);
                }

                if problems.is_empty() {
                    EXIT_OK
                } else {
                    EXIT_DIAGNOSTIC_ERROR
                }
            }
        },
        Command::Run {
            input,
            offline,
            sandbox,
        } => {
            let out = std::env::temp_dir().join("aicore_run_bin");
            let build_code = build_file(&input, &out, offline)?;
            if build_code != EXIT_OK {
                build_code
            } else {
                if !cfg!(target_os = "linux") && !matches!(sandbox, SandboxProfileArg::None) {
                    eprintln!(
                        "sandbox profile '{}' requires Linux `prlimit`; use --sandbox none",
                        sandbox.to_profile().as_str()
                    );
                    return Ok(EXIT_USAGE_ERROR);
                }

                let profile = sandbox.to_profile();
                let limits = profile.limits();
                let run_args: Vec<String> = Vec::new();
                let status = run_with_limits(&out, &run_args, limits.as_ref())?;
                if status.success() {
                    EXIT_OK
                } else {
                    EXIT_DIAGNOSTIC_ERROR
                }
            }
        }
    };

    Ok(exit)
}

fn print_harness_report(report: &aicore::test_harness::HarnessReport) {
    println!(
        "aic test: total={} passed={} failed={}",
        report.total, report.passed, report.failed
    );
    if !report.by_category.is_empty() {
        println!("categories:");
        for (category, count) in &report.by_category {
            println!("- {}: {}", category, count);
        }
    }
    for case in &report.cases {
        let status = if case.passed { "ok" } else { "fail" };
        println!(
            "{} [{}] {} -> {}",
            status, case.category, case.file, case.details
        );
    }
}

fn build_file(input: &Path, output: &Path, offline: bool) -> anyhow::Result<i32> {
    let front = run_frontend_with_options(input, FrontendOptions { offline })?;
    if has_errors(&front.diagnostics) {
        print!("{}", diagnostics_pretty(&front.diagnostics));
        return Ok(EXIT_DIAGNOSTIC_ERROR);
    }

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = match emit_llvm(&lowered, &input.to_string_lossy()) {
        Ok(v) => v,
        Err(diags) => {
            print!("{}", diagnostics_pretty(&diags));
            return Ok(EXIT_DIAGNOSTIC_ERROR);
        }
    };

    let work = std::env::temp_dir().join("aicore_build");
    compile_with_clang(&llvm.llvm_ir, output, &work)?;
    Ok(EXIT_OK)
}

fn default_build_output_name(input: &Path, artifact: BuildArtifact) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("a.out");
    match artifact {
        BuildArtifact::Exe => PathBuf::from(stem),
        BuildArtifact::Obj => PathBuf::from(format!("{stem}.o")),
        BuildArtifact::Lib => PathBuf::from(format!("lib{stem}.a")),
    }
}

impl BuildArtifact {
    fn to_codegen(self) -> ArtifactKind {
        match self {
            BuildArtifact::Exe => ArtifactKind::Exe,
            BuildArtifact::Obj => ArtifactKind::Obj,
            BuildArtifact::Lib => ArtifactKind::Lib,
        }
    }
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

fn read_signing_key(env_name: &str) -> anyhow::Result<String> {
    let key = std::env::var(env_name).map_err(|_| {
        anyhow::anyhow!(
            "missing signing key in environment variable `{}`; set it before invoking release provenance commands",
            env_name
        )
    })?;
    if key.trim().is_empty() {
        anyhow::bail!("environment variable `{}` is empty", env_name);
    }
    Ok(key)
}
