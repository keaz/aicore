use std::path::{Path, PathBuf};

use aicore::codegen::{
    compile_with_clang, compile_with_clang_artifact_with_options, emit_llvm,
    emit_llvm_with_options, ArtifactKind, CodegenOptions, CompileOptions,
};
use aicore::contracts::lower_runtime_asserts;
use aicore::diagnostics::Severity;
use aicore::docgen::generate_docs;
use aicore::driver::{diagnostics_pretty, has_errors, run_frontend_with_options, FrontendOptions};
use aicore::formatter::format_program;
use aicore::ir::migrate_json_to_current;
use aicore::ir_builder;
use aicore::package_workflow::generate_and_write_lockfile;
use aicore::parser;
use aicore::project::init_project;
use aicore::std_policy::{
    collect_std_api_snapshot, compare_snapshots, default_std_root, StdApiSnapshot,
};
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
        #[arg(long)]
        json: bool,
        #[arg(long)]
        offline: bool,
    },
    Diag {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        offline: bool,
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
    Run {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long)]
        offline: bool,
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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { path } => {
            init_project(&path)?;
            println!("initialized AICore project at {}", path.display());
        }
        Command::Check {
            input,
            json,
            offline,
        }
        | Command::Diag {
            input,
            json,
            offline,
        } => {
            let front = run_frontend_with_options(&input, FrontendOptions { offline })?;
            if json {
                println!("{}", serde_json::to_string_pretty(&front.diagnostics)?);
            } else if front.diagnostics.is_empty() {
                println!("check: ok");
            } else {
                print!("{}", diagnostics_pretty(&front.diagnostics));
            }
            if has_errors(&front.diagnostics) {
                std::process::exit(1);
            }
        }
        Command::Fmt { input, check } => {
            let source = std::fs::read_to_string(&input)?;
            let (ast, diags) = parser::parse(&source, &input.to_string_lossy());
            if diags.iter().any(|d| matches!(d.severity, Severity::Error)) {
                print!("{}", diagnostics_pretty(&diags));
                std::process::exit(1);
            }
            let Some(ast) = ast else {
                eprintln!("format failed: parser returned no AST");
                std::process::exit(1);
            };
            let ir = ir_builder::build(&ast);
            let formatted = format_program(&ir);
            if check {
                let current = std::fs::read_to_string(&input)?;
                if current != formatted {
                    eprintln!("format check failed: {}", input.display());
                    std::process::exit(1);
                }
            } else {
                std::fs::write(&input, formatted)?;
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
                std::process::exit(1);
            }
            match emit {
                EmitKind::Json => {
                    println!("{}", serde_json::to_string_pretty(&front.ir)?);
                }
                EmitKind::Text => {
                    println!("{}", format_program(&front.ir));
                }
            }
        }
        Command::IrMigrate { input } => {
            let raw = std::fs::read_to_string(&input)?;
            let migrated = migrate_json_to_current(&raw)?;
            println!("{}", serde_json::to_string_pretty(&migrated)?);
        }
        Command::Lock { path } => {
            let root = resolve_project_root(&path);
            let lock_path = generate_and_write_lockfile(&root)?;
            println!("generated {}", lock_path.display());
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
                std::process::exit(1);
            }

            let lowered = lower_runtime_asserts(&front.ir);
            let llvm = match emit_llvm_with_options(
                &lowered,
                &input.to_string_lossy(),
                CodegenOptions { debug_info },
            ) {
                Ok(v) => v,
                Err(diags) => {
                    if let Ok(text) = serde_json::to_string_pretty(&diags) {
                        eprintln!("{}", text);
                    }
                    std::process::exit(1);
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
        }
        Command::Doc {
            input,
            output,
            offline,
        } => {
            let front = run_frontend_with_options(&input, FrontendOptions { offline })?;
            if has_errors(&front.diagnostics) {
                print!("{}", diagnostics_pretty(&front.diagnostics));
                std::process::exit(1);
            }
            let out = generate_docs(&front, &output)?;
            println!("generated {}", out.index_path.display());
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
                    std::process::exit(1);
                }
                println!(
                    "std compatibility check: ok ({} additions)",
                    report.additions.len()
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&current)?);
            }
        }
        Command::Run { input, offline } => {
            let out = std::env::temp_dir().join("aicore_run_bin");
            build_file(&input, &out, offline)?;
            let status = std::process::Command::new(&out).status()?;
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
    }

    Ok(())
}

fn build_file(input: &Path, output: &Path, offline: bool) -> anyhow::Result<()> {
    let front = run_frontend_with_options(input, FrontendOptions { offline })?;
    if has_errors(&front.diagnostics) {
        print!("{}", diagnostics_pretty(&front.diagnostics));
        anyhow::bail!("build failed")
    }

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &input.to_string_lossy())
        .map_err(|_| anyhow::anyhow!("llvm generation failed"))?;

    let work = std::env::temp_dir().join("aicore_build");
    compile_with_clang(&llvm.llvm_ir, output, &work)?;
    Ok(())
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
