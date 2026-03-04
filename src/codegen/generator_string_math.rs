use super::*;

impl<'a> Generator<'a> {
    pub(super) fn gen_string_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "len" | "aic_string_len_intrinsic" => "len",
            "contains" | "aic_string_contains_intrinsic" => "contains",
            "starts_with" | "aic_string_starts_with_intrinsic" => "starts_with",
            "ends_with" | "aic_string_ends_with_intrinsic" => "ends_with",
            "index_of" | "aic_string_index_of_intrinsic" => "index_of",
            "last_index_of" | "aic_string_last_index_of_intrinsic" => "last_index_of",
            "substring" | "aic_string_substring_intrinsic" => "substring",
            "char_at" | "aic_string_char_at_intrinsic" => "char_at",
            "split" | "aic_string_split_intrinsic" => "split",
            "split_first" | "aic_string_split_first_intrinsic" => "split_first",
            "trim" | "aic_string_trim_intrinsic" => "trim",
            "trim_start" | "aic_string_trim_start_intrinsic" => "trim_start",
            "trim_end" | "aic_string_trim_end_intrinsic" => "trim_end",
            "to_upper" | "aic_string_to_upper_intrinsic" => "to_upper",
            "to_lower" | "aic_string_to_lower_intrinsic" => "to_lower",
            "replace" | "aic_string_replace_intrinsic" | "aic_time_string_replace_intrinsic" => {
                "replace"
            }
            "repeat" | "aic_string_repeat_intrinsic" => "repeat",
            "parse_int" | "aic_string_parse_int_intrinsic" => "parse_int",
            "parse_float" | "aic_string_parse_float_intrinsic" => "parse_float",
            "aic_numeric_bigint_parse_intrinsic" => "numeric_bigint_parse",
            "aic_numeric_bigint_add_intrinsic" => "numeric_bigint_add",
            "aic_numeric_bigint_sub_intrinsic" => "numeric_bigint_sub",
            "aic_numeric_bigint_mul_intrinsic" => "numeric_bigint_mul",
            "aic_numeric_bigint_div_intrinsic" => "numeric_bigint_div",
            "aic_numeric_biguint_parse_intrinsic" => "numeric_biguint_parse",
            "aic_numeric_biguint_add_intrinsic" => "numeric_biguint_add",
            "aic_numeric_biguint_sub_intrinsic" => "numeric_biguint_sub",
            "aic_numeric_biguint_mul_intrinsic" => "numeric_biguint_mul",
            "aic_numeric_biguint_div_intrinsic" => "numeric_biguint_div",
            "aic_numeric_decimal_parse_intrinsic" => "numeric_decimal_parse",
            "aic_numeric_decimal_add_intrinsic" => "numeric_decimal_add",
            "aic_numeric_decimal_sub_intrinsic" => "numeric_decimal_sub",
            "aic_numeric_decimal_mul_intrinsic" => "numeric_decimal_mul",
            "aic_numeric_decimal_div_intrinsic" => "numeric_decimal_div",
            "int_to_string" | "aic_string_int_to_string_intrinsic" => "int_to_string",
            "float_to_string" | "aic_string_float_to_string_intrinsic" => "float_to_string",
            "bool_to_string" | "aic_string_bool_to_string_intrinsic" => "bool_to_string",
            "is_valid_utf8" | "aic_string_is_valid_utf8_intrinsic" => "is_valid_utf8",
            "aic_bytes_is_valid_utf8_intrinsic" => "is_valid_utf8",
            "is_ascii" | "aic_string_is_ascii_intrinsic" => "is_ascii",
            "aic_bytes_byte_at_intrinsic" => "bytes_byte_at",
            "aic_bytes_from_byte_values_intrinsic" => "bytes_from_byte_values",
            "bytes_to_string_lossy" | "aic_string_bytes_to_string_lossy_intrinsic" => {
                "bytes_to_string_lossy"
            }
            "aic_bytes_to_string_lossy_intrinsic" => "bytes_to_string_lossy",
            "join" | "aic_string_join_intrinsic" => "join",
            "format" | "aic_string_format_intrinsic" => "format",
            _ => return None,
        };

        match canonical {
            "len" if self.sig_matches_shape(name, &["String"], "Int") => {
                Some(self.gen_string_len_call(name, args, span, fctx))
            }
            "contains" if self.sig_matches_shape(name, &["String", "String"], "Bool") => {
                Some(self.gen_string_bool_binary_call(
                    "contains",
                    "aic_rt_string_contains",
                    args,
                    span,
                    fctx,
                ))
            }
            "starts_with" if self.sig_matches_shape(name, &["String", "String"], "Bool") => {
                Some(self.gen_string_bool_binary_call(
                    "starts_with",
                    "aic_rt_string_starts_with",
                    args,
                    span,
                    fctx,
                ))
            }
            "ends_with" if self.sig_matches_shape(name, &["String", "String"], "Bool") => {
                Some(self.gen_string_bool_binary_call(
                    "ends_with",
                    "aic_rt_string_ends_with",
                    args,
                    span,
                    fctx,
                ))
            }
            "index_of" if self.sig_matches_shape(name, &["String", "String"], "Option[Int]") => {
                Some(self.gen_string_option_int_binary_call(
                    name,
                    "index_of",
                    "aic_rt_string_index_of",
                    args,
                    span,
                    fctx,
                ))
            }
            "last_index_of"
                if self.sig_matches_shape(name, &["String", "String"], "Option[Int]") =>
            {
                Some(self.gen_string_option_int_binary_call(
                    name,
                    "last_index_of",
                    "aic_rt_string_last_index_of",
                    args,
                    span,
                    fctx,
                ))
            }
            "substring" if self.sig_matches_shape(name, &["String", "Int", "Int"], "String") => {
                Some(self.gen_string_substring_call(args, span, fctx))
            }
            "char_at" if self.sig_matches_shape(name, &["String", "Int"], "Option[String]") => {
                Some(self.gen_string_char_at_call(name, args, span, fctx))
            }
            "split" if self.sig_matches_shape(name, &["String", "String"], "Vec[String]") => {
                Some(self.gen_string_split_call(name, args, span, fctx))
            }
            "split_first"
                if self.sig_matches_shape(name, &["String", "String"], "Option[Vec[String]]") =>
            {
                Some(self.gen_string_split_first_call(name, args, span, fctx))
            }
            "trim" if self.sig_matches_shape(name, &["String"], "String") => Some(
                self.gen_string_string_unary_call("trim", "aic_rt_string_trim", args, span, fctx),
            ),
            "trim_start" if self.sig_matches_shape(name, &["String"], "String") => {
                Some(self.gen_string_string_unary_call(
                    "trim_start",
                    "aic_rt_string_trim_start",
                    args,
                    span,
                    fctx,
                ))
            }
            "trim_end" if self.sig_matches_shape(name, &["String"], "String") => {
                Some(self.gen_string_string_unary_call(
                    "trim_end",
                    "aic_rt_string_trim_end",
                    args,
                    span,
                    fctx,
                ))
            }
            "to_upper" if self.sig_matches_shape(name, &["String"], "String") => {
                Some(self.gen_string_string_unary_call(
                    "to_upper",
                    "aic_rt_string_to_upper",
                    args,
                    span,
                    fctx,
                ))
            }
            "to_lower" if self.sig_matches_shape(name, &["String"], "String") => {
                Some(self.gen_string_string_unary_call(
                    "to_lower",
                    "aic_rt_string_to_lower",
                    args,
                    span,
                    fctx,
                ))
            }
            "replace"
                if self.sig_matches_shape(name, &["String", "String", "String"], "String") =>
            {
                Some(self.gen_string_replace_call(args, span, fctx))
            }
            "repeat" if self.sig_matches_shape(name, &["String", "Int"], "String") => {
                Some(self.gen_string_repeat_call(args, span, fctx))
            }
            "parse_int" if self.sig_matches_shape(name, &["String"], "Result[Int, String]") => {
                Some(self.gen_string_parse_int_call(name, args, span, fctx))
            }
            "parse_float" if self.sig_matches_shape(name, &["String"], "Result[Float, String]") => {
                Some(self.gen_string_parse_float_call(name, args, span, fctx))
            }
            "numeric_bigint_parse"
                if self.sig_matches_shape(name, &["String"], "Result[String, String]") =>
            {
                Some(self.gen_string_result_string_unary_call(
                    name,
                    "aic_numeric_bigint_parse_intrinsic",
                    "aic_rt_numeric_bigint_parse",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_bigint_add"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_bigint_add_intrinsic",
                    "aic_rt_numeric_bigint_add",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_bigint_sub"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_bigint_sub_intrinsic",
                    "aic_rt_numeric_bigint_sub",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_bigint_mul"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_bigint_mul_intrinsic",
                    "aic_rt_numeric_bigint_mul",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_bigint_div"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_bigint_div_intrinsic",
                    "aic_rt_numeric_bigint_div",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_biguint_parse"
                if self.sig_matches_shape(name, &["String"], "Result[String, String]") =>
            {
                Some(self.gen_string_result_string_unary_call(
                    name,
                    "aic_numeric_biguint_parse_intrinsic",
                    "aic_rt_numeric_biguint_parse",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_biguint_add"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_biguint_add_intrinsic",
                    "aic_rt_numeric_biguint_add",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_biguint_sub"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_biguint_sub_intrinsic",
                    "aic_rt_numeric_biguint_sub",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_biguint_mul"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_biguint_mul_intrinsic",
                    "aic_rt_numeric_biguint_mul",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_biguint_div"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_biguint_div_intrinsic",
                    "aic_rt_numeric_biguint_div",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_decimal_parse"
                if self.sig_matches_shape(name, &["String"], "Result[String, String]") =>
            {
                Some(self.gen_string_result_string_unary_call(
                    name,
                    "aic_numeric_decimal_parse_intrinsic",
                    "aic_rt_numeric_decimal_parse",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_decimal_add"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_decimal_add_intrinsic",
                    "aic_rt_numeric_decimal_add",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_decimal_sub"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_decimal_sub_intrinsic",
                    "aic_rt_numeric_decimal_sub",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_decimal_mul"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_decimal_mul_intrinsic",
                    "aic_rt_numeric_decimal_mul",
                    args,
                    span,
                    fctx,
                ))
            }
            "numeric_decimal_div"
                if self.sig_matches_shape(
                    name,
                    &["String", "String"],
                    "Result[String, String]",
                ) =>
            {
                Some(self.gen_string_result_string_binary_call(
                    name,
                    "aic_numeric_decimal_div_intrinsic",
                    "aic_rt_numeric_decimal_div",
                    args,
                    span,
                    fctx,
                ))
            }
            "int_to_string" if self.sig_matches_shape(name, &["Int"], "String") => {
                Some(self.gen_string_int_to_string_call(args, span, fctx))
            }
            "float_to_string" if self.sig_matches_shape(name, &["Float"], "String") => {
                Some(self.gen_string_float_to_string_call(args, span, fctx))
            }
            "bool_to_string" if self.sig_matches_shape(name, &["Bool"], "String") => {
                Some(self.gen_string_bool_to_string_call(args, span, fctx))
            }
            "is_valid_utf8" if self.sig_matches_shape(name, &["String"], "Bool") => {
                Some(self.gen_string_bool_unary_call(
                    "is_valid_utf8",
                    "aic_rt_string_is_valid_utf8",
                    args,
                    span,
                    fctx,
                ))
            }
            "is_ascii" if self.sig_matches_shape(name, &["String"], "Bool") => {
                Some(self.gen_string_bool_unary_call(
                    "is_ascii",
                    "aic_rt_string_is_ascii",
                    args,
                    span,
                    fctx,
                ))
            }
            "bytes_to_string_lossy" if self.sig_matches_shape(name, &["String"], "String") => {
                Some(self.gen_string_string_unary_call(
                    "bytes_to_string_lossy",
                    "aic_rt_string_bytes_to_string_lossy",
                    args,
                    span,
                    fctx,
                ))
            }
            "bytes_byte_at" if self.sig_matches_shape(name, &["String", "Int"], "UInt8") => {
                Some(self.gen_bytes_byte_at_call(args, span, LType::UInt8, fctx))
            }
            "bytes_byte_at" if self.sig_matches_shape(name, &["String", "Int"], "Int") => {
                Some(self.gen_bytes_byte_at_call(args, span, LType::Int, fctx))
            }
            "bytes_from_byte_values"
                if self.sig_matches_shape(name, &["Vec[UInt8]"], "String")
                    || self.sig_matches_shape(name, &["Vec[Int]"], "String") =>
            {
                Some(self.gen_bytes_from_byte_values_call(args, span, fctx))
            }
            "join" if self.sig_matches_shape(name, &["Vec[String]", "String"], "String") => {
                Some(self.gen_string_join_call(args, span, fctx))
            }
            "format" if self.sig_matches_shape(name, &["String", "Vec[String]"], "String") => {
                Some(self.gen_string_format_call(args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_char_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "is_digit" | "aic_char_is_digit_intrinsic" => "is_digit",
            "is_alpha" | "aic_char_is_alpha_intrinsic" => "is_alpha",
            "is_whitespace" | "aic_char_is_whitespace_intrinsic" => "is_whitespace",
            "char_to_int" | "aic_char_to_int_intrinsic" => "char_to_int",
            "int_to_char" | "aic_char_int_to_char_intrinsic" => "int_to_char",
            "chars" | "aic_char_chars_intrinsic" => "chars",
            "from_chars" | "aic_char_from_chars_intrinsic" => "from_chars",
            _ => return None,
        };

        match canonical {
            "is_digit" if self.sig_matches_shape(name, &["Char"], "Bool") => Some(
                self.gen_char_bool_unary_call("is_digit", "aic_rt_char_is_digit", args, span, fctx),
            ),
            "is_alpha" if self.sig_matches_shape(name, &["Char"], "Bool") => Some(
                self.gen_char_bool_unary_call("is_alpha", "aic_rt_char_is_alpha", args, span, fctx),
            ),
            "is_whitespace" if self.sig_matches_shape(name, &["Char"], "Bool") => {
                Some(self.gen_char_bool_unary_call(
                    "is_whitespace",
                    "aic_rt_char_is_whitespace",
                    args,
                    span,
                    fctx,
                ))
            }
            "char_to_int" if self.sig_matches_shape(name, &["Char"], "Int") => {
                Some(self.gen_char_to_int_call(args, span, fctx))
            }
            "int_to_char" if self.sig_matches_shape(name, &["Int"], "Option[Char]") => {
                Some(self.gen_char_int_to_char_call(name, args, span, fctx))
            }
            "chars" if self.sig_matches_shape(name, &["String"], "Vec[Char]") => {
                Some(self.gen_char_chars_call(name, args, span, fctx))
            }
            "from_chars" if self.sig_matches_shape(name, &["Vec[Char]"], "String") => {
                Some(self.gen_char_from_chars_call(args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_char_bool_unary_call(
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
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Char {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Char"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i32 {})",
            raw,
            runtime_fn,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", reg, raw));
        Some(Value {
            ty: LType::Bool,
            repr: Some(reg),
        })
    }

    pub(super) fn gen_char_to_int_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "char_to_int expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Char {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "char_to_int expects Char",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_char_to_int(i32 {})",
            reg,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        Some(Value {
            ty: LType::Int,
            repr: Some(reg),
        })
    }

    pub(super) fn gen_char_int_to_char_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "int_to_char expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "int_to_char expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }

        let out_char_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i32", out_char_slot));
        fctx.lines
            .push(format!("  store i32 0, i32* {}", out_char_slot));

        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_char_int_to_char(i64 {}, i32* {})",
            found,
            value.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_char_slot
        ));

        let out_char = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i32, i32* {}", out_char, out_char_slot));

        let has_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", has_value, found));

        let option_ty = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("unknown function '{name}' in codegen"),
                    self.file,
                    span,
                ));
                None
            })?;

        self.wrap_option_with_condition(
            &option_ty,
            Value {
                ty: LType::Char,
                repr: Some(out_char),
            },
            &has_value,
            span,
            fctx,
        )
    }

    pub(super) fn gen_char_chars_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "chars expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "chars expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&value, args[0].span, fctx)?;
        let out_items_slot = self.new_temp();
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_items_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_char_chars(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            ptr, len, cap, out_items_slot, out_count_slot
        ));
        let out_items = self.new_temp();
        let out_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_items, out_items_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_count, out_count_slot
        ));

        let result_ty = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("unknown function '{name}' in codegen"),
                    self.file,
                    span,
                ));
                None
            })?;

        self.build_vec_value_from_raw_i8_ptr(&result_ty, &out_items, &out_count, span, fctx)
    }

    pub(super) fn gen_char_from_chars_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "from_chars expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let chars_vec = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, _elem_repr, _elem_kind) =
            self.vec_element_info(&chars_vec.ty, "from_chars", args[0].span)?;
        if elem_ty != LType::Char {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "from_chars expects Vec[Char]",
                self.file,
                args[0].span,
            ));
            return None;
        }

        let (chars_ptr_int, chars_len, chars_cap) =
            self.vec_parts(&chars_vec, args[0].span, fctx)?;
        let chars_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = inttoptr i64 {} to i8*",
            chars_ptr, chars_ptr_int
        ));

        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_char_from_chars(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            chars_ptr, chars_len, chars_cap, out_ptr_slot, out_len_slot
        ));

        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_bytes_byte_at_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        result_ty: LType,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_bytes_byte_at_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let data = self.gen_expr(&args[0], fctx)?;
        let index = self.gen_expr(&args[1], fctx)?;
        if data.ty != LType::String || index.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_bytes_byte_at_intrinsic expects (String, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (data_ptr, data_len, data_cap) = self.string_parts(&data, args[0].span, fctx)?;
        let out = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_bytes_byte_at(i8* {}, i64 {}, i64 {}, i64 {})",
            out,
            data_ptr,
            data_len,
            data_cap,
            index.repr.clone().unwrap_or_else(|| "0".to_string())
        ));

        let repr = if result_ty == LType::Int {
            out
        } else {
            let coerced = self.new_temp();
            fctx.lines.push(format!(
                "  {} = trunc i64 {} to {}",
                coerced,
                out,
                llvm_type(&result_ty)
            ));
            coerced
        };
        Some(Value {
            ty: result_ty,
            repr: Some(repr),
        })
    }

    pub(super) fn gen_bytes_from_byte_values_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_bytes_from_byte_values_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let values = self.gen_expr(&args[0], fctx)?;
        let (elem_ty, _elem_repr, _elem_kind) = self.vec_element_info(
            &values.ty,
            "aic_bytes_from_byte_values_intrinsic",
            args[0].span,
        )?;
        if elem_ty != LType::Int && elem_ty != LType::UInt8 {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_bytes_from_byte_values_intrinsic expects Vec[UInt8]",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (values_ptr_int, values_len, values_cap) =
            self.vec_parts(&values, args[0].span, fctx)?;
        let values_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = inttoptr i64 {} to i8*",
            values_ptr, values_ptr_int
        ));

        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let runtime_fn = if elem_ty == LType::UInt8 {
            "aic_rt_bytes_from_u8_values"
        } else {
            "aic_rt_bytes_from_byte_values"
        };
        fctx.lines.push(format!(
            "  call void @{}(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            runtime_fn, values_ptr, values_len, values_cap, out_ptr_slot, out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_string_bool_binary_call(
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
                format!("{name} expects (String, String)"),
                self.file,
                span,
            ));
            return None;
        }
        let (lhs_ptr, lhs_len, lhs_cap) = self.string_parts(&lhs, args[0].span, fctx)?;
        let (rhs_ptr, rhs_len, rhs_cap) = self.string_parts(&rhs, args[1].span, fctx)?;
        let raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {})",
            raw, runtime_fn, lhs_ptr, lhs_len, lhs_cap, rhs_ptr, rhs_len, rhs_cap
        ));
        let reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", reg, raw));
        Some(Value {
            ty: LType::Bool,
            repr: Some(reg),
        })
    }

    pub(super) fn gen_string_bool_unary_call(
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
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&value, args[0].span, fctx)?;
        let raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {})",
            raw, runtime_fn, ptr, len, cap
        ));
        let reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", reg, raw));
        Some(Value {
            ty: LType::Bool,
            repr: Some(reg),
        })
    }

    pub(super) fn gen_string_len_call(
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
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&value, args[0].span, fctx)?;
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_strlen(i8* {}, i64 {}, i64 {})",
            reg, ptr, len, cap
        ));
        Some(Value {
            ty: LType::Int,
            repr: Some(reg),
        })
    }

    pub(super) fn gen_string_option_int_binary_call(
        &mut self,
        fn_name: &str,
        display_name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{display_name} expects two arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let s = self.gen_expr(&args[0], fctx)?;
        let needle = self.gen_expr(&args[1], fctx)?;
        if s.ty != LType::String || needle.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{display_name} expects (String, String)"),
                self.file,
                span,
            ));
            return None;
        }
        let (s_ptr, s_len, s_cap) = self.string_parts(&s, args[0].span, fctx)?;
        let (needle_ptr, needle_len, needle_cap) =
            self.string_parts(&needle, args[1].span, fctx)?;
        let out_index_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_index_slot));
        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            found,
            runtime_fn,
            s_ptr,
            s_len,
            s_cap,
            needle_ptr,
            needle_len,
            needle_cap,
            out_index_slot
        ));
        let out_index = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_index, out_index_slot
        ));
        let has_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", has_value, found));
        let option_ty = self
            .fn_sigs
            .get(fn_name)
            .map(|sig| sig.ret.clone())
            .or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("unknown function '{fn_name}' in codegen"),
                    self.file,
                    span,
                ));
                None
            })?;
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

    pub(super) fn gen_string_substring_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "substring expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let s = self.gen_expr(&args[0], fctx)?;
        let start = self.gen_expr(&args[1], fctx)?;
        let end = self.gen_expr(&args[2], fctx)?;
        if s.ty != LType::String || start.ty != LType::Int || end.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "substring expects (String, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (s_ptr, s_len, s_cap) = self.string_parts(&s, args[0].span, fctx)?;
        let start_repr = start.repr.clone().unwrap_or_else(|| "0".to_string());
        let end_repr = end.repr.clone().unwrap_or_else(|| "0".to_string());
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_string_substring(i8* {}, i64 {}, i64 {}, i64 {}, i64 {}, i8** {}, i64* {})",
            s_ptr, s_len, s_cap, start_repr, end_repr, out_ptr_slot, out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_string_char_at_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "char_at expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let s = self.gen_expr(&args[0], fctx)?;
        let index = self.gen_expr(&args[1], fctx)?;
        if s.ty != LType::String || index.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "char_at expects (String, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (s_ptr, s_len, s_cap) = self.string_parts(&s, args[0].span, fctx)?;
        let index_repr = index.repr.clone().unwrap_or_else(|| "0".to_string());
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_string_char_at(i8* {}, i64 {}, i64 {}, i64 {}, i8** {}, i64* {})",
            found, s_ptr, s_len, s_cap, index_repr, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let some_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let has_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", has_value, found));
        let option_ty = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("unknown function '{name}' in codegen"),
                    self.file,
                    span,
                ));
                None
            })?;
        self.wrap_option_with_condition(&option_ty, some_payload, &has_value, span, fctx)
    }

    pub(super) fn gen_string_split_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "split expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let s = self.gen_expr(&args[0], fctx)?;
        let delimiter = self.gen_expr(&args[1], fctx)?;
        if s.ty != LType::String || delimiter.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "split expects (String, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (s_ptr, s_len, s_cap) = self.string_parts(&s, args[0].span, fctx)?;
        let (delimiter_ptr, delimiter_len, delimiter_cap) =
            self.string_parts(&delimiter, args[1].span, fctx)?;
        let out_items_ptr_slot = self.new_temp();
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_items_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_string_split(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            s_ptr,
            s_len,
            s_cap,
            delimiter_ptr,
            delimiter_len,
            delimiter_cap,
            out_items_ptr_slot,
            out_count_slot
        ));
        let out_items_ptr = self.new_temp();
        let out_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_items_ptr, out_items_ptr_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_count, out_count_slot
        ));
        let result_ty = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("unknown function '{name}' in codegen"),
                    self.file,
                    span,
                ));
                None
            })?;
        self.build_vec_string_from_raw_parts(&result_ty, &out_items_ptr, &out_count, span, fctx)
    }

    pub(super) fn gen_string_split_first_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "split_first expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let s = self.gen_expr(&args[0], fctx)?;
        let delimiter = self.gen_expr(&args[1], fctx)?;
        if s.ty != LType::String || delimiter.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "split_first expects (String, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (s_ptr, s_len, s_cap) = self.string_parts(&s, args[0].span, fctx)?;
        let (delimiter_ptr, delimiter_len, delimiter_cap) =
            self.string_parts(&delimiter, args[1].span, fctx)?;
        let out_items_ptr_slot = self.new_temp();
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_items_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_string_split_first(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            found,
            s_ptr,
            s_len,
            s_cap,
            delimiter_ptr,
            delimiter_len,
            delimiter_cap,
            out_items_ptr_slot,
            out_count_slot
        ));
        let out_items_ptr = self.new_temp();
        let out_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_items_ptr, out_items_ptr_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_count, out_count_slot
        ));
        let option_ty = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("unknown function '{name}' in codegen"),
                    self.file,
                    span,
                ));
                None
            })?;
        let some_payload =
            self.build_vec_string_payload_from_ptr(&out_items_ptr, &out_count, span, fctx)?;
        let has_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", has_value, found));
        self.wrap_option_with_condition(&option_ty, some_payload, &has_value, span, fctx)
    }

    pub(super) fn gen_string_string_unary_call(
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
        let s = self.gen_expr(&args[0], fctx)?;
        if s.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (s_ptr, s_len, s_cap) = self.string_parts(&s, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @{}(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            runtime_fn, s_ptr, s_len, s_cap, out_ptr_slot, out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_string_replace_call(
        &mut self,
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
        let s = self.gen_expr(&args[0], fctx)?;
        let from = self.gen_expr(&args[1], fctx)?;
        let to = self.gen_expr(&args[2], fctx)?;
        if s.ty != LType::String || from.ty != LType::String || to.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "replace expects (String, String, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (s_ptr, s_len, s_cap) = self.string_parts(&s, args[0].span, fctx)?;
        let (from_ptr, from_len, from_cap) = self.string_parts(&from, args[1].span, fctx)?;
        let (to_ptr, to_len, to_cap) = self.string_parts(&to, args[2].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_string_replace(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            s_ptr,
            s_len,
            s_cap,
            from_ptr,
            from_len,
            from_cap,
            to_ptr,
            to_len,
            to_cap,
            out_ptr_slot,
            out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_string_repeat_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "repeat expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let s = self.gen_expr(&args[0], fctx)?;
        let count = self.gen_expr(&args[1], fctx)?;
        if s.ty != LType::String || count.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "repeat expects (String, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (s_ptr, s_len, s_cap) = self.string_parts(&s, args[0].span, fctx)?;
        let count_repr = count.repr.clone().unwrap_or_else(|| "0".to_string());
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_string_repeat(i8* {}, i64 {}, i64 {}, i64 {}, i8** {}, i64* {})",
            s_ptr, s_len, s_cap, count_repr, out_ptr_slot, out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_string_parse_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "parse_int expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let s = self.gen_expr(&args[0], fctx)?;
        if s.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "parse_int expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (s_ptr, s_len, s_cap) = self.string_parts(&s, args[0].span, fctx)?;
        let out_value_slot = self.new_temp();
        let out_err_ptr_slot = self.new_temp();
        let out_err_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_value_slot));
        fctx.lines
            .push(format!("  {} = alloca i8*", out_err_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", out_err_len_slot));
        let status = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_string_parse_int(i8* {}, i64 {}, i64 {}, i64* {}, i8** {}, i64* {})",
            status, s_ptr, s_len, s_cap, out_value_slot, out_err_ptr_slot, out_err_len_slot
        ));
        let out_value = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_value, out_value_slot
        ));
        let out_err_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_err_ptr, out_err_ptr_slot
        ));
        let out_err_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_err_len, out_err_len_slot
        ));

        let result_ty = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("unknown function '{name}' in codegen"),
                    self.file,
                    span,
                ));
                None
            })?;
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(&result_ty, span)
        else {
            return None;
        };
        if ok_ty != LType::Int || err_ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "parse_int expects Result[Int, String] return type",
                self.file,
                span,
            ));
            return None;
        }

        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_value),
        };
        let err_payload = self.build_string_value(&out_err_ptr, &out_err_len, &out_err_len, fctx);
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(&result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, status));
        let ok_label = self.new_label("string_parse_ok");
        let err_label = self.new_label("string_parse_err");
        let cont_label = self.new_label("string_parse_cont");
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

    pub(super) fn gen_string_parse_float_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "parse_float expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let s = self.gen_expr(&args[0], fctx)?;
        if s.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "parse_float expects String",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (s_ptr, s_len, s_cap) = self.string_parts(&s, args[0].span, fctx)?;
        let out_value_slot = self.new_temp();
        let out_err_ptr_slot = self.new_temp();
        let out_err_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca double", out_value_slot));
        fctx.lines
            .push(format!("  {} = alloca i8*", out_err_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", out_err_len_slot));
        let status = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_string_parse_float(i8* {}, i64 {}, i64 {}, double* {}, i8** {}, i64* {})",
            status, s_ptr, s_len, s_cap, out_value_slot, out_err_ptr_slot, out_err_len_slot
        ));
        let out_value = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load double, double* {}",
            out_value, out_value_slot
        ));
        let out_err_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_err_ptr, out_err_ptr_slot
        ));
        let out_err_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_err_len, out_err_len_slot
        ));

        let result_ty = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("unknown function '{name}' in codegen"),
                    self.file,
                    span,
                ));
                None
            })?;
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(&result_ty, span)
        else {
            return None;
        };
        if ok_ty != LType::Float || err_ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "parse_float expects Result[Float, String] return type",
                self.file,
                span,
            ));
            return None;
        }

        let ok_payload = Value {
            ty: LType::Float,
            repr: Some(out_value),
        };
        let err_payload = self.build_string_value(&out_err_ptr, &out_err_len, &out_err_len, fctx);
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(&result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, status));
        let ok_label = self.new_label("string_parse_float_ok");
        let err_label = self.new_label("string_parse_float_err");
        let cont_label = self.new_label("string_parse_float_cont");
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

    pub(super) fn gen_string_result_string_unary_call(
        &mut self,
        fn_name: &str,
        display_name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{display_name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{display_name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (value_ptr, value_len, value_cap) = self.string_parts(&value, args[0].span, fctx)?;
        let out_ok_ptr_slot = self.new_temp();
        let out_ok_len_slot = self.new_temp();
        let out_err_ptr_slot = self.new_temp();
        let out_err_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_ok_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", out_ok_len_slot));
        fctx.lines
            .push(format!("  {} = alloca i8*", out_err_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", out_err_len_slot));
        let status = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i8** {}, i64* {}, i8** {}, i64* {})",
            status,
            runtime_fn,
            value_ptr,
            value_len,
            value_cap,
            out_ok_ptr_slot,
            out_ok_len_slot,
            out_err_ptr_slot,
            out_err_len_slot
        ));
        let out_ok_ptr = self.new_temp();
        let out_ok_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_ok_ptr, out_ok_ptr_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_ok_len, out_ok_len_slot
        ));
        let out_err_ptr = self.new_temp();
        let out_err_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_err_ptr, out_err_ptr_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_err_len, out_err_len_slot
        ));

        let result_ty = self
            .fn_sigs
            .get(fn_name)
            .map(|sig| sig.ret.clone())
            .or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("unknown function '{fn_name}' in codegen"),
                    self.file,
                    span,
                ));
                None
            })?;
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(&result_ty, span)
        else {
            return None;
        };
        if ok_ty != LType::String || err_ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{display_name} expects Result[String, String] return type"),
                self.file,
                span,
            ));
            return None;
        }

        let ok_payload = self.build_string_value(&out_ok_ptr, &out_ok_len, &out_ok_len, fctx);
        let err_payload = self.build_string_value(&out_err_ptr, &out_err_len, &out_err_len, fctx);
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(&result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, status));
        let ok_label = self.new_label("result_string_unary_ok");
        let err_label = self.new_label("result_string_unary_err");
        let cont_label = self.new_label("result_string_unary_cont");
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

    pub(super) fn gen_string_result_string_binary_call(
        &mut self,
        fn_name: &str,
        display_name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{display_name} expects two arguments"),
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
                format!("{display_name} expects (String, String)"),
                self.file,
                span,
            ));
            return None;
        }
        let (lhs_ptr, lhs_len, lhs_cap) = self.string_parts(&lhs, args[0].span, fctx)?;
        let (rhs_ptr, rhs_len, rhs_cap) = self.string_parts(&rhs, args[1].span, fctx)?;
        let out_ok_ptr_slot = self.new_temp();
        let out_ok_len_slot = self.new_temp();
        let out_err_ptr_slot = self.new_temp();
        let out_err_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_ok_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", out_ok_len_slot));
        fctx.lines
            .push(format!("  {} = alloca i8*", out_err_ptr_slot));
        fctx.lines
            .push(format!("  {} = alloca i64", out_err_len_slot));
        let status = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {}, i8** {}, i64* {})",
            status,
            runtime_fn,
            lhs_ptr,
            lhs_len,
            lhs_cap,
            rhs_ptr,
            rhs_len,
            rhs_cap,
            out_ok_ptr_slot,
            out_ok_len_slot,
            out_err_ptr_slot,
            out_err_len_slot
        ));
        let out_ok_ptr = self.new_temp();
        let out_ok_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_ok_ptr, out_ok_ptr_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_ok_len, out_ok_len_slot
        ));
        let out_err_ptr = self.new_temp();
        let out_err_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_err_ptr, out_err_ptr_slot
        ));
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_err_len, out_err_len_slot
        ));

        let result_ty = self
            .fn_sigs
            .get(fn_name)
            .map(|sig| sig.ret.clone())
            .or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E5012",
                    format!("unknown function '{fn_name}' in codegen"),
                    self.file,
                    span,
                ));
                None
            })?;
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(&result_ty, span)
        else {
            return None;
        };
        if ok_ty != LType::String || err_ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{display_name} expects Result[String, String] return type"),
                self.file,
                span,
            ));
            return None;
        }

        let ok_payload = self.build_string_value(&out_ok_ptr, &out_ok_len, &out_ok_len, fctx);
        let err_payload = self.build_string_value(&out_err_ptr, &out_err_len, &out_err_len, fctx);
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(&result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, status));
        let ok_label = self.new_label("result_string_binary_ok");
        let err_label = self.new_label("result_string_binary_err");
        let cont_label = self.new_label("result_string_binary_cont");
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

    pub(super) fn gen_string_int_to_string_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "int_to_string expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "int_to_string expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let value_repr = value.repr.clone().unwrap_or_else(|| "0".to_string());
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_string_int_to_string(i64 {}, i8** {}, i64* {})",
            value_repr, out_ptr_slot, out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_string_float_to_string_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "float_to_string expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Float {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "float_to_string expects Float",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let value_repr = value
            .repr
            .clone()
            .unwrap_or_else(|| llvm_float_literal(0.0_f64));
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_string_float_to_string(double {}, i8** {}, i64* {})",
            value_repr, out_ptr_slot, out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_string_bool_to_string_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "bool_to_string expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "bool_to_string expects Bool",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let bool_repr = value.repr.clone().unwrap_or_else(|| "0".to_string());
        let bool_i64 = self.new_temp();
        fctx.lines
            .push(format!("  {} = zext i1 {} to i64", bool_i64, bool_repr));
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_string_bool_to_string(i64 {}, i8** {}, i64* {})",
            bool_i64, out_ptr_slot, out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_string_join_call(
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
        let parts = self.gen_expr(&args[0], fctx)?;
        let separator = self.gen_expr(&args[1], fctx)?;
        if separator.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "join expects (Vec[String], String)",
                self.file,
                span,
            ));
            return None;
        }
        let (parts_ptr_int, parts_len, parts_cap) = self.vec_parts(&parts, args[0].span, fctx)?;
        let parts_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = inttoptr i64 {} to i8*",
            parts_ptr, parts_ptr_int
        ));
        let (sep_ptr, sep_len, sep_cap) = self.string_parts(&separator, args[1].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_string_join(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            parts_ptr, parts_len, parts_cap, sep_ptr, sep_len, sep_cap, out_ptr_slot, out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_string_format_call(
        &mut self,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "format expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let template = self.gen_expr(&args[0], fctx)?;
        let format_args = self.gen_expr(&args[1], fctx)?;
        if template.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "format expects (String, Vec[String])",
                self.file,
                span,
            ));
            return None;
        }
        let (template_ptr, template_len, template_cap) =
            self.string_parts(&template, args[0].span, fctx)?;
        let (args_ptr_int, args_len, args_cap) =
            self.vec_parts(&format_args, args[1].span, fctx)?;
        let args_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = inttoptr i64 {} to i8*",
            args_ptr, args_ptr_int
        ));
        let out_ptr_slot = self.new_temp();
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        fctx.lines.push(format!(
            "  call void @aic_rt_string_format(i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            template_ptr,
            template_len,
            template_cap,
            args_ptr,
            args_len,
            args_cap,
            out_ptr_slot,
            out_len_slot
        ));
        self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)
    }

    pub(super) fn gen_math_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "abs" | "aic_math_abs_intrinsic" => "abs",
            "abs_float" | "aic_math_abs_float_intrinsic" => "abs_float",
            "min" | "aic_math_min_intrinsic" => "min",
            "max" | "aic_math_max_intrinsic" => "max",
            "pow" | "aic_math_pow_intrinsic" => "pow",
            "sqrt" | "aic_math_sqrt_intrinsic" => "sqrt",
            "floor" | "aic_math_floor_intrinsic" => "floor",
            "ceil" | "aic_math_ceil_intrinsic" => "ceil",
            "round" | "aic_math_round_intrinsic" => "round",
            "log" | "aic_math_log_intrinsic" => "log",
            "sin" | "aic_math_sin_intrinsic" => "sin",
            "cos" | "aic_math_cos_intrinsic" => "cos",
            _ => return None,
        };

        match canonical {
            "abs" if self.sig_matches_shape(name, &["Int"], "Int") => {
                Some(self.gen_math_unary_int_call(name, "aic_rt_math_abs", args, span, fctx))
            }
            "abs_float" if self.sig_matches_shape(name, &["Float"], "Float") => Some(
                self.gen_math_unary_float_call(name, "aic_rt_math_abs_float", args, span, fctx),
            ),
            "min" if self.sig_matches_shape(name, &["Int", "Int"], "Int") => {
                Some(self.gen_math_binary_int_call(name, "aic_rt_math_min", args, span, fctx))
            }
            "max" if self.sig_matches_shape(name, &["Int", "Int"], "Int") => {
                Some(self.gen_math_binary_int_call(name, "aic_rt_math_max", args, span, fctx))
            }
            "pow" if self.sig_matches_shape(name, &["Float", "Float"], "Float") => {
                Some(self.gen_math_binary_float_call(name, "aic_rt_math_pow", args, span, fctx))
            }
            "sqrt" if self.sig_matches_shape(name, &["Float"], "Float") => {
                Some(self.gen_math_unary_float_call(name, "aic_rt_math_sqrt", args, span, fctx))
            }
            "floor" if self.sig_matches_shape(name, &["Float"], "Int") => Some(
                self.gen_math_unary_float_to_int_call(name, "aic_rt_math_floor", args, span, fctx),
            ),
            "ceil" if self.sig_matches_shape(name, &["Float"], "Int") => Some(
                self.gen_math_unary_float_to_int_call(name, "aic_rt_math_ceil", args, span, fctx),
            ),
            "round" if self.sig_matches_shape(name, &["Float"], "Int") => Some(
                self.gen_math_unary_float_to_int_call(name, "aic_rt_math_round", args, span, fctx),
            ),
            "log" if self.sig_matches_shape(name, &["Float"], "Float") => {
                Some(self.gen_math_unary_float_call(name, "aic_rt_math_log", args, span, fctx))
            }
            "sin" if self.sig_matches_shape(name, &["Float"], "Float") => {
                Some(self.gen_math_unary_float_call(name, "aic_rt_math_sin", args, span, fctx))
            }
            "cos" if self.sig_matches_shape(name, &["Float"], "Float") => {
                Some(self.gen_math_unary_float_call(name, "aic_rt_math_cos", args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_math_unary_int_call(
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
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let arg = value.repr.unwrap_or_else(|| "0".to_string());
        let reg = self.new_temp();
        fctx.lines
            .push(format!("  {} = call i64 @{}(i64 {})", reg, runtime_fn, arg));
        Some(Value {
            ty: LType::Int,
            repr: Some(reg),
        })
    }

    pub(super) fn gen_math_binary_int_call(
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
        if lhs.ty != LType::Int || rhs.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects (Int, Int)"),
                self.file,
                span,
            ));
            return None;
        }
        let lhs_arg = lhs.repr.unwrap_or_else(|| "0".to_string());
        let rhs_arg = rhs.repr.unwrap_or_else(|| "0".to_string());
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64 {})",
            reg, runtime_fn, lhs_arg, rhs_arg
        ));
        Some(Value {
            ty: LType::Int,
            repr: Some(reg),
        })
    }

    pub(super) fn gen_math_unary_float_call(
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
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Float {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Float"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let arg = value.repr.unwrap_or_else(|| llvm_float_literal(0.0_f64));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call double @{}(double {})",
            reg, runtime_fn, arg
        ));
        Some(Value {
            ty: LType::Float,
            repr: Some(reg),
        })
    }

    pub(super) fn gen_math_binary_float_call(
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
        if lhs.ty != LType::Float || rhs.ty != LType::Float {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects (Float, Float)"),
                self.file,
                span,
            ));
            return None;
        }
        let lhs_arg = lhs.repr.unwrap_or_else(|| llvm_float_literal(0.0_f64));
        let rhs_arg = rhs.repr.unwrap_or_else(|| llvm_float_literal(0.0_f64));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call double @{}(double {}, double {})",
            reg, runtime_fn, lhs_arg, rhs_arg
        ));
        Some(Value {
            ty: LType::Float,
            repr: Some(reg),
        })
    }

    pub(super) fn gen_math_unary_float_to_int_call(
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
        let value = self.gen_expr(&args[0], fctx)?;
        if value.ty != LType::Float {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Float"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let arg = value.repr.unwrap_or_else(|| llvm_float_literal(0.0_f64));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(double {})",
            reg, runtime_fn, arg
        ));
        Some(Value {
            ty: LType::Int,
            repr: Some(reg),
        })
    }

    pub(super) fn wrap_option_with_condition(
        &mut self,
        option_ty: &LType,
        some_payload: Value,
        has_value: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((layout, payload_ty, some_index, none_index)) =
            self.option_layout_parts(option_ty, span)
        else {
            return None;
        };
        if some_payload.ty != payload_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "option payload expects '{}', found '{}'",
                    render_type(&payload_ty),
                    render_type(&some_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let some_value =
            self.build_enum_variant(&layout, some_index, Some(some_payload), span, fctx)?;
        let none_value = self.build_enum_variant(&layout, none_index, None, span, fctx)?;

        let slot = self.alloc_entry_slot(option_ty, fctx);
        let some_label = self.new_label("option_some");
        let none_label = self.new_label("option_none");
        let cont_label = self.new_label("option_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            has_value, some_label, none_label
        ));

        fctx.lines.push(format!("{}:", some_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(option_ty),
            some_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(option_ty)),
            llvm_type(option_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", none_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(option_ty),
            none_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(option_ty)),
            llvm_type(option_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(option_ty),
            llvm_type(option_ty),
            slot
        ));
        Some(Value {
            ty: option_ty.clone(),
            repr: Some(reg),
        })
    }

    pub(super) fn option_layout_parts(
        &mut self,
        option_ty: &LType,
        span: crate::span::Span,
    ) -> Option<(EnumLayoutType, LType, usize, usize)> {
        let LType::Enum(layout) = option_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "builtin expects Option return type",
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Option" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "builtin expects Option return type, found '{}'",
                    layout.repr
                ),
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
                "Option return type is missing Some variant",
                self.file,
                span,
            ));
            return None;
        };
        let Some(none_index) = layout
            .variants
            .iter()
            .position(|variant| variant.name == "None")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Option return type is missing None variant",
                self.file,
                span,
            ));
            return None;
        };
        let Some(payload_ty) = layout.variants[some_index].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Option Some variant must have a payload",
                self.file,
                span,
            ));
            return None;
        };
        Some((layout.clone(), payload_ty, some_index, none_index))
    }

    pub(super) fn load_string_from_out_slots(
        &mut self,
        out_ptr_slot: &str,
        out_len_slot: &str,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let out_ptr = self.new_temp();
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        Some(self.build_string_value(&out_ptr, &out_len, &out_len, fctx))
    }

    pub(super) fn vec_parts(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String)> {
        let LType::Struct(layout) = value.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("expected Vec value, found '{}'", render_type(&value.ty)),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "Vec" {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("expected Vec value, found '{}'", layout.repr),
                self.file,
                span,
            ));
            return None;
        }
        let Some((ptr_index, ptr_field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == "ptr")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Vec is missing ptr field",
                self.file,
                span,
            ));
            return None;
        };
        let Some((len_index, len_field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == "len")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Vec is missing len field",
                self.file,
                span,
            ));
            return None;
        };
        let Some((cap_index, cap_field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == "cap")
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Vec is missing cap field",
                self.file,
                span,
            ));
            return None;
        };
        if ptr_field.ty != LType::Int || len_field.ty != LType::Int || cap_field.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "Vec fields ptr/len/cap must be Int",
                self.file,
                span,
            ));
            return None;
        }

        let vec_repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let ptr_reg = self.new_temp();
        let len_reg = self.new_temp();
        let cap_reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            ptr_reg,
            llvm_type(&value.ty),
            vec_repr,
            ptr_index
        ));
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            len_reg,
            llvm_type(&value.ty),
            value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&value.ty)),
            len_index
        ));
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            cap_reg,
            llvm_type(&value.ty),
            value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&value.ty)),
            cap_index
        ));
        Some((ptr_reg, len_reg, cap_reg))
    }

    pub(super) fn build_vec_string_from_raw_parts(
        &mut self,
        expected_ty: &LType,
        items_ptr: &str,
        count: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let payload = self.build_vec_string_payload_from_ptr(items_ptr, count, span, fctx)?;
        if payload.ty != *expected_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "split expects return type '{}', found '{}'",
                    render_type(&payload.ty),
                    render_type(expected_ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        Some(payload)
    }

    pub(super) fn build_vec_string_payload_from_ptr(
        &mut self,
        items_ptr: &str,
        count: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some(vec_ty) = self.parse_type_repr("Vec[String]", span) else {
            return None;
        };
        let LType::Struct(layout) = vec_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "expected Vec[String] layout in codegen",
                self.file,
                span,
            ));
            return None;
        };
        let ptr_as_int = self.new_temp();
        fctx.lines.push(format!(
            "  {} = ptrtoint i8* {} to i64",
            ptr_as_int, items_ptr
        ));
        self.build_struct_value(
            &layout,
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
}
