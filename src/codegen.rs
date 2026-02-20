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
            command
                .arg("-O0")
                .arg(&ll_path)
                .arg(&runtime_path)
                .arg("-o")
                .arg(output_path);
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
            ir::ExprKind::Await { expr: inner } => self.gen_expr(inner, fctx),
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
        let mut true_arm = None;
        let mut false_arm = None;
        let mut wildcard_arm = None;
        for arm in arms {
            match &arm.pattern.kind {
                ir::PatternKind::Bool(true) => true_arm = Some(arm),
                ir::PatternKind::Bool(false) => false_arm = Some(arm),
                ir::PatternKind::Wildcard | ir::PatternKind::Var(_) => wildcard_arm = Some(arm),
                _ => {}
            }
        }

        let true_arm = true_arm.or(wildcard_arm)?;
        let false_arm = false_arm.or(wildcard_arm)?;

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
        let wildcard_arm = arms.iter().find(|arm| {
            matches!(
                arm.pattern.kind,
                ir::PatternKind::Wildcard | ir::PatternKind::Var(_)
            )
        });

        let mut selected_arms = Vec::new();
        for variant in &layout.variants {
            let explicit = arms.iter().find(|arm| match &arm.pattern.kind {
                ir::PatternKind::Variant { name, .. } => name == &variant.name,
                _ => false,
            });
            let Some(arm) = explicit.or(wildcard_arm) else {
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
            selected_arms.push(arm);
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
        for (idx, arm) in selected_arms.iter().enumerate() {
            let variant = &layout.variants[idx];
            fctx.vars = saved_scope.clone();
            fctx.terminated = false;
            fctx.lines.push(format!("{}:", case_labels[idx]));

            match &arm.pattern.kind {
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
        assert!(runtime_c_source().contains(
            "void aic_rt_panic(const char* ptr, long len, long cap, long line, long column)"
        ));
    }
}
