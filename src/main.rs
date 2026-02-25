use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

mod coverage;
mod profile;

use aicore::cli_contract::{
    contract_json, EXIT_DIAGNOSTIC_ERROR, EXIT_INTERNAL_ERROR, EXIT_OK, EXIT_USAGE_ERROR,
};
use aicore::codegen::{
    compile_with_clang_artifact_with_options, compile_with_clang_artifact_with_options_and_runtime,
    emit_llvm, emit_llvm_with_options, ArtifactKind, CodegenOptions, CompileOptions, LinkOptions,
    RuntimeInstrumentationOptions,
};
use aicore::contracts::lower_runtime_asserts;
use aicore::daemon;
use aicore::diag_fixes::apply_safe_fixes;
use aicore::diagnostic_explain::{explain, explain_text};
use aicore::diagnostics::{Diagnostic, Severity};
use aicore::docgen::generate_docs;
use aicore::driver::{
    diagnostics_pretty, has_errors, run_frontend_with_options, sort_and_cap_diagnostics,
    sort_diagnostics, FrontendOptions, FrontendOutput,
};
use aicore::formatter::format_program;
use aicore::ir::migrate_json_to_current;
use aicore::ir_builder;
use aicore::lsp;
use aicore::metrics::{self, MetricsThresholdOverrides};
use aicore::migration::{run_migration, write_report as write_migration_report};
use aicore::package_registry::{
    install_with_options as pkg_install_with_options,
    publish_with_options as pkg_publish_with_options,
    search_with_options as pkg_search_with_options, RegistryClientOptions,
};
use aicore::package_workflow::{
    compute_package_checksum_for_path, generate_and_write_lockfile, metrics_thresholds_for_input,
    native_link_config, workspace_build_plan, NativeLinkConfig,
};
use aicore::parser;
use aicore::perf_gate::{
    build_trend_report, host_target_label, load_budget, load_compare_baseline, run_perf_gate,
};
use aicore::project::init_project;
use aicore::release_ops::{
    check_compatibility_policy, check_lts_policy, compatibility_policy,
    effective_source_date_epoch, generate_provenance, generate_repro_manifest, generate_sbom,
    lts_policy, read_provenance, read_repro_manifest, run_security_audit, verify_checksum_file,
    verify_provenance, verify_repro_manifest, write_provenance, write_repro_manifest, write_sbom,
};
use aicore::sandbox::{load_policy as load_sandbox_policy, run_with_policy, SandboxProfile};
use aicore::sarif::diagnostics_to_sarif;
use aicore::semantic_diff;
use aicore::std_policy::{
    collect_std_api_snapshot, compare_snapshots, default_std_root, StdApiSnapshot,
};
use aicore::telemetry;
use aicore::test_harness::{run_harness_with_golden_mode, GoldenMode, HarnessMode};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

const DEFAULT_MAX_ERRORS: usize = 20;
const GRAMMAR_VERSION: &str = "mvp-grammar-v6";
const GRAMMAR_FORMAT: &str = "ebnf";
const GRAMMAR_SOURCE_PATH: &str = "docs/grammar.ebnf";
const GRAMMAR_SOURCE_CONTRACT_PATH: &str = "docs/syntax.md";
const GRAMMAR_EBNF: &str = include_str!("../docs/grammar.ebnf");
const AST_RESPONSE_VERSION: &str = "1.0";

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
        #[arg(long, conflicts_with_all = ["json", "sarif"])]
        show_holes: bool,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        warn_unused: bool,
        #[arg(
            long,
            value_name = "N",
            default_value_t = DEFAULT_MAX_ERRORS,
            value_parser = parse_max_errors
        )]
        max_errors: usize,
    },
    Ast {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long, required = true)]
        json: bool,
        #[arg(long)]
        offline: bool,
    },
    Impact {
        function: String,
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long)]
        offline: bool,
    },
    SuggestEffects {
        input: PathBuf,
        #[arg(long)]
        offline: bool,
    },
    SuggestContracts {
        input: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        offline: bool,
    },
    Coverage {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long)]
        check: bool,
        #[arg(
            long,
            value_name = "PCT",
            default_value_t = 0.0,
            value_parser = parse_coverage_percent
        )]
        min: f64,
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(long)]
        offline: bool,
    },
    Metrics {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long)]
        check: bool,
        #[arg(long, value_name = "N", value_parser = parse_positive_u32)]
        max_cyclomatic: Option<u32>,
    },
    Bench {
        #[arg(long, default_value = "benchmarks/service_baseline/budget.v1.json")]
        budget: PathBuf,
        #[arg(short, long, default_value = "bench.json")]
        output: PathBuf,
        #[arg(long, value_name = "BASELINE_JSON")]
        compare: Option<PathBuf>,
    },
    Diag {
        #[command(subcommand)]
        command: Option<DiagSubcommand>,
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long, conflicts_with = "sarif")]
        json: bool,
        #[arg(long, conflicts_with = "json")]
        sarif: bool,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        warn_unused: bool,
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
    Migrate {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    Lock {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Pkg {
        #[command(subcommand)]
        command: PkgCommand,
    },
    Build {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "exe")]
        artifact: BuildArtifact,
        #[arg(long, value_enum)]
        target: Option<BuildTarget>,
        #[arg(long)]
        static_link: bool,
        #[arg(long)]
        debug_info: bool,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        verify_hash: Option<String>,
        #[arg(long)]
        manifest: Option<PathBuf>,
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
    Diff {
        #[arg(long, required = true)]
        semantic: bool,
        #[arg(long)]
        fail_on_breaking: bool,
        old_file: PathBuf,
        new_file: PathBuf,
    },
    Lsp,
    Daemon,
    Repl {
        #[arg(long)]
        json: bool,
    },
    Test {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum, default_value = "all")]
        mode: TestModeArg,
        #[arg(long)]
        json: bool,
        #[arg(long, conflicts_with = "check_golden")]
        update_golden: bool,
        #[arg(long, conflicts_with = "update_golden")]
        check_golden: bool,
    },
    Contract {
        #[arg(long)]
        json: bool,
        #[arg(long = "accept-version", value_delimiter = ',')]
        accept_versions: Vec<String>,
    },
    Grammar {
        #[arg(long, conflicts_with = "json", required_unless_present = "json")]
        ebnf: bool,
        #[arg(long, conflicts_with = "ebnf", required_unless_present = "ebnf")]
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
        #[arg(long)]
        sandbox_config: Option<PathBuf>,
        #[arg(long)]
        profile: bool,
        #[arg(long, default_value = "profile.json", requires = "profile")]
        profile_output: PathBuf,
        #[arg(long)]
        check_leaks: bool,
        #[arg(long)]
        asan: bool,
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EmitKind {
    Json,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BuildArtifact {
    Exe,
    Obj,
    Lib,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BuildTarget {
    #[value(name = "x86_64-linux")]
    X8664Linux,
    #[value(name = "aarch64-linux")]
    Aarch64Linux,
    #[value(name = "x86_64-macos")]
    X8664Macos,
    #[value(name = "aarch64-macos")]
    Aarch64Macos,
    #[value(name = "x86_64-windows")]
    X8664Windows,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TestModeArg {
    All,
    RunPass,
    CompileFail,
    Golden,
}

#[derive(Debug, Clone, Subcommand)]
enum DiagSubcommand {
    #[command(name = "apply-fixes")]
    ApplyFixes {
        #[arg(default_value = "src/main.aic")]
        input: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        warn_unused: bool,
    },
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
    VerifyChecksum {
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        checksum: PathBuf,
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
    Lts {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        check: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
enum PkgCommand {
    Publish {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        registry: Option<String>,
        #[arg(long)]
        registry_config: Option<PathBuf>,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Search {
        query: Option<String>,
        #[arg(long)]
        registry: Option<String>,
        #[arg(long)]
        registry_config: Option<PathBuf>,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Install {
        #[arg(required = true)]
        specs: Vec<String>,
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        registry: Option<String>,
        #[arg(long)]
        registry_config: Option<PathBuf>,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        json: bool,
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

fn parse_max_errors(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("invalid value `{value}` for --max-errors"))?;
    if parsed == 0 {
        Err("--max-errors must be greater than 0".to_string())
    } else {
        Ok(parsed)
    }
}

fn parse_coverage_percent(value: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("invalid value `{value}` for --min"))?;
    if !parsed.is_finite() {
        return Err("--min must be a finite number".to_string());
    }
    if !(0.0..=100.0).contains(&parsed) {
        return Err("--min must be within [0, 100]".to_string());
    }
    Ok(parsed)
}

fn parse_positive_u32(value: &str) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("invalid value `{value}`"))?;
    if parsed == 0 {
        Err("value must be greater than 0".to_string())
    } else {
        Ok(parsed)
    }
}

fn env_flag_enabled(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    !matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no"
    )
}

fn grammar_contract_json() -> serde_json::Value {
    serde_json::json!({
        "version": GRAMMAR_VERSION,
        "format": GRAMMAR_FORMAT,
        "grammar": GRAMMAR_EBNF,
        "source_path": GRAMMAR_SOURCE_PATH,
        "source_contract_path": GRAMMAR_SOURCE_CONTRACT_PATH
    })
}

#[derive(Debug, Clone, Serialize)]
struct AstJsonResponse {
    version: &'static str,
    module: Option<String>,
    ast: aicore::ast::Program,
    ir: aicore::ir::Program,
    resolved_types: Vec<ResolvedTypeEntry>,
    generic_instantiations: Vec<aicore::ir::GenericInstantiation>,
    function_effects: BTreeMap<String, Vec<String>>,
    contracts: AstContracts,
    import_graph: AstImportGraph,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize)]
struct ResolvedTypeEntry {
    id: u32,
    repr: String,
}

#[derive(Debug, Clone, Serialize)]
struct AstContracts {
    functions: Vec<AstFunctionContract>,
}

#[derive(Debug, Clone, Serialize)]
struct AstFunctionContract {
    item_index: usize,
    method_index: Option<usize>,
    module: Option<String>,
    function: String,
    function_span: aicore::span::Span,
    requires: Option<AstContractClause>,
    ensures: Option<AstContractClause>,
}

#[derive(Debug, Clone, Serialize)]
struct AstContractClause {
    span: aicore::span::Span,
    expr: aicore::ast::Expr,
}

#[derive(Debug, Clone, Serialize)]
struct AstImportGraph {
    entry_module: Option<String>,
    imports: Vec<String>,
    item_modules: Vec<Option<String>>,
    nodes: Vec<String>,
    edges: Vec<AstImportEdge>,
}

#[derive(Debug, Clone, Serialize)]
struct AstImportEdge {
    from: String,
    to: String,
}

#[derive(Debug, Clone, Serialize)]
struct CheckHolesResponse {
    holes: Vec<CheckHoleEntry>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct CheckHoleEntry {
    line: usize,
    inferred: String,
    context: String,
}

fn build_ast_response(front: FrontendOutput) -> AstJsonResponse {
    let FrontendOutput {
        ast,
        ir,
        resolution,
        typecheck,
        diagnostics,
        item_modules,
        ..
    } = front;

    let module = ir.module.as_ref().map(|path| path.join("."));

    let mut resolved_types = ir
        .types
        .iter()
        .map(|ty| ResolvedTypeEntry {
            id: ty.id.0,
            repr: ty.repr.clone(),
        })
        .collect::<Vec<_>>();
    resolved_types.sort_by_key(|entry| entry.id);

    let aicore::typecheck::TypecheckOutput {
        function_effect_usage,
        mut generic_instantiations,
        ..
    } = typecheck;

    generic_instantiations.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.mangled.cmp(&right.mangled))
    });

    let function_effects = function_effect_usage
        .into_iter()
        .map(|(function, effects)| (function, effects.into_iter().collect::<Vec<_>>()))
        .collect::<BTreeMap<_, _>>();

    let contracts = collect_contracts(&ast, &item_modules);
    let import_graph = collect_import_graph(&resolution, &item_modules);

    AstJsonResponse {
        version: AST_RESPONSE_VERSION,
        module,
        ast,
        ir,
        resolved_types,
        generic_instantiations,
        function_effects,
        contracts,
        import_graph,
        diagnostics,
    }
}

fn collect_contracts(
    ast: &aicore::ast::Program,
    item_modules: &[Option<Vec<String>>],
) -> AstContracts {
    let mut functions = Vec::new();
    for (item_index, item) in ast.items.iter().enumerate() {
        let module = item_modules
            .get(item_index)
            .and_then(|entry| entry.as_ref().map(|segments| segments.join(".")));
        match item {
            aicore::ast::Item::Function(function) => {
                push_function_contract(
                    &mut functions,
                    item_index,
                    None,
                    &module,
                    function.name.clone(),
                    function,
                );
            }
            aicore::ast::Item::Trait(trait_def) => {
                for (method_index, method) in trait_def.methods.iter().enumerate() {
                    push_function_contract(
                        &mut functions,
                        item_index,
                        Some(method_index),
                        &module,
                        format!("{}::{}", trait_def.name, method.name),
                        method,
                    );
                }
            }
            aicore::ast::Item::Impl(impl_def) => {
                for (method_index, method) in impl_def.methods.iter().enumerate() {
                    push_function_contract(
                        &mut functions,
                        item_index,
                        Some(method_index),
                        &module,
                        format!("{}::{}", impl_def.trait_name, method.name),
                        method,
                    );
                }
            }
            aicore::ast::Item::Struct(_) | aicore::ast::Item::Enum(_) => {}
        }
    }
    AstContracts { functions }
}

fn push_function_contract(
    functions: &mut Vec<AstFunctionContract>,
    item_index: usize,
    method_index: Option<usize>,
    module: &Option<String>,
    function_name: String,
    function: &aicore::ast::Function,
) {
    if function.requires.is_none() && function.ensures.is_none() {
        return;
    }

    functions.push(AstFunctionContract {
        item_index,
        method_index,
        module: module.clone(),
        function: function_name,
        function_span: function.span,
        requires: function.requires.as_ref().map(|expr| AstContractClause {
            span: expr.span,
            expr: expr.clone(),
        }),
        ensures: function.ensures.as_ref().map(|expr| AstContractClause {
            span: expr.span,
            expr: expr.clone(),
        }),
    });
}

fn collect_import_graph(
    resolution: &aicore::resolver::Resolution,
    item_modules: &[Option<Vec<String>>],
) -> AstImportGraph {
    let entry_module = resolution.entry_module.clone();
    let imports = resolution.imports.iter().cloned().collect::<Vec<_>>();
    let item_modules = item_modules
        .iter()
        .map(|module| module.as_ref().map(|segments| segments.join(".")))
        .collect::<Vec<_>>();

    let edge_from = entry_module.clone().unwrap_or_else(|| "<root>".to_string());
    let edges = imports
        .iter()
        .map(|to| AstImportEdge {
            from: edge_from.clone(),
            to: to.clone(),
        })
        .collect::<Vec<_>>();

    let mut nodes = BTreeSet::new();
    nodes.insert(edge_from);
    for import in &imports {
        nodes.insert(import.clone());
    }
    for module in item_modules.iter().flatten() {
        nodes.insert(module.clone());
    }

    AstImportGraph {
        entry_module,
        imports,
        item_modules,
        nodes: nodes.into_iter().collect(),
        edges,
    }
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
            show_holes,
            offline,
            warn_unused,
            max_errors,
        } => {
            let (diagnostics, holes) = collect_check_data(&input, offline, warn_unused)?;
            let has_any_errors = has_errors(&diagnostics);
            if show_holes {
                let response = typed_holes_response(holes);
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                let diagnostics = sort_and_cap_diagnostics(diagnostics, max_errors);
                if sarif {
                    let sarif =
                        diagnostics_to_sarif(&diagnostics, "aic", env!("CARGO_PKG_VERSION"));
                    println!("{}", serde_json::to_string_pretty(&sarif)?);
                } else if json {
                    println!("{}", serde_json::to_string_pretty(&diagnostics)?);
                } else if diagnostics.is_empty() {
                    println!("check: ok");
                } else {
                    print!("{}", diagnostics_pretty(&diagnostics));
                }
            }

            if has_any_errors {
                EXIT_DIAGNOSTIC_ERROR
            } else {
                EXIT_OK
            }
        }
        Command::Ast {
            input,
            json,
            offline,
        } => {
            if !json {
                anyhow::bail!("`aic ast` requires --json");
            }
            let front = run_frontend_with_options(&input, FrontendOptions { offline })?;
            let has_any_errors = has_errors(&front.diagnostics);
            let response = build_ast_response(front);
            println!("{}", serde_json::to_string_pretty(&response)?);
            if has_any_errors {
                EXIT_DIAGNOSTIC_ERROR
            } else {
                EXIT_OK
            }
        }
        Command::Impact {
            function,
            input,
            offline,
        } => {
            let front = run_frontend_with_options(&input, FrontendOptions { offline })?;
            if has_errors(&front.diagnostics) {
                print!("{}", diagnostics_pretty(&front.diagnostics));
                EXIT_DIAGNOSTIC_ERROR
            } else {
                match aicore::impact::analyze(&front, &function) {
                    Ok(report) => {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                        EXIT_OK
                    }
                    Err(err) => {
                        eprintln!("impact: {err}");
                        EXIT_DIAGNOSTIC_ERROR
                    }
                }
            }
        }
        Command::SuggestEffects { input, offline } => {
            let front = run_frontend_with_options(&input, FrontendOptions { offline })?;
            let has_any_errors = has_errors(&front.diagnostics);
            let report = aicore::suggest_effects::analyze(&front);
            println!("{}", serde_json::to_string_pretty(&report)?);
            if has_any_errors {
                EXIT_DIAGNOSTIC_ERROR
            } else {
                EXIT_OK
            }
        }
        Command::SuggestContracts {
            input,
            json,
            offline,
        } => {
            let front = run_frontend_with_options(&input, FrontendOptions { offline })?;
            let has_any_errors = has_errors(&front.diagnostics);
            let report = aicore::suggest_contracts::analyze(&front);
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", aicore::suggest_contracts::format_text(&report));
            }
            if has_any_errors {
                EXIT_DIAGNOSTIC_ERROR
            } else {
                EXIT_OK
            }
        }
        Command::Coverage {
            input,
            check,
            min,
            report,
            offline,
        } => {
            let diagnostics = collect_diagnostics(&input, offline, false)?;
            let mut coverage_report = coverage::build_report(&input, &diagnostics)?;
            if check {
                coverage::apply_threshold(&mut coverage_report, min);
            }
            if let Some(report_path) = report {
                coverage::write_report(&report_path, &coverage_report)?;
            }
            println!("{}", serde_json::to_string_pretty(&coverage_report)?);
            if coverage_report
                .check
                .as_ref()
                .map(|result| result.passed)
                .unwrap_or(true)
            {
                EXIT_OK
            } else {
                EXIT_DIAGNOSTIC_ERROR
            }
        }
        Command::Metrics {
            input,
            check,
            max_cyclomatic,
        } => {
            let mut metrics_report = metrics::build_report(&input)?;
            if check {
                let configured_thresholds = metrics_thresholds_for_input(&input)?;
                let thresholds = metrics::resolve_thresholds(
                    configured_thresholds,
                    MetricsThresholdOverrides {
                        max_cyclomatic,
                        ..MetricsThresholdOverrides::default()
                    },
                );
                metrics::apply_thresholds(&mut metrics_report, thresholds);
            }
            println!("{}", serde_json::to_string_pretty(&metrics_report)?);
            if metrics_report
                .check
                .as_ref()
                .map(|result| result.passed)
                .unwrap_or(true)
            {
                EXIT_OK
            } else {
                EXIT_DIAGNOSTIC_ERROR
            }
        }
        Command::Bench {
            budget,
            output,
            compare,
        } => {
            let root = std::env::current_dir()?;
            let budget_spec = load_budget(&budget)?;
            let baseline = match compare.as_deref() {
                Some(path) => Some(load_compare_baseline(path)?),
                None => None,
            };
            let report = run_perf_gate(&root, &budget_spec, baseline.as_ref())?;
            let target = host_target_label();
            let trend = build_trend_report(&report, target);
            let ok = report.violations.is_empty();

            let payload = serde_json::json!({
                "phase": "bench",
                "schema_version": "1.0",
                "target": target,
                "ok": ok,
                "budget_path": budget.display().to_string(),
                "output_path": output.display().to_string(),
                "compare_path": compare.as_ref().map(|path| path.display().to_string()),
                "report": report,
                "trend": trend,
            });

            if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            let json = serde_json::to_string_pretty(&payload)?;
            std::fs::write(&output, &json)?;
            println!("{json}");

            if ok {
                EXIT_OK
            } else {
                EXIT_DIAGNOSTIC_ERROR
            }
        }
        Command::Diag {
            command,
            input,
            json,
            sarif,
            offline,
            warn_unused: diag_warn_unused,
        } => match command {
            Some(DiagSubcommand::ApplyFixes {
                input,
                dry_run,
                json,
                offline,
                warn_unused: apply_warn_unused,
            }) => {
                let diagnostics =
                    collect_diagnostics(&input, offline, diag_warn_unused || apply_warn_unused)?;

                let response = apply_safe_fixes(&diagnostics, dry_run)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                } else {
                    println!(
                        "diag apply-fixes (mode={}): {} planned edits, {} conflicts",
                        response.mode,
                        response.applied_edits.len(),
                        response.conflicts.len()
                    );
                    for edit in &response.applied_edits {
                        println!(
                            "  apply {}:{}..{} {}",
                            edit.file, edit.start, edit.end, edit.message
                        );
                    }
                    for conflict in &response.conflicts {
                        eprintln!(
                            "  conflict {}:{}..{} {}",
                            conflict.file, conflict.start, conflict.end, conflict.message
                        );
                    }
                }
                if response.ok {
                    EXIT_OK
                } else {
                    EXIT_DIAGNOSTIC_ERROR
                }
            }
            None => {
                let diagnostics = collect_diagnostics(&input, offline, diag_warn_unused)?;

                if sarif {
                    let sarif =
                        diagnostics_to_sarif(&diagnostics, "aic", env!("CARGO_PKG_VERSION"));
                    println!("{}", serde_json::to_string_pretty(&sarif)?);
                } else if json {
                    println!("{}", serde_json::to_string_pretty(&diagnostics)?);
                } else if diagnostics.is_empty() {
                    println!("check: ok");
                } else {
                    print!("{}", diagnostics_pretty(&diagnostics));
                }

                if has_errors(&diagnostics) {
                    EXIT_DIAGNOSTIC_ERROR
                } else {
                    EXIT_OK
                }
            }
        },
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
        Command::Migrate {
            path,
            dry_run,
            json,
            report,
        } => {
            let migration = run_migration(&path, dry_run)?;
            if let Some(report_path) = report {
                write_migration_report(&report_path, &migration)?;
                if !json {
                    println!("wrote migration report {}", report_path.display());
                }
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&migration)?);
            } else {
                println!(
                    "migration: scanned={} changed={} edits={} high-risk={} dry-run={}",
                    migration.files_scanned,
                    migration.files_changed,
                    migration.edits_planned,
                    migration.high_risk_edits,
                    migration.dry_run
                );
                for file in migration.files.iter().filter(|file| file.changed) {
                    let highest = file
                        .highest_risk
                        .as_deref()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "none".to_string());
                    println!(
                        "  {} [{}] edits={} highest-risk={}",
                        file.path,
                        file.file_kind,
                        file.edits.len(),
                        highest
                    );
                    for edit in &file.edits {
                        println!(
                            "    {} {}:{} {}",
                            edit.rule, edit.start_line, edit.start_col, edit.description
                        );
                    }
                }
                if !migration.warnings.is_empty() {
                    println!("warnings:");
                    for warning in &migration.warnings {
                        println!("  - {}", warning);
                    }
                }
                if migration.high_risk_edits > 0 {
                    println!("note: high-risk migrations detected; review report before release.");
                }
            }
            EXIT_OK
        }
        Command::Lock { path } => {
            let root = resolve_project_root(&path);
            let lock_path = generate_and_write_lockfile(&root)?;
            println!("generated {}", lock_path.display());
            EXIT_OK
        }
        Command::Pkg { command } => match command {
            PkgCommand::Publish {
                path,
                registry,
                registry_config,
                token,
                json,
            } => {
                let root = resolve_project_root(&path);
                let options = RegistryClientOptions {
                    registry,
                    registry_config,
                    token,
                };
                match pkg_publish_with_options(&root, &options) {
                    Ok(result) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&result)?);
                        } else {
                            println!(
                                "published {}@{} ({})",
                                result.package, result.version, result.checksum
                            );
                            println!("registry path: {}", result.registry_path);
                        }
                        EXIT_OK
                    }
                    Err(diag) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&vec![diag])?);
                        } else {
                            print!("{}", diagnostics_pretty(&[diag]));
                        }
                        EXIT_DIAGNOSTIC_ERROR
                    }
                }
            }
            PkgCommand::Search {
                query,
                registry,
                registry_config,
                token,
                json,
            } => {
                let root = std::env::current_dir()?;
                let options = RegistryClientOptions {
                    registry,
                    registry_config,
                    token,
                };
                match pkg_search_with_options(&root, query.as_deref(), &options) {
                    Ok(results) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&results)?);
                        } else if results.is_empty() {
                            println!("no packages found");
                        } else {
                            for result in results {
                                println!(
                                    "{} latest={} versions={}",
                                    result.package,
                                    result.latest,
                                    result.versions.join(",")
                                );
                            }
                        }
                        EXIT_OK
                    }
                    Err(diag) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&vec![diag])?);
                        } else {
                            print!("{}", diagnostics_pretty(&[diag]));
                        }
                        EXIT_DIAGNOSTIC_ERROR
                    }
                }
            }
            PkgCommand::Install {
                specs,
                path,
                registry,
                registry_config,
                token,
                json,
            } => {
                let root = resolve_project_root(&path);
                let options = RegistryClientOptions {
                    registry,
                    registry_config,
                    token,
                };
                match pkg_install_with_options(&root, &specs, &options) {
                    Ok(result) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&result)?);
                        } else {
                            for item in &result.installed {
                                println!(
                                    "installed {}@{} ({}) -> {}",
                                    item.package, item.version, item.requirement, item.path
                                );
                            }
                            for record in &result.audit {
                                println!(
                                    "trust {}@{} {}: {}",
                                    record.package, record.version, record.decision, record.reason
                                );
                            }
                            println!("updated lockfile {}", result.lockfile);
                        }
                        EXIT_OK
                    }
                    Err(diag) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&vec![diag])?);
                        } else {
                            print!("{}", diagnostics_pretty(&[diag]));
                        }
                        EXIT_DIAGNOSTIC_ERROR
                    }
                }
            }
        },
        Command::Build {
            input,
            output,
            artifact,
            target,
            static_link,
            debug_info,
            offline,
            verify_hash,
            manifest,
        } => {
            let target_label = target
                .or_else(host_build_target)
                .map(|entry| entry.canonical_label().to_string())
                .unwrap_or_else(|| host_target_label().to_string());
            let target_triple = target.map(|entry| entry.clang_triple().to_string());

            if static_link && artifact != BuildArtifact::Exe {
                eprintln!("--static-link is supported only with --artifact exe");
                return Ok(EXIT_USAGE_ERROR);
            }
            if static_link
                && !target
                    .map(|entry| entry.supports_static_link())
                    .unwrap_or(cfg!(target_os = "linux"))
            {
                eprintln!(
                    "--static-link currently supports linux targets only; requested target={target_label}"
                );
                return Ok(EXIT_USAGE_ERROR);
            }

            if let Some(expected) = verify_hash.as_deref() {
                if !is_valid_sha256_hex(expected) {
                    eprintln!("--verify-hash must be a 64-character SHA256 hex digest");
                    return Ok(EXIT_USAGE_ERROR);
                }
            }

            if input.is_dir() {
                match workspace_build_plan(&input) {
                    Ok(Some(plan)) => {
                        if output.is_some() {
                            eprintln!("--output is not supported for workspace builds");
                            return Ok(EXIT_USAGE_ERROR);
                        }
                        if verify_hash.is_some() || manifest.is_some() {
                            eprintln!(
                                "--verify-hash and --manifest are not supported for workspace builds"
                            );
                            eprintln!(
                                "help: build a workspace member entry path to emit hermetic artifacts"
                            );
                            return Ok(EXIT_USAGE_ERROR);
                        }
                        if static_link {
                            eprintln!("--static-link is not supported for workspace builds");
                            return Ok(EXIT_USAGE_ERROR);
                        }

                        let workspace_artifact = match artifact {
                            BuildArtifact::Exe => BuildArtifact::Lib,
                            other => other,
                        };

                        let mut member_fingerprints =
                            std::collections::BTreeMap::<String, String>::new();
                        for member in &plan.members {
                            let entry = member.root.join(&member.main);
                            let source_fingerprint =
                                compute_package_checksum_for_path(&member.root)?;
                            let mut fingerprint = format!("self={source_fingerprint}\n");
                            for dep in &member.workspace_dependencies {
                                let dep_fingerprint = member_fingerprints
                                    .get(dep)
                                    .cloned()
                                    .unwrap_or_else(|| "missing".to_string());
                                fingerprint.push_str(&format!("{dep}={dep_fingerprint}\n"));
                            }

                            let link = resolve_native_link_options(&member.root)?;
                            let front =
                                run_frontend_with_options(&entry, FrontendOptions { offline })?;
                            if has_errors(&front.diagnostics) {
                                print!("{}", diagnostics_pretty(&front.diagnostics));
                                return Ok(EXIT_DIAGNOSTIC_ERROR);
                            }

                            let lowered = lower_runtime_asserts(&front.ir);
                            let llvm = match emit_llvm_with_options(
                                &lowered,
                                &entry.to_string_lossy(),
                                CodegenOptions { debug_info },
                            ) {
                                Ok(v) => v,
                                Err(diags) => {
                                    print!("{}", diagnostics_pretty(&diags));
                                    return Ok(EXIT_DIAGNOSTIC_ERROR);
                                }
                            };

                            let out = workspace_output_path(
                                &plan.root,
                                &member.name,
                                &entry,
                                workspace_artifact,
                            );
                            let fingerprint_path = out
                                .parent()
                                .unwrap_or_else(|| std::path::Path::new("."))
                                .join(".aic-fingerprint");
                            if out.exists() && fingerprint_path.exists() {
                                if let Ok(existing) = std::fs::read_to_string(&fingerprint_path) {
                                    if existing == fingerprint {
                                        println!("up-to-date {}", out.display());
                                        member_fingerprints
                                            .insert(member.name.clone(), fingerprint.clone());
                                        continue;
                                    }
                                }
                            }
                            if let Some(parent) = out.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            let work = fresh_work_dir("workspace-build");
                            compile_with_clang_artifact_with_options(
                                &llvm.llvm_ir,
                                &out,
                                &work,
                                workspace_artifact.to_codegen(),
                                CompileOptions {
                                    debug_info,
                                    target_triple: target_triple.clone(),
                                    static_link: false,
                                    link,
                                },
                            )?;
                            std::fs::write(&fingerprint_path, &fingerprint)?;
                            member_fingerprints.insert(member.name.clone(), fingerprint);
                            println!("built {}", out.display());
                        }
                        EXIT_OK
                    }
                    Ok(None) => {
                        let project_root = resolve_project_root(&input);
                        let link = resolve_native_link_options(&project_root)?;
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

                            let out = output
                                .unwrap_or_else(|| default_build_output_name(&input, artifact));
                            let work = fresh_work_dir("build");
                            compile_with_clang_artifact_with_options(
                                &llvm.llvm_ir,
                                &out,
                                &work,
                                artifact.to_codegen(),
                                CompileOptions {
                                    debug_info,
                                    target_triple: target_triple.clone(),
                                    static_link,
                                    link,
                                },
                            )?;
                            let manifest_path = manifest
                                .as_deref()
                                .unwrap_or_else(|| std::path::Path::new("build.json"));
                            if let Some(message) = process_built_artifact(
                                &input,
                                &out,
                                artifact,
                                verify_hash.as_deref(),
                                manifest_path,
                                &target_label,
                                static_link,
                            )? {
                                eprintln!("{message}");
                                return Ok(EXIT_DIAGNOSTIC_ERROR);
                            }
                            println!("built {}", out.display());
                            EXIT_OK
                        }
                    }
                    Err(diag) => {
                        print!("{}", diagnostics_pretty(&[diag]));
                        EXIT_DIAGNOSTIC_ERROR
                    }
                }
            } else {
                let project_root = resolve_project_root(&input);
                let link = resolve_native_link_options(&project_root)?;
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
                    let work = fresh_work_dir("build");
                    compile_with_clang_artifact_with_options(
                        &llvm.llvm_ir,
                        &out,
                        &work,
                        artifact.to_codegen(),
                        CompileOptions {
                            debug_info,
                            target_triple,
                            static_link,
                            link,
                        },
                    )?;
                    let manifest_path = manifest
                        .as_deref()
                        .unwrap_or_else(|| std::path::Path::new("build.json"));
                    if let Some(message) = process_built_artifact(
                        &input,
                        &out,
                        artifact,
                        verify_hash.as_deref(),
                        manifest_path,
                        &target_label,
                        static_link,
                    )? {
                        eprintln!("{message}");
                        return Ok(EXIT_DIAGNOSTIC_ERROR);
                    }
                    println!("built {}", out.display());
                    EXIT_OK
                }
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
        Command::Diff {
            semantic: _,
            fail_on_breaking,
            old_file,
            new_file,
        } => match semantic_diff::diff_files(&old_file, &new_file) {
            Ok(report) => {
                println!("{}", serde_json::to_string_pretty(&report)?);
                if fail_on_breaking && report.summary.breaking > 0 {
                    EXIT_DIAGNOSTIC_ERROR
                } else {
                    EXIT_OK
                }
            }
            Err(err) => {
                eprintln!("diff: {err}");
                EXIT_DIAGNOSTIC_ERROR
            }
        },
        Command::Lsp => {
            lsp::run_stdio()?;
            EXIT_OK
        }
        Command::Daemon => {
            daemon::run_stdio()?;
            EXIT_OK
        }
        Command::Repl { json } => {
            run_repl(json)?;
            EXIT_OK
        }
        Command::Test {
            path,
            mode,
            json,
            update_golden,
            check_golden,
        } => {
            let golden_mode = if update_golden {
                GoldenMode::Update
            } else if check_golden {
                GoldenMode::Check
            } else {
                GoldenMode::Legacy
            };
            let report = run_harness_with_golden_mode(&path, mode.to_harness_mode(), golden_mode)?;
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
        Command::Grammar { ebnf, json } => {
            debug_assert!(ebnf ^ json);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&grammar_contract_json())?
                );
            } else {
                print!("{GRAMMAR_EBNF}");
            }
            EXIT_OK
        }
        Command::Contract {
            json,
            accept_versions,
        } => {
            let contract = contract_json(&accept_versions);
            let protocol = &contract["protocol"];
            let compatible = protocol["compatible"].as_bool().unwrap_or(true);
            if json {
                println!("{}", serde_json::to_string_pretty(&contract)?);
            } else {
                println!(
                    "AICore CLI contract v{}",
                    contract["version"].as_str().unwrap_or("1.0")
                );
                println!(
                    "protocol selected version: {}",
                    protocol["selected_version"].as_str().unwrap_or("none")
                );
                println!("exit codes:");
                println!("  {} -> success", EXIT_OK);
                println!("  {} -> diagnostics/runtime failure", EXIT_DIAGNOSTIC_ERROR);
                println!("  {} -> command-line usage error", EXIT_USAGE_ERROR);
                println!("  {} -> internal/tooling failure", EXIT_INTERNAL_ERROR);
            }
            if compatible {
                EXIT_OK
            } else {
                if !json {
                    eprintln!(
                        "error: requested protocol versions are incompatible with supported versions"
                    );
                }
                EXIT_DIAGNOSTIC_ERROR
            }
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
            ReleaseCommand::VerifyChecksum {
                artifact,
                checksum,
                json,
            } => {
                let errors = verify_checksum_file(&artifact, &checksum)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&errors)?);
                } else if errors.is_empty() {
                    println!("checksum verification: ok");
                } else {
                    eprintln!("checksum verification failed:");
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
            ReleaseCommand::Lts { root, json, check } => {
                let policy = lts_policy();
                let problems = if check {
                    check_lts_policy(&root, &policy)
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
                        println!("lts policy check: ok");
                    } else {
                        eprintln!("lts policy check failed:");
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
            sandbox_config,
            profile,
            profile_output,
            check_leaks,
            asan,
            args,
        } => {
            let profile_policy = sandbox.to_profile().policy();
            let policy = if let Some(config_path) = sandbox_config {
                load_sandbox_policy(&config_path)?
            } else {
                profile_policy
            };

            if !cfg!(target_os = "linux") && policy.limits.is_some() {
                eprintln!(
                    "sandbox profile '{}' requires Linux `prlimit`; use --sandbox none",
                    policy.profile
                );
                return Ok(EXIT_USAGE_ERROR);
            }

            if profile && check_leaks {
                eprintln!("--check-leaks is not supported with --profile");
                return Ok(EXIT_USAGE_ERROR);
            }
            if profile && asan {
                eprintln!("--asan is not supported with --profile");
                return Ok(EXIT_USAGE_ERROR);
            }

            if profile {
                let outcome = profile::run_profiled(profile::RunProfileOptions {
                    input: &input,
                    offline,
                    args: &args,
                    policy: &policy,
                    output_path: &profile_output,
                })?;
                outcome.exit_code
            } else {
                let runtime = RuntimeInstrumentationOptions {
                    check_leaks,
                    asan: asan || env_flag_enabled("AIC_RUN_ASAN"),
                };
                let run_work = fresh_work_dir("run-bin");
                let out = run_work.join("aicore_run_bin");
                let build_code = build_file(&input, &out, offline, runtime)?;
                if build_code != EXIT_OK {
                    build_code
                } else {
                    let trace_id = telemetry::current_trace_id();
                    let execute_started = Instant::now();
                    let status = run_with_policy(&out, &args, &policy, Some(&trace_id))?;
                    let attrs = std::collections::BTreeMap::from([
                        (
                            "input".to_string(),
                            serde_json::json!(input.display().to_string()),
                        ),
                        (
                            "profile".to_string(),
                            serde_json::json!(policy.profile.clone()),
                        ),
                    ]);
                    telemetry::emit_phase(
                        "run",
                        "execute",
                        if status.success() { "ok" } else { "error" },
                        execute_started.elapsed(),
                        attrs.clone(),
                    );
                    telemetry::emit_metric(
                        "run",
                        "exit_code",
                        status.code().unwrap_or(-1) as f64,
                        attrs,
                    );
                    if status.success() {
                        EXIT_OK
                    } else {
                        EXIT_DIAGNOSTIC_ERROR
                    }
                }
            }
        }
    };

    Ok(exit)
}

#[derive(Debug, Clone, PartialEq)]
enum ReplValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Unit,
}

impl ReplValue {
    fn type_name(&self) -> &'static str {
        match self {
            ReplValue::Int(_) => "Int",
            ReplValue::Float(_) => "Float",
            ReplValue::Bool(_) => "Bool",
            ReplValue::String(_) => "String",
            ReplValue::Unit => "Unit",
        }
    }

    fn render_text(&self) -> String {
        match self {
            ReplValue::Int(v) => v.to_string(),
            ReplValue::Float(v) => {
                let mut text = v.to_string();
                if !text.contains('.') && !text.contains('e') && !text.contains('E') {
                    text.push_str(".0");
                }
                text
            }
            ReplValue::Bool(v) => v.to_string(),
            ReplValue::String(v) => format!("{v:?}"),
            ReplValue::Unit => "()".to_string(),
        }
    }

    fn to_json_value(&self) -> serde_json::Value {
        match self {
            ReplValue::Int(v) => serde_json::json!(v),
            ReplValue::Float(v) => serde_json::json!(v),
            ReplValue::Bool(v) => serde_json::json!(v),
            ReplValue::String(v) => serde_json::json!(v),
            ReplValue::Unit => serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone)]
struct ReplBinding {
    value: ReplValue,
    mutable: bool,
}

#[derive(Debug, Default)]
struct ReplState {
    env: BTreeMap<String, ReplBinding>,
}

impl ReplState {
    fn new() -> Self {
        let mut state = Self::default();
        state.set_last(ReplValue::Unit);
        state
    }

    fn set_last(&mut self, value: ReplValue) {
        self.env.insert(
            "_".to_string(),
            ReplBinding {
                value,
                mutable: true,
            },
        );
    }
}

#[derive(Debug, Clone)]
struct ReplEvalResult {
    value: ReplValue,
    binding: Option<String>,
}

#[derive(Debug, Default)]
struct ReplHistory {
    entries: Vec<String>,
}

impl ReplHistory {
    fn push(&mut self, line: &str) {
        self.entries.push(line.to_string());
    }

    fn latest(&self) -> Option<&str> {
        self.entries.last().map(String::as_str)
    }

    fn by_index(&self, one_based_index: usize) -> Option<&str> {
        one_based_index
            .checked_sub(1)
            .and_then(|index| self.entries.get(index))
            .map(String::as_str)
    }
}

fn run_repl(json: bool) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut state = ReplState::new();
    let mut history = ReplHistory::default();

    if json {
        repl_emit_json(
            &mut stdout,
            &serde_json::json!({
                "event": "ready",
                "mode": "json",
                "commands": [":type <expr>", ":effects <fn>", ":quit"],
            }),
        )?;
    } else {
        repl_emit_text(
            &mut stdout,
            "aic repl ready (:type <expr>, :effects <fn>, :history, :quit)",
        )?;
    }

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                let message = format!("stdin read failed: {err}");
                if json {
                    repl_emit_json(
                        &mut stdout,
                        &serde_json::json!({
                            "event": "error",
                            "message": message,
                        }),
                    )?;
                } else {
                    repl_emit_text(&mut stdout, &format!("error: {message}"))?;
                }
                break;
            }
        };
        let line = if json {
            line
        } else {
            repl_apply_line_editing(&line)
        };
        let mut trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        if !json && trimmed == ":history" {
            repl_print_history(&mut stdout, &history)?;
            continue;
        }

        if !json {
            match repl_expand_history_command(&trimmed, &history) {
                Ok(Some(expanded)) => trimmed = expanded,
                Ok(None) => {}
                Err(message) => {
                    repl_emit_text(&mut stdout, &format!("error: {message}"))?;
                    continue;
                }
            }
        }

        if trimmed == ":quit" {
            if json {
                repl_emit_json(&mut stdout, &serde_json::json!({ "event": "bye" }))?;
            } else {
                repl_emit_text(&mut stdout, "bye")?;
            }
            break;
        }

        if !json {
            history.push(&trimmed);
        }

        if let Some(expr) = trimmed.strip_prefix(":type ") {
            let expr = expr.trim();
            match repl_eval_type(expr, &state) {
                Ok(ty) => {
                    if json {
                        repl_emit_json(
                            &mut stdout,
                            &serde_json::json!({
                                "event": "type",
                                "type": ty,
                            }),
                        )?;
                    } else {
                        repl_emit_text(&mut stdout, &ty)?;
                    }
                }
                Err(message) => {
                    if json {
                        repl_emit_json(
                            &mut stdout,
                            &serde_json::json!({
                                "event": "error",
                                "message": message,
                            }),
                        )?;
                    } else {
                        repl_emit_text(&mut stdout, &format!("error: {message}"))?;
                    }
                }
            }
            continue;
        }

        if trimmed == ":type" {
            let message = "missing expression; usage: :type <expr>".to_string();
            if json {
                repl_emit_json(
                    &mut stdout,
                    &serde_json::json!({
                        "event": "error",
                        "message": message,
                    }),
                )?;
            } else {
                repl_emit_text(&mut stdout, &format!("error: {message}"))?;
            }
            continue;
        }

        if let Some(name) = trimmed.strip_prefix(":effects ") {
            let name = name.trim();
            match repl_effects_for(name) {
                Some(effects) => {
                    if json {
                        repl_emit_json(
                            &mut stdout,
                            &serde_json::json!({
                                "event": "effects",
                                "function": name,
                                "effects": effects,
                            }),
                        )?;
                    } else {
                        repl_emit_text(
                            &mut stdout,
                            &format!("{name} effects {}", repl_effects_text(effects)),
                        )?;
                    }
                }
                None => {
                    let message = format!("unknown function `{name}`");
                    if json {
                        repl_emit_json(
                            &mut stdout,
                            &serde_json::json!({
                                "event": "error",
                                "message": message,
                            }),
                        )?;
                    } else {
                        repl_emit_text(&mut stdout, &format!("error: {message}"))?;
                    }
                }
            }
            continue;
        }

        if trimmed == ":effects" {
            let message = "missing function name; usage: :effects <fn>".to_string();
            if json {
                repl_emit_json(
                    &mut stdout,
                    &serde_json::json!({
                        "event": "error",
                        "message": message,
                    }),
                )?;
            } else {
                repl_emit_text(&mut stdout, &format!("error: {message}"))?;
            }
            continue;
        }

        if trimmed.starts_with(':') {
            let message = format!("unknown command `{trimmed}`");
            if json {
                repl_emit_json(
                    &mut stdout,
                    &serde_json::json!({
                        "event": "error",
                        "message": message,
                    }),
                )?;
            } else {
                repl_emit_text(&mut stdout, &format!("error: {message}"))?;
            }
            continue;
        }

        match parse_repl_statement(&trimmed).and_then(|stmt| eval_repl_statement(&stmt, &mut state))
        {
            Ok(result) => {
                if json {
                    repl_emit_json(
                        &mut stdout,
                        &serde_json::json!({
                            "event": "result",
                            "binding": result.binding,
                            "value": result.value.to_json_value(),
                            "type": result.value.type_name(),
                        }),
                    )?;
                } else {
                    let value = result.value.render_text();
                    let ty = result.value.type_name();
                    if let Some(name) = result.binding {
                        repl_emit_text(&mut stdout, &format!("{name} = {value} : {ty}"))?;
                    } else {
                        repl_emit_text(&mut stdout, &format!("{value} : {ty}"))?;
                    }
                }
            }
            Err(message) => {
                if json {
                    repl_emit_json(
                        &mut stdout,
                        &serde_json::json!({
                            "event": "error",
                            "message": message,
                        }),
                    )?;
                } else {
                    repl_emit_text(&mut stdout, &format!("error: {message}"))?;
                }
            }
        }
    }

    Ok(())
}

fn repl_apply_line_editing(line: &str) -> String {
    let mut edited = String::new();
    for ch in line.chars() {
        match ch {
            '\u{08}' | '\u{7f}' => {
                edited.pop();
            }
            '\u{15}' => edited.clear(),
            '\u{17}' => repl_delete_last_word(&mut edited),
            _ => edited.push(ch),
        }
    }
    edited
}

fn repl_delete_last_word(buffer: &mut String) {
    while matches!(buffer.chars().last(), Some(ch) if ch.is_whitespace()) {
        buffer.pop();
    }
    while matches!(buffer.chars().last(), Some(ch) if !ch.is_whitespace()) {
        buffer.pop();
    }
}

fn repl_print_history(out: &mut impl Write, history: &ReplHistory) -> anyhow::Result<()> {
    if history.entries.is_empty() {
        repl_emit_text(out, "history is empty")
    } else {
        for (index, entry) in history.entries.iter().enumerate() {
            repl_emit_text(out, &format!("{}: {entry}", index + 1))?;
        }
        Ok(())
    }
}

fn repl_expand_history_command(
    line: &str,
    history: &ReplHistory,
) -> Result<Option<String>, String> {
    if line == "!!" {
        return history
            .latest()
            .map(|entry| Some(entry.to_string()))
            .ok_or_else(|| "history is empty".to_string());
    }

    let Some(raw_index) = line.strip_prefix('!') else {
        return Ok(None);
    };
    if raw_index.is_empty() || !raw_index.chars().all(|ch| ch.is_ascii_digit()) {
        return Ok(None);
    }

    let index = raw_index
        .parse::<usize>()
        .map_err(|_| format!("invalid history reference `{line}`"))?;
    if index == 0 {
        return Err("history references are 1-based; use !1 for the first entry".to_string());
    }
    history
        .by_index(index)
        .map(|entry| Some(entry.to_string()))
        .ok_or_else(|| format!("history entry `{line}` not found"))
}

fn repl_emit_text(out: &mut impl Write, line: &str) -> anyhow::Result<()> {
    writeln!(out, "{line}")?;
    out.flush()?;
    Ok(())
}

fn repl_emit_json(out: &mut impl Write, payload: &serde_json::Value) -> anyhow::Result<()> {
    writeln!(out, "{}", serde_json::to_string(payload)?)?;
    out.flush()?;
    Ok(())
}

fn parse_repl_statement(line: &str) -> Result<aicore::ast::Stmt, String> {
    let statement = if line.trim_end().ends_with(';') {
        line.trim().to_string()
    } else {
        format!("{line};")
    };
    let source = format!("module repl.session;\nfn main() -> Int {{\n    {statement}\n    0\n}}\n");
    let (program, diagnostics) = parser::parse(&source, "<repl>");
    if diagnostics
        .iter()
        .any(|diag| matches!(diag.severity, Severity::Error))
    {
        return Err(repl_first_diagnostic(&diagnostics));
    }
    let program = program.ok_or_else(|| "parse failed".to_string())?;
    let main_fn = program
        .items
        .iter()
        .find_map(|item| match item {
            aicore::ast::Item::Function(func) => Some(func),
            _ => None,
        })
        .ok_or_else(|| "parse failed: wrapper function missing".to_string())?;
    let stmt = main_fn
        .body
        .stmts
        .first()
        .cloned()
        .ok_or_else(|| "expected a single-line expression".to_string())?;
    Ok(stmt)
}

fn repl_first_diagnostic(diagnostics: &[Diagnostic]) -> String {
    let diag = diagnostics
        .iter()
        .find(|diag| matches!(diag.severity, Severity::Error))
        .or_else(|| diagnostics.first());
    match diag {
        Some(diag) => {
            let severity = match diag.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Note => "note",
            };
            format!("{severity}[{}]: {}", diag.code, diag.message)
        }
        None => "parse failed".to_string(),
    }
}

fn repl_eval_type(expr_source: &str, state: &ReplState) -> Result<String, String> {
    let stmt = parse_repl_statement(expr_source)?;
    let expr = match stmt {
        aicore::ast::Stmt::Expr { expr, .. } => expr,
        _ => return Err(":type expects an expression".to_string()),
    };
    let value = eval_repl_expr(&expr, &state.env)?;
    Ok(value.type_name().to_string())
}

fn repl_effects_for(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "print_int" | "print_float" | "print_bool" | "print_string" => Some(&["io"]),
        "len" | "replace" => Some(&[]),
        _ => None,
    }
}

fn repl_effects_text(effects: &[&str]) -> String {
    if effects.is_empty() {
        "{}".to_string()
    } else {
        format!("{{ {} }}", effects.join(", "))
    }
}

fn eval_repl_statement(
    stmt: &aicore::ast::Stmt,
    state: &mut ReplState,
) -> Result<ReplEvalResult, String> {
    match stmt {
        aicore::ast::Stmt::Let {
            name,
            mutable,
            ty,
            expr,
            ..
        } => {
            if name == "_" {
                return Err("`_` is reserved in repl".to_string());
            }
            if state.env.contains_key(name) {
                return Err(format!("`{name}` is already defined"));
            }
            let value = eval_repl_expr(expr, &state.env)?;
            if let Some(expected) = ty {
                let expected_name = repl_type_expr_name(expected)?;
                if expected_name != value.type_name() {
                    return Err(format!(
                        "type mismatch in `let {name}`: expected {expected_name}, got {}",
                        value.type_name()
                    ));
                }
            }
            state.env.insert(
                name.clone(),
                ReplBinding {
                    value: value.clone(),
                    mutable: *mutable,
                },
            );
            state.set_last(value.clone());
            Ok(ReplEvalResult {
                value,
                binding: Some(name.clone()),
            })
        }
        aicore::ast::Stmt::Assign { target, expr, .. } => {
            let value = eval_repl_expr(expr, &state.env)?;
            let Some(binding) = state.env.get_mut(target) else {
                return Err(format!("unknown variable `{target}`"));
            };
            if !binding.mutable {
                return Err(format!("cannot assign to immutable binding `{target}`"));
            }
            if binding.value.type_name() != value.type_name() {
                return Err(format!(
                    "type mismatch in assignment to `{target}`: expected {}, got {}",
                    binding.value.type_name(),
                    value.type_name()
                ));
            }
            binding.value = value.clone();
            state.set_last(value.clone());
            Ok(ReplEvalResult {
                value,
                binding: Some(target.clone()),
            })
        }
        aicore::ast::Stmt::Expr { expr, .. } => {
            let value = eval_repl_expr(expr, &state.env)?;
            state.set_last(value.clone());
            Ok(ReplEvalResult {
                value,
                binding: None,
            })
        }
        _ => Err("unsupported statement in repl; use expressions, let, or assignment".to_string()),
    }
}

fn repl_type_expr_name(ty: &aicore::ast::TypeExpr) -> Result<&'static str, String> {
    match &ty.kind {
        aicore::ast::TypeKind::Unit => Ok("Unit"),
        aicore::ast::TypeKind::Named { name, args } => {
            if !args.is_empty() {
                return Err("generic type annotations are not supported in repl".to_string());
            }
            match name.as_str() {
                "Int" => Ok("Int"),
                "Float" => Ok("Float"),
                "Bool" => Ok("Bool"),
                "String" => Ok("String"),
                "Unit" => Ok("Unit"),
                other => Err(format!("unsupported type annotation `{other}` in repl")),
            }
        }
        aicore::ast::TypeKind::Hole => {
            Err("typed hole `_` annotations are not supported in repl".to_string())
        }
    }
}

fn eval_repl_expr(
    expr: &aicore::ast::Expr,
    env: &BTreeMap<String, ReplBinding>,
) -> Result<ReplValue, String> {
    use aicore::ast::{BinOp, ExprKind, UnaryOp};

    match &expr.kind {
        ExprKind::Int(v) => Ok(ReplValue::Int(*v)),
        ExprKind::Float(v) => Ok(ReplValue::Float(*v)),
        ExprKind::Bool(v) => Ok(ReplValue::Bool(*v)),
        ExprKind::String(v) => Ok(ReplValue::String(v.clone())),
        ExprKind::Unit => Ok(ReplValue::Unit),
        ExprKind::Var(name) => env
            .get(name)
            .map(|binding| binding.value.clone())
            .ok_or_else(|| format!("unknown variable `{name}`")),
        ExprKind::Unary { op, expr } => {
            let value = eval_repl_expr(expr, env)?;
            match op {
                UnaryOp::Neg => match value {
                    ReplValue::Int(v) => Ok(ReplValue::Int(-v)),
                    ReplValue::Float(v) => Ok(ReplValue::Float(-v)),
                    other => Err(format!(
                        "unary `-` expects number, got {}",
                        other.type_name()
                    )),
                },
                UnaryOp::Not => match value {
                    ReplValue::Bool(v) => Ok(ReplValue::Bool(!v)),
                    other => Err(format!("unary `!` expects Bool, got {}", other.type_name())),
                },
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            if matches!(op, BinOp::And) {
                let left = eval_repl_expr(lhs, env)?;
                let ReplValue::Bool(left_bool) = left else {
                    return Err("`&&` expects Bool operands".to_string());
                };
                if !left_bool {
                    return Ok(ReplValue::Bool(false));
                }
                let right = eval_repl_expr(rhs, env)?;
                let ReplValue::Bool(right_bool) = right else {
                    return Err("`&&` expects Bool operands".to_string());
                };
                return Ok(ReplValue::Bool(right_bool));
            }
            if matches!(op, BinOp::Or) {
                let left = eval_repl_expr(lhs, env)?;
                let ReplValue::Bool(left_bool) = left else {
                    return Err("`||` expects Bool operands".to_string());
                };
                if left_bool {
                    return Ok(ReplValue::Bool(true));
                }
                let right = eval_repl_expr(rhs, env)?;
                let ReplValue::Bool(right_bool) = right else {
                    return Err("`||` expects Bool operands".to_string());
                };
                return Ok(ReplValue::Bool(right_bool));
            }

            let left = eval_repl_expr(lhs, env)?;
            let right = eval_repl_expr(rhs, env)?;
            match op {
                BinOp::Add => match (left, right) {
                    (ReplValue::Int(a), ReplValue::Int(b)) => Ok(ReplValue::Int(a + b)),
                    (ReplValue::Float(a), ReplValue::Float(b)) => Ok(ReplValue::Float(a + b)),
                    (ReplValue::Int(a), ReplValue::Float(b)) => Ok(ReplValue::Float(a as f64 + b)),
                    (ReplValue::Float(a), ReplValue::Int(b)) => Ok(ReplValue::Float(a + b as f64)),
                    (ReplValue::String(a), ReplValue::String(b)) => {
                        Ok(ReplValue::String(format!("{a}{b}")))
                    }
                    (a, b) => Err(format!(
                        "`+` expects (Int,Int), (Float,Float), or (String,String); got ({},{})",
                        a.type_name(),
                        b.type_name()
                    )),
                },
                BinOp::Sub => repl_eval_numeric_binop(left, right, "-", |a, b| a - b, |a, b| a - b),
                BinOp::Mul => repl_eval_numeric_binop(left, right, "*", |a, b| a * b, |a, b| a * b),
                BinOp::Div => match (left, right) {
                    (ReplValue::Int(_), ReplValue::Int(0)) => {
                        Err("division by zero is not allowed".to_string())
                    }
                    (ReplValue::Int(a), ReplValue::Int(b)) => Ok(ReplValue::Int(a / b)),
                    (ReplValue::Float(_), ReplValue::Float(0.0))
                    | (ReplValue::Float(_), ReplValue::Int(0))
                    | (ReplValue::Int(_), ReplValue::Float(0.0)) => {
                        Err("division by zero is not allowed".to_string())
                    }
                    (ReplValue::Float(a), ReplValue::Float(b)) => Ok(ReplValue::Float(a / b)),
                    (ReplValue::Int(a), ReplValue::Float(b)) => Ok(ReplValue::Float(a as f64 / b)),
                    (ReplValue::Float(a), ReplValue::Int(b)) => Ok(ReplValue::Float(a / b as f64)),
                    (a, b) => Err(format!(
                        "`/` expects numeric operands, got ({},{})",
                        a.type_name(),
                        b.type_name()
                    )),
                },
                BinOp::Mod => match (left, right) {
                    (ReplValue::Int(_), ReplValue::Int(0)) => {
                        Err("modulo by zero is not allowed".to_string())
                    }
                    (ReplValue::Int(a), ReplValue::Int(b)) => Ok(ReplValue::Int(a % b)),
                    (a, b) => Err(format!(
                        "`%` expects Int operands, got ({},{})",
                        a.type_name(),
                        b.type_name()
                    )),
                },
                BinOp::Eq => Ok(ReplValue::Bool(repl_eq_values(&left, &right)?)),
                BinOp::Ne => Ok(ReplValue::Bool(!repl_eq_values(&left, &right)?)),
                BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    repl_eval_order_comparison(*op, left, right)
                }
                BinOp::And | BinOp::Or => {
                    Err("internal repl error: short-circuit op not handled".to_string())
                }
            }
        }
        kind => Err(format!(
            "unsupported expression in repl: {}",
            repl_expr_kind_name(kind)
        )),
    }
}

fn repl_eval_numeric_binop(
    left: ReplValue,
    right: ReplValue,
    op_name: &str,
    int_op: fn(i64, i64) -> i64,
    float_op: fn(f64, f64) -> f64,
) -> Result<ReplValue, String> {
    match (left, right) {
        (ReplValue::Int(a), ReplValue::Int(b)) => Ok(ReplValue::Int(int_op(a, b))),
        (ReplValue::Float(a), ReplValue::Float(b)) => Ok(ReplValue::Float(float_op(a, b))),
        (ReplValue::Int(a), ReplValue::Float(b)) => Ok(ReplValue::Float(float_op(a as f64, b))),
        (ReplValue::Float(a), ReplValue::Int(b)) => Ok(ReplValue::Float(float_op(a, b as f64))),
        (a, b) => Err(format!(
            "`{op_name}` expects numeric operands, got ({},{})",
            a.type_name(),
            b.type_name()
        )),
    }
}

fn repl_eq_values(left: &ReplValue, right: &ReplValue) -> Result<bool, String> {
    match (left, right) {
        (ReplValue::Int(a), ReplValue::Int(b)) => Ok(a == b),
        (ReplValue::Float(a), ReplValue::Float(b)) => Ok(a == b),
        (ReplValue::Int(a), ReplValue::Float(b)) => Ok((*a as f64) == *b),
        (ReplValue::Float(a), ReplValue::Int(b)) => Ok(*a == (*b as f64)),
        (ReplValue::Bool(a), ReplValue::Bool(b)) => Ok(a == b),
        (ReplValue::String(a), ReplValue::String(b)) => Ok(a == b),
        (ReplValue::Unit, ReplValue::Unit) => Ok(true),
        (a, b) => Err(format!(
            "cannot compare equality between {} and {}",
            a.type_name(),
            b.type_name()
        )),
    }
}

fn repl_eval_order_comparison(
    op: aicore::ast::BinOp,
    left: ReplValue,
    right: ReplValue,
) -> Result<ReplValue, String> {
    use aicore::ast::BinOp;

    let result = match (left, right) {
        (ReplValue::Int(a), ReplValue::Int(b)) => repl_apply_order(op, a, b),
        (ReplValue::Float(a), ReplValue::Float(b)) => repl_apply_order(op, a, b),
        (ReplValue::Int(a), ReplValue::Float(b)) => repl_apply_order(op, a as f64, b),
        (ReplValue::Float(a), ReplValue::Int(b)) => repl_apply_order(op, a, b as f64),
        (ReplValue::String(a), ReplValue::String(b)) => repl_apply_order(op, a, b),
        (a, b) => {
            return Err(format!(
                "`{}` expects comparable operands, got ({},{})",
                match op {
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Mod
                    | BinOp::Eq
                    | BinOp::Ne
                    | BinOp::And
                    | BinOp::Or => "?",
                },
                a.type_name(),
                b.type_name()
            ));
        }
    };
    Ok(ReplValue::Bool(result))
}

fn repl_apply_order<T: PartialOrd>(op: aicore::ast::BinOp, left: T, right: T) -> bool {
    use aicore::ast::BinOp;

    match op {
        BinOp::Lt => left < right,
        BinOp::Le => left <= right,
        BinOp::Gt => left > right,
        BinOp::Ge => left >= right,
        BinOp::Add
        | BinOp::Sub
        | BinOp::Mul
        | BinOp::Div
        | BinOp::Mod
        | BinOp::Eq
        | BinOp::Ne
        | BinOp::And
        | BinOp::Or => false,
    }
}

fn repl_expr_kind_name(kind: &aicore::ast::ExprKind) -> &'static str {
    match kind {
        aicore::ast::ExprKind::Int(_) => "integer literal",
        aicore::ast::ExprKind::Float(_) => "float literal",
        aicore::ast::ExprKind::Bool(_) => "bool literal",
        aicore::ast::ExprKind::String(_) => "string literal",
        aicore::ast::ExprKind::Unit => "unit literal",
        aicore::ast::ExprKind::Var(_) => "variable",
        aicore::ast::ExprKind::Call { .. } => "function call",
        aicore::ast::ExprKind::Closure { .. } => "closure",
        aicore::ast::ExprKind::If { .. } => "if expression",
        aicore::ast::ExprKind::While { .. } => "while expression",
        aicore::ast::ExprKind::Loop { .. } => "loop expression",
        aicore::ast::ExprKind::Break { .. } => "break expression",
        aicore::ast::ExprKind::Continue => "continue expression",
        aicore::ast::ExprKind::Match { .. } => "match expression",
        aicore::ast::ExprKind::Binary { .. } => "binary expression",
        aicore::ast::ExprKind::Unary { .. } => "unary expression",
        aicore::ast::ExprKind::Borrow { .. } => "borrow expression",
        aicore::ast::ExprKind::Await { .. } => "await expression",
        aicore::ast::ExprKind::Try { .. } => "try expression",
        aicore::ast::ExprKind::UnsafeBlock { .. } => "unsafe block",
        aicore::ast::ExprKind::StructInit { .. } => "struct initializer",
        aicore::ast::ExprKind::FieldAccess { .. } => "field access",
    }
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

fn build_file(
    input: &Path,
    output: &Path,
    offline: bool,
    runtime: RuntimeInstrumentationOptions,
) -> anyhow::Result<i32> {
    let project_root = resolve_project_root(input);
    let link = resolve_native_link_options(&project_root)?;
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

    let work = fresh_work_dir("run");
    compile_with_clang_artifact_with_options_and_runtime(
        &llvm.llvm_ir,
        output,
        &work,
        ArtifactKind::Exe,
        CompileOptions {
            debug_info: false,
            target_triple: None,
            static_link: false,
            link,
        },
        runtime,
    )?;
    Ok(EXIT_OK)
}

fn typed_holes_response(holes: Vec<aicore::typecheck::TypedHole>) -> CheckHolesResponse {
    let files = holes
        .iter()
        .map(|hole| hole.file.clone())
        .collect::<BTreeSet<_>>();
    let multiple_files = files.len() > 1;

    let mut source_cache = BTreeMap::<String, String>::new();
    let mut entries = holes
        .into_iter()
        .map(|hole| {
            let source = source_cache
                .entry(hole.file.clone())
                .or_insert_with(|| fs::read_to_string(&hole.file).unwrap_or_default());
            let line = line_number_for_offset(source, hole.span.start);
            let context = if multiple_files {
                format!("{}: {}", hole.file, hole.context)
            } else {
                hole.context
            };
            CheckHoleEntry {
                line,
                inferred: hole.inferred,
                context,
            }
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries.dedup();
    CheckHolesResponse { holes: entries }
}

fn line_number_for_offset(source: &str, offset: usize) -> usize {
    if source.is_empty() {
        return 1;
    }
    let bytes = source.as_bytes();
    let upto = offset.min(bytes.len());
    1 + bytes[..upto].iter().filter(|&&byte| byte == b'\n').count()
}

fn collect_check_data(
    input: &Path,
    offline: bool,
    warn_unused: bool,
) -> anyhow::Result<(Vec<Diagnostic>, Vec<aicore::typecheck::TypedHole>)> {
    if input.is_dir() {
        match workspace_build_plan(input) {
            Ok(Some(plan)) => {
                let mut all = Vec::new();
                let mut holes = Vec::new();
                for member in &plan.members {
                    let entry = member.root.join(&member.main);
                    let front = run_frontend_with_options(&entry, FrontendOptions { offline })?;
                    let (diagnostics, member_holes) =
                        finalize_frontend_output(front, warn_unused, &entry);
                    all.extend(diagnostics);
                    holes.extend(member_holes);
                }
                Ok((all, holes))
            }
            Ok(None) => {
                let front = run_frontend_with_options(input, FrontendOptions { offline })?;
                Ok(finalize_frontend_output(
                    front,
                    warn_unused,
                    &unused_warning_source_path(input),
                ))
            }
            Err(diag) => Ok((vec![diag], Vec::new())),
        }
    } else {
        let front = run_frontend_with_options(input, FrontendOptions { offline })?;
        Ok(finalize_frontend_output(front, warn_unused, input))
    }
}

fn finalize_frontend_output(
    mut front: FrontendOutput,
    warn_unused: bool,
    source_path: &Path,
) -> (Vec<Diagnostic>, Vec<aicore::typecheck::TypedHole>) {
    if warn_unused {
        if let Ok(source) = fs::read_to_string(source_path) {
            let source_file = source_path.to_string_lossy().to_string();
            let warnings = aicore::unused_analysis::analyze_unused_warnings(
                &front.ast,
                &front.resolution,
                &front.item_modules,
                &source_file,
                &source,
            );
            front.diagnostics.extend(warnings);
            sort_diagnostics(&mut front.diagnostics);
        }
    }
    (front.diagnostics, front.typecheck.holes)
}

fn unused_warning_source_path(input: &Path) -> PathBuf {
    if input.is_dir() {
        input.join("src/main.aic")
    } else {
        input.to_path_buf()
    }
}

fn collect_diagnostics(
    input: &Path,
    offline: bool,
    warn_unused: bool,
) -> anyhow::Result<Vec<Diagnostic>> {
    let (diagnostics, _) = collect_check_data(input, offline, warn_unused)?;
    Ok(diagnostics)
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

fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn process_built_artifact(
    input: &Path,
    output: &Path,
    artifact: BuildArtifact,
    verify_hash: Option<&str>,
    manifest_path: &Path,
    target: &str,
    static_link: bool,
) -> anyhow::Result<Option<String>> {
    let output_sha256 = build_output_sha256(output)?;
    if let Some(expected) = verify_hash {
        if !expected.eq_ignore_ascii_case(&output_sha256) {
            return Ok(Some(format!(
                "--verify-hash mismatch: expected {expected}, got {output_sha256}"
            )));
        }
    }

    let content_addressed_path = content_addressed_artifact_path(output, artifact, &output_sha256);
    materialize_content_addressed_artifact(output, &content_addressed_path)?;
    write_build_manifest(
        manifest_path,
        input,
        output,
        artifact,
        &output_sha256,
        &content_addressed_path,
        target,
        static_link,
    )?;
    Ok(None)
}

fn build_output_sha256(output: &Path) -> anyhow::Result<String> {
    use sha2::Digest;
    let payload = std::fs::read(output)?;
    let mut hasher = sha2::Sha256::new();
    hasher.update(payload);
    Ok(format!("{:x}", hasher.finalize()))
}

fn content_addressed_artifact_path(
    output: &Path,
    artifact: BuildArtifact,
    output_sha256: &str,
) -> PathBuf {
    let parent = output.parent().unwrap_or_else(|| std::path::Path::new("."));
    let file_name = output
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("artifact"));
    parent
        .join(".aic")
        .join("artifacts")
        .join(artifact.as_str())
        .join(output_sha256)
        .join(file_name)
}

fn materialize_content_addressed_artifact(
    output: &Path,
    content_addressed_path: &Path,
) -> anyhow::Result<()> {
    if let Some(parent) = content_addressed_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(output, content_addressed_path)?;
    Ok(())
}

fn write_build_manifest(
    manifest_path: &Path,
    input: &Path,
    output: &Path,
    artifact: BuildArtifact,
    output_sha256: &str,
    content_addressed_path: &Path,
    target: &str,
    static_link: bool,
) -> anyhow::Result<()> {
    if let Some(parent) = manifest_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let manifest = serde_json::json!({
        "input_path": input.to_string_lossy(),
        "output_path": output.to_string_lossy(),
        "output_sha256": output_sha256,
        "content_addressed_artifact_path": content_addressed_path.to_string_lossy(),
        "artifact_kind": artifact.as_str(),
        "target": target,
        "static_link": static_link,
    });
    let json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(manifest_path, format!("{json}\n"))?;
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

fn workspace_output_path(
    workspace_root: &Path,
    package_name: &str,
    entry: &Path,
    artifact: BuildArtifact,
) -> PathBuf {
    workspace_root
        .join("target/workspace")
        .join(package_name)
        .join(default_build_output_name(entry, artifact))
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

impl BuildArtifact {
    fn as_str(self) -> &'static str {
        match self {
            BuildArtifact::Exe => "exe",
            BuildArtifact::Obj => "obj",
            BuildArtifact::Lib => "lib",
        }
    }

    fn to_codegen(self) -> ArtifactKind {
        match self {
            BuildArtifact::Exe => ArtifactKind::Exe,
            BuildArtifact::Obj => ArtifactKind::Obj,
            BuildArtifact::Lib => ArtifactKind::Lib,
        }
    }
}

impl BuildTarget {
    fn canonical_label(self) -> &'static str {
        match self {
            BuildTarget::X8664Linux => "x86_64-linux",
            BuildTarget::Aarch64Linux => "aarch64-linux",
            BuildTarget::X8664Macos => "x86_64-macos",
            BuildTarget::Aarch64Macos => "aarch64-macos",
            BuildTarget::X8664Windows => "x86_64-windows",
        }
    }

    fn clang_triple(self) -> &'static str {
        match self {
            BuildTarget::X8664Linux => "x86_64-unknown-linux-gnu",
            BuildTarget::Aarch64Linux => "aarch64-unknown-linux-gnu",
            BuildTarget::X8664Macos => "x86_64-apple-darwin",
            BuildTarget::Aarch64Macos => "arm64-apple-darwin",
            BuildTarget::X8664Windows => "x86_64-pc-windows-msvc",
        }
    }

    fn supports_static_link(self) -> bool {
        matches!(self, BuildTarget::X8664Linux | BuildTarget::Aarch64Linux)
    }
}

fn host_build_target() -> Option<BuildTarget> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some(BuildTarget::X8664Linux),
        ("aarch64", "linux") => Some(BuildTarget::Aarch64Linux),
        ("x86_64", "macos") => Some(BuildTarget::X8664Macos),
        ("aarch64", "macos") => Some(BuildTarget::Aarch64Macos),
        ("x86_64", "windows") => Some(BuildTarget::X8664Windows),
        _ => None,
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
