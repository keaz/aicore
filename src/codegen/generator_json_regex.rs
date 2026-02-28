use super::*;

impl<'a> Generator<'a> {
    pub(super) fn gen_json_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "parse" | "aic_json_parse_intrinsic" => "parse",
            "stringify" | "aic_json_stringify_intrinsic" => "stringify",
            "encode_int" | "aic_json_encode_int_intrinsic" => "encode_int",
            "encode_float" | "aic_json_encode_float_intrinsic" => "encode_float",
            "encode_bool" | "aic_json_encode_bool_intrinsic" => "encode_bool",
            "encode_string" | "aic_json_encode_string_intrinsic" => "encode_string",
            "encode_null" | "aic_json_encode_null_intrinsic" => "encode_null",
            "encode" | "aic_json_serde_encode_intrinsic" => "encode_any",
            "decode_int" | "aic_json_decode_int_intrinsic" => "decode_int",
            "decode_float" | "aic_json_decode_float_intrinsic" => "decode_float",
            "decode_bool" | "aic_json_decode_bool_intrinsic" => "decode_bool",
            "decode_string" | "aic_json_decode_string_intrinsic" => "decode_string",
            "decode_with" | "aic_json_serde_decode_intrinsic" => "decode_any",
            "schema" | "aic_json_serde_schema_intrinsic" => "schema_any",
            "object_empty" | "aic_json_object_empty_intrinsic" => "object_empty",
            "object_set" | "aic_json_object_set_intrinsic" => "object_set",
            "object_get" | "aic_json_object_get_intrinsic" => "object_get",
            "kind" | "aic_json_kind_intrinsic" => "kind",
            _ => return None,
        };

        match canonical {
            "parse"
                if self.sig_matches_shape(name, &["String"], "Result[JsonValue, JsonError]") =>
            {
                Some(self.gen_json_parse_call(name, args, span, fctx))
            }
            "stringify"
                if self.sig_matches_shape(name, &["JsonValue"], "Result[String, JsonError]") =>
            {
                Some(self.gen_json_stringify_call(name, args, span, fctx))
            }
            "encode_int" if self.sig_matches_shape(name, &["Int"], "JsonValue") => {
                Some(self.gen_json_encode_int_call(args, span, fctx))
            }
            "encode_float" if self.sig_matches_shape(name, &["Float"], "JsonValue") => {
                Some(self.gen_json_encode_float_call(args, span, fctx))
            }
            "encode_bool" if self.sig_matches_shape(name, &["Bool"], "JsonValue") => {
                Some(self.gen_json_encode_bool_call(args, span, fctx))
            }
            "encode_string" if self.sig_matches_shape(name, &["String"], "JsonValue") => {
                Some(self.gen_json_encode_string_call(args, span, fctx))
            }
            "encode_null" if self.sig_matches_shape(name, &[], "JsonValue") => {
                Some(self.gen_json_encode_null_call(args, span, fctx))
            }
            "encode_any" => Some(self.gen_json_derive_encode_call(name, args, span, fctx)),
            "decode_int"
                if self.sig_matches_shape(name, &["JsonValue"], "Result[Int, JsonError]") =>
            {
                Some(self.gen_json_decode_int_call(name, args, span, fctx))
            }
            "decode_float"
                if self.sig_matches_shape(name, &["JsonValue"], "Result[Float, JsonError]") =>
            {
                Some(self.gen_json_decode_float_call(name, args, span, fctx))
            }
            "decode_bool"
                if self.sig_matches_shape(name, &["JsonValue"], "Result[Bool, JsonError]") =>
            {
                Some(self.gen_json_decode_bool_call(name, args, span, fctx))
            }
            "decode_string"
                if self.sig_matches_shape(name, &["JsonValue"], "Result[String, JsonError]") =>
            {
                Some(self.gen_json_decode_string_call(name, args, span, fctx))
            }
            "decode_any" => Some(self.gen_json_derive_decode_call(name, args, span, fctx)),
            "schema_any" => Some(self.gen_json_derive_schema_call(name, args, span, fctx)),
            "object_empty" if self.sig_matches_shape(name, &[], "JsonValue") => {
                Some(self.gen_json_object_empty_call(args, span, fctx))
            }
            "object_set"
                if self.sig_matches_shape(
                    name,
                    &["JsonValue", "String", "JsonValue"],
                    "Result[JsonValue, JsonError]",
                ) =>
            {
                Some(self.gen_json_object_set_call(name, args, span, fctx))
            }
            "object_get"
                if self.sig_matches_shape(
                    name,
                    &["JsonValue", "String"],
                    "Result[Option[JsonValue], JsonError]",
                ) =>
            {
                Some(self.gen_json_object_get_call(name, args, span, fctx))
            }
            "kind" if self.sig_matches_shape(name, &["JsonValue"], "JsonKind") => {
                Some(self.gen_json_kind_call(args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_json_parse_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "parse expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let text = self.gen_expr(&args[0], fctx)?;
        if text.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "parse expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&text, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let out_kind_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_kind_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_parse(i8* {}, i64 {}, i64 {}, i8** {}, i64* {}, i64* {})",
            err, ptr, len, cap, out_ptr_slot, out_len_slot, out_kind_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let out_kind = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_kind, out_kind_slot));

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
        let raw = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&ok_ty, span)?.3;
        let kind_value = self.build_json_kind_from_code(&kind_ty, &out_kind, span, fctx)?;
        let ok_payload = self.build_json_value_struct(&ok_ty, raw, kind_value, span, fctx)?;
        self.wrap_json_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_json_stringify_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "stringify expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        let (raw_ptr, raw_len, raw_cap) = self.json_raw_parts(&value, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_stringify(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err, raw_ptr, raw_len, raw_cap, out_ptr_slot, out_len_slot
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
        self.wrap_json_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_json_encode_int_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "encode_int expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "encode_int expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let int_repr = value.repr.clone().unwrap_or_else(|| "0".to_string());
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_encode_int(i64 {}, i8** {}, i64* {})",
            _err, int_repr, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let raw_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let json_ty = self
            .fn_sigs
            .get("encode_int")
            .map(|sig| sig.ret.clone())
            .unwrap_or_else(|| {
                LType::Struct(StructLayoutType {
                    repr: "JsonValue".to_string(),
                    fields: Vec::new(),
                })
            });
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind_value = self.build_json_kind_from_code(&kind_ty, "2", span, fctx)?;
        self.build_json_value_struct(&json_ty, raw_value, kind_value, span, fctx)
    }

    pub(super) fn gen_json_encode_float_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "encode_float expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Float {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "encode_float expects Float",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let value_repr = value
            .repr
            .clone()
            .unwrap_or_else(|| llvm_float_literal(0.0_f64));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_encode_float(double {}, i8** {}, i64* {})",
            _err, value_repr, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let raw_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let json_ty = self
            .fn_sigs
            .get("encode_float")
            .map(|sig| sig.ret.clone())
            .unwrap_or_else(|| {
                LType::Struct(StructLayoutType {
                    repr: "JsonValue".to_string(),
                    fields: Vec::new(),
                })
            });
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind_value = self.build_json_kind_from_code(&kind_ty, "2", span, fctx)?;
        self.build_json_value_struct(&json_ty, raw_value, kind_value, span, fctx)
    }

    pub(super) fn gen_json_encode_bool_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "encode_bool expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "encode_bool expects Bool",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let bool_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = zext i1 {} to i64",
            bool_i64,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_encode_bool(i64 {}, i8** {}, i64* {})",
            _err, bool_i64, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let raw_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let json_ty = self
            .fn_sigs
            .get("encode_bool")
            .map(|sig| sig.ret.clone())
            .unwrap_or_else(|| {
                LType::Struct(StructLayoutType {
                    repr: "JsonValue".to_string(),
                    fields: Vec::new(),
                })
            });
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind_value = self.build_json_kind_from_code(&kind_ty, "1", span, fctx)?;
        self.build_json_value_struct(&json_ty, raw_value, kind_value, span, fctx)
    }

    pub(super) fn gen_json_encode_string_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "encode_string expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "encode_string expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&value, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_encode_string(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            _err, ptr, len, cap, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let raw_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let json_ty = self
            .fn_sigs
            .get("encode_string")
            .map(|sig| sig.ret.clone())
            .unwrap_or_else(|| {
                LType::Struct(StructLayoutType {
                    repr: "JsonValue".to_string(),
                    fields: Vec::new(),
                })
            });
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind_value = self.build_json_kind_from_code(&kind_ty, "3", span, fctx)?;
        self.build_json_value_struct(&json_ty, raw_value, kind_value, span, fctx)
    }

    pub(super) fn gen_json_encode_null_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "encode_null expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_encode_null(i8** {}, i64* {})",
            _err, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let raw_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let json_ty = self
            .fn_sigs
            .get("encode_null")
            .map(|sig| sig.ret.clone())
            .unwrap_or_else(|| {
                LType::Struct(StructLayoutType {
                    repr: "JsonValue".to_string(),
                    fields: Vec::new(),
                })
            });
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind_value = self.build_json_kind_from_code(&kind_ty, "0", span, fctx)?;
        self.build_json_value_struct(&json_ty, raw_value, kind_value, span, fctx)
    }

    pub(super) fn gen_json_decode_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "decode_int expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        let (raw_ptr, raw_len, raw_cap) = self.json_raw_parts(&value, args[0].span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_decode_int(i8* {}, i64 {}, i64 {}, i64* {})",
            err, raw_ptr, raw_len, raw_cap, out_slot
        ));
        let out_reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_reg, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_reg),
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
        self.wrap_json_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_json_decode_float_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "decode_float expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        let (raw_ptr, raw_len, raw_cap) = self.json_raw_parts(&value, args[0].span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca double", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_decode_float(i8* {}, i64 {}, i64 {}, double* {})",
            err, raw_ptr, raw_len, raw_cap, out_slot
        ));
        let out_reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = load double, double* {}", out_reg, out_slot));
        let ok_payload = Value {
            ty: LType::Float,
            repr: Some(out_reg),
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
        self.wrap_json_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_json_decode_bool_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "decode_bool expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        let (raw_ptr, raw_len, raw_cap) = self.json_raw_parts(&value, args[0].span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_decode_bool(i8* {}, i64 {}, i64 {}, i64* {})",
            err, raw_ptr, raw_len, raw_cap, out_slot
        ));
        let out_reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_reg, out_slot));
        let bool_reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", bool_reg, out_reg));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(bool_reg),
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
        self.wrap_json_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_json_decode_string_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "decode_string expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        let (raw_ptr, raw_len, raw_cap) = self.json_raw_parts(&value, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_decode_string(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err, raw_ptr, raw_len, raw_cap, out_ptr_slot, out_len_slot
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
        self.wrap_json_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_json_object_empty_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "object_empty expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_object_empty(i8** {}, i64* {})",
            _err, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let json_ty = self
            .fn_sigs
            .get("object_empty")
            .map(|sig| sig.ret.clone())
            .unwrap_or_else(|| {
                LType::Struct(StructLayoutType {
                    repr: "JsonValue".to_string(),
                    fields: Vec::new(),
                })
            });
        let raw_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind_value = self.build_json_kind_from_code(&kind_ty, "5", span, fctx)?;
        self.build_json_value_struct(&json_ty, raw_value, kind_value, span, fctx)
    }

    pub(super) fn gen_json_object_set_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "object_set expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let object = self.gen_expr(&args[0], fctx)?;
        let key = self.gen_expr(&args[1], fctx)?;
        let value = self.gen_expr(&args[2], fctx)?;
        if key.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "object_set key expects String",
                self.file,
                args[1].span,
            ));
            return None;
        }
        let (obj_ptr, obj_len, obj_cap) = self.json_raw_parts(&object, args[0].span, fctx)?;
        let (key_ptr, key_len, key_cap) = self.string_parts(&key, args[1].span, fctx)?;
        let (value_ptr, value_len, value_cap) = self.json_raw_parts(&value, args[2].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let out_kind_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_kind_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_object_set(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {}, i64* {})",
            err,
            obj_ptr,
            obj_len,
            obj_cap,
            key_ptr,
            key_len,
            key_cap,
            value_ptr,
            value_len,
            value_cap,
            out_ptr_slot,
            out_len_slot,
            out_kind_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let out_kind = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_kind, out_kind_slot));
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
        let raw_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&ok_ty, span)?.3;
        let kind_value = self.build_json_kind_from_code(&kind_ty, &out_kind, span, fctx)?;
        let ok_payload = self.build_json_value_struct(&ok_ty, raw_value, kind_value, span, fctx)?;
        self.wrap_json_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_json_object_get_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "object_get expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let object = self.gen_expr(&args[0], fctx)?;
        let key = self.gen_expr(&args[1], fctx)?;
        if key.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "object_get key expects String",
                self.file,
                args[1].span,
            ));
            return None;
        }
        let (obj_ptr, obj_len, obj_cap) = self.json_raw_parts(&object, args[0].span, fctx)?;
        let (key_ptr, key_len, key_cap) = self.string_parts(&key, args[1].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let out_kind_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_kind_slot));
        let out_found_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_found_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_object_get(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {}, i64* {}, i64* {})",
            err,
            obj_ptr,
            obj_len,
            obj_cap,
            key_ptr,
            key_len,
            key_cap,
            out_ptr_slot,
            out_len_slot,
            out_kind_slot,
            out_found_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let out_kind = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_kind, out_kind_slot));
        let out_found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_found, out_found_slot
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
        let LType::Enum(option_layout) = ok_ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "object_get expects Result[Option[JsonValue], JsonError] return type",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&option_layout.repr) != "Option" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "object_get expects Option payload",
                self.file,
                span,
            ));
            return None;
        }
        let Some(none_index) = option_layout
            .variants
            .iter()
            .position(|variant| variant.name == "None")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Option payload missing None variant",
                self.file,
                span,
            ));
            return None;
        };
        let Some(some_index) = option_layout
            .variants
            .iter()
            .position(|variant| variant.name == "Some")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Option payload missing Some variant",
                self.file,
                span,
            ));
            return None;
        };
        let Some(some_payload_ty) = option_layout.variants[some_index].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Option Some variant missing payload type",
                self.file,
                span,
            ));
            return None;
        };
        let raw_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&some_payload_ty, span)?.3;
        let kind_value = self.build_json_kind_from_code(&kind_ty, &out_kind, span, fctx)?;
        let json_payload =
            self.build_json_value_struct(&some_payload_ty, raw_value, kind_value, span, fctx)?;

        let none_value = self.build_enum_variant(&option_layout, none_index, None, span, fctx)?;
        let some_value =
            self.build_enum_variant(&option_layout, some_index, Some(json_payload), span, fctx)?;
        let option_slot = self.alloc_entry_slot(&ok_ty, fctx);
        let is_found = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", is_found, out_found));
        let some_label = self.new_label("json_opt_some");
        let none_label = self.new_label("json_opt_none");
        let cont_label = self.new_label("json_opt_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_found, some_label, none_label
        ));

        fctx.lines.push(format!("{}:", some_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&ok_ty),
            some_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&ok_ty)),
            llvm_type(&ok_ty),
            option_slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", none_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&ok_ty),
            none_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&ok_ty)),
            llvm_type(&ok_ty),
            option_slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let ok_payload_reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            ok_payload_reg,
            llvm_type(&ok_ty),
            llvm_type(&ok_ty),
            option_slot
        ));
        let ok_payload = Value {
            ty: ok_ty,
            repr: Some(ok_payload_reg),
        };
        self.wrap_json_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_json_kind_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "kind expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        let (layout, _, kind_index, kind_ty) = self.json_value_layout(&value.ty, args[0].span)?;
        let value_repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            reg,
            llvm_type(&LType::Struct(layout.clone())),
            value_repr,
            kind_index
        ));
        Some(Value {
            ty: kind_ty,
            repr: Some(reg),
        })
    }

    pub(super) fn wrap_json_result(
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
                    "json builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_json_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("json_ok");
        let err_label = self.new_label("json_err");
        let cont_label = self.new_label("json_cont");
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

    pub(super) fn json_value_layout(
        &mut self,
        ty: &LType,
        span: crate::span::Span,
    ) -> Option<(StructLayoutType, usize, usize, LType)> {
        let LType::Struct(layout) = ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("expected JsonValue struct, found '{}'", render_type(ty)),
                self.file,
                span,
            ));
            return None;
        };
        let Some((raw_index, raw_field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == "raw")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "JsonValue struct is missing `raw` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some((kind_index, kind_field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == "kind")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "JsonValue struct is missing `kind` field",
                self.file,
                span,
            ));
            return None;
        };
        if raw_field.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "JsonValue.raw must be String",
                self.file,
                span,
            ));
            return None;
        }
        let LType::Enum(kind_layout) = kind_field.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "JsonValue.kind must be JsonKind enum",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&kind_layout.repr) != "JsonKind" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "JsonValue.kind must use JsonKind enum",
                self.file,
                span,
            ));
            return None;
        }
        Some((layout.clone(), raw_index, kind_index, kind_field.ty.clone()))
    }

    pub(super) fn build_json_value_struct(
        &mut self,
        json_ty: &LType,
        raw_value: Value,
        kind_value: Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let (layout, _, _, _) = self.json_value_layout(json_ty, span)?;
        if raw_value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "JsonValue.raw payload must be String",
                self.file,
                span,
            ));
            return None;
        }

        let mut ordered = Vec::new();
        for field in &layout.fields {
            if field.name == "raw" {
                ordered.push(raw_value.clone());
            } else if field.name == "kind" {
                if field.ty != kind_value.ty {
                    self.diagnostics.push(Diagnostic::error(
                        "E5011",
                        "JsonValue.kind payload type mismatch",
                        self.file,
                        span,
                    ));
                    return None;
                }
                ordered.push(kind_value.clone());
            } else {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    format!(
                        "JsonValue contains unsupported field '{}'; expected only raw/kind",
                        field.name
                    ),
                    self.file,
                    span,
                ));
                return None;
            }
        }
        self.build_struct_value(&layout, &ordered, span, fctx)
    }

    pub(super) fn json_raw_parts(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String)> {
        let (layout, raw_index, _, _) = self.json_value_layout(&value.ty, span)?;
        let value_repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let raw_reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            raw_reg,
            llvm_type(&LType::Struct(layout.clone())),
            value_repr,
            raw_index
        ));
        let raw_value = Value {
            ty: LType::String,
            repr: Some(raw_reg),
        };
        self.string_parts(&raw_value, span, fctx)
    }

    pub(super) fn gen_json_derive_encode_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "encode expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let input = self.gen_expr(&args[0], fctx)?;
        let arg_types = vec![input.ty.clone()];
        let call_sig = self
            .resolve_call_sig_for_types(name, &arg_types, span)
            .or_else(|| {
                if name == "aic_json_serde_encode_intrinsic" {
                    let ret_ty = self.parse_type_repr("Result[JsonValue, JsonError]", span)?;
                    Some(FnSig {
                        is_extern: false,
                        extern_symbol: None,
                        extern_abi: None,
                        is_intrinsic: false,
                        intrinsic_abi: None,
                        params: arg_types.clone(),
                        ret: ret_ty,
                    })
                } else {
                    None
                }
            });
        let Some(call_sig) = call_sig else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{}' in codegen", name),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&call_sig.ret, span) else {
            return None;
        };
        if render_type(&ok_ty) != "JsonValue" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "encode expects Result[JsonValue, JsonError] return type",
                self.file,
                span,
            ));
            return None;
        }
        let encoded = self.json_encode_value(&input, span, fctx)?;
        self.wrap_json_result(&call_sig.ret, encoded.value, &encoded.err_code, span, fctx)
    }

    pub(super) fn gen_json_derive_decode_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "decode_with expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let input = self.gen_expr(&args[0], fctx)?;
        let marker = self.gen_expr(&args[1], fctx)?;
        let target_ty = self.json_marker_payload_ty(&marker.ty, args[1].span)?;
        let arg_types = vec![input.ty.clone(), marker.ty.clone()];
        let call_sig = self
            .resolve_call_sig_for_types(name, &arg_types, span)
            .or_else(|| {
                if name == "aic_json_serde_decode_intrinsic" {
                    let ret_repr = format!("Result[{}, JsonError]", render_type(&target_ty));
                    let ret_ty = self.parse_type_repr(&ret_repr, span)?;
                    Some(FnSig {
                        is_extern: false,
                        extern_symbol: None,
                        extern_abi: None,
                        is_intrinsic: false,
                        intrinsic_abi: None,
                        params: arg_types.clone(),
                        ret: ret_ty,
                    })
                } else {
                    None
                }
            });
        let Some(call_sig) = call_sig else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{}' in codegen", name),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&call_sig.ret, span) else {
            return None;
        };
        if ok_ty != target_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "decode_with marker type does not match function return payload",
                self.file,
                span,
            ));
            return None;
        }
        let decoded = self.json_decode_value(&target_ty, &input, span, fctx)?;
        self.wrap_json_result(&call_sig.ret, decoded.value, &decoded.err_code, span, fctx)
    }

    pub(super) fn gen_json_derive_schema_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "schema expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let marker = self.gen_expr(&args[0], fctx)?;
        let target_ty = self.json_marker_payload_ty(&marker.ty, args[0].span)?;
        let arg_types = vec![marker.ty.clone()];
        let call_sig = self
            .resolve_call_sig_for_types(name, &arg_types, span)
            .or_else(|| {
                if name == "aic_json_serde_schema_intrinsic" {
                    let ret_ty = self.parse_type_repr("Result[String, JsonError]", span)?;
                    Some(FnSig {
                        is_extern: false,
                        extern_symbol: None,
                        extern_abi: None,
                        is_intrinsic: false,
                        intrinsic_abi: None,
                        params: arg_types.clone(),
                        ret: ret_ty,
                    })
                } else {
                    None
                }
            });
        let Some(call_sig) = call_sig else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{}' in codegen", name),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&call_sig.ret, span) else {
            return None;
        };
        if ok_ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "schema expects Result[String, JsonError] return type",
                self.file,
                span,
            ));
            return None;
        }
        let mut stack = Vec::new();
        let schema = self.json_schema_for_type(&target_ty, &mut stack, span)?;
        let payload = self.string_literal(&schema, fctx);
        self.wrap_json_result(&call_sig.ret, payload, "0", span, fctx)
    }

    pub(super) fn json_marker_payload_ty(
        &mut self,
        marker_ty: &LType,
        span: crate::span::Span,
    ) -> Option<LType> {
        let LType::Enum(layout) = marker_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "serde marker must be Option[T]",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Option" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "serde marker must be Option[T]",
                self.file,
                span,
            ));
            return None;
        }
        let Some(some_index) = layout
            .variants
            .iter()
            .position(|variant| variant.name == "Some")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Option marker missing Some variant",
                self.file,
                span,
            ));
            return None;
        };
        let Some(payload) = layout.variants[some_index].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Option marker Some variant is missing payload",
                self.file,
                span,
            ));
            return None;
        };
        Some(payload)
    }

    pub(super) fn json_encode_value(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        match &value.ty {
            LType::Int => self.json_encode_int_runtime(value, span, fctx),
            LType::Float => self.json_encode_float_runtime(value, span, fctx),
            LType::Bool => self.json_encode_bool_runtime(value, span, fctx),
            LType::Char => self.json_encode_char_runtime(value, span, fctx),
            LType::String => self.json_encode_string_runtime(value, span, fctx),
            LType::Unit => self.json_encode_null_runtime(span, fctx),
            LType::Fn(_) => {
                self.diagnostics.push(Diagnostic::error(
                    "E5036",
                    "JSON encoding of function values is not supported",
                    self.file,
                    span,
                ));
                None
            }
            LType::DynTrait(_) => {
                self.diagnostics.push(Diagnostic::error(
                    "E5036",
                    "JSON encoding of dyn trait values is not supported",
                    self.file,
                    span,
                ));
                None
            }
            LType::Async(_) => {
                self.diagnostics.push(Diagnostic::error(
                    "E5036",
                    "JSON encoding of Async values is not supported",
                    self.file,
                    span,
                ));
                None
            }
            LType::Struct(layout) => self.json_encode_struct(value, layout, span, fctx),
            LType::Enum(layout) => self.json_encode_enum(value, layout, span, fctx),
        }
    }

    pub(super) fn json_encode_struct(
        &mut self,
        value: &Value,
        layout: &StructLayoutType,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let mut object = self.json_object_empty_runtime(span, fctx)?;
        let value_repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let mut ordered = (0..layout.fields.len()).collect::<Vec<_>>();
        ordered.sort_by(|a, b| layout.fields[*a].name.cmp(&layout.fields[*b].name));

        for index in ordered {
            let field = &layout.fields[index];
            let field_reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, {}",
                field_reg,
                llvm_type(&value.ty),
                value_repr,
                index
            ));
            let field_value = Value {
                ty: field.ty.clone(),
                repr: Some(field_reg),
            };
            let encoded_field = self.json_encode_value(&field_value, span, fctx)?;
            let key = self.string_literal(&field.name, fctx);
            let updated = self.json_object_set_runtime(
                &object.value,
                &key,
                &encoded_field.value,
                span,
                fctx,
            )?;
            let err_after_field =
                self.combine_error_codes(&object.err_code, &encoded_field.err_code, fctx);
            let err_after_set = self.combine_error_codes(&err_after_field, &updated.err_code, fctx);
            let ok = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp eq i64 {}, 0", ok, err_after_set));
            let next_repr = self.select_value_repr(
                &ok,
                &object.value.ty,
                &updated
                    .value
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&object.value.ty)),
                &object
                    .value
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&object.value.ty)),
                fctx,
            );
            object = ValueWithErr {
                value: Value {
                    ty: object.value.ty.clone(),
                    repr: Some(next_repr),
                },
                err_code: err_after_set,
            };
        }
        Some(object)
    }

    pub(super) fn json_encode_enum(
        &mut self,
        value: &Value,
        layout: &EnumLayoutType,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let mut object = self.json_object_empty_runtime(span, fctx)?;
        let tag_i32 = self.enum_tag_i32(value, span, fctx)?;
        let tag_i64 = self.new_temp();
        fctx.lines
            .push(format!("  {} = sext i32 {} to i64", tag_i64, tag_i32));
        let tag_json = self.json_encode_int_runtime(
            &Value {
                ty: LType::Int,
                repr: Some(tag_i64),
            },
            span,
            fctx,
        )?;
        let tag_key = self.string_literal("tag", fctx);
        let tagged_object =
            self.json_object_set_runtime(&object.value, &tag_key, &tag_json.value, span, fctx)?;
        let err_after_tag_payload =
            self.combine_error_codes(&object.err_code, &tag_json.err_code, fctx);
        let err_after_tag_set =
            self.combine_error_codes(&err_after_tag_payload, &tagged_object.err_code, fctx);
        let tag_non_negative = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp sge i32 {}, 0",
            tag_non_negative, tag_i32
        ));
        let tag_lt_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp slt i32 {}, {}",
            tag_lt_count,
            tag_i32,
            layout.variants.len() as i32
        ));
        let tag_in_range = self.new_temp();
        fctx.lines.push(format!(
            "  {} = and i1 {}, {}",
            tag_in_range, tag_non_negative, tag_lt_count
        ));
        let tag_range_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 0, i64 2",
            tag_range_err, tag_in_range
        ));
        let err_after_tag = self.combine_error_codes(&err_after_tag_set, &tag_range_err, fctx);
        let tagged_ok = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp eq i64 {}, 0",
            tagged_ok, err_after_tag
        ));
        let tagged_repr = self.select_value_repr(
            &tagged_ok,
            &object.value.ty,
            &tagged_object
                .value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&object.value.ty)),
            &object
                .value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&object.value.ty)),
            fctx,
        );
        object = ValueWithErr {
            value: Value {
                ty: object.value.ty.clone(),
                repr: Some(tagged_repr),
            },
            err_code: err_after_tag,
        };

        let mut payload = self.json_encode_null_runtime(span, fctx)?;
        let enum_repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        for (index, variant) in layout.variants.iter().enumerate() {
            let Some(payload_ty) = variant.payload.clone() else {
                continue;
            };
            let payload_reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, {}",
                payload_reg,
                llvm_type(&value.ty),
                enum_repr,
                index + 1
            ));
            let encoded_variant = self.json_encode_value(
                &Value {
                    ty: payload_ty.clone(),
                    repr: Some(payload_reg),
                },
                span,
                fctx,
            )?;
            let is_variant = self.new_temp();
            fctx.lines.push(format!(
                "  {} = icmp eq i32 {}, {}",
                is_variant, tag_i32, index as i32
            ));
            let next_payload = self.select_value_repr(
                &is_variant,
                &payload.value.ty,
                &encoded_variant
                    .value
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&payload.value.ty)),
                &payload
                    .value
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&payload.value.ty)),
                fctx,
            );
            let active_err = self.new_temp();
            fctx.lines.push(format!(
                "  {} = select i1 {}, i64 {}, i64 0",
                active_err, is_variant, encoded_variant.err_code
            ));
            let payload_err = self.combine_error_codes(&payload.err_code, &active_err, fctx);
            payload = ValueWithErr {
                value: Value {
                    ty: payload.value.ty.clone(),
                    repr: Some(next_payload),
                },
                err_code: payload_err,
            };
        }

        let value_key = self.string_literal("value", fctx);
        let valued_object =
            self.json_object_set_runtime(&object.value, &value_key, &payload.value, span, fctx)?;
        let err_after_payload = self.combine_error_codes(&object.err_code, &payload.err_code, fctx);
        let final_err = self.combine_error_codes(&err_after_payload, &valued_object.err_code, fctx);
        let final_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", final_ok, final_err));
        let final_repr = self.select_value_repr(
            &final_ok,
            &object.value.ty,
            &valued_object
                .value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&object.value.ty)),
            &object
                .value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&object.value.ty)),
            fctx,
        );
        Some(ValueWithErr {
            value: Value {
                ty: object.value.ty,
                repr: Some(final_repr),
            },
            err_code: final_err,
        })
    }

    pub(super) fn json_decode_value(
        &mut self,
        target_ty: &LType,
        json: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        match target_ty {
            LType::Int => self.json_decode_int_runtime(json, span, fctx),
            LType::Float => self.json_decode_float_runtime(json, span, fctx),
            LType::Bool => self.json_decode_bool_runtime(json, span, fctx),
            LType::Char => self.json_decode_char_runtime(json, span, fctx),
            LType::String => self.json_decode_string_runtime(json, span, fctx),
            LType::Unit => {
                let kind_tag = self.json_kind_tag_i32(json, span, fctx)?;
                let null_index = self.json_kind_variant_index(&json.ty, "NullValue", span)? as i32;
                let is_null = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = icmp eq i32 {}, {}",
                    is_null, kind_tag, null_index
                ));
                let err = self.new_temp();
                fctx.lines
                    .push(format!("  {} = select i1 {}, i64 0, i64 2", err, is_null));
                Some(ValueWithErr {
                    value: Value {
                        ty: LType::Unit,
                        repr: None,
                    },
                    err_code: err,
                })
            }
            LType::Fn(_) => {
                self.diagnostics.push(Diagnostic::error(
                    "E5036",
                    "JSON decoding into function values is not supported",
                    self.file,
                    span,
                ));
                None
            }
            LType::DynTrait(_) => {
                self.diagnostics.push(Diagnostic::error(
                    "E5036",
                    "JSON decoding into dyn trait values is not supported",
                    self.file,
                    span,
                ));
                None
            }
            LType::Async(_) => {
                self.diagnostics.push(Diagnostic::error(
                    "E5036",
                    "JSON decoding into Async values is not supported",
                    self.file,
                    span,
                ));
                None
            }
            LType::Struct(layout) => self.json_decode_struct(layout, json, span, fctx),
            LType::Enum(layout) => self.json_decode_enum(layout, json, span, fctx),
        }
    }

    pub(super) fn json_decode_struct(
        &mut self,
        layout: &StructLayoutType,
        json: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let mut err_code = "0".to_string();
        let mut decoded_fields = vec![None; layout.fields.len()];
        let mut ordered = (0..layout.fields.len()).collect::<Vec<_>>();
        ordered.sort_by(|a, b| layout.fields[*a].name.cmp(&layout.fields[*b].name));

        for index in ordered {
            let field = &layout.fields[index];
            let key = self.string_literal(&field.name, fctx);
            let field_json = self.json_object_get_runtime(json, &key, span, fctx)?;
            err_code = self.combine_error_codes(&err_code, &field_json.err_code, fctx);

            let found = self.new_temp();
            fctx.lines
                .push(format!("  {} = icmp ne i64 {}, 0", found, field_json.found));
            let missing_err = self.new_temp();
            fctx.lines.push(format!(
                "  {} = select i1 {}, i64 0, i64 3",
                missing_err, found
            ));
            err_code = self.combine_error_codes(&err_code, &missing_err, fctx);

            let null_json = self.json_encode_null_runtime(span, fctx)?.value;
            let selected_json_repr = self.select_value_repr(
                &found,
                &field_json.value.ty,
                &field_json
                    .value
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&field_json.value.ty)),
                &null_json
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&field_json.value.ty)),
                fctx,
            );
            let selected_json = Value {
                ty: field_json.value.ty.clone(),
                repr: Some(selected_json_repr),
            };
            let decoded = self.json_decode_value(&field.ty, &selected_json, span, fctx)?;
            let active_err = self.new_temp();
            fctx.lines.push(format!(
                "  {} = select i1 {}, i64 {}, i64 0",
                active_err, found, decoded.err_code
            ));
            err_code = self.combine_error_codes(&err_code, &active_err, fctx);

            let decoded_repr = decoded
                .value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&field.ty));
            let selected_repr = self.select_value_repr(
                &found,
                &field.ty,
                &decoded_repr,
                &default_value(&field.ty),
                fctx,
            );
            decoded_fields[index] = Some(Value {
                ty: field.ty.clone(),
                repr: Some(selected_repr),
            });
        }

        let values = decoded_fields
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                value.unwrap_or_else(|| Value {
                    ty: layout.fields[index].ty.clone(),
                    repr: Some(default_value(&layout.fields[index].ty)),
                })
            })
            .collect::<Vec<_>>();
        let decoded_struct = self.build_struct_value(layout, &values, span, fctx)?;
        Some(ValueWithErr {
            value: decoded_struct,
            err_code,
        })
    }

    pub(super) fn json_decode_enum(
        &mut self,
        layout: &EnumLayoutType,
        json: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        if layout.variants.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "enum decoding requires at least one variant",
                self.file,
                span,
            ));
            return None;
        }

        let tag_key = self.string_literal("tag", fctx);
        let tag_json = self.json_object_get_runtime(json, &tag_key, span, fctx)?;
        let value_key = self.string_literal("value", fctx);
        let payload_json = self.json_object_get_runtime(json, &value_key, span, fctx)?;

        let mut err_code = "0".to_string();
        err_code = self.combine_error_codes(&err_code, &tag_json.err_code, fctx);
        err_code = self.combine_error_codes(&err_code, &payload_json.err_code, fctx);

        let tag_found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp ne i64 {}, 0",
            tag_found, tag_json.found
        ));
        let value_found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp ne i64 {}, 0",
            value_found, payload_json.found
        ));
        let missing_tag_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 0, i64 3",
            missing_tag_err, tag_found
        ));
        let missing_value_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 0, i64 3",
            missing_value_err, value_found
        ));
        err_code = self.combine_error_codes(&err_code, &missing_tag_err, fctx);
        err_code = self.combine_error_codes(&err_code, &missing_value_err, fctx);

        let tag_fallback_json = self
            .json_encode_int_runtime(
                &Value {
                    ty: LType::Int,
                    repr: Some("0".to_string()),
                },
                span,
                fctx,
            )?
            .value;
        let selected_tag_json_repr = self.select_value_repr(
            &tag_found,
            &tag_json.value.ty,
            &tag_json
                .value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&tag_json.value.ty)),
            &tag_fallback_json
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&tag_json.value.ty)),
            fctx,
        );
        let selected_tag_json = Value {
            ty: tag_json.value.ty.clone(),
            repr: Some(selected_tag_json_repr),
        };
        let decoded_tag = self.json_decode_int_runtime(&selected_tag_json, span, fctx)?;
        let tag_decode_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 {}, i64 0",
            tag_decode_err, tag_found, decoded_tag.err_code
        ));
        err_code = self.combine_error_codes(&err_code, &tag_decode_err, fctx);
        let tag_value = decoded_tag
            .value
            .repr
            .clone()
            .unwrap_or_else(|| "0".to_string());

        let tag_non_negative = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp sge i64 {}, 0",
            tag_non_negative, tag_value
        ));
        let tag_lt_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp slt i64 {}, {}",
            tag_lt_count,
            tag_value,
            layout.variants.len() as i64
        ));
        let tag_in_range = self.new_temp();
        fctx.lines.push(format!(
            "  {} = and i1 {}, {}",
            tag_in_range, tag_non_negative, tag_lt_count
        ));
        let unknown_tag_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 0, i64 2",
            unknown_tag_err, tag_in_range
        ));
        err_code = self.combine_error_codes(&err_code, &unknown_tag_err, fctx);

        let fields_ready = self.new_temp();
        fctx.lines.push(format!(
            "  {} = and i1 {}, {}",
            fields_ready, tag_found, value_found
        ));

        let payload_fallback_json = self.json_encode_null_runtime(span, fctx)?.value;
        let selected_payload_json_repr = self.select_value_repr(
            &value_found,
            &payload_json.value.ty,
            &payload_json
                .value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&payload_json.value.ty)),
            &payload_fallback_json
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&payload_json.value.ty)),
            fctx,
        );
        let selected_payload_json = Value {
            ty: payload_json.value.ty.clone(),
            repr: Some(selected_payload_json_repr),
        };

        let enum_ty = LType::Enum(layout.clone());
        let initial_payload = layout.variants[0].payload.as_ref().map(|ty| Value {
            ty: ty.clone(),
            repr: Some(default_value(ty)),
        });
        let mut selected = self.build_enum_variant(layout, 0, initial_payload, span, fctx)?;
        for (index, variant) in layout.variants.iter().enumerate() {
            let is_variant = self.new_temp();
            fctx.lines.push(format!(
                "  {} = icmp eq i64 {}, {}",
                is_variant, tag_value, index as i64
            ));
            let active_variant = self.new_temp();
            fctx.lines.push(format!(
                "  {} = and i1 {}, {}",
                active_variant, fields_ready, is_variant
            ));

            let (candidate, candidate_err) = if let Some(payload_ty) = variant.payload.clone() {
                let decoded_payload =
                    self.json_decode_value(&payload_ty, &selected_payload_json, span, fctx)?;
                let enum_value = self.build_enum_variant(
                    layout,
                    index,
                    Some(decoded_payload.value.clone()),
                    span,
                    fctx,
                )?;
                (enum_value, decoded_payload.err_code)
            } else {
                let payload_kind = self.json_kind_tag_i32(&selected_payload_json, span, fctx)?;
                let null_index =
                    self.json_kind_variant_index(&selected_payload_json.ty, "NullValue", span)?
                        as i32;
                let is_null = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = icmp eq i32 {}, {}",
                    is_null, payload_kind, null_index
                ));
                let null_err = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = select i1 {}, i64 0, i64 2",
                    null_err, is_null
                ));
                let enum_value = self.build_enum_variant(layout, index, None, span, fctx)?;
                (enum_value, null_err)
            };
            let active_err = self.new_temp();
            fctx.lines.push(format!(
                "  {} = select i1 {}, i64 {}, i64 0",
                active_err, active_variant, candidate_err
            ));
            err_code = self.combine_error_codes(&err_code, &active_err, fctx);

            let selected_repr = self.select_value_repr(
                &active_variant,
                &enum_ty,
                &candidate
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&enum_ty)),
                &selected
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&enum_ty)),
                fctx,
            );
            selected = Value {
                ty: enum_ty.clone(),
                repr: Some(selected_repr),
            };
        }

        Some(ValueWithErr {
            value: selected,
            err_code,
        })
    }

    pub(super) fn json_schema_for_type(
        &mut self,
        ty: &LType,
        stack: &mut Vec<String>,
        span: crate::span::Span,
    ) -> Option<String> {
        match ty {
            LType::Int => Some("{\"kind\":\"int\"}".to_string()),
            LType::Float => Some("{\"kind\":\"float\"}".to_string()),
            LType::Bool => Some("{\"kind\":\"bool\"}".to_string()),
            LType::Char => Some("{\"kind\":\"char\"}".to_string()),
            LType::String => Some("{\"kind\":\"string\"}".to_string()),
            LType::Unit => Some("{\"kind\":\"unit\"}".to_string()),
            LType::Fn(layout) => Some(format!(
                "{{\"kind\":\"function\",\"name\":\"{}\"}}",
                json_escape_string(&layout.repr)
            )),
            LType::DynTrait(trait_name) => Some(format!(
                "{{\"kind\":\"dyn_trait\",\"name\":\"{}\"}}",
                json_escape_string(trait_name)
            )),
            LType::Async(inner) => {
                let inner_schema = self.json_schema_for_type(inner, stack, span)?;
                Some(format!("{{\"kind\":\"async\",\"output\":{inner_schema}}}"))
            }
            LType::Struct(layout) => {
                if stack.iter().any(|name| name == &layout.repr) {
                    return Some(format!(
                        "{{\"kind\":\"ref\",\"name\":\"{}\"}}",
                        json_escape_string(&layout.repr)
                    ));
                }
                stack.push(layout.repr.clone());
                let mut ordered = (0..layout.fields.len()).collect::<Vec<_>>();
                ordered.sort_by(|a, b| layout.fields[*a].name.cmp(&layout.fields[*b].name));
                let mut fields = Vec::new();
                for index in ordered {
                    let field = &layout.fields[index];
                    let field_schema = self.json_schema_for_type(&field.ty, stack, span)?;
                    fields.push(format!(
                        "{{\"name\":\"{}\",\"type\":{}}}",
                        json_escape_string(&field.name),
                        field_schema
                    ));
                }
                stack.pop();
                Some(format!(
                    "{{\"kind\":\"struct\",\"name\":\"{}\",\"fields\":[{}]}}",
                    json_escape_string(&layout.repr),
                    fields.join(",")
                ))
            }
            LType::Enum(layout) => {
                if stack.iter().any(|name| name == &layout.repr) {
                    return Some(format!(
                        "{{\"kind\":\"ref\",\"name\":\"{}\"}}",
                        json_escape_string(&layout.repr)
                    ));
                }
                if layout.variants.is_empty() {
                    self.diagnostics.push(Diagnostic::error(
                        "E5011",
                        "schema generation requires enums with at least one variant",
                        self.file,
                        span,
                    ));
                    return None;
                }
                stack.push(layout.repr.clone());
                let mut variants = Vec::new();
                for (index, variant) in layout.variants.iter().enumerate() {
                    let payload = if let Some(payload_ty) = variant.payload.clone() {
                        self.json_schema_for_type(&payload_ty, stack, span)?
                    } else {
                        "null".to_string()
                    };
                    variants.push(format!(
                        "{{\"name\":\"{}\",\"tag\":{},\"payload\":{}}}",
                        json_escape_string(&variant.name),
                        index,
                        payload
                    ));
                }
                stack.pop();
                Some(format!(
                    "{{\"kind\":\"enum\",\"name\":\"{}\",\"tag_encoding\":\"indexed\",\"variants\":[{}]}}",
                    json_escape_string(&layout.repr),
                    variants.join(",")
                ))
            }
        }
    }

    pub(super) fn json_value_type(&mut self, span: crate::span::Span) -> Option<LType> {
        let Some(ty) = self.parse_type_repr("JsonValue", span) else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "JsonValue type is unavailable for serde json lowering",
                self.file,
                span,
            ));
            return None;
        };
        Some(ty)
    }

    pub(super) fn json_encode_char_runtime(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        if value.ty != LType::Char {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "json encode expects Char input",
                self.file,
                span,
            ));
            return None;
        }
        let char_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = sext i32 {} to i64",
            char_i64,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let int_value = Value {
            ty: LType::Int,
            repr: Some(char_i64),
        };
        self.json_encode_int_runtime(&int_value, span, fctx)
    }

    pub(super) fn json_encode_int_runtime(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        if value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "json encode expects Int input",
                self.file,
                span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_encode_int(i64 {}, i8** {}, i64* {})",
            err_code,
            value.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let json_ty = self.json_value_type(span)?;
        let raw = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind = self.build_json_kind_from_code(&kind_ty, "2", span, fctx)?;
        let json = self.build_json_value_struct(&json_ty, raw, kind, span, fctx)?;
        Some(ValueWithErr {
            value: json,
            err_code,
        })
    }

    pub(super) fn json_encode_float_runtime(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        if value.ty != LType::Float {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "json encode expects Float input",
                self.file,
                span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_encode_float(double {}, i8** {}, i64* {})",
            err_code,
            value
                .repr
                .clone()
                .unwrap_or_else(|| llvm_float_literal(0.0_f64)),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let json_ty = self.json_value_type(span)?;
        let raw = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind = self.build_json_kind_from_code(&kind_ty, "2", span, fctx)?;
        let json = self.build_json_value_struct(&json_ty, raw, kind, span, fctx)?;
        Some(ValueWithErr {
            value: json,
            err_code,
        })
    }

    pub(super) fn json_encode_bool_runtime(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        if value.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "json encode expects Bool input",
                self.file,
                span,
            ));
            return None;
        }
        let bool_int = self.new_temp();
        fctx.lines.push(format!(
            "  {} = zext i1 {} to i64",
            bool_int,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_encode_bool(i64 {}, i8** {}, i64* {})",
            err_code, bool_int, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let json_ty = self.json_value_type(span)?;
        let raw = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind = self.build_json_kind_from_code(&kind_ty, "1", span, fctx)?;
        let json = self.build_json_value_struct(&json_ty, raw, kind, span, fctx)?;
        Some(ValueWithErr {
            value: json,
            err_code,
        })
    }

    pub(super) fn json_encode_string_runtime(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "json encode expects String input",
                self.file,
                span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(value, span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_encode_string(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err_code, ptr, len, cap, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let json_ty = self.json_value_type(span)?;
        let raw = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind = self.build_json_kind_from_code(&kind_ty, "3", span, fctx)?;
        let json = self.build_json_value_struct(&json_ty, raw, kind, span, fctx)?;
        Some(ValueWithErr {
            value: json,
            err_code,
        })
    }

    pub(super) fn json_encode_null_runtime(
        &mut self,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_encode_null(i8** {}, i64* {})",
            err_code, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let json_ty = self.json_value_type(span)?;
        let raw = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind = self.build_json_kind_from_code(&kind_ty, "0", span, fctx)?;
        let json = self.build_json_value_struct(&json_ty, raw, kind, span, fctx)?;
        Some(ValueWithErr {
            value: json,
            err_code,
        })
    }

    pub(super) fn json_object_empty_runtime(
        &mut self,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_object_empty(i8** {}, i64* {})",
            err_code, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let json_ty = self.json_value_type(span)?;
        let raw = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind = self.build_json_kind_from_code(&kind_ty, "5", span, fctx)?;
        let json = self.build_json_value_struct(&json_ty, raw, kind, span, fctx)?;
        Some(ValueWithErr {
            value: json,
            err_code,
        })
    }

    pub(super) fn json_object_set_runtime(
        &mut self,
        object: &Value,
        key: &Value,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let (obj_ptr, obj_len, obj_cap) = self.json_raw_parts(object, span, fctx)?;
        let (key_ptr, key_len, key_cap) = self.string_parts(key, span, fctx)?;
        let (value_ptr, value_len, value_cap) = self.json_raw_parts(value, span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let out_kind_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_kind_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_object_set(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {}, i64* {})",
            err_code,
            obj_ptr,
            obj_len,
            obj_cap,
            key_ptr,
            key_len,
            key_cap,
            value_ptr,
            value_len,
            value_cap,
            out_ptr_slot,
            out_len_slot,
            out_kind_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let out_kind = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_kind, out_kind_slot));

        let raw = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let kind_ty = self.json_value_layout(&object.ty, span)?.3;
        let kind = self.build_json_kind_from_code(&kind_ty, &out_kind, span, fctx)?;
        let json = self.build_json_value_struct(&object.ty, raw, kind, span, fctx)?;
        Some(ValueWithErr {
            value: json,
            err_code,
        })
    }

    pub(super) fn json_object_get_runtime(
        &mut self,
        object: &Value,
        key: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<JsonObjectGetValue> {
        let (obj_ptr, obj_len, obj_cap) = self.json_raw_parts(object, span, fctx)?;
        let (key_ptr, key_len, key_cap) = self.string_parts(key, span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let out_kind_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_kind_slot));
        let found_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", found_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_object_get(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {}, i64* {}, i64* {})",
            err_code,
            obj_ptr,
            obj_len,
            obj_cap,
            key_ptr,
            key_len,
            key_cap,
            out_ptr_slot,
            out_len_slot,
            out_kind_slot,
            found_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let out_kind = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_kind, out_kind_slot));
        let found = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", found, found_slot));
        let raw = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let json_ty = self.json_value_type(span)?;
        let kind_ty = self.json_value_layout(&json_ty, span)?.3;
        let kind = self.build_json_kind_from_code(&kind_ty, &out_kind, span, fctx)?;
        let json = self.build_json_value_struct(&json_ty, raw, kind, span, fctx)?;
        Some(JsonObjectGetValue {
            value: json,
            found,
            err_code,
        })
    }

    pub(super) fn json_decode_int_runtime(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let (raw_ptr, raw_len, raw_cap) = self.json_raw_parts(value, span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_decode_int(i8* {}, i64 {}, i64 {}, i64* {})",
            err_code, raw_ptr, raw_len, raw_cap, out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        Some(ValueWithErr {
            value: Value {
                ty: LType::Int,
                repr: Some(out),
            },
            err_code,
        })
    }

    pub(super) fn json_decode_char_runtime(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let decoded_int = self.json_decode_int_runtime(value, span, fctx)?;
        let int_repr = decoded_int
            .value
            .repr
            .clone()
            .unwrap_or_else(|| "0".to_string());

        let non_negative = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp sge i64 {}, 0", non_negative, int_repr));
        let within_max = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp sle i64 {}, 1114111",
            within_max, int_repr
        ));
        let in_scalar_range = self.new_temp();
        fctx.lines.push(format!(
            "  {} = and i1 {}, {}",
            in_scalar_range, non_negative, within_max
        ));

        let below_surrogate = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp slt i64 {}, 55296",
            below_surrogate, int_repr
        ));
        let above_surrogate = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp sgt i64 {}, 57343",
            above_surrogate, int_repr
        ));
        let outside_surrogate = self.new_temp();
        fctx.lines.push(format!(
            "  {} = or i1 {}, {}",
            outside_surrogate, below_surrogate, above_surrogate
        ));

        let valid = self.new_temp();
        fctx.lines.push(format!(
            "  {} = and i1 {}, {}",
            valid, in_scalar_range, outside_surrogate
        ));
        let char_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 0, i64 2",
            char_err, valid
        ));
        let err_code = self.combine_error_codes(&decoded_int.err_code, &char_err, fctx);

        let char_i32 = self.new_temp();
        fctx.lines
            .push(format!("  {} = trunc i64 {} to i32", char_i32, int_repr));
        Some(ValueWithErr {
            value: Value {
                ty: LType::Char,
                repr: Some(char_i32),
            },
            err_code,
        })
    }

    pub(super) fn json_decode_float_runtime(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let (raw_ptr, raw_len, raw_cap) = self.json_raw_parts(value, span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca double", out_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_decode_float(i8* {}, i64 {}, i64 {}, double* {})",
            err_code, raw_ptr, raw_len, raw_cap, out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load double, double* {}", out, out_slot));
        Some(ValueWithErr {
            value: Value {
                ty: LType::Float,
                repr: Some(out),
            },
            err_code,
        })
    }

    pub(super) fn json_decode_bool_runtime(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let (raw_ptr, raw_len, raw_cap) = self.json_raw_parts(value, span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_decode_bool(i8* {}, i64 {}, i64 {}, i64* {})",
            err_code, raw_ptr, raw_len, raw_cap, out_slot
        ));
        let out_int = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_int, out_slot));
        let out_bool = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", out_bool, out_int));
        Some(ValueWithErr {
            value: Value {
                ty: LType::Bool,
                repr: Some(out_bool),
            },
            err_code,
        })
    }

    pub(super) fn json_decode_string_runtime(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<ValueWithErr> {
        let (raw_ptr, raw_len, raw_cap) = self.json_raw_parts(value, span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err_code = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_json_decode_string(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err_code, raw_ptr, raw_len, raw_cap, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let decoded = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        Some(ValueWithErr {
            value: decoded,
            err_code,
        })
    }

    pub(super) fn json_kind_tag_i32(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<String> {
        let (layout, _, kind_index, kind_ty) = self.json_value_layout(&value.ty, span)?;
        let value_repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let kind_reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            kind_reg,
            llvm_type(&LType::Struct(layout)),
            value_repr,
            kind_index
        ));
        self.enum_tag_i32(
            &Value {
                ty: kind_ty,
                repr: Some(kind_reg),
            },
            span,
            fctx,
        )
    }

    pub(super) fn enum_tag_i32(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<String> {
        let LType::Enum(_) = value.ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "expected enum value while extracting tag",
                self.file,
                span,
            ));
            return None;
        };
        let value_repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let tag = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            tag,
            llvm_type(&value.ty),
            value_repr
        ));
        Some(tag)
    }

    pub(super) fn json_kind_variant_index(
        &mut self,
        json_ty: &LType,
        variant_name: &str,
        span: crate::span::Span,
    ) -> Option<usize> {
        let (_, _, _, kind_ty) = self.json_value_layout(json_ty, span)?;
        let LType::Enum(layout) = kind_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "JsonValue.kind must be enum",
                self.file,
                span,
            ));
            return None;
        };
        let Some(index) = layout
            .variants
            .iter()
            .position(|variant| variant.name == variant_name)
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("JsonKind is missing {} variant", variant_name),
                self.file,
                span,
            ));
            return None;
        };
        Some(index)
    }

    pub(super) fn combine_error_codes(
        &mut self,
        left: &str,
        right: &str,
        fctx: &mut FnCtx,
    ) -> String {
        let left_is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", left_is_ok, left));
        let merged = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, i64 {}, i64 {}",
            merged, left_is_ok, right, left
        ));
        merged
    }

    pub(super) fn select_value_repr(
        &mut self,
        cond: &str,
        ty: &LType,
        when_true: &str,
        when_false: &str,
        fctx: &mut FnCtx,
    ) -> String {
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = select i1 {}, {} {}, {} {}",
            reg,
            cond,
            llvm_type(ty),
            when_true,
            llvm_type(ty),
            when_false
        ));
        reg
    }

    pub(super) fn gen_regex_builtin_call(
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
            "captures" | "aic_regex_captures_intrinsic" => "captures",
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
            "captures"
                if self.sig_matches_shape(
                    name,
                    &["Regex", "String"],
                    "Result[Option[RegexMatch], RegexError]",
                ) =>
            {
                Some(self.gen_regex_captures_call(name, args, span, fctx))
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

    pub(super) fn gen_regex_compile_call(
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

    pub(super) fn gen_regex_is_match_call(
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

    pub(super) fn gen_regex_find_call(
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

    pub(super) fn gen_regex_replace_call(
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

    pub(super) fn gen_regex_captures_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "captures expects two arguments",
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
                "captures expects Regex and String",
                self.file,
                span,
            ));
            return None;
        }
        let (pattern_ptr, pattern_len, pattern_cap, flags_repr) =
            self.regex_parts(&regex, args[0].span, fctx)?;
        let (text_ptr, text_len, text_cap) = self.string_parts(&text, args[1].span, fctx)?;

        let out_full_ptr_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_full_ptr_slot));
        let out_full_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_full_len_slot));
        let out_groups_ptr_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_groups_ptr_slot));
        let out_groups_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_groups_count_slot));
        let out_start_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_start_slot));
        let out_end_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_end_slot));
        let out_found_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_found_slot));

        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_regex_captures(i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {}, i8** {}, i64* {}, i64* {}, i64* {}, i64* {})",
            err,
            pattern_ptr,
            pattern_len,
            pattern_cap,
            flags_repr,
            text_ptr,
            text_len,
            text_cap,
            out_full_ptr_slot,
            out_full_len_slot,
            out_groups_ptr_slot,
            out_groups_count_slot,
            out_start_slot,
            out_end_slot,
            out_found_slot
        ));

        let out_full_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_full_ptr, out_full_ptr_slot
        ));
        let out_full_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_full_len, out_full_len_slot
        ));
        let out_groups_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_groups_ptr, out_groups_ptr_slot
        ));
        let out_groups_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_groups_count, out_groups_count_slot
        ));
        let out_start = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_start, out_start_slot
        ));
        let out_end = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_end, out_end_slot));
        let out_found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_found, out_found_slot
        ));
        let found_bool = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", found_bool, out_found));

        let full_value = self.build_string_value(&out_full_ptr, &out_full_len, &out_full_len, fctx);
        let groups_value =
            self.build_vec_string_payload_from_ptr(&out_groups_ptr, &out_groups_count, span, fctx)?;

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
        let Some((_, match_ty, _, _)) = self.option_layout_parts(&ok_ty, span) else {
            return None;
        };
        let match_value = self.build_http_struct_value(
            &match_ty,
            "RegexMatch",
            &[
                ("full", full_value),
                ("groups", groups_value),
                (
                    "start",
                    Value {
                        ty: LType::Int,
                        repr: Some(out_start),
                    },
                ),
                (
                    "end",
                    Value {
                        ty: LType::Int,
                        repr: Some(out_end),
                    },
                ),
            ],
            span,
            fctx,
        )?;
        let ok_payload =
            self.wrap_option_with_condition(&ok_ty, match_value, &found_bool, span, fctx)?;
        self.wrap_regex_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn regex_parts(
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

    pub(super) fn wrap_regex_result(
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

    pub(super) fn result_layout_parts(
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

    pub(super) fn build_enum_variant(
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
                let slot_ty_for_payload = if *payload_ty == LType::Unit {
                    "i8".to_string()
                } else {
                    llvm_type(payload_ty)
                };
                let slot_default_for_payload = if *payload_ty == LType::Unit {
                    "0".to_string()
                } else {
                    default_value(payload_ty)
                };
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
                            (slot_ty_for_payload, slot_default_for_payload)
                        } else {
                            let slot_repr = if *payload_ty == LType::Unit {
                                "0".to_string()
                            } else {
                                payload_value
                                    .repr
                                    .clone()
                                    .unwrap_or_else(|| default_value(payload_ty))
                            };
                            (slot_ty_for_payload, slot_repr)
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5011",
                            "enum variant expects payload",
                            self.file,
                            span,
                        ));
                        (slot_ty_for_payload, slot_default_for_payload)
                    }
                } else {
                    (slot_ty_for_payload, slot_default_for_payload)
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

    pub(super) fn build_struct_value(
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

    pub(super) fn build_string_value(
        &mut self,
        ptr: &str,
        len: &str,
        cap: &str,
        fctx: &mut FnCtx,
    ) -> Value {
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

    pub(super) fn build_error_from_code(
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

    pub(super) fn build_io_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "IoError",
            "io",
            &[(1, "EndOfInput"), (2, "InvalidInput"), (3, "Io")],
            "Io",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_fs_error_from_code(
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

    pub(super) fn build_env_error_from_code(
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

    pub(super) fn build_time_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "TimeError",
            "time",
            &[
                (1, "InvalidFormat"),
                (2, "InvalidDate"),
                (3, "InvalidTime"),
                (4, "InvalidOffset"),
                (5, "InvalidInput"),
                (6, "Internal"),
            ],
            "Internal",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_proc_error_from_code(
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

    pub(super) fn build_signal_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "SignalError",
            "signal",
            &[
                (1, "UnsupportedPlatform"),
                (2, "InvalidSignal"),
                (3, "PermissionDenied"),
                (4, "Internal"),
            ],
            "Internal",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_net_error_from_code(
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

    pub(super) fn build_tls_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "TlsError",
            "tls",
            &[
                (1, "HandshakeFailed"),
                (2, "CertificateInvalid"),
                (3, "CertificateExpired"),
                (4, "HostnameMismatch"),
                (5, "ProtocolError"),
                (6, "ConnectionClosed"),
                (7, "Io"),
            ],
            "Io",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_buffer_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "BufferError",
            "buffer",
            &[
                (1, "Underflow"),
                (2, "Overflow"),
                (3, "InvalidUtf8"),
                (4, "InvalidInput"),
            ],
            "InvalidInput",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_crypto_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "CryptoError",
            "crypto",
            &[
                (1, "InvalidInput"),
                (2, "UnsupportedAlgorithm"),
                (3, "Internal"),
            ],
            "Internal",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_url_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "UrlError",
            "url",
            &[
                (1, "InvalidUrl"),
                (2, "InvalidScheme"),
                (3, "InvalidHost"),
                (4, "InvalidPort"),
                (5, "InvalidPath"),
                (6, "InvalidInput"),
                (7, "Internal"),
            ],
            "Internal",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_http_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "HttpError",
            "http",
            &[
                (1, "InvalidMethod"),
                (2, "InvalidStatus"),
                (3, "InvalidHeaderName"),
                (4, "InvalidHeaderValue"),
                (5, "InvalidTarget"),
                (6, "InvalidInput"),
                (7, "Internal"),
            ],
            "Internal",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_http_server_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "ServerError",
            "http_server",
            &[
                (1, "InvalidRequest"),
                (2, "InvalidMethod"),
                (3, "InvalidHeader"),
                (4, "InvalidTarget"),
                (5, "Timeout"),
                (6, "ConnectionClosed"),
                (7, "BodyTooLarge"),
                (8, "Net"),
                (9, "Internal"),
            ],
            "Internal",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_router_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "RouterError",
            "router",
            &[
                (1, "InvalidPattern"),
                (2, "InvalidMethod"),
                (3, "Capacity"),
                (4, "Internal"),
            ],
            "Internal",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_regex_error_from_code(
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

    pub(super) fn build_concurrency_error_from_code(
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

    pub(super) fn build_channel_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "ChannelError",
            "channel",
            &[(2, "Timeout"), (6, "Closed"), (8, "Full"), (9, "Empty")],
            "Closed",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_json_error_from_code(
        &mut self,
        err_ty: &LType,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            err_ty,
            "JsonError",
            "json",
            &[
                (1, "InvalidJson"),
                (2, "InvalidType"),
                (3, "MissingField"),
                (4, "InvalidNumber"),
                (5, "InvalidString"),
                (6, "InvalidInput"),
                (7, "Internal"),
            ],
            "Internal",
            err_code,
            span,
            fctx,
        )
    }

    pub(super) fn build_json_kind_from_code(
        &mut self,
        kind_ty: &LType,
        kind_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.build_error_from_code(
            kind_ty,
            "JsonKind",
            "json kind",
            &[
                (0, "NullValue"),
                (1, "BoolValue"),
                (2, "NumberValue"),
                (3, "StringValue"),
                (4, "ArrayValue"),
                (5, "ObjectValue"),
            ],
            "NullValue",
            kind_code,
            span,
            fctx,
        )
    }
}
