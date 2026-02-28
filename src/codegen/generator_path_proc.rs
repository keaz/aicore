use super::*;

impl<'a> Generator<'a> {
    pub(super) fn gen_path_builtin_call(
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

    pub(super) fn gen_path_join_call(
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

    pub(super) fn gen_path_string_unary_call(
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

    pub(super) fn gen_path_is_abs_call(
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

    pub(super) fn gen_proc_builtin_call(
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
            "run_with" | "aic_proc_run_with_intrinsic" => "run_with",
            "is_running" | "aic_proc_is_running_intrinsic" => "is_running",
            "current_pid" | "aic_proc_current_pid_intrinsic" => "current_pid",
            "run_timeout" | "aic_proc_run_timeout_intrinsic" => "run_timeout",
            "pipe_chain" | "aic_proc_pipe_chain_intrinsic" => "pipe_chain",
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
            "run_with"
                if self.sig_matches_shape(
                    name,
                    &["String", "RunOptions"],
                    "Result[ProcOutput, ProcError]",
                ) =>
            {
                Some(self.gen_proc_run_with_call(name, args, span, fctx))
            }
            "is_running" if self.sig_matches_shape(name, &["Int"], "Result[Bool, ProcError]") => {
                Some(self.gen_proc_is_running_call(name, args, span, fctx))
            }
            "current_pid" if self.sig_matches_shape(name, &[], "Result[Int, ProcError]") => {
                Some(self.gen_proc_current_pid_call(name, args, span, fctx))
            }
            "run_timeout"
                if self.sig_matches_shape(
                    name,
                    &["String", "Int"],
                    "Result[ProcOutput, ProcError]",
                ) =>
            {
                Some(self.gen_proc_run_timeout_call(name, args, span, fctx))
            }
            "pipe_chain"
                if self.sig_matches_shape(
                    name,
                    &["Vec[String]"],
                    "Result[ProcOutput, ProcError]",
                ) =>
            {
                Some(self.gen_proc_pipe_chain_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_proc_spawn_call(
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

    pub(super) fn gen_proc_wait_call(
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

    pub(super) fn gen_proc_kill_call(
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

    pub(super) fn gen_proc_run_call(
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

    pub(super) fn gen_proc_pipe_call(
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

    pub(super) fn gen_proc_run_with_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "run_with expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let command = self.gen_expr(&args[0], fctx)?;
        if command.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "run_with expects (String, RunOptions)",
                self.file,
                span,
            ));
            return None;
        }
        let options = self.gen_expr(&args[1], fctx)?;
        let (stdin_value, cwd_value, env_value, timeout_value) =
            self.proc_run_options_parts(&options, args[1].span, fctx)?;

        let (command_ptr, command_len, command_cap) =
            self.string_parts(&command, args[0].span, fctx)?;
        let (stdin_ptr, stdin_len, stdin_cap) =
            self.string_parts(&stdin_value, args[1].span, fctx)?;
        let (cwd_ptr, cwd_len, cwd_cap) = self.string_parts(&cwd_value, args[1].span, fctx)?;
        let (env_ptr, env_len, env_cap) =
            self.vec_ptr_len_cap_i8(&env_value, args[1].span, fctx)?;

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
            "  {} = call i64 @aic_rt_proc_run_with(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i64* {}, i8** {}, i64* {}, i8** {}, i64* {})",
            err,
            command_ptr,
            command_len,
            command_cap,
            stdin_ptr,
            stdin_len,
            stdin_cap,
            cwd_ptr,
            cwd_len,
            cwd_cap,
            env_ptr,
            env_len,
            env_cap,
            timeout_value.repr.clone().unwrap_or_else(|| "0".to_string()),
            status_slot,
            stdout_ptr_slot,
            stdout_len_slot,
            stderr_ptr_slot,
            stderr_len_slot
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

    pub(super) fn gen_proc_is_running_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "is_running expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "is_running expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }

        let running_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", running_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_proc_is_running(i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            running_slot
        ));
        let running_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            running_raw, running_slot
        ));
        let running = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", running, running_raw));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(running),
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

    pub(super) fn gen_proc_current_pid_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "current_pid expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let pid_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", pid_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_proc_current_pid(i64* {})",
            err, pid_slot
        ));
        let pid = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", pid, pid_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(pid),
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

    pub(super) fn gen_proc_run_timeout_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "run_timeout expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let command = self.gen_expr(&args[0], fctx)?;
        let timeout_ms = self.gen_expr(&args[1], fctx)?;
        if command.ty != LType::String || timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "run_timeout expects (String, Int)",
                self.file,
                span,
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
            "  {} = call i64 @aic_rt_proc_run_timeout(i8* {}, i64 {}, i64 {}, i64 {}, i64* {}, i8** {}, i64* {}, i8** {}, i64* {})",
            err,
            ptr,
            len,
            cap,
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            status_slot,
            stdout_ptr_slot,
            stdout_len_slot,
            stderr_ptr_slot,
            stderr_len_slot
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

    pub(super) fn gen_proc_pipe_chain_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "pipe_chain expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let stages = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, _, _) = self.vec_element_info(&stages.ty, "pipe_chain", args[0].span)?;
        if elem_ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "pipe_chain expects Vec[String]",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (stages_ptr, stages_len, stages_cap) =
            self.vec_ptr_len_cap_i8(&stages, args[0].span, fctx)?;

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
            "  {} = call i64 @aic_rt_proc_pipe_chain(i8* {}, i64 {}, i64 {}, i64* {}, i8** {}, i64* {}, i8** {}, i64* {})",
            err,
            stages_ptr,
            stages_len,
            stages_cap,
            status_slot,
            stdout_ptr_slot,
            stdout_len_slot,
            stderr_ptr_slot,
            stderr_len_slot
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

    pub(super) fn proc_run_options_parts(
        &mut self,
        options: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(Value, Value, Value, Value)> {
        let LType::Struct(layout) = &options.ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "run_with expects RunOptions",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "RunOptions" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("run_with expects RunOptions, found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }

        let options_repr = options
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&options.ty));
        let options_llvm_ty = llvm_type(&options.ty);
        let mut stdin_value = None;
        let mut cwd_value = None;
        let mut env_value = None;
        let mut timeout_value = None;
        for (index, field) in layout.fields.iter().enumerate() {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, {}",
                reg, options_llvm_ty, options_repr, index
            ));
            let value = Value {
                ty: field.ty.clone(),
                repr: Some(reg),
            };
            match field.name.as_str() {
                "stdin" => stdin_value = Some(value),
                "cwd" => cwd_value = Some(value),
                "env" => env_value = Some(value),
                "timeout_ms" => timeout_value = Some(value),
                _ => {}
            }
        }

        let Some(stdin_value) = stdin_value else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "RunOptions is missing `stdin` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(cwd_value) = cwd_value else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "RunOptions is missing `cwd` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(env_value) = env_value else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "RunOptions is missing `env` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(timeout_value) = timeout_value else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "RunOptions is missing `timeout_ms` field",
                self.file,
                span,
            ));
            return None;
        };

        if stdin_value.ty != LType::String
            || cwd_value.ty != LType::String
            || timeout_value.ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "RunOptions fields must be stdin: String, cwd: String, env: Vec[String], timeout_ms: Int",
                self.file,
                span,
            ));
            return None;
        }
        let (env_elem, _, _) = self.vec_element_info(&env_value.ty, "RunOptions.env", span)?;
        if env_elem != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "RunOptions.env must be Vec[String]",
                self.file,
                span,
            ));
            return None;
        }
        Some((stdin_value, cwd_value, env_value, timeout_value))
    }

    pub(super) fn build_proc_output_result(
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

    pub(super) fn wrap_proc_result(
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

    pub(super) fn wrap_signal_result(
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
                    "signal builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_signal_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("signal_ok");
        let err_label = self.new_label("signal_err");
        let cont_label = self.new_label("signal_cont");
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
}
