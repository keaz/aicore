use std::path::{Path, PathBuf};

use aicore::codegen::{compile_with_clang, emit_llvm};
use aicore::contracts::lower_runtime_asserts;
use aicore::driver::{diagnostics_pretty, has_errors, run_frontend};
use aicore::formatter::format_program;
use aicore::project::init_project;
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
    },
    Diag {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
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
    },
    Build {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    Run {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EmitKind {
    Json,
    Text,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { path } => {
            init_project(&path)?;
            println!("initialized AICore project at {}", path.display());
        }
        Command::Check { input, json } | Command::Diag { input, json } => {
            let front = run_frontend(&input)?;
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
            let front = run_frontend(&input)?;
            if has_errors(&front.diagnostics) {
                print!("{}", diagnostics_pretty(&front.diagnostics));
                std::process::exit(1);
            }
            let formatted = format_program(&front.ir);
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
        Command::Ir { input, emit } => {
            let front = run_frontend(&input)?;
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
        Command::Build { input, output } => {
            let front = run_frontend(&input)?;
            if has_errors(&front.diagnostics) {
                print!("{}", diagnostics_pretty(&front.diagnostics));
                std::process::exit(1);
            }

            let lowered = lower_runtime_asserts(&front.ir);
            let llvm = match emit_llvm(&lowered, &input.to_string_lossy()) {
                Ok(v) => v,
                Err(diags) => {
                    if let Ok(text) = serde_json::to_string_pretty(&diags) {
                        eprintln!("{}", text);
                    }
                    std::process::exit(1);
                }
            };

            let out = output.unwrap_or_else(|| default_binary_name(&input));
            let work = std::env::temp_dir().join("aicore_build");
            compile_with_clang(&llvm.llvm_ir, &out, &work)?;
            println!("built {}", out.display());
        }
        Command::Run { input } => {
            let out = std::env::temp_dir().join("aicore_run_bin");
            build_file(&input, &out)?;
            let status = std::process::Command::new(&out).status()?;
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
    }

    Ok(())
}

fn build_file(input: &Path, output: &Path) -> anyhow::Result<()> {
    let front = run_frontend(input)?;
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

fn default_binary_name(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("a.out");
    PathBuf::from(stem)
}
