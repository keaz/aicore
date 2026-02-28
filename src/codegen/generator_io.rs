use super::*;

impl<'a> Generator<'a> {
    pub(super) fn gen_io_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "read_line" | "aic_io_read_line_intrinsic" => "read_line",
            "read_int" | "aic_io_read_int_intrinsic" => "read_int",
            "read_char" | "aic_io_read_char_intrinsic" => "read_char",
            "prompt" | "aic_io_prompt_intrinsic" => "prompt",
            "eprint_str" | "aic_io_eprint_str_intrinsic" => "eprint_str",
            "eprint_int" | "aic_io_eprint_int_intrinsic" => "eprint_int",
            "println_str" | "aic_io_println_str_intrinsic" => "println_str",
            "println_int" | "aic_io_println_int_intrinsic" => "println_int",
            "print_bool" | "aic_io_print_bool_intrinsic" => "print_bool",
            "println_bool" | "aic_io_println_bool_intrinsic" => "println_bool",
            "flush_stdout" | "aic_io_flush_stdout_intrinsic" => "flush_stdout",
            "flush_stderr" | "aic_io_flush_stderr_intrinsic" => "flush_stderr",
            "aic_io_write_stdout_intrinsic" => "io_write_stdout",
            "aic_io_write_stderr_intrinsic" => "io_write_stderr",
            "aic_io_file_read_line_intrinsic" => "io_file_read_line",
            "aic_io_file_write_str_intrinsic" => "io_file_write_str",
            "aic_io_file_close_intrinsic" => "io_file_close",
            "aic_io_tcp_send_intrinsic" => "io_tcp_send",
            "aic_io_tcp_recv_intrinsic" => "io_tcp_recv",
            "aic_io_tcp_close_intrinsic" => "io_tcp_close",
            "aic_io_mock_reader_install_intrinsic" => "io_mock_reader_install",
            "aic_io_mock_writer_take_stdout_intrinsic" => "io_mock_writer_take_stdout",
            "aic_io_mock_writer_take_stderr_intrinsic" => "io_mock_writer_take_stderr",
            "aic_log_emit_intrinsic" => "log_emit",
            "aic_log_set_level_intrinsic" => "log_set_level",
            "aic_log_set_json_output_intrinsic" => "log_set_json",
            _ => return None,
        };

        match canonical {
            "read_line" if self.sig_matches_shape(name, &[], "Result[String, IoError]") => {
                Some(self.gen_io_string_result_noarg_call(
                    name,
                    "aic_rt_read_line",
                    args,
                    "read_line",
                    span,
                    fctx,
                ))
            }
            "read_int" if self.sig_matches_shape(name, &[], "Result[Int, IoError]") => {
                Some(self.gen_io_int_result_noarg_call(
                    name,
                    "aic_rt_read_int",
                    args,
                    "read_int",
                    span,
                    fctx,
                ))
            }
            "read_char" if self.sig_matches_shape(name, &[], "Result[String, IoError]") => {
                Some(self.gen_io_string_result_noarg_call(
                    name,
                    "aic_rt_read_char",
                    args,
                    "read_char",
                    span,
                    fctx,
                ))
            }
            "prompt" if self.sig_matches_shape(name, &["String"], "Result[String, IoError]") => {
                Some(self.gen_io_prompt_call(name, args, span, fctx))
            }
            "eprint_str" if self.sig_matches_shape(name, &["String"], "()") => Some(
                self.gen_io_string_void_call("aic_rt_eprint_str", args, "eprint_str", span, fctx),
            ),
            "eprint_int" if self.sig_matches_shape(name, &["Int"], "()") => {
                Some(self.gen_io_int_void_call("aic_rt_eprint_int", args, "eprint_int", span, fctx))
            }
            "println_str" if self.sig_matches_shape(name, &["String"], "()") => Some(
                self.gen_io_string_void_call("aic_rt_println_str", args, "println_str", span, fctx),
            ),
            "println_int" if self.sig_matches_shape(name, &["Int"], "()") => Some(
                self.gen_io_int_void_call("aic_rt_println_int", args, "println_int", span, fctx),
            ),
            "print_bool" if self.sig_matches_shape(name, &["Bool"], "()") => Some(
                self.gen_io_bool_void_call("aic_rt_print_bool", args, "print_bool", span, fctx),
            ),
            "println_bool" if self.sig_matches_shape(name, &["Bool"], "()") => Some(
                self.gen_io_bool_void_call("aic_rt_println_bool", args, "println_bool", span, fctx),
            ),
            "flush_stdout" if self.sig_matches_shape(name, &[], "()") => Some(
                self.gen_io_flush_call("aic_rt_flush_stdout", args, "flush_stdout", span, fctx),
            ),
            "flush_stderr" if self.sig_matches_shape(name, &[], "()") => Some(
                self.gen_io_flush_call("aic_rt_flush_stderr", args, "flush_stderr", span, fctx),
            ),
            "io_write_stdout"
                if self.sig_matches_shape(name, &["String"], "Result[Int, IoError]") =>
            {
                Some(self.gen_io_write_string_call(
                    name,
                    "aic_rt_print_str",
                    args,
                    "aic_io_write_stdout_intrinsic",
                    span,
                    fctx,
                ))
            }
            "io_write_stderr"
                if self.sig_matches_shape(name, &["String"], "Result[Int, IoError]") =>
            {
                Some(self.gen_io_write_string_call(
                    name,
                    "aic_rt_eprint_str",
                    args,
                    "aic_io_write_stderr_intrinsic",
                    span,
                    fctx,
                ))
            }
            "io_file_read_line"
                if self.sig_matches_shape(
                    name,
                    &["FileHandle"],
                    "Result[Option[String], IoError]",
                ) =>
            {
                Some(self.gen_io_file_read_line_call(name, args, span, fctx))
            }
            "io_file_write_str"
                if self.sig_matches_shape(
                    name,
                    &["FileHandle", "String"],
                    "Result[Int, IoError]",
                ) =>
            {
                Some(self.gen_io_file_write_str_call(name, args, span, fctx))
            }
            "io_file_close"
                if self.sig_matches_shape(name, &["FileHandle"], "Result[Bool, IoError]") =>
            {
                Some(self.gen_io_file_close_call(name, args, span, fctx))
            }
            "io_tcp_send"
                if self.sig_matches_shape(name, &["Int", "String"], "Result[Int, IoError]") =>
            {
                Some(self.gen_io_tcp_send_call(name, args, span, fctx))
            }
            "io_tcp_recv"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[String, IoError]",
                ) =>
            {
                Some(self.gen_io_tcp_recv_call(name, args, span, fctx))
            }
            "io_tcp_close" if self.sig_matches_shape(name, &["Int"], "Result[Bool, IoError]") => {
                Some(self.gen_io_tcp_close_call(name, args, span, fctx))
            }
            "io_mock_reader_install"
                if self.sig_matches_shape(name, &["String"], "Result[Bool, IoError]") =>
            {
                Some(self.gen_io_mock_reader_install_call(name, args, span, fctx))
            }
            "io_mock_writer_take_stdout"
                if self.sig_matches_shape(name, &[], "Result[String, IoError]") =>
            {
                Some(self.gen_io_string_result_noarg_call(
                    name,
                    "aic_rt_mock_io_take_stdout",
                    args,
                    "aic_io_mock_writer_take_stdout_intrinsic",
                    span,
                    fctx,
                ))
            }
            "io_mock_writer_take_stderr"
                if self.sig_matches_shape(name, &[], "Result[String, IoError]") =>
            {
                Some(self.gen_io_string_result_noarg_call(
                    name,
                    "aic_rt_mock_io_take_stderr",
                    args,
                    "aic_io_mock_writer_take_stderr_intrinsic",
                    span,
                    fctx,
                ))
            }
            "log_emit" if self.sig_matches_shape(name, &["Int", "String"], "()") => {
                Some(self.gen_log_emit_call(args, span, fctx))
            }
            "log_set_level" if self.sig_matches_shape(name, &["Int"], "()") => {
                Some(self.gen_io_int_void_call(
                    "aic_rt_log_set_level",
                    args,
                    "aic_log_set_level_intrinsic",
                    span,
                    fctx,
                ))
            }
            "log_set_json" if self.sig_matches_shape(name, &["Bool"], "()") => {
                Some(self.gen_io_bool_void_call(
                    "aic_rt_log_set_json",
                    args,
                    "aic_log_set_json_output_intrinsic",
                    span,
                    fctx,
                ))
            }
            _ => None,
        }
    }

    pub(super) fn gen_log_emit_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_log_emit_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }

        let level = self.gen_expr(&args[0], fctx)?;
        if level.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_log_emit_intrinsic expects (Int, String)",
                self.file,
                args[0].span,
            ));
            return None;
        }

        let message = self.gen_expr(&args[1], fctx)?;
        if message.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_log_emit_intrinsic expects (Int, String)",
                self.file,
                args[1].span,
            ));
            return None;
        }

        let (ptr, len, cap) = self.string_parts(&message, args[1].span, fctx)?;
        let level_repr = coerce_repr(&level, &LType::Int);
        fctx.lines.push(format!(
            "  call void @aic_rt_log_emit(i64 {}, i8* {}, i64 {}, i64 {})",
            level_repr, ptr, len, cap
        ));
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    pub(super) fn gen_io_string_result_noarg_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context} expects zero arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8** {}, i64* {})",
            err, runtime_fn, out_ptr_slot, out_len_slot
        ));
        let payload = self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_io_result(&result_ty, payload, &err, span, fctx)
    }

    pub(super) fn gen_io_int_result_noarg_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context} expects zero arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let out_value_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_value_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64* {})",
            err, runtime_fn, out_value_slot
        ));
        let out_value = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_value, out_value_slot
        ));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_value),
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
        self.wrap_io_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_io_prompt_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "prompt expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let message = self.gen_expr(&args[0], fctx)?;
        if message.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "prompt expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&message, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_prompt(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err, ptr, len, cap, out_ptr_slot, out_len_slot
        ));
        let payload = self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_io_result(&result_ty, payload, &err, span, fctx)
    }

    pub(super) fn gen_io_write_string_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&value, args[0].span, fctx)?;
        fctx.lines.push(format!(
            "  call void @{}(i8* {}, i64 {}, i64 {})",
            runtime_fn, ptr, len, cap
        ));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(len),
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
        self.wrap_io_result(&result_ty, ok_payload, "0", span, fctx)
    }

    pub(super) fn gen_io_mock_reader_install_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_io_mock_reader_install_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }

        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_io_mock_reader_install_intrinsic expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }

        let (ptr, len, cap) = self.string_parts(&value, args[0].span, fctx)?;
        let io_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_mock_io_set_stdin(i8* {}, i64 {}, i64 {})",
            io_err, ptr, len, cap
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

        self.wrap_io_result(&result_ty, ok_payload, &io_err, span, fctx)
    }

    pub(super) fn map_fs_err_to_io_code(&mut self, fs_err: &str, fctx: &mut FnCtx) -> String {
        let is_not_found = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 1", is_not_found, fs_err));
        let mapped_non_invalid = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 1, i64 3",
            mapped_non_invalid, is_not_found
        ));
        let is_invalid = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 4", is_invalid, fs_err));
        let mapped_non_ok = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 2, i64 {}",
            mapped_non_ok, is_invalid, mapped_non_invalid
        ));
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, fs_err));
        let mapped = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 0, i64 {}",
            mapped, is_ok, mapped_non_ok
        ));
        mapped
    }

    pub(super) fn map_net_err_to_io_code(&mut self, net_err: &str, fctx: &mut FnCtx) -> String {
        let is_timeout = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 4", is_timeout, net_err));
        let mapped_non_invalid = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 1, i64 3",
            mapped_non_invalid, is_timeout
        ));
        let is_invalid = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 6", is_invalid, net_err));
        let mapped_non_ok = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 2, i64 {}",
            mapped_non_ok, is_invalid, mapped_non_invalid
        ));
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, net_err));
        let mapped = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 0, i64 {}",
            mapped, is_ok, mapped_non_ok
        ));
        mapped
    }

    pub(super) fn gen_io_file_read_line_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_io_file_read_line_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let file = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &file,
            "FileHandle",
            "aic_io_file_read_line_intrinsic",
            args[0].span,
            fctx,
        )?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let out_has_line_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_has_line_slot));
        let fs_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_file_read_line(i64 {}, i8** {}, i64* {}, i64* {})",
            fs_err, handle, out_ptr_slot, out_len_slot, out_has_line_slot
        ));
        let io_err = self.map_fs_err_to_io_code(&fs_err, fctx);

        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let out_has_line = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_has_line, out_has_line_slot
        ));
        let has_line = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", has_line, out_has_line));
        let line_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);

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
        let option_value =
            self.wrap_option_with_condition(&ok_ty, line_value, &has_line, span, fctx)?;
        self.wrap_io_result(&result_ty, option_value, &io_err, span, fctx)
    }

    pub(super) fn gen_io_file_write_str_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_io_file_write_str_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let file = self.gen_expr(&args[0], fctx)?;
        let content = self.gen_expr(&args[1], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &file,
            "FileHandle",
            "aic_io_file_write_str_intrinsic",
            args[0].span,
            fctx,
        )?;
        if content.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_io_file_write_str_intrinsic expects (FileHandle, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&content, args[1].span, fctx)?;
        let fs_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_file_write_str(i64 {}, i8* {}, i64 {}, i64 {})",
            fs_err, handle, ptr, len, cap
        ));
        let io_err = self.map_fs_err_to_io_code(&fs_err, fctx);
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(len),
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
        self.wrap_io_result(&result_ty, ok_payload, &io_err, span, fctx)
    }

    pub(super) fn gen_io_file_close_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_io_file_close_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let file = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &file,
            "FileHandle",
            "aic_io_file_close_intrinsic",
            args[0].span,
            fctx,
        )?;
        let fs_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_file_close(i64 {})",
            fs_err, handle
        ));
        let io_err = self.map_fs_err_to_io_code(&fs_err, fctx);
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
        self.wrap_io_result(&result_ty, ok_payload, &io_err, span, fctx)
    }

    pub(super) fn gen_io_tcp_send_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_io_tcp_send_intrinsic expects two arguments",
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
                "aic_io_tcp_send_intrinsic expects (Int, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&payload, args[1].span, fctx)?;
        let sent_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", sent_slot));
        let net_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_send(i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            net_err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            ptr,
            len,
            cap,
            sent_slot
        ));
        let io_err = self.map_net_err_to_io_code(&net_err, fctx);
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
        self.wrap_io_result(&result_ty, ok_payload, &io_err, span, fctx)
    }

    pub(super) fn gen_io_tcp_recv_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_io_tcp_recv_intrinsic expects three arguments",
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
                "aic_io_tcp_recv_intrinsic expects (Int, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let net_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_recv(i64 {}, i64 {}, i64 {}, i8** {}, i64* {})",
            net_err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            max_bytes.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let io_err = self.map_net_err_to_io_code(&net_err, fctx);
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
        self.wrap_io_result(&result_ty, ok_payload, &io_err, span, fctx)
    }

    pub(super) fn gen_io_tcp_close_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_io_tcp_close_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_io_tcp_close_intrinsic expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let net_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_close(i64 {})",
            net_err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let io_err = self.map_net_err_to_io_code(&net_err, fctx);
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
        self.wrap_io_result(&result_ty, ok_payload, &io_err, span, fctx)
    }

    pub(super) fn gen_io_string_void_call(
        &mut self,
        runtime_fn: &str,
        args: &[ir::Expr],
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&value, args[0].span, fctx)?;
        fctx.lines.push(format!(
            "  call void @{}(i8* {}, i64 {}, i64 {})",
            runtime_fn, ptr, len, cap
        ));
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    pub(super) fn gen_io_int_void_call(
        &mut self,
        runtime_fn: &str,
        args: &[ir::Expr],
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        fctx.lines.push(format!(
            "  call void @{}(i64 {})",
            runtime_fn,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    pub(super) fn gen_io_bool_void_call(
        &mut self,
        runtime_fn: &str,
        args: &[ir::Expr],
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Bool"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let bool_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = zext i1 {} to i64",
            bool_i64,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        fctx.lines
            .push(format!("  call void @{}(i64 {})", runtime_fn, bool_i64));
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    pub(super) fn gen_io_flush_call(
        &mut self,
        runtime_fn: &str,
        args: &[ir::Expr],
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context} expects zero arguments"),
                self.file,
                span,
            ));
            return None;
        }
        fctx.lines.push(format!("  call void @{}()", runtime_fn));
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    pub(super) fn gen_time_builtin_call(
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
            "parse_rfc3339" | "aic_time_parse_rfc3339_intrinsic" => "parse_rfc3339",
            "parse_iso8601" | "aic_time_parse_iso8601_intrinsic" => "parse_iso8601",
            "format_rfc3339" | "aic_time_format_rfc3339_intrinsic" => "format_rfc3339",
            "format_iso8601" | "aic_time_format_iso8601_intrinsic" => "format_iso8601",
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
            "parse_rfc3339"
                if self.sig_matches_shape(name, &["String"], "Result[DateTime, TimeError]") =>
            {
                Some(self.gen_time_parse_call(name, "aic_rt_time_parse_rfc3339", args, span, fctx))
            }
            "parse_iso8601"
                if self.sig_matches_shape(name, &["String"], "Result[DateTime, TimeError]") =>
            {
                Some(self.gen_time_parse_call(name, "aic_rt_time_parse_iso8601", args, span, fctx))
            }
            "format_rfc3339"
                if self.sig_matches_shape(name, &["DateTime"], "Result[String, TimeError]") =>
            {
                Some(self.gen_time_format_call(
                    name,
                    "aic_rt_time_format_rfc3339",
                    args,
                    span,
                    fctx,
                ))
            }
            "format_iso8601"
                if self.sig_matches_shape(name, &["DateTime"], "Result[String, TimeError]") =>
            {
                Some(self.gen_time_format_call(
                    name,
                    "aic_rt_time_format_iso8601",
                    args,
                    span,
                    fctx,
                ))
            }
            _ => None,
        }
    }

    pub(super) fn gen_rand_builtin_call(
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

    pub(super) fn gen_signal_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "register" | "aic_signal_register_intrinsic" => "register",
            "wait_for_signal" | "aic_signal_wait_intrinsic" => "wait_for_signal",
            _ => return None,
        };

        match canonical {
            "register" if self.sig_matches_shape(name, &["Int"], "Result[Bool, SignalError]") => {
                Some(self.gen_signal_register_call(name, args, span, fctx))
            }
            "wait_for_signal" if self.sig_matches_shape(name, &[], "Result[Int, SignalError]") => {
                Some(self.gen_signal_wait_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_signal_register_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "signal.register expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let signal_code = self.gen_expr(&args[0], fctx)?;
        if signal_code.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "signal.register expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_signal_register(i64 {})",
            err,
            signal_code.repr.clone().unwrap_or_else(|| "0".to_string())
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
        self.wrap_signal_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_signal_wait_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "signal.wait_for_signal expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let signal_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", signal_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_signal_wait(i64* {})",
            err, signal_slot
        ));
        let signal_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            signal_code, signal_slot
        ));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(signal_code),
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
        self.wrap_signal_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_time_now_call(
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

    pub(super) fn gen_time_sleep_call(
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

    pub(super) fn gen_time_parse_call(
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
        let text = self.gen_expr(&args[0], fctx)?;
        if text.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&text, args[0].span, fctx)?;
        let year_slot = self.new_temp();
        let month_slot = self.new_temp();
        let day_slot = self.new_temp();
        let hour_slot = self.new_temp();
        let minute_slot = self.new_temp();
        let second_slot = self.new_temp();
        let millis_slot = self.new_temp();
        let offset_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", year_slot));
        fctx.lines.push(format!("  {} = alloca i64", month_slot));
        fctx.lines.push(format!("  {} = alloca i64", day_slot));
        fctx.lines.push(format!("  {} = alloca i64", hour_slot));
        fctx.lines.push(format!("  {} = alloca i64", minute_slot));
        fctx.lines.push(format!("  {} = alloca i64", second_slot));
        fctx.lines.push(format!("  {} = alloca i64", millis_slot));
        fctx.lines.push(format!("  {} = alloca i64", offset_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i64* {}, i64* {}, i64* {}, i64* {}, i64* {}, i64* {}, i64* {}, i64* {})",
            err,
            runtime_fn,
            ptr,
            len,
            cap,
            year_slot,
            month_slot,
            day_slot,
            hour_slot,
            minute_slot,
            second_slot,
            millis_slot,
            offset_slot
        ));

        let year = self.new_temp();
        let month = self.new_temp();
        let day = self.new_temp();
        let hour = self.new_temp();
        let minute = self.new_temp();
        let second = self.new_temp();
        let millis = self.new_temp();
        let offset = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", year, year_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", month, month_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", day, day_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", hour, hour_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", minute, minute_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", second, second_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", millis, millis_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", offset, offset_slot));

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

        let ok_payload = self.build_datetime_struct_value(
            &ok_ty,
            Value {
                ty: LType::Int,
                repr: Some(year),
            },
            Value {
                ty: LType::Int,
                repr: Some(month),
            },
            Value {
                ty: LType::Int,
                repr: Some(day),
            },
            Value {
                ty: LType::Int,
                repr: Some(hour),
            },
            Value {
                ty: LType::Int,
                repr: Some(minute),
            },
            Value {
                ty: LType::Int,
                repr: Some(second),
            },
            Value {
                ty: LType::Int,
                repr: Some(millis),
            },
            Value {
                ty: LType::Int,
                repr: Some(offset),
            },
            span,
            fctx,
        )?;
        self.wrap_time_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_time_format_call(
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
        let datetime = self.gen_expr(&args[0], fctx)?;
        let (year, month, day, hour, minute, second, millis, offset) =
            self.datetime_parts(&datetime, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64 {}, i64 {}, i64 {}, i64 {}, i64 {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            runtime_fn,
            year.repr.clone().unwrap_or_else(|| "0".to_string()),
            month.repr.clone().unwrap_or_else(|| "0".to_string()),
            day.repr.clone().unwrap_or_else(|| "0".to_string()),
            hour.repr.clone().unwrap_or_else(|| "0".to_string()),
            minute.repr.clone().unwrap_or_else(|| "0".to_string()),
            second.repr.clone().unwrap_or_else(|| "0".to_string()),
            millis.repr.clone().unwrap_or_else(|| "0".to_string()),
            offset.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
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
        self.wrap_time_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn build_datetime_struct_value(
        &mut self,
        datetime_ty: &LType,
        year: Value,
        month: Value,
        day: Value,
        hour: Value,
        minute: Value,
        second: Value,
        millisecond: Value,
        offset_minutes: Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Struct(layout) = datetime_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "expected DateTime struct, found '{}'",
                    render_type(datetime_ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "DateTime" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("expected DateTime struct, found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }
        let mut ordered = Vec::new();
        for field in &layout.fields {
            let value = match field.name.as_str() {
                "year" => year.clone(),
                "month" => month.clone(),
                "day" => day.clone(),
                "hour" => hour.clone(),
                "minute" => minute.clone(),
                "second" => second.clone(),
                "millisecond" => millisecond.clone(),
                "offset_minutes" => offset_minutes.clone(),
                _ => {
                    self.diagnostics.push(Diagnostic::error(
                        "E5011",
                        format!("DateTime contains unsupported field '{}'", field.name),
                        self.file,
                        span,
                    ));
                    return None;
                }
            };
            ordered.push(value);
        }
        self.build_struct_value(layout, &ordered, span, fctx)
    }

    pub(super) fn datetime_parts(
        &mut self,
        datetime: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(Value, Value, Value, Value, Value, Value, Value, Value)> {
        let LType::Struct(layout) = &datetime.ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "expected DateTime struct value",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "DateTime" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("expected DateTime struct value, found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }
        let datetime_repr = datetime
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&datetime.ty));
        let datetime_llvm_ty = llvm_type(&datetime.ty);
        let mut year = None;
        let mut month = None;
        let mut day = None;
        let mut hour = None;
        let mut minute = None;
        let mut second = None;
        let mut millisecond = None;
        let mut offset_minutes = None;
        for (index, field) in layout.fields.iter().enumerate() {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, {}",
                reg, datetime_llvm_ty, datetime_repr, index
            ));
            let value = Value {
                ty: field.ty.clone(),
                repr: Some(reg),
            };
            match field.name.as_str() {
                "year" => year = Some(value),
                "month" => month = Some(value),
                "day" => day = Some(value),
                "hour" => hour = Some(value),
                "minute" => minute = Some(value),
                "second" => second = Some(value),
                "millisecond" => millisecond = Some(value),
                "offset_minutes" => offset_minutes = Some(value),
                _ => {}
            }
        }
        let Some(year) = year else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "DateTime is missing year field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(month) = month else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "DateTime is missing month field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(day) = day else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "DateTime is missing day field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(hour) = hour else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "DateTime is missing hour field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(minute) = minute else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "DateTime is missing minute field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(second) = second else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "DateTime is missing second field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(millisecond) = millisecond else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "DateTime is missing millisecond field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(offset_minutes) = offset_minutes else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "DateTime is missing offset_minutes field",
                self.file,
                span,
            ));
            return None;
        };
        if year.ty != LType::Int
            || month.ty != LType::Int
            || day.ty != LType::Int
            || hour.ty != LType::Int
            || minute.ty != LType::Int
            || second.ty != LType::Int
            || millisecond.ty != LType::Int
            || offset_minutes.ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "DateTime fields must all be Int",
                self.file,
                span,
            ));
            return None;
        }
        Some((
            year,
            month,
            day,
            hour,
            minute,
            second,
            millisecond,
            offset_minutes,
        ))
    }

    pub(super) fn wrap_io_result(
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
                    "io builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_io_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("io_ok");
        let err_label = self.new_label("io_err");
        let cont_label = self.new_label("io_cont");
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

    pub(super) fn wrap_time_result(
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
                    "time builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_time_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("time_ok");
        let err_label = self.new_label("time_err");
        let cont_label = self.new_label("time_cont");
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

    pub(super) fn gen_rand_seed_call(
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

    pub(super) fn gen_rand_next_call(
        &mut self,
        _span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Value {
        let reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = call i64 @aic_rt_rand_next()", reg));
        Value {
            ty: LType::Int,
            repr: Some(reg),
        }
    }

    pub(super) fn gen_rand_range_call(
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
}
