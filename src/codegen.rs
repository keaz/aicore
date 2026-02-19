use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    Option(Box<LType>),
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

pub struct CodegenOutput {
    pub llvm_ir: String,
}

pub fn emit_llvm(program: &ir::Program, file: &str) -> Result<CodegenOutput, Vec<Diagnostic>> {
    let mut gen = Generator::new(program, file);
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
    fs::create_dir_all(work_dir)?;
    let ll_path = work_dir.join("main.ll");
    let runtime_path = work_dir.join("runtime.c");

    fs::write(&ll_path, llvm_ir)?;
    fs::write(&runtime_path, runtime_c_source())?;

    let status = Command::new("clang")
        .arg("-O0")
        .arg(&ll_path)
        .arg(&runtime_path)
        .arg("-o")
        .arg(output_path)
        .status()?;
    if !status.success() {
        anyhow::bail!("clang failed to build executable")
    }
    Ok(output_path.to_path_buf())
}

struct Generator<'a> {
    program: &'a ir::Program,
    file: &'a str,
    diagnostics: Vec<Diagnostic>,
    out: Vec<String>,
    globals: Vec<String>,
    string_counter: usize,
    temp_counter: usize,
    label_counter: usize,
    fn_sigs: BTreeMap<String, FnSig>,
    type_map: BTreeMap<ir::TypeId, String>,
}

impl<'a> Generator<'a> {
    fn new(program: &'a ir::Program, file: &'a str) -> Self {
        let mut type_map = BTreeMap::new();
        for ty in &program.types {
            type_map.insert(ty.id, ty.repr.clone());
        }
        Self {
            program,
            file,
            diagnostics: Vec::new(),
            out: Vec::new(),
            globals: Vec::new(),
            string_counter: 0,
            temp_counter: 0,
            label_counter: 0,
            fn_sigs: BTreeMap::new(),
            type_map,
        }
    }

    fn finish(self) -> String {
        let mut text = String::new();
        text.push_str("; AICore LLVM IR (deterministic)\n");
        text.push_str("declare void @aic_rt_print_int(i64)\n");
        text.push_str("declare void @aic_rt_print_str(i8*)\n");
        text.push_str("declare i64 @aic_rt_strlen(i8*)\n");
        text.push_str("declare void @aic_rt_panic(i8*)\n\n");

        for global in &self.globals {
            text.push_str(global);
            text.push('\n');
        }
        if !self.globals.is_empty() {
            text.push('\n');
        }

        for line in self.out {
            text.push_str(&line);
            text.push('\n');
        }
        text
    }

    fn generate(&mut self) {
        self.collect_fn_sigs();

        for item in &self.program.items {
            if let ir::Item::Function(func) = item {
                self.gen_function(func);
            }
        }

        self.gen_entry_wrapper();
    }

    fn collect_fn_sigs(&mut self) {
        for item in &self.program.items {
            if let ir::Item::Function(func) = item {
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

        let llvm_ret = llvm_type(&sig.ret);
        let mut param_defs = Vec::new();
        for (idx, ty) in sig.params.iter().enumerate() {
            param_defs.push(format!("{} %arg{}", llvm_type(ty), idx));
        }

        self.out.push(format!(
            "define {} @{}({}) {{",
            llvm_ret,
            mangle(&func.name),
            param_defs.join(", ")
        ));

        let mut fctx = FnCtx {
            lines: Vec::new(),
            vars: vec![BTreeMap::new()],
            terminated: false,
            current_label: "entry".to_string(),
        };
        fctx.lines.push("entry:".to_string());

        for (idx, param) in func.params.iter().enumerate() {
            let Some(ty) = self.type_from_id(param.ty, param.span) else {
                return;
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

        self.out.extend(fctx.lines.into_iter());
        self.out.push("}".to_string());
        self.out.push(String::new());
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
                        let msg_ptr = self.string_literal(message, fctx);
                        fctx.lines
                            .push(format!("  call void @aic_rt_panic(i8* {})", msg_ptr));
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
            ir::ExprKind::String(s) => {
                let ptr = self.string_literal(s, fctx);
                Some(Value {
                    ty: LType::String,
                    repr: Some(ptr),
                })
            }
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
            ir::ExprKind::StructInit { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5004",
                    "struct codegen is not yet implemented in MVP backend",
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::FieldAccess { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5005",
                    "field access codegen is not yet implemented in MVP backend",
                    self.file,
                    expr.span,
                ));
                None
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
        // Option constructors.
        if name == "None" {
            if !args.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    "E5009",
                    "None constructor takes no arguments",
                    self.file,
                    span,
                ));
                return None;
            }
            // Default to Option[Int] unless context infers otherwise. MVP keeps Option[Int].
            let ty = LType::Option(Box::new(LType::Int));
            let reg0 = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} undef, i1 0, 0",
                reg0,
                llvm_type(&ty)
            ));
            let reg1 = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} {}, i64 0, 1",
                reg1,
                llvm_type(&ty),
                reg0
            ));
            return Some(Value {
                ty,
                repr: Some(reg1),
            });
        }
        if name == "Some" {
            if args.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E5009",
                    "Some constructor takes one argument",
                    self.file,
                    span,
                ));
                return None;
            }
            let payload = self.gen_expr(&args[0], fctx)?;
            let ty = LType::Option(Box::new(payload.ty.clone()));
            let reg0 = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} undef, i1 1, 0",
                reg0,
                llvm_type(&ty)
            ));
            let reg1 = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} {}, {} {}, 1",
                reg1,
                llvm_type(&ty),
                reg0,
                llvm_type(&payload.ty),
                payload.repr.unwrap_or_else(|| default_value(&payload.ty))
            ));
            return Some(Value {
                ty,
                repr: Some(reg1),
            });
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
            fctx.lines.push(format!(
                "  call void @aic_rt_print_str(i8* {})",
                arg.repr.unwrap_or_else(|| "null".to_string())
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
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call i64 @aic_rt_strlen(i8* {})",
                reg,
                arg.repr.unwrap_or_else(|| "null".to_string())
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
            fctx.lines.push(format!(
                "  call void @aic_rt_panic(i8* {})",
                arg.repr.unwrap_or_else(|| "null".to_string())
            ));
            fctx.lines.push("  unreachable".to_string());
            fctx.terminated = true;
            return Some(Value {
                ty: LType::Unit,
                repr: None,
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

        // Determine result type by a dry-run guess from tails when possible.
        let then_ty = infer_block_tail_type(then_block, &self.fn_sigs, &self.type_map, &fctx.vars);
        let else_ty = infer_block_tail_type(else_block, &self.fn_sigs, &self.type_map, &fctx.vars);
        let result_ty = if then_ty == else_ty {
            then_ty
        } else {
            LType::Unit
        };
        let result_slot = if result_ty != LType::Unit {
            let ptr = self.new_temp();
            fctx.lines
                .push(format!("  {} = alloca {}", ptr, llvm_type(&result_ty)));
            Some(ptr)
        } else {
            None
        };

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
            if let (Some(slot), Some(value)) = (result_slot.as_ref(), then_value) {
                let repr = value.repr.unwrap_or_else(|| default_value(&result_ty));
                fctx.lines.push(format!(
                    "  store {} {}, {}* {}",
                    llvm_type(&result_ty),
                    repr,
                    llvm_type(&result_ty),
                    slot
                ));
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
            if let (Some(slot), Some(value)) = (result_slot.as_ref(), else_value) {
                let repr = value.repr.unwrap_or_else(|| default_value(&result_ty));
                fctx.lines.push(format!(
                    "  store {} {}, {}* {}",
                    llvm_type(&result_ty),
                    repr,
                    llvm_type(&result_ty),
                    slot
                ));
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

        if let Some(slot) = result_slot {
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
            LType::Option(inner) => self.gen_match_option(scrutinee, &inner, arms, fctx),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E5016",
                    "match codegen currently supports Bool and Option[T]",
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

        let result_ty = infer_expr_type(&true_arm.body, &self.fn_sigs, &self.type_map, &fctx.vars)
            .zip(infer_expr_type(
                &false_arm.body,
                &self.fn_sigs,
                &self.type_map,
                &fctx.vars,
            ))
            .and_then(|(a, b)| if a == b { Some(a) } else { None })
            .unwrap_or(LType::Unit);

        let result_slot = if result_ty != LType::Unit {
            let ptr = self.new_temp();
            fctx.lines
                .push(format!("  {} = alloca {}", ptr, llvm_type(&result_ty)));
            Some(ptr)
        } else {
            None
        };

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
            if let (Some(slot), Some(tv)) = (result_slot.as_ref(), tv) {
                let repr = tv.repr.unwrap_or_else(|| default_value(&result_ty));
                fctx.lines.push(format!(
                    "  store {} {}, {}* {}",
                    llvm_type(&result_ty),
                    repr,
                    llvm_type(&result_ty),
                    slot
                ));
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        fctx.vars = saved_scope.clone();
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", else_label));
        let ev = self.gen_expr(&false_arm.body, fctx);
        let e_term = fctx.terminated;
        if !e_term {
            if let (Some(slot), Some(ev)) = (result_slot.as_ref(), ev) {
                let repr = ev.repr.unwrap_or_else(|| default_value(&result_ty));
                fctx.lines.push(format!(
                    "  store {} {}, {}* {}",
                    llvm_type(&result_ty),
                    repr,
                    llvm_type(&result_ty),
                    slot
                ));
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
        if let Some(slot) = result_slot {
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

    fn gen_match_option(
        &mut self,
        scrutinee: Value,
        inner: &LType,
        arms: &[ir::MatchArm],
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let mut none_arm = None;
        let mut some_arm = None;
        let mut wildcard_arm = None;

        for arm in arms {
            match &arm.pattern.kind {
                ir::PatternKind::Variant { name, .. } if name == "None" => none_arm = Some(arm),
                ir::PatternKind::Variant { name, .. } if name == "Some" => some_arm = Some(arm),
                ir::PatternKind::Wildcard | ir::PatternKind::Var(_) => wildcard_arm = Some(arm),
                _ => {}
            }
        }

        let none_arm = none_arm.or(wildcard_arm)?;
        let some_arm = some_arm.or(wildcard_arm)?;

        let none_label = self.new_label("match_none");
        let some_label = self.new_label("match_some");
        let cont_label = self.new_label("match_cont");

        let none_ty = infer_expr_type(&none_arm.body, &self.fn_sigs, &self.type_map, &fctx.vars)
            .unwrap_or(LType::Unit);
        let some_ty =
            infer_some_arm_type(some_arm, inner, &self.fn_sigs, &self.type_map, &fctx.vars)
                .unwrap_or(LType::Unit);
        let result_ty = if none_ty == some_ty {
            none_ty
        } else if none_ty == LType::Unit {
            some_ty
        } else if some_ty == LType::Unit {
            none_ty
        } else {
            LType::Unit
        };

        let result_slot = if result_ty != LType::Unit {
            let ptr = self.new_temp();
            fctx.lines
                .push(format!("  {} = alloca {}", ptr, llvm_type(&result_ty)));
            Some(ptr)
        } else {
            None
        };

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
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            tag, some_label, none_label
        ));

        let saved_scope = fctx.vars.clone();
        let saved_terminated = fctx.terminated;

        // none
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", none_label));
        let nv = self.gen_expr(&none_arm.body, fctx);
        let n_term = fctx.terminated;
        if !n_term {
            if let (Some(slot), Some(nv)) = (result_slot.as_ref(), nv) {
                let repr = nv.repr.unwrap_or_else(|| default_value(&result_ty));
                fctx.lines.push(format!(
                    "  store {} {}, {}* {}",
                    llvm_type(&result_ty),
                    repr,
                    llvm_type(&result_ty),
                    slot
                ));
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        // some
        fctx.vars = saved_scope.clone();
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", some_label));

        if let ir::PatternKind::Variant { args, .. } = &some_arm.pattern.kind {
            if let Some(binding_pat) = args.first() {
                match &binding_pat.kind {
                    ir::PatternKind::Var(name) => {
                        let payload = self.new_temp();
                        fctx.lines.push(format!(
                            "  {} = extractvalue {} {}, 1",
                            payload,
                            llvm_type(&scrutinee.ty),
                            scrutinee
                                .repr
                                .clone()
                                .unwrap_or_else(|| default_value(&scrutinee.ty))
                        ));
                        let ptr = self.new_temp();
                        fctx.lines
                            .push(format!("  {} = alloca {}", ptr, llvm_type(inner)));
                        fctx.lines.push(format!(
                            "  store {} {}, {}* {}",
                            llvm_type(inner),
                            payload,
                            llvm_type(inner),
                            ptr
                        ));
                        fctx.vars.last_mut().expect("scope").insert(
                            name.clone(),
                            Local {
                                ty: inner.clone(),
                                ptr,
                            },
                        );
                    }
                    ir::PatternKind::Wildcard => {}
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E5017",
                            "Option Some pattern codegen supports var or wildcard payload",
                            self.file,
                            binding_pat.span,
                        ));
                    }
                }
            }
        }

        let sv = self.gen_expr(&some_arm.body, fctx);
        let s_term = fctx.terminated;
        if !s_term {
            if let (Some(slot), Some(sv)) = (result_slot.as_ref(), sv) {
                let repr = sv.repr.unwrap_or_else(|| default_value(&result_ty));
                fctx.lines.push(format!(
                    "  store {} {}, {}* {}",
                    llvm_type(&result_ty),
                    repr,
                    llvm_type(&result_ty),
                    slot
                ));
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        fctx.vars = saved_scope;
        fctx.terminated = saved_terminated;

        if n_term && s_term {
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        fctx.lines.push(format!("{}:", cont_label));
        if let Some(slot) = result_slot {
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
        match parse_type(&repr) {
            Some(ty) => Some(ty),
            None => {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!("unsupported type '{}' in codegen MVP", repr),
                    self.file,
                    span,
                ));
                None
            }
        }
    }

    fn string_literal(&mut self, s: &str, fctx: &mut FnCtx) -> String {
        let id = self.string_counter;
        self.string_counter += 1;
        let name = format!("@.str.{}", id);
        let (bytes, len) = escape_c_string_bytes(s);
        let const_text = format!(
            "{} = private unnamed_addr constant [{} x i8] c\"{}\"",
            name, len, bytes
        );
        self.globals.push(const_text);

        let gep = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            gep, len, len, name
        ));
        gep
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
}

fn infer_block_tail_type(
    block: &ir::Block,
    fns: &BTreeMap<String, FnSig>,
    types: &BTreeMap<ir::TypeId, String>,
    scopes: &[BTreeMap<String, Local>],
) -> LType {
    let mut env = scopes.to_vec();
    env.push(BTreeMap::new());
    for stmt in &block.stmts {
        if let ir::Stmt::Let { name, expr, .. } = stmt {
            if let Some(ty) = infer_expr_type(expr, fns, types, &env) {
                env.last_mut().expect("scope").insert(
                    name.clone(),
                    Local {
                        ty,
                        ptr: String::new(),
                    },
                );
            }
        }
    }
    if let Some(tail) = &block.tail {
        infer_expr_type(tail, fns, types, &env).unwrap_or(LType::Unit)
    } else {
        LType::Unit
    }
}

fn infer_expr_type(
    expr: &ir::Expr,
    fns: &BTreeMap<String, FnSig>,
    types: &BTreeMap<ir::TypeId, String>,
    scopes: &[BTreeMap<String, Local>],
) -> Option<LType> {
    match &expr.kind {
        ir::ExprKind::Int(_) => Some(LType::Int),
        ir::ExprKind::Bool(_) => Some(LType::Bool),
        ir::ExprKind::String(_) => Some(LType::String),
        ir::ExprKind::Unit => Some(LType::Unit),
        ir::ExprKind::Var(name) => find_local(scopes, name).map(|l| l.ty),
        ir::ExprKind::Call { callee, args } => {
            if let Some(path) = extract_callee_path(callee) {
                if let Some(name) = path.last() {
                    if name == "Some" {
                        let inner = args
                            .first()
                            .and_then(|a| infer_expr_type(a, fns, types, scopes))
                            .unwrap_or(LType::Int);
                        Some(LType::Option(Box::new(inner)))
                    } else if name == "None" {
                        Some(LType::Option(Box::new(LType::Int)))
                    } else {
                        fns.get(name).map(|s| s.ret.clone())
                    }
                } else {
                    Some(LType::Unit)
                }
            } else {
                Some(LType::Unit)
            }
        }
        ir::ExprKind::If {
            then_block,
            else_block,
            ..
        } => {
            let t = infer_block_tail_type(then_block, fns, types, scopes);
            let e = infer_block_tail_type(else_block, fns, types, scopes);
            if t == e {
                Some(t)
            } else {
                Some(LType::Unit)
            }
        }
        ir::ExprKind::Match { arms, .. } => {
            let mut ty = None;
            for arm in arms {
                let arm_ty = infer_expr_type(&arm.body, fns, types, scopes);
                if ty.is_none() {
                    ty = arm_ty;
                }
            }
            ty
        }
        ir::ExprKind::Binary { op, .. } => match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => Some(LType::Int),
            _ => Some(LType::Bool),
        },
        ir::ExprKind::Unary { op, .. } => match op {
            UnaryOp::Neg => Some(LType::Int),
            UnaryOp::Not => Some(LType::Bool),
        },
        ir::ExprKind::StructInit { .. } => None,
        ir::ExprKind::FieldAccess { .. } => None,
    }
}

fn infer_some_arm_type(
    arm: &ir::MatchArm,
    inner: &LType,
    fns: &BTreeMap<String, FnSig>,
    types: &BTreeMap<ir::TypeId, String>,
    scopes: &[BTreeMap<String, Local>],
) -> Option<LType> {
    if let ir::PatternKind::Variant { name, args } = &arm.pattern.kind {
        if name == "Some" {
            if let Some(arg0) = args.first() {
                if let ir::PatternKind::Var(binding) = &arg0.kind {
                    if let ir::ExprKind::Var(use_name) = &arm.body.kind {
                        if binding == use_name {
                            return Some(inner.clone());
                        }
                    }
                }
            }
        }
    }
    infer_expr_type(&arm.body, fns, types, scopes)
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
    // MVP fallback for None: Option[Int] into Option[T] where compatible.
    default_value(expected)
}

fn parse_type(repr: &str) -> Option<LType> {
    match repr {
        "Int" => Some(LType::Int),
        "Bool" => Some(LType::Bool),
        "String" => Some(LType::String),
        "()" => Some(LType::Unit),
        _ => {
            if let Some(inner) = repr
                .strip_prefix("Option[")
                .and_then(|s| s.strip_suffix(']'))
            {
                return parse_type(inner).map(|ty| LType::Option(Box::new(ty)));
            }
            None
        }
    }
}

fn llvm_type(ty: &LType) -> String {
    match ty {
        LType::Int => "i64".to_string(),
        LType::Bool => "i1".to_string(),
        LType::Unit => "void".to_string(),
        LType::String => "i8*".to_string(),
        LType::Option(inner) => format!("{{ i1, {} }}", llvm_type(inner)),
    }
}

fn default_value(ty: &LType) -> String {
    match ty {
        LType::Int => "0".to_string(),
        LType::Bool => "0".to_string(),
        LType::Unit => String::new(),
        LType::String => "null".to_string(),
        LType::Option(inner) => {
            format!("{{ i1 0, {} {} }}", llvm_type(inner), default_value(inner))
        }
    }
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

void aic_rt_print_int(long x) {
    printf("%ld\n", x);
}

void aic_rt_print_str(const char* s) {
    if (s == NULL) {
        printf("<null>\n");
        return;
    }
    printf("%s\n", s);
}

long aic_rt_strlen(const char* s) {
    if (s == NULL) {
        return 0;
    }
    return (long)strlen(s);
}

void aic_rt_panic(const char* msg) {
    if (msg == NULL) {
        fprintf(stderr, "AICore panic\n");
    } else {
        fprintf(stderr, "AICore panic: %s\n", msg);
    }
    fflush(stderr);
    exit(1);
}
"#
}

#[cfg(test)]
mod tests {
    use crate::{contracts::lower_runtime_asserts, ir_builder::build, parser::parse};

    use super::emit_llvm;

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
}
