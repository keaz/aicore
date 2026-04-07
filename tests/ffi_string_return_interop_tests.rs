use std::fs;
use std::process::{Command, Stdio};

use aicore::codegen::{
    compile_with_clang_artifact_with_options, emit_llvm, ArtifactKind, CompileOptions, LinkOptions,
};
use aicore::contracts::lower_runtime_asserts;
use aicore::driver::{has_errors, run_frontend};
use tempfile::tempdir;

fn lower(source: &str) -> aicore::ir::Program {
    let (program, diags) = aicore::parser::parse(source, "ffi_string_return.aic");
    assert!(diags.is_empty(), "parse diagnostics: {diags:#?}");
    aicore::ir_builder::build(&program.expect("program"))
}

fn compile_and_run_with_c_stub(source: &str, c_stub: &str) -> (i32, String, String) {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("main.aic");
    fs::write(&src, source).expect("write source");

    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &src.to_string_lossy()).expect("emit llvm");

    let c_path = dir.path().join("ffi_string_return.c");
    fs::write(&c_path, c_stub).expect("write c stub");
    let obj_path = dir.path().join("ffi_string_return.o");
    let compile_obj = Command::new("clang")
        .current_dir(dir.path())
        .arg("-O0")
        .arg("-c")
        .arg(&c_path)
        .arg("-o")
        .arg(&obj_path)
        .output()
        .expect("compile c object");
    assert!(
        compile_obj.status.success(),
        "clang stderr={}",
        String::from_utf8_lossy(&compile_obj.stderr)
    );

    let exe = dir.path().join("ffi_string_return_demo");
    compile_with_clang_artifact_with_options(
        &llvm.llvm_ir,
        &exe,
        dir.path(),
        ArtifactKind::Exe,
        CompileOptions {
            link: LinkOptions {
                objects: vec![obj_path],
                ..LinkOptions::default()
            },
            ..CompileOptions::default()
        },
    )
    .expect("clang build");

    let output = Command::new(&exe)
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run exe");

    (
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn unit_extern_string_return_is_accepted_and_scalarized_in_llvm() {
    let src = r#"
extern "C" fn ffi_string_banner() -> String;

fn main() -> Int {
    0
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = aicore::resolver::resolve(&ir, "ffi_string_return.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = aicore::typecheck::check(&ir, &res, "ffi_string_return.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);

    let llvm = emit_llvm(&ir, "ffi_string_return.aic").expect("emit llvm");
    assert!(
        llvm.llvm_ir.contains(
            "declare void @ffi_string_banner({ i8*, i64, i64 }* sret({ i8*, i64, i64 }))"
        ),
        "llvm={}",
        llvm.llvm_ir
    );
    assert!(
        llvm.llvm_ir
            .contains("call void @ffi_string_banner({ i8*, i64, i64 }* sret({ i8*, i64, i64 })"),
        "llvm={}",
        llvm.llvm_ir
    );
    assert!(
        llvm.llvm_ir.contains("ret { i8*, i64, i64 }"),
        "llvm={}",
        llvm.llvm_ir
    );
}

#[test]
fn unit_extern_string_return_with_params_scalarizes_and_forwards_arguments() {
    let src = r#"
extern "C" fn ffi_string_repeat(seed: String, times: Int) -> String;

fn main() -> Int {
    let _out = unsafe { ffi_string_repeat("ab", 4) };
    0
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = aicore::resolver::resolve(&ir, "ffi_string_return.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = aicore::typecheck::check(&ir, &res, "ffi_string_return.aic");
    assert!(out.diagnostics.is_empty(), "diags={:#?}", out.diagnostics);

    let llvm = emit_llvm(&ir, "ffi_string_return.aic").expect("emit llvm");
    assert!(
        llvm.llvm_ir.contains(
            "declare void @ffi_string_repeat({ i8*, i64, i64 }* sret({ i8*, i64, i64 }), i8*, i64, i64, i64)"
        ),
        "llvm={}",
        llvm.llvm_ir
    );
    assert!(
        llvm.llvm_ir
            .contains("call void @ffi_string_repeat({ i8*, i64, i64 }* sret({ i8*, i64, i64 })"),
        "llvm={}",
        llvm.llvm_ir
    );
    assert!(
        llvm.llvm_ir.contains(", i8* ")
            && llvm.llvm_ir.contains(", i64 ")
            && llvm.llvm_ir.contains(", i64 ")
            && llvm.llvm_ir.contains(", i64 %arg1"),
        "llvm={}",
        llvm.llvm_ir
    );
}

#[test]
fn unit_extern_unsupported_return_shape_is_still_rejected() {
    let src = r#"
extern "C" fn ffi_bad() -> Bytes;

fn main() -> Int {
    0
}
"#;
    let ir = lower(src);
    let (res, resolve_diags) = aicore::resolver::resolve(&ir, "ffi_string_return.aic");
    assert!(resolve_diags.is_empty(), "resolve={resolve_diags:#?}");
    let out = aicore::typecheck::check(&ir, &res, "ffi_string_return.aic");
    assert!(
        out.diagnostics.iter().any(|d| d.code == "E2123"),
        "diags={:#?}",
        out.diagnostics
    );
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_extern_c_string_return_round_trips_through_native_stub() {
    let source = fs::read_to_string("examples/io/ffi_string_view_return_demo/src/main.aic")
        .expect("read example source");
    let c_stub =
        fs::read_to_string("examples/io/ffi_string_view_return_demo/native/ffi_string_return.c")
            .expect("read example c stub");

    let (code, stdout, stderr) = compile_and_run_with_c_stub(&source, &c_stub);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "8\n", "stderr={stderr}");
}
