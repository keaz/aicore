use super::*;

impl<'a> Generator<'a> {
    pub(super) fn gen_concurrency_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "spawn_task" | "aic_conc_spawn_intrinsic" => "spawn_task",
            "join_task" | "aic_conc_join_intrinsic" => "join_task",
            "timeout_task" | "aic_conc_join_timeout_intrinsic" => "timeout_task",
            "cancel_task" | "aic_conc_cancel_intrinsic" => "cancel_task",
            "aic_conc_spawn_fn_intrinsic" => "spawn_fn",
            "aic_conc_spawn_fn_named_intrinsic" => "spawn_fn_named",
            "aic_conc_join_value_intrinsic" => "join_value",
            "aic_conc_scope_new_intrinsic" => "scope_new",
            "aic_conc_scope_spawn_fn_intrinsic" => "scope_spawn_fn",
            "aic_conc_scope_join_all_intrinsic" => "scope_join_all",
            "aic_conc_scope_cancel_intrinsic" => "scope_cancel",
            "aic_conc_scope_close_intrinsic" => "scope_close",
            "spawn_group" | "aic_conc_spawn_group_intrinsic" => "spawn_group",
            "select_first" | "aic_conc_select_first_intrinsic" => "select_first",
            "channel_int" | "aic_conc_channel_int_intrinsic" => "channel_int",
            "buffered_channel_int"
            | "channel_int_buffered"
            | "aic_conc_channel_int_buffered_intrinsic" => "channel_int_buffered",
            "send_int" | "aic_conc_send_int_intrinsic" => "send_int",
            "try_send_int" | "aic_conc_try_send_int_intrinsic" => "try_send_int",
            "recv_int" | "aic_conc_recv_int_intrinsic" => "recv_int",
            "try_recv_int" | "aic_conc_try_recv_int_intrinsic" => "try_recv_int",
            "select_recv_int" | "aic_conc_select_recv_int_intrinsic" => "select_recv_int",
            "close_channel" | "aic_conc_close_channel_intrinsic" => "close_channel",
            "mutex_int" | "aic_conc_mutex_int_intrinsic" => "mutex_int",
            "lock_int" | "aic_conc_mutex_lock_intrinsic" => "lock_int",
            "unlock_int" | "aic_conc_mutex_unlock_intrinsic" => "unlock_int",
            "close_mutex" | "aic_conc_mutex_close_intrinsic" => "close_mutex",
            "rwlock_int" | "aic_conc_rwlock_int_intrinsic" => "rwlock_int",
            "read_lock_int" | "aic_conc_rwlock_read_intrinsic" => "read_lock_int",
            "write_lock_int" | "aic_conc_rwlock_write_lock_intrinsic" => "write_lock_int",
            "write_unlock_int" | "aic_conc_rwlock_write_unlock_intrinsic" => "write_unlock_int",
            "close_rwlock" | "aic_conc_rwlock_close_intrinsic" => "close_rwlock",
            "aic_conc_payload_store_intrinsic" => "payload_store",
            "aic_conc_payload_take_intrinsic" => "payload_take",
            "aic_conc_payload_drop_intrinsic" => "payload_drop",
            "aic_conc_arc_new_intrinsic" => "arc_new",
            "aic_conc_arc_clone_intrinsic" => "arc_clone",
            "aic_conc_arc_get_intrinsic" => "arc_get",
            "aic_conc_arc_strong_count_intrinsic" => "arc_strong_count",
            "aic_conc_atomic_int_intrinsic" => "atomic_int",
            "aic_conc_atomic_load_intrinsic" => "atomic_load",
            "aic_conc_atomic_store_intrinsic" => "atomic_store",
            "aic_conc_atomic_add_intrinsic" => "atomic_add",
            "aic_conc_atomic_sub_intrinsic" => "atomic_sub",
            "aic_conc_atomic_cas_intrinsic" => "atomic_cas",
            "aic_conc_atomic_bool_intrinsic" => "atomic_bool",
            "aic_conc_atomic_load_bool_intrinsic" => "atomic_load_bool",
            "aic_conc_atomic_store_bool_intrinsic" => "atomic_store_bool",
            "aic_conc_atomic_swap_bool_intrinsic" => "atomic_swap_bool",
            "aic_conc_tl_new_intrinsic" => "thread_local_new",
            "aic_conc_tl_get_intrinsic" => "thread_local_get",
            "aic_conc_tl_set_intrinsic" => "thread_local_set",
            _ => return None,
        };

        match canonical {
            "spawn_task"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int"],
                    "Result[Task[Int], ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_spawn_task_call(name, args, span, fctx))
            }
            "join_task"
                if self.sig_matches_shape(
                    name,
                    &["Task[Int]"],
                    "Result[Int, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_join_task_call(name, args, span, fctx))
            }
            "timeout_task"
                if self.sig_matches_shape(
                    name,
                    &["Task[Int]", "Int"],
                    "Result[Int, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_timeout_task_call(name, args, span, fctx))
            }
            "cancel_task"
                if self.sig_matches_shape(
                    name,
                    &["Task[Int]"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_cancel_task_call(name, args, span, fctx))
            }
            "spawn_fn" => Some(self.gen_concurrency_spawn_fn_call(name, args, span, fctx)),
            "spawn_fn_named" => {
                Some(self.gen_concurrency_spawn_fn_named_call(name, args, span, fctx))
            }
            "join_value" => Some(self.gen_concurrency_join_value_call(name, args, span, fctx)),
            "scope_new" if self.sig_matches_shape(name, &[], "Result[Scope, ConcurrencyError]") => {
                Some(self.gen_concurrency_scope_new_call(name, args, span, fctx))
            }
            "scope_spawn_fn" => {
                Some(self.gen_concurrency_scope_spawn_fn_call(name, args, span, fctx))
            }
            "scope_join_all"
                if self.sig_matches_shape(name, &["Scope"], "Result[Bool, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_scope_join_all_call(name, args, span, fctx))
            }
            "scope_cancel"
                if self.sig_matches_shape(name, &["Scope"], "Result[Bool, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_scope_cancel_call(name, args, span, fctx))
            }
            "scope_close"
                if self.sig_matches_shape(name, &["Scope"], "Result[Bool, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_scope_close_call(name, args, span, fctx))
            }
            "spawn_group"
                if self.sig_matches_shape(
                    name,
                    &["Vec[Int]", "Int"],
                    "Result[Vec[Int], ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_spawn_group_call(name, args, span, fctx))
            }
            "select_first"
                if self.sig_matches_shape(
                    name,
                    &["Vec[Task[Int]]", "Int"],
                    "Result[IntTaskSelection, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_select_first_call(name, args, span, fctx))
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
            "channel_int_buffered"
                if self.sig_matches_shape(
                    name,
                    &["Int"],
                    "Result[IntChannel, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_channel_int_buffered_call(name, args, span, fctx))
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
            "try_send_int"
                if self.sig_matches_shape(
                    name,
                    &["IntChannel", "Int"],
                    "Result[Bool, ChannelError]",
                ) =>
            {
                Some(self.gen_concurrency_try_send_int_call(name, args, span, fctx))
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
            "try_recv_int"
                if self.sig_matches_shape(name, &["IntChannel"], "Result[Int, ChannelError]") =>
            {
                Some(self.gen_concurrency_try_recv_int_call(name, args, span, fctx))
            }
            "select_recv_int"
                if self.sig_matches_shape(
                    name,
                    &["IntChannel", "IntChannel", "Int"],
                    "Result[IntChannelSelection, ChannelError]",
                ) =>
            {
                Some(self.gen_concurrency_select_recv_int_call(name, args, span, fctx))
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
            "rwlock_int"
                if self.sig_matches_shape(
                    name,
                    &["Int"],
                    "Result[IntRwLock, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_rwlock_int_call(name, args, span, fctx))
            }
            "read_lock_int"
                if self.sig_matches_shape(
                    name,
                    &["IntRwLock", "Int"],
                    "Result[Int, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_rwlock_read_call(name, args, span, fctx))
            }
            "write_lock_int"
                if self.sig_matches_shape(
                    name,
                    &["IntRwLock", "Int"],
                    "Result[Int, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_rwlock_write_lock_call(name, args, span, fctx))
            }
            "write_unlock_int"
                if self.sig_matches_shape(
                    name,
                    &["IntRwLock", "Int"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_rwlock_write_unlock_call(name, args, span, fctx))
            }
            "close_rwlock"
                if self.sig_matches_shape(
                    name,
                    &["IntRwLock"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_close_rwlock_call(name, args, span, fctx))
            }
            "payload_store"
                if self.sig_matches_shape(name, &["String"], "Result[Int, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_payload_store_call(name, args, span, fctx))
            }
            "payload_take"
                if self.sig_matches_shape(name, &["Int"], "Result[String, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_payload_take_call(name, args, span, fctx))
            }
            "payload_drop"
                if self.sig_matches_shape(name, &["Int"], "Result[Bool, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_payload_drop_call(name, args, span, fctx))
            }
            "arc_new"
                if self.sig_matches_shape(name, &["String"], "Result[Int, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_arc_new_call(name, args, span, fctx))
            }
            "arc_clone"
                if self.sig_matches_shape(name, &["Int"], "Result[Int, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_arc_clone_call(name, args, span, fctx))
            }
            "arc_get"
                if self.sig_matches_shape(name, &["Int"], "Result[String, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_arc_get_call(name, args, span, fctx))
            }
            "arc_strong_count"
                if self.sig_matches_shape(name, &["Int"], "Result[Int, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_arc_strong_count_call(name, args, span, fctx))
            }
            "atomic_int"
                if self.sig_matches_shape(name, &["Int"], "Result[Int, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_atomic_int_new_call(name, args, span, fctx))
            }
            "atomic_load"
                if self.sig_matches_shape(name, &["Int"], "Result[Int, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_atomic_int_load_call(name, args, span, fctx))
            }
            "atomic_store"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_atomic_int_store_call(name, args, span, fctx))
            }
            "atomic_add"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int"],
                    "Result[Int, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_atomic_int_add_call(name, args, span, fctx))
            }
            "atomic_sub"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int"],
                    "Result[Int, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_atomic_int_sub_call(name, args, span, fctx))
            }
            "atomic_cas"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_atomic_int_cas_call(name, args, span, fctx))
            }
            "atomic_bool"
                if self.sig_matches_shape(name, &["Bool"], "Result[Int, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_atomic_bool_new_call(name, args, span, fctx))
            }
            "atomic_load_bool"
                if self.sig_matches_shape(name, &["Int"], "Result[Bool, ConcurrencyError]") =>
            {
                Some(self.gen_concurrency_atomic_bool_load_call(name, args, span, fctx))
            }
            "atomic_store_bool"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Bool"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_atomic_bool_store_call(name, args, span, fctx))
            }
            "atomic_swap_bool"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Bool"],
                    "Result[Bool, ConcurrencyError]",
                ) =>
            {
                Some(self.gen_concurrency_atomic_bool_swap_call(name, args, span, fctx))
            }
            "thread_local_new" => {
                Some(self.gen_concurrency_thread_local_new_call(name, args, span, fctx))
            }
            "thread_local_get" => Some(self.gen_concurrency_thread_local_get_call(
                name,
                args,
                span,
                expected_ty,
                fctx,
            )),
            "thread_local_set" => {
                Some(self.gen_concurrency_thread_local_set_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn concurrency_result_ty(
        &mut self,
        name: &str,
        span: crate::span::Span,
    ) -> Option<LType> {
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

    pub(super) fn concurrency_spawn_fn_result_ty(
        &mut self,
        name: &str,
        return_ty: &LType,
        span: crate::span::Span,
    ) -> Option<LType> {
        if let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) {
            return Some(result_ty);
        }
        let rendered = render_type(return_ty);
        let repr = format!("Result[Task[{rendered}], ConcurrencyError]");
        self.parse_type_repr(&repr, span)
    }

    pub(super) fn concurrency_join_value_result_ty(
        &mut self,
        name: &str,
        task_ty: &LType,
        span: crate::span::Span,
    ) -> Option<LType> {
        if let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) {
            return Some(result_ty);
        }
        let LType::Struct(layout) = task_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "join expects Task[T]",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Task" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "join expects Task[T]",
                self.file,
                span,
            ));
            return None;
        }
        let task_args = extract_generic_args(&layout.repr).unwrap_or_default();
        if task_args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "join expects Task[T]",
                self.file,
                span,
            ));
            return None;
        }
        let repr = format!("Result[{}, ConcurrencyError]", task_args[0]);
        self.parse_type_repr(&repr, span)
    }

    pub(super) fn extract_named_handle_from_value(
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

    pub(super) fn build_concurrency_ok_handle_payload(
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

    pub(super) fn gen_concurrency_spawn_task_call(
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

    pub(super) fn emit_concurrency_spawn_entry_helper(
        &mut self,
        helper_name: &str,
        fn_layout: &FnLayoutType,
    ) {
        let fn_pair_ty = llvm_type(&LType::Fn(fn_layout.clone()));
        let ret_ty = llvm_type(&fn_layout.ret);
        let fn_sig_ty = format!("{} (i8*)*", ret_ty);
        let mut lines = vec![
            format!("define i64 @{}(i8* %ctx_raw) {{", helper_name),
            "entry:".to_string(),
            format!("  %ctx = bitcast i8* %ctx_raw to {}*", fn_pair_ty),
            format!("  %pair = load {}, {}* %ctx", fn_pair_ty, fn_pair_ty),
            format!("  %fn_ptr = extractvalue {} %pair, 0", fn_pair_ty),
            format!("  %fn_env = extractvalue {} %pair, 1", fn_pair_ty),
            format!("  %fn_typed = bitcast i8* %fn_ptr to {}", fn_sig_ty),
        ];
        if *fn_layout.ret == LType::Unit {
            lines.push("  call void %fn_typed(i8* %fn_env)".to_string());
            lines.push("  ret i64 0".to_string());
        } else {
            lines.push(format!(
                "  %ret_value = call {} %fn_typed(i8* %fn_env)",
                ret_ty
            ));
            lines.push(format!(
                "  %ret_size_ptr = getelementptr inbounds {}, {}* null, i32 1",
                ret_ty, ret_ty
            ));
            lines.push(format!(
                "  %ret_size = ptrtoint {}* %ret_size_ptr to i64",
                ret_ty
            ));
            lines.push("  %ret_heap = call i8* @malloc(i64 %ret_size)".to_string());
            lines.push(format!(
                "  %ret_heap_typed = bitcast i8* %ret_heap to {}*",
                ret_ty
            ));
            lines.push(format!(
                "  store {} %ret_value, {}* %ret_heap_typed",
                ret_ty, ret_ty
            ));
            lines.push("  %ret_raw = ptrtoint i8* %ret_heap to i64".to_string());
            lines.push("  ret i64 %ret_raw".to_string());
        }
        lines.push("}".to_string());
        self.deferred_fn_defs.push(lines);
    }

    pub(super) fn gen_concurrency_spawn_fn_call(
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

        let fn_value = self.gen_expr(&args[0], fctx)?;
        let LType::Fn(fn_layout) = fn_value.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "spawn_fn expects Fn() -> T",
                self.file,
                span,
            ));
            return None;
        };
        if !fn_layout.params.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "spawn_fn expects Fn() -> T",
                self.file,
                span,
            ));
            return None;
        }

        self.extern_decls
            .insert("declare i8* @malloc(i64)".to_string());

        let fn_pair_ty = llvm_type(&LType::Fn(fn_layout.clone()));
        let fn_repr = fn_value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&fn_value.ty));
        let pair_tmp = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca {}", pair_tmp, fn_pair_ty));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            fn_pair_ty, fn_repr, fn_pair_ty, pair_tmp
        ));
        let pair_size_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds {}, {}* null, i32 1",
            pair_size_ptr, fn_pair_ty, fn_pair_ty
        ));
        let pair_size = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint {}* {} to i64",
            pair_size, fn_pair_ty, pair_size_ptr
        ));
        let pair_heap = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i8* @malloc(i64 {})",
            pair_heap, pair_size
        ));
        let pair_heap_typed = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to {}*",
            pair_heap_typed, pair_heap, fn_pair_ty
        ));
        let pair_loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            pair_loaded, fn_pair_ty, fn_pair_ty, pair_tmp
        ));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            fn_pair_ty, pair_loaded, fn_pair_ty, pair_heap_typed
        ));

        let helper_name = format!("__aic_spawn_entry_{}", self.closure_counter);
        self.closure_counter += 1;
        self.emit_concurrency_spawn_entry_helper(&helper_name, &fn_layout);

        let entry_fn_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint i64 (i8*)* @{} to i64",
            entry_fn_raw, helper_name
        ));
        let entry_env_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint i8* {} to i64",
            entry_env_raw, pair_heap
        ));

        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_spawn_fn(i64 {}, i64 {}, i64* {})",
            err, entry_fn_raw, entry_env_raw, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.concurrency_spawn_fn_result_ty(name, fn_layout.ret.as_ref(), span)?;
        let ok_payload =
            self.build_concurrency_ok_handle_payload(&result_ty, "Task", &handle, span, fctx)?;
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_spawn_fn_named_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "spawn_named expects two arguments",
                self.file,
                span,
            ));
            return None;
        }

        let thread_name = self.gen_expr(&args[0], fctx)?;
        if thread_name.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "spawn_named expects (String, Fn() -> T)",
                self.file,
                span,
            ));
            return None;
        }
        let (name_ptr, name_len, name_cap) = self.string_parts(&thread_name, args[0].span, fctx)?;

        let fn_value = self.gen_expr(&args[1], fctx)?;
        let LType::Fn(fn_layout) = fn_value.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "spawn_named expects (String, Fn() -> T)",
                self.file,
                span,
            ));
            return None;
        };
        if !fn_layout.params.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "spawn_named expects (String, Fn() -> T)",
                self.file,
                span,
            ));
            return None;
        }

        self.extern_decls
            .insert("declare i8* @malloc(i64)".to_string());

        let fn_pair_ty = llvm_type(&LType::Fn(fn_layout.clone()));
        let fn_repr = fn_value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&fn_value.ty));
        let pair_tmp = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca {}", pair_tmp, fn_pair_ty));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            fn_pair_ty, fn_repr, fn_pair_ty, pair_tmp
        ));
        let pair_size_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds {}, {}* null, i32 1",
            pair_size_ptr, fn_pair_ty, fn_pair_ty
        ));
        let pair_size = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint {}* {} to i64",
            pair_size, fn_pair_ty, pair_size_ptr
        ));
        let pair_heap = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i8* @malloc(i64 {})",
            pair_heap, pair_size
        ));
        let pair_heap_typed = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to {}*",
            pair_heap_typed, pair_heap, fn_pair_ty
        ));
        let pair_loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            pair_loaded, fn_pair_ty, fn_pair_ty, pair_tmp
        ));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            fn_pair_ty, pair_loaded, fn_pair_ty, pair_heap_typed
        ));

        let helper_name = format!("__aic_spawn_entry_{}", self.closure_counter);
        self.closure_counter += 1;
        self.emit_concurrency_spawn_entry_helper(&helper_name, &fn_layout);

        let entry_fn_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint i64 (i8*)* @{} to i64",
            entry_fn_raw, helper_name
        ));
        let entry_env_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint i8* {} to i64",
            entry_env_raw, pair_heap
        ));

        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_spawn_fn_named(i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            err, entry_fn_raw, entry_env_raw, name_ptr, name_len, name_cap, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.concurrency_spawn_fn_result_ty(name, fn_layout.ret.as_ref(), span)?;
        let ok_payload =
            self.build_concurrency_ok_handle_payload(&result_ty, "Task", &handle, span, fctx)?;
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_scope_new_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "scope_new expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let scope_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", scope_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_scope_new(i64* {})",
            err, scope_slot
        ));
        let scope_handle = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            scope_handle, scope_slot
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = self.build_concurrency_ok_handle_payload(
            &result_ty,
            "Scope",
            &scope_handle,
            span,
            fctx,
        )?;
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_scope_spawn_fn_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "scope_spawn expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let scope_value = self.gen_expr(&args[0], fctx)?;
        let scope_handle = self.extract_named_handle_from_value(
            &scope_value,
            "Scope",
            "scope_spawn",
            args[0].span,
            fctx,
        )?;

        let fn_value = self.gen_expr(&args[1], fctx)?;
        let LType::Fn(fn_layout) = fn_value.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "scope_spawn expects (Scope, Fn() -> T)",
                self.file,
                span,
            ));
            return None;
        };
        if !fn_layout.params.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "scope_spawn expects (Scope, Fn() -> T)",
                self.file,
                span,
            ));
            return None;
        }

        self.extern_decls
            .insert("declare i8* @malloc(i64)".to_string());

        let fn_pair_ty = llvm_type(&LType::Fn(fn_layout.clone()));
        let fn_repr = fn_value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&fn_value.ty));
        let pair_tmp = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca {}", pair_tmp, fn_pair_ty));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            fn_pair_ty, fn_repr, fn_pair_ty, pair_tmp
        ));
        let pair_size_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds {}, {}* null, i32 1",
            pair_size_ptr, fn_pair_ty, fn_pair_ty
        ));
        let pair_size = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint {}* {} to i64",
            pair_size, fn_pair_ty, pair_size_ptr
        ));
        let pair_heap = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i8* @malloc(i64 {})",
            pair_heap, pair_size
        ));
        let pair_heap_typed = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to {}*",
            pair_heap_typed, pair_heap, fn_pair_ty
        ));
        let pair_loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            pair_loaded, fn_pair_ty, fn_pair_ty, pair_tmp
        ));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            fn_pair_ty, pair_loaded, fn_pair_ty, pair_heap_typed
        ));

        let helper_name = format!("__aic_spawn_entry_{}", self.closure_counter);
        self.closure_counter += 1;
        self.emit_concurrency_spawn_entry_helper(&helper_name, &fn_layout);

        let entry_fn_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint i64 (i8*)* @{} to i64",
            entry_fn_raw, helper_name
        ));
        let entry_env_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint i8* {} to i64",
            entry_env_raw, pair_heap
        ));

        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_scope_spawn_fn(i64 {}, i64 {}, i64 {}, i64* {})",
            err, scope_handle, entry_fn_raw, entry_env_raw, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.concurrency_spawn_fn_result_ty(name, fn_layout.ret.as_ref(), span)?;
        let ok_payload =
            self.build_concurrency_ok_handle_payload(&result_ty, "Task", &handle, span, fctx)?;
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_scope_join_all_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "scope_join_all expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let scope_value = self.gen_expr(&args[0], fctx)?;
        let scope_handle = self.extract_named_handle_from_value(
            &scope_value,
            "Scope",
            "scope_join_all",
            args[0].span,
            fctx,
        )?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_scope_join_all(i64 {})",
            err, scope_handle
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_scope_cancel_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "scope_cancel expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let scope_value = self.gen_expr(&args[0], fctx)?;
        let scope_handle = self.extract_named_handle_from_value(
            &scope_value,
            "Scope",
            "scope_cancel",
            args[0].span,
            fctx,
        )?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_scope_cancel(i64 {})",
            err, scope_handle
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_scope_close_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "scope_close expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let scope_value = self.gen_expr(&args[0], fctx)?;
        let scope_handle = self.extract_named_handle_from_value(
            &scope_value,
            "Scope",
            "scope_close",
            args[0].span,
            fctx,
        )?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_scope_close(i64 {})",
            err, scope_handle
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_join_task_call(
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

    pub(super) fn gen_concurrency_join_value_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "join expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let task = self.gen_expr(&args[0], fctx)?;
        let handle =
            self.extract_named_handle_from_value(&task, "Task", "join", args[0].span, fctx)?;
        let value_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_join_value(i64 {}, i64* {})",
            err, handle, value_slot
        ));
        let out_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_value, value_slot));

        let result_ty = self.concurrency_join_value_result_ty(name, &task.ty, span)?;
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };

        let ok_payload = if ok_ty == LType::Unit {
            Value {
                ty: LType::Unit,
                repr: None,
            }
        } else {
            self.extern_decls
                .insert("declare void @aic_rt_heap_free(i8*)".to_string());
            let ok_llvm = llvm_type(&ok_ty);
            let ok_stack = self.new_temp();
            fctx.lines
                .push(format!("  {} = alloca {}", ok_stack, ok_llvm));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                ok_llvm,
                default_value(&ok_ty),
                ok_llvm,
                ok_stack
            ));

            let has_ptr = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp ne i64 {}, 0", has_ptr, out_value));
            let load_label = self.new_label("conc_join_value_load");
            let cont_label = self.new_label("conc_join_value_cont");
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                has_ptr, load_label, cont_label
            ));
            fctx.lines.push(format!("{}:", load_label));
            let out_ptr_i8 = self.new_temp();
            fctx.lines.push(format!(
                "  {} = inttoptr i64 {} to i8*",
                out_ptr_i8, out_value
            ));
            let typed_ptr = self.new_temp();
            fctx.lines.push(format!(
                "  {} = bitcast i8* {} to {}*",
                typed_ptr, out_ptr_i8, ok_llvm
            ));
            let loaded_ok = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                loaded_ok, ok_llvm, ok_llvm, typed_ptr
            ));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                ok_llvm, loaded_ok, ok_llvm, ok_stack
            ));
            fctx.lines
                .push(format!("  call void @aic_rt_heap_free(i8* {})", out_ptr_i8));
            fctx.lines.push(format!("  br label %{}", cont_label));
            fctx.lines.push(format!("{}:", cont_label));
            let final_ok = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                final_ok, ok_llvm, ok_llvm, ok_stack
            ));
            Value {
                ty: ok_ty,
                repr: Some(final_ok),
            }
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_timeout_task_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "timeout_task expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let task = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &task,
            "Task",
            "timeout_task",
            args[0].span,
            fctx,
        )?;
        let timeout_ms = self.gen_expr(&args[1], fctx)?;
        if timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "timeout_task expects (Task, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let value_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_join_timeout(i64 {}, i64 {}, i64* {})",
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

    pub(super) fn gen_concurrency_cancel_task_call(
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

    pub(super) fn gen_concurrency_spawn_group_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "spawn_group expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let values = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, _, _) = self.vec_element_info(&values.ty, "spawn_group", args[0].span)?;
        if elem_ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "spawn_group expects (Vec[Int], Int)",
                self.file,
                span,
            ));
            return None;
        }
        let delay_ms = self.gen_expr(&args[1], fctx)?;
        if delay_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "spawn_group expects (Vec[Int], Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (values_ptr, values_len, values_cap) =
            self.vec_ptr_len_cap_i8(&values, args[0].span, fctx)?;
        let out_values_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64*", out_values_slot));
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_spawn_group(i8* {}, i64 {}, i64 {}, i64 {}, i64** {}, i64* {})",
            err,
            values_ptr,
            values_len,
            values_cap,
            delay_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_values_slot,
            out_count_slot
        ));
        let out_values_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64*, i64** {}",
            out_values_i64, out_values_slot
        ));
        let out_values = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i64* {} to i8*",
            out_values, out_values_i64
        ));
        let out_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_count, out_count_slot
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let ok_payload =
            self.build_vec_value_from_raw_i8_ptr(&ok_ty, &out_values, &out_count, span, fctx)?;
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_select_first_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "select_first expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let tasks = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, _, _) = self.vec_element_info(&tasks.ty, "select_first", args[0].span)?;
        let is_task_element = match elem_ty {
            LType::Struct(layout)
                if base_type_name(&layout.repr) == "Task"
                    && layout.fields.len() == 1
                    && layout.fields[0].ty == LType::Int =>
            {
                true
            }
            _ => false,
        };
        if !is_task_element {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "select_first expects (Vec[Task[Int]], Int)",
                self.file,
                span,
            ));
            return None;
        }
        let timeout_ms = self.gen_expr(&args[1], fctx)?;
        if timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "select_first expects (Vec[Task[Int]], Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (tasks_ptr, tasks_len, tasks_cap) =
            self.vec_ptr_len_cap_i8(&tasks, args[0].span, fctx)?;
        let selected_index_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", selected_index_slot));
        let selected_value_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", selected_value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_select_first(i8* {}, i64 {}, i64 {}, i64 {}, i64* {}, i64* {})",
            err,
            tasks_ptr,
            tasks_len,
            tasks_cap,
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            selected_index_slot,
            selected_value_slot
        ));
        let selected_index = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            selected_index, selected_index_slot
        ));
        let selected_value = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            selected_value, selected_value_slot
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let LType::Struct(layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "select_first expects Result[IntTaskSelection, ConcurrencyError] return type",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "IntTaskSelection"
            || layout.fields.len() != 2
            || layout.fields[0].ty != LType::Int
            || layout.fields[1].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "select_first expects Result[IntTaskSelection, ConcurrencyError] return type",
                self.file,
                span,
            ));
            return None;
        }
        let ok_payload = self.build_struct_value(
            &layout,
            &[
                Value {
                    ty: LType::Int,
                    repr: Some(selected_index),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(selected_value),
                },
            ],
            span,
            fctx,
        )?;
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_channel_int_call(
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

    pub(super) fn gen_concurrency_channel_int_buffered_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "channel_int_buffered expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let capacity = self.gen_expr(&args[0], fctx)?;
        if capacity.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "channel_int_buffered expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_channel_int_buffered(i64 {}, i64* {})",
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

    pub(super) fn gen_concurrency_send_int_call(
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

    pub(super) fn gen_concurrency_try_send_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "try_send_int expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let channel = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &channel,
            "IntChannel",
            "try_send_int",
            args[0].span,
            fctx,
        )?;
        let value = self.gen_expr(&args[1], fctx)?;
        if value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "try_send_int expects (IntChannel, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_try_send_int(i64 {}, i64 {})",
            err,
            handle,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_channel_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_recv_int_call(
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

    pub(super) fn gen_concurrency_try_recv_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "try_recv_int expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let channel = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &channel,
            "IntChannel",
            "try_recv_int",
            args[0].span,
            fctx,
        )?;
        let value_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_try_recv_int(i64 {}, i64* {})",
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
        self.wrap_channel_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_select_recv_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "select_recv_int expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let first = self.gen_expr(&args[0], fctx)?;
        let first_handle = self.extract_named_handle_from_value(
            &first,
            "IntChannel",
            "select_recv_int",
            args[0].span,
            fctx,
        )?;
        let second = self.gen_expr(&args[1], fctx)?;
        let second_handle = self.extract_named_handle_from_value(
            &second,
            "IntChannel",
            "select_recv_int",
            args[1].span,
            fctx,
        )?;
        let timeout_ms = self.gen_expr(&args[2], fctx)?;
        if timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "select_recv_int expects (IntChannel, IntChannel, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let selected_index_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", selected_index_slot));
        let value_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_select_recv_int(i64 {}, i64 {}, i64 {}, i64* {}, i64* {})",
            err,
            first_handle,
            second_handle,
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            selected_index_slot,
            value_slot
        ));
        let selected_index = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            selected_index, selected_index_slot
        ));
        let selected_value = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            selected_value, value_slot
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let LType::Struct(layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "select_recv_int expects Result[IntChannelSelection, ChannelError] return type",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "IntChannelSelection"
            || layout.fields.len() != 2
            || layout.fields[0].ty != LType::Int
            || layout.fields[1].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "select_recv_int expects Result[IntChannelSelection, ChannelError] return type",
                self.file,
                span,
            ));
            return None;
        }
        let ok_payload = self.build_struct_value(
            &layout,
            &[
                Value {
                    ty: LType::Int,
                    repr: Some(selected_index),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(selected_value),
                },
            ],
            span,
            fctx,
        )?;
        self.wrap_channel_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_close_channel_call(
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

    pub(super) fn gen_concurrency_mutex_int_call(
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

    pub(super) fn gen_concurrency_lock_int_call(
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

    pub(super) fn gen_concurrency_unlock_int_call(
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

    pub(super) fn gen_concurrency_close_mutex_call(
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

    pub(super) fn gen_concurrency_rwlock_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "rwlock_int expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let initial = self.gen_expr(&args[0], fctx)?;
        if initial.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "rwlock_int expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_rwlock_int(i64 {}, i64* {})",
            err,
            initial.repr.clone().unwrap_or_else(|| "0".to_string()),
            handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload =
            self.build_concurrency_ok_handle_payload(&result_ty, "IntRwLock", &handle, span, fctx)?;
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_rwlock_read_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "read_lock_int expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let rwlock = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &rwlock,
            "IntRwLock",
            "read_lock_int",
            args[0].span,
            fctx,
        )?;
        let timeout_ms = self.gen_expr(&args[1], fctx)?;
        if timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "read_lock_int expects (IntRwLock, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let value_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_rwlock_read(i64 {}, i64 {}, i64* {})",
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

    pub(super) fn gen_concurrency_rwlock_write_lock_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "write_lock_int expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let rwlock = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &rwlock,
            "IntRwLock",
            "write_lock_int",
            args[0].span,
            fctx,
        )?;
        let timeout_ms = self.gen_expr(&args[1], fctx)?;
        if timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "write_lock_int expects (IntRwLock, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let value_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", value_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_rwlock_write_lock(i64 {}, i64 {}, i64* {})",
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

    pub(super) fn gen_concurrency_rwlock_write_unlock_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "write_unlock_int expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let rwlock = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &rwlock,
            "IntRwLock",
            "write_unlock_int",
            args[0].span,
            fctx,
        )?;
        let value = self.gen_expr(&args[1], fctx)?;
        if value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "write_unlock_int expects (IntRwLock, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_rwlock_write_unlock(i64 {}, i64 {})",
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

    pub(super) fn gen_concurrency_close_rwlock_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "close_rwlock expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let rwlock = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &rwlock,
            "IntRwLock",
            "close_rwlock",
            args[0].span,
            fctx,
        )?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_rwlock_close(i64 {})",
            err, handle
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_payload_store_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_payload_store_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let payload = self.gen_expr(&args[0], fctx)?;
        if payload.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_payload_store_intrinsic expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&payload, args[0].span, fctx)?;
        let payload_id_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", payload_id_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", payload_id_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_payload_store(i8* {}, i64 {}, i64 {}, i64* {})",
            err, ptr, len, cap, payload_id_slot
        ));
        let payload_id = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            payload_id, payload_id_slot
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(payload_id),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_payload_take_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_payload_take_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let payload_id = self.gen_expr(&args[0], fctx)?;
        if payload_id.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_payload_take_intrinsic expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_payload_take(i64 {}, i8** {}, i64* {})",
            err,
            payload_id.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let payload = self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
        let result_ty = self.concurrency_result_ty(name, span)?;
        self.wrap_concurrency_result(&result_ty, payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_payload_drop_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_payload_drop_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let payload_id = self.gen_expr(&args[0], fctx)?;
        if payload_id.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_payload_drop_intrinsic expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let dropped_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", dropped_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", dropped_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_payload_drop(i64 {}, i64* {})",
            err,
            payload_id.repr.clone().unwrap_or_else(|| "0".to_string()),
            dropped_slot
        ));
        let dropped_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            dropped_i64, dropped_slot
        ));
        let dropped_bool = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp ne i64 {}, 0",
            dropped_bool, dropped_i64
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(dropped_bool),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_arc_new_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_arc_new_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let payload = self.gen_expr(&args[0], fctx)?;
        if payload.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_arc_new_intrinsic expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&payload, args[0].span, fctx)?;
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_arc_new(i8* {}, i64 {}, i64 {}, i64* {})",
            err, ptr, len, cap, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.parse_type_repr("Result[Int, ConcurrencyError]", span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_arc_clone_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_arc_clone_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_arc_clone_intrinsic expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_handle_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_handle_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_arc_clone(i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_handle_slot
        ));
        let out_handle = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_handle, out_handle_slot
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_handle),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_arc_get_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_arc_get_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_arc_get_intrinsic expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_arc_get(i64 {}, i8** {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let payload = self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
        let result_ty = self.concurrency_result_ty(name, span)?;
        self.wrap_concurrency_result(&result_ty, payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_arc_strong_count_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_arc_strong_count_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_arc_strong_count_intrinsic expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_count_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_arc_strong_count(i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_count_slot
        ));
        let count = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", count, out_count_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(count),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_atomic_int_new_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_atomic_int_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let initial = self.gen_expr(&args[0], fctx)?;
        if initial.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_atomic_int_intrinsic expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_atomic_int_new(i64 {}, i64* {})",
            err,
            initial.repr.clone().unwrap_or_else(|| "0".to_string()),
            handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_atomic_int_load_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_atomic_load_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_atomic_load_intrinsic expects Int",
                self.file,
                args[0].span,
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
            "  {} = call i64 @aic_rt_conc_atomic_int_load(i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_value_slot
        ));
        let out_value = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_value, out_value_slot
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_value),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_atomic_int_store_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_atomic_store_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let value = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int || value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_atomic_store_intrinsic expects (Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_atomic_int_store(i64 {}, i64 {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_atomic_int_add_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_atomic_add_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let delta = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int || delta.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_atomic_add_intrinsic expects (Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_old_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_old_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_old_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_atomic_int_add(i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            delta.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_old_slot
        ));
        let out_old = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_old, out_old_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_old),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_atomic_int_sub_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_atomic_sub_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let delta = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int || delta.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_atomic_sub_intrinsic expects (Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_old_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_old_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_old_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_atomic_int_sub(i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            delta.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_old_slot
        ));
        let out_old = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_old, out_old_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_old),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_atomic_int_cas_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_atomic_cas_intrinsic expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let expected = self.gen_expr(&args[1], fctx)?;
        let desired = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || expected.ty != LType::Int || desired.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_atomic_cas_intrinsic expects (Int, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_swapped_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_swapped_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_swapped_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_atomic_int_cas(i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            expected.repr.clone().unwrap_or_else(|| "0".to_string()),
            desired.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_swapped_slot
        ));
        let out_swapped_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_swapped_i64, out_swapped_slot
        ));
        let out_swapped = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp ne i64 {}, 0",
            out_swapped, out_swapped_i64
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(out_swapped),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_atomic_bool_new_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_atomic_bool_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let initial = self.gen_expr(&args[0], fctx)?;
        if initial.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_atomic_bool_intrinsic expects Bool",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let initial_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = zext i1 {} to i64",
            initial_i64,
            initial.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_atomic_bool_new(i64 {}, i64* {})",
            err, initial_i64, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_atomic_bool_load_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_atomic_load_bool_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_atomic_load_bool_intrinsic expects Int",
                self.file,
                args[0].span,
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
            "  {} = call i64 @aic_rt_conc_atomic_bool_load(i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_value_slot
        ));
        let out_value_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_value_i64, out_value_slot
        ));
        let out_value = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp ne i64 {}, 0",
            out_value, out_value_i64
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(out_value),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_atomic_bool_store_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_atomic_store_bool_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let value = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int || value.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_atomic_store_bool_intrinsic expects (Int, Bool)",
                self.file,
                span,
            ));
            return None;
        }
        let value_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = zext i1 {} to i64",
            value_i64,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_atomic_bool_store(i64 {}, i64 {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            value_i64
        ));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_atomic_bool_swap_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_atomic_swap_bool_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let desired = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int || desired.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_atomic_swap_bool_intrinsic expects (Int, Bool)",
                self.file,
                span,
            ));
            return None;
        }
        let desired_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = zext i1 {} to i64",
            desired_i64,
            desired.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let out_old_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_old_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_old_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_atomic_bool_swap(i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            desired_i64,
            out_old_slot
        ));
        let out_old_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_old_i64, out_old_slot
        ));
        let out_old = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", out_old, out_old_i64));
        let result_ty = self.concurrency_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(out_old),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_thread_local_new_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_tl_new_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }

        let init_fn = self.gen_expr(&args[0], fctx)?;
        let LType::Fn(fn_layout) = init_fn.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_tl_new_intrinsic expects Fn() -> T",
                self.file,
                span,
            ));
            return None;
        };
        if !fn_layout.params.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_tl_new_intrinsic expects Fn() -> T",
                self.file,
                span,
            ));
            return None;
        }

        self.extern_decls
            .insert("declare i8* @malloc(i64)".to_string());

        let fn_pair_ty = llvm_type(&LType::Fn(fn_layout.clone()));
        let fn_repr = init_fn
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&init_fn.ty));
        let pair_tmp = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca {}", pair_tmp, fn_pair_ty));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            fn_pair_ty, fn_repr, fn_pair_ty, pair_tmp
        ));
        let pair_size_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds {}, {}* null, i32 1",
            pair_size_ptr, fn_pair_ty, fn_pair_ty
        ));
        let pair_size = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint {}* {} to i64",
            pair_size, fn_pair_ty, pair_size_ptr
        ));
        let pair_heap = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i8* @malloc(i64 {})",
            pair_heap, pair_size
        ));
        let pair_heap_typed = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to {}*",
            pair_heap_typed, pair_heap, fn_pair_ty
        ));
        let pair_loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            pair_loaded, fn_pair_ty, fn_pair_ty, pair_tmp
        ));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            fn_pair_ty, pair_loaded, fn_pair_ty, pair_heap_typed
        ));

        let helper_name = format!("__aic_tl_init_entry_{}", self.closure_counter);
        self.closure_counter += 1;
        self.emit_concurrency_spawn_entry_helper(&helper_name, &fn_layout);

        let entry_fn_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint i64 (i8*)* @{} to i64",
            entry_fn_raw, helper_name
        ));
        let entry_env_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint i8* {} to i64",
            entry_env_raw, pair_heap
        ));
        let init_ret_ty = fn_layout.ret.as_ref().clone();
        let init_ret_llvm = llvm_type(&init_ret_ty);
        let init_ret_size_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds {}, {}* null, i32 1",
            init_ret_size_ptr, init_ret_llvm, init_ret_llvm
        ));
        let init_ret_size = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint {}* {} to i64",
            init_ret_size, init_ret_llvm, init_ret_size_ptr
        ));

        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_tl_new(i64 {}, i64 {}, i64 {}, i64* {})",
            err, entry_fn_raw, entry_env_raw, init_ret_size, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let result_ty = self.parse_type_repr("Result[Int, ConcurrencyError]", span)?;
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_thread_local_get_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_tl_get_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_tl_get_intrinsic expects Int",
                self.file,
                args[0].span,
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
            "  {} = call i64 @aic_rt_conc_tl_get(i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_value_slot
        ));
        let result_ty = if let Some(expected) = expected_ty {
            expected.clone()
        } else if let Some(sig) = self.fn_sigs.get(name) {
            sig.ret.clone()
        } else {
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

        let ok_payload = if ok_ty == LType::Unit {
            Value {
                ty: LType::Unit,
                repr: None,
            }
        } else {
            let out_value = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load i64, i64* {}",
                out_value, out_value_slot
            ));
            let ok_llvm = llvm_type(&ok_ty);
            let ok_stack = self.new_temp();
            fctx.lines
                .push(format!("  {} = alloca {}", ok_stack, ok_llvm));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                ok_llvm,
                default_value(&ok_ty),
                ok_llvm,
                ok_stack
            ));

            let has_ptr = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp ne i64 {}, 0", has_ptr, out_value));
            let load_label = self.new_label("conc_tl_get_load");
            let cont_label = self.new_label("conc_tl_get_cont");
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                has_ptr, load_label, cont_label
            ));
            fctx.lines.push(format!("{}:", load_label));
            let raw_ptr = self.new_temp();
            fctx.lines
                .push(format!("  {} = inttoptr i64 {} to i8*", raw_ptr, out_value));
            let typed_ptr = self.new_temp();
            fctx.lines.push(format!(
                "  {} = bitcast i8* {} to {}*",
                typed_ptr, raw_ptr, ok_llvm
            ));
            let loaded = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                loaded, ok_llvm, ok_llvm, typed_ptr
            ));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                ok_llvm, loaded, ok_llvm, ok_stack
            ));
            fctx.lines.push(format!("  br label %{}", cont_label));
            fctx.lines.push(format!("{}:", cont_label));
            let final_ok = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                final_ok, ok_llvm, ok_llvm, ok_stack
            ));
            Value {
                ty: ok_ty,
                repr: Some(final_ok),
            }
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_concurrency_thread_local_set_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_conc_tl_set_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let value = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_conc_tl_set_intrinsic expects (Int, T)",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, size) = if value.ty == LType::Unit {
            ("null".to_string(), "0".to_string())
        } else {
            let value_llvm = llvm_type(&value.ty);
            let value_stack = self.new_temp();
            fctx.lines
                .push(format!("  {} = alloca {}", value_stack, value_llvm));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                value_llvm,
                value
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&value.ty)),
                value_llvm,
                value_stack
            ));
            let ptr = self.new_temp();
            fctx.lines.push(format!(
                "  {} = bitcast {}* {} to i8*",
                ptr, value_llvm, value_stack
            ));
            let size_ptr = self.new_temp();
            fctx.lines.push(format!(
                "  {} = getelementptr inbounds {}, {}* null, i32 1",
                size_ptr, value_llvm, value_llvm
            ));
            let size = self.new_temp();
            fctx.lines.push(format!(
                "  {} = ptrtoint {}* {} to i64",
                size, value_llvm, size_ptr
            ));
            (ptr, size)
        };
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_conc_tl_set(i64 {}, i8* {}, i64 {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            ptr,
            size
        ));
        let result_ty = self.parse_type_repr("Result[Bool, ConcurrencyError]", span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_concurrency_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn wrap_concurrency_result(
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

    pub(super) fn wrap_channel_result(
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
                    "channel builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_channel_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("chan_ok");
        let err_label = self.new_label("chan_err");
        let cont_label = self.new_label("chan_cont");
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
