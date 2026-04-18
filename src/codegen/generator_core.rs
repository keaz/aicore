use super::*;

impl<'a> Generator<'a> {
    pub(super) fn new(
        program: &'a ir::Program,
        resolution: Option<&'a crate::resolver::Resolution>,
        file: &'a str,
        options: CodegenOptions,
    ) -> Self {
        let mut type_map = BTreeMap::new();
        for ty in &program.types {
            type_map.insert(ty.id, ty.repr.clone());
        }
        let (type_aliases, const_defs) = collect_internal_aliases_and_consts(program, &type_map);
        let (
            struct_templates,
            struct_templates_by_module,
            enum_templates,
            enum_templates_by_module,
            variant_ctors,
        ) = collect_type_templates(program, &type_map, resolution);
        let drop_impl_methods = collect_drop_impl_methods(program, &type_map);
        let recursive_call_targets = collect_recursive_call_targets(program);
        let function_modules_by_symbol = resolution
            .map(|resolution| {
                resolution
                    .module_function_infos
                    .iter()
                    .map(|((module, _), info)| (info.symbol, module.clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
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
            resolution,
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
            fn_sigs_by_symbol: BTreeMap::new(),
            fn_llvm_names: BTreeMap::new(),
            extern_decls: BTreeSet::new(),
            type_map,
            type_aliases,
            const_defs,
            const_values: BTreeMap::new(),
            const_failures: BTreeSet::new(),
            struct_templates,
            struct_templates_by_module,
            enum_templates,
            enum_templates_by_module,
            variant_ctors,
            drop_impl_methods,
            generic_fn_instances: BTreeMap::new(),
            generic_fn_instances_by_symbol: BTreeMap::new(),
            active_type_bindings: None,
            closure_counter: 0,
            async_counter: 0,
            deferred_fn_defs: Vec::new(),
            async_ready_helpers: BTreeMap::new(),
            fn_value_adapters: BTreeMap::new(),
            function_modules_by_symbol,
            recursive_call_targets,
            dyn_traits: BTreeMap::new(),
            dyn_vtable_globals: BTreeMap::new(),
            generated_dyn_wrappers: BTreeSet::new(),
            call_sig_overrides: Vec::new(),
            type_module_stack: Vec::new(),
        }
    }

    pub(super) fn finish(self) -> String {
        let mut text = String::new();
        text.push_str("; AICore LLVM IR (deterministic)\n");
        if self.debug.is_some() {
            let source_file = escape_llvm_string(self.file);
            text.push_str(&format!("source_filename = \"{}\"\n", source_file));
        }
        text.push_str("declare void @llvm.lifetime.end.p0i8(i64, i8*)\n");
        text.push_str("declare void @aic_rt_print_int(i64)\n");
        text.push_str("declare void @aic_rt_print_float(double)\n");
        text.push_str("declare void @aic_rt_print_str(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_read_line(i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_read_int(i64*)\n");
        text.push_str("declare i64 @aic_rt_read_char(i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_prompt(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_eprint_str(i8*, i64, i64)\n");
        text.push_str("declare void @aic_rt_eprint_int(i64)\n");
        text.push_str("declare void @aic_rt_println_str(i8*, i64, i64)\n");
        text.push_str("declare void @aic_rt_println_int(i64)\n");
        text.push_str("declare void @aic_rt_print_bool(i64)\n");
        text.push_str("declare void @aic_rt_println_bool(i64)\n");
        text.push_str("declare void @aic_rt_flush_stdout()\n");
        text.push_str("declare void @aic_rt_flush_stderr()\n");
        text.push_str("declare i64 @aic_rt_mock_io_set_stdin(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_mock_io_take_stdout(i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_mock_io_take_stderr(i8**, i64*)\n");
        text.push_str("declare void @aic_rt_log_emit(i64, i8*, i64, i64)\n");
        text.push_str("declare void @aic_rt_log_set_level(i64)\n");
        text.push_str("declare void @aic_rt_log_set_json(i64)\n");
        text.push_str("declare i64 @aic_rt_strlen(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_string_contains(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_string_starts_with(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_string_ends_with(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_string_index_of(i8*, i64, i64, i8*, i64, i64, i64*)\n");
        text.push_str(
            "declare i64 @aic_rt_string_last_index_of(i8*, i64, i64, i8*, i64, i64, i64*)\n",
        );
        text.push_str(
            "declare void @aic_rt_string_substring(i8*, i64, i64, i64, i64, i8**, i64*)\n",
        );
        text.push_str(
            "declare void @aic_rt_string_byte_substring(i8*, i64, i64, i64, i64, i8**, i64*)\n",
        );
        text.push_str("declare i64 @aic_rt_string_char_at(i8*, i64, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_string_compare(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str(
            "declare void @aic_rt_string_split(i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_string_split_first(i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str("declare void @aic_rt_string_trim(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_string_trim_start(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_string_trim_end(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_string_to_upper(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_string_to_lower(i8*, i64, i64, i8**, i64*)\n");
        text.push_str(
            "declare void @aic_rt_string_replace(i8*, i64, i64, i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str("declare void @aic_rt_string_repeat(i8*, i64, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_string_parse_int(i8*, i64, i64, i64*, i8**, i64*)\n");
        text.push_str(
            "declare i64 @aic_rt_string_parse_float(i8*, i64, i64, double*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_bigint_parse(i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_bigint_add(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_bigint_sub(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_bigint_mul(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_bigint_div(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_biguint_parse(i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_biguint_add(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_biguint_sub(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_biguint_mul(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_biguint_div(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_decimal_parse(i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_decimal_add(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_decimal_sub(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_decimal_mul(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_numeric_decimal_div(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str("declare void @aic_rt_string_int_to_string(i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_string_float_to_string(double, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_string_bool_to_string(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_string_is_valid_utf8(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_string_is_ascii(i8*, i64, i64)\n");
        text.push_str(
            "declare void @aic_rt_string_bytes_to_string_lossy(i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str(
            "declare void @aic_rt_string_join(i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str(
            "declare void @aic_rt_string_format(i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str("declare i64 @aic_rt_char_is_digit(i32)\n");
        text.push_str("declare i64 @aic_rt_char_is_alpha(i32)\n");
        text.push_str("declare i64 @aic_rt_char_is_whitespace(i32)\n");
        text.push_str("declare i64 @aic_rt_char_to_int(i32)\n");
        text.push_str("declare i64 @aic_rt_char_int_to_char(i64, i32*)\n");
        text.push_str("declare void @aic_rt_char_chars(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_char_from_chars(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_math_abs(i64)\n");
        text.push_str("declare double @aic_rt_math_abs_float(double)\n");
        text.push_str("declare i64 @aic_rt_math_min(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_math_max(i64, i64)\n");
        text.push_str("declare double @aic_rt_math_pow(double, double)\n");
        text.push_str("declare double @aic_rt_math_sqrt(double)\n");
        text.push_str("declare i64 @aic_rt_math_floor(double)\n");
        text.push_str("declare i64 @aic_rt_math_ceil(double)\n");
        text.push_str("declare i64 @aic_rt_math_round(double)\n");
        text.push_str("declare double @aic_rt_math_log(double)\n");
        text.push_str("declare double @aic_rt_math_sin(double)\n");
        text.push_str("declare double @aic_rt_math_cos(double)\n");
        text.push_str("declare i64 @aic_rt_vec_len(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_vec_cap(i8*, i64, i64)\n");
        text.push_str("declare void @aic_rt_vec_new(i8**, i64*, i64*)\n");
        text.push_str("declare i64 @aic_rt_vec_with_capacity(i64, i64, i8**, i64*, i64*)\n");
        text.push_str("declare i64 @aic_rt_vec_of(i8*, i64, i8**, i64*, i64*)\n");
        text.push_str("declare i64 @aic_rt_vec_get(i8*, i64, i64, i64, i64, i8*)\n");
        text.push_str("declare i64 @aic_rt_vec_push(i8**, i64*, i64*, i64, i8*)\n");
        text.push_str("declare i64 @aic_rt_vec_pop(i8**, i64*, i64*, i64)\n");
        text.push_str("declare i64 @aic_rt_vec_set(i8*, i64, i64, i64, i64, i8*)\n");
        text.push_str("declare i64 @aic_rt_vec_insert(i8**, i64*, i64*, i64, i64, i8*)\n");
        text.push_str("declare i64 @aic_rt_vec_remove_at(i8**, i64*, i64*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_vec_reserve(i8**, i64*, i64*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_vec_shrink_to_fit(i8**, i64*, i64*, i64)\n");
        text.push_str("declare i64 @aic_rt_vec_contains(i8*, i64, i64, i64, i64, i8*)\n");
        text.push_str("declare i64 @aic_rt_vec_index_of(i8*, i64, i64, i64, i64, i8*, i64*)\n");
        text.push_str("declare i64 @aic_rt_vec_reverse(i8*, i64, i64, i64)\n");
        text.push_str(
            "declare i64 @aic_rt_vec_slice(i8*, i64, i64, i64, i64, i64, i8**, i64*, i64*)\n",
        );
        text.push_str("declare i64 @aic_rt_vec_append(i8**, i64*, i64*, i64, i8*, i64, i64)\n");
        text.push_str("declare void @aic_rt_vec_clear(i8**, i64*, i64*)\n");
        text.push_str("declare void @aic_rt_panic(i8*, i64, i64, i64, i64)\n\n");
        text.push_str("declare i64 @aic_rt_time_now_ms()\n");
        text.push_str("declare i64 @aic_rt_time_monotonic_ms()\n");
        text.push_str("declare void @aic_rt_time_sleep_ms(i64)\n\n");
        text.push_str(
            "declare i64 @aic_rt_time_parse_rfc3339(i8*, i64, i64, i64*, i64*, i64*, i64*, i64*, i64*, i64*, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_time_parse_iso8601(i8*, i64, i64, i64*, i64*, i64*, i64*, i64*, i64*, i64*, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_time_format_rfc3339(i64, i64, i64, i64, i64, i64, i64, i64, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_time_format_iso8601(i64, i64, i64, i64, i64, i64, i64, i64, i8**, i64*)\n\n",
        );
        text.push_str("declare i64 @aic_rt_signal_register(i64)\n");
        text.push_str("declare i64 @aic_rt_signal_wait(i64*)\n\n");
        text.push_str("declare void @aic_rt_rand_seed(i64)\n");
        text.push_str("declare i64 @aic_rt_rand_next()\n");
        text.push_str("declare i64 @aic_rt_rand_range(i64, i64)\n\n");
        text.push_str("declare i64 @aic_rt_conc_spawn(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_spawn_fn(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_spawn_fn_named(i64, i64, i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_join(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_join_value(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_join_poll(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_scope_new(i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_scope_spawn_fn(i64, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_scope_join_all(i64)\n");
        text.push_str("declare i64 @aic_rt_conc_scope_cancel(i64)\n");
        text.push_str("declare i64 @aic_rt_conc_scope_close(i64)\n");
        text.push_str("declare i64 @aic_rt_conc_join_timeout(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_cancel(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_spawn_group(i8*, i64, i64, i64, i64**, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_select_first(i8*, i64, i64, i64, i64*, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_channel_int(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_channel_int_buffered(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_send_int(i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_conc_try_send_int(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_conc_recv_int(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_try_recv_int(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_select_recv_int(i64, i64, i64, i64*, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_close_channel(i64)\n");
        text.push_str("declare i64 @aic_rt_conc_mutex_int(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_mutex_lock(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_mutex_unlock(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_conc_mutex_close(i64)\n");
        text.push_str("declare i64 @aic_rt_conc_rwlock_int(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_rwlock_read(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_rwlock_write_lock(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_rwlock_write_unlock(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_conc_rwlock_close(i64)\n");
        text.push_str("declare i64 @aic_rt_conc_payload_store(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_payload_take(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_payload_drop(i64, i64*)\n\n");
        text.push_str("declare i64 @aic_rt_conc_arc_new(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_arc_clone(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_arc_get(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_arc_strong_count(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_arc_release(i64)\n\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_int_new(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_int_load(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_int_store(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_int_add(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_int_sub(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_int_cas(i64, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_int_close(i64)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_bool_new(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_bool_load(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_bool_store(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_bool_swap(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_atomic_bool_close(i64)\n\n");
        text.push_str("declare i64 @aic_rt_conc_tl_new(i64, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_tl_get(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_conc_tl_set(i64, i8*, i64)\n\n");
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
        text.push_str("declare i64 @aic_rt_fs_read_bytes(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_write_bytes(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_append_bytes(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_open_read(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_open_write(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_open_append(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_file_read_line(i64, i8**, i64*, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_file_write_str(i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_file_close(i64)\n");
        text.push_str("declare i64 @aic_rt_fs_mkdir(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_mkdir_all(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_rmdir(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_list_dir(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_create_symlink(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_read_symlink(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_set_readonly(i8*, i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_fs_async_submit_allowed(i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_async_shutdown(i64*)\n");
        text.push_str("declare i64 @aic_rt_fs_async_pressure(i64*, i64*, i64*, i64*)\n\n");
        text.push_str("declare void @aic_rt_env_set_args(i32, i8**)\n");
        text.push_str("declare void @aic_rt_env_args(i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_env_arg_count()\n");
        text.push_str("declare i64 @aic_rt_env_arg_at(i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_stack_ensure_min(i64)\n");
        text.push_str("declare void @aic_rt_exit(i64)\n");
        text.push_str("declare void @aic_rt_env_all_vars(i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_env_home_dir(i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_env_temp_dir(i8**, i64*)\n");
        text.push_str("declare void @aic_rt_env_os_name(i8**, i64*)\n");
        text.push_str("declare void @aic_rt_env_arch(i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_env_get(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_env_set(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_env_remove(i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_env_cwd(i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_env_set_cwd(i8*, i64, i64)\n\n");
        text.push_str("declare i64 @aic_rt_map_new(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_close(i64)\n");
        text.push_str("declare i64 @aic_rt_map_insert_string(i64, i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_insert_string_int_key(i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_insert_string_bool_key(i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_insert_int(i64, i8*, i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_insert_int_int_key(i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_insert_int_bool_key(i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_get_string(i64, i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_get_string_int_key(i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_get_string_bool_key(i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_get_int(i64, i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_get_int_int_key(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_get_int_bool_key(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_contains(i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_contains_int(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_contains_bool(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_remove(i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_remove_int(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_remove_bool(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_map_size(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_keys(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_keys_int(i64, i64**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_keys_bool(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_values_string(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_values_int(i64, i64**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_entries_string(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_entries_int(i64, i8**, i64*)\n\n");
        text.push_str("declare i64 @aic_rt_map_entries_string_int_key(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_entries_string_bool_key(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_entries_int_int_key(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_map_entries_int_bool_key(i64, i8**, i64*)\n\n");
        text.push_str("declare i64 @aic_rt_bytes_byte_at(i8*, i64, i64, i64)\n");
        text.push_str("declare void @aic_rt_bytes_from_byte_values(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_bytes_from_u8_values(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_new(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_new_growable(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_from_bytes(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_to_bytes(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_position(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_remaining(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_seek(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_reset(i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_close(i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_u8(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_i16_be(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_u16_be(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_i32_be(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_u32_be(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_i64_be(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_u64_be(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_i16_le(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_u16_le(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_i32_le(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_u32_le(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_i64_le(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_u64_le(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_bytes(i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_cstring(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_read_length_prefixed(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_u8(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_i16_be(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_u16_be(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_i32_be(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_u32_be(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_i64_be(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_u64_be(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_i16_le(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_u16_le(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_i32_le(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_u32_le(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_i64_le(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_u64_le(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_bytes(i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_cstring(i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_write_string_prefixed(i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_patch_u16_be(i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_patch_u32_be(i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_patch_u64_be(i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_patch_u16_le(i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_patch_u32_le(i64, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_buffer_patch_u64_le(i64, i64, i64)\n\n");
        text.push_str("declare void @aic_rt_crypto_md5(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_crypto_sha256(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_crypto_sha256_raw(i8*, i64, i64, i8**, i64*)\n");
        text.push_str(
            "declare void @aic_rt_crypto_hmac_sha256(i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str(
            "declare void @aic_rt_crypto_hmac_sha256_raw(i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_crypto_pbkdf2_sha256(i8*, i64, i64, i8*, i64, i64, i64, i64, i8**, i64*)\n",
        );
        text.push_str("declare void @aic_rt_crypto_hex_encode(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_crypto_hex_decode(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_crypto_base64_encode(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_crypto_base64_decode(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare void @aic_rt_crypto_random_bytes(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_crypto_secure_eq(i8*, i64, i64, i8*, i64, i64)\n\n");
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
            "declare i64 @aic_rt_proc_pipe(i8*, i64, i64, i8*, i64, i64, i64*, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_proc_run_with(i8*, i64, i64, i8*, i64, i64, i8*, i64, i64, i8*, i64, i64, i64, i64*, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str("declare i64 @aic_rt_proc_is_running(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_proc_current_pid(i64*)\n");
        text.push_str(
            "declare i64 @aic_rt_proc_run_timeout(i8*, i64, i64, i64, i64*, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_proc_pipe_chain(i8*, i64, i64, i64*, i8**, i64*, i8**, i64*)\n\n",
        );
        text.push_str("declare i64 @aic_rt_net_tcp_listen(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_local_addr(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_accept(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_connect(i8*, i64, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_send(i64, i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_send_timeout(i64, i8*, i64, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_recv(i64, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_close(i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_set_nodelay(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_get_nodelay(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_set_keepalive(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_get_keepalive(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_set_keepalive_idle_secs(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_get_keepalive_idle_secs(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_set_keepalive_interval_secs(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_get_keepalive_interval_secs(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_set_keepalive_count(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_get_keepalive_count(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_peer_addr(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_shutdown(i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_shutdown_read(i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_shutdown_write(i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_set_send_buffer_size(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_get_send_buffer_size(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_set_recv_buffer_size(i64, i64)\n");
        text.push_str("declare i64 @aic_rt_net_tcp_get_recv_buffer_size(i64, i64*)\n");
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
        text.push_str("declare i64 @aic_rt_net_dns_lookup_all(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_dns_reverse(i8*, i64, i64, i8**, i64*)\n\n");
        text.push_str("declare i64 @aic_rt_net_async_accept_submit(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_async_send_submit(i64, i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_async_recv_submit(i64, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_async_wait_int(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_async_wait_string(i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_async_cancel(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_net_async_shutdown()\n\n");
        text.push_str("declare i64 @aic_rt_net_async_pressure(i64*, i64*, i64*, i64*)\n\n");
        text.push_str(
            "declare i64 @aic_rt_tls_connect(i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_tls_connect_addr(i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i64, i64, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_tls_accept(i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i64, i64*)\n",
        );
        text.push_str("declare i64 @aic_rt_tls_send(i64, i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_send_timeout(i64, i8*, i64, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_recv(i64, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_async_send_submit(i64, i8*, i64, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_async_recv_submit(i64, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_async_wait_int(i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_async_wait_string(i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_async_cancel(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_async_shutdown()\n");
        text.push_str("declare i64 @aic_rt_tls_async_pressure(i64*, i64*, i64*, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_close(i64)\n");
        text.push_str("declare i64 @aic_rt_tls_peer_subject(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_peer_issuer(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_peer_fingerprint_sha256(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_tls_peer_san_entries(i64, i8**, i64*)\n\n");
        text.push_str("declare i64 @aic_rt_tls_version(i64, i64*)\n\n");
        text.push_str("declare i64 @aic_rt_async_poll_int(i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_async_poll_string(i64, i8**, i64*)\n\n");
        text.push_str(
            "declare i64 @aic_rt_url_parse(i8*, i64, i64, i8**, i64*, i8**, i64*, i64*, i8**, i64*, i8**, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_url_normalize(i8*, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_url_net_addr(i8*, i64, i64, i8*, i64, i64, i64, i8**, i64*)\n\n",
        );
        text.push_str("declare i64 @aic_rt_http_parse_method(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_http_method_name(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_http_status_reason(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_http_validate_header(i8*, i64, i64, i8*, i64, i64)\n");
        text.push_str("declare i64 @aic_rt_http_validate_target(i8*, i64, i64)\n\n");
        text.push_str("declare i64 @aic_rt_http_server_listen(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_http_server_accept(i64, i64, i64*)\n");
        text.push_str(
            "declare i64 @aic_rt_http_server_read_request(i64, i64, i64, i8**, i64*, i8**, i64*, i64*, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_http_server_async_read_request(i64, i64, i64, i8**, i64*, i8**, i64*, i64*, i64*, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_http_server_write_response(i64, i64, i64, i8*, i64, i64, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_http_server_async_write_response(i64, i64, i64, i8*, i64, i64, i64*)\n",
        );
        text.push_str("declare i64 @aic_rt_http_server_close(i64)\n\n");
        text.push_str("declare i64 @aic_rt_router_new(i64*)\n");
        text.push_str("declare i64 @aic_rt_router_add(i64, i8*, i64, i64, i8*, i64, i64, i64)\n");
        text.push_str(
            "declare i64 @aic_rt_router_match(i64, i8*, i64, i64, i8*, i64, i64, i64*, i64*, i64*)\n\n",
        );
        text.push_str("declare i64 @aic_rt_json_parse(i8*, i64, i64, i8**, i64*, i64*)\n");
        text.push_str("declare i64 @aic_rt_json_stringify(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_json_encode_int(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_json_encode_float(double, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_json_encode_bool(i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_json_encode_string(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_json_encode_null(i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_json_decode_int(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_json_decode_float(i8*, i64, i64, double*)\n");
        text.push_str("declare i64 @aic_rt_json_decode_bool(i8*, i64, i64, i64*)\n");
        text.push_str("declare i64 @aic_rt_json_decode_string(i8*, i64, i64, i8**, i64*)\n");
        text.push_str("declare i64 @aic_rt_json_object_empty(i8**, i64*)\n");
        text.push_str(
            "declare i64 @aic_rt_json_object_set(i8*, i64, i64, i8*, i64, i64, i8*, i64, i64, i8**, i64*, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_json_object_get(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i64*, i64*)\n\n",
        );
        text.push_str("declare i64 @aic_rt_regex_compile(i8*, i64, i64, i64)\n");
        text.push_str(
            "declare i64 @aic_rt_regex_is_match(i8*, i64, i64, i64, i8*, i64, i64, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_regex_find(i8*, i64, i64, i64, i8*, i64, i64, i8**, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_regex_captures(i8*, i64, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*, i64*, i64*, i64*)\n",
        );
        text.push_str(
            "declare i64 @aic_rt_regex_replace(i8*, i64, i64, i64, i8*, i64, i64, i8*, i64, i64, i8**, i64*)\n\n",
        );
        if !self.extern_decls.is_empty() {
            for decl in &self.extern_decls {
                text.push_str(decl);
                text.push('\n');
            }
            text.push('\n');
        }

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

    pub(super) fn generate(&mut self) {
        self.evaluate_all_consts();
        self.collect_fn_sigs();
        self.gen_extern_wrappers();

        for item in &self.program.items {
            if let ir::Item::Function(func) = item {
                self.generate_function_item(func);
            } else if let ir::Item::Impl(impl_def) = item {
                for method in &impl_def.methods {
                    self.generate_function_item(method);
                }
            }
        }

        self.gen_entry_wrapper();
    }

    pub(super) fn generate_function_item(&mut self, func: &ir::Function) {
        if decode_internal_type_alias(&func.name).is_some()
            || decode_internal_const(&func.name).is_some()
        {
            return;
        }
        if func.is_extern || func.is_intrinsic {
            return;
        }
        if func.generics.is_empty() {
            self.gen_function(func);
            self.flush_deferred_fn_defs();
        } else if let Some(instances) = self
            .generic_fn_instances_by_symbol
            .get(&func.symbol)
            .cloned()
        {
            for instance in instances {
                self.gen_monomorphized_function(func, &instance);
                self.flush_deferred_fn_defs();
            }
        }
    }

    pub(super) fn collect_fn_sigs(&mut self) {
        let mut function_items_by_symbol = BTreeMap::new();
        let mut function_items_by_name: BTreeMap<String, Vec<&ir::Function>> = BTreeMap::new();
        let mut name_counts: BTreeMap<String, usize> = BTreeMap::new();
        for item in &self.program.items {
            if let ir::Item::Function(func) = item {
                self.collect_function_sig_item(
                    func,
                    &mut function_items_by_symbol,
                    &mut function_items_by_name,
                    &mut name_counts,
                );
            } else if let ir::Item::Impl(impl_def) = item {
                for method in &impl_def.methods {
                    self.collect_function_sig_item(
                        method,
                        &mut function_items_by_symbol,
                        &mut function_items_by_name,
                        &mut name_counts,
                    );
                }
            }
        }

        for inst in self
            .program
            .generic_instantiations
            .iter()
            .filter(|inst| inst.kind == ir::GenericInstantiationKind::Function)
        {
            let func = if let Some(symbol) = inst.symbol {
                function_items_by_symbol.get(&symbol).copied()
            } else {
                match function_items_by_name.get(&inst.name) {
                    Some(items) if items.len() == 1 => items.first().copied(),
                    Some(items) if items.len() > 1 => {
                        self.diagnostics.push(Diagnostic::error(
                            "E5019",
                            format!(
                                "ambiguous generic function instantiation '{}': {} symbols match; include symbol metadata",
                                inst.name,
                                items.len()
                            ),
                            self.file,
                            self.program.span,
                        ));
                        None
                    }
                    _ => None,
                }
            };
            let Some(func) = func else {
                continue;
            };
            let _module_guard = self.type_module_guard_for_symbol(func.symbol);
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
            let (Some(params), Some(mut ret)) = (params, ret) else {
                continue;
            };
            if func.is_async {
                ret = LType::Async(Box::new(ret));
            }

            let instance = GenericFnInstance {
                mangled: inst.mangled.clone(),
                params,
                ret,
                bindings,
            };
            self.generic_fn_instances
                .entry(func.name.clone())
                .or_default()
                .push(instance.clone());
            if let Some(symbol) = inst.symbol {
                self.generic_fn_instances_by_symbol
                    .entry(symbol)
                    .or_default()
                    .push(instance);
            }
        }
        for instances in self.generic_fn_instances.values_mut() {
            instances.sort_by(|a, b| a.mangled.cmp(&b.mangled));
            instances.dedup_by(|a, b| a.mangled == b.mangled);
        }
        for instances in self.generic_fn_instances_by_symbol.values_mut() {
            instances.sort_by(|a, b| a.mangled.cmp(&b.mangled));
            instances.dedup_by(|a, b| a.mangled == b.mangled);
        }

        self.fn_sigs.insert(
            "print_int".to_string(),
            FnSig {
                is_extern: false,
                extern_symbol: None,
                extern_abi: None,
                is_intrinsic: false,
                intrinsic_abi: None,
                params: vec![LType::Int],
                ret: LType::Unit,
            },
        );
        self.fn_sigs.insert(
            "print_str".to_string(),
            FnSig {
                is_extern: false,
                extern_symbol: None,
                extern_abi: None,
                is_intrinsic: false,
                intrinsic_abi: None,
                params: vec![LType::String],
                ret: LType::Unit,
            },
        );
        self.fn_sigs.insert(
            "print_float".to_string(),
            FnSig {
                is_extern: false,
                extern_symbol: None,
                extern_abi: None,
                is_intrinsic: false,
                intrinsic_abi: None,
                params: vec![LType::Float],
                ret: LType::Unit,
            },
        );
        self.fn_sigs.insert(
            "len".to_string(),
            FnSig {
                is_extern: false,
                extern_symbol: None,
                extern_abi: None,
                is_intrinsic: false,
                intrinsic_abi: None,
                params: vec![LType::String],
                ret: LType::Int,
            },
        );
        self.fn_sigs.insert(
            "panic".to_string(),
            FnSig {
                is_extern: false,
                extern_symbol: None,
                extern_abi: None,
                is_intrinsic: false,
                intrinsic_abi: None,
                params: vec![LType::String],
                ret: LType::Unit,
            },
        );
    }

    pub(super) fn collect_function_sig_item<'b>(
        &mut self,
        func: &'b ir::Function,
        function_items_by_symbol: &mut BTreeMap<ir::SymbolId, &'b ir::Function>,
        function_items_by_name: &mut BTreeMap<String, Vec<&'b ir::Function>>,
        name_counts: &mut BTreeMap<String, usize>,
    ) {
        let _module_guard = self.type_module_guard_for_symbol(func.symbol);
        if decode_internal_type_alias(&func.name).is_some()
            || decode_internal_const(&func.name).is_some()
        {
            return;
        }
        function_items_by_symbol.insert(func.symbol, func);
        function_items_by_name
            .entry(func.name.clone())
            .or_default()
            .push(func);
        let count = name_counts.entry(func.name.clone()).or_insert(0);
        let llvm_name = if *count == 0 {
            mangle(&func.name)
        } else {
            format!("{}__s{}", mangle(&func.name), func.symbol.0)
        };
        *count += 1;
        self.fn_llvm_names.insert(func.symbol, llvm_name);
        if !func.generics.is_empty() {
            return;
        }
        let params = func
            .params
            .iter()
            .map(|p| self.type_from_id(p.ty, p.span))
            .collect::<Option<Vec<_>>>();
        let ret = self.type_from_id(func.ret_type, func.span);
        if let (Some(params), Some(mut ret)) = (params, ret) {
            if func.is_async {
                ret = LType::Async(Box::new(ret));
            }
            let sig = FnSig {
                is_extern: func.is_extern,
                extern_symbol: if func.is_extern {
                    Some(func.name.clone())
                } else {
                    None
                },
                extern_abi: func.extern_abi.clone(),
                is_intrinsic: func.is_intrinsic,
                intrinsic_abi: func.intrinsic_abi.clone(),
                params,
                ret,
            };
            self.fn_sigs.insert(func.name.clone(), sig.clone());
            self.fn_sigs_by_symbol.insert(func.symbol, sig);
        }
    }

    pub(super) fn function_signature(&mut self, func: &ir::Function) -> Option<FnSig> {
        let _module_guard = self.type_module_guard_for_symbol(func.symbol);
        let params = func
            .params
            .iter()
            .map(|p| self.type_from_id(p.ty, p.span))
            .collect::<Option<Vec<_>>>()?;
        let mut ret = self.type_from_id(func.ret_type, func.span)?;
        if func.is_async {
            ret = LType::Async(Box::new(ret));
        }
        Some(FnSig {
            is_extern: func.is_extern,
            extern_symbol: if func.is_extern {
                Some(func.name.clone())
            } else {
                None
            },
            extern_abi: func.extern_abi.clone(),
            is_intrinsic: func.is_intrinsic,
            intrinsic_abi: func.intrinsic_abi.clone(),
            params,
            ret,
        })
    }

    pub(super) fn llvm_name_for_function(&self, func: &ir::Function) -> String {
        self.fn_llvm_names
            .get(&func.symbol)
            .cloned()
            .unwrap_or_else(|| mangle(&func.name))
    }

    pub(super) fn gen_extern_wrappers(&mut self) {
        for item in &self.program.items {
            let ir::Item::Function(func) = item else {
                continue;
            };
            if !func.is_extern {
                continue;
            }

            let Some(sig) = self.function_signature(func) else {
                continue;
            };
            if !sig.is_extern {
                continue;
            }
            let abi = sig.extern_abi.clone().unwrap_or_else(|| "<?>".to_string());
            if abi != "C" {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E5024",
                        format!(
                            "backend only supports extern \"C\"; function '{}' uses extern \"{}\"",
                            func.name, abi
                        ),
                        self.file,
                        func.span,
                    )
                    .with_help("change the declaration to `extern \"C\" fn ...;`"),
                );
                continue;
            }

            let Some(raw_symbol) = sig.extern_symbol.clone() else {
                self.diagnostics.push(Diagnostic::error(
                    "E5024",
                    format!(
                        "extern function '{}' is missing a native symbol name",
                        func.name
                    ),
                    self.file,
                    func.span,
                ));
                continue;
            };

            let raw_params = sig
                .params
                .iter()
                .flat_map(|ty| match ty {
                    LType::String => vec!["i8*".to_string(), "i64".to_string(), "i64".to_string()],
                    _ => vec![llvm_type(ty)],
                })
                .collect::<Vec<_>>()
                .join(", ");
            if sig.ret == LType::String {
                self.extern_decls.insert(format!(
                    "declare void @{}({}* sret({}){})",
                    raw_symbol,
                    llvm_type(&sig.ret),
                    llvm_type(&sig.ret),
                    if raw_params.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", raw_params)
                    }
                ));
            } else {
                self.extern_decls.insert(format!(
                    "declare {} @{}({})",
                    llvm_type(&sig.ret),
                    raw_symbol,
                    raw_params
                ));
            }

            let wrapper_name = self.llvm_name_for_function(func);
            let wrapper_params = sig
                .params
                .iter()
                .enumerate()
                .map(|(idx, ty)| format!("{} %arg{}", llvm_type(ty), idx))
                .collect::<Vec<_>>()
                .join(", ");
            self.out.push(format!(
                "define {} @{}({}) {{",
                llvm_type(&sig.ret),
                wrapper_name,
                wrapper_params
            ));
            self.out.push("entry:".to_string());
            let mut call_args = Vec::new();
            for (idx, ty) in sig.params.iter().enumerate() {
                if *ty == LType::String {
                    let ptr = self.new_temp();
                    let len = self.new_temp();
                    let cap = self.new_temp();
                    self.out.push(format!(
                        "  {} = extractvalue {} %arg{}, 0",
                        ptr,
                        llvm_type(ty),
                        idx
                    ));
                    self.out.push(format!(
                        "  {} = extractvalue {} %arg{}, 1",
                        len,
                        llvm_type(ty),
                        idx
                    ));
                    self.out.push(format!(
                        "  {} = extractvalue {} %arg{}, 2",
                        cap,
                        llvm_type(ty),
                        idx
                    ));
                    call_args.push(format!("i8* {}", ptr));
                    call_args.push(format!("i64 {}", len));
                    call_args.push(format!("i64 {}", cap));
                } else {
                    call_args.push(format!("{} %arg{}", llvm_type(ty), idx));
                }
            }
            let call_args = call_args.join(", ");
            if sig.ret == LType::Unit {
                self.out
                    .push(format!("  call void @{}({})", raw_symbol, call_args));
                self.out.push("  ret void".to_string());
            } else if sig.ret == LType::String {
                let raw_slot = self.new_temp();
                self.out
                    .push(format!("  {} = alloca {}", raw_slot, llvm_type(&sig.ret)));
                self.out.push(format!(
                    "  call void @{}({}* sret({}) {}{})",
                    raw_symbol,
                    llvm_type(&sig.ret),
                    llvm_type(&sig.ret),
                    raw_slot,
                    if call_args.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", call_args)
                    }
                ));
                let raw_loaded = self.new_temp();
                self.out.push(format!(
                    "  {} = load {}, {}* {}",
                    raw_loaded,
                    llvm_type(&sig.ret),
                    llvm_type(&sig.ret),
                    raw_slot
                ));
                self.out
                    .push(format!("  ret {} {}", llvm_type(&sig.ret), raw_loaded));
            } else {
                let out = self.new_temp();
                self.out.push(format!(
                    "  {} = call {} @{}({})",
                    out,
                    llvm_type(&sig.ret),
                    raw_symbol,
                    call_args
                ));
                self.out
                    .push(format!("  ret {} {}", llvm_type(&sig.ret), out));
            }
            self.out.push("}".to_string());
            self.out.push(String::new());
        }
    }

    pub(super) fn gen_function(&mut self, func: &ir::Function) {
        let Some(sig) = self.function_signature(func) else {
            return;
        };
        let llvm_name = self.llvm_name_for_function(func);
        self.gen_function_with_signature(func, &sig, &llvm_name, None);
    }

    pub(super) fn gen_monomorphized_function(
        &mut self,
        func: &ir::Function,
        inst: &GenericFnInstance,
    ) {
        let sig = FnSig {
            is_extern: false,
            extern_symbol: None,
            extern_abi: None,
            is_intrinsic: false,
            intrinsic_abi: None,
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

    pub(super) fn gen_function_with_signature(
        &mut self,
        func: &ir::Function,
        sig: &FnSig,
        llvm_name: &str,
        bindings: Option<BTreeMap<String, String>>,
    ) {
        let _module_guard = self.type_module_guard_for_symbol(func.symbol);
        let previous_bindings = self.active_type_bindings.clone();
        self.active_type_bindings = bindings;
        let (line, _) = self.span_line_col(func.span);
        let debug_scope = self
            .debug
            .as_mut()
            .map(|debug| debug.new_subprogram(&func.name, llvm_name, line));
        if func.is_async {
            self.gen_async_function_with_signature(func, sig, llvm_name, debug_scope);
            self.active_type_bindings = previous_bindings;
            return;
        }
        let async_inner_ret = if func.is_async {
            if let LType::Async(inner) = &sig.ret {
                Some((**inner).clone())
            } else {
                None
            }
        } else {
            None
        };

        let llvm_ret = llvm_type(&sig.ret);
        let mut param_defs = Vec::new();
        for (idx, ty) in sig.params.iter().enumerate() {
            param_defs.push(format!("{} %arg{}", llvm_type(ty), idx));
        }

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
            drop_scopes: vec![DropScope::default()],
            terminated: false,
            current_label: "entry".to_string(),
            ret_ty: async_inner_ret.clone().unwrap_or_else(|| sig.ret.clone()),
            async_inner_ret: async_inner_ret.clone(),
            debug_scope,
            loop_stack: Vec::new(),
            current_fn_name: func.name.clone(),
            current_fn_llvm_name: llvm_name.to_string(),
            current_fn_sig: sig.clone(),
            tail_return_mode: false,
            suppress_lifetime_end: false,
            async_poll_ctx: None,
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
            fctx.vars.last_mut().expect("scope").insert(
                param.name.clone(),
                Local {
                    symbol: None,
                    ty,
                    ptr,
                },
            );
        }

        let expected_tail = async_inner_ret.as_ref().unwrap_or(&sig.ret);
        fctx.tail_return_mode = true;
        let tail = self.gen_block_with_expected_tail(&func.body, Some(expected_tail), &mut fctx);
        fctx.tail_return_mode = false;

        if !fctx.terminated {
            if let Some(inner_ty) = async_inner_ret.as_ref() {
                let async_value = if let Some(value) = tail {
                    self.build_ready_async_value(value, inner_ty, &mut fctx)
                } else {
                    self.build_ready_async_value(
                        Value {
                            ty: inner_ty.clone(),
                            repr: if *inner_ty == LType::Unit {
                                None
                            } else {
                                Some(default_value(inner_ty))
                            },
                        },
                        inner_ty,
                        &mut fctx,
                    )
                };
                fctx.lines.push(format!(
                    "  ret {} {}",
                    llvm_type(&async_value.ty),
                    async_value
                        .repr
                        .unwrap_or_else(|| default_value(&async_value.ty))
                ));
            } else {
                match sig.ret {
                    LType::Unit => fctx.lines.push("  ret void".to_string()),
                    _ => {
                        if let Some(value) = tail {
                            let coerced = self
                                .coerce_value_to_expected(value, &sig.ret, func.span, &mut fctx);
                            if let Some(value) = coerced {
                                if value.ty == sig.ret {
                                    fctx.lines.push(format!(
                                        "  ret {} {}",
                                        llvm_type(&value.ty),
                                        value.repr.unwrap_or_else(|| default_value(&value.ty))
                                    ));
                                } else {
                                    self.diagnostics.push(Diagnostic::error(
                                        "E5007",
                                        format!(
                                            "function '{}' return type mismatch in codegen",
                                            func.name
                                        ),
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
                                self.diagnostics.push(Diagnostic::error(
                                    "E5007",
                                    format!(
                                        "function '{}' return type mismatch in codegen",
                                        func.name
                                    ),
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
        }

        self.out.extend(fctx.lines);
        self.out.push("}".to_string());
        self.out.push(String::new());
        self.active_type_bindings = previous_bindings;
    }

    fn gen_async_function_with_signature(
        &mut self,
        func: &ir::Function,
        sig: &FnSig,
        llvm_name: &str,
        debug_scope: Option<usize>,
    ) {
        let LType::Async(_) = &sig.ret else {
            self.diagnostics.push(Diagnostic::error(
                "E5020",
                format!("async function '{}' must lower to Async[T]", func.name),
                self.file,
                func.span,
            ));
            return;
        };

        let suffix = self.async_counter;
        self.async_counter += 1;
        let frame_repr = format!("__AicAsyncFrame_{suffix}");
        let Some(frame_plan) = self.build_async_frame_plan(func, sig, &frame_repr) else {
            return;
        };
        let poll_name = format!("__aic_async_poll_{suffix}");
        let drop_name = format!("__aic_async_drop_{suffix}");

        self.emit_async_constructor_wrapper(
            func,
            sig,
            llvm_name,
            debug_scope,
            &frame_plan,
            &poll_name,
            &drop_name,
        );
        let pending_states = self.emit_async_poll_helper(func, sig, &frame_plan, &poll_name);
        self.emit_async_drop_helper(func, sig, &frame_plan, &drop_name, &pending_states);
    }

    fn emit_async_constructor_wrapper(
        &mut self,
        func: &ir::Function,
        sig: &FnSig,
        llvm_name: &str,
        debug_scope: Option<usize>,
        frame_plan: &AsyncFramePlan,
        poll_name: &str,
        drop_name: &str,
    ) {
        self.extern_decls
            .insert("declare i8* @malloc(i64)".to_string());

        let llvm_ret = llvm_type(&sig.ret);
        let mut param_defs = Vec::new();
        for (idx, ty) in sig.params.iter().enumerate() {
            param_defs.push(format!("{} %arg{}", llvm_type(ty), idx));
        }
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
        self.out.push("entry:".to_string());
        let frame_size_ptr = self.new_temp();
        self.out.push(format!(
            "  {} = getelementptr inbounds {}, {}* null, i32 1",
            frame_size_ptr, frame_plan.frame_llvm, frame_plan.frame_llvm
        ));
        let frame_size = self.new_temp();
        self.out.push(format!(
            "  {} = ptrtoint {}* {} to i64",
            frame_size, frame_plan.frame_llvm, frame_size_ptr
        ));
        let frame_raw = self.new_temp();
        self.out.push(format!(
            "  {} = call i8* @malloc(i64 {})",
            frame_raw, frame_size
        ));
        let frame_ptr = self.new_temp();
        self.out.push(format!(
            "  {} = bitcast i8* {} to {}*",
            frame_ptr, frame_raw, frame_plan.frame_llvm
        ));
        self.out.push(format!(
            "  store {} {}, {}* {}",
            frame_plan.frame_llvm,
            default_value(&frame_plan.frame_ty),
            frame_plan.frame_llvm,
            frame_ptr
        ));
        for (idx, field_index) in frame_plan.param_indices.iter().enumerate() {
            let Some(param_ty) = sig.params.get(idx) else {
                continue;
            };
            let field_ptr = self.new_temp();
            self.out.push(format!(
                "  {} = getelementptr inbounds {}, {}* {}, i32 0, i32 {}",
                field_ptr, frame_plan.frame_llvm, frame_plan.frame_llvm, frame_ptr, field_index
            ));
            self.out.push(format!(
                "  store {} %arg{}, {}* {}",
                llvm_type(param_ty),
                idx,
                llvm_type(param_ty),
                field_ptr
            ));
        }

        let with_frame = self.new_temp();
        self.out.push(format!(
            "  {} = insertvalue {} undef, i8* {}, 0",
            with_frame, llvm_ret, frame_raw
        ));
        let with_poll = self.new_temp();
        self.out.push(format!(
            "  {} = insertvalue {} {}, i8* bitcast (i64 (i8*, i8*)* @{} to i8*), 1",
            with_poll, llvm_ret, with_frame, poll_name
        ));
        let repr = self.new_temp();
        self.out.push(format!(
            "  {} = insertvalue {} {}, i8* bitcast (void (i8*)* @{} to i8*), 2",
            repr, llvm_ret, with_poll, drop_name
        ));
        self.out.push(format!("  ret {} {}", llvm_ret, repr));
        self.out.push("}".to_string());
        self.out.push(String::new());
        let _ = func;
    }

    fn emit_async_poll_helper(
        &mut self,
        func: &ir::Function,
        sig: &FnSig,
        frame_plan: &AsyncFramePlan,
        poll_name: &str,
    ) -> Vec<(i32, AsyncPendingKind)> {
        let LType::Async(inner_ret) = &sig.ret else {
            return Vec::new();
        };
        let entry_label = "entry".to_string();
        let start_label = self.new_label("async_state_0");
        let completed_label = self.new_label("async_completed");
        let invalid_label = self.new_label("async_invalid");
        let dispatch_placeholder = format!("  ; __aic_async_dispatch_{poll_name}");
        let mut fctx = FnCtx {
            lines: vec![entry_label.clone() + ":"],
            vars: vec![BTreeMap::new()],
            drop_scopes: vec![DropScope::default()],
            terminated: false,
            current_label: start_label.clone(),
            ret_ty: (**inner_ret).clone(),
            async_inner_ret: None,
            debug_scope: None,
            loop_stack: Vec::new(),
            current_fn_name: poll_name.to_string(),
            current_fn_llvm_name: poll_name.to_string(),
            current_fn_sig: FnSig {
                is_extern: false,
                extern_symbol: None,
                extern_abi: None,
                is_intrinsic: false,
                intrinsic_abi: None,
                params: vec![LType::Async(Box::new((**inner_ret).clone()))],
                ret: LType::Int,
            },
            tail_return_mode: false,
            suppress_lifetime_end: true,
            async_poll_ctx: None,
        };

        let frame_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* %frame_raw to {}*",
            frame_ptr, frame_plan.frame_llvm
        ));
        let state_ptr = self.emit_async_frame_field_ptr(
            &frame_plan.frame_llvm,
            &frame_ptr,
            frame_plan.state_index,
            &mut fctx,
        );
        let await_storage_ptr = self.emit_async_frame_field_ptr(
            &frame_plan.frame_llvm,
            &frame_ptr,
            frame_plan.await_storage_index,
            &mut fctx,
        );

        let mut local_ptrs = BTreeMap::new();
        for (symbol, field_index) in &frame_plan.local_indices {
            let ptr = self.emit_async_frame_field_ptr(
                &frame_plan.frame_llvm,
                &frame_ptr,
                *field_index,
                &mut fctx,
            );
            local_ptrs.insert(*symbol, ptr);
        }
        for (idx, param) in func.params.iter().enumerate() {
            let Some(field_index) = frame_plan.param_indices.get(idx) else {
                continue;
            };
            let Some(param_ty) = sig.params.get(idx).cloned() else {
                continue;
            };
            let ptr = self.emit_async_frame_field_ptr(
                &frame_plan.frame_llvm,
                &frame_ptr,
                *field_index,
                &mut fctx,
            );
            fctx.vars.last_mut().expect("scope").insert(
                param.name.clone(),
                Local {
                    symbol: Some(param.symbol),
                    ty: param_ty,
                    ptr,
                },
            );
            fctx.drop_scopes[0].locals.insert(
                param.symbol,
                DropSlot {
                    ty: sig.params[idx].clone(),
                    ptr: fctx
                        .vars
                        .last()
                        .expect("scope")
                        .get(&param.name)
                        .expect("param")
                        .ptr
                        .clone(),
                    skip_resource_cleanup: false,
                },
            );
            fctx.drop_scopes[0].lexical_order.push(param.symbol);
        }

        fctx.lines.push(dispatch_placeholder.clone());
        fctx.lines.push(format!("{start_label}:"));

        fctx.async_poll_ctx = Some(AsyncPollCtx {
            state_ptr: state_ptr.clone(),
            await_storage_ptr,
            local_ptrs,
            state_labels: vec![(0, start_label.clone())],
            pending_states: Vec::new(),
            next_state_id: 1,
            dispatch_placeholder: dispatch_placeholder.clone(),
            completed_label: completed_label.clone(),
            invalid_label: invalid_label.clone(),
        });

        fctx.tail_return_mode = true;
        let tail = self.gen_block_with_expected_tail(&func.body, Some(inner_ret), &mut fctx);
        fctx.tail_return_mode = false;
        if !fctx.terminated {
            let value = tail.unwrap_or(Value {
                ty: (**inner_ret).clone(),
                repr: if **inner_ret == LType::Unit {
                    None
                } else {
                    Some(default_value(inner_ret))
                },
            });
            self.emit_async_poll_return(value, func.body.span, &mut fctx);
        }

        let async_ctx = fctx.async_poll_ctx.clone().expect("async poll ctx");
        if let Some(idx) = fctx
            .lines
            .iter()
            .position(|line| line == &async_ctx.dispatch_placeholder)
        {
            let dispatch = self.emit_async_dispatch_lines(&async_ctx);
            fctx.lines.splice(idx..=idx, dispatch);
        }
        fctx.lines.push(format!("{}:", completed_label));
        fctx.lines.push("  ret i64 4".to_string());
        fctx.lines.push(format!("{}:", invalid_label));
        fctx.lines.push("  ret i64 4".to_string());

        let mut lines = Vec::new();
        lines.push(format!(
            "define i64 @{}(i8* %frame_raw, i8* %out_raw) {{",
            poll_name
        ));
        lines.extend(fctx.lines);
        lines.push("}".to_string());
        self.deferred_fn_defs.push(lines);
        async_ctx
            .pending_states
            .iter()
            .map(|state| (state.state_id, state.kind))
            .collect()
    }

    fn emit_async_drop_helper(
        &mut self,
        func: &ir::Function,
        sig: &FnSig,
        frame_plan: &AsyncFramePlan,
        drop_name: &str,
        pending_states: &[(i32, AsyncPendingKind)],
    ) {
        self.extern_decls
            .insert("declare void @aic_rt_heap_free(i8*)".to_string());
        self.extern_decls
            .insert("declare void @aic_rt_async_drop(i8*, i8*)".to_string());

        let mut lines = vec![
            format!("define void @{}(i8* %frame_raw) {{", drop_name),
            "entry:".to_string(),
            format!(
                "  %frame = bitcast i8* %frame_raw to {}*",
                frame_plan.frame_llvm
            ),
        ];
        let state_ptr = self.new_temp();
        lines.push(format!(
            "  {} = getelementptr inbounds {}, {}* %frame, i32 0, i32 {}",
            state_ptr, frame_plan.frame_llvm, frame_plan.frame_llvm, frame_plan.state_index
        ));
        let state = self.new_temp();
        lines.push(format!("  {} = load i32, i32* {}", state, state_ptr));

        let await_slot_ptr = self.new_temp();
        lines.push(format!(
            "  {} = getelementptr inbounds {}, {}* %frame, i32 0, i32 {}",
            await_slot_ptr,
            frame_plan.frame_llvm,
            frame_plan.frame_llvm,
            frame_plan.await_storage_index
        ));

        let mut pending_cases = Vec::new();
        let mut async_case_fctx = FnCtx {
            lines: Vec::new(),
            vars: vec![BTreeMap::new()],
            drop_scopes: vec![DropScope::default()],
            terminated: false,
            current_label: "entry".to_string(),
            ret_ty: LType::Unit,
            async_inner_ret: None,
            debug_scope: None,
            loop_stack: Vec::new(),
            current_fn_name: drop_name.to_string(),
            current_fn_llvm_name: drop_name.to_string(),
            current_fn_sig: FnSig {
                is_extern: false,
                extern_symbol: None,
                extern_abi: None,
                is_intrinsic: false,
                intrinsic_abi: None,
                params: vec![sig.ret.clone()],
                ret: LType::Unit,
            },
            tail_return_mode: false,
            suppress_lifetime_end: true,
            async_poll_ctx: None,
        };

        let cleanup_done_label = self.new_label("async_drop_cleanup_done");
        pending_cases.push(format!(
            "  switch i32 {}, label %{} [",
            state, cleanup_done_label
        ));
        for (state_id, _) in pending_states {
            pending_cases.push(format!(
                "    i32 {}, label %async_drop_state_{}",
                state_id, state_id
            ));
        }
        pending_cases.push("  ]".to_string());
        lines.extend(pending_cases);

        for (state_id, kind) in pending_states {
            lines.push(format!("async_drop_state_{}:", state_id));
            match kind {
                AsyncPendingKind::Future => {
                    let typed = self.new_temp();
                    lines.push(format!(
                        "  {} = bitcast {}* {} to {}*",
                        typed,
                        llvm_type(&self.async_await_storage_ty()),
                        await_slot_ptr,
                        llvm_type(&LType::Async(Box::new(LType::Unit)))
                    ));
                    let slot = DropSlot {
                        ty: LType::Async(Box::new(LType::Unit)),
                        ptr: typed,
                        skip_resource_cleanup: false,
                    };
                    self.emit_async_drop_action(&slot, &mut async_case_fctx);
                }
                AsyncPendingKind::NetInt => {
                    let typed = self.new_temp();
                    lines.push(format!(
                        "  {} = bitcast {}* {} to i64*",
                        typed,
                        llvm_type(&self.async_await_storage_ty()),
                        await_slot_ptr
                    ));
                    let handle = self.new_temp();
                    lines.push(format!("  {} = load i64, i64* {}", handle, typed));
                    let cancelled = self.new_temp();
                    lines.push(format!("  {} = alloca i64", cancelled));
                    let cancel = self.new_temp();
                    lines.push(format!(
                        "  {} = call i64 @aic_rt_net_async_cancel(i64 {}, i64* {})",
                        cancel, handle, cancelled
                    ));
                    let discard = self.new_temp();
                    lines.push(format!("  {} = alloca i64", discard));
                    let wait = self.new_temp();
                    lines.push(format!(
                        "  {} = call i64 @aic_rt_net_async_wait_int(i64 {}, i64 0, i64* {})",
                        wait, handle, discard
                    ));
                    let _ = (cancel, wait);
                }
                AsyncPendingKind::NetString => {
                    let typed = self.new_temp();
                    lines.push(format!(
                        "  {} = bitcast {}* {} to i64*",
                        typed,
                        llvm_type(&self.async_await_storage_ty()),
                        await_slot_ptr
                    ));
                    let handle = self.new_temp();
                    lines.push(format!("  {} = load i64, i64* {}", handle, typed));
                    let cancelled = self.new_temp();
                    lines.push(format!("  {} = alloca i64", cancelled));
                    let cancel = self.new_temp();
                    lines.push(format!(
                        "  {} = call i64 @aic_rt_net_async_cancel(i64 {}, i64* {})",
                        cancel, handle, cancelled
                    ));
                    let out_ptr = self.new_temp();
                    lines.push(format!("  {} = alloca i8*", out_ptr));
                    let out_len = self.new_temp();
                    lines.push(format!("  {} = alloca i64", out_len));
                    let wait = self.new_temp();
                    lines.push(format!(
                            "  {} = call i64 @aic_rt_net_async_wait_string(i64 {}, i64 0, i8** {}, i64* {})",
                            wait, handle, out_ptr, out_len
                        ));
                    let _ = (cancel, wait);
                }
                AsyncPendingKind::TlsInt => {
                    let typed = self.new_temp();
                    lines.push(format!(
                        "  {} = bitcast {}* {} to i64*",
                        typed,
                        llvm_type(&self.async_await_storage_ty()),
                        await_slot_ptr
                    ));
                    let handle = self.new_temp();
                    lines.push(format!("  {} = load i64, i64* {}", handle, typed));
                    let cancelled = self.new_temp();
                    lines.push(format!("  {} = alloca i64", cancelled));
                    let cancel = self.new_temp();
                    lines.push(format!(
                        "  {} = call i64 @aic_rt_tls_async_cancel(i64 {}, i64* {})",
                        cancel, handle, cancelled
                    ));
                    let discard = self.new_temp();
                    lines.push(format!("  {} = alloca i64", discard));
                    let wait = self.new_temp();
                    lines.push(format!(
                        "  {} = call i64 @aic_rt_tls_async_wait_int(i64 {}, i64 0, i64* {})",
                        wait, handle, discard
                    ));
                    let _ = (cancel, wait);
                }
                AsyncPendingKind::TlsString => {
                    let typed = self.new_temp();
                    lines.push(format!(
                        "  {} = bitcast {}* {} to i64*",
                        typed,
                        llvm_type(&self.async_await_storage_ty()),
                        await_slot_ptr
                    ));
                    let handle = self.new_temp();
                    lines.push(format!("  {} = load i64, i64* {}", handle, typed));
                    let cancelled = self.new_temp();
                    lines.push(format!("  {} = alloca i64", cancelled));
                    let cancel = self.new_temp();
                    lines.push(format!(
                        "  {} = call i64 @aic_rt_tls_async_cancel(i64 {}, i64* {})",
                        cancel, handle, cancelled
                    ));
                    let out_ptr = self.new_temp();
                    lines.push(format!("  {} = alloca i8*", out_ptr));
                    let out_len = self.new_temp();
                    lines.push(format!("  {} = alloca i64", out_len));
                    let wait = self.new_temp();
                    lines.push(format!(
                            "  {} = call i64 @aic_rt_tls_async_wait_string(i64 {}, i64 0, i8** {}, i64* {})",
                            wait, handle, out_ptr, out_len
                        ));
                    let _ = (cancel, wait);
                }
                AsyncPendingKind::FsTask => {
                    let typed = self.new_temp();
                    lines.push(format!(
                        "  {} = bitcast {}* {} to i64*",
                        typed,
                        llvm_type(&self.async_await_storage_ty()),
                        await_slot_ptr
                    ));
                    let handle = self.new_temp();
                    lines.push(format!("  {} = load i64, i64* {}", handle, typed));
                    let cancelled = self.new_temp();
                    lines.push(format!("  {} = alloca i64", cancelled));
                    let discard = self.new_temp();
                    lines.push(format!("  {} = alloca i64", discard));
                    let cancel = self.new_temp();
                    lines.push(format!(
                        "  {} = call i64 @aic_rt_conc_cancel(i64 {}, i64* {})",
                        cancel, handle, cancelled
                    ));
                    let join = self.new_temp();
                    lines.push(format!(
                        "  {} = call i64 @aic_rt_conc_join(i64 {}, i64* {})",
                        join, handle, discard
                    ));
                    let _ = (cancel, join);
                }
            }
            lines.extend(async_case_fctx.lines.drain(..));
            lines.push(format!("  br label %{}", cleanup_done_label));
        }

        lines.push(format!("{}:", cleanup_done_label));
        let mut cleanup_fctx = FnCtx {
            lines: Vec::new(),
            vars: vec![BTreeMap::new()],
            drop_scopes: vec![DropScope::default()],
            terminated: false,
            current_label: cleanup_done_label.clone(),
            ret_ty: LType::Unit,
            async_inner_ret: None,
            debug_scope: None,
            loop_stack: Vec::new(),
            current_fn_name: drop_name.to_string(),
            current_fn_llvm_name: drop_name.to_string(),
            current_fn_sig: FnSig {
                is_extern: false,
                extern_symbol: None,
                extern_abi: None,
                is_intrinsic: false,
                intrinsic_abi: None,
                params: vec![sig.ret.clone()],
                ret: LType::Unit,
            },
            tail_return_mode: false,
            suppress_lifetime_end: true,
            async_poll_ctx: None,
        };
        for (idx, param) in func.params.iter().enumerate() {
            let Some(field_index) = frame_plan.param_indices.get(idx) else {
                continue;
            };
            let Some(param_ty) = sig.params.get(idx).cloned() else {
                continue;
            };
            let ptr = self.new_temp();
            lines.push(format!(
                "  {} = getelementptr inbounds {}, {}* %frame, i32 0, i32 {}",
                ptr, frame_plan.frame_llvm, frame_plan.frame_llvm, field_index
            ));
            cleanup_fctx.vars.last_mut().expect("scope").insert(
                param.name.clone(),
                Local {
                    symbol: Some(param.symbol),
                    ty: param_ty,
                    ptr: ptr.clone(),
                },
            );
            cleanup_fctx.drop_scopes[0].locals.insert(
                param.symbol,
                DropSlot {
                    ty: sig.params[idx].clone(),
                    ptr,
                    skip_resource_cleanup: false,
                },
            );
            cleanup_fctx.drop_scopes[0].lexical_order.push(param.symbol);
        }
        for (symbol, field_index) in &frame_plan.local_indices {
            let ptr = self.new_temp();
            lines.push(format!(
                "  {} = getelementptr inbounds {}, {}* %frame, i32 0, i32 {}",
                ptr, frame_plan.frame_llvm, frame_plan.frame_llvm, field_index
            ));
            let local_ty = frame_plan
                .local_types
                .get(symbol)
                .cloned()
                .unwrap_or(LType::Unit);
            cleanup_fctx.drop_scopes[0].locals.insert(
                *symbol,
                DropSlot {
                    ty: local_ty,
                    ptr,
                    skip_resource_cleanup: false,
                },
            );
            cleanup_fctx.drop_scopes[0].lexical_order.push(*symbol);
        }
        self.emit_scope_drops_at(0, &mut cleanup_fctx);
        lines.extend(cleanup_fctx.lines);
        lines.push("  call void @aic_rt_heap_free(i8* %frame_raw)".to_string());
        lines.push("  ret void".to_string());
        lines.push("}".to_string());
        self.deferred_fn_defs.push(lines);
        let _ = (func, sig);
    }

    pub(super) fn gen_entry_wrapper(&mut self) {
        let Some(main_sig) = self.fn_sig("main").cloned() else {
            return;
        };
        self.out
            .push("define i32 @main(i32 %argc, i8** %argv) {".to_string());
        self.out.push("entry:".to_string());
        self.out
            .push("  call void @aic_rt_stack_ensure_min(i64 67108864)".to_string());
        self.out
            .push("  call void @aic_rt_env_set_args(i32 %argc, i8** %argv)".to_string());
        match main_sig.ret {
            ref ty if is_integral_type(ty) => {
                let r = self.new_temp();
                let c = self.new_temp();
                self.out
                    .push(format!("  {} = call {} @aic_main()", r, llvm_type(ty)));
                let width = integer_width_bits(ty).unwrap_or(64);
                if width > 32 {
                    self.out
                        .push(format!("  {} = trunc {} {} to i32", c, llvm_type(ty), r));
                } else if width < 32 {
                    let cast = if is_unsigned_integer_type(ty) {
                        "zext"
                    } else {
                        "sext"
                    };
                    self.out
                        .push(format!("  {} = {} {} {} to i32", c, cast, llvm_type(ty), r));
                } else {
                    self.out.push(format!("  {} = add i32 {}, 0", c, r));
                }
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
            LType::Async(ref inner) => {
                self.extern_decls
                    .insert("declare i64 @aic_rt_async_drive(i8*, i8*, i8*)".to_string());
                self.extern_decls
                    .insert("declare void @aic_rt_async_drop(i8*, i8*)".to_string());
                let async_reg = self.new_temp();
                self.out.push(format!(
                    "  {} = call {} @aic_main()",
                    async_reg,
                    llvm_type(&main_sig.ret)
                ));
                let frame = self.new_temp();
                let poll = self.new_temp();
                let drop_fn = self.new_temp();
                self.out.push(format!(
                    "  {} = extractvalue {} {}, 0",
                    frame,
                    llvm_type(&main_sig.ret),
                    async_reg
                ));
                self.out.push(format!(
                    "  {} = extractvalue {} {}, 1",
                    poll,
                    llvm_type(&main_sig.ret),
                    async_reg
                ));
                self.out.push(format!(
                    "  {} = extractvalue {} {}, 2",
                    drop_fn,
                    llvm_type(&main_sig.ret),
                    async_reg
                ));
                match inner.as_ref() {
                    ty if is_integral_type(ty) => {
                        let out_slot = self.new_temp();
                        self.out
                            .push(format!("  {} = alloca {}", out_slot, llvm_type(ty)));
                        let out_raw = self.new_temp();
                        self.out.push(format!(
                            "  {} = bitcast {}* {} to i8*",
                            out_raw,
                            llvm_type(ty),
                            out_slot
                        ));
                        let drive_rc = self.new_temp();
                        self.out.push(format!(
                            "  {} = call i64 @aic_rt_async_drive(i8* {}, i8* {}, i8* {})",
                            drive_rc, frame, poll, out_raw
                        ));
                        let _ = drive_rc;
                        self.out.push(format!(
                            "  call void @aic_rt_async_drop(i8* {}, i8* {})",
                            frame, drop_fn
                        ));
                        let value = self.new_temp();
                        let c = self.new_temp();
                        self.out.push(format!(
                            "  {} = load {}, {}* {}",
                            value,
                            llvm_type(ty),
                            llvm_type(ty),
                            out_slot
                        ));
                        let width = integer_width_bits(ty).unwrap_or(64);
                        if width > 32 {
                            self.out.push(format!(
                                "  {} = trunc {} {} to i32",
                                c,
                                llvm_type(ty),
                                value
                            ));
                        } else if width < 32 {
                            let cast = if is_unsigned_integer_type(ty) {
                                "zext"
                            } else {
                                "sext"
                            };
                            self.out.push(format!(
                                "  {} = {} {} {} to i32",
                                c,
                                cast,
                                llvm_type(ty),
                                value
                            ));
                        } else {
                            self.out.push(format!("  {} = add i32 {}, 0", c, value));
                        }
                        self.out.push(format!("  ret i32 {}", c));
                    }
                    LType::Bool => {
                        let out_slot = self.new_temp();
                        self.out.push(format!("  {} = alloca i1", out_slot));
                        let out_raw = self.new_temp();
                        self.out
                            .push(format!("  {} = bitcast i1* {} to i8*", out_raw, out_slot));
                        let drive_rc = self.new_temp();
                        self.out.push(format!(
                            "  {} = call i64 @aic_rt_async_drive(i8* {}, i8* {}, i8* {})",
                            drive_rc, frame, poll, out_raw
                        ));
                        let _ = drive_rc;
                        self.out.push(format!(
                            "  call void @aic_rt_async_drop(i8* {}, i8* {})",
                            frame, drop_fn
                        ));
                        let value = self.new_temp();
                        let c = self.new_temp();
                        self.out
                            .push(format!("  {} = load i1, i1* {}", value, out_slot));
                        self.out.push(format!("  {} = zext i1 {} to i32", c, value));
                        self.out.push(format!("  ret i32 {}", c));
                    }
                    LType::Unit => {
                        let drive_rc = self.new_temp();
                        self.out.push(format!(
                            "  {} = call i64 @aic_rt_async_drive(i8* {}, i8* {}, i8* null)",
                            drive_rc, frame, poll
                        ));
                        let _ = drive_rc;
                        self.out.push(format!(
                            "  call void @aic_rt_async_drop(i8* {}, i8* {})",
                            frame, drop_fn
                        ));
                        self.out.push("  ret i32 0".to_string());
                    }
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E5020",
                            "async main must return Async[Int], Async[Bool], or Async[()]",
                            self.file,
                            crate::span::Span::new(0, 0),
                        ));
                        self.out.push("  ret i32 1".to_string());
                    }
                }
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

    fn async_await_storage_ty(&self) -> LType {
        LType::Struct(StructLayoutType {
            repr: "__AsyncAwaitStorage".to_string(),
            fields: vec![
                StructFieldType {
                    name: "w0".to_string(),
                    ty: LType::Int,
                },
                StructFieldType {
                    name: "w1".to_string(),
                    ty: LType::Int,
                },
                StructFieldType {
                    name: "w2".to_string(),
                    ty: LType::Int,
                },
                StructFieldType {
                    name: "w3".to_string(),
                    ty: LType::Int,
                },
            ],
        })
    }

    fn collect_async_locals_in_block(
        &mut self,
        block: &ir::Block,
        locals: &mut Vec<(ir::SymbolId, String, LType)>,
    ) -> Option<()> {
        for stmt in &block.stmts {
            match stmt {
                ir::Stmt::Let {
                    symbol,
                    name,
                    ty,
                    span,
                    expr,
                    ..
                } => {
                    let Some(ty_id) = ty else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5020",
                            format!(
                                "async lowering requires a concrete local type for '{}'",
                                name
                            ),
                            self.file,
                            *span,
                        ));
                        return None;
                    };
                    let local_ty = self.type_from_id(*ty_id, *span)?;
                    locals.push((*symbol, name.clone(), local_ty));
                    self.collect_async_locals_in_expr(expr, locals)?;
                }
                ir::Stmt::Assign { expr, .. }
                | ir::Stmt::Expr { expr, .. }
                | ir::Stmt::Assert { expr, .. } => {
                    self.collect_async_locals_in_expr(expr, locals)?;
                }
                ir::Stmt::Return { expr, .. } => {
                    if let Some(expr) = expr {
                        self.collect_async_locals_in_expr(expr, locals)?;
                    }
                }
            }
        }
        if let Some(tail) = &block.tail {
            self.collect_async_locals_in_expr(tail, locals)?;
        }
        Some(())
    }

    fn collect_async_locals_in_expr(
        &mut self,
        expr: &ir::Expr,
        locals: &mut Vec<(ir::SymbolId, String, LType)>,
    ) -> Option<()> {
        match &expr.kind {
            ir::ExprKind::Call { callee, args, .. } => {
                self.collect_async_locals_in_expr(callee, locals)?;
                for arg in args {
                    self.collect_async_locals_in_expr(arg, locals)?;
                }
            }
            ir::ExprKind::Unary { expr: inner, .. }
            | ir::ExprKind::Borrow { expr: inner, .. }
            | ir::ExprKind::Await { expr: inner }
            | ir::ExprKind::Try { expr: inner } => {
                self.collect_async_locals_in_expr(inner, locals)?;
            }
            ir::ExprKind::Binary { lhs, rhs, .. } => {
                self.collect_async_locals_in_expr(lhs, locals)?;
                self.collect_async_locals_in_expr(rhs, locals)?;
            }
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                self.collect_async_locals_in_expr(cond, locals)?;
                self.collect_async_locals_in_block(then_block, locals)?;
                self.collect_async_locals_in_block(else_block, locals)?;
            }
            ir::ExprKind::While { cond, body } => {
                self.collect_async_locals_in_expr(cond, locals)?;
                self.collect_async_locals_in_block(body, locals)?;
            }
            ir::ExprKind::Loop { body } | ir::ExprKind::UnsafeBlock { block: body } => {
                self.collect_async_locals_in_block(body, locals)?;
            }
            ir::ExprKind::Break { expr } => {
                if let Some(expr) = expr {
                    self.collect_async_locals_in_expr(expr, locals)?;
                }
            }
            ir::ExprKind::Match { expr, arms } => {
                self.collect_async_locals_in_expr(expr, locals)?;
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        self.collect_async_locals_in_expr(guard, locals)?;
                    }
                    self.collect_async_locals_in_expr(&arm.body, locals)?;
                }
            }
            ir::ExprKind::StructInit { fields, .. } => {
                for (_, value, _) in fields {
                    self.collect_async_locals_in_expr(value, locals)?;
                }
            }
            ir::ExprKind::FieldAccess { base, .. } => {
                self.collect_async_locals_in_expr(base, locals)?;
            }
            ir::ExprKind::Closure { .. }
            | ir::ExprKind::Var(_)
            | ir::ExprKind::Int(_)
            | ir::ExprKind::Float(_)
            | ir::ExprKind::Bool(_)
            | ir::ExprKind::Char(_)
            | ir::ExprKind::String(_)
            | ir::ExprKind::TemplateLiteral { .. }
            | ir::ExprKind::Unit
            | ir::ExprKind::Continue => {}
        }
        Some(())
    }

    fn build_async_frame_plan(
        &mut self,
        func: &ir::Function,
        sig: &FnSig,
        frame_repr: &str,
    ) -> Option<AsyncFramePlan> {
        let mut fields = Vec::new();
        fields.push(StructFieldType {
            name: "state".to_string(),
            ty: LType::Int32,
        });
        fields.push(StructFieldType {
            name: "await_slot".to_string(),
            ty: self.async_await_storage_ty(),
        });

        let mut param_indices = Vec::with_capacity(sig.params.len());
        for (idx, ty) in sig.params.iter().enumerate() {
            param_indices.push(fields.len());
            fields.push(StructFieldType {
                name: format!("arg{idx}"),
                ty: ty.clone(),
            });
        }

        let mut collected_locals = Vec::new();
        self.collect_async_locals_in_block(&func.body, &mut collected_locals)?;
        let mut local_indices = BTreeMap::new();
        let mut local_types = BTreeMap::new();
        for (symbol, name, ty) in collected_locals {
            let field_index = fields.len();
            fields.push(StructFieldType {
                name: format!("{}_{}", name, symbol.0),
                ty: ty.clone(),
            });
            local_indices.insert(symbol, field_index);
            local_types.insert(symbol, ty);
        }

        let frame_ty = LType::Struct(StructLayoutType {
            repr: frame_repr.to_string(),
            fields,
        });
        let frame_llvm = llvm_type(&frame_ty);
        Some(AsyncFramePlan {
            frame_ty,
            frame_llvm,
            state_index: 0,
            await_storage_index: 1,
            param_indices,
            local_indices,
            local_types,
        })
    }

    fn emit_async_frame_field_ptr(
        &mut self,
        frame_llvm: &str,
        frame_ptr: &str,
        field_index: usize,
        fctx: &mut FnCtx,
    ) -> String {
        let ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds {}, {}* {}, i32 0, i32 {}",
            ptr, frame_llvm, frame_llvm, frame_ptr, field_index
        ));
        ptr
    }

    fn emit_async_poll_return(&mut self, value: Value, span: crate::span::Span, fctx: &mut FnCtx) {
        let Some(state_ptr) = fctx
            .async_poll_ctx
            .as_ref()
            .map(|async_ctx| async_ctx.state_ptr.clone())
        else {
            return;
        };
        let ret_ty = fctx.ret_ty.clone();
        let Some(value) = self.coerce_value_to_expected(value, &ret_ty, span, fctx) else {
            fctx.lines.push("  ret i64 4".to_string());
            fctx.terminated = true;
            return;
        };
        if ret_ty != LType::Unit {
            let out_typed = self.new_temp();
            fctx.lines.push(format!(
                "  {} = bitcast i8* %out_raw to {}*",
                out_typed,
                llvm_type(&ret_ty)
            ));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                llvm_type(&ret_ty),
                coerce_repr(&value, &ret_ty),
                llvm_type(&ret_ty),
                out_typed
            ));
        }
        fctx.lines
            .push(format!("  store i32 -1, i32* {}", state_ptr));
        fctx.lines.push("  ret i64 0".to_string());
        fctx.terminated = true;
    }

    fn emit_async_dispatch_lines(&self, async_ctx: &AsyncPollCtx) -> Vec<String> {
        let mut lines = vec![format!(
            "  %async_state = load i32, i32* {}",
            async_ctx.state_ptr
        )];
        let mut cases = vec![
            format!("    i32 -1, label %{}", async_ctx.completed_label),
            format!("    i32 0, label %{}", async_ctx.state_labels[0].1),
        ];
        cases.extend(
            async_ctx
                .pending_states
                .iter()
                .map(|state| format!("    i32 {}, label %{}", state.state_id, state.label)),
        );
        lines.push(format!(
            "  switch i32 %async_state, label %{} [",
            async_ctx.invalid_label
        ));
        lines.extend(cases);
        lines.push("  ]".to_string());
        lines
    }

    fn register_async_pending_state(
        &mut self,
        kind: AsyncPendingKind,
        fctx: &mut FnCtx,
    ) -> Option<(i32, String)> {
        let label = self.new_label("async_resume");
        let async_ctx = fctx.async_poll_ctx.as_mut()?;
        let state_id = async_ctx.next_state_id;
        async_ctx.next_state_id += 1;
        async_ctx.state_labels.push((state_id, label.clone()));
        async_ctx.pending_states.push(AsyncPendingState {
            state_id,
            label: label.clone(),
            kind,
        });
        Some((state_id, label))
    }

    fn async_storage_ptr_for_type(&mut self, ty: &LType, fctx: &mut FnCtx) -> Option<String> {
        let async_ctx = fctx.async_poll_ctx.as_ref()?;
        let ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast {}* {} to {}*",
            ptr,
            llvm_type(&self.async_await_storage_ty()),
            async_ctx.await_storage_ptr,
            llvm_type(ty)
        ));
        Some(ptr)
    }

    pub(super) fn gen_block(&mut self, block: &ir::Block, fctx: &mut FnCtx) -> Option<Value> {
        self.gen_block_with_expected_tail(block, None, fctx)
    }

    pub(super) fn gen_block_with_expected_tail(
        &mut self,
        block: &ir::Block,
        expected_tail: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let inherited_tail_return_mode = fctx.tail_return_mode;
        fctx.tail_return_mode = false;
        fctx.vars.push(BTreeMap::new());
        fctx.drop_scopes.push(DropScope {
            lexical_order: lexical_block_drop_order(block),
            locals: BTreeMap::new(),
        });

        for stmt in &block.stmts {
            if fctx.terminated {
                break;
            }
            match stmt {
                ir::Stmt::Let {
                    symbol,
                    name,
                    ty,
                    expr,
                    span,
                    ..
                } => {
                    let expected = if let Some(ty) = ty {
                        self.type_from_id(*ty, *span)
                    } else {
                        None
                    };
                    let value = self.gen_expr_with_expected(expr, expected.as_ref(), fctx);
                    let Some(value) = value else {
                        // Preserve the local slot shape after a failed initializer so
                        // later statements do not cascade into unrelated unknown-local
                        // diagnostics while backend errors are already being reported.
                        if let Some(expected) = expected.clone() {
                            let ptr = fctx
                                .async_poll_ctx
                                .as_ref()
                                .and_then(|async_ctx| async_ctx.local_ptrs.get(symbol).cloned())
                                .unwrap_or_else(|| {
                                    let ptr = self.new_temp();
                                    fctx.lines.push(format!(
                                        "  {} = alloca {}",
                                        ptr,
                                        llvm_type(&expected)
                                    ));
                                    ptr
                                });
                            fctx.lines.push(format!(
                                "  store {} {}, {}* {}",
                                llvm_type(&expected),
                                default_value(&expected),
                                llvm_type(&expected),
                                ptr
                            ));
                            fctx.vars.last_mut().expect("scope").insert(
                                name.clone(),
                                Local {
                                    symbol: Some(*symbol),
                                    ty: expected.clone(),
                                    ptr: ptr.clone(),
                                },
                            );
                            if let Some(scope) = fctx.drop_scopes.last_mut() {
                                scope.locals.insert(
                                    *symbol,
                                    DropSlot {
                                        ty: expected,
                                        ptr,
                                        skip_resource_cleanup: true,
                                    },
                                );
                            }
                        }
                        continue;
                    };
                    let expected = expected.or_else(|| Some(value.ty.clone()));
                    let Some(expected) = expected else {
                        continue;
                    };
                    let Some(value) = self.coerce_value_to_expected(value, &expected, *span, fctx)
                    else {
                        continue;
                    };
                    if value.ty != expected {
                        self.diagnostics.push(Diagnostic::error(
                            "E5007",
                            format!(
                                "let codegen type mismatch for '{}': expected '{}', found '{}'",
                                name,
                                render_type(&expected),
                                render_type(&value.ty)
                            ),
                            self.file,
                            *span,
                        ));
                    }
                    let mut skip_resource_cleanup = false;
                    if self.type_needs_explicit_drop(&expected) {
                        if let ir::ExprKind::Var(source) = &expr.kind {
                            if let Some(source_local) = find_local(&fctx.vars, source) {
                                if source_local.symbol.is_some() {
                                    self.mark_local_resource_moved(source, fctx);
                                } else {
                                    skip_resource_cleanup = true;
                                }
                            }
                        }
                    }
                    self.mark_moved_resource_locals_in_expr(expr, fctx);
                    let ptr = fctx
                        .async_poll_ctx
                        .as_ref()
                        .and_then(|async_ctx| async_ctx.local_ptrs.get(symbol).cloned())
                        .unwrap_or_else(|| {
                            let ptr = self.new_temp();
                            fctx.lines
                                .push(format!("  {} = alloca {}", ptr, llvm_type(&expected)));
                            ptr
                        });
                    let repr = coerce_repr(&value, &expected);
                    fctx.lines.push(format!(
                        "  store {} {}, {}* {}",
                        llvm_type(&expected),
                        repr,
                        llvm_type(&expected),
                        ptr
                    ));
                    fctx.vars.last_mut().expect("scope").insert(
                        name.clone(),
                        Local {
                            symbol: Some(*symbol),
                            ty: expected.clone(),
                            ptr: ptr.clone(),
                        },
                    );
                    if let Some(scope) = fctx.drop_scopes.last_mut() {
                        scope.locals.insert(
                            *symbol,
                            DropSlot {
                                ty: expected,
                                ptr,
                                skip_resource_cleanup,
                            },
                        );
                    }
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
                    let local_ty = local.ty.clone();
                    let Some(value) = self.gen_expr_with_expected(expr, Some(&local_ty), fctx)
                    else {
                        continue;
                    };
                    let Some(value) = self.coerce_value_to_expected(value, &local.ty, *span, fctx)
                    else {
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
                        let ret_hint = fctx.ret_ty.clone();
                        if self.type_needs_explicit_drop(&ret_hint) {
                            self.mark_moved_resource_locals_in_expr(expr, fctx);
                        }
                        if self.try_emit_musttail_return(expr, fctx) {
                            continue;
                        }
                        if let Some(value) =
                            self.gen_expr_with_expected(expr, Some(&ret_hint), fctx)
                        {
                            self.emit_scope_drops_to_depth(0, fctx);
                            if fctx.async_poll_ctx.is_some() {
                                self.emit_async_poll_return(value, expr.span, fctx);
                            } else if let Some(async_inner) = fctx.async_inner_ret.clone() {
                                let Some(value) = self.coerce_value_to_expected(
                                    value,
                                    &async_inner,
                                    expr.span,
                                    fctx,
                                ) else {
                                    fctx.terminated = true;
                                    continue;
                                };
                                let async_value =
                                    self.build_ready_async_value(value, &async_inner, fctx);
                                let repr = async_value
                                    .repr
                                    .unwrap_or_else(|| default_value(&async_value.ty));
                                fctx.lines.push(format!(
                                    "  ret {} {}",
                                    llvm_type(&async_value.ty),
                                    repr
                                ));
                            } else {
                                let Some(value) = self
                                    .coerce_value_to_expected(value, &ret_hint, expr.span, fctx)
                                else {
                                    fctx.terminated = true;
                                    continue;
                                };
                                let repr = coerce_repr(&value, &ret_hint);
                                fctx.lines
                                    .push(format!("  ret {} {}", llvm_type(&ret_hint), repr));
                            }
                            fctx.terminated = true;
                        }
                    } else {
                        self.emit_scope_drops_to_depth(0, fctx);
                        if fctx.async_poll_ctx.is_some() {
                            self.emit_async_poll_return(
                                Value {
                                    ty: LType::Unit,
                                    repr: None,
                                },
                                block.span,
                                fctx,
                            );
                        } else if let Some(async_inner) = fctx.async_inner_ret.clone() {
                            let ready = self.build_ready_async_value(
                                Value {
                                    ty: async_inner.clone(),
                                    repr: if async_inner == LType::Unit {
                                        None
                                    } else {
                                        Some(default_value(&async_inner))
                                    },
                                },
                                &async_inner,
                                fctx,
                            );
                            fctx.lines.push(format!(
                                "  ret {} {}",
                                llvm_type(&ready.ty),
                                ready.repr.unwrap_or_else(|| default_value(&ready.ty))
                            ));
                        } else {
                            fctx.lines.push("  ret void".to_string());
                        }
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
                let previous_tail_mode = fctx.tail_return_mode;
                fctx.tail_return_mode = inherited_tail_return_mode;
                let tail = if inherited_tail_return_mode
                    && self.try_emit_musttail_tail_expr_return(expr, fctx)
                {
                    None
                } else {
                    if let Some(expected_tail) = expected_tail {
                        if self.type_needs_explicit_drop(expected_tail) {
                            self.mark_moved_resource_locals_in_expr(expr, fctx);
                        }
                    }
                    self.gen_expr_with_expected(expr, expected_tail, fctx)
                };
                fctx.tail_return_mode = previous_tail_mode;
                tail
            } else {
                Some(Value {
                    ty: LType::Unit,
                    repr: None,
                })
            }
        } else {
            None
        };

        if !fctx.terminated {
            let scope_index = fctx.drop_scopes.len().saturating_sub(1);
            self.emit_scope_drops_at(scope_index, fctx);
        }
        fctx.drop_scopes.pop();
        fctx.vars.pop();
        fctx.tail_return_mode = inherited_tail_return_mode;
        tail
    }

    pub(super) fn emit_scope_drops_to_depth(&mut self, min_depth: usize, fctx: &mut FnCtx) {
        let start = min_depth.min(fctx.drop_scopes.len());
        for scope_index in (start..fctx.drop_scopes.len()).rev() {
            self.emit_scope_drops_at(scope_index, fctx);
        }
    }

    pub(super) fn emit_scope_drops_at(&mut self, scope_index: usize, fctx: &mut FnCtx) {
        let Some(scope) = fctx.drop_scopes.get(scope_index).cloned() else {
            return;
        };
        for symbol in scope.lexical_order.iter().rev() {
            let Some(local) = scope.locals.get(symbol) else {
                continue;
            };
            if !local.skip_resource_cleanup {
                if matches!(local.ty, LType::Async(_)) {
                    self.emit_async_drop_action(local, fctx);
                } else if let Some(drop_method) = self.drop_impl_method_for_type(&local.ty) {
                    self.emit_trait_drop_action(&drop_method, local, fctx);
                } else if let Some(action) = resource_drop_action_for_type(&local.ty) {
                    self.emit_resource_drop_action(action, local, fctx);
                }
            }
            if type_has_runtime_drop(&local.ty) && !fctx.suppress_lifetime_end {
                let cast = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = bitcast {}* {} to i8*",
                    cast,
                    llvm_type(&local.ty),
                    local.ptr
                ));
                fctx.lines.push(format!(
                    "  call void @llvm.lifetime.end.p0i8(i64 -1, i8* {})",
                    cast
                ));
            }
        }
    }

    pub(super) fn mark_local_resource_moved(&mut self, name: &str, fctx: &mut FnCtx) {
        let Some(local) = find_local(&fctx.vars, name) else {
            return;
        };
        if !self.type_needs_explicit_drop(&local.ty) {
            return;
        }
        let Some(symbol) = local.symbol else {
            return;
        };
        for scope in fctx.drop_scopes.iter_mut().rev() {
            let Some(slot) = scope.locals.get_mut(&symbol) else {
                continue;
            };
            slot.skip_resource_cleanup = true;
            break;
        }
    }

    pub(super) fn mark_moved_resource_locals_in_block(
        &mut self,
        block: &ir::Block,
        fctx: &mut FnCtx,
    ) {
        for stmt in &block.stmts {
            match stmt {
                ir::Stmt::Let { expr, .. }
                | ir::Stmt::Assign { expr, .. }
                | ir::Stmt::Expr { expr, .. }
                | ir::Stmt::Assert { expr, .. } => {
                    self.mark_moved_resource_locals_in_expr(expr, fctx);
                }
                ir::Stmt::Return { expr, .. } => {
                    if let Some(expr) = expr {
                        self.mark_moved_resource_locals_in_expr(expr, fctx);
                    }
                }
            }
        }
        if let Some(tail) = &block.tail {
            self.mark_moved_resource_locals_in_expr(tail, fctx);
        }
    }

    pub(super) fn mark_moved_resource_locals_in_expr(&mut self, expr: &ir::Expr, fctx: &mut FnCtx) {
        match &expr.kind {
            ir::ExprKind::Var(name) => self.mark_local_resource_moved(name, fctx),
            ir::ExprKind::Call { callee, args, .. } => {
                self.mark_moved_resource_locals_in_expr(callee, fctx);
                for arg in args {
                    self.mark_moved_resource_locals_in_expr(arg, fctx);
                }
            }
            ir::ExprKind::StructInit { fields, .. } => {
                for (_, value, _) in fields {
                    self.mark_moved_resource_locals_in_expr(value, fctx);
                }
            }
            ir::ExprKind::FieldAccess { base, field } => {
                if let ir::ExprKind::Var(name) = &base.kind {
                    if let Some(local) = find_local(&fctx.vars, name) {
                        if let LType::Struct(layout) = &local.ty {
                            if let Some(field_layout) =
                                layout.fields.iter().find(|f| f.name == *field)
                            {
                                if self.type_needs_explicit_drop(&field_layout.ty) {
                                    self.mark_local_resource_moved(name, fctx);
                                }
                                return;
                            }
                        }
                    }
                }
                self.mark_moved_resource_locals_in_expr(base, fctx);
            }
            ir::ExprKind::Binary { lhs, rhs, .. } => {
                self.mark_moved_resource_locals_in_expr(lhs, fctx);
                self.mark_moved_resource_locals_in_expr(rhs, fctx);
            }
            ir::ExprKind::Unary { expr, .. }
            | ir::ExprKind::Borrow { expr, .. }
            | ir::ExprKind::Await { expr }
            | ir::ExprKind::Try { expr } => {
                self.mark_moved_resource_locals_in_expr(expr, fctx);
            }
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                self.mark_moved_resource_locals_in_expr(cond, fctx);
                self.mark_moved_resource_locals_in_block(then_block, fctx);
                self.mark_moved_resource_locals_in_block(else_block, fctx);
            }
            ir::ExprKind::Match { expr, arms } => {
                self.mark_moved_resource_locals_in_expr(expr, fctx);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        self.mark_moved_resource_locals_in_expr(guard, fctx);
                    }
                    self.mark_moved_resource_locals_in_expr(&arm.body, fctx);
                }
            }
            ir::ExprKind::While { cond, body } => {
                self.mark_moved_resource_locals_in_expr(cond, fctx);
                self.mark_moved_resource_locals_in_block(body, fctx);
            }
            ir::ExprKind::Loop { body } | ir::ExprKind::UnsafeBlock { block: body } => {
                self.mark_moved_resource_locals_in_block(body, fctx);
            }
            ir::ExprKind::Break { expr } => {
                if let Some(expr) = expr {
                    self.mark_moved_resource_locals_in_expr(expr, fctx);
                }
            }
            _ => {}
        }
    }

    pub(super) fn ensure_async_ready_helpers(&mut self, inner_ty: &LType) -> (String, String) {
        let key = render_type(inner_ty);
        if let Some(existing) = self.async_ready_helpers.get(&key) {
            return existing.clone();
        }

        self.extern_decls
            .insert("declare void @aic_rt_heap_free(i8*)".to_string());

        let suffix = self.async_counter;
        self.async_counter += 1;
        let poll_name = format!("__aic_async_ready_poll_{suffix}");
        let drop_name = format!("__aic_async_ready_drop_{suffix}");
        let frame_ty = if *inner_ty == LType::Unit {
            "{ i1 }".to_string()
        } else {
            format!("{{ i1, {} }}", llvm_type(inner_ty))
        };

        let mut poll_lines = vec![
            format!("define i64 @{}(i8* %frame_raw, i8* %out_raw) {{", poll_name),
            "entry:".to_string(),
            format!("  %frame = bitcast i8* %frame_raw to {}*", frame_ty),
            format!("  %loaded = load {}, {}* %frame", frame_ty, frame_ty),
            format!("  %consumed = extractvalue {} %loaded, 0", frame_ty),
            "  %was_ready = xor i1 %consumed, true".to_string(),
            "  br i1 %was_ready, label %mark_ready, label %done".to_string(),
            "mark_ready:".to_string(),
        ];
        if *inner_ty != LType::Unit {
            poll_lines.push(format!("  %payload = extractvalue {} %loaded, 1", frame_ty));
            poll_lines.push(format!(
                "  %out_typed = bitcast i8* %out_raw to {}*",
                llvm_type(inner_ty)
            ));
            poll_lines.push(format!(
                "  store {} %payload, {}* %out_typed",
                llvm_type(inner_ty),
                llvm_type(inner_ty)
            ));
        }
        poll_lines.push(format!(
            "  %updated = insertvalue {} %loaded, i1 1, 0",
            frame_ty
        ));
        poll_lines.push(format!(
            "  store {} %updated, {}* %frame",
            frame_ty, frame_ty
        ));
        poll_lines.push("  br label %done".to_string());
        poll_lines.push("done:".to_string());
        poll_lines.push("  ret i64 0".to_string());
        poll_lines.push("}".to_string());

        let mut drop_lines = vec![
            format!("define void @{}(i8* %frame_raw) {{", drop_name),
            "entry:".to_string(),
            format!("  %frame = bitcast i8* %frame_raw to {}*", frame_ty),
            format!("  %loaded = load {}, {}* %frame", frame_ty, frame_ty),
        ];
        if *inner_ty != LType::Unit && self.type_needs_explicit_drop(inner_ty) {
            drop_lines.push(format!(
                "  %consumed = extractvalue {} %loaded, 0",
                frame_ty
            ));
            drop_lines
                .push("  br i1 %consumed, label %free_frame, label %drop_payload".to_string());
            drop_lines.push("drop_payload:".to_string());
            drop_lines.push(format!("  %payload = extractvalue {} %loaded, 1", frame_ty));
            drop_lines.push(format!("  %payload_slot = alloca {}", llvm_type(inner_ty)));
            drop_lines.push(format!(
                "  store {} %payload, {}* %payload_slot",
                llvm_type(inner_ty),
                llvm_type(inner_ty)
            ));
            let mut helper_ctx = FnCtx {
                lines: Vec::new(),
                vars: Vec::new(),
                drop_scopes: Vec::new(),
                terminated: false,
                current_label: "drop_payload".to_string(),
                ret_ty: LType::Unit,
                async_inner_ret: None,
                debug_scope: None,
                loop_stack: Vec::new(),
                current_fn_name: drop_name.clone(),
                current_fn_llvm_name: drop_name.clone(),
                current_fn_sig: FnSig {
                    is_extern: false,
                    extern_symbol: None,
                    extern_abi: None,
                    is_intrinsic: false,
                    intrinsic_abi: None,
                    params: vec![LType::Async(Box::new(inner_ty.clone()))],
                    ret: LType::Unit,
                },
                tail_return_mode: false,
                suppress_lifetime_end: true,
                async_poll_ctx: None,
            };
            let slot = DropSlot {
                ty: inner_ty.clone(),
                ptr: "%payload_slot".to_string(),
                skip_resource_cleanup: false,
            };
            if matches!(inner_ty, LType::Async(_)) {
                self.emit_async_drop_action(&slot, &mut helper_ctx);
            } else if let Some(drop_method) = self.drop_impl_method_for_type(inner_ty) {
                self.emit_trait_drop_action(&drop_method, &slot, &mut helper_ctx);
            } else if let Some(action) = resource_drop_action_for_type(inner_ty) {
                self.emit_resource_drop_action(action, &slot, &mut helper_ctx);
            }
            drop_lines.extend(helper_ctx.lines);
            drop_lines.push("  br label %free_frame".to_string());
            drop_lines.push("free_frame:".to_string());
        }
        drop_lines.push("  call void @aic_rt_heap_free(i8* %frame_raw)".to_string());
        drop_lines.push("  ret void".to_string());
        drop_lines.push("}".to_string());

        self.deferred_fn_defs.push(poll_lines);
        self.deferred_fn_defs.push(drop_lines);
        self.async_ready_helpers
            .insert(key, (poll_name.clone(), drop_name.clone()));
        (poll_name, drop_name)
    }

    pub(super) fn build_ready_async_value(
        &mut self,
        value: Value,
        inner_ty: &LType,
        fctx: &mut FnCtx,
    ) -> Value {
        self.extern_decls
            .insert("declare i8* @malloc(i64)".to_string());
        let (poll_name, drop_name) = self.ensure_async_ready_helpers(inner_ty);
        let async_ty = LType::Async(Box::new(inner_ty.clone()));
        let frame_ty = if *inner_ty == LType::Unit {
            "{ i1 }".to_string()
        } else {
            format!("{{ i1, {} }}", llvm_type(inner_ty))
        };
        let frame_size_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds {}, {}* null, i32 1",
            frame_size_ptr, frame_ty, frame_ty
        ));
        let frame_size = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint {}* {} to i64",
            frame_size, frame_ty, frame_size_ptr
        ));
        let frame_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i8* @malloc(i64 {})",
            frame_raw, frame_size
        ));
        let frame_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to {}*",
            frame_ptr, frame_raw, frame_ty
        ));

        let frame_value = if *inner_ty == LType::Unit {
            let ready = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} undef, i1 0, 0",
                ready, frame_ty
            ));
            ready
        } else {
            let ready = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} undef, i1 0, 0",
                ready, frame_ty
            ));
            let with_payload = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} {}, {} {}, 1",
                with_payload,
                frame_ty,
                ready,
                llvm_type(inner_ty),
                coerce_repr(&value, inner_ty)
            ));
            with_payload
        };
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            frame_ty, frame_value, frame_ty, frame_ptr
        ));

        let with_frame = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} undef, i8* {}, 0",
            with_frame,
            llvm_type(&async_ty),
            frame_raw
        ));
        let with_poll = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} {}, i8* bitcast (i64 (i8*, i8*)* @{} to i8*), 1",
            with_poll,
            llvm_type(&async_ty),
            with_frame,
            poll_name
        ));
        let repr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} {}, i8* bitcast (void (i8*)* @{} to i8*), 2",
            repr,
            llvm_type(&async_ty),
            with_poll,
            drop_name
        ));

        Value {
            ty: async_ty,
            repr: Some(repr),
        }
    }

    pub(super) fn load_boxed_runtime_value(
        &mut self,
        value_ty: &LType,
        boxed_value: &str,
        label_prefix: &str,
        _span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if *value_ty == LType::Unit {
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        self.extern_decls
            .insert("declare void @aic_rt_heap_free(i8*)".to_string());
        let value_llvm = llvm_type(value_ty);
        let value_stack = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca {}", value_stack, value_llvm));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            value_llvm,
            default_value(value_ty),
            value_llvm,
            value_stack
        ));

        let has_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", has_ptr, boxed_value));
        let load_label = self.new_label(&format!("{label_prefix}_load"));
        let cont_label = self.new_label(&format!("{label_prefix}_cont"));
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            has_ptr, load_label, cont_label
        ));
        fctx.lines.push(format!("{}:", load_label));
        let out_ptr_i8 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = inttoptr i64 {} to i8*",
            out_ptr_i8, boxed_value
        ));
        let typed_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to {}*",
            typed_ptr, out_ptr_i8, value_llvm
        ));
        let loaded_value = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            loaded_value, value_llvm, value_llvm, typed_ptr
        ));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            value_llvm, loaded_value, value_llvm, value_stack
        ));
        fctx.lines
            .push(format!("  call void @aic_rt_heap_free(i8* {})", out_ptr_i8));
        fctx.lines.push(format!("  br label %{}", cont_label));
        fctx.lines.push(format!("{}:", cont_label));
        let final_value = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            final_value, value_llvm, value_llvm, value_stack
        ));
        Some(Value {
            ty: value_ty.clone(),
            repr: Some(final_value),
        })
    }

    pub(super) fn classify_await_submit_result(
        &mut self,
        ty: &LType,
        span: crate::span::Span,
    ) -> Option<(usize, usize, LType, LType, LType, String, String)> {
        let LType::Enum(layout) = ty else {
            return None;
        };
        if base_type_name(&layout.repr) != "Result" {
            return None;
        }
        let ok_idx = layout
            .variants
            .iter()
            .position(|variant| variant.name == "Ok")?;
        let err_idx = layout
            .variants
            .iter()
            .position(|variant| variant.name == "Err")?;
        let ok_payload_ty = layout.variants.get(ok_idx)?.payload.clone()?;
        let err_payload_ty = layout.variants.get(err_idx)?.payload.clone()?;
        let LType::Enum(err_layout) = &err_payload_ty else {
            return None;
        };
        let err_name = method_base_name(base_type_name(&err_layout.repr));
        let LType::Struct(op_layout) = &ok_payload_ty else {
            return None;
        };
        let op_name = method_base_name(base_type_name(&op_layout.repr));
        let output_ty = match (op_name, err_name) {
            ("AsyncIntOp", "NetError") => self.parse_type_repr("Result[Int, NetError]", span)?,
            ("AsyncStringOp", "NetError") => {
                self.parse_type_repr("Result[Bytes, NetError]", span)?
            }
            ("AsyncIntOp", "TlsError") => self.parse_type_repr("Result[Int, TlsError]", span)?,
            ("AsyncStringOp", "TlsError") => {
                self.parse_type_repr("Result[Bytes, TlsError]", span)?
            }
            ("AsyncFsBoolOp", "FsError") => self.parse_type_repr("Result[Bool, FsError]", span)?,
            ("AsyncFsTextOp", "FsError") => {
                self.parse_type_repr("Result[String, FsError]", span)?
            }
            ("AsyncFsBytesOp", "FsError") => {
                self.parse_type_repr("Result[Bytes, FsError]", span)?
            }
            _ => return None,
        };
        let op_name = op_name.to_string();
        let err_name = err_name.to_string();
        Some((
            ok_idx,
            err_idx,
            ok_payload_ty,
            err_payload_ty,
            output_ty,
            op_name,
            err_name,
        ))
    }

    pub(super) fn gen_await_submit_result(
        &mut self,
        submitted: Value,
        ok_idx: usize,
        err_idx: usize,
        submit_ok_payload_ty: LType,
        submit_err_payload_ty: LType,
        output_ty: LType,
        op_name: String,
        err_name: String,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let submitted_repr = submitted
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&submitted.ty));
        let Some((output_layout, output_ok_ty, output_err_ty, _, output_err_idx)) =
            self.result_layout_parts(&output_ty, span)
        else {
            return None;
        };
        if output_err_ty != submit_err_payload_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5002",
                "await submit bridge requires matching submit error payload type",
                self.file,
                span,
            ));
            return None;
        }

        let out_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca {}", out_slot, llvm_type(&output_ty)));

        let tag = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            tag,
            llvm_type(&submitted.ty),
            submitted_repr
        ));
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i32 {}, {}", is_ok, tag, ok_idx));

        let ok_label = self.new_label("await_submit_ok");
        let err_label = self.new_label("await_submit_err");
        let done_label = self.new_label("await_submit_done");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", err_label));
        let err_payload = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            err_payload,
            llvm_type(&submitted.ty),
            submitted_repr,
            err_idx + 1
        ));
        let err_value = self.build_enum_variant(
            &output_layout,
            output_err_idx,
            Some(Value {
                ty: submit_err_payload_ty.clone(),
                repr: Some(err_payload),
            }),
            span,
            fctx,
        )?;
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&output_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&output_ty)),
            llvm_type(&output_ty),
            out_slot
        ));
        fctx.lines.push(format!("  br label %{}", done_label));

        fctx.lines.push(format!("{}:", ok_label));
        let op_payload = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            op_payload,
            llvm_type(&submitted.ty),
            submitted_repr,
            ok_idx + 1
        ));
        let op_value = Value {
            ty: submit_ok_payload_ty.clone(),
            repr: Some(op_payload),
        };
        let op_handle = self.extract_named_handle_from_value(
            &op_value,
            &op_name,
            "await submit bridge",
            span,
            fctx,
        )?;

        let bridged_result = if matches!(
            (op_name.as_str(), err_name.as_str()),
            ("AsyncStringOp", "NetError") | ("AsyncStringOp", "TlsError")
        ) {
            let out_ptr_slot = self.new_temp();
            fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
            let out_len_slot = self.new_temp();
            fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
            let poll_err = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call i64 @aic_rt_async_poll_string(i64 {}, i8** {}, i64* {})",
                poll_err, op_handle, out_ptr_slot, out_len_slot
            ));
            let out_ptr = self.new_temp();
            fctx.lines
                .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
            let out_len = self.new_temp();
            fctx.lines
                .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
            let data_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
            let payload = if output_ok_ty == LType::String {
                data_value
            } else {
                self.build_bytes_value_from_data(
                    &output_ok_ty,
                    data_value,
                    "await submit bridge",
                    span,
                    fctx,
                )?
            };
            if err_name == "TlsError" {
                self.wrap_tls_result(&output_ty, payload, &poll_err, span, fctx)?
            } else {
                self.wrap_net_result(&output_ty, payload, &poll_err, span, fctx)?
            }
        } else if matches!(
            (op_name.as_str(), err_name.as_str()),
            ("AsyncIntOp", "NetError") | ("AsyncIntOp", "TlsError")
        ) {
            let out_int_slot = self.new_temp();
            fctx.lines.push(format!("  {} = alloca i64", out_int_slot));
            let poll_err = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call i64 @aic_rt_async_poll_int(i64 {}, i64* {})",
                poll_err, op_handle, out_int_slot
            ));
            let out_int = self.new_temp();
            fctx.lines
                .push(format!("  {} = load i64, i64* {}", out_int, out_int_slot));
            if err_name == "TlsError" {
                self.wrap_tls_result(
                    &output_ty,
                    Value {
                        ty: LType::Int,
                        repr: Some(out_int),
                    },
                    &poll_err,
                    span,
                    fctx,
                )?
            } else {
                self.wrap_net_result(
                    &output_ty,
                    Value {
                        ty: LType::Int,
                        repr: Some(out_int),
                    },
                    &poll_err,
                    span,
                    fctx,
                )?
            }
        } else {
            let joined_slot = self.new_temp();
            fctx.lines.push(format!("  {} = alloca i64", joined_slot));
            let join_err = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call i64 @aic_rt_conc_join_value(i64 {}, i64* {})",
                join_err, op_handle, joined_slot
            ));
            let joined_value = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load i64, i64* {}",
                joined_value, joined_slot
            ));
            let joined_payload = self.load_boxed_runtime_value(
                &output_ty,
                &joined_value,
                "await_submit_fs",
                span,
                fctx,
            )?;
            let joined_slot_out = self.alloc_entry_slot(&output_ty, fctx);
            let runtime_done = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp eq i64 {}, 0", runtime_done, join_err));
            let joined_ok_label = self.new_label("await_submit_fs_ok");
            let joined_err_label = self.new_label("await_submit_fs_err");
            let joined_done_label = self.new_label("await_submit_fs_done");
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                runtime_done, joined_ok_label, joined_err_label
            ));
            fctx.lines.push(format!("{}:", joined_ok_label));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                llvm_type(&output_ty),
                joined_payload
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&output_ty)),
                llvm_type(&output_ty),
                joined_slot_out
            ));
            fctx.lines
                .push(format!("  br label %{}", joined_done_label));
            fctx.lines.push(format!("{}:", joined_err_label));
            let async_err_payload =
                self.build_fs_error_from_concurrency_code(&output_err_ty, &join_err, span, fctx)?;
            let async_err_value = self.build_enum_variant(
                &output_layout,
                output_err_idx,
                Some(async_err_payload),
                span,
                fctx,
            )?;
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                llvm_type(&output_ty),
                async_err_value
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&output_ty)),
                llvm_type(&output_ty),
                joined_slot_out
            ));
            fctx.lines
                .push(format!("  br label %{}", joined_done_label));
            fctx.lines.push(format!("{}:", joined_done_label));
            let joined_result = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                joined_result,
                llvm_type(&output_ty),
                llvm_type(&output_ty),
                joined_slot_out
            ));
            Value {
                ty: output_ty.clone(),
                repr: Some(joined_result),
            }
        };
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&output_ty),
            bridged_result
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&output_ty)),
            llvm_type(&output_ty),
            out_slot
        ));
        fctx.lines.push(format!("  br label %{}", done_label));

        fctx.lines.push(format!("{}:", done_label));
        let out_reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            out_reg,
            llvm_type(&output_ty),
            llvm_type(&output_ty),
            out_slot
        ));
        Some(Value {
            ty: output_ty,
            repr: Some(out_reg),
        })
    }

    pub(super) fn gen_await(
        &mut self,
        inner: &ir::Expr,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if fctx.async_poll_ctx.is_some() {
            return self.gen_async_poll_await(inner, span, fctx);
        }
        let awaited = self.gen_expr(inner, fctx)?;
        if let LType::Async(output_ty) = awaited.ty.clone() {
            let repr = awaited.repr.unwrap_or_else(|| default_value(&awaited.ty));
            self.extern_decls
                .insert("declare i64 @aic_rt_async_drive(i8*, i8*, i8*)".to_string());
            self.extern_decls
                .insert("declare void @aic_rt_async_drop(i8*, i8*)".to_string());
            let frame = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, 0",
                frame,
                llvm_type(&awaited.ty),
                repr
            ));
            let poll = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, 1",
                poll,
                llvm_type(&awaited.ty),
                repr
            ));
            let drop_fn = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, 2",
                drop_fn,
                llvm_type(&awaited.ty),
                repr
            ));
            let out_arg = if matches!(&*output_ty, LType::Unit) {
                "null".to_string()
            } else {
                let out_slot = self.alloc_entry_slot(&output_ty, fctx);
                let out_raw = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = bitcast {}* {} to i8*",
                    out_raw,
                    llvm_type(&output_ty),
                    out_slot
                ));
                out_raw
            };
            let drive_rc = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call i64 @aic_rt_async_drive(i8* {}, i8* {}, i8* {})",
                drive_rc, frame, poll, out_arg
            ));
            let _ = drive_rc;
            fctx.lines.push(format!(
                "  call void @aic_rt_async_drop(i8* {}, i8* {})",
                frame, drop_fn
            ));
            if matches!(&*output_ty, LType::Unit) {
                return Some(Value {
                    ty: LType::Unit,
                    repr: None,
                });
            }
            let value = self.new_temp();
            let out_slot = out_arg;
            fctx.lines.push(format!(
                "  {} = bitcast i8* {} to {}*",
                value,
                out_slot,
                llvm_type(&output_ty)
            ));
            let loaded = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                loaded,
                llvm_type(&output_ty),
                llvm_type(&output_ty),
                value
            ));
            return Some(Value {
                ty: (*output_ty).clone(),
                repr: Some(loaded),
            });
        }

        if let Some((ok_idx, err_idx, submit_ok_ty, submit_err_ty, output_ty, op_name, err_name)) =
            self.classify_await_submit_result(&awaited.ty, span)
        {
            return self.gen_await_submit_result(
                awaited,
                ok_idx,
                err_idx,
                submit_ok_ty,
                submit_err_ty,
                output_ty,
                op_name,
                err_name,
                span,
                fctx,
            );
        }

        self.diagnostics.push(Diagnostic::error(
            "E5002",
            "await expects Async[T] or Result[Async*Op, NetError/TlsError/FsError] during codegen",
            self.file,
            span,
        ));
        None
    }

    fn gen_async_poll_await(
        &mut self,
        inner: &ir::Expr,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let awaited = self.gen_expr(inner, fctx)?;
        let state_ptr = fctx
            .async_poll_ctx
            .as_ref()
            .map(|ctx| ctx.state_ptr.clone())?;

        if let LType::Async(output_ty) = awaited.ty.clone() {
            self.extern_decls
                .insert("declare i64 @aic_rt_async_poll_once(i8*, i8*, i8*)".to_string());
            self.extern_decls
                .insert("declare void @aic_rt_async_drop(i8*, i8*)".to_string());

            let storage_ptr = self.async_storage_ptr_for_type(&awaited.ty, fctx)?;
            let repr = awaited.repr.unwrap_or_else(|| default_value(&awaited.ty));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                llvm_type(&awaited.ty),
                repr,
                llvm_type(&awaited.ty),
                storage_ptr
            ));

            let (state_id, resume_label) =
                self.register_async_pending_state(AsyncPendingKind::Future, fctx)?;
            fctx.lines
                .push(format!("  store i32 {}, i32* {}", state_id, state_ptr));
            fctx.lines.push(format!("  br label %{}", resume_label));
            fctx.lines.push(format!("{}:", resume_label));
            let storage_ptr = self.async_storage_ptr_for_type(&awaited.ty, fctx)?;
            let loaded_future = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                loaded_future,
                llvm_type(&awaited.ty),
                llvm_type(&awaited.ty),
                storage_ptr
            ));
            let frame = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, 0",
                frame,
                llvm_type(&awaited.ty),
                loaded_future
            ));
            let poll = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, 1",
                poll,
                llvm_type(&awaited.ty),
                loaded_future
            ));
            let drop_fn = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, 2",
                drop_fn,
                llvm_type(&awaited.ty),
                loaded_future
            ));

            let out_arg = if matches!(&*output_ty, LType::Unit) {
                "null".to_string()
            } else {
                let out_slot = self.alloc_entry_slot(&output_ty, fctx);
                let out_raw = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = bitcast {}* {} to i8*",
                    out_raw,
                    llvm_type(&output_ty),
                    out_slot
                ));
                out_raw
            };
            let poll_rc = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call i64 @aic_rt_async_poll_once(i8* {}, i8* {}, i8* {})",
                poll_rc, frame, poll, out_arg
            ));
            let is_pending = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp eq i64 {}, 1", is_pending, poll_rc));
            let pending_label = self.new_label("await_pending");
            let check_ready_label = self.new_label("await_check_ready");
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                is_pending, pending_label, check_ready_label
            ));
            fctx.lines.push(format!("{}:", pending_label));
            fctx.lines.push("  ret i64 1".to_string());
            fctx.lines.push(format!("{}:", check_ready_label));
            let is_ready = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp eq i64 {}, 0", is_ready, poll_rc));
            let ready_label = self.new_label("await_ready");
            let error_label = self.new_label("await_error");
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                is_ready, ready_label, error_label
            ));
            fctx.lines.push(format!("{}:", error_label));
            fctx.lines.push(format!("  ret i64 {}", poll_rc));
            fctx.lines.push(format!("{}:", ready_label));
            fctx.lines.push(format!(
                "  call void @aic_rt_async_drop(i8* {}, i8* {})",
                frame, drop_fn
            ));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                llvm_type(&awaited.ty),
                default_value(&awaited.ty),
                llvm_type(&awaited.ty),
                storage_ptr
            ));
            if matches!(&*output_ty, LType::Unit) {
                return Some(Value {
                    ty: LType::Unit,
                    repr: None,
                });
            }
            let typed_out = self.new_temp();
            fctx.lines.push(format!(
                "  {} = bitcast i8* {} to {}*",
                typed_out,
                out_arg,
                llvm_type(&output_ty)
            ));
            let loaded = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                loaded,
                llvm_type(&output_ty),
                llvm_type(&output_ty),
                typed_out
            ));
            return Some(Value {
                ty: (*output_ty).clone(),
                repr: Some(loaded),
            });
        }

        let Some((ok_idx, err_idx, submit_ok_ty, submit_err_ty, output_ty, op_name, err_name)) =
            self.classify_await_submit_result(&awaited.ty, span)
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5002",
                "await expects Async[T] or Result[Async*Op, NetError/TlsError/FsError] during codegen",
                self.file,
                span,
            ));
            return None;
        };

        let Some((output_layout, output_ok_ty, output_err_ty, _output_ok_idx, output_err_idx)) =
            self.result_layout_parts(&output_ty, span)
        else {
            return None;
        };
        if output_err_ty != submit_err_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5002",
                "await submit bridge requires matching submit error payload type",
                self.file,
                span,
            ));
            return None;
        }

        let submitted_repr = awaited
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&awaited.ty));
        let tag = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            tag,
            llvm_type(&awaited.ty),
            submitted_repr
        ));
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i32 {}, {}", is_ok, tag, ok_idx));
        let ok_label = self.new_label("await_submit_ok");
        let err_label = self.new_label("await_submit_err");
        let done_label = self.new_label("await_submit_done");
        let out_slot = self.alloc_entry_slot(&output_ty, fctx);
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));
        fctx.lines.push(format!("{}:", err_label));
        let err_payload = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            err_payload,
            llvm_type(&awaited.ty),
            submitted_repr,
            err_idx + 1
        ));
        let err_value = self.build_enum_variant(
            &output_layout,
            output_err_idx,
            Some(Value {
                ty: submit_err_ty.clone(),
                repr: Some(err_payload),
            }),
            span,
            fctx,
        )?;
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&output_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&output_ty)),
            llvm_type(&output_ty),
            out_slot
        ));
        fctx.lines.push(format!("  br label %{}", done_label));

        fctx.lines.push(format!("{}:", ok_label));
        let op_payload = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            op_payload,
            llvm_type(&awaited.ty),
            submitted_repr,
            ok_idx + 1
        ));
        let op_value = Value {
            ty: submit_ok_ty.clone(),
            repr: Some(op_payload),
        };
        let op_handle = self.extract_named_handle_from_value(
            &op_value,
            &op_name,
            "await submit bridge",
            span,
            fctx,
        )?;
        let storage_ptr = self.async_storage_ptr_for_type(&LType::Int, fctx)?;
        fctx.lines
            .push(format!("  store i64 {}, i64* {}", op_handle, storage_ptr));
        let pending_kind = match (op_name.as_str(), err_name.as_str()) {
            ("AsyncIntOp", "NetError") => AsyncPendingKind::NetInt,
            ("AsyncStringOp", "NetError") => AsyncPendingKind::NetString,
            ("AsyncIntOp", "TlsError") => AsyncPendingKind::TlsInt,
            ("AsyncStringOp", "TlsError") => AsyncPendingKind::TlsString,
            _ => AsyncPendingKind::FsTask,
        };
        let (state_id, resume_label) = self.register_async_pending_state(pending_kind, fctx)?;
        fctx.lines
            .push(format!("  store i32 {}, i32* {}", state_id, state_ptr));
        fctx.lines.push(format!("  br label %{}", resume_label));
        fctx.lines.push(format!("{}:", resume_label));
        let storage_ptr = self.async_storage_ptr_for_type(&LType::Int, fctx)?;
        let loaded_handle = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            loaded_handle, storage_ptr
        ));

        let bridged_result = if matches!(
            (op_name.as_str(), err_name.as_str()),
            ("AsyncStringOp", "NetError") | ("AsyncStringOp", "TlsError")
        ) {
            let helper = if err_name == "TlsError" {
                self.extern_decls.insert(
                    "declare i64 @aic_rt_tls_async_poll_string_once(i64, i8**, i64*)".to_string(),
                );
                "@aic_rt_tls_async_poll_string_once"
            } else {
                self.extern_decls.insert(
                    "declare i64 @aic_rt_net_async_poll_string_once(i64, i8**, i64*)".to_string(),
                );
                "@aic_rt_net_async_poll_string_once"
            };
            let out_ptr_slot = self.new_temp();
            fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
            let out_len_slot = self.new_temp();
            fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
            let poll_err = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call i64 {}(i64 {}, i8** {}, i64* {})",
                poll_err, helper, loaded_handle, out_ptr_slot, out_len_slot
            ));
            let is_pending = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp eq i64 {}, -1", is_pending, poll_err));
            let pending_label = self.new_label("await_submit_pending");
            let ready_label = self.new_label("await_submit_ready");
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                is_pending, pending_label, ready_label
            ));
            fctx.lines.push(format!("{}:", pending_label));
            fctx.lines.push("  ret i64 1".to_string());
            fctx.lines.push(format!("{}:", ready_label));
            let out_ptr = self.new_temp();
            fctx.lines
                .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
            let out_len = self.new_temp();
            fctx.lines
                .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
            let data_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
            let payload = if output_ok_ty == LType::String {
                data_value
            } else {
                self.build_bytes_value_from_data(
                    &output_ok_ty,
                    data_value,
                    "await submit bridge",
                    span,
                    fctx,
                )?
            };
            if err_name == "TlsError" {
                self.wrap_tls_result(&output_ty, payload, &poll_err, span, fctx)?
            } else {
                self.wrap_net_result(&output_ty, payload, &poll_err, span, fctx)?
            }
        } else if matches!(
            (op_name.as_str(), err_name.as_str()),
            ("AsyncIntOp", "NetError") | ("AsyncIntOp", "TlsError")
        ) {
            let helper = if err_name == "TlsError" {
                self.extern_decls
                    .insert("declare i64 @aic_rt_tls_async_poll_int_once(i64, i64*)".to_string());
                "@aic_rt_tls_async_poll_int_once"
            } else {
                self.extern_decls
                    .insert("declare i64 @aic_rt_net_async_poll_int_once(i64, i64*)".to_string());
                "@aic_rt_net_async_poll_int_once"
            };
            let out_int_slot = self.new_temp();
            fctx.lines.push(format!("  {} = alloca i64", out_int_slot));
            let poll_err = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call i64 {}(i64 {}, i64* {})",
                poll_err, helper, loaded_handle, out_int_slot
            ));
            let is_pending = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp eq i64 {}, -1", is_pending, poll_err));
            let pending_label = self.new_label("await_submit_pending");
            let ready_label = self.new_label("await_submit_ready");
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                is_pending, pending_label, ready_label
            ));
            fctx.lines.push(format!("{}:", pending_label));
            fctx.lines.push("  ret i64 1".to_string());
            fctx.lines.push(format!("{}:", ready_label));
            let out_int = self.new_temp();
            fctx.lines
                .push(format!("  {} = load i64, i64* {}", out_int, out_int_slot));
            let payload = Value {
                ty: LType::Int,
                repr: Some(out_int),
            };
            if err_name == "TlsError" {
                self.wrap_tls_result(&output_ty, payload, &poll_err, span, fctx)?
            } else {
                self.wrap_net_result(&output_ty, payload, &poll_err, span, fctx)?
            }
        } else {
            let joined_slot = self.new_temp();
            fctx.lines.push(format!("  {} = alloca i64", joined_slot));
            let join_err = self.new_temp();
            fctx.lines.push(format!(
                "  {} = call i64 @aic_rt_conc_join_poll(i64 {}, i64* {})",
                join_err, loaded_handle, joined_slot
            ));
            let is_pending = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp eq i64 {}, 2", is_pending, join_err));
            let pending_label = self.new_label("await_submit_pending");
            let ready_label = self.new_label("await_submit_ready");
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                is_pending, pending_label, ready_label
            ));
            fctx.lines.push(format!("{}:", pending_label));
            fctx.lines.push("  ret i64 1".to_string());
            fctx.lines.push(format!("{}:", ready_label));
            let joined_value = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load i64, i64* {}",
                joined_value, joined_slot
            ));
            let joined_payload = self.load_boxed_runtime_value(
                &output_ty,
                &joined_value,
                "await_submit_fs",
                span,
                fctx,
            )?;
            let runtime_done = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp eq i64 {}, 0", runtime_done, join_err));
            let ok_payload_label = self.new_label("await_submit_fs_ok");
            let err_payload_label = self.new_label("await_submit_fs_err");
            let payload_done_label = self.new_label("await_submit_fs_done");
            let payload_slot = self.alloc_entry_slot(&output_ty, fctx);
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                runtime_done, ok_payload_label, err_payload_label
            ));
            fctx.lines.push(format!("{}:", ok_payload_label));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                llvm_type(&output_ty),
                joined_payload
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&output_ty)),
                llvm_type(&output_ty),
                payload_slot
            ));
            fctx.lines
                .push(format!("  br label %{}", payload_done_label));
            fctx.lines.push(format!("{}:", err_payload_label));
            let async_err_payload =
                self.build_fs_error_from_concurrency_code(&output_err_ty, &join_err, span, fctx)?;
            let async_err_value = self.build_enum_variant(
                &output_layout,
                output_err_idx,
                Some(async_err_payload),
                span,
                fctx,
            )?;
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                llvm_type(&output_ty),
                async_err_value
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&output_ty)),
                llvm_type(&output_ty),
                payload_slot
            ));
            fctx.lines
                .push(format!("  br label %{}", payload_done_label));
            fctx.lines.push(format!("{}:", payload_done_label));
            let bridged = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                bridged,
                llvm_type(&output_ty),
                llvm_type(&output_ty),
                payload_slot
            ));
            Value {
                ty: output_ty.clone(),
                repr: Some(bridged),
            }
        };

        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&output_ty),
            bridged_result
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&output_ty)),
            llvm_type(&output_ty),
            out_slot
        ));
        fctx.lines.push(format!("  br label %{}", done_label));
        fctx.lines.push(format!("{}:", done_label));
        let out_reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            out_reg,
            llvm_type(&output_ty),
            llvm_type(&output_ty),
            out_slot
        ));
        Some(Value {
            ty: output_ty,
            repr: Some(out_reg),
        })
    }

    pub(super) fn type_needs_explicit_drop(&self, ty: &LType) -> bool {
        matches!(ty, LType::Async(_))
            || self.drop_impl_method_for_type(ty).is_some()
            || resource_drop_action_for_type(ty).is_some()
    }

    pub(super) fn drop_impl_method_for_type(&self, ty: &LType) -> Option<String> {
        let ty_repr = render_type(ty);
        self.drop_impl_methods.get(&ty_repr).cloned()
    }

    pub(super) fn emit_trait_drop_action(
        &mut self,
        drop_method: &str,
        local: &DropSlot,
        fctx: &mut FnCtx,
    ) {
        let Some(sig) = self.fn_sig(drop_method).cloned() else {
            return;
        };
        if sig.params.len() != 1 || sig.params[0] != local.ty || sig.ret != LType::Unit {
            return;
        }

        let loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            loaded,
            llvm_type(&local.ty),
            llvm_type(&local.ty),
            local.ptr
        ));
        fctx.lines.push(format!(
            "  call void @{}({} {})",
            mangle(drop_method),
            llvm_type(&local.ty),
            loaded
        ));
    }

    pub(super) fn emit_resource_drop_action(
        &mut self,
        action: ResourceDropAction,
        local: &DropSlot,
        fctx: &mut FnCtx,
    ) {
        let LType::Struct(layout) = &local.ty else {
            return;
        };
        let loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            loaded,
            llvm_type(&local.ty),
            llvm_type(&local.ty),
            local.ptr
        ));

        let handle = match action {
            ResourceDropAction::SetCloseInnerMap => {
                let Some(items_idx) = self.struct_field_index(layout, "items") else {
                    return;
                };
                let Some(items_ty) = layout.fields.get(items_idx).map(|field| field.ty.clone())
                else {
                    return;
                };
                let LType::Struct(map_layout) = items_ty else {
                    return;
                };
                let Some(map_handle_idx) = self.struct_int_field_index(&map_layout, "handle")
                else {
                    return;
                };
                let items = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = extractvalue {} {}, {}",
                    items,
                    llvm_type(&local.ty),
                    loaded,
                    items_idx
                ));
                let map_handle = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = extractvalue {} {}, {}",
                    map_handle,
                    llvm_type(&LType::Struct(map_layout.clone())),
                    items,
                    map_handle_idx
                ));
                map_handle
            }
            _ => {
                let Some(handle_idx) = self.struct_int_field_index(layout, "handle") else {
                    return;
                };
                let handle = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = extractvalue {} {}, {}",
                    handle,
                    llvm_type(&local.ty),
                    loaded,
                    handle_idx
                ));
                handle
            }
        };

        let drop_call = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {})",
            drop_call,
            resource_drop_runtime_fn(action),
            handle
        ));
    }

    pub(super) fn emit_async_drop_action(&mut self, local: &DropSlot, fctx: &mut FnCtx) {
        let LType::Async(_) = &local.ty else {
            return;
        };
        self.extern_decls
            .insert("declare void @aic_rt_async_drop(i8*, i8*)".to_string());
        let loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            loaded,
            llvm_type(&local.ty),
            llvm_type(&local.ty),
            local.ptr
        ));
        let frame = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            frame,
            llvm_type(&local.ty),
            loaded
        ));
        let drop_fn = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 2",
            drop_fn,
            llvm_type(&local.ty),
            loaded
        ));
        fctx.lines.push(format!(
            "  call void @aic_rt_async_drop(i8* {}, i8* {})",
            frame, drop_fn
        ));
    }

    pub(super) fn try_emit_musttail_return(&mut self, expr: &ir::Expr, fctx: &mut FnCtx) -> bool {
        if fctx.async_inner_ret.is_some() || fctx.async_poll_ctx.is_some() {
            return false;
        }
        let ir::ExprKind::Call { callee, args, .. } = &expr.kind else {
            return false;
        };
        self.try_emit_musttail_call(callee, args, fctx)
    }

    pub(super) fn try_emit_musttail_tail_expr_return(
        &mut self,
        expr: &ir::Expr,
        fctx: &mut FnCtx,
    ) -> bool {
        if !fctx.tail_return_mode || fctx.async_inner_ret.is_some() || fctx.async_poll_ctx.is_some()
        {
            return false;
        }
        let ir::ExprKind::Call { callee, args, .. } = &expr.kind else {
            return false;
        };
        self.try_emit_musttail_call(callee, args, fctx)
    }

    pub(super) fn try_emit_musttail_call(
        &mut self,
        callee: &ir::Expr,
        args: &[ir::Expr],
        fctx: &mut FnCtx,
    ) -> bool {
        let Some(path) = extract_callee_path(callee) else {
            return false;
        };
        if path.len() != 1 {
            return false;
        }
        let callee_name = &path[0];
        let Some(callee_sig) = self.fn_sig(callee_name).cloned() else {
            return false;
        };
        if callee_sig.is_extern || callee_sig.is_intrinsic {
            return false;
        }
        let caller = &fctx.current_fn_name;
        let recursive_target = self
            .recursive_call_targets
            .get(caller)
            .map(|targets| targets.contains(callee_name))
            .unwrap_or(false);
        if !recursive_target {
            return false;
        }
        if callee_sig.params != fctx.current_fn_sig.params
            || callee_sig.ret != fctx.current_fn_sig.ret
        {
            return false;
        }
        if args.len() != callee_sig.params.len() {
            return false;
        }

        let mut call_args = Vec::with_capacity(args.len());
        for (arg, expected_ty) in args.iter().zip(callee_sig.params.iter()) {
            let Some(value) = self.gen_expr_with_expected(arg, Some(expected_ty), fctx) else {
                self.emit_scope_drops_to_depth(0, fctx);
                if callee_sig.ret == LType::Unit {
                    fctx.lines.push("  ret void".to_string());
                } else {
                    fctx.lines.push(format!(
                        "  ret {} {}",
                        llvm_type(&callee_sig.ret),
                        default_value(&callee_sig.ret)
                    ));
                }
                fctx.terminated = true;
                return true;
            };
            let Some(value) = self.coerce_value_to_expected(value, expected_ty, arg.span, fctx)
            else {
                return false;
            };
            let value_repr = value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(expected_ty));
            call_args.push(format!("{} {}", llvm_type(expected_ty), value_repr));
        }

        let callee_llvm = if *callee_name == fctx.current_fn_name {
            fctx.current_fn_llvm_name.clone()
        } else {
            mangle(callee_name)
        };

        self.emit_scope_drops_to_depth(0, fctx);
        if callee_sig.ret == LType::Unit {
            fctx.lines.push(format!(
                "  musttail call void @{}({})",
                callee_llvm,
                call_args.join(", ")
            ));
            fctx.lines.push("  ret void".to_string());
        } else {
            let out = self.new_temp();
            fctx.lines.push(format!(
                "  {} = musttail call {} @{}({})",
                out,
                llvm_type(&callee_sig.ret),
                callee_llvm,
                call_args.join(", ")
            ));
            fctx.lines
                .push(format!("  ret {} {}", llvm_type(&callee_sig.ret), out));
        }
        fctx.terminated = true;
        true
    }

    pub(super) fn struct_int_field_index(
        &self,
        layout: &StructLayoutType,
        field_name: &str,
    ) -> Option<usize> {
        layout
            .fields
            .iter()
            .position(|field| field.name == field_name && field.ty == LType::Int)
    }

    pub(super) fn struct_field_index(
        &self,
        layout: &StructLayoutType,
        field_name: &str,
    ) -> Option<usize> {
        layout
            .fields
            .iter()
            .position(|field| field.name == field_name)
    }

    fn int_literal_type_hint(&self, expr: &ir::Expr, expected_ty: Option<&LType>) -> Option<LType> {
        if let Some(meta) = expr.int_literal_metadata() {
            return Some(match (meta.kind.signedness, meta.kind.width) {
                (crate::ast::IntLiteralSignedness::Signed, crate::ast::IntLiteralWidth::W8) => {
                    LType::Int8
                }
                (crate::ast::IntLiteralSignedness::Signed, crate::ast::IntLiteralWidth::W16) => {
                    LType::Int16
                }
                (crate::ast::IntLiteralSignedness::Signed, crate::ast::IntLiteralWidth::W32) => {
                    LType::Int32
                }
                (crate::ast::IntLiteralSignedness::Signed, crate::ast::IntLiteralWidth::W64) => {
                    LType::Int64
                }
                (crate::ast::IntLiteralSignedness::Signed, crate::ast::IntLiteralWidth::W128) => {
                    LType::Int128
                }
                (crate::ast::IntLiteralSignedness::Unsigned, crate::ast::IntLiteralWidth::W8) => {
                    LType::UInt8
                }
                (crate::ast::IntLiteralSignedness::Unsigned, crate::ast::IntLiteralWidth::W16) => {
                    LType::UInt16
                }
                (crate::ast::IntLiteralSignedness::Unsigned, crate::ast::IntLiteralWidth::W32) => {
                    LType::UInt32
                }
                (crate::ast::IntLiteralSignedness::Unsigned, crate::ast::IntLiteralWidth::W64) => {
                    LType::UInt64
                }
                (crate::ast::IntLiteralSignedness::Unsigned, crate::ast::IntLiteralWidth::W128) => {
                    LType::UInt128
                }
            });
        }
        expected_ty.filter(|ty| is_integral_type(ty)).cloned()
    }

    fn int_literal_llvm_repr(&self, expr: &ir::Expr, fallback: i64) -> String {
        expr.int_literal_metadata()
            .and_then(|meta| parse_raw_int_literal_magnitude(&meta.raw_literal_text))
            .map(|magnitude| magnitude.to_string())
            .unwrap_or_else(|| fallback.to_string())
    }

    fn float_literal_type_hint(&self, expr: &ir::Expr, expected_ty: Option<&LType>) -> LType {
        if let Some(meta) = expr.float_literal_metadata() {
            return match meta.kind.width {
                crate::ast::FloatLiteralWidth::W32 => LType::Float32,
                crate::ast::FloatLiteralWidth::W64 => LType::Float64,
            };
        }
        if let Some(expected) = expected_ty {
            if let Some(kind) = float_common_type(expected, expected) {
                return kind;
            }
        }
        LType::Float64
    }

    pub(super) fn gen_expr(&mut self, expr: &ir::Expr, fctx: &mut FnCtx) -> Option<Value> {
        self.gen_expr_with_expected(expr, None, fctx)
    }

    pub(super) fn gen_expr_with_expected(
        &mut self,
        expr: &ir::Expr,
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        match &expr.kind {
            ir::ExprKind::Int(v) => {
                let literal_ty = self
                    .int_literal_type_hint(expr, expected_ty)
                    .unwrap_or(LType::Int);
                Some(Value {
                    ty: literal_ty,
                    repr: Some(self.int_literal_llvm_repr(expr, *v)),
                })
            }
            ir::ExprKind::Float(v) => {
                let literal_ty = self.float_literal_type_hint(expr, expected_ty);
                if literal_ty == LType::Float32 {
                    let narrowed = self.new_temp();
                    fctx.lines.push(format!(
                        "  {} = fptrunc double {} to float",
                        narrowed,
                        llvm_float_literal(*v)
                    ));
                    Some(Value {
                        ty: LType::Float32,
                        repr: Some(narrowed),
                    })
                } else {
                    Some(Value {
                        ty: literal_ty,
                        repr: Some(llvm_float_literal(*v)),
                    })
                }
            }
            ir::ExprKind::Bool(v) => Some(Value {
                ty: LType::Bool,
                repr: Some(if *v { "1".to_string() } else { "0".to_string() }),
            }),
            ir::ExprKind::Char(v) => Some(Value {
                ty: LType::Char,
                repr: Some((*v as u32).to_string()),
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
                    if fctx.async_poll_ctx.is_some()
                        && local.symbol.is_some()
                        && self.type_needs_explicit_drop(&local.ty)
                    {
                        fctx.lines.push(format!(
                            "  store {} {}, {}* {}",
                            llvm_type(&local.ty),
                            default_value(&local.ty),
                            llvm_type(&local.ty),
                            local.ptr
                        ));
                    }
                    Some(Value {
                        ty: local.ty,
                        repr: Some(reg),
                    })
                } else if let Some(const_value) = self.const_values.get(name).cloned() {
                    let mut value = self.runtime_value_from_const(&const_value, fctx);
                    if let Some(def) = self.const_defs.get(name).cloned() {
                        if let Some(const_ty) = self.parse_type_repr(&def.declared_ty, def.span) {
                            value =
                                self.coerce_value_to_expected(value, &const_ty, expr.span, fctx)?;
                        }
                    }
                    Some(value)
                } else if self.const_defs.contains_key(name) {
                    self.diagnostics.push(Diagnostic::error(
                        "E5023",
                        format!(
                            "const '{}' is unavailable during codegen because its initializer could not be evaluated",
                            name
                        ),
                        self.file,
                        expr.span,
                    ));
                    None
                } else if let Some(sig) = self.fn_sig(name).cloned() {
                    self.gen_function_value(name, &sig, expr.span, fctx)
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
                    (UnaryOp::Neg, ty) if is_signed_integer_type(&ty) => {
                        let reg = self.new_temp();
                        let repr = value.repr.unwrap_or_else(|| default_value(&ty));
                        fctx.lines
                            .push(format!("  {} = sub {} 0, {}", reg, llvm_type(&ty), repr));
                        Some(Value {
                            ty,
                            repr: Some(reg),
                        })
                    }
                    (UnaryOp::Neg, ty) if is_float_type(&ty) => {
                        let reg = self.new_temp();
                        let repr = value.repr.unwrap_or_else(|| default_value(&ty));
                        fctx.lines
                            .push(format!("  {} = fneg {} {}", reg, llvm_type(&ty), repr));
                        Some(Value {
                            ty,
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
                    (UnaryOp::BitNot, ty) if is_integral_type(&ty) => {
                        let reg = self.new_temp();
                        let repr = value.repr.unwrap_or_else(|| default_value(&ty));
                        fctx.lines
                            .push(format!("  {} = xor {} {}, -1", reg, llvm_type(&ty), repr));
                        Some(Value {
                            ty,
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
            ir::ExprKind::Await { expr: inner } => self.gen_await(inner, expr.span, fctx),
            ir::ExprKind::Try { expr: inner } => self.gen_try(inner, expr.span, fctx),
            ir::ExprKind::UnsafeBlock { block } => self.gen_block(block, fctx),
            ir::ExprKind::Binary { op, lhs, rhs } => {
                let numeric_expected =
                    expected_ty.filter(|ty| is_integral_type(ty) || is_float_type(ty));
                let lv = self.gen_expr_with_expected(lhs, numeric_expected, fctx)?;
                let rhs_expected = if let Some(expected) = numeric_expected {
                    Some(expected)
                } else if is_integral_type(&lv.ty) || is_float_type(&lv.ty) {
                    Some(&lv.ty)
                } else {
                    None
                };
                let rv = self.gen_expr_with_expected(rhs, rhs_expected, fctx)?;
                self.gen_binary(*op, lv, rv, fctx, expr.span)
            }
            ir::ExprKind::Call {
                callee,
                args,
                symbol,
                ..
            } => {
                if let ir::ExprKind::Var(name) = &callee.kind {
                    if let Some(local) = find_local(&fctx.vars, name) {
                        if matches!(local.ty, LType::Fn(_)) {
                            let callee_value = self.gen_expr(callee, fctx)?;
                            return self.gen_fn_value_call(callee_value, args, expr.span, fctx);
                        }
                    }
                }
                if let ir::ExprKind::FieldAccess { base, field } = &callee.kind {
                    if !self.is_module_qualified_callee(callee, fctx) {
                        return self.gen_method_call(base, field, args, expr.span, fctx);
                    }
                }

                let Some(path) = extract_callee_path(callee) else {
                    let callee_value = self.gen_expr(callee, fctx)?;
                    return self.gen_fn_value_call(callee_value, args, expr.span, fctx);
                };
                if path.last().is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        "E5003",
                        "callee path cannot be empty",
                        self.file,
                        callee.span,
                    ));
                    return None;
                }
                self.gen_call(&path, args, *symbol, expr.span, expected_ty, fctx)
            }
            ir::ExprKind::TemplateLiteral { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    "template literals must be lowered before LLVM codegen",
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::Closure {
                params,
                ret_type,
                body,
            } => self.gen_closure_value(params, *ret_type, body, expected_ty, expr.span, fctx),
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => self.gen_if(cond, then_block, else_block, expected_ty, fctx),
            ir::ExprKind::While { cond, body } => self.gen_while(cond, body, fctx),
            ir::ExprKind::Loop { body } => self.gen_loop(body, fctx),
            ir::ExprKind::Break { expr: break_expr } => {
                self.gen_break(break_expr.as_deref(), expr.span, fctx)
            }
            ir::ExprKind::Continue => self.gen_continue(expr.span, fctx),
            ir::ExprKind::Match {
                expr: scrutinee,
                arms,
            } => self.gen_match(scrutinee, arms, expected_ty, fctx),
            ir::ExprKind::StructInit { name, fields } => {
                self.gen_struct_init(name, fields, expected_ty, expr.span, fctx)
            }
            ir::ExprKind::FieldAccess { base, field } => {
                self.gen_field_access(base, field, expr.span, fctx)
            }
        }
    }

    pub(super) fn gen_binary(
        &mut self,
        op: BinOp,
        lhs: Value,
        rhs: Value,
        fctx: &mut FnCtx,
        span: crate::span::Span,
    ) -> Option<Value> {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                let mut lhs = lhs;
                let mut rhs = rhs;
                if is_integral_type(&lhs.ty) && is_integral_type(&rhs.ty) && lhs.ty != rhs.ty {
                    if let Some(common_ty) = comparison_integral_common_type(&lhs.ty, &rhs.ty) {
                        if let Some(coerced) =
                            self.coerce_value_to_expected(lhs.clone(), &common_ty, span, fctx)
                        {
                            lhs = coerced;
                        }
                        if let Some(coerced) =
                            self.coerce_value_to_expected(rhs.clone(), &common_ty, span, fctx)
                        {
                            rhs = coerced;
                        }
                    }
                }
                if is_float_type(&lhs.ty) && is_float_type(&rhs.ty) && lhs.ty != rhs.ty {
                    if let Some(common_ty) = float_common_type(&lhs.ty, &rhs.ty) {
                        if let Some(coerced) =
                            self.coerce_value_to_expected(lhs.clone(), &common_ty, span, fctx)
                        {
                            lhs = coerced;
                        }
                        if let Some(coerced) =
                            self.coerce_value_to_expected(rhs.clone(), &common_ty, span, fctx)
                        {
                            rhs = coerced;
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5006",
                            "arithmetic codegen expects matching float widths",
                            self.file,
                            span,
                        ));
                        return None;
                    }
                }
                match (&lhs.ty, &rhs.ty) {
                    (lhs_ty, rhs_ty) if lhs_ty == rhs_ty && is_integral_type(lhs_ty) => {
                        let inst = match op {
                            BinOp::Add => "add",
                            BinOp::Sub => "sub",
                            BinOp::Mul => "mul",
                            BinOp::Div => {
                                if is_unsigned_integer_type(lhs_ty) {
                                    "udiv"
                                } else {
                                    "sdiv"
                                }
                            }
                            BinOp::Mod => {
                                if is_unsigned_integer_type(lhs_ty) {
                                    "urem"
                                } else {
                                    "srem"
                                }
                            }
                            _ => unreachable!(),
                        };
                        let reg = self.new_temp();
                        fctx.lines.push(format!(
                            "  {} = {} {} {}, {}",
                            reg,
                            inst,
                            llvm_type(lhs_ty),
                            lhs.repr.unwrap_or_else(|| "0".to_string()),
                            rhs.repr.unwrap_or_else(|| "0".to_string())
                        ));
                        Some(Value {
                            ty: lhs_ty.clone(),
                            repr: Some(reg),
                        })
                    }
                    (lhs_ty, rhs_ty)
                        if lhs_ty == rhs_ty
                            && is_float_type(lhs_ty)
                            && !matches!(op, BinOp::Mod) =>
                    {
                        let inst = match op {
                            BinOp::Add => "fadd",
                            BinOp::Sub => "fsub",
                            BinOp::Mul => "fmul",
                            BinOp::Div => "fdiv",
                            _ => unreachable!(),
                        };
                        let reg = self.new_temp();
                        fctx.lines.push(format!(
                            "  {} = {} {} {}, {}",
                            reg,
                            inst,
                            llvm_type(lhs_ty),
                            lhs.repr.unwrap_or_else(|| default_value(lhs_ty)),
                            rhs.repr.unwrap_or_else(|| default_value(rhs_ty))
                        ));
                        Some(Value {
                            ty: lhs_ty.clone(),
                            repr: Some(reg),
                        })
                    }
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E5006",
                            "arithmetic codegen expects matching integer or float operands",
                            self.file,
                            span,
                        ));
                        None
                    }
                }
            }
            BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::BitXor
            | BinOp::Shl
            | BinOp::Shr
            | BinOp::Ushr => {
                if lhs.ty != rhs.ty || !is_integral_type(&lhs.ty) {
                    self.diagnostics.push(Diagnostic::error(
                        "E5006",
                        "bitwise codegen only supports matching integer operands",
                        self.file,
                        span,
                    ));
                    return None;
                }
                let inst = match op {
                    BinOp::BitAnd => "and",
                    BinOp::BitOr => "or",
                    BinOp::BitXor => "xor",
                    BinOp::Shl => "shl",
                    BinOp::Shr => {
                        if is_unsigned_integer_type(&lhs.ty) {
                            "lshr"
                        } else {
                            "ashr"
                        }
                    }
                    BinOp::Ushr => "lshr",
                    _ => unreachable!(),
                };
                let reg = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = {} {} {}, {}",
                    reg,
                    inst,
                    llvm_type(&lhs.ty),
                    lhs.repr.unwrap_or_else(|| "0".to_string()),
                    rhs.repr.unwrap_or_else(|| "0".to_string())
                ));
                Some(Value {
                    ty: lhs.ty,
                    repr: Some(reg),
                })
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let mut lhs = lhs;
                let mut rhs = rhs;
                if is_integral_type(&lhs.ty) && is_integral_type(&rhs.ty) && lhs.ty != rhs.ty {
                    if let Some(common_ty) = comparison_integral_common_type(&lhs.ty, &rhs.ty) {
                        if let Some(coerced) =
                            self.coerce_value_to_expected(lhs.clone(), &common_ty, span, fctx)
                        {
                            lhs = coerced;
                        }
                        if let Some(coerced) =
                            self.coerce_value_to_expected(rhs.clone(), &common_ty, span, fctx)
                        {
                            rhs = coerced;
                        }
                    }
                }
                if is_float_type(&lhs.ty) && is_float_type(&rhs.ty) && lhs.ty != rhs.ty {
                    if let Some(common_ty) = float_common_type(&lhs.ty, &rhs.ty) {
                        if let Some(coerced) =
                            self.coerce_value_to_expected(lhs.clone(), &common_ty, span, fctx)
                        {
                            lhs = coerced;
                        }
                        if let Some(coerced) =
                            self.coerce_value_to_expected(rhs.clone(), &common_ty, span, fctx)
                        {
                            rhs = coerced;
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5006",
                            "comparison codegen expects matching float widths",
                            self.file,
                            span,
                        ));
                        return None;
                    }
                }
                if lhs.ty == LType::String && rhs.ty == LType::String {
                    if !matches!(op, BinOp::Eq | BinOp::Ne) {
                        self.diagnostics.push(Diagnostic::error(
                            "E5006",
                            "string ordering comparisons are unsupported; typecheck must reject them",
                            self.file,
                            span,
                        ));
                        return None;
                    }
                    let lhs_repr = lhs.repr.unwrap_or_else(|| default_value(&lhs.ty));
                    let rhs_repr = rhs.repr.unwrap_or_else(|| default_value(&rhs.ty));
                    let lhs_ptr = self.new_temp();
                    let lhs_len = self.new_temp();
                    let lhs_cap = self.new_temp();
                    let rhs_ptr = self.new_temp();
                    let rhs_len = self.new_temp();
                    let rhs_cap = self.new_temp();
                    let lhs_llvm_ty = llvm_type(&lhs.ty);
                    let rhs_llvm_ty = llvm_type(&rhs.ty);
                    fctx.lines.push(format!(
                        "  {} = extractvalue {} {}, 0",
                        lhs_ptr, lhs_llvm_ty, &lhs_repr
                    ));
                    fctx.lines.push(format!(
                        "  {} = extractvalue {} {}, 1",
                        lhs_len, lhs_llvm_ty, &lhs_repr
                    ));
                    fctx.lines.push(format!(
                        "  {} = extractvalue {} {}, 2",
                        lhs_cap, lhs_llvm_ty, &lhs_repr
                    ));
                    fctx.lines.push(format!(
                        "  {} = extractvalue {} {}, 0",
                        rhs_ptr, rhs_llvm_ty, &rhs_repr
                    ));
                    fctx.lines.push(format!(
                        "  {} = extractvalue {} {}, 1",
                        rhs_len, rhs_llvm_ty, &rhs_repr
                    ));
                    fctx.lines.push(format!(
                        "  {} = extractvalue {} {}, 2",
                        rhs_cap, rhs_llvm_ty, &rhs_repr
                    ));
                    let cmp = self.new_temp();
                    fctx.lines.push(format!(
                        "  {} = call i64 @aic_rt_string_compare(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
                        cmp, lhs_ptr, lhs_len, lhs_cap, rhs_ptr, rhs_len, rhs_cap
                    ));
                    let pred = if matches!(op, BinOp::Eq) { "eq" } else { "ne" };
                    let reg = self.new_temp();
                    fctx.lines
                        .push(format!("  {} = icmp {} i64 {}, 0", reg, pred, cmp));
                    return Some(Value {
                        ty: LType::Bool,
                        repr: Some(reg),
                    });
                }

                let (cmp, ty) = match (&lhs.ty, &rhs.ty) {
                    (lhs_ty, rhs_ty) if lhs_ty == rhs_ty && is_integral_type(lhs_ty) => {
                        let cmp = match op {
                            BinOp::Eq => "eq",
                            BinOp::Ne => "ne",
                            BinOp::Lt => {
                                if is_unsigned_integer_type(lhs_ty) {
                                    "ult"
                                } else {
                                    "slt"
                                }
                            }
                            BinOp::Le => {
                                if is_unsigned_integer_type(lhs_ty) {
                                    "ule"
                                } else {
                                    "sle"
                                }
                            }
                            BinOp::Gt => {
                                if is_unsigned_integer_type(lhs_ty) {
                                    "ugt"
                                } else {
                                    "sgt"
                                }
                            }
                            BinOp::Ge => {
                                if is_unsigned_integer_type(lhs_ty) {
                                    "uge"
                                } else {
                                    "sge"
                                }
                            }
                            _ => unreachable!(),
                        };
                        (cmp, llvm_type(lhs_ty))
                    }
                    (lhs_ty, rhs_ty) if lhs_ty == rhs_ty && is_float_type(lhs_ty) => {
                        let cmp = match op {
                            BinOp::Eq => "oeq",
                            BinOp::Ne => "une",
                            BinOp::Lt => "olt",
                            BinOp::Le => "ole",
                            BinOp::Gt => "ogt",
                            BinOp::Ge => "oge",
                            _ => unreachable!(),
                        };
                        (cmp, llvm_type(lhs_ty))
                    }
                    (LType::Bool, LType::Bool) if matches!(op, BinOp::Eq | BinOp::Ne) => {
                        let cmp = if matches!(op, BinOp::Eq) { "eq" } else { "ne" };
                        (cmp, "i1".to_string())
                    }
                    (LType::Char, LType::Char) => {
                        let cmp = match op {
                            BinOp::Eq => "eq",
                            BinOp::Ne => "ne",
                            BinOp::Lt => "slt",
                            BinOp::Le => "sle",
                            BinOp::Gt => "sgt",
                            BinOp::Ge => "sge",
                            _ => unreachable!(),
                        };
                        (cmp, "i32".to_string())
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
                let is_float_cmp = is_float_type(&lhs.ty) && is_float_type(&rhs.ty);
                let (inst, lhs_repr, rhs_repr) = if is_float_cmp {
                    (
                        "fcmp",
                        lhs.repr.unwrap_or_else(|| default_value(&lhs.ty)),
                        rhs.repr.unwrap_or_else(|| default_value(&rhs.ty)),
                    )
                } else {
                    (
                        "icmp",
                        lhs.repr.unwrap_or_else(|| default_value(&lhs.ty)),
                        rhs.repr.unwrap_or_else(|| default_value(&rhs.ty)),
                    )
                };
                fctx.lines.push(format!(
                    "  {} = {} {} {} {}, {}",
                    reg, inst, cmp, ty, lhs_repr, rhs_repr
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

    pub(super) fn gen_call(
        &mut self,
        call_path: &[String],
        args: &[ir::Expr],
        call_symbol: Option<ir::SymbolId>,
        span: crate::span::Span,
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some(name) = call_path.last().map(String::as_str) else {
            self.diagnostics.push(Diagnostic::error(
                "E5003",
                "callee path cannot be empty",
                self.file,
                span,
            ));
            return None;
        };
        let builtin_name = qualified_builtin_intrinsic(call_path).unwrap_or(name);
        let exact_sig = call_symbol.and_then(|symbol| self.fn_sigs_by_symbol.get(&symbol).cloned());
        let _sig_guard = if let Some(sig) = exact_sig.clone() {
            self.call_sig_overrides.push(CallSigOverride {
                name: name.to_string(),
                sig,
            });
            CallSigOverrideGuard {
                stack: &mut self.call_sig_overrides as *mut _,
                pushed: true,
            }
        } else {
            CallSigOverrideGuard {
                stack: std::ptr::null_mut(),
                pushed: false,
            }
        };

        if let Some(value) = self.gen_variant_constructor(name, args, expected_ty, span, fctx) {
            return value;
        }

        if name == "aic_for_into_iter" {
            return self.gen_for_into_iter_call(args, span, expected_ty, fctx);
        }
        if name == "aic_for_next_iter" {
            return self.gen_for_next_iter_call(args, span, expected_ty, fctx);
        }

        if name == "print_int" || name == "aic_io_print_int_intrinsic" {
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

        if name == "print_str" || name == "aic_io_print_str_intrinsic" {
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

        if name == "print_float" || name == "aic_io_print_float_intrinsic" {
            if args.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E5010",
                    "print_float expects one argument",
                    self.file,
                    span,
                ));
                return None;
            }
            let arg = self.gen_expr(&args[0], fctx)?;
            if !is_float_type(&arg.ty) {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    "print_float expects Float32, Float64, or Float",
                    self.file,
                    args[0].span,
                ));
                return None;
            }
            let arg = self.coerce_value_to_expected(arg, &LType::Float, args[0].span, fctx)?;
            let repr = arg.repr.unwrap_or_else(|| default_value(&LType::Float));
            fctx.lines
                .push(format!("  call void @aic_rt_print_float(double {})", repr));
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

        if let Some(result) = self.gen_string_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_char_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_math_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }

        if name == "panic" || name == "aic_io_panic_intrinsic" {
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

        if let Some(result) = self.gen_io_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }

        if let Some(result) = self.gen_time_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_signal_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_rand_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) =
            self.gen_concurrency_builtin_call(builtin_name, args, span, expected_ty, fctx)
        {
            return result;
        }
        if let Some(result) = self.gen_fs_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_env_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_map_builtin_call(builtin_name, args, span, expected_ty, fctx)
        {
            return result;
        }
        if let Some(result) = self.gen_vec_builtin_call(builtin_name, args, span, expected_ty, fctx)
        {
            return result;
        }
        if let Some(result) = self.gen_path_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_proc_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_net_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_tls_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_buffer_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_crypto_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_url_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_http_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_http_server_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_router_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_json_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }
        if let Some(result) = self.gen_regex_builtin_call(builtin_name, args, span, fctx) {
            return result;
        }

        let sig_hint = exact_sig.clone().or_else(|| self.fn_sig(name).cloned());
        let mut values = Vec::new();
        for (idx, expr) in args.iter().enumerate() {
            let expected_hint = sig_hint.as_ref().and_then(|sig| sig.params.get(idx));
            let value = if let Some(expected_hint) = expected_hint {
                self.gen_expr_with_expected(expr, Some(expected_hint), fctx)?
            } else {
                self.gen_expr(expr, fctx)?
            };
            values.push(value);
        }
        if let Some(instance) = self.resolve_generic_instance(name, &values, expected_ty, span) {
            let mut rendered_args = Vec::with_capacity(values.len());
            for (idx, (value, expected)) in values.iter().zip(instance.params.iter()).enumerate() {
                let Some(coerced) =
                    self.coerce_value_to_expected(value.clone(), expected, args[idx].span, fctx)
                else {
                    return None;
                };
                if !self.types_compatible_for_codegen(expected, &coerced.ty, args[idx].span) {
                    self.diagnostics.push(Diagnostic::error(
                        "E5014",
                        format!(
                            "argument type mismatch for call to '{}': expected '{}', found '{}'",
                            name,
                            render_type(expected),
                            render_type(&coerced.ty)
                        ),
                        self.file,
                        args[idx].span,
                    ));
                    return None;
                }
                rendered_args.push(format!(
                    "{} {}",
                    llvm_type(expected),
                    coerced
                        .repr
                        .clone()
                        .unwrap_or_else(|| default_value(expected))
                ));
            }

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
        let Some(sig) = exact_sig.clone().or_else(|| self.fn_sig(name).cloned()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{}' in codegen", name),
                self.file,
                span,
            ));
            return None;
        };

        if sig.is_intrinsic {
            let abi = sig
                .intrinsic_abi
                .clone()
                .unwrap_or_else(|| "<unknown>".to_string());
            self.diagnostics.push(
                Diagnostic::error(
                    "E5020",
                    format!(
                        "missing runtime lowering for intrinsic '{}' (abi '{}')",
                        name, abi
                    ),
                    self.file,
                    span,
                )
                .with_help(
                    "add backend lowering for this intrinsic or call a supported std API wrapper",
                ),
            );
            return None;
        }

        if values.len() != sig.params.len() {
            self.diagnostics.push(Diagnostic::error(
                "E5013",
                format!(
                    "call to '{}' arity mismatch: expected {}, got {}",
                    name,
                    sig.params.len(),
                    values.len()
                ),
                self.file,
                span,
            ));
            return None;
        }

        let mut rendered_args = Vec::new();
        for (idx, value) in values.iter().enumerate() {
            let expected = &sig.params[idx];
            let Some(coerced) =
                self.coerce_value_to_expected(value.clone(), expected, args[idx].span, fctx)
            else {
                return None;
            };
            if !self.types_compatible_for_codegen(expected, &coerced.ty, args[idx].span) {
                self.diagnostics.push(Diagnostic::error(
                    "E5014",
                    format!("argument type mismatch for call to '{}'", name),
                    self.file,
                    args[idx].span,
                ));
                return None;
            }
            rendered_args.push(format!(
                "{} {}",
                llvm_type(expected),
                coerced
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(expected))
            ));
        }

        let llvm_name = call_symbol
            .and_then(|symbol| self.fn_llvm_names.get(&symbol).cloned())
            .unwrap_or_else(|| mangle(name));
        if sig.ret == LType::Unit {
            fctx.lines.push(format!(
                "  call void @{}({})",
                llvm_name,
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
                llvm_name,
                rendered_args.join(", ")
            ));
            Some(Value {
                ty: sig.ret,
                repr: Some(reg),
            })
        }
    }

    pub(super) fn is_module_qualified_callee(&self, callee: &ir::Expr, fctx: &FnCtx) -> bool {
        let Some(path) = extract_callee_path(callee) else {
            return false;
        };
        if path.len() < 2 {
            return false;
        }
        let qualifier = &path[..path.len() - 1];
        if qualifier.len() == 1 && find_local(&fctx.vars, &qualifier[0]).is_some() {
            return false;
        }
        let qualifier_joined = qualifier.join(".");
        if qualifier_joined == "std" || qualifier_joined.starts_with("std.") {
            return true;
        }
        if self
            .program
            .imports
            .iter()
            .any(|import| import.join(".") == qualifier_joined)
        {
            return true;
        }
        if qualifier.len() == 1 {
            let alias = &qualifier[0];
            return self
                .program
                .imports
                .iter()
                .any(|import| import.last().map(|tail| tail == alias).unwrap_or(false));
        }
        false
    }

    pub(super) fn gen_method_call(
        &mut self,
        base: &ir::Expr,
        field: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let receiver = self.gen_expr(base, fctx)?;
        if let LType::DynTrait(trait_name) = &receiver.ty {
            let trait_name = trait_name.clone();
            return self.gen_dyn_trait_method_call(receiver, &trait_name, field, args, span, fctx);
        }
        let receiver_type_name = self.method_receiver_type_name(&receiver, base.span)?;

        let associated = format!("{receiver_type_name}::{field}");
        let mut values = Vec::with_capacity(args.len() + 1);
        values.push(receiver);
        for arg in args {
            values.push(self.gen_expr(arg, fctx)?);
        }
        self.gen_named_function_call_with_values(&associated, values, span, fctx)
    }

    fn has_callable_name(&self, name: &str) -> bool {
        self.fn_sigs.contains_key(name)
            || self.generic_fn_instances.contains_key(name)
            || self.callable_declared_in_program(name)
    }

    fn callable_declared_in_program(&self, name: &str) -> bool {
        self.program.items.iter().any(|item| match item {
            ir::Item::Function(func) => func.name == name,
            ir::Item::Impl(impl_def) => impl_def.methods.iter().any(|method| method.name == name),
            _ => false,
        })
    }

    fn method_receiver_type_name(
        &mut self,
        receiver: &Value,
        span: crate::span::Span,
    ) -> Option<String> {
        match &receiver.ty {
            LType::Struct(layout) => {
                let base = base_type_name(&layout.repr);
                if base == "Ref" || base == "RefMut" {
                    Some(
                        extract_generic_args(&layout.repr)
                            .and_then(|args| args.first().cloned())
                            .map(|inner| base_type_name(&inner).to_string())
                            .unwrap_or_else(|| base.to_string()),
                    )
                } else {
                    Some(base.to_string())
                }
            }
            LType::Enum(layout) => Some(base_type_name(&layout.repr).to_string()),
            LType::Int => Some("Int".to_string()),
            LType::ISize => Some("ISize".to_string()),
            LType::USize => Some("USize".to_string()),
            LType::Int8 => Some("Int8".to_string()),
            LType::Int16 => Some("Int16".to_string()),
            LType::Int32 => Some("Int32".to_string()),
            LType::Int64 => Some("Int64".to_string()),
            LType::Int128 => Some("Int128".to_string()),
            LType::UInt8 => Some("UInt8".to_string()),
            LType::UInt16 => Some("UInt16".to_string()),
            LType::UInt32 => Some("UInt32".to_string()),
            LType::UInt64 => Some("UInt64".to_string()),
            LType::UInt128 => Some("UInt128".to_string()),
            LType::Float32 => Some("Float32".to_string()),
            LType::Float64 => Some("Float64".to_string()),
            LType::Float => Some("Float".to_string()),
            LType::Bool => Some("Bool".to_string()),
            LType::String => Some("String".to_string()),
            LType::Unit => Some("()".to_string()),
            LType::DynTrait(trait_name) => Some(format!("dyn {}", trait_name)),
            other => {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("type '{other:?}' does not support method call syntax"),
                    self.file,
                    span,
                ));
                None
            }
        }
    }

    pub(super) fn types_compatible_for_codegen(
        &mut self,
        expected: &LType,
        found: &LType,
        span: crate::span::Span,
    ) -> bool {
        if expected == found {
            return true;
        }
        if float_types_compatible(expected, found) {
            return true;
        }

        match (expected, found) {
            (LType::DynTrait(expected_trait), LType::DynTrait(found_trait)) => {
                expected_trait == found_trait
            }
            (LType::DynTrait(expected_trait), concrete) => {
                let concrete_repr = render_type(concrete);
                self.ensure_dyn_trait_info(expected_trait, span)
                    .map(|info| info.impl_methods.contains_key(&concrete_repr))
                    .unwrap_or(false)
            }
            (LType::Async(expected_inner), LType::Async(found_inner)) => {
                self.types_compatible_for_codegen(expected_inner, found_inner, span)
            }
            (LType::Enum(expected_layout), LType::Enum(found_layout)) => {
                if expected_layout.repr != found_layout.repr
                    || expected_layout.variants.len() != found_layout.variants.len()
                {
                    return false;
                }
                expected_layout
                    .variants
                    .iter()
                    .zip(found_layout.variants.iter())
                    .all(|(exp, got)| match (&exp.payload, &got.payload) {
                        (Some(exp_payload), Some(got_payload)) => {
                            self.types_compatible_for_codegen(exp_payload, got_payload, span)
                        }
                        (None, None) => true,
                        _ => false,
                    })
            }
            _ => false,
        }
    }

    pub(super) fn coerce_value_to_expected(
        &mut self,
        value: Value,
        expected: &LType,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if value.ty == *expected {
            return Some(value);
        }
        if is_integral_type(&value.ty) && is_integral_type(expected) {
            return Some(self.coerce_integral_value(value, expected, fctx));
        }
        if is_float_type(&value.ty) && is_float_type(expected) {
            return Some(self.coerce_float_value(value, expected, fctx));
        }
        match expected {
            LType::DynTrait(trait_name) => {
                self.coerce_value_to_dyn_trait(value, trait_name, span, fctx)
            }
            _ => Some(value),
        }
    }

    fn coerce_integral_value(&mut self, value: Value, expected: &LType, fctx: &mut FnCtx) -> Value {
        let Some(src_bits) = integer_width_bits(&value.ty) else {
            return value;
        };
        let Some(dst_bits) = integer_width_bits(expected) else {
            return value;
        };
        let repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        if src_bits == dst_bits {
            return Value {
                ty: expected.clone(),
                repr: Some(repr),
            };
        }

        let casted = self.new_temp();
        if src_bits > dst_bits {
            fctx.lines.push(format!(
                "  {} = trunc {} {} to {}",
                casted,
                llvm_type(&value.ty),
                repr,
                llvm_type(expected)
            ));
        } else {
            let op = if is_unsigned_integer_type(&value.ty) {
                "zext"
            } else {
                "sext"
            };
            fctx.lines.push(format!(
                "  {} = {} {} {} to {}",
                casted,
                op,
                llvm_type(&value.ty),
                repr,
                llvm_type(expected)
            ));
        }

        Value {
            ty: expected.clone(),
            repr: Some(casted),
        }
    }

    fn coerce_float_value(&mut self, value: Value, expected: &LType, fctx: &mut FnCtx) -> Value {
        let Some(src_bits) = float_width_bits(&value.ty) else {
            return value;
        };
        let Some(dst_bits) = float_width_bits(expected) else {
            return value;
        };
        let repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        if src_bits == dst_bits {
            return Value {
                ty: expected.clone(),
                repr: Some(repr),
            };
        }

        let casted = self.new_temp();
        let op = if src_bits > dst_bits {
            "fptrunc"
        } else {
            "fpext"
        };
        fctx.lines.push(format!(
            "  {} = {} {} {} to {}",
            casted,
            op,
            llvm_type(&value.ty),
            repr,
            llvm_type(expected)
        ));
        Value {
            ty: expected.clone(),
            repr: Some(casted),
        }
    }

    fn coerce_value_to_dyn_trait(
        &mut self,
        value: Value,
        trait_name: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if let LType::DynTrait(found_trait) = &value.ty {
            if found_trait == trait_name {
                return Some(value);
            }
            self.diagnostics.push(
                Diagnostic::error(
                    "E5014",
                    format!(
                        "cannot coerce dyn '{}' to dyn '{}'",
                        found_trait, trait_name
                    ),
                    self.file,
                    span,
                )
                .with_help("coerce from a concrete type that implements the target trait"),
            );
            return None;
        }
        if value.ty == LType::Unit {
            self.diagnostics.push(Diagnostic::error(
                "E5014",
                "cannot coerce unit value to dyn trait object",
                self.file,
                span,
            ));
            return None;
        }

        let concrete_ty = value.ty.clone();
        let (vtable_global, trait_info) =
            self.ensure_dyn_vtable_for_concrete(trait_name, &concrete_ty, span)?;

        self.extern_decls
            .insert("declare i8* @malloc(i64)".to_string());
        let size = self.vec_elem_size(&concrete_ty, fctx);
        let data_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = call i8* @malloc(i64 {})", data_ptr, size));
        let typed_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to {}*",
            typed_ptr,
            data_ptr,
            llvm_type(&concrete_ty)
        ));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&concrete_ty),
            value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&concrete_ty)),
            llvm_type(&concrete_ty),
            typed_ptr
        ));

        let vtable_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast [{} x i8*]* @{} to i8*",
            vtable_ptr,
            trait_info.methods.len(),
            vtable_global
        ));

        let dyn_ty = LType::DynTrait(trait_name.to_string());
        let with_data = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} undef, i8* {}, 0",
            with_data,
            llvm_type(&dyn_ty),
            data_ptr
        ));
        let with_vtable = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} {}, i8* {}, 1",
            with_vtable,
            llvm_type(&dyn_ty),
            with_data,
            vtable_ptr
        ));

        Some(Value {
            ty: dyn_ty,
            repr: Some(with_vtable),
        })
    }

    fn ensure_dyn_trait_info(
        &mut self,
        trait_name: &str,
        span: crate::span::Span,
    ) -> Option<DynTraitInfo> {
        if let Some(info) = self.dyn_traits.get(trait_name).cloned() {
            return Some(info);
        }

        let short_name = method_base_name(trait_name).to_string();
        let trait_def = self.program.items.iter().find_map(|item| match item {
            ir::Item::Trait(def) if def.name == trait_name || def.name == short_name => Some(def),
            _ => None,
        })?;

        let resolved_trait_name = trait_def.name.clone();
        let mut methods = Vec::new();
        let mut method_index = BTreeMap::new();
        for method in &trait_def.methods {
            if !method.generics.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E5019",
                        format!(
                            "dyn trait '{}' method '{}' cannot be generic",
                            resolved_trait_name,
                            method_base_name(&method.name)
                        ),
                        self.file,
                        method.span,
                    )
                    .with_help("remove method generics for dyn dispatch"),
                );
                return None;
            }
            if method.params.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!(
                        "dyn trait '{}' method '{}' is missing receiver parameter",
                        resolved_trait_name,
                        method_base_name(&method.name)
                    ),
                    self.file,
                    method.span,
                ));
                return None;
            }

            let receiver_repr = self
                .type_map
                .get(&method.params[0].ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            if receiver_repr.trim() != "Self" {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E5019",
                        format!(
                            "dyn trait '{}' method '{}' is not object-safe: first parameter must be Self",
                            resolved_trait_name,
                            method_base_name(&method.name)
                        ),
                        self.file,
                        method.span,
                    )
                    .with_help("use `self: Self` as the first parameter"),
                );
                return None;
            }

            let mut param_tys = Vec::new();
            for param in method.params.iter().skip(1) {
                let raw = self
                    .type_map
                    .get(&param.ty)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string());
                if type_uses_self_repr(&raw) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E5019",
                            format!(
                                "dyn trait '{}' method '{}' is not object-safe: Self appears outside receiver",
                                resolved_trait_name,
                                method_base_name(&method.name)
                            ),
                            self.file,
                            method.span,
                        )
                        .with_help("remove `Self` from non-receiver parameters"),
                    );
                    return None;
                }
                param_tys.push(self.type_from_id(param.ty, param.span)?);
            }

            let ret_raw = self
                .type_map
                .get(&method.ret_type)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            if type_uses_self_repr(&ret_raw) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E5019",
                        format!(
                            "dyn trait '{}' method '{}' is not object-safe: Self appears in return type",
                            resolved_trait_name,
                            method_base_name(&method.name)
                        ),
                        self.file,
                        method.span,
                    )
                    .with_help("use concrete return types for dyn dispatch"),
                );
                return None;
            }
            let mut ret = self.type_from_id(method.ret_type, method.span)?;
            if method.is_async {
                ret = LType::Async(Box::new(ret));
            }

            let name = method_base_name(&method.name).to_string();
            if method_index.contains_key(&name) {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!(
                        "duplicate dyn trait method '{}' in trait '{}'",
                        name, resolved_trait_name
                    ),
                    self.file,
                    method.span,
                ));
                return None;
            }
            method_index.insert(name.clone(), methods.len());
            methods.push(DynTraitMethodInfo {
                name,
                params: param_tys,
                ret,
            });
        }

        let mut impl_methods: BTreeMap<String, BTreeMap<String, ir::SymbolId>> = BTreeMap::new();
        for item in &self.program.items {
            let ir::Item::Impl(impl_def) = item else {
                continue;
            };
            if impl_def.is_inherent {
                continue;
            }
            if impl_def.trait_name != resolved_trait_name && impl_def.trait_name != short_name {
                continue;
            }
            let target = if let Some(target) = impl_def.target {
                Some(target)
            } else if trait_def.generics.is_empty() {
                impl_def.trait_args.first().copied()
            } else {
                None
            };
            let Some(target) = target else {
                continue;
            };
            let target_raw = self
                .type_map
                .get(&target)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            let Some(target_repr) = self.normalize_type_repr(&target_raw, span) else {
                continue;
            };
            let mut methods_for_impl = BTreeMap::new();
            for method in &impl_def.methods {
                methods_for_impl.insert(method_base_name(&method.name).to_string(), method.symbol);
            }
            impl_methods.insert(target_repr, methods_for_impl);
        }

        let info = DynTraitInfo {
            methods,
            method_index,
            impl_methods,
        };
        self.dyn_traits
            .insert(resolved_trait_name.clone(), info.clone());
        if trait_name != resolved_trait_name {
            self.dyn_traits.insert(trait_name.to_string(), info.clone());
        }
        Some(info)
    }

    fn emit_dyn_wrapper_if_needed(
        &mut self,
        wrapper_llvm_name: &str,
        impl_llvm_name: &str,
        concrete_ty: &LType,
        method: &DynTraitMethodInfo,
    ) {
        if !self
            .generated_dyn_wrappers
            .insert(wrapper_llvm_name.to_string())
        {
            return;
        }

        let ret_llvm = llvm_type(&method.ret);
        let mut param_defs = vec!["i8* %arg0".to_string()];
        param_defs.extend(
            method
                .params
                .iter()
                .enumerate()
                .map(|(idx, ty)| format!("{} %arg{}", llvm_type(ty), idx + 1)),
        );

        let mut lines = vec![format!(
            "define {} @{}({}) {{",
            ret_llvm,
            wrapper_llvm_name,
            param_defs.join(", ")
        )];
        lines.push("entry:".to_string());

        let concrete_llvm = llvm_type(concrete_ty);
        let self_ptr = self.new_temp();
        lines.push(format!(
            "  {} = bitcast i8* %arg0 to {}*",
            self_ptr, concrete_llvm
        ));
        let self_val = self.new_temp();
        lines.push(format!(
            "  {} = load {}, {}* {}",
            self_val, concrete_llvm, concrete_llvm, self_ptr
        ));

        let mut call_args = vec![format!("{} {}", concrete_llvm, self_val)];
        call_args.extend(
            method
                .params
                .iter()
                .enumerate()
                .map(|(idx, ty)| format!("{} %arg{}", llvm_type(ty), idx + 1)),
        );

        if method.ret == LType::Unit {
            lines.push(format!(
                "  call void @{}({})",
                impl_llvm_name,
                call_args.join(", ")
            ));
            lines.push("  ret void".to_string());
        } else {
            let out = self.new_temp();
            lines.push(format!(
                "  {} = call {} @{}({})",
                out,
                ret_llvm,
                impl_llvm_name,
                call_args.join(", ")
            ));
            lines.push(format!("  ret {} {}", ret_llvm, out));
        }
        lines.push("}".to_string());
        self.deferred_fn_defs.push(lines);
    }

    fn ensure_dyn_vtable_for_concrete(
        &mut self,
        trait_name: &str,
        concrete_ty: &LType,
        span: crate::span::Span,
    ) -> Option<(String, DynTraitInfo)> {
        let concrete_repr = render_type(concrete_ty);
        let dyn_info = self.ensure_dyn_trait_info(trait_name, span)?;
        let vtable_key = format!("{}|{}", trait_name, concrete_repr);
        if let Some(global) = self.dyn_vtable_globals.get(&vtable_key).cloned() {
            return Some((global, dyn_info));
        }

        let Some(impl_methods) = dyn_info.impl_methods.get(&concrete_repr) else {
            self.diagnostics.push(
                Diagnostic::error(
                    "E5014",
                    format!(
                        "type '{}' does not implement dyn trait '{}'",
                        concrete_repr, trait_name
                    ),
                    self.file,
                    span,
                )
                .with_help("add a matching trait impl before coercing to dyn"),
            );
            return None;
        };

        let mut entry_exprs = Vec::new();
        for method in &dyn_info.methods {
            let Some(symbol) = impl_methods.get(&method.name) else {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E5014",
                        format!(
                            "impl for '{}' is missing method '{}' required by dyn trait '{}'",
                            concrete_repr, method.name, trait_name
                        ),
                        self.file,
                        span,
                    )
                    .with_help("implement all trait methods for dyn dispatch"),
                );
                return None;
            };
            let Some(impl_llvm_name) = self.fn_llvm_names.get(symbol).cloned() else {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    "internal codegen error: missing LLVM name for dyn impl method",
                    self.file,
                    span,
                ));
                return None;
            };

            let wrapper_llvm_name = format!(
                "aic_dynwrap_{}_{}_{}",
                mangle_generic_component(trait_name),
                mangle_generic_component(&method.name),
                mangle_generic_component(&concrete_repr)
            );
            self.emit_dyn_wrapper_if_needed(
                &wrapper_llvm_name,
                &impl_llvm_name,
                concrete_ty,
                method,
            );

            let wrapper_fn_ty = dyn_wrapper_function_type(method);
            entry_exprs.push(format!(
                "i8* bitcast ({}* @{} to i8*)",
                wrapper_fn_ty, wrapper_llvm_name
            ));
        }

        let global_name = format!(
            "aic_dyn_vtable_{}_{}",
            mangle_generic_component(trait_name),
            mangle_generic_component(&concrete_repr)
        );
        self.globals.push(format!(
            "@{} = private unnamed_addr constant [{} x i8*] [{}]",
            global_name,
            entry_exprs.len(),
            entry_exprs.join(", ")
        ));
        self.dyn_vtable_globals
            .insert(vtable_key, global_name.clone());
        Some((global_name, dyn_info))
    }

    fn gen_dyn_trait_method_call(
        &mut self,
        receiver: Value,
        trait_name: &str,
        field: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let dyn_info = self.ensure_dyn_trait_info(trait_name, span)?;
        let method_name = method_base_name(field);
        let Some(method_idx) = dyn_info.method_index.get(method_name).copied() else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown dyn trait method '{}.{}'", trait_name, method_name),
                self.file,
                span,
            ));
            return None;
        };
        let method = dyn_info.methods.get(method_idx).cloned()?;

        if args.len() != method.params.len() {
            self.diagnostics.push(Diagnostic::error(
                "E5013",
                format!(
                    "method '{}' expects {} args, got {}",
                    method_name,
                    method.params.len(),
                    args.len()
                ),
                self.file,
                span,
            ));
            return None;
        }

        let mut call_args = Vec::new();
        for (arg, expected_ty) in args.iter().zip(method.params.iter()) {
            let value = self.gen_expr_with_expected(arg, Some(expected_ty), fctx)?;
            let value = self.coerce_value_to_expected(value, expected_ty, arg.span, fctx)?;
            if value.ty != *expected_ty {
                self.diagnostics.push(Diagnostic::error(
                    "E5014",
                    format!(
                        "dyn method argument type mismatch: expected '{}', found '{}'",
                        render_type(expected_ty),
                        render_type(&value.ty)
                    ),
                    self.file,
                    arg.span,
                ));
                return None;
            }
            call_args.push(value);
        }

        let obj_slot = self.new_temp();
        fctx.lines.push(format!(
            "  {} = alloca {}",
            obj_slot,
            llvm_type(&receiver.ty)
        ));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&receiver.ty),
            receiver
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&receiver.ty)),
            llvm_type(&receiver.ty),
            obj_slot
        ));

        let data_ptr_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr {}, {}* {}, i32 0, i32 0",
            data_ptr_ptr,
            llvm_type(&receiver.ty),
            llvm_type(&receiver.ty),
            obj_slot
        ));
        let data_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", data_ptr, data_ptr_ptr));

        let vtable_ptr_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr {}, {}* {}, i32 0, i32 1",
            vtable_ptr_ptr,
            llvm_type(&receiver.ty),
            llvm_type(&receiver.ty),
            obj_slot
        ));
        let vtable_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            vtable_ptr, vtable_ptr_ptr
        ));

        let entries_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to i8**",
            entries_ptr, vtable_ptr
        ));
        let entry_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr i8*, i8** {}, i64 {}",
            entry_ptr, entries_ptr, method_idx
        ));
        let fn_i8 = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", fn_i8, entry_ptr));

        let dyn_fn_ty = dyn_wrapper_function_type(&method);
        let fn_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to {}*",
            fn_ptr, fn_i8, dyn_fn_ty
        ));

        let mut rendered_args = vec![format!("i8* {}", data_ptr)];
        rendered_args.extend(call_args.iter().zip(method.params.iter()).map(|(arg, ty)| {
            format!(
                "{} {}",
                llvm_type(ty),
                arg.repr.clone().unwrap_or_else(|| default_value(ty))
            )
        }));

        if method.ret == LType::Unit {
            fctx.lines.push(format!(
                "  call {} {}({})",
                dyn_fn_ty,
                fn_ptr,
                rendered_args.join(", ")
            ));
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        let out = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call {} {}({})",
            out,
            llvm_type(&method.ret),
            fn_ptr,
            rendered_args.join(", ")
        ));
        Some(Value {
            ty: method.ret,
            repr: Some(out),
        })
    }

    pub(super) fn gen_for_into_iter_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "'aic_for_into_iter' expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let receiver = self.gen_expr(&args[0], fctx)?;
        let receiver_name = self.method_receiver_type_name(&receiver, args[0].span)?;
        let iter_assoc = format!("{receiver_name}::iter");
        if self.has_callable_name(&iter_assoc) {
            return self.gen_named_function_call_with_values(
                &iter_assoc,
                vec![receiver],
                span,
                fctx,
            );
        }
        let next_assoc = format!("{receiver_name}::next");
        if self.has_callable_name(&next_assoc) {
            return Some(receiver);
        }
        self.diagnostics.push(
            Diagnostic::error(
                "E5012",
                format!(
                    "for-in source type '{}' is not iterable (missing '{}.iter' or '{}.next')",
                    receiver_name, receiver_name, receiver_name
                ),
                self.file,
                span,
            )
            .with_help("implement iterator methods for this type"),
        );
        None
    }

    pub(super) fn gen_for_next_iter_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "'aic_for_next_iter' expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let receiver = self.gen_expr(&args[0], fctx)?;
        let receiver_name = self.method_receiver_type_name(&receiver, args[0].span)?;
        let next_assoc = format!("{receiver_name}::next");
        if !self.has_callable_name(&next_assoc) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E5012",
                    format!(
                        "for-in iterator type '{}' does not define '{}.next'",
                        receiver_name, receiver_name
                    ),
                    self.file,
                    span,
                )
                .with_help("implement `next` for this iterator type"),
            );
            return None;
        }
        self.gen_named_function_call_with_values(&next_assoc, vec![receiver], span, fctx)
    }

    pub(super) fn gen_named_function_call_with_values(
        &mut self,
        name: &str,
        values: Vec<Value>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if let Some(instance) = self.resolve_generic_instance(name, &values, None, span) {
            let mut rendered_args = Vec::with_capacity(values.len());
            for (value, expected) in values.iter().zip(instance.params.iter()) {
                let Some(coerced) =
                    self.coerce_value_to_expected(value.clone(), expected, span, fctx)
                else {
                    return None;
                };
                if !self.types_compatible_for_codegen(expected, &coerced.ty, span) {
                    self.diagnostics.push(Diagnostic::error(
                        "E5014",
                        format!(
                            "argument type mismatch for call to '{}': expected '{}', found '{}'",
                            name,
                            render_type(expected),
                            render_type(&coerced.ty)
                        ),
                        self.file,
                        span,
                    ));
                    return None;
                }
                rendered_args.push(format!(
                    "{} {}",
                    llvm_type(expected),
                    coerced
                        .repr
                        .clone()
                        .unwrap_or_else(|| default_value(expected))
                ));
            }

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

        let Some(sig) = self.fn_sig(name).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{}' in codegen", name),
                self.file,
                span,
            ));
            return None;
        };

        if values.len() != sig.params.len() {
            self.diagnostics.push(Diagnostic::error(
                "E5013",
                format!(
                    "call to '{}' arity mismatch: expected {}, got {}",
                    name,
                    sig.params.len(),
                    values.len()
                ),
                self.file,
                span,
            ));
            return None;
        }

        let mut rendered_args = Vec::new();
        for (idx, value) in values.iter().enumerate() {
            let expected = &sig.params[idx];
            let Some(coerced) = self.coerce_value_to_expected(value.clone(), expected, span, fctx)
            else {
                return None;
            };
            if !self.types_compatible_for_codegen(expected, &coerced.ty, span) {
                self.diagnostics.push(Diagnostic::error(
                    "E5014",
                    format!(
                        "argument type mismatch for call to '{}': expected '{}', found '{}'",
                        name,
                        render_type(expected),
                        render_type(&coerced.ty)
                    ),
                    self.file,
                    span,
                ));
                return None;
            }
            rendered_args.push(format!(
                "{} {}",
                llvm_type(expected),
                coerced
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(expected))
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

    pub(super) fn resolve_generic_instance(
        &mut self,
        name: &str,
        values: &[Value],
        expected_ret: Option<&LType>,
        span: crate::span::Span,
    ) -> Option<GenericFnInstance> {
        if let Some(instances) = self.generic_fn_instances.get(name) {
            let mut matches = instances
                .iter()
                .filter(|inst| {
                    inst.params.len() == values.len()
                        && inst
                            .params
                            .iter()
                            .zip(values.iter())
                            .all(|(expected, value)| *expected == value.ty)
                })
                .collect::<Vec<_>>();
            if let Some(expected) = expected_ret {
                matches.retain(|inst| inst.ret == *expected);
            }
            match matches.len() {
                0 => {}
                1 => return Some((*matches[0]).clone()),
                _ => {
                    self.diagnostics.push(Diagnostic::error(
                        "E5014",
                        format!("ambiguous generic call to '{}'", name),
                        self.file,
                        span,
                    ));
                    return None;
                }
            }
        }
        self.instantiate_generic_instance_on_demand(name, values, expected_ret, span)
    }

    pub(super) fn instantiate_generic_instance_on_demand(
        &mut self,
        name: &str,
        values: &[Value],
        expected_ret: Option<&LType>,
        span: crate::span::Span,
    ) -> Option<GenericFnInstance> {
        let mut matches: Vec<(ir::Function, GenericFnInstance)> = Vec::new();
        for item in &self.program.items {
            match item {
                ir::Item::Function(func) => {
                    self.collect_on_demand_instance_candidate(
                        func,
                        name,
                        values,
                        expected_ret,
                        span,
                        &mut matches,
                    );
                }
                ir::Item::Impl(impl_def) => {
                    for method in &impl_def.methods {
                        self.collect_on_demand_instance_candidate(
                            method,
                            name,
                            values,
                            expected_ret,
                            span,
                            &mut matches,
                        );
                    }
                }
                _ => {}
            }
        }

        if matches.is_empty() {
            return None;
        }
        if matches.len() > 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5014",
                format!("argument type mismatch for generic call to '{}'", name),
                self.file,
                span,
            ));
            return None;
        }

        let (func, instance) = matches.remove(0);
        let inserted = self.register_generic_instance(name, &instance);
        if inserted {
            self.defer_monomorphized_function(&func, &instance);
        }
        Some(instance)
    }

    pub(super) fn collect_on_demand_instance_candidate(
        &mut self,
        func: &ir::Function,
        name: &str,
        values: &[Value],
        expected_ret: Option<&LType>,
        span: crate::span::Span,
        out: &mut Vec<(ir::Function, GenericFnInstance)>,
    ) {
        if func.name != name || func.generics.is_empty() || func.params.len() != values.len() {
            return;
        }

        let _module_guard = self.type_module_guard_for_symbol(func.symbol);
        let generic_names = func
            .generics
            .iter()
            .map(|g| g.name.clone())
            .collect::<Vec<_>>();
        let mut bindings = BTreeMap::new();
        for (param, value) in func.params.iter().zip(values.iter()) {
            let raw = self
                .type_map
                .get(&param.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            infer_generic_bindings(&raw, &render_type(&value.ty), &generic_names, &mut bindings);
        }
        if let Some(expected) = expected_ret {
            let expected_rendered = if func.is_async {
                let LType::Async(inner) = expected else {
                    return;
                };
                render_type(inner)
            } else {
                render_type(expected)
            };
            let ret_raw = self
                .type_map
                .get(&func.ret_type)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            if !infer_generic_bindings(&ret_raw, &expected_rendered, &generic_names, &mut bindings)
            {
                return;
            }
        }

        for generic in &generic_names {
            if bindings.contains_key(generic) {
                continue;
            }
            if let Some(active) = self
                .active_type_bindings
                .as_ref()
                .and_then(|map| map.get(generic))
                .cloned()
            {
                bindings.insert(generic.clone(), active);
            }
        }
        // Some std trait methods carry placeholder generics that do not appear in
        // argument or return types. Bind those to a concrete fallback so
        // monomorphization can proceed for call sites that otherwise resolve
        // unambiguously from arguments.
        let param_reprs = func
            .params
            .iter()
            .map(|param| {
                self.type_map
                    .get(&param.ty)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string())
            })
            .collect::<Vec<_>>();
        let ret_repr = self
            .type_map
            .get(&func.ret_type)
            .cloned()
            .unwrap_or_else(|| "<?>".to_string());
        for generic in &generic_names {
            if bindings.contains_key(generic) {
                continue;
            }
            let used_in_sig = param_reprs
                .iter()
                .any(|param_repr| type_uses_generic(param_repr, generic))
                || type_uses_generic(&ret_repr, generic);
            if !used_in_sig {
                bindings.insert(generic.clone(), "Int".to_string());
            }
        }
        if generic_names
            .iter()
            .any(|generic| !bindings.contains_key(generic))
        {
            return;
        }

        let mut params = Vec::with_capacity(func.params.len());
        for param in &func.params {
            let raw = self
                .type_map
                .get(&param.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            let concrete = substitute_type_vars(&raw, &bindings);
            let Some(ty) = self.parse_type_repr(&concrete, span) else {
                return;
            };
            params.push(ty);
        }
        for (expected, actual) in params.iter().zip(values.iter()) {
            if !self.types_compatible_for_codegen(expected, &actual.ty, span) {
                return;
            }
        }

        let ret_raw = self
            .type_map
            .get(&func.ret_type)
            .cloned()
            .unwrap_or_else(|| "<?>".to_string());
        let ret_concrete = substitute_type_vars(&ret_raw, &bindings);
        let Some(mut ret) = self.parse_type_repr(&ret_concrete, span) else {
            return;
        };
        if func.is_async {
            ret = LType::Async(Box::new(ret));
        }

        let type_args = generic_names
            .iter()
            .map(|generic| bindings.get(generic).cloned().unwrap_or_default())
            .collect::<Vec<_>>();
        let instance = GenericFnInstance {
            mangled: mangle_generic_instantiation("fn", &func.name, &type_args),
            params,
            ret,
            bindings,
        };
        out.push((func.clone(), instance));
    }

    pub(super) fn register_generic_instance(
        &mut self,
        name: &str,
        instance: &GenericFnInstance,
    ) -> bool {
        let entry = self
            .generic_fn_instances
            .entry(name.to_string())
            .or_default();
        if entry
            .iter()
            .any(|existing| existing.mangled == instance.mangled)
        {
            return false;
        }
        entry.push(instance.clone());
        true
    }

    pub(super) fn defer_monomorphized_function(
        &mut self,
        func: &ir::Function,
        inst: &GenericFnInstance,
    ) {
        if func.is_extern || func.is_intrinsic {
            return;
        }
        let start = self.out.len();
        self.gen_monomorphized_function(func, inst);
        let lines = self.out.split_off(start);
        if !lines.is_empty() {
            self.deferred_fn_defs.push(lines);
        }
    }

    pub(super) fn fn_layout_from_signature(&self, sig: &FnSig) -> FnLayoutType {
        FnLayoutType {
            repr: render_applied_type("Fn", &{
                let mut all = sig.params.clone();
                all.push(sig.ret.clone());
                all
            }),
            params: sig.params.clone(),
            ret: Box::new(sig.ret.clone()),
        }
    }

    pub(super) fn gen_function_value(
        &mut self,
        name: &str,
        sig: &FnSig,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let has_definition = self.program.items.iter().any(|item| {
            matches!(
                item,
                ir::Item::Function(func) if func.generics.is_empty() && func.name == name
            )
        });
        if !has_definition {
            self.diagnostics.push(Diagnostic::error(
                "E5034",
                format!(
                    "function '{}' cannot be lowered as a first-class value in codegen",
                    name
                ),
                self.file,
                span,
            ));
            return None;
        }

        let adapter = self.ensure_fn_value_adapter(name, sig);
        let layout = self.fn_layout_from_signature(sig);
        self.build_fn_value_from_symbol(&adapter, &layout, "null", fctx)
    }

    pub(super) fn ensure_fn_value_adapter(&mut self, name: &str, sig: &FnSig) -> String {
        let key = format!(
            "{}({})->{}",
            name,
            sig.params
                .iter()
                .map(render_type)
                .collect::<Vec<_>>()
                .join(","),
            render_type(&sig.ret)
        );
        if let Some(existing) = self.fn_value_adapters.get(&key) {
            return existing.clone();
        }

        let target = mangle(name);
        let adapter = format!(
            "__aic_fn_adapter_{}_{}",
            mangle(name),
            self.fn_value_adapters.len()
        );
        let mut lines = Vec::new();
        let mut params = vec!["i8* %env".to_string()];
        params.extend(
            sig.params
                .iter()
                .enumerate()
                .map(|(idx, ty)| format!("{} %arg{}", llvm_type(ty), idx)),
        );
        let call_args = sig
            .params
            .iter()
            .enumerate()
            .map(|(idx, ty)| format!("{} %arg{}", llvm_type(ty), idx))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "define {} @{}({}) {{",
            llvm_type(&sig.ret),
            adapter,
            params.join(", ")
        ));
        lines.push("entry:".to_string());
        lines.push("  ; adapter ignores closure env for plain functions".to_string());
        lines.push("  %env.ignore = ptrtoint i8* %env to i64".to_string());
        if sig.ret == LType::Unit {
            lines.push(format!("  call void @{}({})", target, call_args));
            lines.push("  ret void".to_string());
        } else {
            let tmp = self.new_temp();
            lines.push(format!(
                "  {} = call {} @{}({})",
                tmp,
                llvm_type(&sig.ret),
                target,
                call_args
            ));
            lines.push(format!("  ret {} {}", llvm_type(&sig.ret), tmp));
        }
        lines.push("}".to_string());
        self.deferred_fn_defs.push(lines);
        self.fn_value_adapters.insert(key, adapter.clone());
        adapter
    }

    pub(super) fn build_fn_value_from_symbol(
        &mut self,
        symbol: &str,
        layout: &FnLayoutType,
        env_ptr: &str,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let fn_sig_text = format!(
            "{} (i8*{})*",
            llvm_type(&layout.ret),
            if layout.params.is_empty() {
                String::new()
            } else {
                format!(
                    ", {}",
                    layout
                        .params
                        .iter()
                        .map(llvm_type)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        );
        let fn_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast {} @{} to i8*",
            fn_ptr, fn_sig_text, symbol
        ));
        let pair_ty = llvm_type(&LType::Fn(layout.clone()));
        let v0 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} undef, i8* {}, 0",
            v0, pair_ty, fn_ptr
        ));
        let v1 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = insertvalue {} {}, i8* {}, 1",
            v1, pair_ty, v0, env_ptr
        ));
        Some(Value {
            ty: LType::Fn(layout.clone()),
            repr: Some(v1),
        })
    }

    pub(super) fn gen_fn_value_call(
        &mut self,
        callee: Value,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Fn(layout) = callee.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5032",
                "indirect call expects callee of type Fn(...) -> ...",
                self.file,
                span,
            ));
            return None;
        };

        if args.len() != layout.params.len() {
            self.diagnostics.push(Diagnostic::error(
                "E5013",
                format!(
                    "function value call arity mismatch: expected {}, got {}",
                    layout.params.len(),
                    args.len()
                ),
                self.file,
                span,
            ));
            return None;
        }

        let mut rendered = Vec::new();
        for (idx, expr) in args.iter().enumerate() {
            let value = self.gen_expr(expr, fctx)?;
            let expected = &layout.params[idx];
            if value.ty != *expected {
                self.diagnostics.push(Diagnostic::error(
                    "E5014",
                    format!(
                        "indirect call argument {} expected '{}', found '{}'",
                        idx + 1,
                        render_type(expected),
                        render_type(&value.ty)
                    ),
                    self.file,
                    expr.span,
                ));
                return None;
            }
            rendered.push(format!(
                "{} {}",
                llvm_type(expected),
                value.repr.unwrap_or_else(|| default_value(expected))
            ));
        }

        let callee_repr = callee
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&callee.ty));
        let pair_ty = llvm_type(&callee.ty);

        let fn_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            fn_raw, pair_ty, callee_repr
        ));
        let env_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 1",
            env_raw, pair_ty, callee_repr
        ));

        let typed_fn = self.new_temp();
        let fn_sig_text = format!(
            "{} (i8*{})*",
            llvm_type(&layout.ret),
            if layout.params.is_empty() {
                String::new()
            } else {
                format!(
                    ", {}",
                    layout
                        .params
                        .iter()
                        .map(llvm_type)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        );
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to {}",
            typed_fn, fn_raw, fn_sig_text
        ));

        let mut call_args = vec![format!("i8* {}", env_raw)];
        call_args.extend(rendered);

        if *layout.ret == LType::Unit {
            fctx.lines.push(format!(
                "  call void {}({})",
                typed_fn,
                call_args.join(", ")
            ));
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        let out = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call {} {}({})",
            out,
            llvm_type(&layout.ret),
            typed_fn,
            call_args.join(", ")
        ));
        Some(Value {
            ty: (*layout.ret).clone(),
            repr: Some(out),
        })
    }

    pub(super) fn gen_closure_value(
        &mut self,
        params: &[ir::ClosureParam],
        ret_type: ir::TypeId,
        body: &ir::Block,
        expected_ty: Option<&LType>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let capture_names = self.collect_closure_capture_names(params, body);
        let mut captures = Vec::new();
        for name in capture_names {
            let Some(local) = find_local(&fctx.vars, &name) else {
                self.diagnostics.push(Diagnostic::error(
                    "E5031",
                    format!("closure capture '{}' is not available in this scope", name),
                    self.file,
                    span,
                ));
                return None;
            };
            captures.push((name, local));
        }

        let inferred_param_tys = match expected_ty {
            Some(LType::Fn(layout)) if layout.params.len() == params.len() => {
                Some(layout.params.clone())
            }
            _ => None,
        };

        let mut param_tys = Vec::new();
        for (idx, param) in params.iter().enumerate() {
            let ty = if let Some(ty_id) = param.ty {
                self.type_from_id(ty_id, param.span)?
            } else if let Some(inferred) = inferred_param_tys
                .as_ref()
                .and_then(|tys| tys.get(idx))
                .cloned()
            {
                inferred
            } else {
                self.diagnostics.push(Diagnostic::error(
                    "E5033",
                    format!(
                        "closure parameter '{}' requires an explicit type",
                        param.name
                    ),
                    self.file,
                    span,
                ));
                return None;
            };
            param_tys.push(ty);
        }
        let ret = self.type_from_id(ret_type, span)?;
        let layout = FnLayoutType {
            repr: render_applied_type("Fn", &{
                let mut all = param_tys.clone();
                all.push(ret.clone());
                all
            }),
            params: param_tys.clone(),
            ret: Box::new(ret.clone()),
        };

        let env_ptr = self.alloc_closure_env(&captures, fctx)?;

        let closure_name = format!("__aic_closure_{}", self.closure_counter);
        self.closure_counter += 1;
        self.emit_closure_helper(&closure_name, params, &param_tys, &ret, body, &captures);
        self.build_fn_value_from_symbol(&closure_name, &layout, &env_ptr, fctx)
    }

    pub(super) fn closure_env_layout(&self, captures: &[(String, Local)]) -> StructLayoutType {
        StructLayoutType {
            repr: "__ClosureEnv".to_string(),
            fields: captures
                .iter()
                .map(|(name, local)| StructFieldType {
                    name: name.clone(),
                    ty: local.ty.clone(),
                })
                .collect(),
        }
    }

    pub(super) fn alloc_closure_env(
        &mut self,
        captures: &[(String, Local)],
        fctx: &mut FnCtx,
    ) -> Option<String> {
        if captures.is_empty() {
            return Some("null".to_string());
        }

        self.extern_decls
            .insert("declare i8* @malloc(i64)".to_string());
        let env_layout = self.closure_env_layout(captures);
        let env_ty = LType::Struct(env_layout);
        let env_llvm = llvm_type(&env_ty);

        let env_tmp = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca {}", env_tmp, env_llvm));

        for (idx, (_name, local)) in captures.iter().enumerate() {
            let field_ptr = self.new_temp();
            fctx.lines.push(format!(
                "  {} = getelementptr inbounds {}, {}* {}, i32 0, i32 {}",
                field_ptr, env_llvm, env_llvm, env_tmp, idx
            ));
            let captured = self.new_temp();
            fctx.lines.push(format!(
                "  {} = load {}, {}* {}",
                captured,
                llvm_type(&local.ty),
                llvm_type(&local.ty),
                local.ptr
            ));
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                llvm_type(&local.ty),
                captured,
                llvm_type(&local.ty),
                field_ptr
            ));
        }

        let size_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds {}, {}* null, i32 1",
            size_ptr, env_llvm, env_llvm
        ));
        let size = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint {}* {} to i64",
            size, env_llvm, size_ptr
        ));

        let env_heap = self.new_temp();
        fctx.lines
            .push(format!("  {} = call i8* @malloc(i64 {})", env_heap, size));
        let env_heap_typed = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast i8* {} to {}*",
            env_heap_typed, env_heap, env_llvm
        ));
        let env_value = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            env_value, env_llvm, env_llvm, env_tmp
        ));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            env_llvm, env_value, env_llvm, env_heap_typed
        ));
        Some(env_heap)
    }

    pub(super) fn emit_closure_helper(
        &mut self,
        closure_name: &str,
        params: &[ir::ClosureParam],
        param_tys: &[LType],
        ret_ty: &LType,
        body: &ir::Block,
        captures: &[(String, Local)],
    ) {
        let mut param_defs = vec!["i8* %env".to_string()];
        param_defs.extend(
            param_tys
                .iter()
                .enumerate()
                .map(|(idx, ty)| format!("{} %arg{}", llvm_type(ty), idx)),
        );

        let mut fctx = FnCtx {
            lines: vec!["entry:".to_string()],
            vars: vec![BTreeMap::new()],
            drop_scopes: vec![DropScope::default()],
            terminated: false,
            current_label: "entry".to_string(),
            ret_ty: ret_ty.clone(),
            async_inner_ret: None,
            debug_scope: None,
            loop_stack: Vec::new(),
            current_fn_name: closure_name.to_string(),
            current_fn_llvm_name: closure_name.to_string(),
            current_fn_sig: FnSig {
                is_extern: false,
                extern_symbol: None,
                extern_abi: None,
                is_intrinsic: false,
                intrinsic_abi: None,
                params: param_tys.to_vec(),
                ret: ret_ty.clone(),
            },
            tail_return_mode: false,
            suppress_lifetime_end: false,
            async_poll_ctx: None,
        };

        if captures.is_empty() {
            fctx.lines
                .push("  %env_ignore = ptrtoint i8* %env to i64".to_string());
        } else {
            let env_layout = self.closure_env_layout(captures);
            let env_ty = LType::Struct(env_layout);
            let env_llvm = llvm_type(&env_ty);
            let env_ptr = self.new_temp();
            fctx.lines
                .push(format!("  {} = bitcast i8* %env to {}*", env_ptr, env_llvm));
            for (idx, (name, local)) in captures.iter().enumerate() {
                let field_ptr = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = getelementptr inbounds {}, {}* {}, i32 0, i32 {}",
                    field_ptr, env_llvm, env_llvm, env_ptr, idx
                ));
                let captured = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load {}, {}* {}",
                    captured,
                    llvm_type(&local.ty),
                    llvm_type(&local.ty),
                    field_ptr
                ));
                let slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca {}", slot, llvm_type(&local.ty)));
                fctx.lines.push(format!(
                    "  store {} {}, {}* {}",
                    llvm_type(&local.ty),
                    captured,
                    llvm_type(&local.ty),
                    slot
                ));
                fctx.vars.last_mut().expect("scope").insert(
                    name.clone(),
                    Local {
                        symbol: None,
                        ty: local.ty.clone(),
                        ptr: slot,
                    },
                );
            }
        }

        for (idx, param) in params.iter().enumerate() {
            let Some(ty) = param_tys.get(idx).cloned() else {
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
            fctx.vars.last_mut().expect("scope").insert(
                param.name.clone(),
                Local {
                    symbol: None,
                    ty,
                    ptr,
                },
            );
        }

        fctx.tail_return_mode = true;
        let tail = self.gen_block_with_expected_tail(body, Some(ret_ty), &mut fctx);
        fctx.tail_return_mode = false;
        if !fctx.terminated {
            match ret_ty {
                LType::Unit => fctx.lines.push("  ret void".to_string()),
                _ => {
                    let value = tail.unwrap_or(Value {
                        ty: ret_ty.clone(),
                        repr: Some(default_value(ret_ty)),
                    });
                    if value.ty != *ret_ty {
                        self.diagnostics.push(Diagnostic::error(
                            "E5035",
                            "closure body return type does not match declared type",
                            self.file,
                            body.span,
                        ));
                        fctx.lines.push(format!(
                            "  ret {} {}",
                            llvm_type(ret_ty),
                            default_value(ret_ty)
                        ));
                    } else {
                        fctx.lines.push(format!(
                            "  ret {} {}",
                            llvm_type(ret_ty),
                            value.repr.unwrap_or_else(|| default_value(ret_ty))
                        ));
                    }
                }
            }
        }

        let mut lines = Vec::new();
        lines.push(format!(
            "define {} @{}({}) {{",
            llvm_type(ret_ty),
            closure_name,
            param_defs.join(", ")
        ));
        lines.extend(fctx.lines);
        lines.push("}".to_string());
        self.deferred_fn_defs.push(lines);
    }

    pub(super) fn collect_closure_capture_names(
        &self,
        params: &[ir::ClosureParam],
        body: &ir::Block,
    ) -> Vec<String> {
        let mut known_functions = self.fn_sigs.keys().cloned().collect::<BTreeSet<_>>();
        known_functions.insert("Some".to_string());
        known_functions.insert("None".to_string());
        known_functions.insert("Ok".to_string());
        known_functions.insert("Err".to_string());

        let mut scopes = vec![params
            .iter()
            .map(|param| param.name.clone())
            .collect::<BTreeSet<_>>()];
        let mut captures = BTreeSet::new();
        collect_closure_captures_block(body, &mut scopes, &mut captures, &known_functions);
        captures.into_iter().collect()
    }

    pub(super) fn fn_sig(&self, name: &str) -> Option<&FnSig> {
        self.call_sig_overrides
            .iter()
            .rev()
            .find(|entry| entry.name == name)
            .map(|entry| &entry.sig)
            .or_else(|| self.fn_sigs.get(name))
    }

    pub(super) fn type_module_guard_for_symbol(&mut self, symbol: ir::SymbolId) -> TypeModuleGuard {
        if let Some(module) = self.function_modules_by_symbol.get(&symbol).cloned() {
            self.type_module_stack.push(module);
            TypeModuleGuard {
                stack: &mut self.type_module_stack as *mut _,
                pushed: true,
            }
        } else {
            TypeModuleGuard {
                stack: std::ptr::null_mut(),
                pushed: false,
            }
        }
    }

    pub(super) fn current_type_module(&self) -> Option<&str> {
        self.type_module_stack.last().map(String::as_str)
    }

    pub(super) fn visible_struct_template(&self, name: &str) -> Option<&StructTemplate> {
        if let Some((module, short)) = name.rsplit_once('.') {
            return self
                .struct_templates_by_module
                .get(&(module.to_string(), short.to_string()));
        }

        let short = name.rsplit('.').next().unwrap_or(name);
        if let Some(module) = self.current_type_module() {
            if let Some(template) = self
                .struct_templates_by_module
                .get(&(module.to_string(), short.to_string()))
            {
                return Some(template);
            }
            if let Some(resolution) = self.resolution {
                if let Some(imports) = resolution.module_imports.get(module) {
                    for import in imports {
                        if let Some(template) = self
                            .struct_templates_by_module
                            .get(&(import.clone(), short.to_string()))
                        {
                            return Some(template);
                        }
                    }
                }
            }
        }
        self.struct_templates.get(short)
    }

    pub(super) fn visible_enum_template(&self, name: &str) -> Option<&EnumTemplate> {
        if let Some((module, short)) = name.rsplit_once('.') {
            return self
                .enum_templates_by_module
                .get(&(module.to_string(), short.to_string()));
        }

        let short = name.rsplit('.').next().unwrap_or(name);
        if let Some(module) = self.current_type_module() {
            if let Some(template) = self
                .enum_templates_by_module
                .get(&(module.to_string(), short.to_string()))
            {
                return Some(template);
            }
            if let Some(resolution) = self.resolution {
                if let Some(imports) = resolution.module_imports.get(module) {
                    for import in imports {
                        if let Some(template) = self
                            .enum_templates_by_module
                            .get(&(import.clone(), short.to_string()))
                        {
                            return Some(template);
                        }
                    }
                }
            }
        }
        self.enum_templates.get(short)
    }

    pub(super) fn sig_matches_shape(&self, name: &str, params: &[&str], ret: &str) -> bool {
        let Some(sig) = self.fn_sig(name) else {
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

    pub(super) fn resolve_call_sig_for_types(
        &mut self,
        name: &str,
        arg_types: &[LType],
        span: crate::span::Span,
    ) -> Option<FnSig> {
        if let Some(sig) = self.fn_sig(name).cloned() {
            if sig.params == arg_types {
                return Some(sig);
            }
        }

        if let Some(instances) = self.generic_fn_instances.get(name) {
            let matches = instances
                .iter()
                .filter(|instance| instance.params == arg_types)
                .collect::<Vec<_>>();
            if matches.len() == 1 {
                let instance = matches[0];
                return Some(FnSig {
                    is_extern: false,
                    extern_symbol: None,
                    extern_abi: None,
                    is_intrinsic: false,
                    intrinsic_abi: None,
                    params: instance.params.clone(),
                    ret: instance.ret.clone(),
                });
            }
            if matches.len() > 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E5014",
                    format!("ambiguous generic call to '{}'", name),
                    self.file,
                    span,
                ));
                return None;
            }
        }
        None
    }
}

fn type_uses_generic(ty: &str, generic: &str) -> bool {
    if ty == generic {
        return true;
    }
    extract_generic_args(ty)
        .map(|args| args.iter().any(|arg| type_uses_generic(arg, generic)))
        .unwrap_or(false)
}

fn parse_raw_int_literal_magnitude(text: &str) -> Option<u128> {
    let trimmed = text.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u128::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse::<u128>().ok()
    }
}
