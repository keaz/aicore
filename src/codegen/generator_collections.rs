use super::*;

impl<'a> Generator<'a> {
    fn fn_sig_ret_by_name_or_suffix(&self, name: &str) -> Option<LType> {
        if let Some(sig) = self.fn_sig(name) {
            return Some(sig.ret.clone());
        }
        let dotted_suffix = format!(".{name}");
        self.fn_sigs
            .iter()
            .find(|(candidate, _)| candidate.ends_with(&dotted_suffix))
            .map(|(_, sig)| sig.ret.clone())
    }

    pub(super) fn gen_map_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "new_map" | "aic_map_new_intrinsic" => "new_map",
            "close_map" | "aic_map_close_intrinsic" => "close_map",
            "insert" | "aic_map_insert_intrinsic" => "insert",
            "get" | "aic_map_get_intrinsic" => "get",
            "contains_key" | "aic_map_contains_key_intrinsic" => "contains_key",
            "remove" | "aic_map_remove_intrinsic" => "remove",
            "size" | "aic_map_size_intrinsic" => "size",
            "keys" | "aic_map_keys_intrinsic" => "keys",
            "values" | "aic_map_values_intrinsic" => "values",
            "entries" | "aic_map_entries_intrinsic" => "entries",
            _ => return None,
        };

        match canonical {
            "new_map" => Some(self.gen_map_new_call(name, args, span, expected_ty, fctx)),
            "close_map" => Some(self.gen_map_close_call(name, args, span, expected_ty, fctx)),
            "insert" => Some(self.gen_map_insert_call(name, args, span, expected_ty, fctx)),
            "get" => Some(self.gen_map_get_call(name, args, span, expected_ty, fctx)),
            "contains_key" => {
                Some(self.gen_map_contains_key_call(name, args, span, expected_ty, fctx))
            }
            "remove" => Some(self.gen_map_remove_call(name, args, span, expected_ty, fctx)),
            "size" => Some(self.gen_map_size_call(name, args, span, expected_ty, fctx)),
            "keys" => Some(self.gen_map_keys_call(name, args, span, expected_ty, fctx)),
            "values" => Some(self.gen_map_values_call(name, args, span, expected_ty, fctx)),
            "entries" => Some(self.gen_map_entries_call(name, args, span, expected_ty, fctx)),
            _ => None,
        }
    }

    pub(super) fn map_result_ty(
        &mut self,
        name: &str,
        span: crate::span::Span,
        expected_ty: Option<&LType>,
    ) -> Option<LType> {
        if let Some(result_ty) = self.fn_sig_ret_by_name_or_suffix(name) {
            return Some(result_ty);
        }
        if let Some(expected) = expected_ty {
            return Some(expected.clone());
        }
        self.diagnostics.push(Diagnostic::error(
            "E5012",
            format!("unknown function '{name}' in codegen"),
            self.file,
            span,
        ));
        None
    }

    pub(super) fn map_key_value_types(
        &mut self,
        map_ty: &LType,
        context: &str,
        span: crate::span::Span,
    ) -> Option<(String, String)> {
        let repr = render_type(map_ty);
        if base_type_name(&repr) != "Map" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Map[K, V], found '{}'", repr),
                self.file,
                span,
            ));
            return None;
        }
        let Some(args) = extract_generic_args(&repr) else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects applied Map type, found '{}'", repr),
                self.file,
                span,
            ));
            return None;
        };
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects two Map type arguments, found '{}'", repr),
                self.file,
                span,
            ));
            return None;
        }
        Some((args[0].clone(), args[1].clone()))
    }

    pub(super) fn map_key_kind(
        &mut self,
        key_ty: &str,
        context: &str,
        span: crate::span::Span,
    ) -> Option<i64> {
        match key_ty {
            "String" => Some(1),
            "Bytes" => Some(1),
            "Int" => Some(2),
            "UInt64" => Some(2),
            "Bool" => Some(3),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    format!(
                        "{context} supports only map keys String, Bytes, Int, UInt64, and Bool"
                    ),
                    self.file,
                    span,
                ));
                None
            }
        }
    }

    pub(super) fn map_value_kind(
        &mut self,
        value_ty: &str,
        context: &str,
        span: crate::span::Span,
    ) -> Option<i64> {
        match value_ty {
            "String" => Some(1),
            "Bytes" => Some(1),
            "Int" => Some(2),
            "Bool" => Some(3),
            "UInt64" => Some(4),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    format!(
                        "{context} supports only map values String, Bytes, Int, Bool, and UInt64"
                    ),
                    self.file,
                    span,
                ));
                None
            }
        }
    }

    fn is_bytes_type_name(ty: &str) -> bool {
        base_type_name(ty) == "Bytes"
    }

    fn map_key_string_parts(
        &mut self,
        value: &Value,
        key_ty: &str,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String)> {
        if Self::is_bytes_type_name(key_ty) {
            self.bytes_parts(value, context, span, fctx)
        } else {
            self.string_parts(value, span, fctx)
        }
    }

    fn map_value_string_parts(
        &mut self,
        value: &Value,
        value_ty: &str,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String)> {
        if Self::is_bytes_type_name(value_ty) {
            self.bytes_parts(value, context, span, fctx)
        } else {
            self.string_parts(value, span, fctx)
        }
    }

    fn ensure_map_bool_runtime_decl(&mut self, decl: &str) {
        self.extern_decls.insert(decl.to_string());
    }

    pub(super) fn build_map_value_from_handle(
        &mut self,
        map_ty: &LType,
        handle: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Struct(layout) = map_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "map builtin expects Map return type, found '{}'",
                    render_type(map_ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Map"
            || layout.fields.len() != 1
            || layout.fields[0].name != "handle"
            || layout.fields[0].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "map builtin expects Map[_, _] layout, found '{}'",
                    layout.repr
                ),
                self.file,
                span,
            ));
            return None;
        }
        self.build_struct_value(
            layout,
            &[Value {
                ty: LType::Int,
                repr: Some(handle.to_string()),
            }],
            span,
            fctx,
        )
    }

    pub(super) fn build_vec_value_from_raw_i8_ptr(
        &mut self,
        expected_ty: &LType,
        items_ptr: &str,
        count: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Struct(layout) = expected_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "map builtin expects Vec return type, found '{}'",
                    render_type(expected_ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Vec"
            || layout.fields.len() != 3
            || layout.fields[0].ty != LType::Int
            || layout.fields[1].ty != LType::Int
            || layout.fields[2].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("map builtin expects Vec layout, found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }
        let ptr_as_int = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint i8* {} to i64",
            ptr_as_int, items_ptr
        ));
        self.build_struct_value(
            layout,
            &[
                Value {
                    ty: LType::Int,
                    repr: Some(ptr_as_int),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(count.to_string()),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(count.to_string()),
                },
            ],
            span,
            fctx,
        )
    }

    pub(super) fn gen_map_new_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "new_map expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let result_ty = self.map_result_ty(name, span, expected_ty)?;
        let (key_ty, value_ty) = self.map_key_value_types(&result_ty, "new_map", span)?;
        let key_kind = self.map_key_kind(&key_ty, "new_map", span)?;
        let value_kind = self.map_value_kind(&value_ty, "new_map", span)?;
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", handle_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_map_new(i64 {}, i64 {}, i64* {})",
            _err, key_kind, value_kind, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        self.build_map_value_from_handle(&result_ty, &handle, span, fctx)
    }

    pub(super) fn gen_map_close_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "close_map expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let map_value = self.gen_expr(&args[0], fctx)?;
        let LType::Struct(layout) = &map_value.ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "close_map expects Map[K, V]",
                self.file,
                args[0].span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Map" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "close_map expects Map[K, V], found '{}'",
                    render_type(&map_value.ty)
                ),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let Some(handle_idx) = self.struct_int_field_index(layout, "handle") else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "close_map expects map layout with Int handle field",
                self.file,
                args[0].span,
            ));
            return None;
        };
        let map_repr = map_value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&map_value.ty));
        let map_handle = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            map_handle,
            llvm_type(&map_value.ty),
            map_repr,
            handle_idx
        ));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_map_close(i64 {})",
            _err, map_handle
        ));
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    pub(super) fn gen_map_insert_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "insert expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let map_value = self.gen_expr(&args[0], fctx)?;
        let key = self.gen_expr(&args[1], fctx)?;
        let value = self.gen_expr(&args[2], fctx)?;
        let (key_ty, value_ty) = self.map_key_value_types(&map_value.ty, "insert", args[0].span)?;
        let actual_key_ty = render_type(&key.ty);
        if actual_key_ty.replace(' ', "") != key_ty.replace(' ', "") {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "insert key type mismatch: expected '{}', found '{}'",
                    key_ty, actual_key_ty
                ),
                self.file,
                args[1].span,
            ));
            return None;
        }
        let key_kind = self.map_key_kind(&key_ty, "insert", args[0].span)?;
        if render_type(&value.ty) != value_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "insert value type mismatch: expected '{}', found '{}'",
                    value_ty,
                    render_type(&value.ty)
                ),
                self.file,
                args[2].span,
            ));
            return None;
        }
        let handle =
            self.extract_named_handle_from_value(&map_value, "Map", "insert", args[0].span, fctx)?;
        let value_kind = self.map_value_kind(&value_ty, "insert", span)?;
        match (key_kind, value_kind) {
            (1, 1) => {
                let (kptr, klen, kcap) =
                    self.map_key_string_parts(&key, &key_ty, "insert", args[1].span, fctx)?;
                let (vptr, vlen, vcap) =
                    self.map_value_string_parts(&value, &value_ty, "insert", args[2].span, fctx)?;
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_insert_string(i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
                    _err, handle, kptr, klen, kcap, vptr, vlen, vcap
                ));
            }
            (2, 1) => {
                let (vptr, vlen, vcap) =
                    self.map_value_string_parts(&value, &value_ty, "insert", args[2].span, fctx)?;
                let key_i64 = key.repr.clone().unwrap_or_else(|| "0".to_string());
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_insert_string_int_key(i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
                    _err, handle, key_i64, vptr, vlen, vcap
                ));
            }
            (3, 1) => {
                let (vptr, vlen, vcap) =
                    self.map_value_string_parts(&value, &value_ty, "insert", args[2].span, fctx)?;
                let key_bool = key.repr.clone().unwrap_or_else(|| "0".to_string());
                let key_i64 = self.new_temp();
                fctx.lines
                    .push(format!("  {} = zext i1 {} to i64", key_i64, key_bool));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_insert_string_bool_key(i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
                    _err, handle, key_i64, vptr, vlen, vcap
                ));
            }
            (1, 2) => {
                let (kptr, klen, kcap) =
                    self.map_key_string_parts(&key, &key_ty, "insert", args[1].span, fctx)?;
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_insert_int(i64 {}, i8* {}, i64 {}, i64 {}, i64 {})",
                    _err,
                    handle,
                    kptr,
                    klen,
                    kcap,
                    value.repr.clone().unwrap_or_else(|| "0".to_string())
                ));
            }
            (2, 2) => {
                let key_i64 = key.repr.clone().unwrap_or_else(|| "0".to_string());
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_insert_int_int_key(i64 {}, i64 {}, i64 {})",
                    _err,
                    handle,
                    key_i64,
                    value.repr.clone().unwrap_or_else(|| "0".to_string())
                ));
            }
            (3, 2) => {
                let key_bool = key.repr.clone().unwrap_or_else(|| "0".to_string());
                let key_i64 = self.new_temp();
                fctx.lines
                    .push(format!("  {} = zext i1 {} to i64", key_i64, key_bool));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_insert_int_bool_key(i64 {}, i64 {}, i64 {})",
                    _err,
                    handle,
                    key_i64,
                    value.repr.clone().unwrap_or_else(|| "0".to_string())
                ));
            }
            (1, 3) => {
                let (kptr, klen, kcap) =
                    self.map_key_string_parts(&key, &key_ty, "insert", args[1].span, fctx)?;
                let value_bool = value.repr.clone().unwrap_or_else(|| "0".to_string());
                let value_i64 = self.new_temp();
                fctx.lines
                    .push(format!("  {} = zext i1 {} to i64", value_i64, value_bool));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_insert_int(i64 {}, i8* {}, i64 {}, i64 {}, i64 {})",
                    _err, handle, kptr, klen, kcap, value_i64
                ));
            }
            (2, 3) => {
                let key_i64 = key.repr.clone().unwrap_or_else(|| "0".to_string());
                let value_bool = value.repr.clone().unwrap_or_else(|| "0".to_string());
                let value_i64 = self.new_temp();
                fctx.lines
                    .push(format!("  {} = zext i1 {} to i64", value_i64, value_bool));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_insert_int_int_key(i64 {}, i64 {}, i64 {})",
                    _err, handle, key_i64, value_i64
                ));
            }
            (3, 3) => {
                let key_bool = key.repr.clone().unwrap_or_else(|| "0".to_string());
                let key_i64 = self.new_temp();
                fctx.lines
                    .push(format!("  {} = zext i1 {} to i64", key_i64, key_bool));
                let value_bool = value.repr.clone().unwrap_or_else(|| "0".to_string());
                let value_i64 = self.new_temp();
                fctx.lines
                    .push(format!("  {} = zext i1 {} to i64", value_i64, value_bool));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_insert_int_bool_key(i64 {}, i64 {}, i64 {})",
                    _err, handle, key_i64, value_i64
                ));
            }
            (1, 4) => {
                let (kptr, klen, kcap) =
                    self.map_key_string_parts(&key, &key_ty, "insert", args[1].span, fctx)?;
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_insert_int(i64 {}, i8* {}, i64 {}, i64 {}, i64 {})",
                    _err,
                    handle,
                    kptr,
                    klen,
                    kcap,
                    value.repr.clone().unwrap_or_else(|| "0".to_string())
                ));
            }
            _ => unreachable!(),
        }
        Some(map_value)
    }

    pub(super) fn gen_map_get_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "get expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let map_value = self.gen_expr(&args[0], fctx)?;
        let key = self.gen_expr(&args[1], fctx)?;
        let (key_ty, value_ty) = self.map_key_value_types(&map_value.ty, "get", args[0].span)?;
        let actual_key_ty = render_type(&key.ty);
        if actual_key_ty.replace(' ', "") != key_ty.replace(' ', "") {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "get key type mismatch: expected '{}', found '{}'",
                    key_ty, actual_key_ty
                ),
                self.file,
                args[1].span,
            ));
            return None;
        }
        let key_kind = self.map_key_kind(&key_ty, "get", args[0].span)?;
        let result_ty = self.parse_type_repr(&format!("Option[{}]", value_ty), span)?;
        let handle =
            self.extract_named_handle_from_value(&map_value, "Map", "get", args[0].span, fctx)?;
        let value_kind = self.map_value_kind(&value_ty, "get", span)?;
        match value_kind {
            1 => {
                let out_ptr_slot = self.new_temp();
                fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
                let out_len_slot = self.new_temp();
                fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
                let found = self.new_temp();
                match key_kind {
                    1 => {
                        let (kptr, klen, kcap) =
                            self.map_key_string_parts(&key, &key_ty, "get", args[1].span, fctx)?;
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_string(i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
                            found, handle, kptr, klen, kcap, out_ptr_slot, out_len_slot
                        ));
                    }
                    2 => {
                        let key_i64 = key.repr.clone().unwrap_or_else(|| "0".to_string());
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_string_int_key(i64 {}, i64 {}, i8** {}, i64* {})",
                            found, handle, key_i64, out_ptr_slot, out_len_slot
                        ));
                    }
                    3 => {
                        let key_bool = key.repr.clone().unwrap_or_else(|| "0".to_string());
                        let key_i64 = self.new_temp();
                        fctx.lines
                            .push(format!("  {} = zext i1 {} to i64", key_i64, key_bool));
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_string_bool_key(i64 {}, i64 {}, i8** {}, i64* {})",
                            found, handle, key_i64, out_ptr_slot, out_len_slot
                        ));
                    }
                    _ => unreachable!(),
                }
                let found_bool = self.new_temp();
                fctx.lines
                    .push(format!("  {} = icmp ne i64 {}, 0", found_bool, found));
                let payload =
                    self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
                if Self::is_bytes_type_name(&value_ty) {
                    let bytes_ty = self.parse_type_repr("Bytes", span)?;
                    let bytes_payload =
                        self.build_bytes_value_from_data(&bytes_ty, payload, "get", span, fctx)?;
                    self.wrap_option_with_condition(
                        &result_ty,
                        bytes_payload,
                        &found_bool,
                        span,
                        fctx,
                    )
                } else {
                    self.wrap_option_with_condition(&result_ty, payload, &found_bool, span, fctx)
                }
            }
            2 => {
                let out_value_slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca i64", out_value_slot));
                let found = self.new_temp();
                match key_kind {
                    1 => {
                        let (kptr, klen, kcap) =
                            self.map_key_string_parts(&key, &key_ty, "get", args[1].span, fctx)?;
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_int(i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
                            found, handle, kptr, klen, kcap, out_value_slot
                        ));
                    }
                    2 => {
                        let key_i64 = key.repr.clone().unwrap_or_else(|| "0".to_string());
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_int_int_key(i64 {}, i64 {}, i64* {})",
                            found, handle, key_i64, out_value_slot
                        ));
                    }
                    3 => {
                        let key_bool = key.repr.clone().unwrap_or_else(|| "0".to_string());
                        let key_i64 = self.new_temp();
                        fctx.lines
                            .push(format!("  {} = zext i1 {} to i64", key_i64, key_bool));
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_int_bool_key(i64 {}, i64 {}, i64* {})",
                            found, handle, key_i64, out_value_slot
                        ));
                    }
                    _ => unreachable!(),
                }
                let found_bool = self.new_temp();
                fctx.lines
                    .push(format!("  {} = icmp ne i64 {}, 0", found_bool, found));
                let out_value = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load i64, i64* {}",
                    out_value, out_value_slot
                ));
                let payload = Value {
                    ty: LType::Int,
                    repr: Some(out_value),
                };
                self.wrap_option_with_condition(&result_ty, payload, &found_bool, span, fctx)
            }
            3 => {
                let out_value_slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca i64", out_value_slot));
                let found = self.new_temp();
                match key_kind {
                    1 => {
                        let (kptr, klen, kcap) =
                            self.map_key_string_parts(&key, &key_ty, "get", args[1].span, fctx)?;
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_int(i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
                            found, handle, kptr, klen, kcap, out_value_slot
                        ));
                    }
                    2 => {
                        let key_i64 = key.repr.clone().unwrap_or_else(|| "0".to_string());
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_int_int_key(i64 {}, i64 {}, i64* {})",
                            found, handle, key_i64, out_value_slot
                        ));
                    }
                    3 => {
                        let key_bool = key.repr.clone().unwrap_or_else(|| "0".to_string());
                        let key_i64 = self.new_temp();
                        fctx.lines
                            .push(format!("  {} = zext i1 {} to i64", key_i64, key_bool));
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_int_bool_key(i64 {}, i64 {}, i64* {})",
                            found, handle, key_i64, out_value_slot
                        ));
                    }
                    _ => unreachable!(),
                }
                let found_bool = self.new_temp();
                fctx.lines
                    .push(format!("  {} = icmp ne i64 {}, 0", found_bool, found));
                let out_value = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load i64, i64* {}",
                    out_value, out_value_slot
                ));
                let bool_value = self.new_temp();
                fctx.lines
                    .push(format!("  {} = icmp ne i64 {}, 0", bool_value, out_value));
                let payload = Value {
                    ty: LType::Bool,
                    repr: Some(bool_value),
                };
                self.wrap_option_with_condition(&result_ty, payload, &found_bool, span, fctx)
            }
            4 => {
                let out_value_slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca i64", out_value_slot));
                let found = self.new_temp();
                match key_kind {
                    1 => {
                        let (kptr, klen, kcap) =
                            self.map_key_string_parts(&key, &key_ty, "get", args[1].span, fctx)?;
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_int(i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
                            found, handle, kptr, klen, kcap, out_value_slot
                        ));
                    }
                    2 => {
                        let key_i64 = key.repr.clone().unwrap_or_else(|| "0".to_string());
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_int_int_key(i64 {}, i64 {}, i64* {})",
                            found, handle, key_i64, out_value_slot
                        ));
                    }
                    3 => {
                        let key_bool = key.repr.clone().unwrap_or_else(|| "0".to_string());
                        let key_i64 = self.new_temp();
                        fctx.lines
                            .push(format!("  {} = zext i1 {} to i64", key_i64, key_bool));
                        fctx.lines.push(format!(
                            "  {} = call i64 @aic_rt_map_get_int_bool_key(i64 {}, i64 {}, i64* {})",
                            found, handle, key_i64, out_value_slot
                        ));
                    }
                    _ => unreachable!(),
                }
                let found_bool = self.new_temp();
                fctx.lines
                    .push(format!("  {} = icmp ne i64 {}, 0", found_bool, found));
                let out_value = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load i64, i64* {}",
                    out_value, out_value_slot
                ));
                let payload = Value {
                    ty: LType::UInt64,
                    repr: Some(out_value),
                };
                self.wrap_option_with_condition(&result_ty, payload, &found_bool, span, fctx)
            }
            _ => unreachable!(),
        }
    }

    pub(super) fn gen_map_contains_key_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "contains_key expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let map_value = self.gen_expr(&args[0], fctx)?;
        let key = self.gen_expr(&args[1], fctx)?;
        let (key_ty, _value_ty) =
            self.map_key_value_types(&map_value.ty, "contains_key", args[0].span)?;
        let actual_key_ty = render_type(&key.ty);
        if actual_key_ty.replace(' ', "") != key_ty.replace(' ', "") {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "contains_key key type mismatch: expected '{}', found '{}'",
                    key_ty, actual_key_ty
                ),
                self.file,
                args[1].span,
            ));
            return None;
        }
        let key_kind = self.map_key_kind(&key_ty, "contains_key", args[0].span)?;
        let handle = self.extract_named_handle_from_value(
            &map_value,
            "Map",
            "contains_key",
            args[0].span,
            fctx,
        )?;
        let found = self.new_temp();
        match key_kind {
            1 => {
                let (kptr, klen, kcap) =
                    self.map_key_string_parts(&key, &key_ty, "contains_key", args[1].span, fctx)?;
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_contains(i64 {}, i8* {}, i64 {}, i64 {})",
                    found, handle, kptr, klen, kcap
                ));
            }
            2 => {
                let key_i64 = key.repr.clone().unwrap_or_else(|| "0".to_string());
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_contains_int(i64 {}, i64 {})",
                    found, handle, key_i64
                ));
            }
            3 => {
                let key_bool = key.repr.clone().unwrap_or_else(|| "0".to_string());
                let key_i64 = self.new_temp();
                fctx.lines
                    .push(format!("  {} = zext i1 {} to i64", key_i64, key_bool));
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_contains_bool(i64 {}, i64 {})",
                    found, handle, key_i64
                ));
            }
            _ => unreachable!(),
        }
        let found_bool = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", found_bool, found));
        Some(Value {
            ty: LType::Bool,
            repr: Some(found_bool),
        })
    }

    pub(super) fn gen_map_remove_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "remove expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let map_value = self.gen_expr(&args[0], fctx)?;
        let key = self.gen_expr(&args[1], fctx)?;
        let (key_ty, _value_ty) =
            self.map_key_value_types(&map_value.ty, "remove", args[0].span)?;
        let actual_key_ty = render_type(&key.ty);
        if actual_key_ty.replace(' ', "") != key_ty.replace(' ', "") {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "remove key type mismatch: expected '{}', found '{}'",
                    key_ty, actual_key_ty
                ),
                self.file,
                args[1].span,
            ));
            return None;
        }
        let key_kind = self.map_key_kind(&key_ty, "remove", args[0].span)?;
        let handle =
            self.extract_named_handle_from_value(&map_value, "Map", "remove", args[0].span, fctx)?;
        let _err = self.new_temp();
        match key_kind {
            1 => {
                let (kptr, klen, kcap) =
                    self.map_key_string_parts(&key, &key_ty, "remove", args[1].span, fctx)?;
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_remove(i64 {}, i8* {}, i64 {}, i64 {})",
                    _err, handle, kptr, klen, kcap
                ));
            }
            2 => {
                let key_i64 = key.repr.clone().unwrap_or_else(|| "0".to_string());
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_remove_int(i64 {}, i64 {})",
                    _err, handle, key_i64
                ));
            }
            3 => {
                let key_bool = key.repr.clone().unwrap_or_else(|| "0".to_string());
                let key_i64 = self.new_temp();
                fctx.lines
                    .push(format!("  {} = zext i1 {} to i64", key_i64, key_bool));
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_remove_bool(i64 {}, i64 {})",
                    _err, handle, key_i64
                ));
            }
            _ => unreachable!(),
        }
        Some(map_value)
    }

    pub(super) fn gen_map_size_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "size expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let map_value = self.gen_expr(&args[0], fctx)?;
        let (key_ty, _value_ty) = self.map_key_value_types(&map_value.ty, "size", args[0].span)?;
        let _key_kind = self.map_key_kind(&key_ty, "size", args[0].span)?;
        let handle =
            self.extract_named_handle_from_value(&map_value, "Map", "size", args[0].span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        fctx.lines.push(format!("  store i64 0, i64* {}", out_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_map_size(i64 {}, i64* {})",
            _err, handle, out_slot
        ));
        let out_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_value, out_slot));
        Some(Value {
            ty: LType::Int,
            repr: Some(out_value),
        })
    }

    pub(super) fn gen_map_keys_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "keys expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let map_value = self.gen_expr(&args[0], fctx)?;
        let (key_ty, _value_ty) = self.map_key_value_types(&map_value.ty, "keys", args[0].span)?;
        let key_kind = self.map_key_kind(&key_ty, "keys", args[0].span)?;
        let result_ty = self.parse_type_repr(&format!("Vec[{}]", key_ty), span)?;
        let handle =
            self.extract_named_handle_from_value(&map_value, "Map", "keys", args[0].span, fctx)?;
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        let out_items = match key_kind {
            1 => {
                let out_items_slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca i8*", out_items_slot));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_keys(i64 {}, i8** {}, i64* {})",
                    _err, handle, out_items_slot, out_count_slot
                ));
                let out_items = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load i8*, i8** {}",
                    out_items, out_items_slot
                ));
                out_items
            }
            2 => {
                let out_items_slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca i64*", out_items_slot));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_keys_int(i64 {}, i64** {}, i64* {})",
                    _err, handle, out_items_slot, out_count_slot
                ));
                let out_items_i64 = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load i64*, i64** {}",
                    out_items_i64, out_items_slot
                ));
                let out_items = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = bitcast i64* {} to i8*",
                    out_items, out_items_i64
                ));
                out_items
            }
            3 => {
                let out_items_slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca i8*", out_items_slot));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_keys_bool(i64 {}, i8** {}, i64* {})",
                    _err, handle, out_items_slot, out_count_slot
                ));
                let out_items = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load i8*, i8** {}",
                    out_items, out_items_slot
                ));
                out_items
            }
            _ => unreachable!(),
        };
        let out_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_count, out_count_slot
        ));
        self.build_vec_value_from_raw_i8_ptr(&result_ty, &out_items, &out_count, span, fctx)
    }

    pub(super) fn gen_map_values_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "values expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let map_value = self.gen_expr(&args[0], fctx)?;
        let (key_ty, value_ty) = self.map_key_value_types(&map_value.ty, "values", args[0].span)?;
        let _key_kind = self.map_key_kind(&key_ty, "values", args[0].span)?;
        let result_ty = self.parse_type_repr(&format!("Vec[{}]", value_ty), span)?;
        let handle =
            self.extract_named_handle_from_value(&map_value, "Map", "values", args[0].span, fctx)?;
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        match self.map_value_kind(&value_ty, "values", span)? {
            1 => {
                let out_items_slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca i8*", out_items_slot));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_values_string(i64 {}, i8** {}, i64* {})",
                    _err, handle, out_items_slot, out_count_slot
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
                self.build_vec_value_from_raw_i8_ptr(&result_ty, &out_items, &out_count, span, fctx)
            }
            2 => {
                let out_items_slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca i64*", out_items_slot));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_values_int(i64 {}, i64** {}, i64* {})",
                    _err, handle, out_items_slot, out_count_slot
                ));
                let out_items_i64 = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load i64*, i64** {}",
                    out_items_i64, out_items_slot
                ));
                let out_items = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = bitcast i64* {} to i8*",
                    out_items, out_items_i64
                ));
                let out_count = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load i64, i64* {}",
                    out_count, out_count_slot
                ));
                self.build_vec_value_from_raw_i8_ptr(&result_ty, &out_items, &out_count, span, fctx)
            }
            4 => {
                let out_items_slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca i64*", out_items_slot));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_values_int(i64 {}, i64** {}, i64* {})",
                    _err, handle, out_items_slot, out_count_slot
                ));
                let out_items_i64 = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load i64*, i64** {}",
                    out_items_i64, out_items_slot
                ));
                let out_items = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = bitcast i64* {} to i8*",
                    out_items, out_items_i64
                ));
                let out_count = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load i64, i64* {}",
                    out_count, out_count_slot
                ));
                self.build_vec_value_from_raw_i8_ptr(&result_ty, &out_items, &out_count, span, fctx)
            }
            3 => {
                self.ensure_map_bool_runtime_decl(
                    "declare i64 @aic_rt_map_values_bool(i64, i8**, i64*)",
                );
                let out_items_slot = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca i8*", out_items_slot));
                let _err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = call i64 @aic_rt_map_values_bool(i64 {}, i8** {}, i64* {})",
                    _err, handle, out_items_slot, out_count_slot
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
                self.build_vec_value_from_raw_i8_ptr(&result_ty, &out_items, &out_count, span, fctx)
            }
            _ => unreachable!(),
        }
    }

    pub(super) fn gen_map_entries_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        _expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "entries expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let map_value = self.gen_expr(&args[0], fctx)?;
        let (key_ty, value_ty) =
            self.map_key_value_types(&map_value.ty, "entries", args[0].span)?;
        let key_kind = self.map_key_kind(&key_ty, "entries", args[0].span)?;
        let result_ty =
            self.parse_type_repr(&format!("Vec[MapEntry[{}, {}]]", key_ty, value_ty), span)?;
        let handle =
            self.extract_named_handle_from_value(&map_value, "Map", "entries", args[0].span, fctx)?;
        let out_items_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_items_slot));
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        let _err = self.new_temp();
        let runtime_fn = match (key_kind, self.map_value_kind(&value_ty, "entries", span)?) {
            (1, 1) => "aic_rt_map_entries_string",
            (2, 1) => "aic_rt_map_entries_string_int_key",
            (3, 1) => "aic_rt_map_entries_string_bool_key",
            (1, 2) => "aic_rt_map_entries_int",
            (2, 2) => "aic_rt_map_entries_int_int_key",
            (3, 2) => "aic_rt_map_entries_int_bool_key",
            (1, 3) => "aic_rt_map_entries_bool",
            (2, 3) => "aic_rt_map_entries_bool_int_key",
            (3, 3) => "aic_rt_map_entries_bool_bool_key",
            (1, 4) => "aic_rt_map_entries_int",
            (2, 4) => "aic_rt_map_entries_int_int_key",
            (3, 4) => "aic_rt_map_entries_int_bool_key",
            _ => unreachable!(),
        };
        match runtime_fn {
            "aic_rt_map_entries_bool"
            | "aic_rt_map_entries_bool_int_key"
            | "aic_rt_map_entries_bool_bool_key" => {
                self.ensure_map_bool_runtime_decl(&format!(
                    "declare i64 @{}(i64, i8**, i64*)",
                    runtime_fn
                ));
            }
            _ => {}
        }
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i8** {}, i64* {})",
            _err, runtime_fn, handle, out_items_slot, out_count_slot
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
        self.build_vec_value_from_raw_i8_ptr(&result_ty, &out_items, &out_count, span, fctx)
    }

    pub(super) fn gen_vec_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "aic_vec_new_intrinsic" => "new_vec",
            "aic_vec_new_with_capacity_intrinsic" => "new_vec_with_capacity",
            "aic_vec_of_intrinsic" => "vec_of",
            "aic_vec_get_intrinsic" => "get",
            "aic_vec_first_intrinsic" => "first",
            "aic_vec_last_intrinsic" => "last",
            "aic_vec_push_intrinsic" => "push",
            "aic_vec_pop_intrinsic" => "pop",
            "aic_vec_set_intrinsic" => "set",
            "aic_vec_insert_intrinsic" => "insert",
            "aic_vec_remove_at_intrinsic" => "remove_at",
            "aic_vec_contains_intrinsic" => "contains",
            "aic_vec_index_of_intrinsic" => "index_of",
            "aic_vec_reverse_intrinsic" => "reverse",
            "aic_vec_slice_intrinsic" => "slice",
            "aic_vec_append_intrinsic" => "append",
            "aic_vec_clear_intrinsic" => "clear",
            "aic_vec_reserve_intrinsic" => "reserve",
            "aic_vec_shrink_to_fit_intrinsic" => "shrink_to_fit",
            _ => return None,
        };
        match canonical {
            "new_vec" => Some(self.gen_vec_new_call(name, args, span, expected_ty, fctx)),
            "new_vec_with_capacity" => {
                Some(self.gen_vec_new_with_capacity_call(name, args, span, expected_ty, fctx))
            }
            "vec_of" => Some(self.gen_vec_of_call(name, args, span, expected_ty, fctx)),
            "get" => Some(self.gen_vec_get_call(args, span, fctx)),
            "first" => Some(self.gen_vec_first_call(args, span, fctx)),
            "last" => Some(self.gen_vec_last_call(args, span, fctx)),
            "push" => Some(self.gen_vec_push_call(args, span, fctx)),
            "pop" => Some(self.gen_vec_pop_call(args, span, fctx)),
            "set" => Some(self.gen_vec_set_call(args, span, fctx)),
            "insert" => Some(self.gen_vec_insert_call(args, span, fctx)),
            "remove_at" => Some(self.gen_vec_remove_at_call(args, span, fctx)),
            "contains" => Some(self.gen_vec_contains_call(args, span, fctx)),
            "index_of" => Some(self.gen_vec_index_of_call(args, span, fctx)),
            "reverse" => Some(self.gen_vec_reverse_call(args, span, fctx)),
            "slice" => Some(self.gen_vec_slice_call(args, span, fctx)),
            "append" => Some(self.gen_vec_append_call(args, span, fctx)),
            "clear" => Some(self.gen_vec_clear_call(args, span, fctx)),
            "reserve" => Some(self.gen_vec_reserve_call(args, span, fctx)),
            "shrink_to_fit" => Some(self.gen_vec_shrink_to_fit_call(args, span, fctx)),
            _ => None,
        }
    }

    pub(super) fn vec_result_ty(
        &mut self,
        name: &str,
        span: crate::span::Span,
        expected_ty: Option<&LType>,
    ) -> Option<LType> {
        let canonical_name = match name {
            "aic_vec_new_intrinsic" => "new_vec",
            "aic_vec_new_with_capacity_intrinsic" => "new_vec_with_capacity",
            "aic_vec_of_intrinsic" => "vec_of",
            "aic_vec_get_intrinsic" => "get",
            "aic_vec_first_intrinsic" => "first",
            "aic_vec_last_intrinsic" => "last",
            "aic_vec_push_intrinsic" => "push",
            "aic_vec_pop_intrinsic" => "pop",
            "aic_vec_set_intrinsic" => "set",
            "aic_vec_insert_intrinsic" => "insert",
            "aic_vec_remove_at_intrinsic" => "remove_at",
            "aic_vec_contains_intrinsic" => "contains",
            "aic_vec_index_of_intrinsic" => "index_of",
            "aic_vec_reverse_intrinsic" => "reverse",
            "aic_vec_slice_intrinsic" => "slice",
            "aic_vec_append_intrinsic" => "append",
            "aic_vec_clear_intrinsic" => "clear",
            "aic_vec_reserve_intrinsic" => "reserve",
            "aic_vec_shrink_to_fit_intrinsic" => "shrink_to_fit",
            _ => name,
        };
        if let Some(result_ty) = self.fn_sig_ret_by_name_or_suffix(name) {
            return Some(result_ty);
        }
        if let Some(result_ty) = self.fn_sig_ret_by_name_or_suffix(canonical_name) {
            return Some(result_ty);
        }
        if let Some(expected) = expected_ty {
            return Some(expected.clone());
        }
        self.diagnostics.push(Diagnostic::error(
            "E5012",
            format!("unknown function '{name}' in codegen"),
            self.file,
            span,
        ));
        None
    }

    pub(super) fn vec_element_info(
        &mut self,
        vec_ty: &LType,
        context: &str,
        span: crate::span::Span,
    ) -> Option<(LType, String, i64)> {
        let LType::Struct(layout) = vec_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Vec[T], found '{}'", render_type(vec_ty)),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Vec" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Vec[T], found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }
        if layout.fields.len() != 3
            || layout.fields[0].ty != LType::Int
            || layout.fields[1].ty != LType::Int
            || layout.fields[2].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Vec[T] layout with ptr/len/cap Int fields"),
                self.file,
                span,
            ));
            return None;
        }
        let Some(args) = extract_generic_args(&layout.repr) else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "{context} expects applied Vec[T] type, found '{}'",
                    layout.repr
                ),
                self.file,
                span,
            ));
            return None;
        };
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "{context} expects one Vec type argument, found '{}'",
                    layout.repr
                ),
                self.file,
                span,
            ));
            return None;
        }
        let elem_repr = args[0].clone();
        let Some(elem_ty) = self.parse_type_repr(&elem_repr, span) else {
            return None;
        };
        let normalized = elem_repr.replace(' ', "");
        let elem_kind = match normalized.as_str() {
            "Int" => 1,
            "Bool" => 2,
            "String" => 3,
            "Option[Int]" => 4,
            _ => 0,
        };
        Some((elem_ty, elem_repr, elem_kind))
    }

    pub(super) fn vec_elem_size(&mut self, elem_ty: &LType, fctx: &mut FnCtx) -> String {
        let elem_ptr = self.new_temp();
        let llvm_elem_ty = llvm_type(elem_ty);
        fctx.lines.push(format!(
            "  {} = getelementptr {}, {}* null, i64 1",
            elem_ptr, llvm_elem_ty, llvm_elem_ty
        ));
        let elem_size = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint {}* {} to i64",
            elem_size, llvm_elem_ty, elem_ptr
        ));
        elem_size
    }

    pub(super) fn value_to_i8_ptr(&mut self, value: &Value, fctx: &mut FnCtx) -> String {
        let slot = self.alloc_entry_slot(&value.ty, fctx);
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&value.ty),
            value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&value.ty)),
            llvm_type(&value.ty),
            slot
        ));
        let out = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast {}* {} to i8*",
            out,
            llvm_type(&value.ty),
            slot
        ));
        out
    }

    pub(super) fn vec_ptr_len_cap_i8(
        &mut self,
        vec_value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String)> {
        let (ptr_int, len, cap) = self.vec_parts(vec_value, span, fctx)?;
        let ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = inttoptr i64 {} to i8*", ptr, ptr_int));
        Some((ptr, len, cap))
    }

    pub(super) fn vec_slots_from_value(
        &mut self,
        vec_value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String)> {
        let (ptr, len, cap) = self.vec_ptr_len_cap_i8(vec_value, span, fctx)?;
        let ptr_slot = self.new_temp();
        let len_slot = self.new_temp();
        let cap_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", len_slot));
        fctx.lines.push(format!("  {} = alloca i64", cap_slot));
        fctx.lines
            .push(format!("  store i8* {}, i8** {}", ptr, ptr_slot));
        fctx.lines
            .push(format!("  store i64 {}, i64* {}", len, len_slot));
        fctx.lines
            .push(format!("  store i64 {}, i64* {}", cap, cap_slot));
        Some((ptr_slot, len_slot, cap_slot))
    }

    pub(super) fn build_vec_value_from_parts(
        &mut self,
        expected_ty: &LType,
        ptr_i8: &str,
        len: &str,
        cap: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Struct(layout) = expected_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "vec builtin expects Vec return type, found '{}'",
                    render_type(expected_ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Vec"
            || layout.fields.len() != 3
            || layout.fields[0].ty != LType::Int
            || layout.fields[1].ty != LType::Int
            || layout.fields[2].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("vec builtin expects Vec[T] layout, found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }
        let ptr_as_int = self.new_temp();
        fctx.lines
            .push(format!("  {} = ptrtoint i8* {} to i64", ptr_as_int, ptr_i8));
        self.build_struct_value(
            layout,
            &[
                Value {
                    ty: LType::Int,
                    repr: Some(ptr_as_int),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(len.to_string()),
                },
                Value {
                    ty: LType::Int,
                    repr: Some(cap.to_string()),
                },
            ],
            span,
            fctx,
        )
    }

    pub(super) fn load_vec_from_slots(
        &mut self,
        expected_ty: &LType,
        ptr_slot: &str,
        len_slot: &str,
        cap_slot: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let ptr = self.new_temp();
        let len = self.new_temp();
        let cap = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", ptr, ptr_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", len, len_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", cap, cap_slot));
        self.build_vec_value_from_parts(expected_ty, &ptr, &len, &cap, span, fctx)
    }

    pub(super) fn gen_vec_new_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "new_vec expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let result_ty = self.vec_result_ty(name, span, expected_ty)?;
        let _ = self.vec_element_info(&result_ty, "new_vec", span)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        let out_cap_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_cap_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_vec_new(i8** {}, i64* {}, i64* {})",
            out_ptr_slot, out_len_slot, out_cap_slot
        ));
        self.load_vec_from_slots(
            &result_ty,
            &out_ptr_slot,
            &out_len_slot,
            &out_cap_slot,
            span,
            fctx,
        )
    }

    pub(super) fn gen_vec_new_with_capacity_call(
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
                "new_vec_with_capacity expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let capacity = self.gen_expr(&args[0], fctx)?;
        if capacity.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "new_vec_with_capacity expects Int capacity",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let result_ty = self.vec_result_ty(name, span, expected_ty)?;
        let (elem_ty, _elem_repr, _elem_kind) =
            self.vec_element_info(&result_ty, "new_vec_with_capacity", span)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        let out_cap_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_cap_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_with_capacity(i64 {}, i64 {}, i8** {}, i64* {}, i64* {})",
            _err,
            capacity.repr.clone().unwrap_or_else(|| "0".to_string()),
            elem_size,
            out_ptr_slot,
            out_len_slot,
            out_cap_slot
        ));
        self.load_vec_from_slots(
            &result_ty,
            &out_ptr_slot,
            &out_len_slot,
            &out_cap_slot,
            span,
            fctx,
        )
    }

    pub(super) fn gen_vec_of_call(
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
                "vec_of expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        let canonical_name = match name {
            "aic_vec_of_intrinsic" => "vec_of",
            _ => name,
        };
        let result_ty = if let Some(expected) = expected_ty {
            expected.clone()
        } else if let Some(known) = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .or_else(|| self.fn_sig(canonical_name).map(|sig| sig.ret.clone()))
        {
            known
        } else {
            let inferred = format!("Vec[{}]", render_type(&value.ty));
            self.parse_type_repr(&inferred, span)?
        };
        let (elem_ty, elem_repr, _elem_kind) = self.vec_element_info(&result_ty, "vec_of", span)?;
        if value.ty != elem_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "vec_of value type mismatch: expected '{}', found '{}'",
                    elem_repr,
                    render_type(&value.ty)
                ),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let value_ptr = self.value_to_i8_ptr(&value, fctx);
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        let out_cap_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_cap_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_of(i8* {}, i64 {}, i8** {}, i64* {}, i64* {})",
            _err, value_ptr, elem_size, out_ptr_slot, out_len_slot, out_cap_slot
        ));
        self.load_vec_from_slots(
            &result_ty,
            &out_ptr_slot,
            &out_len_slot,
            &out_cap_slot,
            span,
            fctx,
        )
    }

    pub(super) fn gen_vec_get_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "get expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let index_value = self.gen_expr(&args[1], fctx)?;
        if index_value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "get expects Int index",
                self.file,
                args[1].span,
            ));
            return None;
        }
        let (elem_ty, elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "get", args[0].span)?;
        let option_ty = self.parse_type_repr(&format!("Option[{}]", elem_repr), span)?;
        let (ptr, len, cap) = self.vec_ptr_len_cap_i8(&vec_value, args[0].span, fctx)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let out_slot = self.alloc_entry_slot(&elem_ty, fctx);
        let out_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast {}* {} to i8*",
            out_ptr,
            llvm_type(&elem_ty),
            out_slot
        ));
        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_get(i8* {}, i64 {}, i64 {}, i64 {}, i64 {}, i8* {})",
            found,
            ptr,
            len,
            cap,
            index_value.repr.clone().unwrap_or_else(|| "0".to_string()),
            elem_size,
            out_ptr
        ));
        let has_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", has_value, found));
        let loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            loaded,
            llvm_type(&elem_ty),
            llvm_type(&elem_ty),
            out_slot
        ));
        self.wrap_option_with_condition(
            &option_ty,
            Value {
                ty: elem_ty,
                repr: Some(loaded),
            },
            &has_value,
            span,
            fctx,
        )
    }

    pub(super) fn gen_vec_first_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "first expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "first", args[0].span)?;
        let option_ty = self.parse_type_repr(&format!("Option[{}]", elem_repr), span)?;
        let (ptr, len, cap) = self.vec_ptr_len_cap_i8(&vec_value, args[0].span, fctx)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let out_slot = self.alloc_entry_slot(&elem_ty, fctx);
        let out_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast {}* {} to i8*",
            out_ptr,
            llvm_type(&elem_ty),
            out_slot
        ));
        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_get(i8* {}, i64 {}, i64 {}, i64 0, i64 {}, i8* {})",
            found, ptr, len, cap, elem_size, out_ptr
        ));
        let has_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", has_value, found));
        let loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            loaded,
            llvm_type(&elem_ty),
            llvm_type(&elem_ty),
            out_slot
        ));
        self.wrap_option_with_condition(
            &option_ty,
            Value {
                ty: elem_ty,
                repr: Some(loaded),
            },
            &has_value,
            span,
            fctx,
        )
    }

    pub(super) fn gen_vec_last_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "last expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "last", args[0].span)?;
        let option_ty = self.parse_type_repr(&format!("Option[{}]", elem_repr), span)?;
        let (ptr, len, cap) = self.vec_ptr_len_cap_i8(&vec_value, args[0].span, fctx)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let out_slot = self.alloc_entry_slot(&elem_ty, fctx);
        let out_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast {}* {} to i8*",
            out_ptr,
            llvm_type(&elem_ty),
            out_slot
        ));
        let index = self.new_temp();
        fctx.lines.push(format!("  {} = sub i64 {}, 1", index, len));
        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_get(i8* {}, i64 {}, i64 {}, i64 {}, i64 {}, i8* {})",
            found, ptr, len, cap, index, elem_size, out_ptr
        ));
        let has_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", has_value, found));
        let loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            loaded,
            llvm_type(&elem_ty),
            llvm_type(&elem_ty),
            out_slot
        ));
        self.wrap_option_with_condition(
            &option_ty,
            Value {
                ty: elem_ty,
                repr: Some(loaded),
            },
            &has_value,
            span,
            fctx,
        )
    }

    pub(super) fn gen_vec_push_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "push expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "push", args[0].span)?;
        let value = self.gen_expr_with_expected(&args[1], Some(&elem_ty), fctx)?;
        let Some(value) = self.coerce_value_to_expected(value, &elem_ty, args[1].span, fctx) else {
            return None;
        };
        if !self.types_compatible_for_codegen(&elem_ty, &value.ty, args[1].span) {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "push value type mismatch: expected '{}', found '{}'",
                    elem_repr,
                    render_type(&value.ty)
                ),
                self.file,
                args[1].span,
            ));
            return None;
        }
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let value_ptr = self.value_to_i8_ptr(&value, fctx);
        let (ptr_slot, len_slot, cap_slot) =
            self.vec_slots_from_value(&vec_value, args[0].span, fctx)?;
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_push(i8** {}, i64* {}, i64* {}, i64 {}, i8* {})",
            _err, ptr_slot, len_slot, cap_slot, elem_size, value_ptr
        ));
        self.load_vec_from_slots(&vec_value.ty, &ptr_slot, &len_slot, &cap_slot, span, fctx)
    }

    pub(super) fn gen_vec_pop_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "pop expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, _elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "pop", args[0].span)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let (ptr_slot, len_slot, cap_slot) =
            self.vec_slots_from_value(&vec_value, args[0].span, fctx)?;
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_pop(i8** {}, i64* {}, i64* {}, i64 {})",
            _err, ptr_slot, len_slot, cap_slot, elem_size
        ));
        self.load_vec_from_slots(&vec_value.ty, &ptr_slot, &len_slot, &cap_slot, span, fctx)
    }

    pub(super) fn gen_vec_set_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "set expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let index = self.gen_expr(&args[1], fctx)?;
        if index.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "set expects Int index",
                self.file,
                args[1].span,
            ));
            return None;
        }
        let (elem_ty, elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "set", args[0].span)?;
        let value = self.gen_expr_with_expected(&args[2], Some(&elem_ty), fctx)?;
        let Some(value) = self.coerce_value_to_expected(value, &elem_ty, args[2].span, fctx) else {
            return None;
        };
        if !self.types_compatible_for_codegen(&elem_ty, &value.ty, args[2].span) {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "set value type mismatch: expected '{}', found '{}'",
                    elem_repr,
                    render_type(&value.ty)
                ),
                self.file,
                args[2].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.vec_ptr_len_cap_i8(&vec_value, args[0].span, fctx)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let value_ptr = self.value_to_i8_ptr(&value, fctx);
        let _updated = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_set(i8* {}, i64 {}, i64 {}, i64 {}, i64 {}, i8* {})",
            _updated,
            ptr,
            len,
            cap,
            index.repr.clone().unwrap_or_else(|| "0".to_string()),
            elem_size,
            value_ptr
        ));
        Some(vec_value)
    }

    pub(super) fn gen_vec_insert_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "insert expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let index = self.gen_expr(&args[1], fctx)?;
        if index.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "insert expects Int index",
                self.file,
                args[1].span,
            ));
            return None;
        }
        let (elem_ty, elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "insert", args[0].span)?;
        let value = self.gen_expr_with_expected(&args[2], Some(&elem_ty), fctx)?;
        let Some(value) = self.coerce_value_to_expected(value, &elem_ty, args[2].span, fctx) else {
            return None;
        };
        if !self.types_compatible_for_codegen(&elem_ty, &value.ty, args[2].span) {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "insert value type mismatch: expected '{}', found '{}'",
                    elem_repr,
                    render_type(&value.ty)
                ),
                self.file,
                args[2].span,
            ));
            return None;
        }
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let value_ptr = self.value_to_i8_ptr(&value, fctx);
        let (ptr_slot, len_slot, cap_slot) =
            self.vec_slots_from_value(&vec_value, args[0].span, fctx)?;
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_insert(i8** {}, i64* {}, i64* {}, i64 {}, i64 {}, i8* {})",
            _err,
            ptr_slot,
            len_slot,
            cap_slot,
            index.repr.clone().unwrap_or_else(|| "0".to_string()),
            elem_size,
            value_ptr
        ));
        self.load_vec_from_slots(&vec_value.ty, &ptr_slot, &len_slot, &cap_slot, span, fctx)
    }

    pub(super) fn gen_vec_remove_at_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "remove_at expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let index = self.gen_expr(&args[1], fctx)?;
        if index.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "remove_at expects Int index",
                self.file,
                args[1].span,
            ));
            return None;
        }
        let (elem_ty, _elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "remove_at", args[0].span)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let (ptr_slot, len_slot, cap_slot) =
            self.vec_slots_from_value(&vec_value, args[0].span, fctx)?;
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_remove_at(i8** {}, i64* {}, i64* {}, i64 {}, i64 {})",
            _err,
            ptr_slot,
            len_slot,
            cap_slot,
            index.repr.clone().unwrap_or_else(|| "0".to_string()),
            elem_size
        ));
        self.load_vec_from_slots(&vec_value.ty, &ptr_slot, &len_slot, &cap_slot, span, fctx)
    }

    pub(super) fn gen_vec_contains_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "contains expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let needle = self.gen_expr(&args[1], fctx)?;
        let (elem_ty, elem_repr, elem_kind) =
            self.vec_element_info(&vec_value.ty, "contains", args[0].span)?;
        if needle.ty != elem_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "contains value type mismatch: expected '{}', found '{}'",
                    elem_repr,
                    render_type(&needle.ty)
                ),
                self.file,
                args[1].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.vec_ptr_len_cap_i8(&vec_value, args[0].span, fctx)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let needle_ptr = self.value_to_i8_ptr(&needle, fctx);
        let raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_contains(i8* {}, i64 {}, i64 {}, i64 {}, i64 {}, i8* {})",
            raw, ptr, len, cap, elem_kind, elem_size, needle_ptr
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", out, raw));
        Some(Value {
            ty: LType::Bool,
            repr: Some(out),
        })
    }

    pub(super) fn gen_vec_index_of_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "index_of expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let needle = self.gen_expr(&args[1], fctx)?;
        let (elem_ty, elem_repr, elem_kind) =
            self.vec_element_info(&vec_value.ty, "index_of", args[0].span)?;
        if needle.ty != elem_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "index_of value type mismatch: expected '{}', found '{}'",
                    elem_repr,
                    render_type(&needle.ty)
                ),
                self.file,
                args[1].span,
            ));
            return None;
        }
        let option_ty = self.parse_type_repr("Option[Int]", span)?;
        let (ptr, len, cap) = self.vec_ptr_len_cap_i8(&vec_value, args[0].span, fctx)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let needle_ptr = self.value_to_i8_ptr(&needle, fctx);
        let out_index_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_index_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_index_slot));
        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_index_of(i8* {}, i64 {}, i64 {}, i64 {}, i64 {}, i8* {}, i64* {})",
            found, ptr, len, cap, elem_kind, elem_size, needle_ptr, out_index_slot
        ));
        let has_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", has_value, found));
        let out_index = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_index, out_index_slot
        ));
        self.wrap_option_with_condition(
            &option_ty,
            Value {
                ty: LType::Int,
                repr: Some(out_index),
            },
            &has_value,
            span,
            fctx,
        )
    }

    pub(super) fn gen_vec_reverse_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "reverse expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, _elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "reverse", args[0].span)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let (ptr, len, cap) = self.vec_ptr_len_cap_i8(&vec_value, args[0].span, fctx)?;
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_reverse(i8* {}, i64 {}, i64 {}, i64 {})",
            _err, ptr, len, cap, elem_size
        ));
        Some(vec_value)
    }

    pub(super) fn gen_vec_slice_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "slice expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let start = self.gen_expr(&args[1], fctx)?;
        let end = self.gen_expr(&args[2], fctx)?;
        if start.ty != LType::Int || end.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "slice expects Int start/end",
                self.file,
                span,
            ));
            return None;
        }
        let (elem_ty, _elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "slice", args[0].span)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let (ptr, len, cap) = self.vec_ptr_len_cap_i8(&vec_value, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        let out_cap_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_cap_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_slice(i8* {}, i64 {}, i64 {}, i64 {}, i64 {}, i64 {}, i8** {}, i64* {}, i64* {})",
            _err,
            ptr,
            len,
            cap,
            start.repr.clone().unwrap_or_else(|| "0".to_string()),
            end.repr.clone().unwrap_or_else(|| "0".to_string()),
            elem_size,
            out_ptr_slot,
            out_len_slot,
            out_cap_slot
        ));
        self.load_vec_from_slots(
            &vec_value.ty,
            &out_ptr_slot,
            &out_len_slot,
            &out_cap_slot,
            span,
            fctx,
        )
    }

    pub(super) fn gen_vec_append_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "append expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let left = self.gen_expr(&args[0], fctx)?;
        let right = self.gen_expr(&args[1], fctx)?;
        let (elem_ty, _elem_repr, _elem_kind) =
            self.vec_element_info(&left.ty, "append", args[0].span)?;
        if left.ty != right.ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "append expects matching Vec types, found '{}' and '{}'",
                    render_type(&left.ty),
                    render_type(&right.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let (right_ptr, right_len, right_cap) =
            self.vec_ptr_len_cap_i8(&right, args[1].span, fctx)?;
        let (ptr_slot, len_slot, cap_slot) =
            self.vec_slots_from_value(&left, args[0].span, fctx)?;
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_append(i8** {}, i64* {}, i64* {}, i64 {}, i8* {}, i64 {}, i64 {})",
            _err,
            ptr_slot,
            len_slot,
            cap_slot,
            elem_size,
            right_ptr,
            right_len,
            right_cap
        ));
        self.load_vec_from_slots(&left.ty, &ptr_slot, &len_slot, &cap_slot, span, fctx)
    }

    pub(super) fn gen_vec_clear_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "clear expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let _ = self.vec_element_info(&vec_value.ty, "clear", args[0].span)?;
        let (ptr_slot, len_slot, cap_slot) =
            self.vec_slots_from_value(&vec_value, args[0].span, fctx)?;
        fctx.lines.push(format!(
            "  call void @aic_rt_vec_clear(i8** {}, i64* {}, i64* {})",
            ptr_slot, len_slot, cap_slot
        ));
        self.load_vec_from_slots(&vec_value.ty, &ptr_slot, &len_slot, &cap_slot, span, fctx)
    }

    pub(super) fn gen_vec_reserve_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "reserve expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let additional = self.gen_expr(&args[1], fctx)?;
        if additional.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "reserve expects Int additional",
                self.file,
                args[1].span,
            ));
            return None;
        }
        let (elem_ty, _elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "reserve", args[0].span)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let (ptr_slot, len_slot, cap_slot) =
            self.vec_slots_from_value(&vec_value, args[0].span, fctx)?;
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_reserve(i8** {}, i64* {}, i64* {}, i64 {}, i64 {})",
            _err,
            ptr_slot,
            len_slot,
            cap_slot,
            additional.repr.clone().unwrap_or_else(|| "0".to_string()),
            elem_size
        ));
        self.load_vec_from_slots(&vec_value.ty, &ptr_slot, &len_slot, &cap_slot, span, fctx)
    }

    pub(super) fn gen_vec_shrink_to_fit_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "shrink_to_fit expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let vec_value = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, _elem_repr, _elem_kind) =
            self.vec_element_info(&vec_value.ty, "shrink_to_fit", args[0].span)?;
        let elem_size = self.vec_elem_size(&elem_ty, fctx);
        let (ptr_slot, len_slot, cap_slot) =
            self.vec_slots_from_value(&vec_value, args[0].span, fctx)?;
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_vec_shrink_to_fit(i8** {}, i64* {}, i64* {}, i64 {})",
            _err, ptr_slot, len_slot, cap_slot, elem_size
        ));
        self.load_vec_from_slots(&vec_value.ty, &ptr_slot, &len_slot, &cap_slot, span, fctx)
    }
}
