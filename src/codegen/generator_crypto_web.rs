use super::*;

impl<'a> Generator<'a> {
    pub(super) fn gen_crypto_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "md5" | "aic_crypto_md5_intrinsic" => "md5",
            "md5_bytes" => "md5_bytes",
            "sha256" | "aic_crypto_sha256_intrinsic" => "sha256",
            "sha256_raw" | "aic_crypto_sha256_raw_intrinsic" => "sha256_raw",
            "hmac_sha256" | "aic_crypto_hmac_sha256_intrinsic" => "hmac_sha256",
            "hmac_sha256_raw" | "aic_crypto_hmac_sha256_raw_intrinsic" => "hmac_sha256_raw",
            "pbkdf2_sha256" | "aic_crypto_pbkdf2_sha256_intrinsic" => "pbkdf2_sha256",
            "hex_encode" | "aic_crypto_hex_encode_intrinsic" => "hex_encode",
            "hex_decode" | "aic_crypto_hex_decode_intrinsic" => "hex_decode",
            "base64_encode" | "aic_crypto_base64_encode_intrinsic" => "base64_encode",
            "base64_decode" | "aic_crypto_base64_decode_intrinsic" => "base64_decode",
            "random_bytes" | "aic_crypto_random_bytes_intrinsic" => "random_bytes",
            "secure_eq" | "aic_crypto_secure_eq_intrinsic" => "secure_eq",
            _ => return None,
        };

        match canonical {
            "md5" if self.sig_matches_shape(name, &["String"], "String") => Some(
                self.gen_crypto_unary_data_call(name, args, "aic_rt_crypto_md5", "md5", span, fctx),
            ),
            "md5_bytes" if self.sig_matches_shape(name, &["Bytes"], "String") => {
                Some(self.gen_crypto_unary_data_call(
                    name,
                    args,
                    "aic_rt_crypto_md5",
                    "md5_bytes",
                    span,
                    fctx,
                ))
            }
            "sha256" if self.sig_matches_shape(name, &["String"], "String") => {
                Some(self.gen_crypto_unary_data_call(
                    name,
                    args,
                    "aic_rt_crypto_sha256",
                    "sha256",
                    span,
                    fctx,
                ))
            }
            "sha256_raw"
                if self.sig_matches_shape(name, &["String"], "Bytes")
                    || self.sig_matches_shape(name, &["String"], "String") =>
            {
                Some(self.gen_crypto_unary_data_call(
                    name,
                    args,
                    "aic_rt_crypto_sha256_raw",
                    "sha256_raw",
                    span,
                    fctx,
                ))
            }
            "hmac_sha256" if self.sig_matches_shape(name, &["String", "String"], "String") => {
                Some(self.gen_crypto_binary_data_call(
                    name,
                    args,
                    "aic_rt_crypto_hmac_sha256",
                    "hmac_sha256",
                    span,
                    fctx,
                ))
            }
            "hmac_sha256_raw"
                if self.sig_matches_shape(name, &["Bytes", "Bytes"], "Bytes")
                    || self.sig_matches_shape(name, &["String", "String"], "String") =>
            {
                Some(self.gen_crypto_binary_data_call(
                    name,
                    args,
                    "aic_rt_crypto_hmac_sha256_raw",
                    "hmac_sha256_raw",
                    span,
                    fctx,
                ))
            }
            "pbkdf2_sha256"
                if self.sig_matches_shape(
                    name,
                    &["String", "Bytes", "Int", "Int"],
                    "Result[Bytes, CryptoError]",
                ) || self.sig_matches_shape(
                    name,
                    &["String", "String", "Int", "Int"],
                    "Result[String, CryptoError]",
                ) =>
            {
                Some(self.gen_crypto_pbkdf2_call(name, args, span, fctx))
            }
            "hex_encode"
                if self.sig_matches_shape(name, &["Bytes"], "String")
                    || self.sig_matches_shape(name, &["String"], "String") =>
            {
                Some(self.gen_crypto_unary_data_call(
                    name,
                    args,
                    "aic_rt_crypto_hex_encode",
                    "hex_encode",
                    span,
                    fctx,
                ))
            }
            "hex_decode"
                if self.sig_matches_shape(name, &["String"], "Result[Bytes, CryptoError]")
                    || self.sig_matches_shape(name, &["String"], "Result[String, CryptoError]") =>
            {
                Some(self.gen_crypto_decode_call(
                    name,
                    args,
                    "aic_rt_crypto_hex_decode",
                    "hex_decode",
                    span,
                    fctx,
                ))
            }
            "base64_encode"
                if self.sig_matches_shape(name, &["Bytes"], "String")
                    || self.sig_matches_shape(name, &["String"], "String") =>
            {
                Some(self.gen_crypto_unary_data_call(
                    name,
                    args,
                    "aic_rt_crypto_base64_encode",
                    "base64_encode",
                    span,
                    fctx,
                ))
            }
            "base64_decode"
                if self.sig_matches_shape(name, &["String"], "Result[Bytes, CryptoError]")
                    || self.sig_matches_shape(name, &["String"], "Result[String, CryptoError]") =>
            {
                Some(self.gen_crypto_decode_call(
                    name,
                    args,
                    "aic_rt_crypto_base64_decode",
                    "base64_decode",
                    span,
                    fctx,
                ))
            }
            "random_bytes"
                if self.sig_matches_shape(name, &["Int"], "Bytes")
                    || self.sig_matches_shape(name, &["Int"], "String") =>
            {
                Some(self.gen_crypto_random_bytes_call(name, args, span, fctx))
            }
            "secure_eq"
                if self.sig_matches_shape(name, &["Bytes", "Bytes"], "Bool")
                    || self.sig_matches_shape(name, &["String", "String"], "Bool") =>
            {
                Some(self.gen_crypto_secure_eq_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn crypto_data_parts(
        &mut self,
        value: &Value,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String)> {
        if value.ty == LType::String {
            self.string_parts(value, span, fctx)
        } else {
            self.bytes_parts(value, context, span, fctx)
        }
    }

    pub(super) fn crypto_data_output_from_slots(
        &mut self,
        output_ty: &LType,
        out_ptr_slot: &str,
        out_len_slot: &str,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let raw = self.load_string_from_out_slots(out_ptr_slot, out_len_slot, fctx)?;
        if *output_ty == LType::String {
            Some(raw)
        } else {
            self.build_bytes_value_from_data(output_ty, raw, context, span, fctx)
        }
    }

    pub(super) fn gen_crypto_unary_data_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        runtime_fn: &str,
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
        let data = self.gen_expr(&args[0], fctx)?;
        let (ptr, len, cap) = self.crypto_data_parts(&data, context, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @{}(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            runtime_fn, ptr, len, cap, out_ptr_slot, out_len_slot
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
        self.crypto_data_output_from_slots(
            &result_ty,
            &out_ptr_slot,
            &out_len_slot,
            context,
            span,
            fctx,
        )
    }

    pub(super) fn gen_crypto_binary_data_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        runtime_fn: &str,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context} expects two arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let left = self.gen_expr(&args[0], fctx)?;
        let right = self.gen_expr(&args[1], fctx)?;
        let (lptr, llen, lcap) = self.crypto_data_parts(&left, context, args[0].span, fctx)?;
        let (rptr, rlen, rcap) = self.crypto_data_parts(&right, context, args[1].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @{}(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            runtime_fn, lptr, llen, lcap, rptr, rlen, rcap, out_ptr_slot, out_len_slot
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
        self.crypto_data_output_from_slots(
            &result_ty,
            &out_ptr_slot,
            &out_len_slot,
            context,
            span,
            fctx,
        )
    }

    pub(super) fn gen_crypto_pbkdf2_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 4 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "pbkdf2_sha256 expects four arguments",
                self.file,
                span,
            ));
            return None;
        }
        let password = self.gen_expr(&args[0], fctx)?;
        if password.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "pbkdf2_sha256 expects first argument as String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let salt = self.gen_expr(&args[1], fctx)?;
        let iterations = self.gen_expr(&args[2], fctx)?;
        let key_length = self.gen_expr(&args[3], fctx)?;
        if iterations.ty != LType::Int || key_length.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "pbkdf2_sha256 expects (String, Bytes/String, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (pwd_ptr, pwd_len, pwd_cap) = self.string_parts(&password, args[0].span, fctx)?;
        let (salt_ptr, salt_len, salt_cap) =
            self.crypto_data_parts(&salt, "pbkdf2_sha256", args[1].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_crypto_pbkdf2_sha256(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            pwd_ptr,
            pwd_len,
            pwd_cap,
            salt_ptr,
            salt_len,
            salt_cap,
            iterations.repr.clone().unwrap_or_else(|| "0".to_string()),
            key_length.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));

        let raw = self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
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
        let ok_payload = if ok_ty == LType::String {
            raw
        } else {
            self.build_bytes_value_from_data(&ok_ty, raw, "pbkdf2_sha256", span, fctx)?
        };
        self.wrap_crypto_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_crypto_decode_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        runtime_fn: &str,
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
        let encoded = self.gen_expr(&args[0], fctx)?;
        if encoded.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&encoded, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err, runtime_fn, ptr, len, cap, out_ptr_slot, out_len_slot
        ));
        let raw = self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
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
        let ok_payload = if ok_ty == LType::String {
            raw
        } else {
            self.build_bytes_value_from_data(&ok_ty, raw, context, span, fctx)?
        };
        self.wrap_crypto_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_crypto_random_bytes_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "random_bytes expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let count = self.gen_expr(&args[0], fctx)?;
        if count.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "random_bytes expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_crypto_random_bytes(i64 {}, i8** {}, i64* {})",
            count.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
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
        self.crypto_data_output_from_slots(
            &result_ty,
            &out_ptr_slot,
            &out_len_slot,
            "random_bytes",
            span,
            fctx,
        )
    }

    pub(super) fn gen_crypto_secure_eq_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "secure_eq expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let left = self.gen_expr(&args[0], fctx)?;
        let right = self.gen_expr(&args[1], fctx)?;
        let (lptr, llen, lcap) = self.crypto_data_parts(&left, "secure_eq", args[0].span, fctx)?;
        let (rptr, rlen, rcap) = self.crypto_data_parts(&right, "secure_eq", args[1].span, fctx)?;
        let raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_crypto_secure_eq(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
            raw, lptr, llen, lcap, rptr, rlen, rcap
        ));
        let bool_reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", bool_reg, raw));
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        if result_ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "secure_eq expects Bool return type",
                self.file,
                span,
            ));
            return None;
        }
        Some(Value {
            ty: LType::Bool,
            repr: Some(bool_reg),
        })
    }

    pub(super) fn wrap_crypto_result(
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
                    "crypto builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_crypto_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("crypto_ok");
        let err_label = self.new_label("crypto_err");
        let cont_label = self.new_label("crypto_cont");
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

    pub(super) fn gen_url_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "parse" | "aic_url_parse_intrinsic" => "parse",
            "normalize" | "aic_url_normalize_intrinsic" => "normalize",
            "net_addr" | "aic_url_net_addr_intrinsic" => "net_addr",
            _ => return None,
        };

        match canonical {
            "parse" if self.sig_matches_shape(name, &["String"], "Result[Url, UrlError]") => {
                Some(self.gen_url_parse_call(name, args, span, fctx))
            }
            "normalize" if self.sig_matches_shape(name, &["Url"], "Result[String, UrlError]") => {
                Some(self.gen_url_normalize_call(name, args, span, fctx))
            }
            "net_addr" if self.sig_matches_shape(name, &["Url"], "Result[String, UrlError]") => {
                Some(self.gen_url_net_addr_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_http_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "parse_method" | "aic_http_parse_method_intrinsic" => "parse_method",
            "method_name" | "aic_http_method_name_intrinsic" => "method_name",
            "status_reason" | "aic_http_status_reason_intrinsic" => "status_reason",
            "validate_header" | "aic_http_validate_header_intrinsic" => "validate_header",
            "validate_target" | "aic_http_validate_target_intrinsic" => "validate_target",
            "header" | "aic_http_header_intrinsic" => "header",
            "request" | "aic_http_request_intrinsic" => "request",
            "response" | "aic_http_response_intrinsic" => "response",
            _ => return None,
        };

        match canonical {
            "parse_method"
                if self.sig_matches_shape(name, &["String"], "Result[HttpMethod, HttpError]") =>
            {
                Some(self.gen_http_parse_method_call(name, args, span, fctx))
            }
            "method_name"
                if self.sig_matches_shape(name, &["HttpMethod"], "Result[String, HttpError]") =>
            {
                Some(self.gen_http_method_name_call(name, args, span, fctx))
            }
            "status_reason"
                if self.sig_matches_shape(name, &["Int"], "Result[String, HttpError]") =>
            {
                Some(self.gen_http_status_reason_call(name, args, span, fctx))
            }
            "validate_header"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[Bool, HttpError]",
                ) =>
            {
                Some(self.gen_http_validate_header_call(name, args, span, fctx))
            }
            "validate_target"
                if self.sig_matches_shape(name, &["String"], "Result[Bool, HttpError]") =>
            {
                Some(self.gen_http_validate_target_call(name, args, span, fctx))
            }
            "header"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[HttpHeader, HttpError]",
                ) =>
            {
                Some(self.gen_http_header_call(name, args, span, fctx))
            }
            "request"
                if self.sig_matches_shape(
                    name,
                    &["HttpMethod", "String", "Vec[HttpHeader]", "String"],
                    "Result[HttpRequest, HttpError]",
                ) =>
            {
                Some(self.gen_http_request_call(name, args, span, fctx))
            }
            "response"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Vec[HttpHeader]", "String"],
                    "Result[HttpResponse, HttpError]",
                ) =>
            {
                Some(self.gen_http_response_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_http_server_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "listen" | "aic_http_server_listen_intrinsic" => "listen",
            "accept" | "aic_http_server_accept_intrinsic" => "accept",
            "read_request" | "aic_http_server_read_request_intrinsic" => "read_request",
            "aic_http_server_async_read_request_intrinsic" => "async_read_request",
            "write_response" | "aic_http_server_write_response_intrinsic" => "write_response",
            "aic_http_server_async_write_response_intrinsic" => "async_write_response",
            "close" | "aic_http_server_close_intrinsic" => "close",
            "text_response" | "aic_http_server_text_response_intrinsic" => "text_response",
            "json_response" | "aic_http_server_json_response_intrinsic" => "json_response",
            "header" | "aic_http_server_header_intrinsic" => "header",
            _ => return None,
        };

        match canonical {
            "listen" if self.sig_matches_shape(name, &["String"], "Result[Int, ServerError]") => {
                Some(self.gen_http_server_listen_call(name, args, span, fctx))
            }
            "accept"
                if self.sig_matches_shape(name, &["Int", "Int"], "Result[Int, ServerError]") =>
            {
                Some(self.gen_http_server_accept_call(name, args, span, fctx))
            }
            "read_request"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[Request, ServerError]",
                ) =>
            {
                Some(self.gen_http_server_read_request_call(
                    name,
                    "aic_rt_http_server_read_request",
                    args,
                    span,
                    fctx,
                ))
            }
            "async_read_request"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[Request, ServerError]",
                ) =>
            {
                Some(self.gen_http_server_read_request_call(
                    name,
                    "aic_rt_http_server_async_read_request",
                    args,
                    span,
                    fctx,
                ))
            }
            "write_response"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Response"],
                    "Result[Int, ServerError]",
                ) =>
            {
                Some(self.gen_http_server_write_response_call(
                    name,
                    "aic_rt_http_server_write_response",
                    args,
                    span,
                    fctx,
                ))
            }
            "async_write_response"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Response"],
                    "Result[Int, ServerError]",
                ) =>
            {
                Some(self.gen_http_server_write_response_call(
                    name,
                    "aic_rt_http_server_async_write_response",
                    args,
                    span,
                    fctx,
                ))
            }
            "close" if self.sig_matches_shape(name, &["Int"], "Result[Bool, ServerError]") => {
                Some(self.gen_http_server_close_call(name, args, span, fctx))
            }
            "text_response" if self.sig_matches_shape(name, &["Int", "String"], "Response") => {
                Some(self.gen_http_server_text_response_call(
                    name,
                    args,
                    "text/plain; charset=utf-8",
                    span,
                    fctx,
                ))
            }
            "json_response" if self.sig_matches_shape(name, &["Int", "String"], "Response") => {
                Some(self.gen_http_server_text_response_call(
                    name,
                    args,
                    "application/json",
                    span,
                    fctx,
                ))
            }
            "header" if self.sig_matches_shape(name, &["Response", "String"], "Option[String]") => {
                Some(self.gen_http_server_header_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_http_server_listen_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "listen expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let addr = self.gen_expr(&args[0], fctx)?;
        if addr.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "listen expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&addr, args[0].span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_http_server_listen(i8* {}, i64 {}, i64 {}, i64* {})",
            err, ptr, len, cap, out_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
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
        self.wrap_http_server_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_server_accept_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "accept expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let listener = self.gen_expr(&args[0], fctx)?;
        let timeout = self.gen_expr(&args[1], fctx)?;
        if listener.ty != LType::Int || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "accept expects (Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_http_server_accept(i64 {}, i64 {}, i64* {})",
            err,
            listener.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let conn = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", conn, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(conn),
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
        self.wrap_http_server_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_server_read_request_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "read_request expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let conn = self.gen_expr(&args[0], fctx)?;
        let max_bytes = self.gen_expr(&args[1], fctx)?;
        let timeout = self.gen_expr(&args[2], fctx)?;
        if conn.ty != LType::Int || max_bytes.ty != LType::Int || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "read_request expects (Int, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }

        let method_ptr_slot = self.new_temp();
        let method_len_slot = self.new_temp();
        let path_ptr_slot = self.new_temp();
        let path_len_slot = self.new_temp();
        let query_slot = self.new_temp();
        let headers_slot = self.new_temp();
        let body_ptr_slot = self.new_temp();
        let body_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", method_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", method_len_slot));
        fctx.lines.push(format!("  {} = alloca i8*", path_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", path_len_slot));
        fctx.lines.push(format!("  {} = alloca i64", query_slot));
        fctx.lines.push(format!("  {} = alloca i64", headers_slot));
        fctx.lines.push(format!("  {} = alloca i8*", body_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", body_len_slot));

        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64 {}, i64 {}, i8** {}, i64* {}, i8** {}, i64* {}, i64* {}, i64* {}, i8** {}, i64* {})",
            err,
            runtime_fn,
            conn.repr.clone().unwrap_or_else(|| "0".to_string()),
            max_bytes.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            method_ptr_slot,
            method_len_slot,
            path_ptr_slot,
            path_len_slot,
            query_slot,
            headers_slot,
            body_ptr_slot,
            body_len_slot
        ));

        let method_ptr = self.new_temp();
        let method_len = self.new_temp();
        let path_ptr = self.new_temp();
        let path_len = self.new_temp();
        let query_handle = self.new_temp();
        let headers_handle = self.new_temp();
        let body_ptr = self.new_temp();
        let body_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            method_ptr, method_ptr_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            method_len, method_len_slot
        ));
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", path_ptr, path_ptr_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", path_len, path_len_slot));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            query_handle, query_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            headers_handle, headers_slot
        ));
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", body_ptr, body_ptr_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", body_len, body_len_slot));

        let method_value = self.build_string_value(&method_ptr, &method_len, &method_len, fctx);
        let path_value = self.build_string_value(&path_ptr, &path_len, &path_len, fctx);
        let body_value = self.build_string_value(&body_ptr, &body_len, &body_len, fctx);

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
        let LType::Struct(layout) = &ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "read_request expects Result[Request, ServerError] return type",
                self.file,
                span,
            ));
            return None;
        };
        let Some(query_ty) = layout
            .fields
            .iter()
            .find(|field| field.name == "query")
            .map(|field| field.ty.clone())
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Request struct is missing `query` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(headers_ty) = layout
            .fields
            .iter()
            .find(|field| field.name == "headers")
            .map(|field| field.ty.clone())
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Request struct is missing `headers` field",
                self.file,
                span,
            ));
            return None;
        };
        let query_value =
            self.build_map_value_from_handle(&query_ty, &query_handle, args[0].span, fctx)?;
        let headers_value =
            self.build_map_value_from_handle(&headers_ty, &headers_handle, args[0].span, fctx)?;

        let ok_payload = self.build_http_struct_value(
            &ok_ty,
            "Request",
            &[
                ("method", method_value),
                ("path", path_value),
                ("query", query_value),
                ("headers", headers_value),
                ("body", body_value),
            ],
            span,
            fctx,
        )?;
        self.wrap_http_server_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_server_write_response_call(
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
                "write_response expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let conn = self.gen_expr(&args[0], fctx)?;
        let response = self.gen_expr(&args[1], fctx)?;
        if conn.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "write_response expects (Int, Response)",
                self.file,
                span,
            ));
            return None;
        }
        let (status, headers, body) =
            self.http_server_response_parts(&response, args[1].span, fctx)?;
        let headers_handle = self.extract_named_handle_from_value(
            &headers,
            "Map",
            "write_response",
            args[1].span,
            fctx,
        )?;
        let (body_ptr, body_len, body_cap) = self.string_parts(&body, args[1].span, fctx)?;
        let sent_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", sent_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            err,
            runtime_fn,
            conn.repr.clone().unwrap_or_else(|| "0".to_string()),
            status.repr.clone().unwrap_or_else(|| "0".to_string()),
            headers_handle,
            body_ptr,
            body_len,
            body_cap,
            sent_slot
        ));
        let sent = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", sent, sent_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(sent),
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
        self.wrap_http_server_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_server_close_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "close expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "close expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_http_server_close(i64 {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string())
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
        self.wrap_http_server_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_server_text_response_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        content_type: &str,
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
        let status = self.gen_expr(&args[0], fctx)?;
        let body = self.gen_expr(&args[1], fctx)?;
        if status.ty != LType::Int || body.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects (Int, String)"),
                self.file,
                span,
            ));
            return None;
        }

        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let LType::Struct(layout) = &result_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Response return type"),
                self.file,
                span,
            ));
            return None;
        };
        let Some(headers_ty) = layout
            .fields
            .iter()
            .find(|field| field.name == "headers")
            .map(|field| field.ty.clone())
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Response struct is missing `headers` field",
                self.file,
                span,
            ));
            return None;
        };

        let map_handle_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", map_handle_slot));
        let map_new_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_map_new(i64 1, i64 1, i64* {})",
            map_new_err, map_handle_slot
        ));
        let map_handle = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            map_handle, map_handle_slot
        ));

        let key = self.string_literal("content-type", fctx);
        let value = self.string_literal(content_type, fctx);
        let (kptr, klen, kcap) = self.string_parts(&key, span, fctx)?;
        let (vptr, vlen, vcap) = self.string_parts(&value, span, fctx)?;
        let map_insert_err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_map_insert_string(i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
            map_insert_err, map_handle, kptr, klen, kcap, vptr, vlen, vcap
        ));

        let headers =
            self.build_map_value_from_handle(&headers_ty, &map_handle, args[0].span, fctx)?;
        self.build_http_struct_value(
            &result_ty,
            "Response",
            &[("status", status), ("headers", headers), ("body", body)],
            span,
            fctx,
        )
    }

    pub(super) fn gen_http_server_header_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "header expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let response = self.gen_expr(&args[0], fctx)?;
        let header_name = self.gen_expr(&args[1], fctx)?;
        if header_name.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "header expects (Response, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (_status, headers, _body) =
            self.http_server_response_parts(&response, args[0].span, fctx)?;
        let headers_handle =
            self.extract_named_handle_from_value(&headers, "Map", "header", args[0].span, fctx)?;
        let (kptr, klen, kcap) = self.string_parts(&header_name, args[1].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_map_get_string(i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            found, headers_handle, kptr, klen, kcap, out_ptr_slot, out_len_slot
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

    pub(super) fn gen_router_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        match name {
            "new_router" | "aic_router_new_intrinsic"
                if self.sig_matches_shape(name, &[], "Result[Router, RouterError]") =>
            {
                Some(self.gen_router_new_call(name, args, span, fctx))
            }
            "add" | "aic_router_add_intrinsic"
                if self.sig_matches_shape(
                    name,
                    &["Router", "String", "String", "Int"],
                    "Result[Router, RouterError]",
                ) =>
            {
                Some(self.gen_router_add_call(name, args, span, fctx))
            }
            "match_route" | "aic_router_match_intrinsic"
                if self.sig_matches_shape(
                    name,
                    &["Router", "String", "String"],
                    "Result[Option[RouteMatch], RouterError]",
                ) =>
            {
                Some(self.gen_router_match_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_router_new_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "new_router expects zero arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_router_new(i64* {})",
            err, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload =
            self.build_concurrency_ok_handle_payload(&result_ty, "Router", &handle, span, fctx)?;
        self.wrap_router_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_router_add_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 4 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "add expects four arguments",
                self.file,
                span,
            ));
            return None;
        }
        let router = self.gen_expr(&args[0], fctx)?;
        let method = self.gen_expr(&args[1], fctx)?;
        let pattern = self.gen_expr(&args[2], fctx)?;
        let route_id = self.gen_expr(&args[3], fctx)?;
        if method.ty != LType::String || pattern.ty != LType::String || route_id.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "add expects (Router, String, String, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let handle =
            self.extract_named_handle_from_value(&router, "Router", "add", args[0].span, fctx)?;
        let (method_ptr, method_len, method_cap) =
            self.string_parts(&method, args[1].span, fctx)?;
        let (pattern_ptr, pattern_len, pattern_cap) =
            self.string_parts(&pattern, args[2].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_router_add(i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {})",
            err,
            handle,
            method_ptr,
            method_len,
            method_cap,
            pattern_ptr,
            pattern_len,
            pattern_cap,
            route_id.repr.clone().unwrap_or_else(|| "0".to_string()),
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
        let ok_payload =
            self.build_concurrency_ok_handle_payload(&result_ty, "Router", &handle, span, fctx)?;
        self.wrap_router_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_router_match_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "match_route expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let router = self.gen_expr(&args[0], fctx)?;
        let method = self.gen_expr(&args[1], fctx)?;
        let path = self.gen_expr(&args[2], fctx)?;
        if method.ty != LType::String || path.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "match_route expects (Router, String, String)",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.extract_named_handle_from_value(
            &router,
            "Router",
            "match_route",
            args[0].span,
            fctx,
        )?;
        let (method_ptr, method_len, method_cap) =
            self.string_parts(&method, args[1].span, fctx)?;
        let (path_ptr, path_len, path_cap) = self.string_parts(&path, args[2].span, fctx)?;
        let route_id_slot = self.new_temp();
        let params_slot = self.new_temp();
        let found_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", route_id_slot));
        fctx.lines.push(format!("  {} = alloca i64", params_slot));
        fctx.lines.push(format!("  {} = alloca i64", found_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_router_match(i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64* {}, i64* {}, i64* {})",
            err,
            handle,
            method_ptr,
            method_len,
            method_cap,
            path_ptr,
            path_len,
            path_cap,
            route_id_slot,
            params_slot,
            found_slot
        ));
        let route_id = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", route_id, route_id_slot));
        let params_handle = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            params_handle, params_slot
        ));
        let found = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", found, found_slot));
        let found_bool = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", found_bool, found));

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
        let Some((_, route_match_ty, _, _)) = self.option_layout_parts(&ok_ty, span) else {
            return None;
        };
        let LType::Struct(route_match_layout) = &route_match_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "match_route expects Option[RouteMatch] payload",
                self.file,
                span,
            ));
            return None;
        };
        let Some(params_ty) = route_match_layout
            .fields
            .iter()
            .find(|field| field.name == "params")
            .map(|field| field.ty.clone())
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "RouteMatch struct is missing `params` field",
                self.file,
                span,
            ));
            return None;
        };
        let params_value =
            self.build_map_value_from_handle(&params_ty, &params_handle, args[0].span, fctx)?;
        let route_match = self.build_http_struct_value(
            &route_match_ty,
            "RouteMatch",
            &[
                (
                    "route_id",
                    Value {
                        ty: LType::Int,
                        repr: Some(route_id),
                    },
                ),
                ("params", params_value),
            ],
            span,
            fctx,
        )?;
        let ok_payload =
            self.wrap_option_with_condition(&ok_ty, route_match, &found_bool, span, fctx)?;
        self.wrap_router_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_url_parse_call(
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
        let scheme_ptr_slot = self.new_temp();
        let scheme_len_slot = self.new_temp();
        let host_ptr_slot = self.new_temp();
        let host_len_slot = self.new_temp();
        let port_slot = self.new_temp();
        let path_ptr_slot = self.new_temp();
        let path_len_slot = self.new_temp();
        let query_ptr_slot = self.new_temp();
        let query_len_slot = self.new_temp();
        let fragment_ptr_slot = self.new_temp();
        let fragment_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", scheme_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", scheme_len_slot));
        fctx.lines.push(format!("  {} = alloca i8*", host_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", host_len_slot));
        fctx.lines.push(format!("  {} = alloca i64", port_slot));
        fctx.lines.push(format!("  {} = alloca i8*", path_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", path_len_slot));
        fctx.lines
            .push(format!("  {} = alloca i8*", query_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", query_len_slot));
        fctx.lines
            .push(format!("  {} = alloca i8*", fragment_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", fragment_len_slot));

        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_url_parse(i8* {}, i64 {}, i64 {}, i8** {}, i64* {}, i8** {}, i64* {}, i64* {}, i8** {}, i64* {}, i8** {}, i64* {}, i8** {}, i64* {})",
            err,
            ptr,
            len,
            cap,
            scheme_ptr_slot,
            scheme_len_slot,
            host_ptr_slot,
            host_len_slot,
            port_slot,
            path_ptr_slot,
            path_len_slot,
            query_ptr_slot,
            query_len_slot,
            fragment_ptr_slot,
            fragment_len_slot
        ));

        let scheme_ptr = self.new_temp();
        let scheme_len = self.new_temp();
        let host_ptr = self.new_temp();
        let host_len = self.new_temp();
        let port_reg = self.new_temp();
        let path_ptr = self.new_temp();
        let path_len = self.new_temp();
        let query_ptr = self.new_temp();
        let query_len = self.new_temp();
        let fragment_ptr = self.new_temp();
        let fragment_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            scheme_ptr, scheme_ptr_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            scheme_len, scheme_len_slot
        ));
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", host_ptr, host_ptr_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", host_len, host_len_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", port_reg, port_slot));
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", path_ptr, path_ptr_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", path_len, path_len_slot));
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            query_ptr, query_ptr_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            query_len, query_len_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            fragment_ptr, fragment_ptr_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            fragment_len, fragment_len_slot
        ));

        let scheme = self.build_string_value(&scheme_ptr, &scheme_len, &scheme_len, fctx);
        let host = self.build_string_value(&host_ptr, &host_len, &host_len, fctx);
        let path = self.build_string_value(&path_ptr, &path_len, &path_len, fctx);
        let query = self.build_string_value(&query_ptr, &query_len, &query_len, fctx);
        let fragment = self.build_string_value(&fragment_ptr, &fragment_len, &fragment_len, fctx);
        let port = Value {
            ty: LType::Int,
            repr: Some(port_reg),
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
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let ok_payload = self.build_url_struct_value(
            &ok_ty, scheme, host, port, path, query, fragment, span, fctx,
        )?;
        self.wrap_url_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_url_normalize_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "normalize expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let url = self.gen_expr(&args[0], fctx)?;
        let (scheme, host, port, path, query, fragment) =
            self.url_parts(&url, args[0].span, fctx)?;
        let (scheme_ptr, scheme_len, scheme_cap) = self.string_parts(&scheme, span, fctx)?;
        let (host_ptr, host_len, host_cap) = self.string_parts(&host, span, fctx)?;
        let (path_ptr, path_len, path_cap) = self.string_parts(&path, span, fctx)?;
        let (query_ptr, query_len, query_cap) = self.string_parts(&query, span, fctx)?;
        let (fragment_ptr, fragment_len, fragment_cap) =
            self.string_parts(&fragment, span, fctx)?;

        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_url_normalize(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            scheme_ptr,
            scheme_len,
            scheme_cap,
            host_ptr,
            host_len,
            host_cap,
            port.repr.clone().unwrap_or_else(|| "0".to_string()),
            path_ptr,
            path_len,
            path_cap,
            query_ptr,
            query_len,
            query_cap,
            fragment_ptr,
            fragment_len,
            fragment_cap,
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_url_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_url_net_addr_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "net_addr expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let url = self.gen_expr(&args[0], fctx)?;
        let (scheme, host, port, _, _, _) = self.url_parts(&url, args[0].span, fctx)?;
        let (scheme_ptr, scheme_len, scheme_cap) = self.string_parts(&scheme, span, fctx)?;
        let (host_ptr, host_len, host_cap) = self.string_parts(&host, span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_url_net_addr(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            scheme_ptr,
            scheme_len,
            scheme_cap,
            host_ptr,
            host_len,
            host_cap,
            port.repr.clone().unwrap_or_else(|| "0".to_string()),
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_url_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_parse_method_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "parse_method expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let text = self.gen_expr(&args[0], fctx)?;
        if text.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "parse_method expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&text, args[0].span, fctx)?;
        let out_tag_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_tag_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_http_parse_method(i8* {}, i64 {}, i64 {}, i64* {})",
            err, ptr, len, cap, out_tag_slot
        ));
        let out_tag = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_tag, out_tag_slot));

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
        let LType::Enum(ok_layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "parse_method expects Result[HttpMethod, HttpError] return type",
                self.file,
                span,
            ));
            return None;
        };
        let out_tag_i32 = self.new_temp();
        fctx.lines
            .push(format!("  {} = trunc i64 {} to i32", out_tag_i32, out_tag));
        let ok_payload =
            self.build_no_payload_enum_with_tag(&ok_layout, &out_tag_i32, span, fctx)?;
        self.wrap_http_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_method_name_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "method_name expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let method = self.gen_expr(&args[0], fctx)?;
        let tag_i32 = self.enum_tag_i32(&method, args[0].span, fctx)?;
        let tag_i64 = self.new_temp();
        fctx.lines
            .push(format!("  {} = sext i32 {} to i64", tag_i64, tag_i32));
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_http_method_name(i64 {}, i8** {}, i64* {})",
            err, tag_i64, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
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
        self.wrap_http_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_status_reason_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "status_reason expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let status = self.gen_expr(&args[0], fctx)?;
        if status.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "status_reason expects Int",
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
            "  {} = call i64 @aic_rt_http_status_reason(i64 {}, i8** {}, i64* {})",
            err,
            status.repr.clone().unwrap_or_else(|| "0".to_string()),
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
        let Some(result_ty) = self.fn_sig(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_http_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_validate_header_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "validate_header expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let name_value = self.gen_expr(&args[0], fctx)?;
        let header_value = self.gen_expr(&args[1], fctx)?;
        if name_value.ty != LType::String || header_value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "validate_header expects (String, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (name_ptr, name_len, name_cap) = self.string_parts(&name_value, args[0].span, fctx)?;
        let (value_ptr, value_len, value_cap) =
            self.string_parts(&header_value, args[1].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_http_validate_header(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
            err, name_ptr, name_len, name_cap, value_ptr, value_len, value_cap
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
        self.wrap_http_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_validate_target_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "validate_target expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let target = self.gen_expr(&args[0], fctx)?;
        if target.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "validate_target expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&target, args[0].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_http_validate_target(i8* {}, i64 {}, i64 {})",
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
        self.wrap_http_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_header_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "header expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let header_name = self.gen_expr(&args[0], fctx)?;
        let header_value = self.gen_expr(&args[1], fctx)?;
        if header_name.ty != LType::String || header_value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "header expects (String, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (name_ptr, name_len, name_cap) = self.string_parts(&header_name, args[0].span, fctx)?;
        let (value_ptr, value_len, value_cap) =
            self.string_parts(&header_value, args[1].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_http_validate_header(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
            err, name_ptr, name_len, name_cap, value_ptr, value_len, value_cap
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
        let ok_payload = self.build_http_struct_value(
            &ok_ty,
            "HttpHeader",
            &[("name", header_name), ("value", header_value)],
            span,
            fctx,
        )?;
        self.wrap_http_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_request_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 4 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "request expects four arguments",
                self.file,
                span,
            ));
            return None;
        }
        let method = self.gen_expr(&args[0], fctx)?;
        let target = self.gen_expr(&args[1], fctx)?;
        let headers = self.gen_expr(&args[2], fctx)?;
        let body = self.gen_expr(&args[3], fctx)?;
        if target.ty != LType::String || body.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "request expects (HttpMethod, String, Vec[HttpHeader], String)",
                self.file,
                span,
            ));
            return None;
        }
        let (target_ptr, target_len, target_cap) =
            self.string_parts(&target, args[1].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_http_validate_target(i8* {}, i64 {}, i64 {})",
            err, target_ptr, target_len, target_cap
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
        let ok_payload = self.build_http_struct_value(
            &ok_ty,
            "HttpRequest",
            &[
                ("method", method),
                ("target", target),
                ("headers", headers),
                ("body", body),
            ],
            span,
            fctx,
        )?;
        self.wrap_http_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_http_response_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "response expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let status = self.gen_expr(&args[0], fctx)?;
        let headers = self.gen_expr(&args[1], fctx)?;
        let body = self.gen_expr(&args[2], fctx)?;
        if status.ty != LType::Int || body.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "response expects (Int, Vec[HttpHeader], String)",
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
            "  {} = call i64 @aic_rt_http_status_reason(i64 {}, i8** {}, i64* {})",
            err,
            status.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let reason = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);

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
        let ok_payload = self.build_http_struct_value(
            &ok_ty,
            "HttpResponse",
            &[
                ("status", status),
                ("reason", reason),
                ("headers", headers),
                ("body", body),
            ],
            span,
            fctx,
        )?;
        self.wrap_http_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn build_http_struct_value(
        &mut self,
        ty: &LType,
        expected_name: &str,
        fields: &[(&str, Value)],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Struct(layout) = ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "expected {expected_name} struct, found '{}'",
                    render_type(ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != expected_name {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("expected {expected_name} struct, found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }
        let mut ordered = Vec::new();
        for field in &layout.fields {
            let Some((_, value)) = fields.iter().find(|(name, _)| *name == field.name) else {
                self.diagnostics.push(Diagnostic::error(
                    "E5011",
                    format!("{} is missing field '{}'", layout.repr, field.name),
                    self.file,
                    span,
                ));
                return None;
            };
            ordered.push(value.clone());
        }
        self.build_struct_value(layout, &ordered, span, fctx)
    }

    pub(super) fn http_server_response_parts(
        &mut self,
        response: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(Value, Value, Value)> {
        let LType::Struct(layout) = &response.ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "expected Response struct value",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Response" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("expected Response struct value, found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }

        let mut status = None;
        let mut headers = None;
        let mut body = None;
        let response_repr = response
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&response.ty));
        let response_ty = llvm_type(&response.ty);

        for (index, field) in layout.fields.iter().enumerate() {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, {}",
                reg, response_ty, response_repr, index
            ));
            let value = Value {
                ty: field.ty.clone(),
                repr: Some(reg),
            };
            match field.name.as_str() {
                "status" => status = Some(value),
                "headers" => headers = Some(value),
                "body" => body = Some(value),
                _ => {}
            }
        }

        let Some(status) = status else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Response struct is missing `status` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(headers) = headers else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Response struct is missing `headers` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(body) = body else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Response struct is missing `body` field",
                self.file,
                span,
            ));
            return None;
        };
        if status.ty != LType::Int || body.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Response struct fields must be `status: Int`, `headers: Map`, and `body: String`",
                self.file,
                span,
            ));
            return None;
        }
        let LType::Struct(headers_layout) = &headers.ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Response headers field must be a Map",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&headers_layout.repr) != "Map" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Response headers field must be a Map",
                self.file,
                span,
            ));
            return None;
        }
        Some((status, headers, body))
    }

    pub(super) fn build_url_struct_value(
        &mut self,
        url_ty: &LType,
        scheme: Value,
        host: Value,
        port: Value,
        path: Value,
        query: Value,
        fragment: Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Struct(layout) = url_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("expected Url struct, found '{}'", render_type(url_ty)),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Url" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("expected Url struct, found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }
        let mut ordered = Vec::new();
        for field in &layout.fields {
            match field.name.as_str() {
                "scheme" => ordered.push(scheme.clone()),
                "host" => ordered.push(host.clone()),
                "port" => ordered.push(port.clone()),
                "path" => ordered.push(path.clone()),
                "query" => ordered.push(query.clone()),
                "fragment" => ordered.push(fragment.clone()),
                _ => {
                    self.diagnostics.push(Diagnostic::error(
                        "E5011",
                        format!("Url contains unsupported field '{}'", field.name),
                        self.file,
                        span,
                    ));
                    return None;
                }
            }
        }
        self.build_struct_value(layout, &ordered, span, fctx)
    }

    pub(super) fn url_parts(
        &mut self,
        url: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(Value, Value, Value, Value, Value, Value)> {
        let LType::Struct(layout) = &url.ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "expected Url struct value",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Url" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("expected Url struct value, found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }

        let mut scheme = None;
        let mut host = None;
        let mut port = None;
        let mut path = None;
        let mut query = None;
        let mut fragment = None;
        let url_repr = url.repr.clone().unwrap_or_else(|| default_value(&url.ty));
        let url_ty = llvm_type(&url.ty);

        for (index, field) in layout.fields.iter().enumerate() {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = extractvalue {} {}, {}",
                reg, url_ty, url_repr, index
            ));
            let value = Value {
                ty: field.ty.clone(),
                repr: Some(reg),
            };
            match field.name.as_str() {
                "scheme" => scheme = Some(value),
                "host" => host = Some(value),
                "port" => port = Some(value),
                "path" => path = Some(value),
                "query" => query = Some(value),
                "fragment" => fragment = Some(value),
                _ => {}
            }
        }

        let Some(scheme) = scheme else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Url struct is missing `scheme` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(host) = host else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Url struct is missing `host` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(port) = port else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Url struct is missing `port` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(path) = path else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Url struct is missing `path` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(query) = query else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Url struct is missing `query` field",
                self.file,
                span,
            ));
            return None;
        };
        let Some(fragment) = fragment else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Url struct is missing `fragment` field",
                self.file,
                span,
            ));
            return None;
        };
        if scheme.ty != LType::String
            || host.ty != LType::String
            || port.ty != LType::Int
            || path.ty != LType::String
            || query.ty != LType::String
            || fragment.ty != LType::String
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Url fields must be scheme/host/path/query/fragment as String and port as Int",
                self.file,
                span,
            ));
            return None;
        }
        Some((scheme, host, port, path, query, fragment))
    }

    pub(super) fn wrap_url_result(
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
                    "url builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_url_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("url_ok");
        let err_label = self.new_label("url_err");
        let cont_label = self.new_label("url_cont");
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

    pub(super) fn wrap_http_result(
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
                    "http builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_http_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("http_ok");
        let err_label = self.new_label("http_err");
        let cont_label = self.new_label("http_cont");
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

    pub(super) fn wrap_http_server_result(
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
                    "http_server builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_http_server_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("http_server_ok");
        let err_label = self.new_label("http_server_err");
        let cont_label = self.new_label("http_server_cont");
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

    pub(super) fn wrap_router_result(
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
                    "router builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_router_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("router_ok");
        let err_label = self.new_label("router_err");
        let cont_label = self.new_label("router_cont");
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
