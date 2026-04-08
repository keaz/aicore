use super::*;

impl<'a> Generator<'a> {
    pub(super) fn gen_fs_builtin_call(
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
            "read_bytes" | "aic_fs_read_bytes_intrinsic" => "read_bytes",
            "write_bytes" | "aic_fs_write_bytes_intrinsic" => "write_bytes",
            "append_bytes" | "aic_fs_append_bytes_intrinsic" => "append_bytes",
            "open_read" | "aic_fs_open_read_intrinsic" => "open_read",
            "open_write" | "aic_fs_open_write_intrinsic" => "open_write",
            "open_append" | "aic_fs_open_append_intrinsic" => "open_append",
            "file_read_line" | "aic_fs_file_read_line_intrinsic" => "file_read_line",
            "file_write_str" | "aic_fs_file_write_str_intrinsic" => "file_write_str",
            "file_close" | "aic_fs_file_close_intrinsic" => "file_close",
            "mkdir" | "aic_fs_mkdir_intrinsic" => "mkdir",
            "mkdir_all" | "aic_fs_mkdir_all_intrinsic" => "mkdir_all",
            "rmdir" | "aic_fs_rmdir_intrinsic" => "rmdir",
            "list_dir" | "aic_fs_list_dir_intrinsic" => "list_dir",
            "create_symlink" | "aic_fs_create_symlink_intrinsic" => "create_symlink",
            "read_symlink" | "aic_fs_read_symlink_intrinsic" => "read_symlink",
            "set_readonly" | "aic_fs_set_readonly_intrinsic" => "set_readonly",
            "aic_fs_async_submit_allowed_intrinsic" => "async_submit_allowed",
            "async_shutdown" | "aic_fs_async_shutdown_intrinsic" => "async_shutdown",
            "async_runtime_pressure" | "aic_fs_async_pressure_intrinsic" => {
                "async_runtime_pressure"
            }
            _ => return None,
        };

        match canonical {
            "exists" if self.sig_matches_shape(name, &["String"], "Bool") => {
                Some(self.gen_fs_exists_call(args, span, fctx))
            }
            "read_text" if self.sig_matches_shape(name, &["String"], "Result[String, FsError]") => {
                Some(self.gen_fs_string_result_call(name, "aic_rt_fs_read_text", args, span, fctx))
            }
            "read_bytes" if self.sig_matches_shape(name, &["String"], "Result[Bytes, FsError]") => {
                Some(self.gen_fs_bytes_result_call(name, "aic_rt_fs_read_bytes", args, span, fctx))
            }
            "read_bytes"
                if self.sig_matches_shape(name, &["String"], "Result[String, FsError]") =>
            {
                Some(self.gen_fs_string_result_call(name, "aic_rt_fs_read_bytes", args, span, fctx))
            }
            "read_symlink"
                if self.sig_matches_shape(name, &["String"], "Result[String, FsError]") =>
            {
                Some(self.gen_fs_string_result_call(
                    name,
                    "aic_rt_fs_read_symlink",
                    args,
                    span,
                    fctx,
                ))
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
            "write_bytes"
                if self.sig_matches_shape(name, &["String", "Bytes"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_write_bytes_like_call(
                    name,
                    "aic_rt_fs_write_bytes",
                    args,
                    span,
                    fctx,
                ))
            }
            "write_bytes"
                if self.sig_matches_shape(name, &["String", "String"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_write_like_call(name, "aic_rt_fs_write_bytes", args, span, fctx))
            }
            "append_bytes"
                if self.sig_matches_shape(name, &["String", "Bytes"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_write_bytes_like_call(
                    name,
                    "aic_rt_fs_append_bytes",
                    args,
                    span,
                    fctx,
                ))
            }
            "append_bytes"
                if self.sig_matches_shape(name, &["String", "String"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_write_like_call(name, "aic_rt_fs_append_bytes", args, span, fctx))
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
            "create_symlink"
                if self.sig_matches_shape(name, &["String", "String"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_write_like_call(
                    name,
                    "aic_rt_fs_create_symlink",
                    args,
                    span,
                    fctx,
                ))
            }
            "delete" if self.sig_matches_shape(name, &["String"], "Result[Bool, FsError]") => {
                Some(self.gen_fs_path_bool_result_call(
                    name,
                    "aic_rt_fs_delete",
                    "delete",
                    args,
                    span,
                    fctx,
                ))
            }
            "mkdir" if self.sig_matches_shape(name, &["String"], "Result[Bool, FsError]") => {
                Some(self.gen_fs_path_bool_result_call(
                    name,
                    "aic_rt_fs_mkdir",
                    "mkdir",
                    args,
                    span,
                    fctx,
                ))
            }
            "mkdir_all" if self.sig_matches_shape(name, &["String"], "Result[Bool, FsError]") => {
                Some(self.gen_fs_path_bool_result_call(
                    name,
                    "aic_rt_fs_mkdir_all",
                    "mkdir_all",
                    args,
                    span,
                    fctx,
                ))
            }
            "rmdir" if self.sig_matches_shape(name, &["String"], "Result[Bool, FsError]") => {
                Some(self.gen_fs_path_bool_result_call(
                    name,
                    "aic_rt_fs_rmdir",
                    "rmdir",
                    args,
                    span,
                    fctx,
                ))
            }
            "set_readonly"
                if self.sig_matches_shape(name, &["String", "Bool"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_set_readonly_call(name, args, span, fctx))
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
            "list_dir"
                if self.sig_matches_shape(name, &["String"], "Result[Vec[String], FsError]") =>
            {
                Some(self.gen_fs_list_dir_call(name, args, span, fctx))
            }
            "open_read"
                if self.sig_matches_shape(name, &["String"], "Result[FileHandle, FsError]") =>
            {
                Some(self.gen_fs_open_file_call(
                    name,
                    "aic_rt_fs_open_read",
                    "open_read",
                    args,
                    span,
                    fctx,
                ))
            }
            "open_write"
                if self.sig_matches_shape(name, &["String"], "Result[FileHandle, FsError]") =>
            {
                Some(self.gen_fs_open_file_call(
                    name,
                    "aic_rt_fs_open_write",
                    "open_write",
                    args,
                    span,
                    fctx,
                ))
            }
            "open_append"
                if self.sig_matches_shape(name, &["String"], "Result[FileHandle, FsError]") =>
            {
                Some(self.gen_fs_open_file_call(
                    name,
                    "aic_rt_fs_open_append",
                    "open_append",
                    args,
                    span,
                    fctx,
                ))
            }
            "file_read_line"
                if self.sig_matches_shape(
                    name,
                    &["FileHandle"],
                    "Result[Option[String], FsError]",
                ) =>
            {
                Some(self.gen_fs_file_read_line_call(name, args, span, fctx))
            }
            "file_write_str"
                if self.sig_matches_shape(
                    name,
                    &["FileHandle", "String"],
                    "Result[Bool, FsError]",
                ) =>
            {
                Some(self.gen_fs_file_write_str_call(name, args, span, fctx))
            }
            "file_close"
                if self.sig_matches_shape(name, &["FileHandle"], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_file_close_call(name, args, span, fctx))
            }
            "async_submit_allowed"
                if self.sig_matches_shape(name, &[], "Result[Bool, FsError]") =>
            {
                Some(self.gen_fs_async_bool_result_noarg_call(
                    name,
                    "aic_rt_fs_async_submit_allowed",
                    "async_submit_allowed",
                    args,
                    span,
                    fctx,
                ))
            }
            "async_shutdown" if self.sig_matches_shape(name, &[], "Result[Bool, FsError]") => {
                Some(self.gen_fs_async_bool_result_noarg_call(
                    name,
                    "aic_rt_fs_async_shutdown",
                    "async_shutdown",
                    args,
                    span,
                    fctx,
                ))
            }
            "async_runtime_pressure"
                if self.sig_matches_shape(name, &[], "Result[FsAsyncRuntimePressure, FsError]") =>
            {
                Some(self.gen_fs_async_pressure_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_fs_exists_call(
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

    pub(super) fn gen_fs_string_result_call(
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_fs_bytes_result_call(
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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
        let data_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let ok_payload = if ok_ty == LType::String {
            data_value
        } else {
            self.build_bytes_value_from_data(&ok_ty, data_value, name, span, fctx)?
        };
        self.wrap_fs_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_fs_write_like_call(
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_fs_write_bytes_like_call(
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
        if lhs.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects (String, Bytes)"),
                self.file,
                span,
            ));
            return None;
        }
        let (lhs_ptr, lhs_len, lhs_cap) = self.string_parts(&lhs, args[0].span, fctx)?;
        let (rhs_ptr, rhs_len, rhs_cap) = if rhs.ty == LType::String {
            self.string_parts(&rhs, args[1].span, fctx)?
        } else {
            self.bytes_parts(&rhs, name, args[1].span, fctx)?
        };
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
            err, runtime_fn, lhs_ptr, lhs_len, lhs_cap, rhs_ptr, rhs_len, rhs_cap
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_fs_path_bool_result_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        op_name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{op_name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let path = self.gen_expr(&args[0], fctx)?;
        if path.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{op_name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&path, args[0].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {})",
            err, runtime_fn, ptr, len, cap
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_fs_open_file_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        op_name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{op_name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let path = self.gen_expr(&args[0], fctx)?;
        if path.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{op_name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&path, args[0].span, fctx)?;
        let out_handle_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i64* {})",
            err, runtime_fn, ptr, len, cap, out_handle_slot
        ));
        let out_handle = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_handle, out_handle_slot
        ));

        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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
                format!("{op_name} expects Result[FileHandle, FsError] return type"),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&ok_layout.repr) != "FileHandle"
            || ok_layout.fields.len() != 1
            || ok_layout.fields[0].name != "handle"
            || ok_layout.fields[0].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{op_name} expects Result[FileHandle, FsError] return type"),
                self.file,
                span,
            ));
            return None;
        }
        let ok_payload = self.build_struct_value(
            &ok_layout,
            &[Value {
                ty: LType::Int,
                repr: Some(out_handle),
            }],
            span,
            fctx,
        )?;
        self.wrap_fs_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_fs_file_write_str_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "file_write_str expects two arguments",
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
            "file_write_str",
            args[0].span,
            fctx,
        )?;
        if content.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "file_write_str expects (FileHandle, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&content, args[1].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_file_write_str(i64 {}, i8* {}, i64 {}, i64 {})",
            err, handle, ptr, len, cap
        ));

        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_fs_file_close_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "file_close expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let file = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &file,
            "FileHandle",
            "file_close",
            args[0].span,
            fctx,
        )?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_file_close(i64 {})",
            err, handle
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_fs_file_read_line_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "file_read_line expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let file = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &file,
            "FileHandle",
            "file_read_line",
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
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_file_read_line(i64 {}, i8** {}, i64* {}, i64* {})",
            err, handle, out_ptr_slot, out_len_slot, out_has_line_slot
        ));
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

        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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
        self.wrap_fs_result(&result_ty, option_value, &err, span, fctx)
    }

    pub(super) fn gen_fs_set_readonly_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "set_readonly expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let path = self.gen_expr(&args[0], fctx)?;
        let readonly = self.gen_expr(&args[1], fctx)?;
        if path.ty != LType::String || readonly.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "set_readonly expects (String, Bool)",
                self.file,
                span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&path, args[0].span, fctx)?;
        let readonly_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = zext i1 {} to i64",
            readonly_i64,
            readonly.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_set_readonly(i8* {}, i64 {}, i64 {}, i64 {})",
            err, ptr, len, cap, readonly_i64
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_fs_metadata_call(
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

        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_fs_walk_dir_call(
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

        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_fs_list_dir_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "list_dir expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let path = self.gen_expr(&args[0], fctx)?;
        if path.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "list_dir expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&path, args[0].span, fctx)?;
        let out_items_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_items_slot));
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_list_dir(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err, ptr, len, cap, out_items_slot, out_count_slot
        ));
        let out_items = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_items, out_items_slot
        ));
        let out_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_count, out_count_slot
        ));
        let ok_payload =
            self.build_vec_string_payload_from_ptr(&out_items, &out_count, span, fctx)?;
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_fs_async_bool_result_noarg_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        op_name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{op_name} expects zero arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let out_bool_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_bool_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64* {})",
            err, runtime_fn, out_bool_slot
        ));
        let out_bool_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_bool_raw, out_bool_slot
        ));
        let out_bool = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", out_bool, out_bool_raw));
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(&result_ty, span)
        else {
            return None;
        };
        if ok_ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "{op_name} expects Result[Bool, FsError], found '{}'",
                    render_type(&result_ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let ok_value = self.build_enum_variant(
            &layout,
            ok_index,
            Some(Value {
                ty: LType::Bool,
                repr: Some(out_bool),
            }),
            span,
            fctx,
        )?;
        let err_payload = self.build_fs_error_from_concurrency_code(&err_ty, &err, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(&result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err));
        let ok_label = self.new_label("fs_async_ok");
        let err_label = self.new_label("fs_async_err");
        let cont_label = self.new_label("fs_async_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&result_ty),
            ok_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&result_ty)),
            llvm_type(&result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", err_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&result_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&result_ty)),
            llvm_type(&result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
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
    }

    pub(super) fn gen_fs_async_pressure_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "async_runtime_pressure expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let active_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", active_slot));
        let queue_depth_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", queue_depth_slot));
        let op_limit_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", op_limit_slot));
        let queue_limit_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", queue_limit_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_fs_async_pressure(i64* {}, i64* {}, i64* {}, i64* {})",
            err, active_slot, queue_depth_slot, op_limit_slot, queue_limit_slot
        ));
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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
                "async_runtime_pressure expects Result[FsAsyncRuntimePressure, FsError] return type",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&ok_layout.repr) != "FsAsyncRuntimePressure"
            || ok_layout.fields.len() != 4
            || ok_layout.fields.iter().any(|field| field.ty != LType::Int)
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "async_runtime_pressure expects Result[FsAsyncRuntimePressure, FsError] return type",
                self.file,
                span,
            ));
            return None;
        }
        let active_ops = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", active_ops, active_slot));
        let queue_depth = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            queue_depth, queue_depth_slot
        ));
        let op_limit = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", op_limit, op_limit_slot));
        let queue_limit = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            queue_limit, queue_limit_slot
        ));
        let ok_payload = self.build_struct_value(
            &ok_layout,
            &[
                Value {
                    ty: LType::Int,
                    repr: Some(active_ops),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(queue_depth),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(op_limit),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(queue_limit),
                },
            ],
            span,
            fctx,
        )?;
        self.wrap_fs_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn wrap_fs_result(
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

    pub(super) fn gen_env_builtin_call(
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
            "args" | "aic_env_args_intrinsic" => "args",
            "arg_count" | "aic_env_arg_count_intrinsic" => "arg_count",
            "arg_at" | "aic_env_arg_at_intrinsic" => "arg_at",
            "exit" | "aic_env_exit_intrinsic" => "exit",
            "all_vars" | "aic_env_all_vars_intrinsic" => "all_vars",
            "home_dir" | "aic_env_home_dir_intrinsic" => "home_dir",
            "temp_dir" | "aic_env_temp_dir_intrinsic" => "temp_dir",
            "os_name" | "aic_env_os_name_intrinsic" => "os_name",
            "arch" | "aic_env_arch_intrinsic" => "arch",
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
            "args" if self.sig_matches_shape(name, &[], "Vec[String]") => {
                Some(self.gen_env_args_call(name, args, span, fctx))
            }
            "arg_count" if self.sig_matches_shape(name, &[], "Int") => {
                Some(self.gen_env_arg_count_call(args, span, fctx))
            }
            "arg_at" if self.sig_matches_shape(name, &["Int"], "Option[String]") => {
                Some(self.gen_env_arg_at_call(name, args, span, fctx))
            }
            "exit" if self.sig_matches_shape(name, &["Int"], "()") => {
                Some(self.gen_env_exit_call(name, args, span, fctx))
            }
            "all_vars" if self.sig_matches_shape(name, &[], "Vec[EnvEntry]") => {
                Some(self.gen_env_all_vars_call(name, args, span, fctx))
            }
            "home_dir" if self.sig_matches_shape(name, &[], "Result[String, EnvError]") => {
                Some(self.gen_env_string_result_noarg_call(
                    name,
                    "aic_rt_env_home_dir",
                    args,
                    "home_dir",
                    span,
                    fctx,
                ))
            }
            "temp_dir" if self.sig_matches_shape(name, &[], "Result[String, EnvError]") => {
                Some(self.gen_env_string_result_noarg_call(
                    name,
                    "aic_rt_env_temp_dir",
                    args,
                    "temp_dir",
                    span,
                    fctx,
                ))
            }
            "os_name" if self.sig_matches_shape(name, &[], "String") => Some(
                self.gen_env_string_noarg_call("aic_rt_env_os_name", args, "os_name", span, fctx),
            ),
            "arch" if self.sig_matches_shape(name, &[], "String") => {
                Some(self.gen_env_string_noarg_call("aic_rt_env_arch", args, "arch", span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_env_get_call(
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_env_set_call(
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_env_remove_call(
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_env_cwd_call(
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_env_set_cwd_call(
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
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

    pub(super) fn gen_env_args_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "args expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let out_items_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_items_slot));
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_env_args(i8** {}, i64* {})",
            out_items_slot, out_count_slot
        ));
        let out_items = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_items, out_items_slot
        ));
        let out_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_count, out_count_slot
        ));
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.build_vec_string_from_raw_parts(&result_ty, &out_items, &out_count, span, fctx)
    }

    pub(super) fn gen_env_arg_count_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "arg_count expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let count = self.new_temp();
        fctx.lines
            .push(format!("  {} = call i64 @aic_rt_env_arg_count()", count));
        Some(Value {
            ty: LType::Int,
            repr: Some(count),
        })
    }

    pub(super) fn gen_env_arg_at_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "arg_at expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let index = self.gen_expr(&args[0], fctx)?;
        if index.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "arg_at expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_env_arg_at(i64 {}, i8** {}, i64* {})",
            found,
            index.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let found_bool = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", found_bool, found));
        let payload = self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_option_with_condition(&result_ty, payload, &found_bool, span, fctx)
    }

    pub(super) fn gen_env_exit_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "exit expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let code = self.gen_expr(&args[0], fctx)?;
        if code.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "exit expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        if !self.sig_matches_shape(name, &["Int"], "()") {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "exit expects signature '(Int) -> ()'",
                self.file,
                span,
            ));
            return None;
        }
        fctx.lines.push(format!(
            "  call void @aic_rt_exit(i64 {})",
            code.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    pub(super) fn gen_env_all_vars_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "all_vars expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let out_items_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_items_slot));
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_env_all_vars(i8** {}, i64* {})",
            out_items_slot, out_count_slot
        ));
        let out_items = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_items, out_items_slot
        ));
        let out_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_count, out_count_slot
        ));
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.build_vec_value_from_raw_i8_ptr(&result_ty, &out_items, &out_count, span, fctx)
    }

    pub(super) fn gen_env_string_result_noarg_call(
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
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8** {}, i64* {})",
            err, runtime_fn, out_ptr_slot, out_len_slot
        ));
        let payload = self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_env_result(&result_ty, payload, &err, span, fctx)
    }

    pub(super) fn gen_env_string_noarg_call(
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
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @{}(i8** {}, i64* {})",
            runtime_fn, out_ptr_slot, out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn wrap_env_result(
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
}
