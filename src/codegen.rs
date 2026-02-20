use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

use crate::ast::{BinOp, UnaryOp};
use crate::diagnostics::Diagnostic;
use crate::ir;

#[derive(Debug, Clone)]
struct FnSig {
    params: Vec<LType>,
    ret: LType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LType {
    Int,
    Bool,
    Unit,
    String,
    Struct(StructLayoutType),
    Enum(EnumLayoutType),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StructLayoutType {
    repr: String,
    fields: Vec<StructFieldType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StructFieldType {
    name: String,
    ty: LType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumLayoutType {
    repr: String,
    variants: Vec<EnumVariantType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumVariantType {
    name: String,
    payload: Option<LType>,
}

#[derive(Debug, Clone)]
struct StructTemplate {
    generics: Vec<String>,
    fields: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct EnumTemplate {
    generics: Vec<String>,
    variants: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone)]
struct VariantCtor {
    enum_name: String,
    variant_index: usize,
}

#[derive(Debug, Clone)]
struct GenericFnInstance {
    mangled: String,
    params: Vec<LType>,
    ret: LType,
    bindings: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct Value {
    ty: LType,
    repr: Option<String>,
}

#[derive(Debug, Clone)]
struct Local {
    ty: LType,
    ptr: String,
}

#[derive(Debug, Clone)]
struct SourceMap {
    line_starts: Vec<usize>,
    source_len: usize,
}

impl SourceMap {
    fn from_source(source: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }
        Self {
            line_starts,
            source_len: source.len(),
        }
    }

    fn line_col(&self, offset: usize) -> (u64, u64) {
        if self.line_starts.is_empty() {
            return (0, 0);
        }
        let max_offset = offset.min(self.source_len);
        let idx = self
            .line_starts
            .partition_point(|start| *start <= max_offset);
        let line_index = idx.saturating_sub(1);
        let line_start = self.line_starts[line_index];
        let line = (line_index + 1) as u64;
        let column = (max_offset.saturating_sub(line_start) + 1) as u64;
        (line, column)
    }
}

#[derive(Debug, Clone)]
struct DebugState {
    metadata: Vec<String>,
    file_id: usize,
    compile_unit_id: usize,
    subroutine_type_id: usize,
    next_id: usize,
}

impl DebugState {
    fn new(file: &str) -> Self {
        let path = Path::new(file);
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file);
        let directory = path
            .parent()
            .and_then(|dir| dir.to_str())
            .filter(|dir| !dir.is_empty())
            .unwrap_or(".");

        let file_name = escape_llvm_string(file_name);
        let directory = escape_llvm_string(directory);

        let compile_unit_id = 0usize;
        let file_id = 1usize;
        let empty_type_list_id = 2usize;
        let dwarf_flag_id = 3usize;
        let debug_version_flag_id = 4usize;
        let ident_id = 5usize;
        let subroutine_type_id = 6usize;

        let metadata = vec![
            format!("!llvm.dbg.cu = !{{!{compile_unit_id}}}"),
            format!("!llvm.module.flags = !{{!{dwarf_flag_id}, !{debug_version_flag_id}}}"),
            format!("!llvm.ident = !{{!{ident_id}}}"),
            format!(
                "!{compile_unit_id} = distinct !DICompileUnit(language: DW_LANG_C, file: !{file_id}, producer: \"aicore\", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug)"
            ),
            format!("!{file_id} = !DIFile(filename: \"{file_name}\", directory: \"{directory}\")"),
            format!("!{empty_type_list_id} = !{{}}"),
            format!("!{dwarf_flag_id} = !{{i32 2, !\"Dwarf Version\", i32 5}}"),
            format!("!{debug_version_flag_id} = !{{i32 2, !\"Debug Info Version\", i32 3}}"),
            format!("!{ident_id} = !{{!\"aicore\"}}"),
            format!("!{subroutine_type_id} = !DISubroutineType(types: !{empty_type_list_id})"),
        ];

        Self {
            metadata,
            file_id,
            compile_unit_id,
            subroutine_type_id,
            next_id: 7,
        }
    }

    fn next_metadata_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn push_node(&mut self, node_text: String) -> usize {
        let id = self.next_metadata_id();
        self.metadata.push(format!("!{id} = {node_text}"));
        id
    }

    fn new_subprogram(&mut self, source_name: &str, linkage_name: &str, line: u64) -> usize {
        let line = line.max(1);
        let source_name = escape_llvm_string(source_name);
        let linkage_name = escape_llvm_string(linkage_name);
        self.push_node(format!(
            "distinct !DISubprogram(name: \"{source_name}\", linkageName: \"{linkage_name}\", scope: !{}, file: !{}, line: {}, type: !{}, scopeLine: {}, spFlags: DISPFlagDefinition, unit: !{})",
            self.file_id,
            self.file_id,
            line,
            self.subroutine_type_id,
            line,
            self.compile_unit_id
        ))
    }

    fn new_location(&mut self, line: u64, column: u64, scope: usize) -> usize {
        let line = line.max(1);
        let column = column.max(1);
        self.push_node(format!(
            "!DILocation(line: {line}, column: {column}, scope: !{scope})"
        ))
    }
}

pub struct CodegenOutput {
    pub llvm_ir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodegenOptions {
    pub debug_info: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    Exe,
    Obj,
    Lib,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompileOptions {
    pub debug_info: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainInfo {
    pub clang_version: String,
    pub llvm_major: u32,
}

pub const MIN_SUPPORTED_LLVM_MAJOR: u32 = 14;

pub fn emit_llvm(program: &ir::Program, file: &str) -> Result<CodegenOutput, Vec<Diagnostic>> {
    emit_llvm_with_options(program, file, CodegenOptions::default())
}

pub fn emit_llvm_with_options(
    program: &ir::Program,
    file: &str,
    options: CodegenOptions,
) -> Result<CodegenOutput, Vec<Diagnostic>> {
    let mut gen = Generator::new(program, file, options);
    gen.generate();
    if !gen.diagnostics.is_empty() {
        return Err(gen.diagnostics);
    }
    Ok(CodegenOutput {
        llvm_ir: gen.finish(),
    })
}

pub fn compile_with_clang(
    llvm_ir: &str,
    output_path: &Path,
    work_dir: &Path,
) -> anyhow::Result<PathBuf> {
    compile_with_clang_artifact_with_options(
        llvm_ir,
        output_path,
        work_dir,
        ArtifactKind::Exe,
        CompileOptions::default(),
    )
}

pub fn compile_with_clang_artifact(
    llvm_ir: &str,
    output_path: &Path,
    work_dir: &Path,
    artifact: ArtifactKind,
) -> anyhow::Result<PathBuf> {
    compile_with_clang_artifact_with_options(
        llvm_ir,
        output_path,
        work_dir,
        artifact,
        CompileOptions::default(),
    )
}

pub fn compile_with_clang_artifact_with_options(
    llvm_ir: &str,
    output_path: &Path,
    work_dir: &Path,
    artifact: ArtifactKind,
    options: CompileOptions,
) -> anyhow::Result<PathBuf> {
    let toolchain = probe_toolchain()?;
    ensure_supported_toolchain(&toolchain)?;

    fs::create_dir_all(work_dir)?;
    ensure_parent_dir(output_path)?;

    let ll_path = work_dir.join("main.ll");
    let runtime_path = work_dir.join("runtime.c");
    let module_obj_path = work_dir.join("module.o");
    let runtime_obj_path = work_dir.join("runtime.o");

    fs::write(&ll_path, llvm_ir)?;
    fs::write(&runtime_path, runtime_c_source())?;

    match artifact {
        ArtifactKind::Exe => {
            let mut command = Command::new("clang");
            if options.debug_info {
                command.arg("-g");
            }
            command.arg("-O0").arg(&ll_path).arg(&runtime_path);
            if cfg!(not(target_os = "windows")) {
                command.arg("-pthread");
            }
            command.arg("-o").arg(output_path);
            run_checked_command(command, "clang", "building executable artifact")?;
        }
        ArtifactKind::Obj => {
            let mut command = Command::new("clang");
            if options.debug_info {
                command.arg("-g");
            }
            command
                .arg("-O0")
                .arg("-c")
                .arg(&ll_path)
                .arg("-o")
                .arg(output_path);
            run_checked_command(command, "clang", "building object artifact")?;
        }
        ArtifactKind::Lib => {
            let mut clang_module = Command::new("clang");
            if options.debug_info {
                clang_module.arg("-g");
            }
            clang_module
                .arg("-O0")
                .arg("-c")
                .arg(&ll_path)
                .arg("-o")
                .arg(&module_obj_path);
            run_checked_command(
                clang_module,
                "clang",
                "building module object for static library",
            )?;

            let mut clang_runtime = Command::new("clang");
            if options.debug_info {
                clang_runtime.arg("-g");
            }
            clang_runtime
                .arg("-O0")
                .arg("-c")
                .arg(&runtime_path)
                .arg("-o")
                .arg(&runtime_obj_path);
            run_checked_command(
                clang_runtime,
                "clang",
                "building runtime object for static library",
            )?;

            let ar_bin = std::env::var("AR").unwrap_or_else(|_| "ar".to_string());
            let mut ar = Command::new(&ar_bin);
            ar.arg("rcs")
                .arg(output_path)
                .arg(&module_obj_path)
                .arg(&runtime_obj_path);
            run_checked_command(ar, &ar_bin, "archiving static library artifact")?;
        }
    }

    Ok(output_path.to_path_buf())
}

fn ensure_parent_dir(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

fn probe_toolchain() -> anyhow::Result<ToolchainInfo> {
    let mut command = Command::new("clang");
    command.arg("--version");
    let output = command
        .output()
        .with_context(|| "failed to execute clang --version; ensure `clang` is in PATH")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("clang --version failed: {}", stderr.trim());
    }
    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    let Some(major) = parse_llvm_major(&raw) else {
        anyhow::bail!(
            "could not parse LLVM major version from clang --version output; output was: {}",
            raw.lines().next().unwrap_or("<empty>")
        );
    };
    Ok(ToolchainInfo {
        clang_version: raw,
        llvm_major: major,
    })
}

fn ensure_supported_toolchain(info: &ToolchainInfo) -> anyhow::Result<()> {
    let pinned_major = std::env::var("AIC_LLVM_PIN_MAJOR")
        .ok()
        .map(|value| {
            value.parse::<u32>().with_context(|| {
                format!("AIC_LLVM_PIN_MAJOR must be an integer major version, got '{value}'")
            })
        })
        .transpose()?;

    ensure_supported_toolchain_with_pin(info, pinned_major)
}

fn ensure_supported_toolchain_with_pin(
    info: &ToolchainInfo,
    pinned_major: Option<u32>,
) -> anyhow::Result<()> {
    if info.llvm_major < MIN_SUPPORTED_LLVM_MAJOR {
        anyhow::bail!(
            "unsupported LLVM/clang major version {}. Minimum supported major is {}. \
Install a newer clang or set AIC_LLVM_PIN_MAJOR to a supported major for reproducible builds.",
            info.llvm_major,
            MIN_SUPPORTED_LLVM_MAJOR
        );
    }

    if let Some(expected) = pinned_major {
        if info.llvm_major != expected {
            anyhow::bail!(
                "toolchain pin mismatch: AIC_LLVM_PIN_MAJOR={} but detected clang major {}. \
Install a matching clang or adjust AIC_LLVM_PIN_MAJOR.",
                expected,
                info.llvm_major
            );
        }
    }

    Ok(())
}

fn parse_llvm_major(version_output: &str) -> Option<u32> {
    for line in version_output.lines() {
        let marker = "version ";
        let Some(idx) = line.find(marker) else {
            continue;
        };
        let tail = &line[idx + marker.len()..];
        let digits = tail
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() {
            continue;
        }
        if let Ok(major) = digits.parse::<u32>() {
            return Some(major);
        }
    }
    None
}

fn run_checked_command(mut command: Command, tool: &str, action: &str) -> anyhow::Result<()> {
    let rendered = render_command(&command);
    let output = command.output().with_context(|| {
        format!("failed to execute {tool} while {action}; ensure `{tool}` is installed and in PATH")
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        format!("stderr: {stderr}")
    } else if !stdout.is_empty() {
        format!("stdout: {stdout}")
    } else {
        "no compiler output".to_string()
    };
    anyhow::bail!("{tool} failed while {action} ({rendered}); {detail}");
}

fn render_command(command: &Command) -> String {
    let mut out = command.get_program().to_string_lossy().to_string();
    for arg in command.get_args() {
        out.push(' ');
        out.push_str(&arg.to_string_lossy());
    }
    out
}

fn collect_type_templates(
    program: &ir::Program,
    type_map: &BTreeMap<ir::TypeId, String>,
) -> (
    BTreeMap<String, StructTemplate>,
    BTreeMap<String, EnumTemplate>,
    BTreeMap<String, Vec<VariantCtor>>,
) {
    let mut struct_templates = BTreeMap::new();
    let mut enum_templates = BTreeMap::new();

    for item in &program.items {
        match item {
            ir::Item::Struct(strukt) => {
                let fields = strukt
                    .fields
                    .iter()
                    .map(|field| {
                        let ty = type_map
                            .get(&field.ty)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string());
                        (field.name.clone(), ty)
                    })
                    .collect::<Vec<_>>();
                struct_templates.insert(
                    strukt.name.clone(),
                    StructTemplate {
                        generics: strukt.generics.iter().map(|g| g.name.clone()).collect(),
                        fields,
                    },
                );
            }
            ir::Item::Enum(enm) => {
                let variants = enm
                    .variants
                    .iter()
                    .map(|variant| {
                        let payload = variant.payload.and_then(|id| type_map.get(&id).cloned());
                        (variant.name.clone(), payload)
                    })
                    .collect::<Vec<_>>();
                enum_templates.insert(
                    enm.name.clone(),
                    EnumTemplate {
                        generics: enm.generics.iter().map(|g| g.name.clone()).collect(),
                        variants,
                    },
                );
            }
            _ => {}
        }
    }

    enum_templates
        .entry("Option".to_string())
        .or_insert_with(|| EnumTemplate {
            generics: vec!["T".to_string()],
            variants: vec![
                ("None".to_string(), None),
                ("Some".to_string(), Some("T".to_string())),
            ],
        });
    enum_templates
        .entry("Result".to_string())
        .or_insert_with(|| EnumTemplate {
            generics: vec!["T".to_string(), "E".to_string()],
            variants: vec![
                ("Ok".to_string(), Some("T".to_string())),
                ("Err".to_string(), Some("E".to_string())),
            ],
        });

    let mut variant_ctors: BTreeMap<String, Vec<VariantCtor>> = BTreeMap::new();
    for (enum_name, template) in &enum_templates {
        for (idx, (variant_name, _)) in template.variants.iter().enumerate() {
            variant_ctors
                .entry(variant_name.clone())
                .or_default()
                .push(VariantCtor {
                    enum_name: enum_name.clone(),
                    variant_index: idx,
                });
        }
    }
    for ctors in variant_ctors.values_mut() {
        ctors.sort_by(|a, b| {
            a.enum_name
                .cmp(&b.enum_name)
                .then(a.variant_index.cmp(&b.variant_index))
        });
    }

    (struct_templates, enum_templates, variant_ctors)
}

struct Generator<'a> {
    program: &'a ir::Program,
    file: &'a str,
    source_map: Option<SourceMap>,
    debug: Option<DebugState>,
    diagnostics: Vec<Diagnostic>,
    out: Vec<String>,
    globals: Vec<String>,
    string_counter: usize,
    temp_counter: usize,
    label_counter: usize,
    fn_sigs: BTreeMap<String, FnSig>,
    type_map: BTreeMap<ir::TypeId, String>,
    struct_templates: BTreeMap<String, StructTemplate>,
    enum_templates: BTreeMap<String, EnumTemplate>,
    variant_ctors: BTreeMap<String, Vec<VariantCtor>>,
    generic_fn_instances: BTreeMap<String, Vec<GenericFnInstance>>,
    active_type_bindings: Option<BTreeMap<String, String>>,
}

impl<'a> Generator<'a> {
    fn new(program: &'a ir::Program, file: &'a str, options: CodegenOptions) -> Self {
        let mut type_map = BTreeMap::new();
        for ty in &program.types {
            type_map.insert(ty.id, ty.repr.clone());
        }
        let (struct_templates, enum_templates, variant_ctors) =
            collect_type_templates(program, &type_map);
        let source_map = fs::read_to_string(file)
            .ok()
            .map(|source| SourceMap::from_source(&source));
        let debug = if options.debug_info {
            Some(DebugState::new(file))
        } else {
            None
        };
        Self {
            program,
            file,
            source_map,
            debug,
            diagnostics: Vec::new(),
            out: Vec::new(),
            globals: Vec::new(),
            string_counter: 0,
            temp_counter: 0,
            label_counter: 0,
            fn_sigs: BTreeMap::new(),
            type_map,
            struct_templates,
            enum_templates,
            variant_ctors,
            generic_fn_instances: BTreeMap::new(),
            active_type_bindings: None,
        }
    }

    fn finish(self) -> String {
        let mut text = String::new();
        text.push_str("; AICore LLVM IR (deterministic)\n");
        if self.debug.is_some() {
            let source_file = escape_llvm_string(self.file);
            text.push_str(&format!("source_filename = \"{}\"\n", source_file));
        }
        text.push_str("declare void @aic_rt_print_int(i64)\n");
        text.push_str("declare void @aic_rt_print_str(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_strlen(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_vec_len(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_vec_cap(i8*, i64, i64)\n");
        text.push_str("declare void @aic_rt_panic(i8*, i64, i64, i64, i64)\n\n");
        text.push_str("declare i64 @aic_rt_time_now_ms()\n");
        text.push_str("declare i64 @aic_rt_time_monotonic_ms()\n");
        text.push_str("declare void @aic_rt_time_sleep_ms(i64)\n\n");
        text.push_str("declare void @aic_rt_rand_seed(i64)\n");
        text.push_str("declare i64 @aic_rt_rand_next()\n");
        text.push_str("declare i64 @aic_rt_rand_range(i64, i64)\n\n");
        text.push_str("declare i64 @aic_rt_conc_spawn(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_join(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_cancel(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_channel_int(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_send_int(i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_conc_recv_int(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_close_channel(i64)\n");
        text.push_str("declare i64 @aic_rt_conc_mutex_int(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_mutex_lock(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_mutex_unlock(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_conc_mutex_close(i64)\n\n");
        text.push_str("declare i64 @aic_rt_fs_exists(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_read_text(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_write_text(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_append_text(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_copy(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_move(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_delete(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_metadata(i8*, i64, i64, i64*, i64*, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_walk_dir(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_temp_file(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_temp_dir(i8*, i64, i64, i8**, i64*)\n\n");
        text.push_str("declare i64 @aic_rt_env_get(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_env_set(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_env_remove(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_env_cwd(i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_env_set_cwd(i8*, i64, i64)\n\n");
        text.push_str("declare void @aic_rt_path_join(i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_path_basename(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_path_dirname(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_path_extension(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_path_is_abs(i8*, i64, i64)\n\n");
        text.push_str("declare i64 @aic_rt_proc_spawn(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_proc_wait(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_proc_kill(i64)\n");
        text.push_str(
            "declare i64 @aic_rt_proc_run(i8*, i64, i64, i64*, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_proc_pipe(i8*, i64, i64, i8*, i64, i64, i64*, i8**, i64*, i8**, i64*)\n\n",
        );
        text.push_str("declare i64 @aic_rt_net_tcp_listen(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_local_addr(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_accept(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_connect(i8*, i64, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_send(i64, i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_recv(i64, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_close(i64)\n");
        text.push_str("declare i64 @aic_rt_net_udp_bind(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_udp_local_addr(i64, i8**, i64*)\n");
        text.push_str(
            "declare i64 @aic_rt_net_udp_send_to(i64, i8*, i64, i64, i8*, i64, i64, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_net_udp_recv_from(i64, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str("declare i64 @aic_rt_net_udp_close(i64)\n");
        text.push_str("declare i64 @aic_rt_net_dns_lookup(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_dns_reverse(i8*, i64, i64, i8**, i64*)\n\n");
        text.push_str("declare i64 @aic_rt_regex_compile(i8*, i64, i64, i64)\n");
        text.push_str(
            "declare i64 @aic_rt_regex_is_match(i8*, i64, i64, i64, i8*, i64, i64, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_regex_find(i8*, i64, i64, i64, i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_regex_replace(i8*, i64, i64, i64, i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n\n",
        );

        for global in &self.globals {
            text.push_str(global);
            text.push('\n');
        }
        if !self.globals.is_empty() {
            text.push('\n');
        }

        for line in &self.out {
            text.push_str(&line);
            text.push('\n');
        }

        if let Some(debug) = &self.debug {
            if !self.out.is_empty() || !self.globals.is_empty() {
                text.push('\n');
            }
            for line in &debug.metadata {
                text.push_str(line);
                text.push('\n');
            }
        }
        text
    }

    fn generate(&mut self) {
        self.collect_fn_sigs();

        for item in &self.program.items {
            if let ir::Item::Function(func) = item {
                if func.generics.is_empty() {
                    self.gen_function(func);
                } else if let Some(instances) = self.generic_fn_instances.get(&func.name).cloned() {
                    for instance in instances {
                        self.gen_monomorphized_function(func, &instance);
                    }
                }
            }
        }

        self.gen_entry_wrapper();
    }

    fn collect_fn_sigs(&mut self) {
        let mut function_items = BTreeMap::new();
        for item in &self.program.items {
            if let ir::Item::Function(func) = item {
                function_items.insert(func.name.clone(), func);
                if !func.generics.is_empty() {
                    continue;
                }
                let params = func
                    .params
                    .iter()
                    .map(|p| self.type_from_id(p.ty, p.span))
                    .collect::<Option<Vec<_>>>();
                let ret = self.type_from_id(func.ret_type, func.span);
                if let (Some(params), Some(ret)) = (params, ret) {
                    self.fn_sigs
                        .insert(func.name.clone(), FnSig { params, ret });
                }
            }
        }

        for inst in self
            .program
            .generic_instantiations
            .iter()
            .filter(|inst| inst.kind == ir::GenericInstantiationKind::Function)
        {
            let Some(func) = function_items.get(&inst.name).copied() else {
                continue;
            };
            if func.generics.len() != inst.type_args.len() {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!(
                        "generic arity mismatch for function '{}': expected {}, found {}",
                        func.name,
                        func.generics.len(),
                        inst.type_args.len()
                    ),
                    self.file,
                    func.span,
                ));
                continue;
            }

            let bindings = func
                .generics
                .iter()
                .zip(inst.type_args.iter())
                .map(|(generic, arg)| (generic.name.clone(), arg.clone()))
                .collect::<BTreeMap<_, _>>();

            let params = func
                .params
                .iter()
                .map(|param| {
                    let raw = self
                        .type_map
                        .get(&param.ty)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string());
                    let concrete = substitute_type_vars(&raw, &bindings);
                    self.parse_type_repr(&concrete, param.span)
                })
                .collect::<Option<Vec<_>>>();
            let ret = {
                let raw = self
                    .type_map
                    .get(&func.ret_type)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string());
                let concrete = substitute_type_vars(&raw, &bindings);
                self.parse_type_repr(&concrete, func.span)
            };
            let (Some(params), Some(ret)) = (params, ret) else {
                continue;
            };

            self.generic_fn_instances
                .entry(func.name.clone())
                .or_default()
                .push(GenericFnInstance {
                    mangled: inst.mangled.clone(),
                    params,
                    ret,
                    bindings,
                });
        }
        for instances in self.generic_fn_instances.values_mut() {
            instances.sort_by(|a, b| a.mangled.cmp(&b.mangled));
            instances.dedup_by(|a, b| a.mangled == b.mangled);
        }

        self.fn_sigs.insert(
            "print_int".to_string(),
            FnSig {
                params: vec![LType::Int],
                ret: LType::Unit,
            },
        );
        self.fn_sigs.insert(
            "print_str".to_string(),
            FnSig {
                params: vec![LType::String],
                ret: LType::Unit,
            },
        );
        self.fn_sigs.insert(
            "len".to_string(),
            FnSig {
                params: vec![LType::String],
                ret: LType::Int,
            },
        );
        self.fn_sigs.insert(
            "panic".to_string(),
            FnSig {
                params: vec![LType::String],
                ret: LType::Unit,
            },
        );
    }

    fn gen_function(&mut self, func: &ir::Function) {
        let Some(sig) = self.fn_sigs.get(&func.name).cloned() else {
            return;
        };
        self.gen_function_with_signature(func, &sig, &mangle(&func.name), None);
    }

    fn gen_monomorphized_function(&mut self, func: &ir::Function, inst: &GenericFnInstance) {
        let sig = FnSig {
            params: inst.params.clone(),
            ret: inst.ret.clone(),
        };
        self.gen_function_with_signature(
            func,
            &sig,
            &mangle(&inst.mangled),
            Some(inst.bindings.clone()),
        );
    }

    fn gen_function_with_signature(
        &mut self,
        func: &ir::Function,
        sig: &FnSig,
        llvm_name: &str,
        bindings: Option<BTreeMap<String, String>>,
    ) {
        let previous_bindings = self.active_type_bindings.clone();
        self.active_type_bindings = bindings;

        let llvm_ret = llvm_type(&sig.ret);
        let mut param_defs = Vec::new();
        for (idx, ty) in sig.params.iter().enumerate() {
            param_defs.push(format!("{} %arg{}", llvm_type(ty), idx));
        }

        let (line, _) = self.span_line_col(func.span);
        let debug_scope = self
            .debug
            .as_mut()
            .map(|debug| debug.new_subprogram(&func.name, llvm_name, line));

        if let Some(scope) = debug_scope {
            self.out.push(format!(
                "define {} @{}({}) !dbg !{} {{",
                llvm_ret,
                llvm_name,
                param_defs.join(", "),
                scope
            ));
        } else {
            self.out.push(format!(
                "define {} @{}({}) {{",
                llvm_ret,
                llvm_name,
                param_defs.join(", ")
            ));
        }

        let mut fctx = FnCtx {
            lines: Vec::new(),
            vars: vec![BTreeMap::new()],
            terminated: false,
            current_label: "entry".to_string(),
            ret_ty: sig.ret.clone(),
            debug_scope,
        };
        fctx.lines.push("entry:".to_string());

        for (idx, param) in func.params.iter().enumerate() {
            let Some(ty) = sig.params.get(idx).cloned() else {
                continue;
            };
            let ptr = self.new_temp();
            fctx.lines
                .push(format!("  {} = alloca {}", ptr, llvm_type(&ty)));
            fctx.lines.push(format!(
                "  store {} %arg{}, {}* {}",
                llvm_type(&ty),
                idx,
                llvm_type(&ty),
                ptr
            ));
            fctx.vars
                .last_mut()
                .expect("scope")
                .insert(param.name.clone(), Local { ty, ptr });
        }

        let tail = self.gen_block(&func.body, &mut fctx);

        if !fctx.terminated {
            match sig.ret {
                LType::Unit => fctx.lines.push("  ret void".to_string()),
                _ => {
                    if let Some(value) = tail {
                        if value.ty == sig.ret {
                            fctx.lines.push(format!(
                                "  ret {} {}",
                                llvm_type(&value.ty),
                                value.repr.unwrap_or_else(|| default_value(&value.ty))
                            ));
                        } else {
                            self.diagnostics.push(Diagnostic::error(
                                "E5007",
                                format!("function '{}' return type mismatch in codegen", func.name),
                                self.file,
                                func.span,
                            ));
                            fctx.lines.push(format!(
                                "  ret {} {}",
                                llvm_type(&sig.ret),
                                default_value(&sig.ret)
                            ));
                        }
                    } else {
                        fctx.lines.push(format!(
                            "  ret {} {}",
                            llvm_type(&sig.ret),
                            default_value(&sig.ret)
                        ));
                    }
                }
            }
        }

        self.out.extend(fctx.lines);
        self.out.push("}".to_string());
        self.out.push(String::new());
        self.active_type_bindings = previous_bindings;
    }

    fn gen_entry_wrapper(&mut self) {
        let Some(main_sig) = self.fn_sigs.get("main").cloned() else {
            return;
        };
        self.out.push("define i32 @main() {".to_string());
        self.out.push("entry:".to_string());
        match main_sig.ret {
            LType::Int => {
                let r = self.new_temp();
                let c = self.new_temp();
                self.out.push(format!("  {} = call i64 @aic_main()", r));
                self.out.push(format!("  {} = trunc i64 {} to i32", c, r));
                self.out.push(format!("  ret i32 {}", c));
            }
            LType::Bool => {
                let r = self.new_temp();
                let c = self.new_temp();
                self.out.push(format!("  {} = call i1 @aic_main()", r));
                self.out.push(format!("  {} = zext i1 {} to i32", c, r));
                self.out.push(format!("  ret i32 {}", c));
            }
            LType::Unit => {
                self.out.push("  call void @aic_main()".to_string());
                self.out.push("  ret i32 0".to_string());
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E5020",
                    "main must return Int, Bool, or () in MVP backend",
                    self.file,
                    crate::span::Span::new(0, 0),
                ));
                self.out.push("  ret i32 1".to_string());
            }
        }
        self.out.push("}".to_string());
        self.out.push(String::new());
    }

    fn gen_block(&mut self, block: &ir::Block, fctx: &mut FnCtx) -> Option<Value> {
        fctx.vars.push(BTreeMap::new());

        for stmt in &block.stmts {
            if fctx.terminated {
                break;
            }
            match stmt {
                ir::Stmt::Let {
                    name,
                    ty,
                    expr,
                    span,
                    ..
                } => {
                    let value = self.gen_expr(expr, fctx);
                    let Some(value) = value else { continue };
                    let expected = if let Some(ty) = ty {
                        self.type_from_id(*ty, *span)
                    } else {
                        Some(value.ty.clone())
                    };
                    let Some(expected) = expected else {
                        continue;
                    };
                    let ptr = self.new_temp();
                    fctx.lines
                        .push(format!("  {} = alloca {}", ptr, llvm_type(&expected)));
                    let repr = coerce_repr(&value, &expected);
                    fctx.lines.push(format!(
                        "  store {} {}, {}* {}",
                        llvm_type(&expected),
                        repr,
                        llvm_type(&expected),
                        ptr
                    ));
                    fctx.vars
                        .last_mut()
                        .expect("scope")
                        .insert(name.clone(), Local { ty: expected, ptr });
                }
                ir::Stmt::Assign { target, expr, span } => {
                    let Some(local) = find_local(&fctx.vars, target) else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5001",
                            format!(
                                "unknown local variable '{}' during assignment codegen",
                                target
                            ),
                            self.file,
                            *span,
                        ));
                        continue;
                    };
                    let Some(value) = self.gen_expr(expr, fctx) else {
                        continue;
                    };
                    if value.ty != local.ty {
                        self.diagnostics.push(Diagnostic::error(
                            "E5007",
                            format!(
                                "assignment codegen type mismatch for '{}': expected '{}', found '{}'",
                                target,
                                render_type(&local.ty),
                                render_type(&value.ty)
                            ),
                            self.file,
                            *span,
                        ));
                    }
                    let repr = coerce_repr(&value, &local.ty);
                    fctx.lines.push(format!(
                        "  store {} {}, {}* {}",
                        llvm_type(&local.ty),
                        repr,
                        llvm_type(&local.ty),
                        local.ptr
                    ));
                }
                ir::Stmt::Expr { expr, .. } => {
                    let _ = self.gen_expr(expr, fctx);
                }
                ir::Stmt::Return { expr, .. } => {
                    if let Some(expr) = expr {
                        if let Some(value) = self.gen_expr(expr, fctx) {
                            let repr = value.repr.unwrap_or_else(|| default_value(&value.ty));
                            fctx.lines
                                .push(format!("  ret {} {}", llvm_type(&value.ty), repr));
                            fctx.terminated = true;
                        }
                    } else {
                        fctx.lines.push("  ret void".to_string());
                        fctx.terminated = true;
                    }
                }
                ir::Stmt::Assert { expr, message, .. } => {
                    if let Some(cond) = self.gen_expr(expr, fctx) {
                        if cond.ty != LType::Bool {
                            self.diagnostics.push(Diagnostic::error(
                                "E5008",
                                "assert lowered with non-bool expression",
                                self.file,
                                expr.span,
                            ));
                            continue;
                        }
                        let cond_repr = cond.repr.unwrap_or_else(|| "0".to_string());
                        let ok_label = self.new_label("assert_ok");
                        let fail_label = self.new_label("assert_fail");
                        fctx.lines.push(format!(
                            "  br i1 {}, label %{}, label %{}",
                            cond_repr, ok_label, fail_label
                        ));
                        fctx.lines.push(format!("{}:", fail_label));
                        let msg = self.string_literal(message, fctx);
                        if let Some((ptr, len, cap)) = self.string_parts(&msg, expr.span, fctx) {
                            self.emit_panic_call(&ptr, &len, &cap, expr.span, fctx);
                        }
                        fctx.lines.push("  unreachable".to_string());
                        fctx.lines.push(format!("{}:", ok_label));
                        fctx.current_label = ok_label;
                    }
                }
            }
        }

        let tail = if !fctx.terminated {
            if let Some(expr) = &block.tail {
                self.gen_expr(expr, fctx)
            } else {
                Some(Value {
                    ty: LType::Unit,
                    repr: None,
                })
            }
        } else {
            None
        };

        fctx.vars.pop();
        tail
    }

    fn gen_expr(&mut self, expr: &ir::Expr, fctx: &mut FnCtx) -> Option<Value> {
        match &expr.kind {
            ir::ExprKind::Int(v) => Some(Value {
                ty: LType::Int,
                repr: Some(v.to_string()),
            }),
            ir::ExprKind::Bool(v) => Some(Value {
                ty: LType::Bool,
                repr: Some(if *v { "1".to_string() } else { "0".to_string() }),
            }),
            ir::ExprKind::String(s) => Some(self.string_literal(s, fctx)),
            ir::ExprKind::Unit => Some(Value {
                ty: LType::Unit,
                repr: None,
            }),
            ir::ExprKind::Var(name) => {
                if let Some(local) = find_local(&fctx.vars, name) {
                    let reg = self.new_temp();
                    fctx.lines.push(format!(
                        "  {} = load {}, {}* {}",
                        reg,
                        llvm_type(&local.ty),
                        llvm_type(&local.ty),
                        local.ptr
                    ));
                    Some(Value {
                        ty: local.ty,
                        repr: Some(reg),
                    })
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        "E5001",
                        format!("unknown local variable '{}' during codegen", name),
                        self.file,
                        expr.span,
                    ));
                    None
                }
            }
            ir::ExprKind::Unary { op, expr: inner } => {
                let value = self.gen_expr(inner, fctx)?;
                match (op, value.ty.clone()) {
                    (UnaryOp::Neg, LType::Int) => {
                        let reg = self.new_temp();
                        let repr = value.repr.unwrap_or_else(|| "0".to_string());
                        fctx.lines.push(format!("  {} = sub i64 0, {}", reg, repr));
                        Some(Value {
                            ty: LType::Int,
                            repr: Some(reg),
                        })
                    }
                    (UnaryOp::Not, LType::Bool) => {
                        let reg = self.new_temp();
                        let repr = value.repr.unwrap_or_else(|| "0".to_string());
                        fctx.lines
                            .push(format!("  {} = xor i1 {}, true", reg, repr));
                        Some(Value {
                            ty: LType::Bool,
                            repr: Some(reg),
                        })
                    }
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E5002",
                            "unsupported unary operation in codegen",
                            self.file,
                            expr.span,
                        ));
                        None
                    }
                }
            }
            ir::ExprKind::Borrow { expr: inner, .. } => self.gen_expr(inner, fctx),
            ir::ExprKind::Await { expr: inner } => self.gen_expr(inner, fctx),
            ir::ExprKind::Try { expr: inner } => self.gen_try(inner, expr.span, fctx),
            ir::ExprKind::Binary { op, lhs, rhs } => {
                let lv = self.gen_expr(lhs, fctx)?;
                let rv = self.gen_expr(rhs, fctx)?;
                self.gen_binary(*op, lv, rv, fctx, expr.span)
            }
            ir::ExprKind::Call { callee, args } => {
                let Some(path) = extract_callee_path(callee) else {
                    self.diagnostics.push(Diagnostic::error(
                        "E5003",
                        "codegen expects callable names or qualified paths",
                        self.file,
                        callee.span,
                    ));
                    return None;
                };
                let Some(name) = path.last() else {
                    self.diagnostics.push(Diagnostic::error(
                        "E5003",
                        "callee path cannot be empty",
                        self.file,
                        callee.span,
                    ));
                    return None;
                };
                self.gen_call(name, args, expr.span, fctx)
            }
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => self.gen_if(cond, then_block, else_block, fctx),
            ir::ExprKind::Match {
                expr: scrutinee,
                arms,
            } => self.gen_match(scrutinee, arms, fctx),
            ir::ExprKind::StructInit { name, fields } => {
                self.gen_struct_init(name, fields, expr.span, fctx)
            }
            ir::ExprKind::FieldAccess { base, field } => {
                self.gen_field_access(base, field, expr.span, fctx)
            }
        }
    }

    fn gen_binary(
        &mut self,
        op: BinOp,
        lhs: Value,
        rhs: Value,
        fctx: &mut FnCtx,
        span: crate::span::Span,
    ) -> Option<Value> {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                if lhs.ty != LType::Int || rhs.ty != LType::Int {
                    self.diagnostics.push(Diagnostic::error(
                        "E5006",
                        "arithmetic codegen only supports Int",
                        self.file,
                        span,
                    ));
                    return None;
                }
                let inst = match op {
                    BinOp::Add => "add",
                    BinOp::Sub => "sub",
                    BinOp::Mul => "mul",
                    BinOp::Div => "sdiv",
                    BinOp::Mod => "srem",
                    _ => unreachable!(),
                };
                let reg = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = {} i64 {}, {}",
                    reg,
                    inst,
                    lhs.repr.unwrap_or_else(|| "0".to_string()),
                    rhs.repr.unwrap_or_else(|| "0".to_string())
                ));
                Some(Value {
                    ty: LType::Int,
                    repr: Some(reg),
                })
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let (cmp, ty) = match (&lhs.ty, &rhs.ty) {
                    (LType::Int, LType::Int) => {
                        let cmp = match op {
                            BinOp::Eq => "eq",
                            BinOp::Ne => "ne",
                            BinOp::Lt => "slt",
                            BinOp::Le => "sle",
                            BinOp::Gt => "sgt",
                            BinOp::Ge => "sge",
                            _ => unreachable!(),
                        };
                        (cmp, "i64")
                    }
                    (LType::Bool, LType::Bool) if matches!(op, BinOp::Eq | BinOp::Ne) => {
                        let cmp = if matches!(op, BinOp::Eq) { "eq" } else { "ne" };
                        (cmp, "i1")
                    }
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E5006",
                            "comparison codegen type mismatch",
                            self.file,
                            span,
                        ));
                        return None;
                    }
                };
                let reg = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = icmp {} {} {}, {}",
                    reg,
                    cmp,
                    ty,
                    lhs.repr.unwrap_or_else(|| default_value(&lhs.ty)),
                    rhs.repr.unwrap_or_else(|| default_value(&rhs.ty))
                ));
                Some(Value {
                    ty: LType::Bool,
                    repr: Some(reg),
                })
            }
            BinOp::And | BinOp::Or => {
                if lhs.ty != LType::Bool || rhs.ty != LType::Bool {
                    self.diagnostics.push(Diagnostic::error(
                        "E5006",
                        "logical codegen only supports Bool",
                        self.file,
                        span,
                    ));
                    return None;
                }
                let reg = self.new_temp();
                let op_str = if matches!(op, BinOp::And) {
                    "and"
                } else {
                    "or"
                };
                fctx.lines.push(format!(
                    "  {} = {} i1 {}, {}",
                    reg,
                    op_str,
                    lhs.repr.unwrap_or_else(|| "0".to_string()),
                    rhs.repr.unwrap_or_else(|| "0".to_string())
                ));
                Some(Value {
                    ty: LType::Bool,
                    repr: Some(reg),
                })
            }
        }
    }

    fn gen_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if let Some(value) = self.gen_variant_constructor(name, args, span, fctx) {
            return value;
        }

        if name == "print_int" {
            if args.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E5010",
                    "print_int expects one argument",
                    self.file,
                    span,
                ));
                return None;
            }
            let arg = self.gen_expr(&args[0], fctx)?;
            if arg.ty != LType::Int {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    "print_int expects Int",
                    self.file,
                    args[0].span,
                ));
                return None;
            }
            fctx.lines.push(format!(
                "  call void @aic_rt_print_int(i64 {})",
                arg.repr.unwrap_or_else(|| "0".to_string())
            ));
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        if name == "print_str" {
            if args.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E5010",
                    "print_str expects one argument",
                    self.file,
                    span,
                ));
                return None;
            }
            let arg = self.gen_expr(&args[0], fctx)?;
            if arg.ty != LType::String {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    "print_str expects String",
                    self.file,
                    args[0].span,
                ));
                return None;
            }
            let (ptr, len, cap) = self.string_parts(&arg, args[0].span, fctx)?;
            fctx.lines.push(format!(
                "  call void @aic_rt_print_str(i8* {}, i64 {}, i64 {})",
                ptr, len, cap
            ));
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        if name == "len" {
            if args.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E5010",
                    "len expects one argument",
                    self.file,
                    span,
                ));
                return None;
            }
            let arg = self.gen_expr(&args[0], fctx)?;
            if arg.ty != LType::String {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    "len expects String",
                    self.file,
                    args[0].span,
                ));
                return None;
            }
            let (ptr, len, cap) = self.string_parts(&arg, args[0].span, fctx)?;
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call i64 @aic_rt_strlen(i8* {}, i64 {}, i64 {})",
                reg, ptr, len, cap
            ));
            return Some(Value {
                ty: LType::Int,
                repr: Some(reg),
            });
        }

        if name == "panic" {
            if args.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E5010",
                    "panic expects one argument",
                    self.file,
                    span,
                ));
                return None;
            }
            let arg = self.gen_expr(&args[0], fctx)?;
            if arg.ty != LType::String {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    "panic expects String",
                    self.file,
                    args[0].span,
                ));
                return None;
            }
            let (ptr, len, cap) = self.string_parts(&arg, args[0].span, fctx)?;
            self.emit_panic_call(&ptr, &len, &cap, args[0].span, fctx);
            fctx.lines.push("  unreachable".to_string());
            fctx.terminated = true;
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        if let Some(result) = self.gen_time_builtin_call(name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_rand_builtin_call(name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_concurrency_builtin_call(name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_fs_builtin_call(name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_env_builtin_call(name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_path_builtin_call(name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_proc_builtin_call(name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_net_builtin_call(name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_regex_builtin_call(name, args, span, fctx) {
            return result;
        }

        if let Some(instances) = self.generic_fn_instances.get(name).cloned() {
            let mut values = Vec::new();
            for expr in args {
                values.push(self.gen_expr(expr, fctx)?);
            }

            let selected = instances.into_iter().find(|inst| {
                inst.params.len() == values.len()
                    && inst
                        .params
                        .iter()
                        .zip(values.iter())
                        .all(|(expected, value)| *expected == value.ty)
            });

            let Some(instance) = selected else {
                self.diagnostics.push(Diagnostic::error(
                    "E5014",
                    format!("argument type mismatch for generic call to '{}'", name),
                    self.file,
                    span,
                ));
                return None;
            };

            let rendered_args = values
                .iter()
                .zip(instance.params.iter())
                .map(|(value, expected)| {
                    format!(
                        "{} {}",
                        llvm_type(expected),
                        value
                            .repr
                            .clone()
                            .unwrap_or_else(|| default_value(expected))
                    )
                })
                .collect::<Vec<_>>();

            let llvm_name = mangle(&instance.mangled);
            if instance.ret == LType::Unit {
                fctx.lines.push(format!(
                    "  call void @{}({})",
                    llvm_name,
                    rendered_args.join(", ")
                ));
                return Some(Value {
                    ty: LType::Unit,
                    repr: None,
                });
            }

            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call {} @{}({})",
                reg,
                llvm_type(&instance.ret),
                llvm_name,
                rendered_args.join(", ")
            ));
            return Some(Value {
                ty: instance.ret,
                repr: Some(reg),
            });
        }

        let Some(sig) = self.fn_sigs.get(name).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{}' in codegen", name),
                self.file,
                span,
            ));
            return None;
        };

        if args.len() != sig.params.len() {
            self.diagnostics.push(Diagnostic::error(
                "E5013",
                format!(
                    "call to '{}' arity mismatch: expected {}, got {}",
                    name,
                    sig.params.len(),
                    args.len()
                ),
                self.file,
                span,
            ));
            return None;
        }

        let mut rendered_args = Vec::new();
        for (idx, expr) in args.iter().enumerate() {
            let value = self.gen_expr(expr, fctx)?;
            let expected = &sig.params[idx];
            if value.ty != *expected {
                self.diagnostics.push(Diagnostic::error(
                    "E5014",
                    format!("argument type mismatch for call to '{}'", name),
                    self.file,
                    expr.span,
                ));
                return None;
            }
            rendered_args.push(format!(
                "{} {}",
                llvm_type(expected),
                value.repr.unwrap_or_else(|| default_value(expected))
            ));
        }

        let mangled = mangle(name);
        if sig.ret == LType::Unit {
            fctx.lines.push(format!(
                "  call void @{}({})",
                mangled,
                rendered_args.join(", ")
            ));
            Some(Value {
                ty: LType::Unit,
                repr: None,
            })
        } else {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call {} @{}({})",
                reg,
                llvm_type(&sig.ret),
                mangled,
                rendered_args.join(", ")
            ));
            Some(Value {
                ty: sig.ret,
                repr: Some(reg),
            })
        }
    }

    fn sig_matches_shape(&self, name: &str, params: &[&str], ret: &str) -> bool {
        let Some(sig) = self.fn_sigs.get(name) else {
            return false;
        };
        if sig.params.len() != params.len() {
            return false;
        }
        if sig
            .params
            .iter()
            .zip(params.iter())
            .any(|(actual, expected)| render_type(actual) != *expected)
        {
            return false;
        }
        render_type(&sig.ret) == ret
    }

    fn gen_time_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "now_ms" | "aic_time_now_ms_intrinsic" => "now_ms",
            "monotonic_ms" | "aic_time_monotonic_ms_intrinsic" => "monotonic_ms",
            "sleep_ms" | "aic_time_sleep_ms_intrinsic" => "sleep_ms",
            _ => return None,
        };

        match canonical {
            "now_ms" if self.sig_matches_shape(name, &[], "Int") => Some(Some(
                self.gen_time_now_call("aic_rt_time_now_ms", span, fctx),
            )),
            "monotonic_ms" if self.sig_matches_shape(name, &[], "Int") => Some(Some(
                self.gen_time_now_call("aic_rt_time_monotonic_ms", span, fctx),
            )),
            "sleep_ms" if self.sig_matches_shape(name, &["Int"], "()") => {
                Some(self.gen_time_sleep_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    fn gen_rand_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "seed" | "aic_rand_seed_intrinsic" => "seed",
            "random_int" | "aic_rand_int_intrinsic" => "random_int",
            "random_range" | "aic_rand_range_intrinsic" => "random_range",
            _ => return None,
        };

        match canonical {
            "seed" if self.sig_matches_shape(name, &["Int"], "()") => {
                Some(self.gen_rand_seed_call(name, args, span, fctx))
            }
            "random_int" if self.sig_matches_shape(name, &[], "Int") => {
                Some(Some(self.gen_rand_next_call(span, fctx)))
            }
            "random_range" if self.sig_matches_shape(name, &["Int", "Int"], "Int") => {
                Some(self.gen_rand_range_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    fn gen_time_now_call(
        &mut self,
        runtime_fn: &str,
        _span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Value {
        let reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = call i64 @{}()", reg, runtime_fn));
        Value {
            ty: LType::Int,
            repr: Some(reg),
        }
    }

    fn gen_time_sleep_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let ms = self.gen_expr(&args[0], fctx)?;
        if ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        fctx.lines.push(format!(
            "  call void @aic_rt_time_sleep_ms(i64 {})",
            ms.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    fn gen_rand_seed_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let seed = self.gen_expr(&args[0], fctx)?;
        if seed.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        fctx.lines.push(format!(
            "  call void @aic_rt_rand_seed(i64 {})",
            seed.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    fn gen_rand_next_call(&mut self, _span: crate::span::Span, fctx: &mut FnCtx) -> Value {
        let reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = call i64 @aic_rt_rand_next()", reg));
        Value {
            ty: LType::Int,
            repr: Some(reg),
        }
    }

    fn gen_rand_range_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects two arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let min_inclusive = self.gen_expr(&args[0], fctx)?;
        let max_exclusive = self.gen_expr(&args[1], fctx)?;
        if min_inclusive.ty != LType::Int || max_exclusive.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects (Int, Int)"),
                self.file,
                span,
            ));
            return None;
        }
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_rand_range(i64 {}, i64 {})",
            reg,
            min_inclusive
                .repr
                .clone()
                .unwrap_or_else(|| "0".to_string()),
            max_exclusive
                .repr
                .clone()
                .unwrap_or_else(|| "0".to_string())
        ));
        Some(Value {
            ty: LType::Int,
            repr: Some(reg),
        })
    }

    fn gen_concurrency_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "spawn_task" | "aic_conc_spawn_intrinsic" => "spawn_task",
            "join_task" | "aic_conc_join_intrinsic" => "join_task",
            "cancel_task" | "aic_conc_cancel_intrinsic" => "cancel_task",
            "channel_int" | "aic_conc_channel_int_intrinsic" => "channel_int",
            "send_int" | "aic_conc_send_int_intrinsic" => "send_int",
            "recv_int" | "aic_conc_recv_int_intrinsic" => "recv_int",
            "close_channel" | "aic_conc_close_channel_intrinsic" => "close_channel",
            "mutex_int" | "aic_conc_mutex_int_intrinsic" => "mutex_int",
            "lock_int" | "aic_conc_mutex_lock_intrinsic" => "lock_int",
            "unlock_int" | "aic_conc_mutex_unlock_intrinsic" => "unlock_int",
            "close_mutex" | "aic_conc_mutex_close_intrinsic" => "close_mutex",
            _ => return None,
        };

        match canonical {
            "spawn_task"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int"],
                    "Result[Task, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_spawn_task_call(name, args, span, fctx))
            }
            "join_task"
                if self.sig_matches_shape(name, &["Task"], "Result[Int, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_join_task_call(name, args, span, fctx))
            }
            "cancel_task"
                if self.sig_matches_shape(name, &["Task"], "Result[Bool, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_cancel_task_call(name, args, span, fctx))
            }
            "channel_int"
                if self.sig_matches_shape(
                    name,
                    &["Int"],
                    "Result[IntChannel, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_channel_int_call(name, args, span, fctx))
            }
            "send_int"
                if self.sig_matches_shape(
                    name,
                    &["IntChannel", "Int", "Int"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_send_int_call(name, args, span, fctx))
            }
            "recv_int"
                if self.sig_matches_shape(
                    name,
                    &["IntChannel", "Int"],
                    "Result[Int, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_recv_int_call(name, args, span, fctx))
            }
            "close_channel"
                if self.sig_matches_shape(
                    name,
                    &["IntChannel"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_close_channel_call(name, args, span, fctx))
            }
            "mutex_int"
                if self.sig_matches_shape(name, &["Int"], "Result[IntMutex, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_mutex_int_call(name, args, span, fctx))
            }
            "lock_int"
                if self.sig_matches_shape(
                    name,
                    &["IntMutex", "Int"],
                    "Result[Int, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_lock_int_call(name, args, span, fctx))
            }
            "unlock_int"
                if self.sig_matches_shape(
                    name,
                    &["IntMutex", "Int"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_unlock_int_call(name, args, span, fctx))
            }
            "close_mutex"
                if self.sig_matches_shape(
                    name,
                    &["IntMutex"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_close_mutex_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    fn concurrency_result_ty(&mut self, name: &str, span: crate::span::Span) -> Option<LType> {
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        Some(result_ty)
    }

    fn extract_named_handle_from_value(
        &mut self,
        value: &Value,
        expected_name: &str,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<String> {
        let LType::Struct(layout) = &value.ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects {expected_name}"),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != expected_name
            || layout.fields.len() != 1
            || layout.fields[0].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects {expected_name}"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            handle,
            llvm_type(&value.ty),
            value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&value.ty))
        ));
        Some(handle)
    }

    fn build_concurrency_ok_handle_payload(
        &mut self,
        result_ty: &LType,
        expected_name: &str,
        handle: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(result_ty, span) else {
            return None;
        };
        let LType::Struct(layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "concurrency builtin expects Result[{expected_name}, ConcurrencyError] return type"
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != expected_name
            || layout.fields.len() != 1
            || layout.fields[0].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "concurrency builtin expects Result[{expected_name}, ConcurrencyError] return type"
                ),
                self.file,
                span,
            ));
            return None;
        }
        self.build_struct_value(
            &layout,
            &[Value {
                ty: LType::Int,
                repr: Some(handle.to_string()),
            }],
            span,
            fctx,
        )
    }

    fn gen_concurrency_spawn_task_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "spawn_task expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        let delay_ms = self.gen_expr(&args[1], fctx)?;
        if value.ty != LType::Int || delay_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "spawn_task expects (Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_spawn(i64 {}, i64 {}, i64* {})",
            err,
            value.repr.clone().unwrap_or_else(|| "0".to_string()),
            delay_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload =
            self.build_concurrency_ok_handle_payload(&result_ty, "Task", &handle, span, fctx)?;
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_concurrency_join_task_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "join_task expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let task = self.gen_expr(&args[0], fctx)?;
        let handle =
            self.extract_named_handle_from_value(&task, "Task", "join_task", args[0].span, fctx)?;
        let value_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_join(i64 {}, i64* {})",
            err, handle, value_slot
        ));
        let out_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_value, value_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_value),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_concurrency_cancel_task_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "cancel_task expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let task = self.gen_expr(&args[0], fctx)?;
        let handle =
            self.extract_named_handle_from_value(&task, "Task", "cancel_task", args[0].span, fctx)?;
        let cancelled_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", cancelled_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_cancel(i64 {}, i64* {})",
            err, handle, cancelled_slot
        ));
        let cancelled_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            cancelled_raw, cancelled_slot
        ));
        let cancelled = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp ne i64 {}, 0",
            cancelled, cancelled_raw
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(cancelled),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_concurrency_channel_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "channel_int expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let capacity = self.gen_expr(&args[0], fctx)?;
        if capacity.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "channel_int expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_channel_int(i64 {}, i64* {})",
            err,
            capacity.repr.clone().unwrap_or_else(|| "0".to_string()),
            handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = self.build_concurrency_ok_handle_payload(
            &result_ty,
            "IntChannel",
            &handle,
            span,
            fctx,
        )?;
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_concurrency_send_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "send_int expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let channel = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &channel,
            "IntChannel",
            "send_int",
            args[0].span,
            fctx,
        )?;
        let value = self.gen_expr(&args[1], fctx)?;
        let timeout_ms = self.gen_expr(&args[2], fctx)?;
        if value.ty != LType::Int || timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "send_int expects (IntChannel, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_send_int(i64 {}, i64 {}, i64 {})",
            err,
            handle,
            value.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_concurrency_recv_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "recv_int expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let channel = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &channel,
            "IntChannel",
            "recv_int",
            args[0].span,
            fctx,
        )?;
        let timeout_ms = self.gen_expr(&args[1], fctx)?;
        if timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "recv_int expects (IntChannel, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let value_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_recv_int(i64 {}, i64 {}, i64* {})",
            err,
            handle,
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            value_slot
        ));
        let out_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_value, value_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_value),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_concurrency_close_channel_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "close_channel expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let channel = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &channel,
            "IntChannel",
            "close_channel",
            args[0].span,
            fctx,
        )?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_close_channel(i64 {})",
            err, handle
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_concurrency_mutex_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "mutex_int expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let initial = self.gen_expr(&args[0], fctx)?;
        if initial.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "mutex_int expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_mutex_int(i64 {}, i64* {})",
            err,
            initial.repr.clone().unwrap_or_else(|| "0".to_string()),
            handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload =
            self.build_concurrency_ok_handle_payload(&result_ty, "IntMutex", &handle, span, fctx)?;
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_concurrency_lock_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "lock_int expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let mutex = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &mutex,
            "IntMutex",
            "lock_int",
            args[0].span,
            fctx,
        )?;
        let timeout_ms = self.gen_expr(&args[1], fctx)?;
        if timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "lock_int expects (IntMutex, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let value_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_mutex_lock(i64 {}, i64 {}, i64* {})",
            err,
            handle,
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            value_slot
        ));
        let out_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_value, value_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_value),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_concurrency_unlock_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "unlock_int expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let mutex = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &mutex,
            "IntMutex",
            "unlock_int",
            args[0].span,
            fctx,
        )?;
        let value = self.gen_expr(&args[1], fctx)?;
        if value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "unlock_int expects (IntMutex, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_mutex_unlock(i64 {}, i64 {})",
            err,
            handle,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_concurrency_close_mutex_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "close_mutex expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let mutex = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &mutex,
            "IntMutex",
            "close_mutex",
            args[0].span,
            fctx,
        )?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_mutex_close(i64 {})",
            err, handle
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn wrap_concurrency_result(
        &mut self,
        result_ty: &LType,
        ok_payload: Value,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(result_ty, span)
        else {
            return None;
        };
        if ok_payload.ty != ok_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "concurrency builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_concurrency_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("conc_ok");
        let err_label = self.new_label("conc_err");
        let cont_label = self.new_label("conc_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            ok_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", err_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(result_ty),
            llvm_type(result_ty),
            slot
        ));
        Some(Value {
            ty: result_ty.clone(),
            repr: Some(reg),
        })
    }

    fn gen_fs_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "exists" | "aic_fs_exists_intrinsic" => "exists",
            "read_text" | "aic_fs_read_text_intrinsic" => "read_text",
            "write_text" | "aic_fs_write_text_intrinsic" => "write_text",
            "append_text" | "aic_fs_append_text_intrinsic" => "append_text",
            "copy" | "aic_fs_copy_intrinsic" => "copy",
            "move" | "aic_fs_move_intrinsic" => "move",
            "delete" | "aic_fs_delete_intrinsic" => "delete",
            "metadata" | "aic_fs_metadata_intrinsic" => "metadata",
            "walk_dir" | "aic_fs_walk_dir_intrinsic" => "walk_dir",
            "temp_file" | "aic_fs_temp_file_intrinsic" => "temp_file",
            "temp_dir" | "aic_fs_temp_dir_intrinsic" => "temp_dir",
            _ => return None,
        };

        match canonical {
            "exists" if self.sig_matches_shape(name, &["String"], "Bool") => {
                Some(self.gen_fs_exists_call(args, span, fctx))
            }
            "read_text" if self.sig_matches_shape(name, &["String"], "Result[String, FsError]") => {
                Some(self.gen_fs_string_result_call(name, "aic_rt_fs_read_text", args, span, fctx))
            }
            "temp_file" if self.sig_matches_shape(name, &["String"], "Result[String, FsError]") => {
                Some(self.gen_fs_string_result_call(name, "aic_rt_fs_temp_file", args, span, fctx))
            }
            "temp_dir" if self.sig_matches_shape(name, &["String"], "Result[String, FsError]") => {
                Some(self.gen_fs_string_result_call(name, "aic_rt_fs_temp_dir", args, span, fctx))
            }
            "write_text"
                if self.sig_matches_shape(name, &["String", "String"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_write_like_call(name, "aic_rt_fs_write_text", args, span, fctx))
            }
            "append_text"
                if self.sig_matches_shape(name, &["String", "String"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_write_like_call(name, "aic_rt_fs_append_text", args, span, fctx))
            }
            "copy"
                if self.sig_matches_shape(name, &["String", "String"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_write_like_call(name, "aic_rt_fs_copy", args, span, fctx))
            }
            "move"
                if self.sig_matches_shape(name, &["String", "String"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_write_like_call(name, "aic_rt_fs_move", args, span, fctx))
            }
            "delete" if self.sig_matches_shape(name, &["String"], "Result[Bool, FsError]") => {
                Some(self.gen_fs_delete_call(name, args, span, fctx))
            }
            "metadata"
                if self.sig_matches_shape(name, &["String"], "Result[FsMetadata, FsError]") =>
            {
                Some(self.gen_fs_metadata_call(name, args, span, fctx))
            }
            "walk_dir"
                if self.sig_matches_shape(name, &["String"], "Result[Vec[String], FsError]") =>
            {
                Some(self.gen_fs_walk_dir_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    fn gen_fs_exists_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "exists expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let arg = self.gen_expr(&args[0], fctx)?;
        if arg.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "exists expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&arg, args[0].span, fctx)?;
        let raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_exists(i8* {}, i64 {}, i64 {})",
            raw, ptr, len, cap
        ));
        let reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", reg, raw));
        Some(Value {
            ty: LType::Bool,
            repr: Some(reg),
        })
    }

    fn gen_fs_string_result_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let path = self.gen_expr(&args[0], fctx)?;
        if path.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&path, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err, runtime_fn, ptr, len, cap, out_ptr_slot, out_len_slot
        ));

        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_fs_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_fs_write_like_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects two arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let lhs = self.gen_expr(&args[0], fctx)?;
        let rhs = self.gen_expr(&args[1], fctx)?;
        if lhs.ty != LType::String || rhs.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let (lhs_ptr, lhs_len, lhs_cap) = self.string_parts(&lhs, args[0].span, fctx)?;
        let (rhs_ptr, rhs_len, rhs_cap) = self.string_parts(&rhs, args[1].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
            err, runtime_fn, lhs_ptr, lhs_len, lhs_cap, rhs_ptr, rhs_len, rhs_cap
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_fs_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_fs_delete_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "delete expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let path = self.gen_expr(&args[0], fctx)?;
        if path.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "delete expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&path, args[0].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_delete(i8* {}, i64 {}, i64 {})",
            err, ptr, len, cap
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_fs_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_fs_metadata_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "metadata expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let path = self.gen_expr(&args[0], fctx)?;
        if path.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "metadata expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&path, args[0].span, fctx)?;
        let is_file_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", is_file_slot));
        let is_dir_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", is_dir_slot));
        let size_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", size_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_metadata(i8* {}, i64 {}, i64 {}, i64* {}, i64* {}, i64* {})",
            err, ptr, len, cap, is_file_slot, is_dir_slot, size_slot
        ));

        let is_file_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            is_file_raw, is_file_slot
        ));
        let is_file = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", is_file, is_file_raw));

        let is_dir_raw = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", is_dir_raw, is_dir_slot));
        let is_dir = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", is_dir, is_dir_raw));

        let size = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", size, size_slot));

        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let LType::Struct(ok_layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "metadata expects Result[FsMetadata, FsError] return type",
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload = self.build_struct_value(
            &ok_layout,
            &[
                Value {
                    ty: LType::Bool,
                    repr: Some(is_file),
                },
                Value {
                    ty: LType::Bool,
                    repr: Some(is_dir),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(size),
                },
            ],
            span,
            fctx,
        )?;
        self.wrap_fs_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_fs_walk_dir_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "walk_dir expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let path = self.gen_expr(&args[0], fctx)?;
        if path.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "walk_dir expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&path, args[0].span, fctx)?;
        let count_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", count_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_walk_dir(i8* {}, i64 {}, i64 {}, i64* {})",
            err, ptr, len, cap, count_slot
        ));
        let count = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", count, count_slot));

        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let LType::Struct(ok_layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "walk_dir expects Result[Vec[String], FsError] return type",
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload = self.build_struct_value(
            &ok_layout,
            &[
                Value {
                    ty: LType::Int,
                    repr: Some("0".to_string()),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(count.clone()),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(count),
                },
            ],
            span,
            fctx,
        )?;
        self.wrap_fs_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn wrap_fs_result(
        &mut self,
        result_ty: &LType,
        ok_payload: Value,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(result_ty, span)
        else {
            return None;
        };
        if ok_payload.ty != ok_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "filesystem builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_fs_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("fs_ok");
        let err_label = self.new_label("fs_err");
        let cont_label = self.new_label("fs_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            ok_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", err_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(result_ty),
            llvm_type(result_ty),
            slot
        ));
        Some(Value {
            ty: result_ty.clone(),
            repr: Some(reg),
        })
    }

    fn gen_env_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "get" | "aic_env_get_intrinsic" => "get",
            "set" | "aic_env_set_intrinsic" => "set",
            "remove" | "aic_env_remove_intrinsic" => "remove",
            "cwd" | "aic_env_cwd_intrinsic" => "cwd",
            "set_cwd" | "aic_env_set_cwd_intrinsic" => "set_cwd",
            _ => return None,
        };

        match canonical {
            "get" if self.sig_matches_shape(name, &["String"], "Result[String, EnvError]") => {
                Some(self.gen_env_get_call(name, args, span, fctx))
            }
            "set"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[Bool, EnvError]",
                ) =>
            {
                Some(self.gen_env_set_call(name, args, span, fctx))
            }
            "remove" if self.sig_matches_shape(name, &["String"], "Result[Bool, EnvError]") => {
                Some(self.gen_env_remove_call(name, args, span, fctx))
            }
            "cwd" if self.sig_matches_shape(name, &[], "Result[String, EnvError]") => {
                Some(self.gen_env_cwd_call(name, args, span, fctx))
            }
            "set_cwd" if self.sig_matches_shape(name, &["String"], "Result[Bool, EnvError]") => {
                Some(self.gen_env_set_cwd_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    fn gen_env_get_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "get expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let key = self.gen_expr(&args[0], fctx)?;
        if key.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "get expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&key, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_env_get(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err, ptr, len, cap, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_env_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_env_set_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "set expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let key = self.gen_expr(&args[0], fctx)?;
        let value = self.gen_expr(&args[1], fctx)?;
        if key.ty != LType::String || value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "set expects String arguments",
                self.file,
                span,
            ));
            return None;
        }
        let (kptr, klen, kcap) = self.string_parts(&key, args[0].span, fctx)?;
        let (vptr, vlen, vcap) = self.string_parts(&value, args[1].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_env_set(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
            err, kptr, klen, kcap, vptr, vlen, vcap
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_env_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_env_remove_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "remove expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let key = self.gen_expr(&args[0], fctx)?;
        if key.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "remove expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&key, args[0].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_env_remove(i8* {}, i64 {}, i64 {})",
            err, ptr, len, cap
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_env_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_env_cwd_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "cwd expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_env_cwd(i8** {}, i64* {})",
            err, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_env_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_env_set_cwd_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "set_cwd expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let path = self.gen_expr(&args[0], fctx)?;
        if path.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "set_cwd expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&path, args[0].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_env_set_cwd(i8* {}, i64 {}, i64 {})",
            err, ptr, len, cap
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_env_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn wrap_env_result(
        &mut self,
        result_ty: &LType,
        ok_payload: Value,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(result_ty, span)
        else {
            return None;
        };
        if ok_payload.ty != ok_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "env builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_env_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("env_ok");
        let err_label = self.new_label("env_err");
        let cont_label = self.new_label("env_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            ok_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", err_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(result_ty),
            llvm_type(result_ty),
            slot
        ));
        Some(Value {
            ty: result_ty.clone(),
            repr: Some(reg),
        })
    }

    fn gen_path_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "join" | "aic_path_join_intrinsic" => "join",
            "basename" | "aic_path_basename_intrinsic" => "basename",
            "dirname" | "aic_path_dirname_intrinsic" => "dirname",
            "extension" | "aic_path_extension_intrinsic" => "extension",
            "is_abs" | "aic_path_is_abs_intrinsic" => "is_abs",
            _ => return None,
        };

        match canonical {
            "join" if self.sig_matches_shape(name, &["String", "String"], "String") => {
                Some(self.gen_path_join_call(args, span, fctx))
            }
            "basename" if self.sig_matches_shape(name, &["String"], "String") => {
                Some(self.gen_path_string_unary_call(
                    "basename",
                    "aic_rt_path_basename",
                    args,
                    span,
                    fctx,
                ))
            }
            "dirname" if self.sig_matches_shape(name, &["String"], "String") => Some(
                self.gen_path_string_unary_call("dirname", "aic_rt_path_dirname", args, span, fctx),
            ),
            "extension" if self.sig_matches_shape(name, &["String"], "String") => {
                Some(self.gen_path_string_unary_call(
                    "extension",
                    "aic_rt_path_extension",
                    args,
                    span,
                    fctx,
                ))
            }
            "is_abs" if self.sig_matches_shape(name, &["String"], "Bool") => {
                Some(self.gen_path_is_abs_call(args, span, fctx))
            }
            _ => None,
        }
    }

    fn gen_path_join_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "join expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let left = self.gen_expr(&args[0], fctx)?;
        let right = self.gen_expr(&args[1], fctx)?;
        if left.ty != LType::String || right.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "join expects String arguments",
                self.file,
                span,
            ));
            return None;
        }
        let (lptr, llen, lcap) = self.string_parts(&left, args[0].span, fctx)?;
        let (rptr, rlen, rcap) = self.string_parts(&right, args[1].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_path_join(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            lptr, llen, lcap, rptr, rlen, rcap, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        Some(self.build_string_value(&out_ptr, &out_len, &out_len, fctx))
    }

    fn gen_path_string_unary_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let input = self.gen_expr(&args[0], fctx)?;
        if input.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&input, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @{}(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            runtime_fn, ptr, len, cap, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        Some(self.build_string_value(&out_ptr, &out_len, &out_len, fctx))
    }

    fn gen_path_is_abs_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "is_abs expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let input = self.gen_expr(&args[0], fctx)?;
        if input.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "is_abs expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&input, args[0].span, fctx)?;
        let raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_path_is_abs(i8* {}, i64 {}, i64 {})",
            raw, ptr, len, cap
        ));
        let reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", reg, raw));
        Some(Value {
            ty: LType::Bool,
            repr: Some(reg),
        })
    }

    fn gen_proc_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "spawn" | "aic_proc_spawn_intrinsic" => "spawn",
            "wait" | "aic_proc_wait_intrinsic" => "wait",
            "kill" | "aic_proc_kill_intrinsic" => "kill",
            "run" | "aic_proc_run_intrinsic" => "run",
            "pipe" | "aic_proc_pipe_intrinsic" => "pipe",
            _ => return None,
        };

        match canonical {
            "spawn" if self.sig_matches_shape(name, &["String"], "Result[Int, ProcError]") => {
                Some(self.gen_proc_spawn_call(name, args, span, fctx))
            }
            "wait" if self.sig_matches_shape(name, &["Int"], "Result[Int, ProcError]") => {
                Some(self.gen_proc_wait_call(name, args, span, fctx))
            }
            "kill" if self.sig_matches_shape(name, &["Int"], "Result[Bool, ProcError]") => {
                Some(self.gen_proc_kill_call(name, args, span, fctx))
            }
            "run" if self.sig_matches_shape(name, &["String"], "Result[ProcOutput, ProcError]") => {
                Some(self.gen_proc_run_call(name, args, span, fctx))
            }
            "pipe"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[ProcOutput, ProcError]",
                ) =>
            {
                Some(self.gen_proc_pipe_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    fn gen_proc_spawn_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "spawn expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let command = self.gen_expr(&args[0], fctx)?;
        if command.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "spawn expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&command, args[0].span, fctx)?;
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_proc_spawn(i8* {}, i64 {}, i64 {}, i64* {})",
            err, ptr, len, cap, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_proc_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_proc_wait_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "wait expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "wait expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let status_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", status_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_proc_wait(i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            status_slot
        ));
        let status = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", status, status_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(status),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_proc_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_proc_kill_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "kill expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "kill expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_proc_kill(i64 {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_proc_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_proc_run_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "run expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let command = self.gen_expr(&args[0], fctx)?;
        if command.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "run expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&command, args[0].span, fctx)?;
        let status_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", status_slot));
        let stdout_ptr_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", stdout_ptr_slot));
        let stdout_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", stdout_len_slot));
        let stderr_ptr_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", stderr_ptr_slot));
        let stderr_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", stderr_len_slot));

        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_proc_run(i8* {}, i64 {}, i64 {}, i64* {}, i8** {}, i64* {}, i8** {}, i64* {})",
            err, ptr, len, cap, status_slot, stdout_ptr_slot, stdout_len_slot, stderr_ptr_slot, stderr_len_slot
        ));
        self.build_proc_output_result(
            name,
            &err,
            status_slot,
            stdout_ptr_slot,
            stdout_len_slot,
            stderr_ptr_slot,
            stderr_len_slot,
            span,
            fctx,
        )
    }

    fn gen_proc_pipe_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "pipe expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let left = self.gen_expr(&args[0], fctx)?;
        let right = self.gen_expr(&args[1], fctx)?;
        if left.ty != LType::String || right.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "pipe expects String arguments",
                self.file,
                span,
            ));
            return None;
        }
        let (lptr, llen, lcap) = self.string_parts(&left, args[0].span, fctx)?;
        let (rptr, rlen, rcap) = self.string_parts(&right, args[1].span, fctx)?;
        let status_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", status_slot));
        let stdout_ptr_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", stdout_ptr_slot));
        let stdout_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", stdout_len_slot));
        let stderr_ptr_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", stderr_ptr_slot));
        let stderr_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", stderr_len_slot));

        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_proc_pipe(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64* {}, i8** {}, i64* {}, i8** {}, i64* {})",
            err, lptr, llen, lcap, rptr, rlen, rcap, status_slot, stdout_ptr_slot, stdout_len_slot, stderr_ptr_slot, stderr_len_slot
        ));
        self.build_proc_output_result(
            name,
            &err,
            status_slot,
            stdout_ptr_slot,
            stdout_len_slot,
            stderr_ptr_slot,
            stderr_len_slot,
            span,
            fctx,
        )
    }

    fn build_proc_output_result(
        &mut self,
        name: &str,
        err: &str,
        status_slot: String,
        stdout_ptr_slot: String,
        stdout_len_slot: String,
        stderr_ptr_slot: String,
        stderr_len_slot: String,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let status = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", status, status_slot));
        let stdout_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            stdout_ptr, stdout_ptr_slot
        ));
        let stdout_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            stdout_len, stdout_len_slot
        ));
        let stderr_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            stderr_ptr, stderr_ptr_slot
        ));
        let stderr_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            stderr_len, stderr_len_slot
        ));

        let stdout_value = self.build_string_value(&stdout_ptr, &stdout_len, &stdout_len, fctx);
        let stderr_value = self.build_string_value(&stderr_ptr, &stderr_len, &stderr_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let LType::Struct(ok_layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "process builtin expects Result[ProcOutput, ProcError] return type",
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload = self.build_struct_value(
            &ok_layout,
            &[
                Value {
                    ty: LType::Int,
                    repr: Some(status),
                },
                stdout_value,
                stderr_value,
            ],
            span,
            fctx,
        )?;
        self.wrap_proc_result(&result_ty, ok_payload, err, span, fctx)
    }

    fn wrap_proc_result(
        &mut self,
        result_ty: &LType,
        ok_payload: Value,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(result_ty, span)
        else {
            return None;
        };
        if ok_payload.ty != ok_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "proc builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_proc_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("proc_ok");
        let err_label = self.new_label("proc_err");
        let cont_label = self.new_label("proc_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            ok_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", err_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(result_ty),
            llvm_type(result_ty),
            slot
        ));
        Some(Value {
            ty: result_ty.clone(),
            repr: Some(reg),
        })
    }

    fn gen_net_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "tcp_listen" | "aic_net_tcp_listen_intrinsic" => "tcp_listen",
            "tcp_local_addr" | "aic_net_tcp_local_addr_intrinsic" => "tcp_local_addr",
            "tcp_accept" | "aic_net_tcp_accept_intrinsic" => "tcp_accept",
            "tcp_connect" | "aic_net_tcp_connect_intrinsic" => "tcp_connect",
            "tcp_send" | "aic_net_tcp_send_intrinsic" => "tcp_send",
            "tcp_recv" | "aic_net_tcp_recv_intrinsic" => "tcp_recv",
            "tcp_close" | "aic_net_tcp_close_intrinsic" => "tcp_close",
            "udp_bind" | "aic_net_udp_bind_intrinsic" => "udp_bind",
            "udp_local_addr" | "aic_net_udp_local_addr_intrinsic" => "udp_local_addr",
            "udp_send_to" | "aic_net_udp_send_to_intrinsic" => "udp_send_to",
            "udp_recv_from" | "aic_net_udp_recv_from_intrinsic" => "udp_recv_from",
            "udp_close" | "aic_net_udp_close_intrinsic" => "udp_close",
            "dns_lookup" | "aic_net_dns_lookup_intrinsic" => "dns_lookup",
            "dns_reverse" | "aic_net_dns_reverse_intrinsic" => "dns_reverse",
            _ => return None,
        };

        match canonical {
            "tcp_listen" if self.sig_matches_shape(name, &["String"], "Result[Int, NetError]") => {
                Some(self.gen_net_listen_or_bind_call(
                    name,
                    "aic_rt_net_tcp_listen",
                    args,
                    span,
                    fctx,
                ))
            }
            "udp_bind" if self.sig_matches_shape(name, &["String"], "Result[Int, NetError]") => {
                Some(self.gen_net_listen_or_bind_call(
                    name,
                    "aic_rt_net_udp_bind",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_local_addr"
                if self.sig_matches_shape(name, &["Int"], "Result[String, NetError]") =>
            {
                Some(self.gen_net_local_addr_call(
                    name,
                    "aic_rt_net_tcp_local_addr",
                    args,
                    span,
                    fctx,
                ))
            }
            "udp_local_addr"
                if self.sig_matches_shape(name, &["Int"], "Result[String, NetError]") =>
            {
                Some(self.gen_net_local_addr_call(
                    name,
                    "aic_rt_net_udp_local_addr",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_accept"
                if self.sig_matches_shape(name, &["Int", "Int"], "Result[Int, NetError]") =>
            {
                Some(self.gen_net_tcp_accept_call(name, args, span, fctx))
            }
            "tcp_connect"
                if self.sig_matches_shape(name, &["String", "Int"], "Result[Int, NetError]") =>
            {
                Some(self.gen_net_tcp_connect_call(name, args, span, fctx))
            }
            "tcp_send"
                if self.sig_matches_shape(name, &["Int", "String"], "Result[Int, NetError]") =>
            {
                Some(self.gen_net_tcp_send_call(name, args, span, fctx))
            }
            "tcp_recv"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[String, NetError]",
                ) =>
            {
                Some(self.gen_net_tcp_recv_call(name, args, span, fctx))
            }
            "tcp_close" if self.sig_matches_shape(name, &["Int"], "Result[Bool, NetError]") => {
                Some(self.gen_net_close_call(name, "aic_rt_net_tcp_close", args, span, fctx))
            }
            "udp_close" if self.sig_matches_shape(name, &["Int"], "Result[Bool, NetError]") => {
                Some(self.gen_net_close_call(name, "aic_rt_net_udp_close", args, span, fctx))
            }
            "udp_send_to"
                if self.sig_matches_shape(
                    name,
                    &["Int", "String", "String"],
                    "Result[Int, NetError]",
                ) =>
            {
                Some(self.gen_net_udp_send_to_call(name, args, span, fctx))
            }
            "udp_recv_from"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[UdpPacket, NetError]",
                ) =>
            {
                Some(self.gen_net_udp_recv_from_call(name, args, span, fctx))
            }
            "dns_lookup"
                if self.sig_matches_shape(name, &["String"], "Result[String, NetError]") =>
            {
                Some(self.gen_net_dns_call(name, "aic_rt_net_dns_lookup", args, span, fctx))
            }
            "dns_reverse"
                if self.sig_matches_shape(name, &["String"], "Result[String, NetError]") =>
            {
                Some(self.gen_net_dns_call(name, "aic_rt_net_dns_reverse", args, span, fctx))
            }
            _ => None,
        }
    }

    fn gen_net_listen_or_bind_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let addr = self.gen_expr(&args[0], fctx)?;
        if addr.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&addr, args[0].span, fctx)?;
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i64* {})",
            err, runtime_fn, ptr, len, cap, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_net_local_addr_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i8** {}, i64* {})",
            err,
            runtime_fn,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_net_tcp_accept_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "tcp_accept expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let listener = self.gen_expr(&args[0], fctx)?;
        let timeout = self.gen_expr(&args[1], fctx)?;
        if listener.ty != LType::Int || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "tcp_accept expects Int arguments",
                self.file,
                span,
            ));
            return None;
        }
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_accept(i64 {}, i64 {}, i64* {})",
            err,
            listener.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_net_tcp_connect_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "tcp_connect expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let addr = self.gen_expr(&args[0], fctx)?;
        let timeout = self.gen_expr(&args[1], fctx)?;
        if addr.ty != LType::String || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "tcp_connect expects (String, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&addr, args[0].span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_connect(i8* {}, i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            ptr,
            len,
            cap,
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_net_tcp_send_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "tcp_send expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let payload = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int || payload.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "tcp_send expects (Int, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (pptr, plen, pcap) = self.string_parts(&payload, args[1].span, fctx)?;
        let sent_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", sent_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_send(i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            pptr,
            plen,
            pcap,
            sent_slot
        ));
        let sent = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", sent, sent_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(sent),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_net_tcp_recv_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "tcp_recv expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let max_bytes = self.gen_expr(&args[1], fctx)?;
        let timeout = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || max_bytes.ty != LType::Int || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "tcp_recv expects (Int, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_recv(i64 {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            max_bytes.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_net_close_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {})",
            err,
            runtime_fn,
            handle.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_net_udp_send_to_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "udp_send_to expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let addr = self.gen_expr(&args[1], fctx)?;
        let payload = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || addr.ty != LType::String || payload.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "udp_send_to expects (Int, String, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (aptr, alen, acap) = self.string_parts(&addr, args[1].span, fctx)?;
        let (pptr, plen, pcap) = self.string_parts(&payload, args[2].span, fctx)?;
        let sent_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", sent_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_udp_send_to(i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            aptr,
            alen,
            acap,
            pptr,
            plen,
            pcap,
            sent_slot
        ));
        let sent = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", sent, sent_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(sent),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_net_udp_recv_from_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "udp_recv_from expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let max_bytes = self.gen_expr(&args[1], fctx)?;
        let timeout = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || max_bytes.ty != LType::Int || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "udp_recv_from expects (Int, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let from_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", from_ptr_slot));
        let from_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", from_len_slot));
        let payload_ptr_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", payload_ptr_slot));
        let payload_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", payload_len_slot));

        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_udp_recv_from(i64 {}, i64 {}, i64 {}, i8** {}, i64* {}, i8** {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            max_bytes.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            from_ptr_slot,
            from_len_slot,
            payload_ptr_slot,
            payload_len_slot
        ));

        let from_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", from_ptr, from_ptr_slot));
        let from_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", from_len, from_len_slot));
        let payload_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            payload_ptr, payload_ptr_slot
        ));
        let payload_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            payload_len, payload_len_slot
        ));

        let from_value = self.build_string_value(&from_ptr, &from_len, &from_len, fctx);
        let payload_value = self.build_string_value(&payload_ptr, &payload_len, &payload_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let LType::Struct(ok_layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "udp_recv_from expects Result[UdpPacket, NetError] return type",
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload =
            self.build_struct_value(&ok_layout, &[from_value, payload_value], span, fctx)?;
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_net_dns_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let input = self.gen_expr(&args[0], fctx)?;
        if input.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&input, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err, runtime_fn, ptr, len, cap, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn wrap_net_result(
        &mut self,
        result_ty: &LType,
        ok_payload: Value,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(result_ty, span)
        else {
            return None;
        };
        if ok_payload.ty != ok_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "net builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_net_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("net_ok");
        let err_label = self.new_label("net_err");
        let cont_label = self.new_label("net_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            ok_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", err_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(result_ty),
            llvm_type(result_ty),
            slot
        ));
        Some(Value {
            ty: result_ty.clone(),
            repr: Some(reg),
        })
    }

    fn gen_regex_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "compile_with_flags" | "aic_regex_compile_intrinsic" => "compile_with_flags",
            "is_match" | "aic_regex_is_match_intrinsic" => "is_match",
            "find" | "aic_regex_find_intrinsic" => "find",
            "replace" | "aic_regex_replace_intrinsic" => "replace",
            _ => return None,
        };

        match canonical {
            "compile_with_flags"
                if self.sig_matches_shape(
                    name,
                    &["String", "Int"],
                    "Result[Regex, RegexError]",
                ) =>
            {
                Some(self.gen_regex_compile_call(name, args, span, fctx))
            }
            "is_match"
                if self.sig_matches_shape(
                    name,
                    &["Regex", "String"],
                    "Result[Bool, RegexError]",
                ) =>
            {
                Some(self.gen_regex_is_match_call(name, args, span, fctx))
            }
            "find"
                if self.sig_matches_shape(
                    name,
                    &["Regex", "String"],
                    "Result[String, RegexError]",
                ) =>
            {
                Some(self.gen_regex_find_call(name, args, span, fctx))
            }
            "replace"
                if self.sig_matches_shape(
                    name,
                    &["Regex", "String", "String"],
                    "Result[String, RegexError]",
                ) =>
            {
                Some(self.gen_regex_replace_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    fn gen_regex_compile_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "compile_with_flags expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let pattern = self.gen_expr(&args[0], fctx)?;
        let flags = self.gen_expr(&args[1], fctx)?;
        if pattern.ty != LType::String || flags.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "compile_with_flags expects (String, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (pattern_ptr, pattern_len, pattern_cap) =
            self.string_parts(&pattern, args[0].span, fctx)?;
        let flags_repr = flags.repr.clone().unwrap_or_else(|| "0".to_string());
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_regex_compile(i8* {}, i64 {}, i64 {}, i64 {})",
            err, pattern_ptr, pattern_len, pattern_cap, flags_repr
        ));

        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let LType::Struct(ok_layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "compile_with_flags expects Result[Regex, RegexError] return type",
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload = self.build_struct_value(&ok_layout, &[pattern, flags], span, fctx)?;
        self.wrap_regex_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_regex_is_match_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "is_match expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let regex = self.gen_expr(&args[0], fctx)?;
        let text = self.gen_expr(&args[1], fctx)?;
        if text.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "is_match expects Regex and String",
                self.file,
                span,
            ));
            return None;
        }
        let (pattern_ptr, pattern_len, pattern_cap, flags_repr) =
            self.regex_parts(&regex, args[0].span, fctx)?;
        let (text_ptr, text_len, text_cap) = self.string_parts(&text, args[1].span, fctx)?;
        let out_match_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_match_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_regex_is_match(i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            err, pattern_ptr, pattern_len, pattern_cap, flags_repr, text_ptr, text_len, text_cap, out_match_slot
        ));
        let out_match = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_match, out_match_slot
        ));
        let is_match = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", is_match, out_match));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(is_match),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_regex_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_regex_find_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "find expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let regex = self.gen_expr(&args[0], fctx)?;
        let text = self.gen_expr(&args[1], fctx)?;
        if text.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "find expects Regex and String",
                self.file,
                span,
            ));
            return None;
        }
        let (pattern_ptr, pattern_len, pattern_cap, flags_repr) =
            self.regex_parts(&regex, args[0].span, fctx)?;
        let (text_ptr, text_len, text_cap) = self.string_parts(&text, args[1].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_regex_find(i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            pattern_ptr,
            pattern_len,
            pattern_cap,
            flags_repr,
            text_ptr,
            text_len,
            text_cap,
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_regex_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn gen_regex_replace_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "replace expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let regex = self.gen_expr(&args[0], fctx)?;
        let text = self.gen_expr(&args[1], fctx)?;
        let replacement = self.gen_expr(&args[2], fctx)?;
        if text.ty != LType::String || replacement.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "replace expects (Regex, String, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (pattern_ptr, pattern_len, pattern_cap, flags_repr) =
            self.regex_parts(&regex, args[0].span, fctx)?;
        let (text_ptr, text_len, text_cap) = self.string_parts(&text, args[1].span, fctx)?;
        let (repl_ptr, repl_len, repl_cap) = self.string_parts(&replacement, args[2].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_regex_replace(i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            pattern_ptr,
            pattern_len,
            pattern_cap,
            flags_repr,
            text_ptr,
            text_len,
            text_cap,
            repl_ptr,
            repl_len,
            repl_cap,
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_regex_result(&result_ty, ok_payload, &err, span, fctx)
    }

    fn regex_parts(
        &mut self,
        regex: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String, String)> {
        let LType::Struct(layout) = regex.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "expected Regex struct value",
                self.file,
                span,
            ));
            return None;
        };
        let Some((pattern_index, pattern_field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == "pattern")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Regex struct is missing `pattern` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some((flags_index, flags_field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == "flags")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Regex struct is missing `flags` field",
                self.file,
                span,
            ));
            return None;
        };
        if pattern_field.ty != LType::String || flags_field.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Regex struct fields must be `pattern: String` and `flags: Int`",
                self.file,
                span,
            ));
            return None;
        }

        let regex_repr = regex
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&regex.ty));

        let pattern_reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            pattern_reg,
            llvm_type(&regex.ty),
            regex_repr,
            pattern_index
        ));
        let pattern_value = Value {
            ty: LType::String,
            repr: Some(pattern_reg),
        };
        let (pattern_ptr, pattern_len, pattern_cap) =
            self.string_parts(&pattern_value, span, fctx)?;

        let flags_reg = self.new_temp();
        let regex_repr = regex
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&regex.ty));
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            flags_reg,
            llvm_type(&regex.ty),
            regex_repr,
            flags_index
        ));

        Some((pattern_ptr, pattern_len, pattern_cap, flags_reg))
    }

    fn wrap_regex_result(
        &mut self,
        result_ty: &LType,
        ok_payload: Value,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(result_ty, span)
        else {
            return None;
        };
        if ok_payload.ty != ok_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "regex builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_regex_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("regex_ok");
        let err_label = self.new_label("regex_err");
        let cont_label = self.new_label("regex_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            ok_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", err_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(result_ty),
            llvm_type(result_ty),
            slot
        ));
        Some(Value {
            ty: result_ty.clone(),
            repr: Some(reg),
        })
    }

    fn result_layout_parts(
        &mut self,
        result_ty: &LType,
        span: crate::span::Span,
    ) -> Option<(EnumLayoutType, LType, LType, usize, usize)> {
        let LType::Enum(layout) = result_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "builtin expects Result return type",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Result" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "builtin expects Result return type, found '{}'",
                    layout.repr
                ),
                self.file,
                span,
            ));
            return None;
        }
        let Some(ok_index) = layout
            .variants
            .iter()
            .position(|variant| variant.name == "Ok")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Result return type is missing Ok variant",
                self.file,
                span,
            ));
            return None;
        };
        let Some(err_index) = layout
            .variants
            .iter()
            .position(|variant| variant.name == "Err")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Result return type is missing Err variant",
                self.file,
                span,
            ));
            return None;
        };
        let Some(ok_ty) = layout.variants[ok_index].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Result Ok variant must have a payload",
                self.file,
                span,
            ));
            return None;
        };
        let Some(err_ty) = layout.variants[err_index].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Result Err variant must have a payload",
                self.file,
                span,
            ));
            return None;
        };
        Some((layout.clone(), ok_ty, err_ty, ok_index, err_index))
    }

    fn build_enum_variant(
        &mut self,
        layout: &EnumLayoutType,
        variant_index: usize,
        payload: Option<Value>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if variant_index >= layout.variants.len() {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "enum variant index out of range",
                self.file,
                span,
            ));
            return None;
        }
        let expected_payload = &layout.variants[variant_index].payload;
        if expected_payload.is_none() && payload.is_some() {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "enum variant does not accept payload",
                self.file,
                span,
            ));
            return None;
        }

        let ty = LType::Enum(layout.clone());
        let mut acc = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} undef, i32 {}, 0",
            acc,
            llvm_type(&ty),
            variant_index
        ));
        for (idx, variant) in layout.variants.iter().enumerate() {
            let (slot_ty, slot_repr) = if let Some(payload_ty) = &variant.payload {
                if idx == variant_index {
                    if let Some(payload_value) = payload.as_ref() {
                        if payload_value.ty != *payload_ty {
                            self.diagnostics.push(Diagnostic::error(
                                "E5011",
                                format!(
                                    "enum payload expects '{}', found '{}'",
                                    render_type(payload_ty),
                                    render_type(&payload_value.ty)
                                ),
                                self.file,
                                span,
                            ));
                            (llvm_type(payload_ty), default_value(payload_ty))
                        } else {
                            (
                                llvm_type(payload_ty),
                                payload_value
                                    .repr
                                    .clone()
                                    .unwrap_or_else(|| default_value(payload_ty)),
                            )
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5011",
                            "enum variant expects payload",
                            self.file,
                            span,
                        ));
                        (llvm_type(payload_ty), default_value(payload_ty))
                    }
                } else {
                    (llvm_type(payload_ty), default_value(payload_ty))
                }
            } else {
                ("i8".to_string(), "0".to_string())
            };
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} {}, {} {}, {}",
                reg,
                llvm_type(&ty),
                acc,
                slot_ty,
                slot_repr,
                idx + 1
            ));
            acc = reg;
        }
        Some(Value {
            ty,
            repr: Some(acc),
        })
    }

    fn build_struct_value(
        &mut self,
        layout: &StructLayoutType,
        field_values: &[Value],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if layout.fields.len() != field_values.len() {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "struct '{}' field count mismatch: expected {}, found {}",
                    layout.repr,
                    layout.fields.len(),
                    field_values.len()
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ty = LType::Struct(layout.clone());
        if layout.fields.is_empty() {
            return Some(Value {
                ty,
                repr: Some(default_value(&LType::Struct(layout.clone()))),
            });
        }

        let mut acc = "undef".to_string();
        for (idx, (field, value)) in layout.fields.iter().zip(field_values.iter()).enumerate() {
            let rendered = if value.ty == field.ty {
                value
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&field.ty))
            } else {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    format!(
                        "field '{}.{}' expects '{}', found '{}'",
                        layout.repr,
                        field.name,
                        render_type(&field.ty),
                        render_type(&value.ty)
                    ),
                    self.file,
                    span,
                ));
                default_value(&field.ty)
            };
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} {}, {} {}, {}",
                reg,
                llvm_type(&ty),
                acc,
                llvm_type(&field.ty),
                rendered,
                idx
            ));
            acc = reg;
        }

        Some(Value {
            ty,
            repr: Some(acc),
        })
    }

    fn build_string_value(&mut self, ptr: &str, len: &str, cap: &str, fctx: &mut FnCtx) -> Value {
        let ty = LType::String;
        let reg0 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} undef, i8* {}, 0",
            reg0,
            llvm_type(&ty),
            ptr
        ));
        let reg1 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} {}, i64 {}, 1",
            reg1,
            llvm_type(&ty),
            reg0,
            len
        ));
        let reg2 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} {}, i64 {}, 2",
            reg2,
            llvm_type(&ty),
            reg1,
            cap
        ));
        Value {
            ty,
            repr: Some(reg2),
        }
    }

    fn build_error_from_code(
        &mut self,
        err_ty: &LType,
        enum_name: &str,
        context: &str,
        mappings: &[(i64, &str)],
        fallback_variant: &str,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Enum(layout) = err_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "{context} builtin expects {enum_name} payload, found '{}'",
                    render_type(err_ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != enum_name {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "{context} builtin expects {enum_name} payload, found '{}'",
                    layout.repr
                ),
                self.file,
                span,
            ));
            return None;
        }

        if layout
            .variants
            .iter()
            .any(|variant| variant.payload.is_some())
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{enum_name} variants must not have payloads"),
                self.file,
                span,
            ));
            return None;
        }

        let variant_index =
            |name: &str| -> Option<usize> { layout.variants.iter().position(|v| v.name == name) };
        let Some(fallback_idx) = variant_index(fallback_variant) else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{enum_name} is missing {fallback_variant} variant"),
                self.file,
                span,
            ));
            return None;
        };

        let mut mapping_indices = Vec::new();
        for (code, variant_name) in mappings {
            let Some(index) = variant_index(variant_name) else {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    format!("{enum_name} is missing {variant_name} variant"),
                    self.file,
                    span,
                ));
                return None;
            };
            mapping_indices.push((*code, index));
        }

        let mut tag = format!("{}", fallback_idx as i32);
        for (code, index) in mapping_indices {
            let is_match = self.new_temp();
            fctx.lines.push(format!(
                "  {} = icmp eq i64 {}, {}",
                is_match, err_code, code
            ));
            let selected = self.new_temp();
            fctx.lines.push(format!(
                "  {} = select i1 {}, i32 {}, i32 {}",
                selected, is_match, index as i32, tag
            ));
            tag = selected;
        }

        self.build_no_payload_enum_with_tag(layout, &tag, span, fctx)
    }

    fn build_fs_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "FsError",
            "filesystem",
            &[
                (1, "NotFound"),
                (2, "PermissionDenied"),
                (3, "AlreadyExists"),
                (4, "InvalidInput"),
                (5, "Io"),
            ],
            "Io",
            err_code,
            span,
            fctx,
        )
    }

    fn build_env_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "EnvError",
            "env",
            &[
                (1, "NotFound"),
                (2, "PermissionDenied"),
                (3, "InvalidInput"),
                (4, "Io"),
            ],
            "Io",
            err_code,
            span,
            fctx,
        )
    }

    fn build_proc_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "ProcError",
            "proc",
            &[
                (1, "NotFound"),
                (2, "PermissionDenied"),
                (3, "InvalidInput"),
                (4, "Io"),
                (5, "UnknownProcess"),
            ],
            "Io",
            err_code,
            span,
            fctx,
        )
    }

    fn build_net_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "NetError",
            "net",
            &[
                (1, "NotFound"),
                (2, "PermissionDenied"),
                (3, "Refused"),
                (4, "Timeout"),
                (5, "AddressInUse"),
                (6, "InvalidInput"),
                (7, "Io"),
            ],
            "Io",
            err_code,
            span,
            fctx,
        )
    }

    fn build_regex_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "RegexError",
            "regex",
            &[
                (1, "InvalidPattern"),
                (2, "InvalidInput"),
                (3, "NoMatch"),
                (4, "UnsupportedFeature"),
                (5, "TooComplex"),
                (6, "Internal"),
            ],
            "Internal",
            err_code,
            span,
            fctx,
        )
    }

    fn build_concurrency_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "ConcurrencyError",
            "concurrency",
            &[
                (1, "NotFound"),
                (2, "Timeout"),
                (3, "Cancelled"),
                (4, "InvalidInput"),
                (5, "Panic"),
                (6, "Closed"),
                (7, "Io"),
            ],
            "Io",
            err_code,
            span,
            fctx,
        )
    }

    fn build_no_payload_enum_with_tag(
        &mut self,
        layout: &EnumLayoutType,
        tag: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if layout
            .variants
            .iter()
            .any(|variant| variant.payload.is_some())
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "expected no-payload enum layout",
                self.file,
                span,
            ));
            return None;
        }
        let ty = LType::Enum(layout.clone());
        let mut acc = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} undef, i32 {}, 0",
            acc,
            llvm_type(&ty),
            tag
        ));
        for idx in 0..layout.variants.len() {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} {}, i8 0, {}",
                reg,
                llvm_type(&ty),
                acc,
                idx + 1
            ));
            acc = reg;
        }
        Some(Value {
            ty,
            repr: Some(acc),
        })
    }

    fn gen_struct_init(
        &mut self,
        name: &str,
        fields: &[(String, ir::Expr, crate::span::Span)],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some(template) = self.struct_templates.get(name).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                "E5004",
                format!("unknown struct '{}' in codegen", name),
                self.file,
                span,
            ));
            return None;
        };

        let mut provided = BTreeMap::new();
        for (field_name, field_expr, field_span) in fields {
            if provided.contains_key(field_name) {
                self.diagnostics.push(Diagnostic::error(
                    "E5004",
                    format!(
                        "duplicate field '{}.{}' in struct literal",
                        name, field_name
                    ),
                    self.file,
                    *field_span,
                ));
                continue;
            }
            let value = self.gen_expr(field_expr, fctx)?;
            provided.insert(field_name.clone(), (value, *field_span));
        }

        let mut bindings = BTreeMap::new();
        for (field_name, expected_ty) in &template.fields {
            let Some((value, _)) = provided.get(field_name) else {
                continue;
            };
            let actual = render_type(&value.ty);
            infer_generic_bindings(expected_ty, &actual, &template.generics, &mut bindings);
        }
        for generic in &template.generics {
            let fallback = self
                .active_type_bindings
                .as_ref()
                .and_then(|map| map.get(generic))
                .cloned()
                .unwrap_or_else(|| "Int".to_string());
            bindings.entry(generic.clone()).or_insert(fallback);
        }

        let applied_args = template
            .generics
            .iter()
            .map(|g| {
                bindings
                    .get(g)
                    .cloned()
                    .unwrap_or_else(|| "Int".to_string())
            })
            .collect::<Vec<_>>();
        let applied_repr = render_applied_type_from_parts(name, &applied_args);
        let ty = self.parse_type_repr(&applied_repr, span)?;
        let LType::Struct(layout) = ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5004",
                format!("failed to lower struct layout for '{}'", applied_repr),
                self.file,
                span,
            ));
            return None;
        };

        let mut acc = "undef".to_string();
        for (idx, field) in layout.fields.iter().enumerate() {
            let rendered = if let Some((value, field_span)) = provided.get(&field.name) {
                if value.ty != field.ty {
                    self.diagnostics.push(Diagnostic::error(
                        "E5004",
                        format!(
                            "field '{}.{}' expects '{}', found '{}'",
                            name,
                            field.name,
                            render_type(&field.ty),
                            render_type(&value.ty)
                        ),
                        self.file,
                        *field_span,
                    ));
                    default_value(&field.ty)
                } else {
                    value
                        .repr
                        .clone()
                        .unwrap_or_else(|| default_value(&field.ty))
                }
            } else {
                self.diagnostics.push(Diagnostic::error(
                    "E5004",
                    format!("missing field '{}.{}' in struct literal", name, field.name),
                    self.file,
                    span,
                ));
                default_value(&field.ty)
            };
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} {}, {} {}, {}",
                reg,
                llvm_type(&ty),
                acc,
                llvm_type(&field.ty),
                rendered,
                idx
            ));
            acc = reg;
        }

        let repr = if layout.fields.is_empty() {
            default_value(&ty)
        } else {
            acc
        };
        Some(Value {
            ty,
            repr: Some(repr),
        })
    }

    fn gen_field_access(
        &mut self,
        base: &ir::Expr,
        field: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let value = self.gen_expr(base, fctx)?;
        let LType::Struct(layout) = value.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5005",
                format!(
                    "field access requires struct value, found '{}'",
                    render_type(&value.ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        let Some((index, field_layout)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, info)| info.name == field)
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5005",
                format!("struct '{}' has no field '{}'", layout.repr, field),
                self.file,
                span,
            ));
            return None;
        };

        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            reg,
            llvm_type(&value.ty),
            value.repr.unwrap_or_else(|| default_value(&value.ty)),
            index
        ));
        Some(Value {
            ty: field_layout.ty.clone(),
            repr: Some(reg),
        })
    }

    fn gen_variant_constructor(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let Some(candidates) = self.variant_ctors.get(name).cloned() else {
            return None;
        };
        if args.len() > 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5009",
                format!("variant constructor '{}' takes at most one argument", name),
                self.file,
                span,
            ));
            return Some(None);
        }

        let payload_value = if args.len() == 1 {
            Some(self.gen_expr(&args[0], fctx)?)
        } else {
            None
        };

        let mut chosen: Option<(EnumLayoutType, usize)> = None;
        for candidate in candidates {
            let Some(template) = self.enum_templates.get(&candidate.enum_name) else {
                continue;
            };
            let Some((_, payload_template)) = template.variants.get(candidate.variant_index) else {
                continue;
            };

            let payload_arity = usize::from(payload_template.is_some());
            if payload_arity != args.len() {
                continue;
            }

            let mut bindings = BTreeMap::new();
            if let (Some(raw_payload), Some(payload)) = (payload_template, payload_value.as_ref()) {
                if !infer_generic_bindings(
                    raw_payload,
                    &render_type(&payload.ty),
                    &template.generics,
                    &mut bindings,
                ) {
                    continue;
                }
            }
            for generic in &template.generics {
                let fallback = self
                    .active_type_bindings
                    .as_ref()
                    .and_then(|map| map.get(generic))
                    .cloned()
                    .unwrap_or_else(|| "Int".to_string());
                bindings.entry(generic.clone()).or_insert(fallback);
            }
            let args = template
                .generics
                .iter()
                .map(|g| {
                    bindings
                        .get(g)
                        .cloned()
                        .unwrap_or_else(|| "Int".to_string())
                })
                .collect::<Vec<_>>();
            let repr = render_applied_type_from_parts(&candidate.enum_name, &args);
            let Some(LType::Enum(layout)) = self.parse_type_repr(&repr, span) else {
                continue;
            };
            chosen = Some((layout, candidate.variant_index));
            break;
        }

        let Some((layout, variant_index)) = chosen else {
            self.diagnostics.push(Diagnostic::error(
                "E5009",
                format!("no viable enum constructor overload for '{}'", name),
                self.file,
                span,
            ));
            return Some(None);
        };

        let ty = LType::Enum(layout.clone());
        let mut acc = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} undef, i32 {}, 0",
            acc,
            llvm_type(&ty),
            variant_index
        ));

        for (idx, variant) in layout.variants.iter().enumerate() {
            let (slot_ty, slot_repr) = if let Some(payload_ty) = &variant.payload {
                if idx == variant_index {
                    if let Some(payload) = payload_value.as_ref() {
                        if payload.ty == *payload_ty {
                            (
                                llvm_type(payload_ty),
                                payload
                                    .repr
                                    .clone()
                                    .unwrap_or_else(|| default_value(payload_ty)),
                            )
                        } else {
                            self.diagnostics.push(Diagnostic::error(
                                "E5009",
                                format!(
                                    "variant '{}' payload expects '{}', found '{}'",
                                    name,
                                    render_type(payload_ty),
                                    render_type(&payload.ty)
                                ),
                                self.file,
                                span,
                            ));
                            (llvm_type(payload_ty), default_value(payload_ty))
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5009",
                            format!("variant '{}' expects a payload argument", name),
                            self.file,
                            span,
                        ));
                        (llvm_type(payload_ty), default_value(payload_ty))
                    }
                } else {
                    (llvm_type(payload_ty), default_value(payload_ty))
                }
            } else {
                ("i8".to_string(), "0".to_string())
            };

            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} {}, {} {}, {}",
                reg,
                llvm_type(&ty),
                acc,
                slot_ty,
                slot_repr,
                idx + 1
            ));
            acc = reg;
        }

        Some(Some(Value {
            ty,
            repr: Some(acc),
        }))
    }

    fn gen_try(
        &mut self,
        inner_expr: &ir::Expr,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let result = self.gen_expr(inner_expr, fctx)?;
        let LType::Enum(result_layout) = result.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                format!(
                    "`?` expects Result[T, E] in codegen, found '{}'",
                    render_type(&result.ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&result_layout.repr) != "Result" {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                format!(
                    "`?` expects Result[T, E] in codegen, found '{}'",
                    result_layout.repr
                ),
                self.file,
                span,
            ));
            return None;
        }
        let Some(ok_idx) = result_layout.variants.iter().position(|v| v.name == "Ok") else {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                "Result layout missing Ok variant for `?`",
                self.file,
                span,
            ));
            return None;
        };
        let Some(err_idx) = result_layout.variants.iter().position(|v| v.name == "Err") else {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                "Result layout missing Err variant for `?`",
                self.file,
                span,
            ));
            return None;
        };
        let Some(ok_payload_ty) = result_layout.variants[ok_idx].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                "Result Ok variant must carry a payload for `?`",
                self.file,
                span,
            ));
            return None;
        };
        let Some(err_payload_ty) = result_layout.variants[err_idx].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                "Result Err variant must carry a payload for `?`",
                self.file,
                span,
            ));
            return None;
        };

        let LType::Enum(fn_ret_layout) = fctx.ret_ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5022",
                format!(
                    "`?` requires Result return type in enclosing function, found '{}'",
                    render_type(&fctx.ret_ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&fn_ret_layout.repr) != "Result" {
            self.diagnostics.push(Diagnostic::error(
                "E5022",
                format!(
                    "`?` requires Result return type in enclosing function, found '{}'",
                    fn_ret_layout.repr
                ),
                self.file,
                span,
            ));
            return None;
        }
        let Some(fn_err_idx) = fn_ret_layout.variants.iter().position(|v| v.name == "Err") else {
            self.diagnostics.push(Diagnostic::error(
                "E5022",
                "enclosing Result return type missing Err variant for `?`",
                self.file,
                span,
            ));
            return None;
        };
        let Some(fn_err_payload_ty) = fn_ret_layout.variants[fn_err_idx].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5022",
                "enclosing Result Err variant must carry a payload for `?`",
                self.file,
                span,
            ));
            return None;
        };
        if err_payload_ty != fn_err_payload_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5022",
                format!(
                    "`?` error type mismatch in codegen: expression has '{}', function expects '{}'",
                    render_type(&err_payload_ty),
                    render_type(&fn_err_payload_ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let result_repr = result
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&result.ty));
        let tag = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            tag,
            llvm_type(&result.ty),
            result_repr.as_str()
        ));
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i32 {}, {}", is_ok, tag, ok_idx));
        let ok_label = self.new_label("try_ok");
        let err_label = self.new_label("try_err");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", err_label));
        let err_payload = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            err_payload,
            llvm_type(&result.ty),
            result_repr.as_str(),
            err_idx + 1
        ));
        let err_value = Value {
            ty: err_payload_ty,
            repr: Some(err_payload),
        };
        let ret_enum = self.build_enum_variant_value(
            &fn_ret_layout,
            fn_err_idx,
            Some(&err_value),
            span,
            fctx,
        )?;
        fctx.lines
            .push(format!("  ret {} {}", llvm_type(&fctx.ret_ty), ret_enum));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.current_label = ok_label;
        if ok_payload_ty == LType::Unit {
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        let ok_payload = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            ok_payload,
            llvm_type(&result.ty),
            result_repr.as_str(),
            ok_idx + 1
        ));
        Some(Value {
            ty: ok_payload_ty,
            repr: Some(ok_payload),
        })
    }

    fn build_enum_variant_value(
        &mut self,
        layout: &EnumLayoutType,
        variant_index: usize,
        payload: Option<&Value>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<String> {
        let ty = LType::Enum(layout.clone());
        let mut acc = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} undef, i32 {}, 0",
            acc,
            llvm_type(&ty),
            variant_index
        ));

        for (idx, variant) in layout.variants.iter().enumerate() {
            let (slot_ty, slot_repr) = if let Some(payload_ty) = &variant.payload {
                if idx == variant_index {
                    if let Some(value) = payload {
                        if value.ty == *payload_ty {
                            (
                                llvm_type(payload_ty),
                                value
                                    .repr
                                    .clone()
                                    .unwrap_or_else(|| default_value(payload_ty)),
                            )
                        } else {
                            self.diagnostics.push(Diagnostic::error(
                                "E5022",
                                format!(
                                    "variant '{}' payload expects '{}', found '{}'",
                                    variant.name,
                                    render_type(payload_ty),
                                    render_type(&value.ty)
                                ),
                                self.file,
                                span,
                            ));
                            return None;
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5022",
                            format!(
                                "variant '{}' requires payload in `?` lowering",
                                variant.name
                            ),
                            self.file,
                            span,
                        ));
                        return None;
                    }
                } else {
                    (llvm_type(payload_ty), default_value(payload_ty))
                }
            } else {
                ("i8".to_string(), "0".to_string())
            };

            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} {}, {} {}, {}",
                reg,
                llvm_type(&ty),
                acc,
                slot_ty,
                slot_repr,
                idx + 1
            ));
            acc = reg;
        }
        Some(acc)
    }

    fn gen_if(
        &mut self,
        cond_expr: &ir::Expr,
        then_block: &ir::Block,
        else_block: &ir::Block,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let cond = self.gen_expr(cond_expr, fctx)?;
        if cond.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5015",
                "if condition must be Bool in codegen",
                self.file,
                cond_expr.span,
            ));
            return None;
        }

        let then_label = self.new_label("if_then");
        let else_label = self.new_label("if_else");
        let cont_label = self.new_label("if_cont");

        let mut result_ty: Option<LType> = None;
        let mut result_slot: Option<String> = None;

        let cond_repr = cond.repr.unwrap_or_else(|| "0".to_string());
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            cond_repr, then_label, else_label
        ));

        // Then branch
        let saved_scope = fctx.vars.clone();
        let saved_terminated = fctx.terminated;
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", then_label));
        fctx.current_label = then_label.clone();
        let then_value = self.gen_block(then_block, fctx);
        let then_terminated = fctx.terminated;
        if !then_terminated {
            if let Some(value) = then_value {
                if value.ty != LType::Unit {
                    if result_slot.is_none() {
                        let ptr = self.alloc_entry_slot(&value.ty, fctx);
                        result_ty = Some(value.ty.clone());
                        result_slot = Some(ptr);
                    }
                    if let (Some(slot), Some(expected_ty)) =
                        (result_slot.as_ref(), result_ty.as_ref())
                    {
                        if value.ty != *expected_ty {
                            self.diagnostics.push(Diagnostic::error(
                                "E5007",
                                "if expression branches resolved to incompatible types",
                                self.file,
                                then_block.span,
                            ));
                        }
                        let repr = coerce_repr(&value, expected_ty);
                        fctx.lines.push(format!(
                            "  store {} {}, {}* {}",
                            llvm_type(expected_ty),
                            repr,
                            llvm_type(expected_ty),
                            slot
                        ));
                    }
                }
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        // Else branch
        fctx.vars = saved_scope.clone();
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", else_label));
        fctx.current_label = else_label.clone();
        let else_value = self.gen_block(else_block, fctx);
        let else_terminated = fctx.terminated;
        if !else_terminated {
            if let Some(value) = else_value {
                if value.ty != LType::Unit {
                    if result_slot.is_none() {
                        let ptr = self.alloc_entry_slot(&value.ty, fctx);
                        result_ty = Some(value.ty.clone());
                        result_slot = Some(ptr);
                    }
                    if let (Some(slot), Some(expected_ty)) =
                        (result_slot.as_ref(), result_ty.as_ref())
                    {
                        if value.ty != *expected_ty {
                            self.diagnostics.push(Diagnostic::error(
                                "E5007",
                                "if expression branches resolved to incompatible types",
                                self.file,
                                else_block.span,
                            ));
                        }
                        let repr = coerce_repr(&value, expected_ty);
                        fctx.lines.push(format!(
                            "  store {} {}, {}* {}",
                            llvm_type(expected_ty),
                            repr,
                            llvm_type(expected_ty),
                            slot
                        ));
                    }
                }
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        fctx.vars = saved_scope;
        fctx.terminated = saved_terminated;

        if then_terminated && else_terminated {
            // expression is unreachable from both branches
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        fctx.lines.push(format!("{}:", cont_label));
        fctx.current_label = cont_label;

        if let (Some(slot), Some(result_ty)) = (result_slot, result_ty) {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                reg,
                llvm_type(&result_ty),
                llvm_type(&result_ty),
                slot
            ));
            Some(Value {
                ty: result_ty,
                repr: Some(reg),
            })
        } else {
            Some(Value {
                ty: LType::Unit,
                repr: None,
            })
        }
    }

    fn gen_match(
        &mut self,
        scrutinee_expr: &ir::Expr,
        arms: &[ir::MatchArm],
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let scrutinee = self.gen_expr(scrutinee_expr, fctx)?;

        match scrutinee.ty.clone() {
            LType::Bool => self.gen_match_bool(scrutinee, arms, fctx),
            LType::Enum(layout) => self.gen_match_enum(scrutinee, &layout, arms, fctx),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E5016",
                    "match codegen currently supports Bool and Enum-like ADTs",
                    self.file,
                    scrutinee_expr.span,
                ));
                None
            }
        }
    }

    fn gen_match_bool(
        &mut self,
        scrutinee: Value,
        arms: &[ir::MatchArm],
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if let Some(guard) = arms.iter().find_map(|arm| arm.guard.as_ref()) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E5023",
                    "match guards are not supported by LLVM backend yet",
                    self.file,
                    guard.span,
                )
                .with_help("remove the guard or evaluate guard logic outside the match"),
            );
            return None;
        }

        let Some((true_arm, true_pattern)) = arms.iter().find_map(|arm| {
            self.select_bool_pattern(&arm.pattern, true)
                .map(|p| (arm, p))
        }) else {
            self.diagnostics.push(Diagnostic::error(
                "E5016",
                "non-exhaustive bool match reached codegen for `true` branch",
                self.file,
                crate::span::Span::new(0, 0),
            ));
            return None;
        };

        let Some((false_arm, false_pattern)) = arms.iter().find_map(|arm| {
            self.select_bool_pattern(&arm.pattern, false)
                .map(|p| (arm, p))
        }) else {
            self.diagnostics.push(Diagnostic::error(
                "E5016",
                "non-exhaustive bool match reached codegen for `false` branch",
                self.file,
                crate::span::Span::new(0, 0),
            ));
            return None;
        };

        let then_label = self.new_label("match_true");
        let else_label = self.new_label("match_false");
        let cont_label = self.new_label("match_cont");

        let mut result_ty: Option<LType> = None;
        let mut result_slot: Option<String> = None;

        let cond_repr = scrutinee.repr.unwrap_or_else(|| "0".to_string());
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            cond_repr, then_label, else_label
        ));

        let saved_scope = fctx.vars.clone();
        let saved_terminated = fctx.terminated;

        fctx.terminated = false;
        fctx.lines.push(format!("{}:", then_label));
        self.bind_bool_match_pattern(true_pattern, true, fctx);
        let tv = self.gen_expr(&true_arm.body, fctx);
        let t_term = fctx.terminated;
        if !t_term {
            if let Some(tv) = tv {
                if tv.ty != LType::Unit {
                    if result_slot.is_none() {
                        let ptr = self.alloc_entry_slot(&tv.ty, fctx);
                        result_ty = Some(tv.ty.clone());
                        result_slot = Some(ptr);
                    }
                    if let (Some(slot), Some(expected_ty)) =
                        (result_slot.as_ref(), result_ty.as_ref())
                    {
                        if tv.ty != *expected_ty {
                            self.diagnostics.push(Diagnostic::error(
                                "E5016",
                                "match arms resolved to incompatible types",
                                self.file,
                                true_arm.span,
                            ));
                        }
                        let repr = coerce_repr(&tv, expected_ty);
                        fctx.lines.push(format!(
                            "  store {} {}, {}* {}",
                            llvm_type(expected_ty),
                            repr,
                            llvm_type(expected_ty),
                            slot
                        ));
                    }
                }
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        fctx.vars = saved_scope.clone();
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", else_label));
        self.bind_bool_match_pattern(false_pattern, false, fctx);
        let ev = self.gen_expr(&false_arm.body, fctx);
        let e_term = fctx.terminated;
        if !e_term {
            if let Some(ev) = ev {
                if ev.ty != LType::Unit {
                    if result_slot.is_none() {
                        let ptr = self.alloc_entry_slot(&ev.ty, fctx);
                        result_ty = Some(ev.ty.clone());
                        result_slot = Some(ptr);
                    }
                    if let (Some(slot), Some(expected_ty)) =
                        (result_slot.as_ref(), result_ty.as_ref())
                    {
                        if ev.ty != *expected_ty {
                            self.diagnostics.push(Diagnostic::error(
                                "E5016",
                                "match arms resolved to incompatible types",
                                self.file,
                                false_arm.span,
                            ));
                        }
                        let repr = coerce_repr(&ev, expected_ty);
                        fctx.lines.push(format!(
                            "  store {} {}, {}* {}",
                            llvm_type(expected_ty),
                            repr,
                            llvm_type(expected_ty),
                            slot
                        ));
                    }
                }
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        fctx.vars = saved_scope;
        fctx.terminated = saved_terminated;

        if t_term && e_term {
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        fctx.lines.push(format!("{}:", cont_label));
        if let (Some(slot), Some(result_ty)) = (result_slot, result_ty) {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                reg,
                llvm_type(&result_ty),
                llvm_type(&result_ty),
                slot
            ));
            Some(Value {
                ty: result_ty,
                repr: Some(reg),
            })
        } else {
            Some(Value {
                ty: LType::Unit,
                repr: None,
            })
        }
    }

    fn gen_match_enum(
        &mut self,
        scrutinee: Value,
        layout: &EnumLayoutType,
        arms: &[ir::MatchArm],
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if let Some(guard) = arms.iter().find_map(|arm| arm.guard.as_ref()) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E5023",
                    "match guards are not supported by LLVM backend yet",
                    self.file,
                    guard.span,
                )
                .with_help("remove the guard or evaluate guard logic outside the match"),
            );
            return None;
        }

        let mut selected_arms = Vec::new();
        for variant in &layout.variants {
            let selected = arms.iter().find_map(|arm| {
                self.select_enum_pattern(&arm.pattern, &variant.name)
                    .map(|p| (arm, p))
            });
            let Some((arm, selected_pattern)) = selected else {
                self.diagnostics.push(Diagnostic::error(
                    "E5016",
                    format!(
                        "non-exhaustive enum match reached codegen for '{}' variant '{}'",
                        layout.repr, variant.name
                    ),
                    self.file,
                    crate::span::Span::new(0, 0),
                ));
                return None;
            };
            selected_arms.push((arm, selected_pattern));
        }

        let mut result_ty: Option<LType> = None;
        let mut result_slot: Option<String> = None;

        let tag = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            tag,
            llvm_type(&scrutinee.ty),
            scrutinee
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&scrutinee.ty))
        ));

        let default_label = self.new_label("match_default");
        let cont_label = self.new_label("match_cont");
        let case_labels = layout
            .variants
            .iter()
            .map(|variant| self.new_label(&format!("match_{}", variant.name.to_lowercase())))
            .collect::<Vec<_>>();

        fctx.lines
            .push(format!("  switch i32 {}, label %{} [", tag, default_label));
        for (idx, label) in case_labels.iter().enumerate() {
            fctx.lines
                .push(format!("    i32 {}, label %{}", idx, label));
        }
        fctx.lines.push("  ]".to_string());

        let saved_scope = fctx.vars.clone();
        let saved_terminated = fctx.terminated;

        let mut terminated_all = true;
        for (idx, (arm, selected_pattern)) in selected_arms.iter().enumerate() {
            let variant = &layout.variants[idx];
            fctx.vars = saved_scope.clone();
            fctx.terminated = false;
            fctx.lines.push(format!("{}:", case_labels[idx]));

            match &selected_pattern.kind {
                ir::PatternKind::Var(binding) => {
                    let ptr = self.new_temp();
                    fctx.lines
                        .push(format!("  {} = alloca {}", ptr, llvm_type(&scrutinee.ty)));
                    fctx.lines.push(format!(
                        "  store {} {}, {}* {}",
                        llvm_type(&scrutinee.ty),
                        scrutinee
                            .repr
                            .clone()
                            .unwrap_or_else(|| default_value(&scrutinee.ty)),
                        llvm_type(&scrutinee.ty),
                        ptr
                    ));
                    fctx.vars.last_mut().expect("scope").insert(
                        binding.clone(),
                        Local {
                            ty: scrutinee.ty.clone(),
                            ptr,
                        },
                    );
                }
                ir::PatternKind::Variant { args, .. } => {
                    if let (Some(payload_ty), Some(binding_pat)) = (&variant.payload, args.first())
                    {
                        match &binding_pat.kind {
                            ir::PatternKind::Var(name) => {
                                let payload = self.new_temp();
                                fctx.lines.push(format!(
                                    "  {} = extractvalue {} {}, {}",
                                    payload,
                                    llvm_type(&scrutinee.ty),
                                    scrutinee
                                        .repr
                                        .clone()
                                        .unwrap_or_else(|| default_value(&scrutinee.ty)),
                                    idx + 1
                                ));
                                let ptr = self.new_temp();
                                fctx.lines.push(format!(
                                    "  {} = alloca {}",
                                    ptr,
                                    llvm_type(payload_ty)
                                ));
                                fctx.lines.push(format!(
                                    "  store {} {}, {}* {}",
                                    llvm_type(payload_ty),
                                    payload,
                                    llvm_type(payload_ty),
                                    ptr
                                ));
                                fctx.vars.last_mut().expect("scope").insert(
                                    name.clone(),
                                    Local {
                                        ty: payload_ty.clone(),
                                        ptr,
                                    },
                                );
                            }
                            ir::PatternKind::Wildcard => {}
                            _ => {
                                self.diagnostics.push(Diagnostic::error(
                                    "E5017",
                                    "enum payload pattern codegen supports var or wildcard payload",
                                    self.file,
                                    binding_pat.span,
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }

            let arm_value = self.gen_expr(&arm.body, fctx);
            let arm_terminated = fctx.terminated;
            if !arm_terminated {
                terminated_all = false;
                if let Some(value) = arm_value {
                    if value.ty != LType::Unit {
                        if result_slot.is_none() {
                            let ptr = self.alloc_entry_slot(&value.ty, fctx);
                            result_ty = Some(value.ty.clone());
                            result_slot = Some(ptr);
                        }
                        if let (Some(slot), Some(expected_ty)) =
                            (result_slot.as_ref(), result_ty.as_ref())
                        {
                            if value.ty != *expected_ty {
                                self.diagnostics.push(Diagnostic::error(
                                    "E5016",
                                    "match arms resolved to incompatible types",
                                    self.file,
                                    arm.span,
                                ));
                            }
                            let repr = coerce_repr(&value, expected_ty);
                            fctx.lines.push(format!(
                                "  store {} {}, {}* {}",
                                llvm_type(expected_ty),
                                repr,
                                llvm_type(expected_ty),
                                slot
                            ));
                        }
                    }
                }
                fctx.lines.push(format!("  br label %{}", cont_label));
            }
        }

        fctx.vars = saved_scope;
        fctx.terminated = saved_terminated;
        let default_cont_label = self.new_label("match_default_cont");
        fctx.lines.push(format!("{}:", default_label));
        fctx.lines
            .push(format!("  br label %{}", default_cont_label));
        fctx.lines.push(format!("{}:", default_cont_label));
        fctx.lines.push(format!("  br label %{}", cont_label));

        if terminated_all {
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        fctx.lines.push(format!("{}:", cont_label));
        if let (Some(slot), Some(result_ty)) = (result_slot, result_ty) {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                reg,
                llvm_type(&result_ty),
                llvm_type(&result_ty),
                slot
            ));
            Some(Value {
                ty: result_ty,
                repr: Some(reg),
            })
        } else {
            Some(Value {
                ty: LType::Unit,
                repr: None,
            })
        }
    }

    fn select_bool_pattern<'p>(
        &self,
        pattern: &'p ir::Pattern,
        value: bool,
    ) -> Option<&'p ir::Pattern> {
        match &pattern.kind {
            ir::PatternKind::Bool(v) if *v == value => Some(pattern),
            ir::PatternKind::Wildcard | ir::PatternKind::Var(_) => Some(pattern),
            ir::PatternKind::Or { patterns } => patterns
                .iter()
                .find_map(|part| self.select_bool_pattern(part, value)),
            _ => None,
        }
    }

    fn select_enum_pattern<'p>(
        &self,
        pattern: &'p ir::Pattern,
        variant_name: &str,
    ) -> Option<&'p ir::Pattern> {
        match &pattern.kind {
            ir::PatternKind::Wildcard | ir::PatternKind::Var(_) => Some(pattern),
            ir::PatternKind::Variant { name, .. } if name == variant_name => Some(pattern),
            ir::PatternKind::Or { patterns } => patterns
                .iter()
                .find_map(|part| self.select_enum_pattern(part, variant_name)),
            _ => None,
        }
    }

    fn bind_bool_match_pattern(&mut self, pattern: &ir::Pattern, value: bool, fctx: &mut FnCtx) {
        if let ir::PatternKind::Var(binding) = &pattern.kind {
            let ptr = self.new_temp();
            fctx.lines.push(format!("  {} = alloca i1", ptr));
            let bit = if value { "1" } else { "0" };
            fctx.lines.push(format!("  store i1 {}, i1* {}", bit, ptr));
            fctx.vars.last_mut().expect("scope").insert(
                binding.clone(),
                Local {
                    ty: LType::Bool,
                    ptr,
                },
            );
        }
    }

    fn type_from_id(&mut self, id: ir::TypeId, span: crate::span::Span) -> Option<LType> {
        let Some(repr) = self.type_map.get(&id).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                "E5018",
                format!("unknown type id {} in codegen", id.0),
                self.file,
                span,
            ));
            return None;
        };
        let concrete = if let Some(bindings) = &self.active_type_bindings {
            substitute_type_vars(&repr, bindings)
        } else {
            repr.clone()
        };
        match self.parse_type_repr(&concrete, span) {
            Some(ty) => Some(ty),
            None => {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!("unsupported type '{}' in codegen MVP", concrete),
                    self.file,
                    span,
                ));
                None
            }
        }
    }

    fn parse_type_repr(&mut self, repr: &str, span: crate::span::Span) -> Option<LType> {
        let repr = repr.trim();
        match repr {
            "Int" => return Some(LType::Int),
            "Bool" => return Some(LType::Bool),
            "String" => return Some(LType::String),
            "()" => return Some(LType::Unit),
            _ => {}
        }

        let base = base_type_name(repr);
        let arg_texts = extract_generic_args(repr).unwrap_or_default();

        if let Some(template) = self.struct_templates.get(base).cloned() {
            if template.generics.len() != arg_texts.len() {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!(
                        "generic arity mismatch for struct '{}': expected {}, found {}",
                        base,
                        template.generics.len(),
                        arg_texts.len()
                    ),
                    self.file,
                    span,
                ));
                return None;
            }

            let args = arg_texts
                .iter()
                .map(|text| self.parse_type_repr(text, span))
                .collect::<Option<Vec<_>>>()?;

            let mut bindings = BTreeMap::new();
            for (generic, arg) in template.generics.iter().zip(args.iter()) {
                bindings.insert(generic.clone(), render_type(arg));
            }

            let mut fields = Vec::new();
            for (name, field_ty) in template.fields {
                let concrete = substitute_type_vars(&field_ty, &bindings);
                let ty = self.parse_type_repr(&concrete, span)?;
                fields.push(StructFieldType { name, ty });
            }

            return Some(LType::Struct(StructLayoutType {
                repr: render_applied_type(base, &args),
                fields,
            }));
        }

        if let Some(template) = self.enum_templates.get(base).cloned() {
            if template.generics.len() != arg_texts.len() {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!(
                        "generic arity mismatch for enum '{}': expected {}, found {}",
                        base,
                        template.generics.len(),
                        arg_texts.len()
                    ),
                    self.file,
                    span,
                ));
                return None;
            }

            let args = arg_texts
                .iter()
                .map(|text| self.parse_type_repr(text, span))
                .collect::<Option<Vec<_>>>()?;

            let mut bindings = BTreeMap::new();
            for (generic, arg) in template.generics.iter().zip(args.iter()) {
                bindings.insert(generic.clone(), render_type(arg));
            }

            let mut variants = Vec::new();
            for (name, payload) in template.variants {
                let payload_ty = if let Some(raw) = payload {
                    let concrete = substitute_type_vars(&raw, &bindings);
                    Some(self.parse_type_repr(&concrete, span)?)
                } else {
                    None
                };
                variants.push(EnumVariantType {
                    name,
                    payload: payload_ty,
                });
            }

            return Some(LType::Enum(EnumLayoutType {
                repr: render_applied_type(base, &args),
                variants,
            }));
        }

        None
    }

    fn string_literal(&mut self, s: &str, fctx: &mut FnCtx) -> Value {
        let id = self.string_counter;
        self.string_counter += 1;
        let name = format!("@.str.{}", id);
        let (bytes, len_with_nul) = escape_c_string_bytes(s);
        let len = len_with_nul.saturating_sub(1) as i64;
        let const_text = format!(
            "{} = private unnamed_addr constant [{} x i8] c\"{}\"",
            name, len_with_nul, bytes
        );
        self.globals.push(const_text);

        let ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            ptr, len_with_nul, len_with_nul, name
        ));

        let ty = LType::String;
        let reg0 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} undef, i8* {}, 0",
            reg0,
            llvm_type(&ty),
            ptr
        ));
        let reg1 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} {}, i64 {}, 1",
            reg1,
            llvm_type(&ty),
            reg0,
            len
        ));
        let reg2 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} {}, i64 {}, 2",
            reg2,
            llvm_type(&ty),
            reg1,
            len
        ));
        Value {
            ty,
            repr: Some(reg2),
        }
    }

    fn alloc_entry_slot(&mut self, ty: &LType, fctx: &mut FnCtx) -> String {
        let ptr = self.new_temp();
        let line = format!("  {} = alloca {}", ptr, llvm_type(ty));
        let mut insert_at = 1usize;
        while insert_at < fctx.lines.len() {
            let text = fctx.lines[insert_at].trim_start();
            if !text.starts_with('%') || !text.contains("alloca") {
                break;
            }
            insert_at += 1;
        }
        fctx.lines.insert(insert_at, line);
        ptr
    }

    fn string_parts(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String)> {
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "expected String value in codegen string ABI path",
                self.file,
                span,
            ));
            return None;
        }
        let repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            ptr,
            llvm_type(&value.ty),
            repr
        ));
        let repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 1",
            len,
            llvm_type(&value.ty),
            repr
        ));
        let repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let cap = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 2",
            cap,
            llvm_type(&value.ty),
            repr
        ));
        Some((ptr, len, cap))
    }

    fn span_line_col(&self, span: crate::span::Span) -> (u64, u64) {
        if let Some(source_map) = &self.source_map {
            source_map.line_col(span.start)
        } else {
            (0, 0)
        }
    }

    fn emit_panic_call(
        &mut self,
        ptr: &str,
        len: &str,
        cap: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) {
        let (line, column) = self.span_line_col(span);
        let mut call = format!(
            "  call void @aic_rt_panic(i8* {}, i64 {}, i64 {}, i64 {}, i64 {})",
            ptr, len, cap, line, column
        );
        if let (Some(scope), Some(debug)) = (fctx.debug_scope, self.debug.as_mut()) {
            let location = debug.new_location(line, column, scope);
            call.push_str(&format!(", !dbg !{location}"));
        }
        fctx.lines.push(call);
    }

    fn new_temp(&mut self) -> String {
        let n = self.temp_counter;
        self.temp_counter += 1;
        format!("%t{}", n)
    }

    fn new_label(&mut self, prefix: &str) -> String {
        let n = self.label_counter;
        self.label_counter += 1;
        format!("{}_{}", prefix, n)
    }
}

#[derive(Debug, Clone)]
struct FnCtx {
    lines: Vec<String>,
    vars: Vec<BTreeMap<String, Local>>,
    terminated: bool,
    current_label: String,
    ret_ty: LType,
    debug_scope: Option<usize>,
}

fn find_local(scopes: &[BTreeMap<String, Local>], name: &str) -> Option<Local> {
    for scope in scopes.iter().rev() {
        if let Some(local) = scope.get(name) {
            return Some(local.clone());
        }
    }
    None
}

fn extract_callee_path(callee: &ir::Expr) -> Option<Vec<String>> {
    fn walk(expr: &ir::Expr, out: &mut Vec<String>) -> bool {
        match &expr.kind {
            ir::ExprKind::Var(name) => {
                out.push(name.clone());
                true
            }
            ir::ExprKind::FieldAccess { base, field } => {
                if !walk(base, out) {
                    return false;
                }
                out.push(field.clone());
                true
            }
            _ => false,
        }
    }

    let mut out = Vec::new();
    if walk(callee, &mut out) {
        Some(out)
    } else {
        None
    }
}

fn coerce_repr(value: &Value, expected: &LType) -> String {
    if value.ty == *expected {
        return value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(expected));
    }
    default_value(expected)
}

fn llvm_type(ty: &LType) -> String {
    match ty {
        LType::Int => "i64".to_string(),
        LType::Bool => "i1".to_string(),
        LType::Unit => "void".to_string(),
        LType::String => "{ i8*, i64, i64 }".to_string(),
        LType::Struct(layout) => {
            if layout.fields.is_empty() {
                "{}".to_string()
            } else {
                let fields = layout
                    .fields
                    .iter()
                    .map(|field| llvm_type(&field.ty))
                    .collect::<Vec<_>>();
                format!("{{ {} }}", fields.join(", "))
            }
        }
        LType::Enum(layout) => {
            let mut parts = Vec::new();
            parts.push("i32".to_string());
            for variant in &layout.variants {
                parts.push(match &variant.payload {
                    Some(payload) => llvm_type(payload),
                    None => "i8".to_string(),
                });
            }
            format!("{{ {} }}", parts.join(", "))
        }
    }
}

fn default_value(ty: &LType) -> String {
    match ty {
        LType::Int => "0".to_string(),
        LType::Bool => "0".to_string(),
        LType::Unit => String::new(),
        LType::String => "{ i8* null, i64 0, i64 0 }".to_string(),
        LType::Struct(layout) => {
            if layout.fields.is_empty() {
                "{}".to_string()
            } else {
                let fields = layout
                    .fields
                    .iter()
                    .map(|field| format!("{} {}", llvm_type(&field.ty), default_value(&field.ty)))
                    .collect::<Vec<_>>();
                format!("{{ {} }}", fields.join(", "))
            }
        }
        LType::Enum(layout) => {
            let mut fields = vec!["i32 0".to_string()];
            for variant in &layout.variants {
                match &variant.payload {
                    Some(payload) => {
                        fields.push(format!("{} {}", llvm_type(payload), default_value(payload)))
                    }
                    None => fields.push("i8 0".to_string()),
                }
            }
            format!("{{ {} }}", fields.join(", "))
        }
    }
}

fn render_type(ty: &LType) -> String {
    match ty {
        LType::Int => "Int".to_string(),
        LType::Bool => "Bool".to_string(),
        LType::Unit => "()".to_string(),
        LType::String => "String".to_string(),
        LType::Struct(layout) => layout.repr.clone(),
        LType::Enum(layout) => layout.repr.clone(),
    }
}

fn render_applied_type(base: &str, args: &[LType]) -> String {
    let parts = args.iter().map(render_type).collect::<Vec<_>>();
    render_applied_type_from_parts(base, &parts)
}

fn render_applied_type_from_parts(base: &str, args: &[String]) -> String {
    if args.is_empty() {
        base.to_string()
    } else {
        format!("{base}[{}]", args.join(", "))
    }
}

fn infer_generic_bindings(
    expected: &str,
    found: &str,
    generic_params: &[String],
    bindings: &mut BTreeMap<String, String>,
) -> bool {
    let expected = expected.trim();
    let found = found.trim();
    if generic_params.iter().any(|g| g == expected) {
        if let Some(existing) = bindings.get(expected) {
            return existing == found;
        }
        bindings.insert(expected.to_string(), found.to_string());
        return true;
    }

    let expected_args = extract_generic_args(expected).unwrap_or_default();
    let found_args = extract_generic_args(found).unwrap_or_default();
    if expected_args.is_empty() || found_args.is_empty() {
        return expected == found;
    }
    if base_type_name(expected) != base_type_name(found) || expected_args.len() != found_args.len()
    {
        return false;
    }
    for (exp, got) in expected_args.iter().zip(found_args.iter()) {
        if !infer_generic_bindings(exp, got, generic_params, bindings) {
            return false;
        }
    }
    true
}

fn substitute_type_vars(ty: &str, bindings: &BTreeMap<String, String>) -> String {
    let ty = ty.trim();
    if let Some(bound) = bindings.get(ty) {
        return bound.clone();
    }

    let Some(args) = extract_generic_args(ty) else {
        return ty.to_string();
    };
    let base = base_type_name(ty);
    let mapped = args
        .iter()
        .map(|arg| substitute_type_vars(arg, bindings))
        .collect::<Vec<_>>();
    render_applied_type_from_parts(base, &mapped)
}

fn base_type_name(ty: &str) -> &str {
    let ty = ty.trim();
    match ty.find('[') {
        Some(idx) => ty[..idx].trim(),
        None => ty,
    }
}

fn extract_generic_args(ty: &str) -> Option<Vec<String>> {
    let ty = ty.trim();
    let start = ty.find('[')?;
    let end = ty.rfind(']')?;
    if end <= start {
        return None;
    }
    if !ty[end + 1..].trim().is_empty() {
        return None;
    }
    Some(split_top_level(&ty[start + 1..end]))
}

fn split_top_level(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in text.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(text[start..idx].trim().to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        parts.push(tail.to_string());
    }
    parts
}

fn mangle(name: &str) -> String {
    let mut out = String::from("aic_");
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

fn escape_llvm_string(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'\\' => out.push_str("\\5C"),
            b'"' => out.push_str("\\22"),
            b'\n' => out.push_str("\\0A"),
            b'\t' => out.push_str("\\09"),
            32..=126 => out.push(byte as char),
            _ => out.push_str(&format!("\\{:02X}", byte)),
        }
    }
    out
}

fn escape_c_string_bytes(text: &str) -> (String, usize) {
    let mut out = String::new();
    let mut len = 0usize;
    for b in text.bytes() {
        len += 1;
        match b {
            b'\\' => out.push_str("\\5C"),
            b'"' => out.push_str("\\22"),
            b'\n' => out.push_str("\\0A"),
            b'\t' => out.push_str("\\09"),
            32..=126 => out.push(b as char),
            _ => out.push_str(&format!("\\{:02X}", b)),
        }
    }
    out.push_str("\\00");
    len += 1;
    (out, len)
}

fn runtime_c_source() -> &'static str {
    r#"#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <limits.h>
#include <stdint.h>
#include <sys/stat.h>

#ifdef _WIN32
#include <direct.h>
#include <io.h>
#include <windows.h>
#else
#include <arpa/inet.h>
#include <dirent.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <pthread.h>
#include <regex.h>
#include <unistd.h>
#include <signal.h>
#include <time.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/wait.h>
#endif

typedef struct {
    const char* ptr;
    long len;
    long cap;
} AicString;

typedef struct {
    unsigned char* ptr;
    long len;
    long cap;
} AicVec;

void aic_rt_print_int(long x) {
    printf("%ld\n", x);
}

void aic_rt_print_str(const char* ptr, long len, long cap) {
    (void)cap;
    if (ptr == NULL) {
        printf("<null>\n");
        return;
    }
    if (len < 0) {
        printf("<invalid-string>\n");
        return;
    }
    fwrite(ptr, 1, (size_t)len, stdout);
    fputc('\n', stdout);
}

long aic_rt_strlen(const char* ptr, long len, long cap) {
    (void)cap;
    if (ptr == NULL || len < 0) {
        return 0;
    }
    return len;
}

long aic_rt_vec_len(unsigned char* ptr, long len, long cap) {
    (void)ptr;
    (void)cap;
    if (len < 0) {
        return 0;
    }
    return len;
}

long aic_rt_vec_cap(unsigned char* ptr, long len, long cap) {
    (void)ptr;
    (void)len;
    if (cap < 0) {
        return 0;
    }
    return cap;
}

long aic_rt_time_now_ms(void) {
#ifdef _WIN32
    FILETIME ft;
    ULARGE_INTEGER ticks;
    GetSystemTimeAsFileTime(&ft);
    ticks.LowPart = ft.dwLowDateTime;
    ticks.HighPart = ft.dwHighDateTime;
    unsigned long long millis_since_windows_epoch = ticks.QuadPart / 10000ULL;
    const unsigned long long unix_epoch_offset_ms = 11644473600000ULL;
    if (millis_since_windows_epoch < unix_epoch_offset_ms) {
        return 0;
    }
    return (long)(millis_since_windows_epoch - unix_epoch_offset_ms);
#else
    struct timeval tv;
    if (gettimeofday(&tv, NULL) != 0) {
        return 0;
    }
    return (long)(tv.tv_sec * 1000L + tv.tv_usec / 1000L);
#endif
}

long aic_rt_time_monotonic_ms(void) {
#ifdef _WIN32
    return (long)GetTickCount64();
#else
#ifdef CLOCK_MONOTONIC
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) == 0) {
        return (long)(ts.tv_sec * 1000L + ts.tv_nsec / 1000000L);
    }
#endif
    return aic_rt_time_now_ms();
#endif
}

void aic_rt_time_sleep_ms(long ms) {
    if (ms <= 0) {
        return;
    }
#ifdef _WIN32
    if (ms > 0x7fffffffL) {
        ms = 0x7fffffffL;
    }
    Sleep((DWORD)ms);
#else
    struct timespec req;
    req.tv_sec = (time_t)(ms / 1000);
    req.tv_nsec = (long)((ms % 1000) * 1000000L);
    while (nanosleep(&req, &req) != 0) {
        if (errno != EINTR) {
            break;
        }
    }
#endif
}

static unsigned long long aic_rt_rand_state = 0x9e3779b97f4a7c15ULL;
static int aic_rt_rand_seeded = 0;

static unsigned long long aic_rt_rand_step(void) {
    unsigned long long x = aic_rt_rand_state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    aic_rt_rand_state = x;
    return x * 0x2545F4914F6CDD1DULL;
}

static void aic_rt_rand_ensure_seeded(void) {
    if (aic_rt_rand_seeded) {
        return;
    }
    unsigned long long seed = (unsigned long long)aic_rt_time_now_ms();
    seed ^= ((unsigned long long)aic_rt_time_monotonic_ms() << 1);
    seed ^= 0xa1c0de5eedULL;
    if (seed == 0) {
        seed = 0x9e3779b97f4a7c15ULL;
    }
    aic_rt_rand_state = seed;
    aic_rt_rand_seeded = 1;
}

void aic_rt_rand_seed(long seed) {
    unsigned long long state = (unsigned long long)seed;
    if (state == 0) {
        state = 0x9e3779b97f4a7c15ULL;
    }
    aic_rt_rand_state = state;
    aic_rt_rand_seeded = 1;
}

long aic_rt_rand_next(void) {
    aic_rt_rand_ensure_seeded();
    return (long)(aic_rt_rand_step() & 0x7FFFFFFFFFFFFFFFULL);
}

long aic_rt_rand_range(long min_inclusive, long max_exclusive) {
    if (max_exclusive <= min_inclusive) {
        return min_inclusive;
    }
    unsigned long long span =
        (unsigned long long)max_exclusive - (unsigned long long)min_inclusive;
    unsigned long long value = (unsigned long long)aic_rt_rand_next();
    unsigned long long offset = value % span;
    return min_inclusive + (long)offset;
}

static long aic_rt_fs_map_errno(int err) {
    switch (err) {
        case ENOENT:
            return 1;  // NotFound
        case EACCES:
        case EPERM:
            return 2;  // PermissionDenied
        case EEXIST:
            return 3;  // AlreadyExists
        case EINVAL:
        #ifdef ENAMETOOLONG
        case ENAMETOOLONG:
        #endif
            return 4;  // InvalidInput
        default:
            return 5;  // Io
    }
}

#ifdef _WIN32
static long aic_rt_fs_map_win_error(DWORD err) {
    switch (err) {
        case ERROR_FILE_NOT_FOUND:
        case ERROR_PATH_NOT_FOUND:
            return 1;
        case ERROR_ACCESS_DENIED:
            return 2;
        case ERROR_ALREADY_EXISTS:
        case ERROR_FILE_EXISTS:
            return 3;
        case ERROR_INVALID_NAME:
        case ERROR_INVALID_PARAMETER:
            return 4;
        default:
            return 5;
    }
}
#endif

static char* aic_rt_fs_copy_slice(const char* ptr, long len) {
    if (ptr == NULL || len < 0) {
        return NULL;
    }
    size_t n = (size_t)len;
    char* out = (char*)malloc(n + 1);
    if (out == NULL) {
        return NULL;
    }
    if (n > 0) {
        memcpy(out, ptr, n);
    }
    out[n] = '\0';
    return out;
}

static int aic_rt_fs_invalid_input_path(const char* path) {
    return path == NULL || path[0] == '\0';
}

long aic_rt_fs_exists(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 0;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 0;
    }
    struct stat info;
    int ok = stat(path, &info) == 0;
    free(path);
    return ok ? 1 : 0;
}

long aic_rt_fs_read_text(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_len
) {
    (void)path_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }

    FILE* f = fopen(path, "rb");
    free(path);
    if (f == NULL) {
        return aic_rt_fs_map_errno(errno);
    }

    if (fseek(f, 0, SEEK_END) != 0) {
        int err = errno;
        fclose(f);
        return aic_rt_fs_map_errno(err);
    }
    long size = ftell(f);
    if (size < 0) {
        int err = errno;
        fclose(f);
        return aic_rt_fs_map_errno(err);
    }
    if (fseek(f, 0, SEEK_SET) != 0) {
        int err = errno;
        fclose(f);
        return aic_rt_fs_map_errno(err);
    }

    char* buffer = (char*)malloc((size_t)size + 1);
    if (buffer == NULL) {
        fclose(f);
        return 5;
    }

    size_t read_n = fread(buffer, 1, (size_t)size, f);
    if (read_n != (size_t)size && ferror(f)) {
        int err = errno;
        free(buffer);
        fclose(f);
        return aic_rt_fs_map_errno(err);
    }
    fclose(f);

    buffer[read_n] = '\0';
    if (out_ptr != NULL) {
        *out_ptr = buffer;
    }
    if (out_len != NULL) {
        *out_len = (long)read_n;
    }
    return 0;
}

long aic_rt_fs_write_text(
    const char* path_ptr,
    long path_len,
    long path_cap,
    const char* content_ptr,
    long content_len,
    long content_cap
) {
    (void)path_cap;
    (void)content_cap;
    if (content_len < 0 || (content_len > 0 && content_ptr == NULL)) {
        return 4;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }

    FILE* f = fopen(path, "wb");
    free(path);
    if (f == NULL) {
        return aic_rt_fs_map_errno(errno);
    }

    size_t target = (size_t)content_len;
    if (target > 0) {
        size_t written = fwrite(content_ptr, 1, target, f);
        if (written != target) {
            int err = errno;
            fclose(f);
            return aic_rt_fs_map_errno(err);
        }
    }

    if (fclose(f) != 0) {
        return aic_rt_fs_map_errno(errno);
    }
    return 0;
}

long aic_rt_fs_append_text(
    const char* path_ptr,
    long path_len,
    long path_cap,
    const char* content_ptr,
    long content_len,
    long content_cap
) {
    (void)path_cap;
    (void)content_cap;
    if (content_len < 0 || (content_len > 0 && content_ptr == NULL)) {
        return 4;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }

    FILE* f = fopen(path, "ab");
    free(path);
    if (f == NULL) {
        return aic_rt_fs_map_errno(errno);
    }

    size_t target = (size_t)content_len;
    if (target > 0) {
        size_t written = fwrite(content_ptr, 1, target, f);
        if (written != target) {
            int err = errno;
            fclose(f);
            return aic_rt_fs_map_errno(err);
        }
    }

    if (fclose(f) != 0) {
        return aic_rt_fs_map_errno(errno);
    }
    return 0;
}

long aic_rt_fs_copy(
    const char* from_ptr,
    long from_len,
    long from_cap,
    const char* to_ptr,
    long to_len,
    long to_cap
) {
    (void)from_cap;
    (void)to_cap;
    char* from_path = aic_rt_fs_copy_slice(from_ptr, from_len);
    char* to_path = aic_rt_fs_copy_slice(to_ptr, to_len);
    if (from_path == NULL || to_path == NULL) {
        free(from_path);
        free(to_path);
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(from_path) || aic_rt_fs_invalid_input_path(to_path)) {
        free(from_path);
        free(to_path);
        return 4;
    }

    FILE* in = fopen(from_path, "rb");
    if (in == NULL) {
        int err = errno;
        free(from_path);
        free(to_path);
        return aic_rt_fs_map_errno(err);
    }
    FILE* out = fopen(to_path, "wb");
    if (out == NULL) {
        int err = errno;
        fclose(in);
        free(from_path);
        free(to_path);
        return aic_rt_fs_map_errno(err);
    }

    unsigned char buf[4096];
    while (1) {
        size_t n = fread(buf, 1, sizeof(buf), in);
        if (n > 0) {
            size_t written = fwrite(buf, 1, n, out);
            if (written != n) {
                int err = errno;
                fclose(in);
                fclose(out);
                free(from_path);
                free(to_path);
                return aic_rt_fs_map_errno(err);
            }
        }
        if (n < sizeof(buf)) {
            if (ferror(in)) {
                int err = errno;
                fclose(in);
                fclose(out);
                free(from_path);
                free(to_path);
                return aic_rt_fs_map_errno(err);
            }
            break;
        }
    }

    if (fclose(in) != 0 || fclose(out) != 0) {
        int err = errno;
        free(from_path);
        free(to_path);
        return aic_rt_fs_map_errno(err);
    }

    free(from_path);
    free(to_path);
    return 0;
}

long aic_rt_fs_move(
    const char* from_ptr,
    long from_len,
    long from_cap,
    const char* to_ptr,
    long to_len,
    long to_cap
) {
    (void)from_cap;
    (void)to_cap;
    char* from_path = aic_rt_fs_copy_slice(from_ptr, from_len);
    char* to_path = aic_rt_fs_copy_slice(to_ptr, to_len);
    if (from_path == NULL || to_path == NULL) {
        free(from_path);
        free(to_path);
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(from_path) || aic_rt_fs_invalid_input_path(to_path)) {
        free(from_path);
        free(to_path);
        return 4;
    }
    int rc = rename(from_path, to_path);
    int err = errno;
    free(from_path);
    free(to_path);
    if (rc != 0) {
        return aic_rt_fs_map_errno(err);
    }
    return 0;
}

long aic_rt_fs_delete(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }
    int rc = remove(path);
    int err = errno;
    free(path);
    if (rc != 0) {
        return aic_rt_fs_map_errno(err);
    }
    return 0;
}

long aic_rt_fs_metadata(
    const char* path_ptr,
    long path_len,
    long path_cap,
    long* out_is_file,
    long* out_is_dir,
    long* out_size
) {
    (void)path_cap;
    if (out_is_file != NULL) {
        *out_is_file = 0;
    }
    if (out_is_dir != NULL) {
        *out_is_dir = 0;
    }
    if (out_size != NULL) {
        *out_size = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }
    struct stat info;
    if (stat(path, &info) != 0) {
        int err = errno;
        free(path);
        return aic_rt_fs_map_errno(err);
    }
    free(path);

    if (out_is_file != NULL) {
        *out_is_file = S_ISREG(info.st_mode) ? 1 : 0;
    }
    if (out_is_dir != NULL) {
        *out_is_dir = S_ISDIR(info.st_mode) ? 1 : 0;
    }
    if (out_size != NULL) {
        *out_size = (long)info.st_size;
    }
    return 0;
}

long aic_rt_fs_walk_dir(
    const char* path_ptr,
    long path_len,
    long path_cap,
    long* out_count
) {
    (void)path_cap;
    if (out_count != NULL) {
        *out_count = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 4;
    }
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 4;
    }

#ifdef _WIN32
    size_t n = strlen(path);
    const char* suffix = (n > 0 && (path[n - 1] == '\\' || path[n - 1] == '/')) ? "*" : "\\*";
    size_t pat_len = n + strlen(suffix) + 1;
    char* pattern = (char*)malloc(pat_len);
    if (pattern == NULL) {
        free(path);
        return 5;
    }
    snprintf(pattern, pat_len, "%s%s", path, suffix);

    WIN32_FIND_DATAA entry;
    HANDLE handle = FindFirstFileA(pattern, &entry);
    free(pattern);
    if (handle == INVALID_HANDLE_VALUE) {
        DWORD err = GetLastError();
        free(path);
        return aic_rt_fs_map_win_error(err);
    }

    long count = 0;
    do {
        const char* name = entry.cFileName;
        if (strcmp(name, ".") != 0 && strcmp(name, "..") != 0) {
            count += 1;
        }
    } while (FindNextFileA(handle, &entry) != 0);
    FindClose(handle);
    free(path);
    if (out_count != NULL) {
        *out_count = count;
    }
    return 0;
#else
    DIR* dir = opendir(path);
    if (dir == NULL) {
        int err = errno;
        free(path);
        return aic_rt_fs_map_errno(err);
    }

    long count = 0;
    struct dirent* entry = NULL;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") != 0 && strcmp(entry->d_name, "..") != 0) {
            count += 1;
        }
    }
    int closed = closedir(dir);
    free(path);
    if (closed != 0) {
        return aic_rt_fs_map_errno(errno);
    }
    if (out_count != NULL) {
        *out_count = count;
    }
    return 0;
#endif
}

long aic_rt_fs_temp_file(
    const char* prefix_ptr,
    long prefix_len,
    long prefix_cap,
    char** out_ptr,
    long* out_len
) {
    (void)prefix_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (prefix_len < 0) {
        return 4;
    }

    char* prefix = aic_rt_fs_copy_slice(prefix_ptr, prefix_len);
    if (prefix == NULL && prefix_len > 0) {
        return 5;
    }
    const char* effective_prefix = (prefix != NULL && prefix[0] != '\0') ? prefix : "aic_";

#ifdef _WIN32
    char temp_dir[MAX_PATH + 1];
    DWORD dir_len = GetTempPathA((DWORD)MAX_PATH, temp_dir);
    if (dir_len == 0 || dir_len > MAX_PATH) {
        free(prefix);
        return 5;
    }
    char filename[MAX_PATH + 1];
    UINT rc = GetTempFileNameA(temp_dir, effective_prefix, 0, filename);
    free(prefix);
    if (rc == 0) {
        return aic_rt_fs_map_win_error(GetLastError());
    }
    size_t out_n = strlen(filename);
    char* owned = (char*)malloc(out_n + 1);
    if (owned == NULL) {
        return 5;
    }
    memcpy(owned, filename, out_n + 1);
    if (out_ptr != NULL) {
        *out_ptr = owned;
    }
    if (out_len != NULL) {
        *out_len = (long)out_n;
    }
    return 0;
#else
    const char* tmp = getenv("TMPDIR");
    if (tmp == NULL || tmp[0] == '\0') {
        tmp = "/tmp";
    }
    size_t needed = strlen(tmp) + 1 + strlen(effective_prefix) + 6 + 1;
    char* tmpl = (char*)malloc(needed);
    if (tmpl == NULL) {
        free(prefix);
        return 5;
    }
    snprintf(tmpl, needed, "%s/%sXXXXXX", tmp, effective_prefix);
    int fd = mkstemp(tmpl);
    free(prefix);
    if (fd < 0) {
        int err = errno;
        free(tmpl);
        return aic_rt_fs_map_errno(err);
    }
    close(fd);
    if (out_ptr != NULL) {
        *out_ptr = tmpl;
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(tmpl);
    }
    return 0;
#endif
}

long aic_rt_fs_temp_dir(
    const char* prefix_ptr,
    long prefix_len,
    long prefix_cap,
    char** out_ptr,
    long* out_len
) {
    (void)prefix_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (prefix_len < 0) {
        return 4;
    }

    char* prefix = aic_rt_fs_copy_slice(prefix_ptr, prefix_len);
    if (prefix == NULL && prefix_len > 0) {
        return 5;
    }
    const char* effective_prefix = (prefix != NULL && prefix[0] != '\0') ? prefix : "aic_";

#ifdef _WIN32
    char temp_dir[MAX_PATH + 1];
    DWORD dir_len = GetTempPathA((DWORD)MAX_PATH, temp_dir);
    if (dir_len == 0 || dir_len > MAX_PATH) {
        free(prefix);
        return 5;
    }

    char candidate[MAX_PATH + 1];
    snprintf(candidate, sizeof(candidate), "%s%s%lu", temp_dir, effective_prefix, (unsigned long)GetTickCount());
    if (_mkdir(candidate) != 0) {
        long mapped = aic_rt_fs_map_errno(errno);
        free(prefix);
        return mapped;
    }
    free(prefix);

    size_t out_n = strlen(candidate);
    char* owned = (char*)malloc(out_n + 1);
    if (owned == NULL) {
        return 5;
    }
    memcpy(owned, candidate, out_n + 1);
    if (out_ptr != NULL) {
        *out_ptr = owned;
    }
    if (out_len != NULL) {
        *out_len = (long)out_n;
    }
    return 0;
#else
    const char* tmp = getenv("TMPDIR");
    if (tmp == NULL || tmp[0] == '\0') {
        tmp = "/tmp";
    }
    size_t needed = strlen(tmp) + 1 + strlen(effective_prefix) + 6 + 1;
    char* tmpl = (char*)malloc(needed);
    if (tmpl == NULL) {
        free(prefix);
        return 5;
    }
    snprintf(tmpl, needed, "%s/%sXXXXXX", tmp, effective_prefix);
    free(prefix);
    char* out = mkdtemp(tmpl);
    if (out == NULL) {
        int err = errno;
        free(tmpl);
        return aic_rt_fs_map_errno(err);
    }
    if (out_ptr != NULL) {
        *out_ptr = tmpl;
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(tmpl);
    }
    return 0;
#endif
}

static long aic_rt_env_map_errno(int err) {
    switch (err) {
        case ENOENT:
            return 1;  // NotFound
        case EACCES:
        case EPERM:
            return 2;  // PermissionDenied
        case EINVAL:
        #ifdef ENAMETOOLONG
        case ENAMETOOLONG:
        #endif
            return 3;  // InvalidInput
        default:
            return 4;  // Io
    }
}

static int aic_rt_env_invalid_name(const char* key) {
    if (key == NULL || key[0] == '\0') {
        return 1;
    }
    for (const char* p = key; *p != '\0'; ++p) {
        if (*p == '=') {
            return 1;
        }
    }
    return 0;
}

long aic_rt_env_get(
    const char* key_ptr,
    long key_len,
    long key_cap,
    char** out_ptr,
    long* out_len
) {
    (void)key_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }

    char* key = aic_rt_fs_copy_slice(key_ptr, key_len);
    if (aic_rt_env_invalid_name(key)) {
        free(key);
        return 3;
    }
    const char* value = getenv(key);
    free(key);
    if (value == NULL) {
        return 1;
    }
    size_t n = strlen(value);
    char* owned = (char*)malloc(n + 1);
    if (owned == NULL) {
        return 4;
    }
    memcpy(owned, value, n + 1);
    if (out_ptr != NULL) {
        *out_ptr = owned;
    } else {
        free(owned);
    }
    if (out_len != NULL) {
        *out_len = (long)n;
    }
    return 0;
}

long aic_rt_env_set(
    const char* key_ptr,
    long key_len,
    long key_cap,
    const char* value_ptr,
    long value_len,
    long value_cap
) {
    (void)key_cap;
    (void)value_cap;
    if (value_len < 0 || (value_len > 0 && value_ptr == NULL)) {
        return 3;
    }
    char* key = aic_rt_fs_copy_slice(key_ptr, key_len);
    char* value = aic_rt_fs_copy_slice(value_ptr, value_len);
    if (aic_rt_env_invalid_name(key) || value == NULL) {
        free(key);
        free(value);
        return 3;
    }
#ifdef _WIN32
    if (_putenv_s(key, value) != 0) {
        long mapped = aic_rt_env_map_errno(errno);
        free(key);
        free(value);
        return mapped;
    }
#else
    if (setenv(key, value, 1) != 0) {
        long mapped = aic_rt_env_map_errno(errno);
        free(key);
        free(value);
        return mapped;
    }
#endif
    free(key);
    free(value);
    return 0;
}

long aic_rt_env_remove(const char* key_ptr, long key_len, long key_cap) {
    (void)key_cap;
    char* key = aic_rt_fs_copy_slice(key_ptr, key_len);
    if (aic_rt_env_invalid_name(key)) {
        free(key);
        return 3;
    }
#ifdef _WIN32
    if (_putenv_s(key, "") != 0) {
        long mapped = aic_rt_env_map_errno(errno);
        free(key);
        return mapped;
    }
#else
    if (unsetenv(key) != 0) {
        long mapped = aic_rt_env_map_errno(errno);
        free(key);
        return mapped;
    }
#endif
    free(key);
    return 0;
}

long aic_rt_env_cwd(char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
#ifdef _WIN32
    char buffer[MAX_PATH + 1];
    DWORD n = GetCurrentDirectoryA(MAX_PATH, buffer);
    if (n == 0 || n > MAX_PATH) {
        return aic_rt_env_map_errno(errno);
    }
#else
    char buffer[PATH_MAX];
    if (getcwd(buffer, sizeof(buffer)) == NULL) {
        return aic_rt_env_map_errno(errno);
    }
#endif
    size_t len = strlen(buffer);
    char* owned = (char*)malloc(len + 1);
    if (owned == NULL) {
        return 4;
    }
    memcpy(owned, buffer, len + 1);
    if (out_ptr != NULL) {
        *out_ptr = owned;
    } else {
        free(owned);
    }
    if (out_len != NULL) {
        *out_len = (long)len;
    }
    return 0;
}

long aic_rt_env_set_cwd(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (aic_rt_fs_invalid_input_path(path)) {
        free(path);
        return 3;
    }
#ifdef _WIN32
    int rc = _chdir(path);
#else
    int rc = chdir(path);
#endif
    int err = errno;
    free(path);
    if (rc != 0) {
        return aic_rt_env_map_errno(err);
    }
    return 0;
}

static char* aic_rt_copy_bytes(const char* src, size_t len) {
    char* out = (char*)malloc(len + 1);
    if (out == NULL) {
        return NULL;
    }
    if (len > 0 && src != NULL) {
        memcpy(out, src, len);
    }
    out[len] = '\0';
    return out;
}

static int aic_rt_path_is_sep(char ch) {
    return ch == '/' || ch == '\\';
}

static int aic_rt_path_is_abs_cstr(const char* path) {
    if (path == NULL || path[0] == '\0') {
        return 0;
    }
#ifdef _WIN32
    if (aic_rt_path_is_sep(path[0])) {
        return 1;
    }
    if (((path[0] >= 'A' && path[0] <= 'Z') || (path[0] >= 'a' && path[0] <= 'z')) &&
        path[1] == ':') {
        return 1;
    }
    return 0;
#else
    return path[0] == '/';
#endif
}

static void aic_rt_write_string_out(char** out_ptr, long* out_len, char* owned) {
    long len = 0;
    if (owned != NULL) {
        len = (long)strlen(owned);
    }
    if (out_len != NULL) {
        *out_len = len;
    }
    if (out_ptr != NULL) {
        *out_ptr = owned;
    } else {
        free(owned);
    }
}

void aic_rt_path_join(
    const char* left_ptr,
    long left_len,
    long left_cap,
    const char* right_ptr,
    long right_len,
    long right_cap,
    char** out_ptr,
    long* out_len
) {
    (void)left_cap;
    (void)right_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* left = aic_rt_fs_copy_slice(left_ptr, left_len);
    char* right = aic_rt_fs_copy_slice(right_ptr, right_len);
    if (left == NULL || right == NULL) {
        free(left);
        free(right);
        return;
    }
    if (right[0] == '\0') {
        aic_rt_write_string_out(out_ptr, out_len, left);
        free(right);
        return;
    }
    if (left[0] == '\0' || aic_rt_path_is_abs_cstr(right)) {
        aic_rt_write_string_out(out_ptr, out_len, right);
        free(left);
        return;
    }
    size_t left_n = strlen(left);
    size_t right_n = strlen(right);
    int need_sep = !(aic_rt_path_is_sep(left[left_n - 1]) || aic_rt_path_is_sep(right[0]));
#ifdef _WIN32
    char sep = '\\';
#else
    char sep = '/';
#endif
    size_t out_n = left_n + (need_sep ? 1 : 0) + right_n;
    char* out = (char*)malloc(out_n + 1);
    if (out == NULL) {
        free(left);
        free(right);
        return;
    }
    size_t pos = 0;
    memcpy(out + pos, left, left_n);
    pos += left_n;
    if (need_sep) {
        out[pos++] = sep;
    }
    memcpy(out + pos, right, right_n);
    out[out_n] = '\0';
    free(left);
    free(right);
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_path_basename(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_len
) {
    (void)path_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return;
    }
    size_t n = strlen(path);
    while (n > 0 && aic_rt_path_is_sep(path[n - 1])) {
        n -= 1;
    }
    if (n == 0) {
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes("", 0));
        return;
    }
    size_t start = n;
    while (start > 0 && !aic_rt_path_is_sep(path[start - 1])) {
        start -= 1;
    }
    char* out = aic_rt_copy_bytes(path + start, n - start);
    free(path);
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_path_dirname(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_len
) {
    (void)path_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return;
    }
    size_t n = strlen(path);
    while (n > 0 && aic_rt_path_is_sep(path[n - 1])) {
        n -= 1;
    }
    if (n == 0) {
#ifdef _WIN32
        char* root = aic_rt_copy_bytes("\\", 1);
#else
        char* root = aic_rt_copy_bytes("/", 1);
#endif
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, root);
        return;
    }
    size_t end = n;
    while (end > 0 && !aic_rt_path_is_sep(path[end - 1])) {
        end -= 1;
    }
    if (end == 0) {
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes(".", 1));
        return;
    }
    if (end == 1 && aic_rt_path_is_sep(path[0])) {
        char* root = aic_rt_copy_bytes(path, 1);
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, root);
        return;
    }
    char* out = aic_rt_copy_bytes(path, end - 1);
    free(path);
    aic_rt_write_string_out(out_ptr, out_len, out);
}

void aic_rt_path_extension(
    const char* path_ptr,
    long path_len,
    long path_cap,
    char** out_ptr,
    long* out_len
) {
    (void)path_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return;
    }
    size_t n = strlen(path);
    while (n > 0 && aic_rt_path_is_sep(path[n - 1])) {
        n -= 1;
    }
    if (n == 0) {
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes("", 0));
        return;
    }
    size_t start = n;
    while (start > 0 && !aic_rt_path_is_sep(path[start - 1])) {
        start -= 1;
    }
    const char* name = path + start;
    size_t name_n = n - start;
    const char* dot = NULL;
    for (size_t i = 0; i < name_n; ++i) {
        if (name[i] == '.') {
            dot = &name[i];
        }
    }
    if (dot == NULL || dot == name) {
        free(path);
        aic_rt_write_string_out(out_ptr, out_len, aic_rt_copy_bytes("", 0));
        return;
    }
    size_t ext_n = (size_t)(name + name_n - (dot + 1));
    char* out = aic_rt_copy_bytes(dot + 1, ext_n);
    free(path);
    aic_rt_write_string_out(out_ptr, out_len, out);
}

long aic_rt_path_is_abs(const char* path_ptr, long path_len, long path_cap) {
    (void)path_cap;
    char* path = aic_rt_fs_copy_slice(path_ptr, path_len);
    if (path == NULL) {
        return 0;
    }
    long out = aic_rt_path_is_abs_cstr(path) ? 1 : 0;
    free(path);
    return out;
}

static long aic_rt_proc_map_errno(int err) {
    switch (err) {
        case ENOENT:
            return 1;  // NotFound
        case EACCES:
        case EPERM:
            return 2;  // PermissionDenied
        case EINVAL:
        #ifdef ENAMETOOLONG
        case ENAMETOOLONG:
        #endif
            return 3;  // InvalidInput
        #ifdef ESRCH
        case ESRCH:
            return 5;  // UnknownProcess
        #endif
        #ifdef ECHILD
        case ECHILD:
            return 5;  // UnknownProcess
        #endif
        default:
            return 4;  // Io
    }
}

static char* aic_rt_proc_read_text_file(const char* path, long* out_len) {
    if (out_len != NULL) {
        *out_len = 0;
    }
    FILE* f = fopen(path, "rb");
    if (f == NULL) {
        return NULL;
    }
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    long size = ftell(f);
    if (size < 0) {
        fclose(f);
        return NULL;
    }
    if (fseek(f, 0, SEEK_SET) != 0) {
        fclose(f);
        return NULL;
    }
    char* buffer = (char*)malloc((size_t)size + 1);
    if (buffer == NULL) {
        fclose(f);
        return NULL;
    }
    size_t read_n = fread(buffer, 1, (size_t)size, f);
    fclose(f);
    buffer[read_n] = '\0';
    if (out_len != NULL) {
        *out_len = (long)read_n;
    }
    return buffer;
}

static long aic_rt_proc_make_temp_file_path(const char* prefix, char** out_path) {
    if (out_path == NULL) {
        return 3;
    }
    *out_path = NULL;
#ifdef _WIN32
    char tmp[L_tmpnam];
    if (tmpnam_s(tmp, sizeof(tmp)) != 0) {
        return 4;
    }
    size_t n = strlen(tmp);
    char* out = (char*)malloc(n + 1);
    if (out == NULL) {
        return 4;
    }
    memcpy(out, tmp, n + 1);
    FILE* f = fopen(out, "wb");
    if (f != NULL) {
        fclose(f);
    }
    *out_path = out;
    return 0;
#else
    const char* tmp = getenv("TMPDIR");
    if (tmp == NULL || tmp[0] == '\0') {
        tmp = "/tmp";
    }
    const char* eff = (prefix != NULL && prefix[0] != '\0') ? prefix : "aic_proc_";
    size_t needed = strlen(tmp) + 1 + strlen(eff) + 6 + 1;
    char* tmpl = (char*)malloc(needed);
    if (tmpl == NULL) {
        return 4;
    }
    snprintf(tmpl, needed, "%s/%sXXXXXX", tmp, eff);
    int fd = mkstemp(tmpl);
    if (fd < 0) {
        int err = errno;
        free(tmpl);
        return aic_rt_proc_map_errno(err);
    }
    close(fd);
    *out_path = tmpl;
    return 0;
#endif
}

static long aic_rt_proc_decode_wait_status(int status) {
#ifdef _WIN32
    return (long)status;
#else
    if (WIFEXITED(status)) {
        return (long)WEXITSTATUS(status);
    }
    if (WIFSIGNALED(status)) {
        return 128 + (long)WTERMSIG(status);
    }
    return 1;
#endif
}

static long aic_rt_proc_run_shell(
    const char* command,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    if (out_status != NULL) {
        *out_status = 0;
    }
    if (out_stdout_ptr != NULL) {
        *out_stdout_ptr = NULL;
    }
    if (out_stdout_len != NULL) {
        *out_stdout_len = 0;
    }
    if (out_stderr_ptr != NULL) {
        *out_stderr_ptr = NULL;
    }
    if (out_stderr_len != NULL) {
        *out_stderr_len = 0;
    }
    if (command == NULL || command[0] == '\0') {
        return 3;
    }

    char* stdout_path = NULL;
    char* stderr_path = NULL;
    long mk_out = aic_rt_proc_make_temp_file_path("aic_proc_out_", &stdout_path);
    if (mk_out != 0) {
        free(stdout_path);
        return mk_out;
    }
    long mk_err = aic_rt_proc_make_temp_file_path("aic_proc_err_", &stderr_path);
    if (mk_err != 0) {
        free(stdout_path);
        free(stderr_path);
        return mk_err;
    }

    size_t wrapped_n = strlen(command) + strlen(stdout_path) + strlen(stderr_path) + 40;
    char* wrapped = (char*)malloc(wrapped_n);
    if (wrapped == NULL) {
        remove(stdout_path);
        remove(stderr_path);
        free(stdout_path);
        free(stderr_path);
        return 4;
    }
    snprintf(
        wrapped,
        wrapped_n,
        "( %s ) >\"%s\" 2>\"%s\"",
        command,
        stdout_path,
        stderr_path
    );

    int rc = system(wrapped);
    free(wrapped);
    if (rc == -1) {
        int err = errno;
        remove(stdout_path);
        remove(stderr_path);
        free(stdout_path);
        free(stderr_path);
        return aic_rt_proc_map_errno(err);
    }

    long stdout_n = 0;
    long stderr_n = 0;
    char* stdout_text = aic_rt_proc_read_text_file(stdout_path, &stdout_n);
    char* stderr_text = aic_rt_proc_read_text_file(stderr_path, &stderr_n);
    remove(stdout_path);
    remove(stderr_path);
    free(stdout_path);
    free(stderr_path);
    if (stdout_text == NULL || stderr_text == NULL) {
        free(stdout_text);
        free(stderr_text);
        return 4;
    }

    if (out_status != NULL) {
        *out_status = aic_rt_proc_decode_wait_status(rc);
    }
    if (out_stdout_ptr != NULL) {
        *out_stdout_ptr = stdout_text;
    } else {
        free(stdout_text);
    }
    if (out_stdout_len != NULL) {
        *out_stdout_len = stdout_n;
    }
    if (out_stderr_ptr != NULL) {
        *out_stderr_ptr = stderr_text;
    } else {
        free(stderr_text);
    }
    if (out_stderr_len != NULL) {
        *out_stderr_len = stderr_n;
    }
    return 0;
}

#define AIC_RT_PROC_TABLE_CAP 64
typedef struct {
    int active;
#ifdef _WIN32
    long pid;
#else
    pid_t pid;
#endif
} AicProcSlot;
static AicProcSlot aic_rt_proc_table[AIC_RT_PROC_TABLE_CAP];

long aic_rt_proc_spawn(const char* command_ptr, long command_len, long command_cap, long* out_handle) {
    (void)command_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    char* command = aic_rt_fs_copy_slice(command_ptr, command_len);
    if (command == NULL || command[0] == '\0') {
        free(command);
        return 3;
    }
#ifdef _WIN32
    free(command);
    return 4;
#else
    pid_t pid = fork();
    if (pid < 0) {
        long mapped = aic_rt_proc_map_errno(errno);
        free(command);
        return mapped;
    }
    if (pid == 0) {
        execl("/bin/sh", "sh", "-c", command, (char*)NULL);
        _exit(127);
    }
    free(command);

    long slot = -1;
    for (long i = 0; i < AIC_RT_PROC_TABLE_CAP; ++i) {
        if (!aic_rt_proc_table[i].active) {
            slot = i;
            break;
        }
    }
    if (slot < 0) {
        kill(pid, SIGKILL);
        waitpid(pid, NULL, 0);
        return 4;
    }
    aic_rt_proc_table[slot].active = 1;
    aic_rt_proc_table[slot].pid = pid;
    if (out_handle != NULL) {
        *out_handle = slot + 1;
    }
    return 0;
#endif
}

long aic_rt_proc_wait(long handle, long* out_status) {
    if (out_status != NULL) {
        *out_status = 0;
    }
#ifdef _WIN32
    (void)handle;
    return 5;
#else
    if (handle <= 0 || handle > AIC_RT_PROC_TABLE_CAP) {
        return 5;
    }
    long slot = handle - 1;
    if (!aic_rt_proc_table[slot].active) {
        return 5;
    }
    int status = 0;
    pid_t rc = waitpid(aic_rt_proc_table[slot].pid, &status, 0);
    if (rc < 0) {
        return aic_rt_proc_map_errno(errno);
    }
    aic_rt_proc_table[slot].active = 0;
    if (out_status != NULL) {
        *out_status = aic_rt_proc_decode_wait_status(status);
    }
    return 0;
#endif
}

long aic_rt_proc_kill(long handle) {
#ifdef _WIN32
    (void)handle;
    return 5;
#else
    if (handle <= 0 || handle > AIC_RT_PROC_TABLE_CAP) {
        return 5;
    }
    long slot = handle - 1;
    if (!aic_rt_proc_table[slot].active) {
        return 5;
    }
    if (kill(aic_rt_proc_table[slot].pid, SIGTERM) != 0) {
        return aic_rt_proc_map_errno(errno);
    }
    waitpid(aic_rt_proc_table[slot].pid, NULL, 0);
    aic_rt_proc_table[slot].active = 0;
    return 0;
#endif
}

long aic_rt_proc_run(
    const char* command_ptr,
    long command_len,
    long command_cap,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    (void)command_cap;
    char* command = aic_rt_fs_copy_slice(command_ptr, command_len);
    if (command == NULL || command[0] == '\0') {
        free(command);
        return 3;
    }
    long result = aic_rt_proc_run_shell(
        command,
        out_status,
        out_stdout_ptr,
        out_stdout_len,
        out_stderr_ptr,
        out_stderr_len
    );
    free(command);
    return result;
}

long aic_rt_proc_pipe(
    const char* left_ptr,
    long left_len,
    long left_cap,
    const char* right_ptr,
    long right_len,
    long right_cap,
    long* out_status,
    char** out_stdout_ptr,
    long* out_stdout_len,
    char** out_stderr_ptr,
    long* out_stderr_len
) {
    (void)left_cap;
    (void)right_cap;
    char* left = aic_rt_fs_copy_slice(left_ptr, left_len);
    char* right = aic_rt_fs_copy_slice(right_ptr, right_len);
    if (left == NULL || right == NULL || left[0] == '\0' || right[0] == '\0') {
        free(left);
        free(right);
        return 3;
    }
    size_t command_n = strlen(left) + strlen(right) + 8;
    char* command = (char*)malloc(command_n);
    if (command == NULL) {
        free(left);
        free(right);
        return 4;
    }
    snprintf(command, command_n, "%s | %s", left, right);
    free(left);
    free(right);
    long result = aic_rt_proc_run_shell(
        command,
        out_status,
        out_stdout_ptr,
        out_stdout_len,
        out_stderr_ptr,
        out_stderr_len
    );
    free(command);
    return result;
}

#ifdef _WIN32
long aic_rt_conc_spawn(long value, long delay_ms, long* out_handle) {
    (void)value;
    (void)delay_ms;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_conc_join(long handle, long* out_value) {
    (void)handle;
    if (out_value != NULL) {
        *out_value = 0;
    }
    return 7;
}

long aic_rt_conc_cancel(long handle, long* out_cancelled) {
    (void)handle;
    if (out_cancelled != NULL) {
        *out_cancelled = 0;
    }
    return 7;
}

long aic_rt_conc_channel_int(long capacity, long* out_handle) {
    (void)capacity;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_conc_send_int(long handle, long value, long timeout_ms) {
    (void)handle;
    (void)value;
    (void)timeout_ms;
    return 7;
}

long aic_rt_conc_recv_int(long handle, long timeout_ms, long* out_value) {
    (void)handle;
    (void)timeout_ms;
    if (out_value != NULL) {
        *out_value = 0;
    }
    return 7;
}

long aic_rt_conc_close_channel(long handle) {
    (void)handle;
    return 7;
}

long aic_rt_conc_mutex_int(long initial, long* out_handle) {
    (void)initial;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_conc_mutex_lock(long handle, long timeout_ms, long* out_value) {
    (void)handle;
    (void)timeout_ms;
    if (out_value != NULL) {
        *out_value = 0;
    }
    return 7;
}

long aic_rt_conc_mutex_unlock(long handle, long value) {
    (void)handle;
    (void)value;
    return 7;
}

long aic_rt_conc_mutex_close(long handle) {
    (void)handle;
    return 7;
}
#else
#define AIC_RT_CONC_TASK_CAP 128
#define AIC_RT_CONC_CHANNEL_CAP 128
#define AIC_RT_CONC_MUTEX_CAP 128

typedef struct {
    int active;
    int finished;
    int cancelled;
    int panic;
    long input_value;
    long delay_ms;
    long result;
    pthread_t thread;
    pthread_mutex_t mutex;
    pthread_cond_t cond;
} AicConcTaskSlot;

typedef struct {
    int active;
    int closed;
    long* values;
    long cap;
    long len;
    long head;
    long tail;
    pthread_mutex_t mutex;
    pthread_cond_t not_empty;
    pthread_cond_t not_full;
} AicConcChannelSlot;

typedef struct {
    int active;
    int closed;
    int locked;
    long value;
    pthread_mutex_t mutex;
    pthread_cond_t cond;
} AicConcMutexSlot;

static AicConcTaskSlot aic_rt_conc_tasks[AIC_RT_CONC_TASK_CAP];
static AicConcChannelSlot aic_rt_conc_channels[AIC_RT_CONC_CHANNEL_CAP];
static AicConcMutexSlot aic_rt_conc_mutexes[AIC_RT_CONC_MUTEX_CAP];

static long aic_rt_conc_map_errno(int err) {
    switch (err) {
#ifdef ETIMEDOUT
        case ETIMEDOUT:
            return 2;  // Timeout
#endif
#ifdef ECANCELED
        case ECANCELED:
            return 3;  // Cancelled
#endif
        case EINVAL:
            return 4;  // InvalidInput
        default:
            return 7;  // Io
    }
}

static int aic_rt_conc_make_deadline(long timeout_ms, struct timespec* out_deadline) {
    if (timeout_ms < 0 || out_deadline == NULL) {
        return EINVAL;
    }
    if (clock_gettime(CLOCK_REALTIME, out_deadline) != 0) {
        return errno;
    }
    out_deadline->tv_sec += (time_t)(timeout_ms / 1000);
    out_deadline->tv_nsec += (long)((timeout_ms % 1000) * 1000000L);
    if (out_deadline->tv_nsec >= 1000000000L) {
        out_deadline->tv_sec += out_deadline->tv_nsec / 1000000000L;
        out_deadline->tv_nsec = out_deadline->tv_nsec % 1000000000L;
    }
    return 0;
}

static AicConcTaskSlot* aic_rt_conc_get_task(long handle) {
    if (handle <= 0 || handle > AIC_RT_CONC_TASK_CAP) {
        return NULL;
    }
    AicConcTaskSlot* slot = &aic_rt_conc_tasks[handle - 1];
    if (!slot->active) {
        return NULL;
    }
    return slot;
}

static AicConcChannelSlot* aic_rt_conc_get_channel(long handle) {
    if (handle <= 0 || handle > AIC_RT_CONC_CHANNEL_CAP) {
        return NULL;
    }
    AicConcChannelSlot* slot = &aic_rt_conc_channels[handle - 1];
    if (!slot->active) {
        return NULL;
    }
    return slot;
}

static AicConcMutexSlot* aic_rt_conc_get_mutex(long handle) {
    if (handle <= 0 || handle > AIC_RT_CONC_MUTEX_CAP) {
        return NULL;
    }
    AicConcMutexSlot* slot = &aic_rt_conc_mutexes[handle - 1];
    if (!slot->active) {
        return NULL;
    }
    return slot;
}

static void* aic_rt_conc_task_main(void* raw_slot) {
    long slot_index = -1;
    if (raw_slot != NULL) {
        slot_index = *(long*)raw_slot;
    }
    free(raw_slot);
    if (slot_index < 0 || slot_index >= AIC_RT_CONC_TASK_CAP) {
        return NULL;
    }
    AicConcTaskSlot* slot = &aic_rt_conc_tasks[slot_index];

    long remaining = slot->delay_ms;
    while (remaining > 0) {
        long step = remaining > 10 ? 10 : remaining;
        aic_rt_time_sleep_ms(step);
        remaining -= step;

        pthread_mutex_lock(&slot->mutex);
        int cancelled = slot->cancelled;
        pthread_mutex_unlock(&slot->mutex);
        if (cancelled) {
            pthread_mutex_lock(&slot->mutex);
            slot->finished = 1;
            pthread_cond_broadcast(&slot->cond);
            pthread_mutex_unlock(&slot->mutex);
            return NULL;
        }
    }

    pthread_mutex_lock(&slot->mutex);
    if (slot->cancelled) {
        slot->finished = 1;
        pthread_cond_broadcast(&slot->cond);
        pthread_mutex_unlock(&slot->mutex);
        return NULL;
    }
    if (slot->input_value < 0) {
        slot->panic = 1;
    } else {
        slot->result = slot->input_value * 2;
    }
    slot->finished = 1;
    pthread_cond_broadcast(&slot->cond);
    pthread_mutex_unlock(&slot->mutex);
    return NULL;
}

long aic_rt_conc_spawn(long value, long delay_ms, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (delay_ms < 0) {
        return 4;
    }
    long slot_index = -1;
    for (long i = 0; i < AIC_RT_CONC_TASK_CAP; ++i) {
        if (!aic_rt_conc_tasks[i].active) {
            slot_index = i;
            break;
        }
    }
    if (slot_index < 0) {
        return 7;
    }

    AicConcTaskSlot* slot = &aic_rt_conc_tasks[slot_index];
    memset(slot, 0, sizeof(*slot));
    slot->active = 1;
    slot->input_value = value;
    slot->delay_ms = delay_ms;
    if (pthread_mutex_init(&slot->mutex, NULL) != 0) {
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_cond_init(&slot->cond, NULL) != 0) {
        pthread_mutex_destroy(&slot->mutex);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }

    long* arg = (long*)malloc(sizeof(long));
    if (arg == NULL) {
        pthread_cond_destroy(&slot->cond);
        pthread_mutex_destroy(&slot->mutex);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    *arg = slot_index;
    int create_rc = pthread_create(&slot->thread, NULL, aic_rt_conc_task_main, arg);
    if (create_rc != 0) {
        free(arg);
        pthread_cond_destroy(&slot->cond);
        pthread_mutex_destroy(&slot->mutex);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (out_handle != NULL) {
        *out_handle = slot_index + 1;
    }
    return 0;
}

long aic_rt_conc_join(long handle, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    AicConcTaskSlot* slot = aic_rt_conc_get_task(handle);
    if (slot == NULL) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    while (!slot->finished) {
        int wait_rc = pthread_cond_wait(&slot->cond, &slot->mutex);
        if (wait_rc != 0) {
            pthread_mutex_unlock(&slot->mutex);
            return aic_rt_conc_map_errno(wait_rc);
        }
    }

    int cancelled = slot->cancelled;
    int panic = slot->panic;
    long result = slot->result;
    pthread_mutex_unlock(&slot->mutex);

    int join_rc = pthread_join(slot->thread, NULL);
    if (join_rc != 0) {
        return 7;
    }
    pthread_cond_destroy(&slot->cond);
    pthread_mutex_destroy(&slot->mutex);
    memset(slot, 0, sizeof(*slot));

    if (cancelled) {
        return 3;
    }
    if (panic) {
        return 5;
    }
    if (out_value != NULL) {
        *out_value = result;
    }
    return 0;
}

long aic_rt_conc_cancel(long handle, long* out_cancelled) {
    if (out_cancelled != NULL) {
        *out_cancelled = 0;
    }
    AicConcTaskSlot* slot = aic_rt_conc_get_task(handle);
    if (slot == NULL) {
        return 1;
    }
    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    if (!slot->finished) {
        slot->cancelled = 1;
        if (out_cancelled != NULL) {
            *out_cancelled = 1;
        }
    }
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}

long aic_rt_conc_channel_int(long capacity, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (capacity <= 0 || capacity > 1048576) {
        return 4;
    }

    long slot_index = -1;
    for (long i = 0; i < AIC_RT_CONC_CHANNEL_CAP; ++i) {
        if (!aic_rt_conc_channels[i].active) {
            slot_index = i;
            break;
        }
    }
    if (slot_index < 0) {
        return 7;
    }

    AicConcChannelSlot* slot = &aic_rt_conc_channels[slot_index];
    memset(slot, 0, sizeof(*slot));
    slot->values = (long*)malloc((size_t)capacity * sizeof(long));
    if (slot->values == NULL) {
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_mutex_init(&slot->mutex, NULL) != 0) {
        free(slot->values);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_cond_init(&slot->not_empty, NULL) != 0) {
        pthread_mutex_destroy(&slot->mutex);
        free(slot->values);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_cond_init(&slot->not_full, NULL) != 0) {
        pthread_cond_destroy(&slot->not_empty);
        pthread_mutex_destroy(&slot->mutex);
        free(slot->values);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    slot->active = 1;
    slot->cap = capacity;
    slot->len = 0;
    slot->head = 0;
    slot->tail = 0;
    slot->closed = 0;

    if (out_handle != NULL) {
        *out_handle = slot_index + 1;
    }
    return 0;
}

long aic_rt_conc_send_int(long handle, long value, long timeout_ms) {
    if (timeout_ms < 0) {
        return 4;
    }
    AicConcChannelSlot* slot = aic_rt_conc_get_channel(handle);
    if (slot == NULL) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    struct timespec deadline;
    int deadline_rc = aic_rt_conc_make_deadline(timeout_ms, &deadline);
    if (deadline_rc != 0) {
        pthread_mutex_unlock(&slot->mutex);
        return aic_rt_conc_map_errno(deadline_rc);
    }

    while (slot->len >= slot->cap) {
        if (slot->closed) {
            pthread_mutex_unlock(&slot->mutex);
            return 6;
        }
        int wait_rc = pthread_cond_timedwait(&slot->not_full, &slot->mutex, &deadline);
#ifdef ETIMEDOUT
        if (wait_rc == ETIMEDOUT) {
            pthread_mutex_unlock(&slot->mutex);
            return 2;
        }
#endif
        if (wait_rc != 0) {
            pthread_mutex_unlock(&slot->mutex);
            return aic_rt_conc_map_errno(wait_rc);
        }
    }
    if (slot->closed) {
        pthread_mutex_unlock(&slot->mutex);
        return 6;
    }

    slot->values[slot->tail] = value;
    slot->tail = (slot->tail + 1) % slot->cap;
    slot->len += 1;
    pthread_cond_signal(&slot->not_empty);
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}

long aic_rt_conc_recv_int(long handle, long timeout_ms, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (timeout_ms < 0) {
        return 4;
    }
    AicConcChannelSlot* slot = aic_rt_conc_get_channel(handle);
    if (slot == NULL) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    struct timespec deadline;
    int deadline_rc = aic_rt_conc_make_deadline(timeout_ms, &deadline);
    if (deadline_rc != 0) {
        pthread_mutex_unlock(&slot->mutex);
        return aic_rt_conc_map_errno(deadline_rc);
    }

    while (slot->len == 0) {
        if (slot->closed) {
            pthread_mutex_unlock(&slot->mutex);
            return 6;
        }
        int wait_rc = pthread_cond_timedwait(&slot->not_empty, &slot->mutex, &deadline);
#ifdef ETIMEDOUT
        if (wait_rc == ETIMEDOUT) {
            pthread_mutex_unlock(&slot->mutex);
            return 2;
        }
#endif
        if (wait_rc != 0) {
            pthread_mutex_unlock(&slot->mutex);
            return aic_rt_conc_map_errno(wait_rc);
        }
    }

    long value = slot->values[slot->head];
    slot->head = (slot->head + 1) % slot->cap;
    slot->len -= 1;
    pthread_cond_signal(&slot->not_full);
    pthread_mutex_unlock(&slot->mutex);
    if (out_value != NULL) {
        *out_value = value;
    }
    return 0;
}

long aic_rt_conc_close_channel(long handle) {
    AicConcChannelSlot* slot = aic_rt_conc_get_channel(handle);
    if (slot == NULL) {
        return 1;
    }
    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    slot->closed = 1;
    pthread_cond_broadcast(&slot->not_empty);
    pthread_cond_broadcast(&slot->not_full);
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}

long aic_rt_conc_mutex_int(long initial, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }

    long slot_index = -1;
    for (long i = 0; i < AIC_RT_CONC_MUTEX_CAP; ++i) {
        if (!aic_rt_conc_mutexes[i].active) {
            slot_index = i;
            break;
        }
    }
    if (slot_index < 0) {
        return 7;
    }

    AicConcMutexSlot* slot = &aic_rt_conc_mutexes[slot_index];
    memset(slot, 0, sizeof(*slot));
    if (pthread_mutex_init(&slot->mutex, NULL) != 0) {
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    if (pthread_cond_init(&slot->cond, NULL) != 0) {
        pthread_mutex_destroy(&slot->mutex);
        memset(slot, 0, sizeof(*slot));
        return 7;
    }
    slot->active = 1;
    slot->closed = 0;
    slot->locked = 0;
    slot->value = initial;

    if (out_handle != NULL) {
        *out_handle = slot_index + 1;
    }
    return 0;
}

long aic_rt_conc_mutex_lock(long handle, long timeout_ms, long* out_value) {
    if (out_value != NULL) {
        *out_value = 0;
    }
    if (timeout_ms < 0) {
        return 4;
    }
    AicConcMutexSlot* slot = aic_rt_conc_get_mutex(handle);
    if (slot == NULL) {
        return 1;
    }

    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    struct timespec deadline;
    int deadline_rc = aic_rt_conc_make_deadline(timeout_ms, &deadline);
    if (deadline_rc != 0) {
        pthread_mutex_unlock(&slot->mutex);
        return aic_rt_conc_map_errno(deadline_rc);
    }

    while (slot->locked && !slot->closed) {
        int wait_rc = pthread_cond_timedwait(&slot->cond, &slot->mutex, &deadline);
#ifdef ETIMEDOUT
        if (wait_rc == ETIMEDOUT) {
            pthread_mutex_unlock(&slot->mutex);
            return 2;
        }
#endif
        if (wait_rc != 0) {
            pthread_mutex_unlock(&slot->mutex);
            return aic_rt_conc_map_errno(wait_rc);
        }
    }
    if (slot->closed) {
        pthread_mutex_unlock(&slot->mutex);
        return 6;
    }
    slot->locked = 1;
    if (out_value != NULL) {
        *out_value = slot->value;
    }
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}

long aic_rt_conc_mutex_unlock(long handle, long value) {
    AicConcMutexSlot* slot = aic_rt_conc_get_mutex(handle);
    if (slot == NULL) {
        return 1;
    }
    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    if (slot->closed) {
        pthread_mutex_unlock(&slot->mutex);
        return 6;
    }
    if (!slot->locked) {
        pthread_mutex_unlock(&slot->mutex);
        return 4;
    }
    slot->value = value;
    slot->locked = 0;
    pthread_cond_signal(&slot->cond);
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}

long aic_rt_conc_mutex_close(long handle) {
    AicConcMutexSlot* slot = aic_rt_conc_get_mutex(handle);
    if (slot == NULL) {
        return 1;
    }
    int lock_rc = pthread_mutex_lock(&slot->mutex);
    if (lock_rc != 0) {
        return aic_rt_conc_map_errno(lock_rc);
    }
    slot->closed = 1;
    slot->locked = 0;
    pthread_cond_broadcast(&slot->cond);
    pthread_mutex_unlock(&slot->mutex);
    return 0;
}
#endif

#ifdef _WIN32
long aic_rt_net_tcp_listen(const char* addr_ptr, long addr_len, long addr_cap, long* out_handle) {
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_net_tcp_local_addr(long handle, char** out_ptr, long* out_len) {
    (void)handle;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_tcp_accept(long listener, long timeout_ms, long* out_handle) {
    (void)listener;
    (void)timeout_ms;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_net_tcp_connect(
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    long timeout_ms,
    long* out_handle
) {
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    (void)timeout_ms;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_net_tcp_send(
    long handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
) {
    (void)handle;
    (void)payload_ptr;
    (void)payload_len;
    (void)payload_cap;
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    return 7;
}

long aic_rt_net_tcp_recv(
    long handle,
    long max_bytes,
    long timeout_ms,
    char** out_ptr,
    long* out_len
) {
    (void)handle;
    (void)max_bytes;
    (void)timeout_ms;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_tcp_close(long handle) {
    (void)handle;
    return 7;
}

long aic_rt_net_udp_bind(const char* addr_ptr, long addr_len, long addr_cap, long* out_handle) {
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    return 7;
}

long aic_rt_net_udp_local_addr(long handle, char** out_ptr, long* out_len) {
    (void)handle;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_udp_send_to(
    long handle,
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
) {
    (void)handle;
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    (void)payload_ptr;
    (void)payload_len;
    (void)payload_cap;
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    return 7;
}

long aic_rt_net_udp_recv_from(
    long handle,
    long max_bytes,
    long timeout_ms,
    char** out_from_ptr,
    long* out_from_len,
    char** out_payload_ptr,
    long* out_payload_len
) {
    (void)handle;
    (void)max_bytes;
    (void)timeout_ms;
    if (out_from_ptr != NULL) {
        *out_from_ptr = NULL;
    }
    if (out_from_len != NULL) {
        *out_from_len = 0;
    }
    if (out_payload_ptr != NULL) {
        *out_payload_ptr = NULL;
    }
    if (out_payload_len != NULL) {
        *out_payload_len = 0;
    }
    return 7;
}

long aic_rt_net_udp_close(long handle) {
    (void)handle;
    return 7;
}

long aic_rt_net_dns_lookup(
    const char* host_ptr,
    long host_len,
    long host_cap,
    char** out_ptr,
    long* out_len
) {
    (void)host_ptr;
    (void)host_len;
    (void)host_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}

long aic_rt_net_dns_reverse(
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    char** out_ptr,
    long* out_len
) {
    (void)addr_ptr;
    (void)addr_len;
    (void)addr_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 7;
}
#else
static long aic_rt_net_map_errno(int err) {
    switch (err) {
        case ENOENT:
            return 1;  // NotFound
        case EACCES:
        case EPERM:
            return 2;  // PermissionDenied
#ifdef ECONNREFUSED
        case ECONNREFUSED:
            return 3;  // Refused
#endif
#ifdef ETIMEDOUT
        case ETIMEDOUT:
            return 4;  // Timeout
#endif
#ifdef EAGAIN
        case EAGAIN:
            return 4;  // Timeout
#endif
#ifdef EWOULDBLOCK
#if !defined(EAGAIN) || EWOULDBLOCK != EAGAIN
        case EWOULDBLOCK:
            return 4;  // Timeout
#endif
#endif
#ifdef EADDRINUSE
        case EADDRINUSE:
            return 5;  // AddressInUse
#endif
        case EINVAL:
#ifdef ENAMETOOLONG
        case ENAMETOOLONG:
#endif
#ifdef EAFNOSUPPORT
        case EAFNOSUPPORT:
#endif
#ifdef ENOTSOCK
        case ENOTSOCK:
#endif
#ifdef EDESTADDRREQ
        case EDESTADDRREQ:
#endif
#ifdef EPROTOTYPE
        case EPROTOTYPE:
#endif
            return 6;  // InvalidInput
        default:
            return 7;  // Io
    }
}

static long aic_rt_net_map_gai_error(int err) {
    switch (err) {
#ifdef EAI_NONAME
        case EAI_NONAME:
            return 1;  // NotFound
#endif
#ifdef EAI_NODATA
        case EAI_NODATA:
            return 1;  // NotFound
#endif
#ifdef EAI_AGAIN
        case EAI_AGAIN:
            return 4;  // Timeout
#endif
#ifdef EAI_BADFLAGS
        case EAI_BADFLAGS:
            return 6;  // InvalidInput
#endif
#ifdef EAI_FAMILY
        case EAI_FAMILY:
            return 6;  // InvalidInput
#endif
#ifdef EAI_SOCKTYPE
        case EAI_SOCKTYPE:
            return 6;  // InvalidInput
#endif
#ifdef EAI_SERVICE
        case EAI_SERVICE:
            return 6;  // InvalidInput
#endif
#ifdef EAI_SYSTEM
        case EAI_SYSTEM:
            return aic_rt_net_map_errno(errno);
#endif
        default:
            return 7;  // Io
    }
}

#define AIC_RT_NET_TABLE_CAP 128
#define AIC_RT_NET_KIND_TCP_LISTENER 1
#define AIC_RT_NET_KIND_TCP_STREAM 2
#define AIC_RT_NET_KIND_UDP 3

typedef struct {
    int active;
    int fd;
    int kind;
} AicNetSlot;

static AicNetSlot aic_rt_net_table[AIC_RT_NET_TABLE_CAP];

static void aic_rt_net_reset_slot(AicNetSlot* slot) {
    if (slot == NULL) {
        return;
    }
    slot->active = 0;
    slot->fd = -1;
    slot->kind = 0;
}

static long aic_rt_net_close_fd(int fd) {
    if (close(fd) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
}

static AicNetSlot* aic_rt_net_get_slot(long handle) {
    if (handle <= 0 || handle > AIC_RT_NET_TABLE_CAP) {
        return NULL;
    }
    AicNetSlot* slot = &aic_rt_net_table[handle - 1];
    if (!slot->active) {
        return NULL;
    }
    return slot;
}

static long aic_rt_net_alloc_handle(int fd, int kind, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    for (long i = 0; i < AIC_RT_NET_TABLE_CAP; ++i) {
        if (!aic_rt_net_table[i].active) {
            aic_rt_net_table[i].active = 1;
            aic_rt_net_table[i].fd = fd;
            aic_rt_net_table[i].kind = kind;
            if (out_handle != NULL) {
                *out_handle = i + 1;
            }
            return 0;
        }
    }
    aic_rt_net_close_fd(fd);
    return 7;
}

static long aic_rt_net_wait_fd(int fd, int want_read, long timeout_ms) {
    if (timeout_ms < 0) {
        return 6;
    }

    fd_set read_set;
    fd_set write_set;
    FD_ZERO(&read_set);
    FD_ZERO(&write_set);
    if (want_read) {
        FD_SET(fd, &read_set);
    } else {
        FD_SET(fd, &write_set);
    }

    struct timeval tv;
    tv.tv_sec = (time_t)(timeout_ms / 1000);
    tv.tv_usec = (suseconds_t)((timeout_ms % 1000) * 1000);

    int rc = select(fd + 1, want_read ? &read_set : NULL, want_read ? NULL : &write_set, NULL, &tv);
    if (rc == 0) {
        return 4;
    }
    if (rc < 0) {
        return aic_rt_net_map_errno(errno);
    }
    return 0;
}

static long aic_rt_net_split_host_port(const char* addr, char** out_host, char** out_port) {
    if (out_host != NULL) {
        *out_host = NULL;
    }
    if (out_port != NULL) {
        *out_port = NULL;
    }
    if (addr == NULL || addr[0] == '\0' || out_host == NULL || out_port == NULL) {
        return 6;
    }

    const char* host_ptr = addr;
    size_t host_len = 0;
    const char* port_ptr = NULL;
    if (addr[0] == '[') {
        const char* close = strchr(addr, ']');
        if (close == NULL || close[1] != ':') {
            return 6;
        }
        host_ptr = addr + 1;
        host_len = (size_t)(close - host_ptr);
        port_ptr = close + 2;
    } else {
        const char* first_colon = strchr(addr, ':');
        const char* last_colon = strrchr(addr, ':');
        if (last_colon == NULL) {
            return 6;
        }
        if (first_colon != last_colon) {
            return 6;
        }
        host_ptr = addr;
        host_len = (size_t)(last_colon - addr);
        port_ptr = last_colon + 1;
    }

    if (port_ptr == NULL || port_ptr[0] == '\0') {
        return 6;
    }

    char* host = aic_rt_copy_bytes(host_ptr, host_len);
    if (host == NULL) {
        return 7;
    }
    char* port = aic_rt_copy_bytes(port_ptr, strlen(port_ptr));
    if (port == NULL) {
        free(host);
        return 7;
    }
    *out_host = host;
    *out_port = port;
    return 0;
}

static long aic_rt_net_resolve(
    const char* host,
    const char* port,
    int socktype,
    int flags,
    int allow_wildcard,
    struct addrinfo** out
) {
    if (out == NULL) {
        return 6;
    }
    *out = NULL;
    if (port == NULL || port[0] == '\0') {
        return 6;
    }
    if (!allow_wildcard && (host == NULL || host[0] == '\0')) {
        return 6;
    }
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = socktype;
    hints.ai_flags = flags;
    const char* host_arg = (host != NULL && host[0] != '\0') ? host : NULL;
    int rc = getaddrinfo(host_arg, port, &hints, out);
    if (rc != 0) {
        return aic_rt_net_map_gai_error(rc);
    }
    if (*out == NULL) {
        return 1;
    }
    return 0;
}

static char* aic_rt_net_format_sockaddr(const struct sockaddr* addr, socklen_t addr_len) {
    if (addr == NULL) {
        return NULL;
    }
    char host[NI_MAXHOST];
    char serv[NI_MAXSERV];
    int rc = getnameinfo(
        addr,
        addr_len,
        host,
        sizeof(host),
        serv,
        sizeof(serv),
        NI_NUMERICHOST | NI_NUMERICSERV
    );
    if (rc != 0) {
        return NULL;
    }
    size_t host_n = strlen(host);
    size_t serv_n = strlen(serv);
    int need_brackets = strchr(host, ':') != NULL;
    size_t out_n = host_n + serv_n + (need_brackets ? 3 : 1);
    char* out = (char*)malloc(out_n + 1);
    if (out == NULL) {
        return NULL;
    }
    if (need_brackets) {
        snprintf(out, out_n + 1, "[%s]:%s", host, serv);
    } else {
        snprintf(out, out_n + 1, "%s:%s", host, serv);
    }
    return out;
}

long aic_rt_net_tcp_listen(const char* addr_ptr, long addr_len, long addr_cap, long* out_handle) {
    (void)addr_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    char* addr = aic_rt_fs_copy_slice(addr_ptr, addr_len);
    if (addr == NULL) {
        return 6;
    }

    char* host = NULL;
    char* port = NULL;
    long split = aic_rt_net_split_host_port(addr, &host, &port);
    free(addr);
    if (split != 0) {
        free(host);
        free(port);
        return split;
    }

    struct addrinfo* infos = NULL;
    long resolved = aic_rt_net_resolve(host, port, SOCK_STREAM, AI_PASSIVE, 1, &infos);
    free(host);
    free(port);
    if (resolved != 0) {
        return resolved;
    }

    long result = 7;
    for (struct addrinfo* ai = infos; ai != NULL; ai = ai->ai_next) {
        int fd = (int)socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
        if (fd < 0) {
            result = aic_rt_net_map_errno(errno);
            continue;
        }
        int one = 1;
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, sizeof(one));
        if (bind(fd, ai->ai_addr, (socklen_t)ai->ai_addrlen) != 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }
        if (listen(fd, 128) != 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }
        result = aic_rt_net_alloc_handle(fd, AIC_RT_NET_KIND_TCP_LISTENER, out_handle);
        if (result == 0) {
            break;
        }
    }
    freeaddrinfo(infos);
    return result;
}

long aic_rt_net_tcp_local_addr(long handle, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }

    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || (slot->kind != AIC_RT_NET_KIND_TCP_LISTENER && slot->kind != AIC_RT_NET_KIND_TCP_STREAM)) {
        return 6;
    }

    struct sockaddr_storage addr;
    socklen_t addr_len = (socklen_t)sizeof(addr);
    if (getsockname(slot->fd, (struct sockaddr*)&addr, &addr_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    char* text = aic_rt_net_format_sockaddr((struct sockaddr*)&addr, addr_len);
    if (text == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = text;
    } else {
        free(text);
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(text);
    }
    return 0;
}

long aic_rt_net_tcp_accept(long listener, long timeout_ms, long* out_handle) {
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(listener);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_LISTENER) {
        return 6;
    }
    long waited = aic_rt_net_wait_fd(slot->fd, 1, timeout_ms);
    if (waited != 0) {
        return waited;
    }
    struct sockaddr_storage peer;
    socklen_t peer_len = (socklen_t)sizeof(peer);
    int client_fd = (int)accept(slot->fd, (struct sockaddr*)&peer, &peer_len);
    if (client_fd < 0) {
        return aic_rt_net_map_errno(errno);
    }
    return aic_rt_net_alloc_handle(client_fd, AIC_RT_NET_KIND_TCP_STREAM, out_handle);
}

long aic_rt_net_tcp_connect(
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    long timeout_ms,
    long* out_handle
) {
    (void)addr_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    if (timeout_ms < 0) {
        return 6;
    }
    char* addr = aic_rt_fs_copy_slice(addr_ptr, addr_len);
    if (addr == NULL) {
        return 6;
    }
    char* host = NULL;
    char* port = NULL;
    long split = aic_rt_net_split_host_port(addr, &host, &port);
    free(addr);
    if (split != 0) {
        free(host);
        free(port);
        return split;
    }

    struct addrinfo* infos = NULL;
    long resolved = aic_rt_net_resolve(host, port, SOCK_STREAM, 0, 0, &infos);
    free(host);
    free(port);
    if (resolved != 0) {
        return resolved;
    }

    long result = 7;
    for (struct addrinfo* ai = infos; ai != NULL; ai = ai->ai_next) {
        int fd = (int)socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
        if (fd < 0) {
            result = aic_rt_net_map_errno(errno);
            continue;
        }

        int prev_flags = fcntl(fd, F_GETFL, 0);
        if (prev_flags < 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }
        if (fcntl(fd, F_SETFL, prev_flags | O_NONBLOCK) != 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }

        int rc = connect(fd, ai->ai_addr, (socklen_t)ai->ai_addrlen);
        if (rc != 0) {
            int err = errno;
            int in_progress = 0;
#ifdef EINPROGRESS
            if (err == EINPROGRESS) {
                in_progress = 1;
            }
#endif
#ifdef EWOULDBLOCK
            if (err == EWOULDBLOCK) {
                in_progress = 1;
            }
#endif
            if (in_progress) {
                long waited = aic_rt_net_wait_fd(fd, 0, timeout_ms);
                if (waited != 0) {
                    result = waited;
                    aic_rt_net_close_fd(fd);
                    continue;
                }
                int so_err = 0;
                socklen_t so_len = (socklen_t)sizeof(so_err);
                if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &so_err, &so_len) != 0) {
                    result = aic_rt_net_map_errno(errno);
                    aic_rt_net_close_fd(fd);
                    continue;
                }
                if (so_err != 0) {
                    result = aic_rt_net_map_errno(so_err);
                    aic_rt_net_close_fd(fd);
                    continue;
                }
            } else {
                result = aic_rt_net_map_errno(err);
                aic_rt_net_close_fd(fd);
                continue;
            }
        }

        if (fcntl(fd, F_SETFL, prev_flags) != 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }

        result = aic_rt_net_alloc_handle(fd, AIC_RT_NET_KIND_TCP_STREAM, out_handle);
        if (result == 0) {
            break;
        }
    }
    freeaddrinfo(infos);
    return result;
}

long aic_rt_net_tcp_send(
    long handle,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
) {
    (void)payload_cap;
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    if (payload_len < 0 || (payload_len > 0 && payload_ptr == NULL)) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
    size_t remaining = (size_t)payload_len;
    const char* cursor = payload_ptr;
    size_t total = 0;
    while (remaining > 0) {
#ifdef MSG_NOSIGNAL
        int flags = MSG_NOSIGNAL;
#else
        int flags = 0;
#endif
        ssize_t n = send(slot->fd, cursor, remaining, flags);
        if (n < 0) {
            if (errno == EINTR) {
                continue;
            }
            return aic_rt_net_map_errno(errno);
        }
        if (n == 0) {
            break;
        }
        cursor += (size_t)n;
        remaining -= (size_t)n;
        total += (size_t)n;
    }
    if (out_sent != NULL) {
        *out_sent = (long)total;
    }
    return 0;
}

long aic_rt_net_tcp_recv(
    long handle,
    long max_bytes,
    long timeout_ms,
    char** out_ptr,
    long* out_len
) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (max_bytes < 0 || timeout_ms < 0) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_TCP_STREAM) {
        return 6;
    }
    long waited = aic_rt_net_wait_fd(slot->fd, 1, timeout_ms);
    if (waited != 0) {
        return waited;
    }
    size_t cap = (size_t)max_bytes;
    char* buffer = (char*)malloc(cap + 1);
    if (buffer == NULL) {
        return 7;
    }
    ssize_t n = recv(slot->fd, buffer, cap, 0);
    if (n < 0) {
        int err = errno;
        free(buffer);
        return aic_rt_net_map_errno(err);
    }
    buffer[(size_t)n] = '\0';
    if (out_ptr != NULL) {
        *out_ptr = buffer;
    } else {
        free(buffer);
    }
    if (out_len != NULL) {
        *out_len = (long)n;
    }
    return 0;
}

long aic_rt_net_tcp_close(long handle) {
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || (slot->kind != AIC_RT_NET_KIND_TCP_LISTENER && slot->kind != AIC_RT_NET_KIND_TCP_STREAM)) {
        return 6;
    }
    int fd = slot->fd;
    aic_rt_net_reset_slot(slot);
    return aic_rt_net_close_fd(fd);
}

long aic_rt_net_udp_bind(const char* addr_ptr, long addr_len, long addr_cap, long* out_handle) {
    (void)addr_cap;
    if (out_handle != NULL) {
        *out_handle = 0;
    }
    char* addr = aic_rt_fs_copy_slice(addr_ptr, addr_len);
    if (addr == NULL) {
        return 6;
    }

    char* host = NULL;
    char* port = NULL;
    long split = aic_rt_net_split_host_port(addr, &host, &port);
    free(addr);
    if (split != 0) {
        free(host);
        free(port);
        return split;
    }

    struct addrinfo* infos = NULL;
    long resolved = aic_rt_net_resolve(host, port, SOCK_DGRAM, AI_PASSIVE, 1, &infos);
    free(host);
    free(port);
    if (resolved != 0) {
        return resolved;
    }

    long result = 7;
    for (struct addrinfo* ai = infos; ai != NULL; ai = ai->ai_next) {
        int fd = (int)socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
        if (fd < 0) {
            result = aic_rt_net_map_errno(errno);
            continue;
        }
        if (bind(fd, ai->ai_addr, (socklen_t)ai->ai_addrlen) != 0) {
            result = aic_rt_net_map_errno(errno);
            aic_rt_net_close_fd(fd);
            continue;
        }
        result = aic_rt_net_alloc_handle(fd, AIC_RT_NET_KIND_UDP, out_handle);
        if (result == 0) {
            break;
        }
    }
    freeaddrinfo(infos);
    return result;
}

long aic_rt_net_udp_local_addr(long handle, char** out_ptr, long* out_len) {
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_UDP) {
        return 6;
    }
    struct sockaddr_storage addr;
    socklen_t addr_len = (socklen_t)sizeof(addr);
    if (getsockname(slot->fd, (struct sockaddr*)&addr, &addr_len) != 0) {
        return aic_rt_net_map_errno(errno);
    }
    char* text = aic_rt_net_format_sockaddr((struct sockaddr*)&addr, addr_len);
    if (text == NULL) {
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = text;
    } else {
        free(text);
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(text);
    }
    return 0;
}

long aic_rt_net_udp_send_to(
    long handle,
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    const char* payload_ptr,
    long payload_len,
    long payload_cap,
    long* out_sent
) {
    (void)addr_cap;
    (void)payload_cap;
    if (out_sent != NULL) {
        *out_sent = 0;
    }
    if (payload_len < 0 || (payload_len > 0 && payload_ptr == NULL)) {
        return 6;
    }
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_UDP) {
        return 6;
    }

    char* addr = aic_rt_fs_copy_slice(addr_ptr, addr_len);
    if (addr == NULL) {
        return 6;
    }
    char* host = NULL;
    char* port = NULL;
    long split = aic_rt_net_split_host_port(addr, &host, &port);
    free(addr);
    if (split != 0) {
        free(host);
        free(port);
        return split;
    }
    if (host[0] == '\0') {
        free(host);
        free(port);
        return 6;
    }

    struct addrinfo* infos = NULL;
    long resolved = aic_rt_net_resolve(host, port, SOCK_DGRAM, 0, 0, &infos);
    free(host);
    free(port);
    if (resolved != 0) {
        return resolved;
    }

    long result = 7;
    for (struct addrinfo* ai = infos; ai != NULL; ai = ai->ai_next) {
        ssize_t sent = sendto(
            slot->fd,
            payload_ptr,
            (size_t)payload_len,
            0,
            ai->ai_addr,
            (socklen_t)ai->ai_addrlen
        );
        if (sent >= 0) {
            if (out_sent != NULL) {
                *out_sent = (long)sent;
            }
            result = 0;
            break;
        }
        result = aic_rt_net_map_errno(errno);
    }
    freeaddrinfo(infos);
    return result;
}

long aic_rt_net_udp_recv_from(
    long handle,
    long max_bytes,
    long timeout_ms,
    char** out_from_ptr,
    long* out_from_len,
    char** out_payload_ptr,
    long* out_payload_len
) {
    if (out_from_ptr != NULL) {
        *out_from_ptr = NULL;
    }
    if (out_from_len != NULL) {
        *out_from_len = 0;
    }
    if (out_payload_ptr != NULL) {
        *out_payload_ptr = NULL;
    }
    if (out_payload_len != NULL) {
        *out_payload_len = 0;
    }
    if (max_bytes < 0 || timeout_ms < 0) {
        return 6;
    }

    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_UDP) {
        return 6;
    }

    long waited = aic_rt_net_wait_fd(slot->fd, 1, timeout_ms);
    if (waited != 0) {
        return waited;
    }

    size_t cap = (size_t)max_bytes;
    char* payload = (char*)malloc(cap + 1);
    if (payload == NULL) {
        return 7;
    }
    struct sockaddr_storage from;
    socklen_t from_len = (socklen_t)sizeof(from);
    ssize_t got = recvfrom(
        slot->fd,
        payload,
        cap,
        0,
        (struct sockaddr*)&from,
        &from_len
    );
    if (got < 0) {
        int err = errno;
        free(payload);
        return aic_rt_net_map_errno(err);
    }
    payload[(size_t)got] = '\0';

    char* from_text = aic_rt_net_format_sockaddr((struct sockaddr*)&from, from_len);
    if (from_text == NULL) {
        free(payload);
        return 7;
    }

    if (out_from_ptr != NULL) {
        *out_from_ptr = from_text;
    } else {
        free(from_text);
    }
    if (out_from_len != NULL) {
        *out_from_len = (long)strlen(from_text);
    }

    if (out_payload_ptr != NULL) {
        *out_payload_ptr = payload;
    } else {
        free(payload);
    }
    if (out_payload_len != NULL) {
        *out_payload_len = (long)got;
    }
    return 0;
}

long aic_rt_net_udp_close(long handle) {
    AicNetSlot* slot = aic_rt_net_get_slot(handle);
    if (slot == NULL || slot->kind != AIC_RT_NET_KIND_UDP) {
        return 6;
    }
    int fd = slot->fd;
    aic_rt_net_reset_slot(slot);
    return aic_rt_net_close_fd(fd);
}

long aic_rt_net_dns_lookup(
    const char* host_ptr,
    long host_len,
    long host_cap,
    char** out_ptr,
    long* out_len
) {
    (void)host_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* host = aic_rt_fs_copy_slice(host_ptr, host_len);
    if (host == NULL || host[0] == '\0') {
        free(host);
        return 6;
    }

    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    struct addrinfo* infos = NULL;
    int rc = getaddrinfo(host, NULL, &hints, &infos);
    free(host);
    if (rc != 0) {
        return aic_rt_net_map_gai_error(rc);
    }

    long result = 1;
    int last_name_rc = 0;
    for (struct addrinfo* ai = infos; ai != NULL; ai = ai->ai_next) {
        char numeric[NI_MAXHOST];
        int name_rc = getnameinfo(
            ai->ai_addr,
            (socklen_t)ai->ai_addrlen,
            numeric,
            sizeof(numeric),
            NULL,
            0,
            NI_NUMERICHOST
        );
        if (name_rc == 0) {
            char* out = aic_rt_copy_bytes(numeric, strlen(numeric));
            if (out == NULL) {
                result = 7;
            } else {
                if (out_ptr != NULL) {
                    *out_ptr = out;
                } else {
                    free(out);
                }
                if (out_len != NULL) {
                    *out_len = (long)strlen(numeric);
                }
                result = 0;
            }
            break;
        }
        last_name_rc = name_rc;
    }
    freeaddrinfo(infos);
    if (result != 0 && last_name_rc != 0) {
        return aic_rt_net_map_gai_error(last_name_rc);
    }
    return result;
}

long aic_rt_net_dns_reverse(
    const char* addr_ptr,
    long addr_len,
    long addr_cap,
    char** out_ptr,
    long* out_len
) {
    (void)addr_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    char* addr = aic_rt_fs_copy_slice(addr_ptr, addr_len);
    if (addr == NULL || addr[0] == '\0') {
        free(addr);
        return 6;
    }

    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_NUMERICHOST;
    struct addrinfo* infos = NULL;
    int rc = getaddrinfo(addr, NULL, &hints, &infos);
    free(addr);
    if (rc != 0) {
        return aic_rt_net_map_gai_error(rc);
    }
    if (infos == NULL) {
        return 1;
    }

    char name[NI_MAXHOST];
    int flags = 0;
#ifdef NI_NAMEREQD
    flags |= NI_NAMEREQD;
#endif
    int name_rc = getnameinfo(
        infos->ai_addr,
        (socklen_t)infos->ai_addrlen,
        name,
        sizeof(name),
        NULL,
        0,
        flags
    );
    if (name_rc != 0) {
        freeaddrinfo(infos);
        return aic_rt_net_map_gai_error(name_rc);
    }
    char* out = aic_rt_copy_bytes(name, strlen(name));
    if (out == NULL) {
        freeaddrinfo(infos);
        return 7;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)strlen(name);
    }
    freeaddrinfo(infos);
    return 0;
}
#endif

#define AIC_RT_REGEX_FLAG_CASE_INSENSITIVE 1L
#define AIC_RT_REGEX_FLAG_MULTILINE 2L
#define AIC_RT_REGEX_FLAG_DOT_MATCHES_NEWLINE 4L
#define AIC_RT_REGEX_SUPPORTED_FLAGS \
    (AIC_RT_REGEX_FLAG_CASE_INSENSITIVE | AIC_RT_REGEX_FLAG_MULTILINE | AIC_RT_REGEX_FLAG_DOT_MATCHES_NEWLINE)

static long aic_rt_regex_validate_flags(long flags) {
    if (flags < 0) {
        return 2;  // InvalidInput
    }
    if ((flags & ~AIC_RT_REGEX_SUPPORTED_FLAGS) != 0) {
        return 2;  // InvalidInput
    }
    if ((flags & AIC_RT_REGEX_FLAG_MULTILINE) != 0 &&
        (flags & AIC_RT_REGEX_FLAG_DOT_MATCHES_NEWLINE) != 0) {
        return 4;  // UnsupportedFeature
    }
    return 0;
}

#ifdef _WIN32
long aic_rt_regex_compile(const char* pattern_ptr, long pattern_len, long pattern_cap, long flags) {
    (void)pattern_ptr;
    (void)pattern_len;
    (void)pattern_cap;
    (void)flags;
    return 4;
}

long aic_rt_regex_is_match(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    long* out_is_match
) {
    (void)pattern_ptr;
    (void)pattern_len;
    (void)pattern_cap;
    (void)flags;
    (void)text_ptr;
    (void)text_len;
    (void)text_cap;
    if (out_is_match != NULL) {
        *out_is_match = 0;
    }
    return 4;
}

long aic_rt_regex_find(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_ptr,
    long* out_len
) {
    (void)pattern_ptr;
    (void)pattern_len;
    (void)pattern_cap;
    (void)flags;
    (void)text_ptr;
    (void)text_len;
    (void)text_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 4;
}

long aic_rt_regex_replace(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    const char* replacement_ptr,
    long replacement_len,
    long replacement_cap,
    char** out_ptr,
    long* out_len
) {
    (void)pattern_ptr;
    (void)pattern_len;
    (void)pattern_cap;
    (void)flags;
    (void)text_ptr;
    (void)text_len;
    (void)text_cap;
    (void)replacement_ptr;
    (void)replacement_len;
    (void)replacement_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    return 4;
}
#else
static long aic_rt_regex_map_compile_error(int err) {
    switch (err) {
#ifdef REG_ESPACE
        case REG_ESPACE:
            return 5;  // TooComplex
#endif
        default:
            return 1;  // InvalidPattern
    }
}

static long aic_rt_regex_map_exec_error(int err) {
    switch (err) {
#ifdef REG_NOMATCH
        case REG_NOMATCH:
            return 3;  // NoMatch
#endif
#ifdef REG_ESPACE
        case REG_ESPACE:
            return 5;  // TooComplex
#endif
        default:
            return 6;  // Internal
    }
}

static long aic_rt_regex_compile_pattern(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    regex_t* out_regex
) {
    (void)pattern_cap;
    if (out_regex == NULL) {
        return 6;
    }
    long flag_check = aic_rt_regex_validate_flags(flags);
    if (flag_check != 0) {
        return flag_check;
    }
    if (pattern_len < 0 || (pattern_len > 0 && pattern_ptr == NULL)) {
        return 2;
    }
    char* pattern = aic_rt_fs_copy_slice(pattern_ptr, pattern_len);
    if (pattern == NULL) {
        return 6;
    }

    int cflags = REG_EXTENDED;
    if ((flags & AIC_RT_REGEX_FLAG_CASE_INSENSITIVE) != 0) {
        cflags |= REG_ICASE;
    }
    if ((flags & AIC_RT_REGEX_FLAG_MULTILINE) != 0) {
        cflags |= REG_NEWLINE;
    }

    int rc = regcomp(out_regex, pattern, cflags);
    free(pattern);
    if (rc != 0) {
        return aic_rt_regex_map_compile_error(rc);
    }
    return 0;
}

long aic_rt_regex_compile(const char* pattern_ptr, long pattern_len, long pattern_cap, long flags) {
    regex_t compiled;
    long err = aic_rt_regex_compile_pattern(pattern_ptr, pattern_len, pattern_cap, flags, &compiled);
    if (err != 0) {
        return err;
    }
    regfree(&compiled);
    return 0;
}

long aic_rt_regex_is_match(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    long* out_is_match
) {
    (void)text_cap;
    if (out_is_match != NULL) {
        *out_is_match = 0;
    }
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 2;
    }
    regex_t compiled;
    long err = aic_rt_regex_compile_pattern(pattern_ptr, pattern_len, pattern_cap, flags, &compiled);
    if (err != 0) {
        return err;
    }

    char* text = aic_rt_fs_copy_slice(text_ptr, text_len);
    if (text == NULL) {
        regfree(&compiled);
        return 6;
    }
    int rc = regexec(&compiled, text, 0, NULL, 0);
    free(text);
    regfree(&compiled);
#ifdef REG_NOMATCH
    if (rc == REG_NOMATCH) {
        if (out_is_match != NULL) {
            *out_is_match = 0;
        }
        return 0;
    }
#endif
    if (rc != 0) {
        return aic_rt_regex_map_exec_error(rc);
    }
    if (out_is_match != NULL) {
        *out_is_match = 1;
    }
    return 0;
}

long aic_rt_regex_find(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    char** out_ptr,
    long* out_len
) {
    (void)text_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 2;
    }
    regex_t compiled;
    long err = aic_rt_regex_compile_pattern(pattern_ptr, pattern_len, pattern_cap, flags, &compiled);
    if (err != 0) {
        return err;
    }

    char* text = aic_rt_fs_copy_slice(text_ptr, text_len);
    if (text == NULL) {
        regfree(&compiled);
        return 6;
    }
    regmatch_t match;
    int rc = regexec(&compiled, text, 1, &match, 0);
    if (rc != 0) {
        free(text);
        regfree(&compiled);
        return aic_rt_regex_map_exec_error(rc);
    }
    if (match.rm_so < 0 || match.rm_eo < match.rm_so) {
        free(text);
        regfree(&compiled);
        return 6;
    }

    size_t start = (size_t)match.rm_so;
    size_t end = (size_t)match.rm_eo;
    char* out = aic_rt_copy_bytes(text + start, end - start);
    free(text);
    regfree(&compiled);
    if (out == NULL) {
        return 6;
    }
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)(end - start);
    }
    return 0;
}

long aic_rt_regex_replace(
    const char* pattern_ptr,
    long pattern_len,
    long pattern_cap,
    long flags,
    const char* text_ptr,
    long text_len,
    long text_cap,
    const char* replacement_ptr,
    long replacement_len,
    long replacement_cap,
    char** out_ptr,
    long* out_len
) {
    (void)text_cap;
    (void)replacement_cap;
    if (out_ptr != NULL) {
        *out_ptr = NULL;
    }
    if (out_len != NULL) {
        *out_len = 0;
    }
    if (text_len < 0 || (text_len > 0 && text_ptr == NULL)) {
        return 2;
    }
    if (replacement_len < 0 || (replacement_len > 0 && replacement_ptr == NULL)) {
        return 2;
    }

    regex_t compiled;
    long err = aic_rt_regex_compile_pattern(pattern_ptr, pattern_len, pattern_cap, flags, &compiled);
    if (err != 0) {
        return err;
    }

    char* text = aic_rt_fs_copy_slice(text_ptr, text_len);
    char* replacement = aic_rt_fs_copy_slice(replacement_ptr, replacement_len);
    if (text == NULL || replacement == NULL) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 6;
    }

    regmatch_t match;
    int rc = regexec(&compiled, text, 1, &match, 0);
    if (rc != 0) {
#ifdef REG_NOMATCH
        if (rc == REG_NOMATCH) {
            size_t text_bytes = strlen(text);
            char* out_copy = aic_rt_copy_bytes(text, text_bytes);
            free(text);
            free(replacement);
            regfree(&compiled);
            if (out_copy == NULL) {
                return 6;
            }
            if (out_ptr != NULL) {
                *out_ptr = out_copy;
            } else {
                free(out_copy);
            }
            if (out_len != NULL) {
                *out_len = (long)text_bytes;
            }
            return 0;
        }
#endif
        free(text);
        free(replacement);
        regfree(&compiled);
        return aic_rt_regex_map_exec_error(rc);
    }
    if (match.rm_so < 0 || match.rm_eo < match.rm_so) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 6;
    }

    size_t text_bytes = strlen(text);
    size_t repl_bytes = strlen(replacement);
    size_t prefix = (size_t)match.rm_so;
    size_t suffix_start = (size_t)match.rm_eo;
    if (suffix_start > text_bytes || prefix > suffix_start) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 6;
    }
    size_t suffix = text_bytes - suffix_start;
    if (prefix > (size_t)LONG_MAX || repl_bytes > (size_t)LONG_MAX || suffix > (size_t)LONG_MAX) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 5;
    }
    if (prefix > SIZE_MAX - repl_bytes || prefix + repl_bytes > SIZE_MAX - suffix) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 5;
    }
    size_t out_bytes = prefix + repl_bytes + suffix;
    if (out_bytes > (size_t)LONG_MAX) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 5;
    }

    char* out = (char*)malloc(out_bytes + 1);
    if (out == NULL) {
        free(text);
        free(replacement);
        regfree(&compiled);
        return 6;
    }
    if (prefix > 0) {
        memcpy(out, text, prefix);
    }
    if (repl_bytes > 0) {
        memcpy(out + prefix, replacement, repl_bytes);
    }
    if (suffix > 0) {
        memcpy(out + prefix + repl_bytes, text + suffix_start, suffix);
    }
    out[out_bytes] = '\0';

    free(text);
    free(replacement);
    regfree(&compiled);
    if (out_ptr != NULL) {
        *out_ptr = out;
    } else {
        free(out);
    }
    if (out_len != NULL) {
        *out_len = (long)out_bytes;
    }
    return 0;
}
#endif

void aic_rt_panic(const char* ptr, long len, long cap, long line, long column) {
    (void)cap;
    if (ptr == NULL) {
        if (line > 0 && column > 0) {
            fprintf(stderr, "AICore panic at %ld:%ld\n", line, column);
        } else {
            fprintf(stderr, "AICore panic\n");
        }
    } else {
        int n = len < 0 ? 0 : (int)len;
        if (line > 0 && column > 0) {
            fprintf(stderr, "AICore panic at %ld:%ld: %.*s\n", line, column, n, ptr);
        } else {
            fprintf(stderr, "AICore panic: %.*s\n", n, ptr);
        }
    }
    fflush(stderr);
    exit(1);
}
"#
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::{
        contracts::lower_runtime_asserts,
        driver::{has_errors, run_frontend},
        ir_builder::build,
        parser::parse,
    };

    use super::{
        emit_llvm, emit_llvm_with_options, ensure_supported_toolchain,
        ensure_supported_toolchain_with_pin, parse_llvm_major, runtime_c_source, CodegenOptions,
        ToolchainInfo,
    };

    #[test]
    fn emits_basic_llvm() {
        let src = "import std.io; fn main() -> Int effects { io } { print_int(1); 0 }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let lowered = lower_runtime_asserts(&ir);
        let output = emit_llvm(&lowered, "test.aic").expect("llvm");
        assert!(output.llvm_ir.contains("define i64 @aic_main()"));
    }

    #[test]
    fn parses_clang_major_from_common_version_strings() {
        let llvm = "clang version 18.1.2 (https://github.com/llvm/llvm-project.git ...)";
        let apple = "Apple clang version 17.0.0 (clang-1700.3.19.1)";
        assert_eq!(parse_llvm_major(llvm), Some(18));
        assert_eq!(parse_llvm_major(apple), Some(17));
    }

    #[test]
    fn rejects_unsupported_llvm_major() {
        let info = ToolchainInfo {
            clang_version: "clang version 10.0.0".to_string(),
            llvm_major: 10,
        };
        let err = ensure_supported_toolchain(&info).expect_err("expected unsupported toolchain");
        assert!(err
            .to_string()
            .contains("unsupported LLVM/clang major version"));
    }

    #[test]
    fn accepts_matching_toolchain_pin() {
        let info = ToolchainInfo {
            clang_version: "clang version 18.1.0".to_string(),
            llvm_major: 18,
        };
        ensure_supported_toolchain_with_pin(&info, Some(18))
            .expect("matching toolchain pin should pass");
    }

    #[test]
    fn rejects_mismatched_toolchain_pin() {
        let info = ToolchainInfo {
            clang_version: "clang version 18.1.0".to_string(),
            llvm_major: 18,
        };
        let err = ensure_supported_toolchain_with_pin(&info, Some(17))
            .expect_err("mismatched toolchain pin should fail");
        assert!(err.to_string().contains("toolchain pin mismatch"));
    }

    #[test]
    fn emits_nested_adt_layout_snapshot() {
        let src = r#"
struct Pair {
    left: Int,
    right: Int,
}

enum Wrap[T] {
    Empty,
    Full(T),
}

fn fold(x: Wrap[Pair]) -> Int {
    match x {
        Empty => 0,
        Full(p) => p.left + p.right,
    }
}

fn main() -> Int {
    fold(Full(Pair { left: 20, right: 22 }))
}
"#;
        let (program, diags) = parse(src, "layout.aic");
        assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
        let ir = build(&program.expect("program"));
        let lowered = lower_runtime_asserts(&ir);
        let output = emit_llvm(&lowered, "layout.aic").expect("llvm");
        assert!(output.llvm_ir.contains("{ i32, i8, { i64, i64 } }"));
        assert!(output.llvm_ir.contains("switch i32"));
    }

    #[test]
    fn monomorphized_generic_symbols_are_deduped_and_stable() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("generic.aic");
        fs::write(
            &file,
            r#"
fn id[T](x: T) -> T {
    x
}

fn main() -> Int {
    let a = id(40);
    let b = id(2);
    let c = id(true);
    if c { a + b } else { 0 }
}
"#,
        )
        .expect("write source");

        let front = run_frontend(&file).expect("frontend");
        assert!(
            !has_errors(&front.diagnostics),
            "diagnostics={:#?}",
            front.diagnostics
        );

        let lowered = lower_runtime_asserts(&front.ir);
        let out1 = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");
        let out2 = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");

        assert_eq!(out1.llvm_ir, out2.llvm_ir, "codegen must be deterministic");

        let int_defs = out1
            .llvm_ir
            .lines()
            .filter(|line| line.starts_with("define i64 @aic_fn_id_Int("))
            .count();
        let bool_defs = out1
            .llvm_ir
            .lines()
            .filter(|line| line.starts_with("define i1 @aic_fn_id_Bool("))
            .count();
        assert_eq!(int_defs, 1, "Int instantiation should be deduped");
        assert_eq!(bool_defs, 1, "Bool instantiation should be emitted");
    }

    #[test]
    fn emits_debug_metadata_and_panic_line_mapping() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("panic_line_map.aic");
        let source = r#"fn main() -> Int effects { io } {
    panic("boom");
    0
}
"#;
        fs::write(&file, source).expect("write source");

        let (program, diags) = parse(source, &file.to_string_lossy());
        assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
        let ir = build(&program.expect("program"));
        let lowered = lower_runtime_asserts(&ir);
        let output = emit_llvm_with_options(
            &lowered,
            &file.to_string_lossy(),
            CodegenOptions { debug_info: true },
        )
        .expect("llvm");

        assert!(output.llvm_ir.contains("!DICompileUnit("));
        assert!(output.llvm_ir.contains("!DISubprogram("));

        let panic_call = output
            .llvm_ir
            .lines()
            .find(|line| line.contains("call void @aic_rt_panic"))
            .expect("panic call line");
        assert!(panic_call.contains("i64 2"), "panic call line={panic_call}");
        assert!(
            panic_call.contains(", !dbg !"),
            "panic call should include debug location"
        );

        let dbg_ref = panic_call.split("!dbg !").nth(1).expect("debug ref").trim();
        let expected = format!("!{} = !DILocation(line: 2,", dbg_ref);
        assert!(
            output.llvm_ir.contains(&expected),
            "missing panic source line location metadata"
        );
    }

    #[test]
    fn release_codegen_omits_debug_metadata() {
        let src = r#"
fn main() -> Int effects { io } {
    panic("boom");
    0
}
"#;
        let (program, diags) = parse(src, "release_mode.aic");
        assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
        let ir = build(&program.expect("program"));
        let lowered = lower_runtime_asserts(&ir);
        let output = emit_llvm(&lowered, "release_mode.aic").expect("llvm");
        assert!(!output.llvm_ir.contains("!DICompileUnit("));
        assert!(!output.llvm_ir.contains("!DILocation("));
    }

    #[test]
    fn panic_runtime_and_ir_abi_match() {
        let src = r#"fn main() -> Int { 0 }"#;
        let (program, diags) = parse(src, "abi_check.aic");
        assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
        let ir = build(&program.expect("program"));
        let lowered = lower_runtime_asserts(&ir);
        let output = emit_llvm(&lowered, "abi_check.aic").expect("llvm");
        assert!(output
            .llvm_ir
            .contains("declare void @aic_rt_panic(i8*, i64, i64, i64, i64)"));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_fs_read_text(i8*, i64, i64, i8**, i64*)"));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_fs_metadata(i8*, i64, i64, i64*, i64*, i64*)"));
        assert!(output.llvm_ir.contains("declare i64 @aic_rt_time_now_ms()"));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_time_monotonic_ms()"));
        assert!(output
            .llvm_ir
            .contains("declare void @aic_rt_time_sleep_ms(i64)"));
        assert!(output
            .llvm_ir
            .contains("declare void @aic_rt_rand_seed(i64)"));
        assert!(output.llvm_ir.contains("declare i64 @aic_rt_rand_next()"));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_rand_range(i64, i64)"));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_conc_spawn(i64, i64, i64*)"));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_conc_join(i64, i64*)"));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_conc_channel_int(i64, i64*)"));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_conc_mutex_lock(i64, i64, i64*)"));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_net_tcp_listen(i8*, i64, i64, i64*)"));
        assert!(output.llvm_ir.contains(
            "declare i64 @aic_rt_net_udp_recv_from(i64, i64, i64, i8**, i64*, i8**, i64*)"
        ));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_net_dns_lookup(i8*, i64, i64, i8**, i64*)"));
        assert!(output
            .llvm_ir
            .contains("declare i64 @aic_rt_regex_compile(i8*, i64, i64, i64)"));
        assert!(output.llvm_ir.contains(
            "declare i64 @aic_rt_regex_is_match(i8*, i64, i64, i64, i8*, i64, i64, i64*)"
        ));
        assert!(output.llvm_ir.contains(
            "declare i64 @aic_rt_regex_replace(i8*, i64, i64, i64, i8*, i64, i64, i8*, i64, i64, i8**, i64*)"
        ));
        assert!(runtime_c_source().contains(
            "void aic_rt_panic(const char* ptr, long len, long cap, long line, long column)"
        ));
        assert!(runtime_c_source().contains("long aic_rt_fs_read_text("));
        assert!(runtime_c_source().contains("long aic_rt_fs_metadata("));
        assert!(runtime_c_source().contains("long aic_rt_time_now_ms(void)"));
        assert!(runtime_c_source().contains("long aic_rt_time_monotonic_ms(void)"));
        assert!(runtime_c_source().contains("void aic_rt_time_sleep_ms(long ms)"));
        assert!(runtime_c_source().contains("void aic_rt_rand_seed(long seed)"));
        assert!(runtime_c_source().contains("long aic_rt_rand_next(void)"));
        assert!(runtime_c_source().contains("long aic_rt_rand_range(long min_inclusive"));
        assert!(runtime_c_source().contains("long aic_rt_conc_spawn(long value, long delay_ms"));
        assert!(runtime_c_source().contains("long aic_rt_conc_join(long handle, long* out_value)"));
        assert!(runtime_c_source()
            .contains("long aic_rt_conc_channel_int(long capacity, long* out_handle)"));
        assert!(runtime_c_source().contains(
            "long aic_rt_conc_mutex_lock(long handle, long timeout_ms, long* out_value)"
        ));
        assert!(runtime_c_source().contains("long aic_rt_net_tcp_listen("));
        assert!(runtime_c_source().contains("long aic_rt_net_udp_recv_from("));
        assert!(runtime_c_source().contains("long aic_rt_net_dns_lookup("));
        assert!(runtime_c_source().contains("long aic_rt_regex_compile("));
        assert!(runtime_c_source().contains("long aic_rt_regex_find("));
        assert!(runtime_c_source().contains("long aic_rt_regex_replace("));
    }
}
