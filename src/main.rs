use std::path::{Path, PathBuf};

use aicore::codegen::{
    compile_with_clang, compile_with_clang_artifact_with_options, emit_llvm,
    emit_llvm_with_options, ArtifactKind, CodegenOptions, CompileOptions,
};
use aicore::contracts::lower_runtime_asserts;
use aicore::diagnostics::Severity;
use aicore::driver::{diagnostics_pretty, has_errors, run_frontend};
use aicore::formatter::format_program;
use aicore::ir::migrate_json_to_current;
use aicore::ir_builder;
use aicore::parser;
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
    IrMigrate {
        #[arg(default_value = "ir.json")]
        input: PathBuf,
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
        Command::IrMigrate { input } => {
            let raw = std::fs::read_to_string(&input)?;
            let migrated = migrate_json_to_current(&raw)?;
            println!("{}", serde_json::to_string_pretty(&migrated)?);
        }
        Command::Build {
            input,
            output,
            artifact,
            debug_info,
        } => {
            let front = run_frontend(&input)?;
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
