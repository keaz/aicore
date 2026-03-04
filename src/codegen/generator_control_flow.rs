use super::*;

impl<'a> Generator<'a> {
    fn store_match_arm_value(
        &mut self,
        value: Value,
        arm_span: crate::span::Span,
        expected_hint: Option<&LType>,
        result_slot: &mut Option<String>,
        result_ty: &mut Option<LType>,
        fctx: &mut FnCtx,
    ) -> bool {
        if value.ty == LType::Unit {
            return true;
        }

        if result_slot.is_none() {
            let slot_ty = expected_hint
                .filter(|ty| **ty != LType::Unit)
                .cloned()
                .unwrap_or_else(|| value.ty.clone());
            let ptr = self.alloc_entry_slot(&slot_ty, fctx);
            *result_ty = Some(slot_ty);
            *result_slot = Some(ptr);
        }

        let (Some(slot), Some(expected_ty)) = (result_slot.as_ref(), result_ty.as_ref()) else {
            return false;
        };

        let Some(value) = self.coerce_value_to_expected(value, expected_ty, arm_span, fctx) else {
            return false;
        };
        if value.ty != *expected_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5016",
                "match arms resolved to incompatible types",
                self.file,
                arm_span,
            ));
            return false;
        }

        let repr = coerce_repr(&value, expected_ty);
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(expected_ty),
            repr,
            llvm_type(expected_ty),
            slot
        ));
        true
    }

    pub(super) fn build_no_payload_enum_with_tag(
        &mut self,
        layout: &EnumLayoutType,
        tag: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if layout
            .variants
            .iter()
            .any(|variant| variant.payload.is_some())
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "expected no-payload enum layout",
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
            tag
        ));
        for idx in 0..layout.variants.len() {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {} = insertvalue {} {}, i8 0, {}",
                reg,
                llvm_type(&ty),
                acc,
                idx + 1
            ));
            acc = reg;
        }
        Some(Value {
            ty,
            repr: Some(acc),
        })
    }

    pub(super) fn gen_struct_init(
        &mut self,
        name: &str,
        fields: &[(String, ir::Expr, crate::span::Span)],
        expected_ty: Option<&LType>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if name == TUPLE_INTERNAL_NAME {
            return self.gen_tuple_init(fields, expected_ty, span, fctx);
        }
        let Some(template) = self.struct_templates.get(name).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                "E5004",
                format!("unknown struct '{}' in codegen", name),
                self.file,
                span,
            ));
            return None;
        };
        let expected_layout = if let Some(LType::Struct(layout)) = expected_ty {
            if base_type_name(&layout.repr) == name {
                Some(layout.clone())
            } else {
                None
            }
        } else if template.generics.is_empty() {
            match self.parse_type_repr(name, span) {
                Some(LType::Struct(layout)) if base_type_name(&layout.repr) == name => Some(layout),
                _ => None,
            }
        } else {
            None
        };

        let mut provided = BTreeMap::new();
        for (field_name, field_expr, field_span) in fields {
            if provided.contains_key(field_name) {
                self.diagnostics.push(Diagnostic::error(
                    "E5004",
                    format!(
                        "duplicate field '{}.{}' in struct literal",
                        name, field_name
                    ),
                    self.file,
                    *field_span,
                ));
                continue;
            }
            let field_expected_from_layout = expected_layout.as_ref().and_then(|layout| {
                layout
                    .fields
                    .iter()
                    .find(|info| info.name == *field_name)
                    .map(|info| &info.ty)
            });
            let value =
                self.gen_expr_with_expected(field_expr, field_expected_from_layout, fctx)?;
            provided.insert(field_name.clone(), (value, *field_span));
        }

        let (ty, layout) = if let Some(layout) = expected_layout.clone() {
            (LType::Struct(layout.clone()), layout)
        } else {
            let mut bindings = BTreeMap::new();
            for (field_name, expected_ty) in &template.fields {
                let Some((value, _)) = provided.get(field_name) else {
                    continue;
                };
                let actual = render_type(&value.ty);
                infer_generic_bindings(expected_ty, &actual, &template.generics, &mut bindings);
            }
            for generic in &template.generics {
                let fallback = self
                    .active_type_bindings
                    .as_ref()
                    .and_then(|map| map.get(generic))
                    .cloned()
                    .unwrap_or_else(|| "Int".to_string());
                bindings.entry(generic.clone()).or_insert(fallback);
            }

            let applied_args = template
                .generics
                .iter()
                .map(|g| {
                    bindings
                        .get(g)
                        .cloned()
                        .unwrap_or_else(|| "Int".to_string())
                })
                .collect::<Vec<_>>();
            let applied_repr = render_applied_type_from_parts(name, &applied_args);
            let ty = self.parse_type_repr(&applied_repr, span)?;
            let LType::Struct(layout) = ty.clone() else {
                self.diagnostics.push(Diagnostic::error(
                    "E5004",
                    format!("failed to lower struct layout for '{}'", applied_repr),
                    self.file,
                    span,
                ));
                return None;
            };
            (ty, layout)
        };

        let mut acc = "undef".to_string();
        for (idx, field) in layout.fields.iter().enumerate() {
            let rendered = if let Some((value, field_span)) = provided.get(&field.name) {
                if value.ty != field.ty {
                    self.diagnostics.push(Diagnostic::error(
                        "E5004",
                        format!(
                            "field '{}.{}' expects '{}', found '{}'",
                            name,
                            field.name,
                            render_type(&field.ty),
                            render_type(&value.ty)
                        ),
                        self.file,
                        *field_span,
                    ));
                    default_value(&field.ty)
                } else {
                    value
                        .repr
                        .clone()
                        .unwrap_or_else(|| default_value(&field.ty))
                }
            } else if let Some(default_expr) = template.field_defaults.get(&field.name) {
                if let Some(default_field_value) =
                    self.eval_struct_field_default(name, &field.name, default_expr, &field.ty, fctx)
                {
                    default_field_value
                        .repr
                        .clone()
                        .unwrap_or_else(|| default_value(&field.ty))
                } else {
                    default_value(&field.ty)
                }
            } else {
                self.diagnostics.push(Diagnostic::error(
                    "E5004",
                    format!("missing field '{}.{}' in struct literal", name, field.name),
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

        let repr = if layout.fields.is_empty() {
            default_value(&ty)
        } else {
            acc
        };
        Some(Value {
            ty,
            repr: Some(repr),
        })
    }

    pub(super) fn gen_tuple_init(
        &mut self,
        fields: &[(String, ir::Expr, crate::span::Span)],
        expected_ty: Option<&LType>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let expected_layout = match expected_ty {
            Some(LType::Struct(layout)) if base_type_name(&layout.repr) == TUPLE_INTERNAL_NAME => {
                Some(layout.clone())
            }
            _ => None,
        };

        let mut provided: BTreeMap<usize, (Value, crate::span::Span)> = BTreeMap::new();
        for (field_name, field_expr, field_span) in fields {
            let Ok(index) = field_name.parse::<usize>() else {
                self.diagnostics.push(Diagnostic::error(
                    "E5004",
                    format!("tuple field '{}' is not a valid numeric index", field_name),
                    self.file,
                    *field_span,
                ));
                continue;
            };
            if provided.contains_key(&index) {
                self.diagnostics.push(Diagnostic::error(
                    "E5004",
                    format!("duplicate tuple element index '{}'", index),
                    self.file,
                    *field_span,
                ));
                continue;
            }
            let expected_field_ty = expected_layout
                .as_ref()
                .and_then(|layout| layout.fields.get(index).map(|field| &field.ty));
            let value = self.gen_expr_with_expected(field_expr, expected_field_ty, fctx)?;
            provided.insert(index, (value, *field_span));
        }

        let (ty, layout) = if let Some(layout) = expected_layout {
            (LType::Struct(layout.clone()), layout)
        } else {
            if provided.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    "E5004",
                    "tuple literal must contain at least one element",
                    self.file,
                    span,
                ));
                return None;
            }
            let max_index = provided.keys().copied().max().unwrap_or(0);
            let mut items = Vec::new();
            for index in 0..=max_index {
                let Some((value, _)) = provided.get(&index) else {
                    self.diagnostics.push(Diagnostic::error(
                        "E5004",
                        format!("tuple literal is missing element index '{}'", index),
                        self.file,
                        span,
                    ));
                    return None;
                };
                items.push(value.ty.clone());
            }
            let fields = items
                .iter()
                .enumerate()
                .map(|(idx, item)| StructFieldType {
                    name: idx.to_string(),
                    ty: item.clone(),
                })
                .collect::<Vec<_>>();
            let layout = StructLayoutType {
                repr: render_applied_type(TUPLE_INTERNAL_NAME, &items),
                fields,
            };
            (LType::Struct(layout.clone()), layout)
        };

        let mut acc = "undef".to_string();
        for (idx, field) in layout.fields.iter().enumerate() {
            let rendered = if let Some((value, field_span)) = provided.get(&idx) {
                if value.ty != field.ty {
                    self.diagnostics.push(Diagnostic::error(
                        "E5004",
                        format!(
                            "tuple element .{} expects '{}', found '{}'",
                            idx,
                            render_type(&field.ty),
                            render_type(&value.ty)
                        ),
                        self.file,
                        *field_span,
                    ));
                    default_value(&field.ty)
                } else {
                    value
                        .repr
                        .clone()
                        .unwrap_or_else(|| default_value(&field.ty))
                }
            } else {
                self.diagnostics.push(Diagnostic::error(
                    "E5004",
                    format!("missing tuple element .{} in tuple literal", idx),
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

        let repr = if layout.fields.is_empty() {
            default_value(&ty)
        } else {
            acc
        };
        Some(Value {
            ty,
            repr: Some(repr),
        })
    }

    pub(super) fn gen_field_access(
        &mut self,
        base: &ir::Expr,
        field: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let value = self.gen_expr(base, fctx)?;
        let LType::Struct(layout) = value.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5005",
                format!(
                    "field access requires struct value, found '{}'",
                    render_type(&value.ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        let Some((index, field_layout)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, info)| info.name == field)
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5005",
                format!("struct '{}' has no field '{}'", layout.repr, field),
                self.file,
                span,
            ));
            return None;
        };

        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            reg,
            llvm_type(&value.ty),
            value.repr.unwrap_or_else(|| default_value(&value.ty)),
            index
        ));
        Some(Value {
            ty: field_layout.ty.clone(),
            repr: Some(reg),
        })
    }

    pub(super) fn gen_variant_constructor(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        expected_ty: Option<&LType>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let Some(mut candidates) = self.variant_ctors.get(name).cloned() else {
            return None;
        };
        let expected_layout = if let Some(LType::Enum(layout)) = expected_ty {
            Some(layout.clone())
        } else {
            None
        };
        if let Some(LType::Enum(layout)) = expected_ty {
            let expected_enum = base_type_name(&layout.repr);
            candidates.retain(|candidate| candidate.enum_name == expected_enum);
        }
        if args.len() > 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5009",
                format!("variant constructor '{}' takes at most one argument", name),
                self.file,
                span,
            ));
            return Some(None);
        }

        let payload_expected_ty = if args.len() == 1 {
            expected_layout.as_ref().and_then(|layout| {
                candidates.iter().find_map(|candidate| {
                    if base_type_name(&layout.repr) != candidate.enum_name {
                        return None;
                    }
                    layout
                        .variants
                        .get(candidate.variant_index)
                        .and_then(|variant| variant.payload.clone())
                })
            })
        } else {
            None
        };
        let payload_value = if args.len() == 1 {
            Some(self.gen_expr_with_expected(&args[0], payload_expected_ty.as_ref(), fctx)?)
        } else {
            None
        };

        let mut chosen: Option<(EnumLayoutType, usize)> = None;
        for candidate in candidates {
            if let Some(layout) = expected_layout.as_ref() {
                if base_type_name(&layout.repr) != candidate.enum_name {
                    continue;
                }
                let Some(variant) = layout.variants.get(candidate.variant_index) else {
                    continue;
                };
                let payload_arity = usize::from(variant.payload.is_some());
                if payload_arity != args.len() {
                    continue;
                }
                chosen = Some((layout.clone(), candidate.variant_index));
                break;
            }
            let Some(template) = self.enum_templates.get(&candidate.enum_name) else {
                continue;
            };
            let Some((_, payload_template)) = template.variants.get(candidate.variant_index) else {
                continue;
            };

            let payload_arity = usize::from(payload_template.is_some());
            if payload_arity != args.len() {
                continue;
            }

            let mut bindings = BTreeMap::new();
            if let (Some(raw_payload), Some(payload)) = (payload_template, payload_value.as_ref()) {
                if !infer_generic_bindings(
                    raw_payload,
                    &render_type(&payload.ty),
                    &template.generics,
                    &mut bindings,
                ) {
                    continue;
                }
            }
            for generic in &template.generics {
                let fallback = self
                    .active_type_bindings
                    .as_ref()
                    .and_then(|map| map.get(generic))
                    .cloned()
                    .unwrap_or_else(|| "Int".to_string());
                bindings.entry(generic.clone()).or_insert(fallback);
            }
            let args = template
                .generics
                .iter()
                .map(|g| {
                    bindings
                        .get(g)
                        .cloned()
                        .unwrap_or_else(|| "Int".to_string())
                })
                .collect::<Vec<_>>();
            let repr = render_applied_type_from_parts(&candidate.enum_name, &args);
            let Some(LType::Enum(layout)) = self.parse_type_repr(&repr, span) else {
                continue;
            };
            chosen = Some((layout, candidate.variant_index));
            break;
        }

        let Some((layout, variant_index)) = chosen else {
            self.diagnostics.push(Diagnostic::error(
                "E5009",
                format!("no viable enum constructor overload for '{}'", name),
                self.file,
                span,
            ));
            return Some(None);
        };

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
                if idx == variant_index {
                    if let Some(payload) = payload_value.as_ref() {
                        if let Some(payload) =
                            self.coerce_value_to_expected(payload.clone(), payload_ty, span, fctx)
                        {
                            if self.types_compatible_for_codegen(payload_ty, &payload.ty, span) {
                                (
                                    llvm_type(payload_ty),
                                    payload
                                        .repr
                                        .clone()
                                        .unwrap_or_else(|| default_value(payload_ty)),
                                )
                            } else {
                                self.diagnostics.push(Diagnostic::error(
                                    "E5009",
                                    format!(
                                        "variant '{}' payload expects '{}', found '{}'",
                                        name,
                                        render_type(payload_ty),
                                        render_type(&payload.ty)
                                    ),
                                    self.file,
                                    span,
                                ));
                                (llvm_type(payload_ty), default_value(payload_ty))
                            }
                        } else {
                            self.diagnostics.push(Diagnostic::error(
                                "E5009",
                                format!(
                                    "variant '{}' payload could not be coerced to '{}'",
                                    name,
                                    render_type(payload_ty)
                                ),
                                self.file,
                                span,
                            ));
                            (llvm_type(payload_ty), default_value(payload_ty))
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5009",
                            format!("variant '{}' expects a payload argument", name),
                            self.file,
                            span,
                        ));
                        (llvm_type(payload_ty), default_value(payload_ty))
                    }
                } else {
                    (llvm_type(payload_ty), default_value(payload_ty))
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

        Some(Some(Value {
            ty,
            repr: Some(acc),
        }))
    }

    pub(super) fn gen_try(
        &mut self,
        inner_expr: &ir::Expr,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let result = self.gen_expr(inner_expr, fctx)?;
        let LType::Enum(result_layout) = result.ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                format!(
                    "`?` expects Result[T, E] in codegen, found '{}'",
                    render_type(&result.ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&result_layout.repr) != "Result" {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                format!(
                    "`?` expects Result[T, E] in codegen, found '{}'",
                    result_layout.repr
                ),
                self.file,
                span,
            ));
            return None;
        }
        let Some(ok_idx) = result_layout.variants.iter().position(|v| v.name == "Ok") else {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                "Result layout missing Ok variant for `?`",
                self.file,
                span,
            ));
            return None;
        };
        let Some(err_idx) = result_layout.variants.iter().position(|v| v.name == "Err") else {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                "Result layout missing Err variant for `?`",
                self.file,
                span,
            ));
            return None;
        };
        let Some(ok_payload_ty) = result_layout.variants[ok_idx].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                "Result Ok variant must carry a payload for `?`",
                self.file,
                span,
            ));
            return None;
        };
        let Some(err_payload_ty) = result_layout.variants[err_idx].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5021",
                "Result Err variant must carry a payload for `?`",
                self.file,
                span,
            ));
            return None;
        };

        let LType::Enum(fn_ret_layout) = fctx.ret_ty.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5022",
                format!(
                    "`?` requires Result return type in enclosing function, found '{}'",
                    render_type(&fctx.ret_ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&fn_ret_layout.repr) != "Result" {
            self.diagnostics.push(Diagnostic::error(
                "E5022",
                format!(
                    "`?` requires Result return type in enclosing function, found '{}'",
                    fn_ret_layout.repr
                ),
                self.file,
                span,
            ));
            return None;
        }
        let Some(fn_err_idx) = fn_ret_layout.variants.iter().position(|v| v.name == "Err") else {
            self.diagnostics.push(Diagnostic::error(
                "E5022",
                "enclosing Result return type missing Err variant for `?`",
                self.file,
                span,
            ));
            return None;
        };
        let Some(fn_err_payload_ty) = fn_ret_layout.variants[fn_err_idx].payload.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5022",
                "enclosing Result Err variant must carry a payload for `?`",
                self.file,
                span,
            ));
            return None;
        };
        if err_payload_ty != fn_err_payload_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5022",
                format!(
                    "`?` error type mismatch in codegen: expression has '{}', function expects '{}'",
                    render_type(&err_payload_ty),
                    render_type(&fn_err_payload_ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let result_repr = result
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&result.ty));
        let tag = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            tag,
            llvm_type(&result.ty),
            result_repr.as_str()
        ));
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i32 {}, {}", is_ok, tag, ok_idx));
        let ok_label = self.new_label("try_ok");
        let err_label = self.new_label("try_err");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", err_label));
        let err_payload = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            err_payload,
            llvm_type(&result.ty),
            result_repr.as_str(),
            err_idx + 1
        ));
        let err_value = Value {
            ty: err_payload_ty,
            repr: Some(err_payload),
        };
        let ret_enum = self.build_enum_variant_value(
            &fn_ret_layout,
            fn_err_idx,
            Some(&err_value),
            span,
            fctx,
        )?;
        self.emit_scope_drops_to_depth(0, fctx);
        fctx.lines
            .push(format!("  ret {} {}", llvm_type(&fctx.ret_ty), ret_enum));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.current_label = ok_label;
        if ok_payload_ty == LType::Unit {
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        let ok_payload = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            ok_payload,
            llvm_type(&result.ty),
            result_repr.as_str(),
            ok_idx + 1
        ));
        Some(Value {
            ty: ok_payload_ty,
            repr: Some(ok_payload),
        })
    }

    pub(super) fn build_enum_variant_value(
        &mut self,
        layout: &EnumLayoutType,
        variant_index: usize,
        payload: Option<&Value>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<String> {
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
                    if let Some(value) = payload {
                        if value.ty == *payload_ty {
                            let slot_repr = if *payload_ty == LType::Unit {
                                "0".to_string()
                            } else {
                                value
                                    .repr
                                    .clone()
                                    .unwrap_or_else(|| default_value(payload_ty))
                            };
                            (slot_ty_for_payload, slot_repr)
                        } else {
                            self.diagnostics.push(Diagnostic::error(
                                "E5022",
                                format!(
                                    "variant '{}' payload expects '{}', found '{}'",
                                    variant.name,
                                    render_type(payload_ty),
                                    render_type(&value.ty)
                                ),
                                self.file,
                                span,
                            ));
                            return None;
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5022",
                            format!(
                                "variant '{}' requires payload in `?` lowering",
                                variant.name
                            ),
                            self.file,
                            span,
                        ));
                        return None;
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
        Some(acc)
    }

    pub(super) fn gen_while(
        &mut self,
        cond_expr: &ir::Expr,
        body: &ir::Block,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let cond_label = self.new_label("while_cond");
        let body_label = self.new_label("while_body");
        let exit_label = self.new_label("while_exit");

        let saved_scope = fctx.vars.clone();
        let saved_drop_scopes = fctx.drop_scopes.clone();
        let saved_terminated = fctx.terminated;

        fctx.lines.push(format!("  br label %{}", cond_label));
        fctx.loop_stack.push(LoopFrame {
            break_label: exit_label.clone(),
            continue_label: cond_label.clone(),
            result_ty: None,
            result_slot: None,
            scope_depth: saved_drop_scopes.len(),
        });

        fctx.vars = saved_scope.clone();
        fctx.drop_scopes = saved_drop_scopes.clone();
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", cond_label));
        fctx.current_label = cond_label.clone();
        let cond = self.gen_expr(cond_expr, fctx)?;
        if cond.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5015",
                "while condition must be Bool in codegen",
                self.file,
                cond_expr.span,
            ));
            return None;
        }
        if !fctx.terminated {
            let cond_repr = cond.repr.unwrap_or_else(|| "0".to_string());
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                cond_repr, body_label, exit_label
            ));
        }

        fctx.vars = saved_scope.clone();
        fctx.drop_scopes = saved_drop_scopes.clone();
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", body_label));
        fctx.current_label = body_label.clone();
        let _ = self.gen_block(body, fctx);
        if !fctx.terminated {
            fctx.lines.push(format!("  br label %{}", cond_label));
        }

        let frame = fctx.loop_stack.pop().expect("loop frame");
        fctx.vars = saved_scope;
        fctx.drop_scopes = saved_drop_scopes;
        fctx.terminated = saved_terminated;
        fctx.lines.push(format!("{}:", exit_label));
        fctx.current_label = exit_label;

        if let (Some(slot), Some(result_ty)) = (frame.result_slot, frame.result_ty) {
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

    pub(super) fn gen_loop(&mut self, body: &ir::Block, fctx: &mut FnCtx) -> Option<Value> {
        let body_label = self.new_label("loop_body");
        let exit_label = self.new_label("loop_exit");

        let saved_scope = fctx.vars.clone();
        let saved_drop_scopes = fctx.drop_scopes.clone();
        let saved_terminated = fctx.terminated;

        fctx.lines.push(format!("  br label %{}", body_label));
        fctx.loop_stack.push(LoopFrame {
            break_label: exit_label.clone(),
            continue_label: body_label.clone(),
            result_ty: None,
            result_slot: None,
            scope_depth: saved_drop_scopes.len(),
        });

        fctx.vars = saved_scope.clone();
        fctx.drop_scopes = saved_drop_scopes.clone();
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", body_label));
        fctx.current_label = body_label.clone();
        let _ = self.gen_block(body, fctx);
        if !fctx.terminated {
            fctx.lines.push(format!("  br label %{}", body_label));
        }

        let frame = fctx.loop_stack.pop().expect("loop frame");
        fctx.vars = saved_scope;
        fctx.drop_scopes = saved_drop_scopes;
        fctx.terminated = saved_terminated;
        fctx.lines.push(format!("{}:", exit_label));
        fctx.current_label = exit_label;

        if let (Some(slot), Some(result_ty)) = (frame.result_slot, frame.result_ty) {
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

    pub(super) fn gen_break(
        &mut self,
        break_expr: Option<&ir::Expr>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if fctx.loop_stack.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5025",
                "`break` used outside of a loop during codegen",
                self.file,
                span,
            ));
            return None;
        }

        let loop_index = fctx.loop_stack.len() - 1;
        if let Some(break_expr) = break_expr {
            let value = self.gen_expr(break_expr, fctx)?;
            if fctx.loop_stack[loop_index].result_slot.is_none() {
                let ptr = self.alloc_entry_slot(&value.ty, fctx);
                fctx.loop_stack[loop_index].result_ty = Some(value.ty.clone());
                fctx.loop_stack[loop_index].result_slot = Some(ptr);
            }

            let expected_ty = fctx.loop_stack[loop_index]
                .result_ty
                .clone()
                .unwrap_or(LType::Unit);
            if value.ty != expected_ty {
                self.diagnostics.push(Diagnostic::error(
                    "E5007",
                    format!(
                        "loop break type mismatch in codegen: expected '{}', found '{}'",
                        render_type(&expected_ty),
                        render_type(&value.ty)
                    ),
                    self.file,
                    span,
                ));
            }
            if let Some(slot) = fctx.loop_stack[loop_index].result_slot.clone() {
                let repr = coerce_repr(&value, &expected_ty);
                fctx.lines.push(format!(
                    "  store {} {}, {}* {}",
                    llvm_type(&expected_ty),
                    repr,
                    llvm_type(&expected_ty),
                    slot
                ));
            }
        } else if let (Some(slot), Some(expected_ty)) = (
            fctx.loop_stack[loop_index].result_slot.clone(),
            fctx.loop_stack[loop_index].result_ty.clone(),
        ) {
            let repr = default_value(&expected_ty);
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                llvm_type(&expected_ty),
                repr,
                llvm_type(&expected_ty),
                slot
            ));
        }

        let scope_depth = fctx.loop_stack[loop_index].scope_depth;
        self.emit_scope_drops_to_depth(scope_depth, fctx);
        let break_label = fctx.loop_stack[loop_index].break_label.clone();
        fctx.lines.push(format!("  br label %{}", break_label));
        fctx.terminated = true;
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    pub(super) fn gen_continue(
        &mut self,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((scope_depth, continue_label)) = fctx
            .loop_stack
            .last()
            .map(|frame| (frame.scope_depth, frame.continue_label.clone()))
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5026",
                "`continue` used outside of a loop during codegen",
                self.file,
                span,
            ));
            return None;
        };
        self.emit_scope_drops_to_depth(scope_depth, fctx);
        fctx.lines.push(format!("  br label %{}", continue_label));
        fctx.terminated = true;
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    pub(super) fn gen_if(
        &mut self,
        cond_expr: &ir::Expr,
        then_block: &ir::Block,
        else_block: &ir::Block,
        expected_ty: Option<&LType>,
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

        let mut result_ty: Option<LType> = None;
        let mut result_slot: Option<String> = None;

        let cond_repr = cond.repr.unwrap_or_else(|| "0".to_string());
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            cond_repr, then_label, else_label
        ));

        // Then branch
        let saved_scope = fctx.vars.clone();
        let saved_drop_scopes = fctx.drop_scopes.clone();
        let saved_terminated = fctx.terminated;
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", then_label));
        fctx.current_label = then_label.clone();
        let then_value = self.gen_block_with_expected_tail(then_block, expected_ty, fctx);
        let then_terminated = fctx.terminated;
        if !then_terminated {
            if let Some(value) = then_value {
                if value.ty != LType::Unit {
                    if result_slot.is_none() {
                        let ptr = self.alloc_entry_slot(&value.ty, fctx);
                        result_ty = Some(value.ty.clone());
                        result_slot = Some(ptr);
                    }
                    if let (Some(slot), Some(expected_ty)) =
                        (result_slot.as_ref(), result_ty.as_ref())
                    {
                        if value.ty != *expected_ty {
                            self.diagnostics.push(Diagnostic::error(
                                "E5007",
                                "if expression branches resolved to incompatible types",
                                self.file,
                                then_block.span,
                            ));
                        }
                        let repr = coerce_repr(&value, expected_ty);
                        fctx.lines.push(format!(
                            "  store {} {}, {}* {}",
                            llvm_type(expected_ty),
                            repr,
                            llvm_type(expected_ty),
                            slot
                        ));
                    }
                }
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        // Else branch
        fctx.vars = saved_scope.clone();
        fctx.drop_scopes = saved_drop_scopes.clone();
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", else_label));
        fctx.current_label = else_label.clone();
        let else_value = self.gen_block_with_expected_tail(else_block, expected_ty, fctx);
        let else_terminated = fctx.terminated;
        if !else_terminated {
            if let Some(value) = else_value {
                if value.ty != LType::Unit {
                    if result_slot.is_none() {
                        let ptr = self.alloc_entry_slot(&value.ty, fctx);
                        result_ty = Some(value.ty.clone());
                        result_slot = Some(ptr);
                    }
                    if let (Some(slot), Some(expected_ty)) =
                        (result_slot.as_ref(), result_ty.as_ref())
                    {
                        if value.ty != *expected_ty {
                            self.diagnostics.push(Diagnostic::error(
                                "E5007",
                                "if expression branches resolved to incompatible types",
                                self.file,
                                else_block.span,
                            ));
                        }
                        let repr = coerce_repr(&value, expected_ty);
                        fctx.lines.push(format!(
                            "  store {} {}, {}* {}",
                            llvm_type(expected_ty),
                            repr,
                            llvm_type(expected_ty),
                            slot
                        ));
                    }
                }
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        fctx.vars = saved_scope;
        fctx.drop_scopes = saved_drop_scopes;
        fctx.terminated = saved_terminated;

        if then_terminated && else_terminated {
            // Both branches end control-flow (for example, `return` in each arm).
            // Propagate termination so enclosing blocks do not attempt to synthesize
            // a fallback tail value after this expression.
            fctx.terminated = true;
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        fctx.lines.push(format!("{}:", cont_label));
        fctx.current_label = cont_label;

        if let (Some(slot), Some(result_ty)) = (result_slot, result_ty) {
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

    pub(super) fn gen_match(
        &mut self,
        scrutinee_expr: &ir::Expr,
        arms: &[ir::MatchArm],
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let scrutinee = self.gen_expr(scrutinee_expr, fctx)?;

        match scrutinee.ty.clone() {
            LType::Bool => self.gen_match_bool(scrutinee, arms, expected_ty, fctx),
            LType::Enum(layout) => self.gen_match_enum(scrutinee, &layout, arms, expected_ty, fctx),
            LType::Struct(layout) if base_type_name(&layout.repr) == TUPLE_INTERNAL_NAME => {
                self.gen_match_tuple(scrutinee, &layout, arms, expected_ty, fctx)
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E5016",
                    "match codegen currently supports Bool, tuple, and Enum-like ADTs",
                    self.file,
                    scrutinee_expr.span,
                ));
                None
            }
        }
    }

    pub(super) fn gen_match_bool(
        &mut self,
        scrutinee: Value,
        arms: &[ir::MatchArm],
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if let Some(guard) = arms.iter().find_map(|arm| arm.guard.as_ref()) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E5023",
                    "match guards are not supported by LLVM backend yet",
                    self.file,
                    guard.span,
                )
                .with_help("remove the guard or evaluate guard logic outside the match"),
            );
            return None;
        }

        let Some((true_arm, true_pattern)) = arms.iter().find_map(|arm| {
            self.select_bool_pattern(&arm.pattern, true)
                .map(|p| (arm, p))
        }) else {
            self.diagnostics.push(Diagnostic::error(
                "E5016",
                "non-exhaustive bool match reached codegen for `true` branch",
                self.file,
                crate::span::Span::new(0, 0),
            ));
            return None;
        };

        let Some((false_arm, false_pattern)) = arms.iter().find_map(|arm| {
            self.select_bool_pattern(&arm.pattern, false)
                .map(|p| (arm, p))
        }) else {
            self.diagnostics.push(Diagnostic::error(
                "E5016",
                "non-exhaustive bool match reached codegen for `false` branch",
                self.file,
                crate::span::Span::new(0, 0),
            ));
            return None;
        };

        let then_label = self.new_label("match_true");
        let else_label = self.new_label("match_false");
        let cont_label = self.new_label("match_cont");

        let mut result_ty: Option<LType> = None;
        let mut result_slot: Option<String> = None;

        let cond_repr = scrutinee.repr.unwrap_or_else(|| "0".to_string());
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            cond_repr, then_label, else_label
        ));

        let saved_scope = fctx.vars.clone();
        let saved_drop_scopes = fctx.drop_scopes.clone();
        let saved_terminated = fctx.terminated;

        fctx.terminated = false;
        fctx.lines.push(format!("{}:", then_label));
        self.bind_bool_match_pattern(true_pattern, true, fctx);
        let tv = self.gen_expr_with_expected(&true_arm.body, expected_ty, fctx);
        let t_term = fctx.terminated;
        if !t_term {
            if let Some(tv) = tv {
                let _ = self.store_match_arm_value(
                    tv,
                    true_arm.span,
                    expected_ty,
                    &mut result_slot,
                    &mut result_ty,
                    fctx,
                );
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        fctx.vars = saved_scope.clone();
        fctx.drop_scopes = saved_drop_scopes.clone();
        fctx.terminated = false;
        fctx.lines.push(format!("{}:", else_label));
        self.bind_bool_match_pattern(false_pattern, false, fctx);
        let ev = self.gen_expr_with_expected(&false_arm.body, expected_ty, fctx);
        let e_term = fctx.terminated;
        if !e_term {
            if let Some(ev) = ev {
                let _ = self.store_match_arm_value(
                    ev,
                    false_arm.span,
                    expected_ty,
                    &mut result_slot,
                    &mut result_ty,
                    fctx,
                );
            }
            fctx.lines.push(format!("  br label %{}", cont_label));
        }

        fctx.vars = saved_scope;
        fctx.drop_scopes = saved_drop_scopes;
        fctx.terminated = saved_terminated;

        if t_term && e_term {
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        fctx.lines.push(format!("{}:", cont_label));
        if let (Some(slot), Some(result_ty)) = (result_slot, result_ty) {
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

    pub(super) fn gen_match_enum(
        &mut self,
        scrutinee: Value,
        layout: &EnumLayoutType,
        arms: &[ir::MatchArm],
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if let Some(guard) = arms.iter().find_map(|arm| arm.guard.as_ref()) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E5023",
                    "match guards are not supported by LLVM backend yet",
                    self.file,
                    guard.span,
                )
                .with_help("remove the guard or evaluate guard logic outside the match"),
            );
            return None;
        }

        let mut selected_arms = Vec::new();
        for variant in &layout.variants {
            let selected = arms.iter().find_map(|arm| {
                self.select_enum_pattern(&arm.pattern, &variant.name)
                    .map(|p| (arm, p))
            });
            let Some((arm, selected_pattern)) = selected else {
                self.diagnostics.push(Diagnostic::error(
                    "E5016",
                    format!(
                        "non-exhaustive enum match reached codegen for '{}' variant '{}'",
                        layout.repr, variant.name
                    ),
                    self.file,
                    crate::span::Span::new(0, 0),
                ));
                return None;
            };
            selected_arms.push((arm, selected_pattern));
        }

        let mut result_ty: Option<LType> = None;
        let mut result_slot: Option<String> = None;

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

        let default_label = self.new_label("match_default");
        let cont_label = self.new_label("match_cont");
        let case_labels = layout
            .variants
            .iter()
            .map(|variant| self.new_label(&format!("match_{}", variant.name.to_lowercase())))
            .collect::<Vec<_>>();

        fctx.lines
            .push(format!("  switch i32 {}, label %{} [", tag, default_label));
        for (idx, label) in case_labels.iter().enumerate() {
            fctx.lines
                .push(format!("    i32 {}, label %{}", idx, label));
        }
        fctx.lines.push("  ]".to_string());

        let saved_scope = fctx.vars.clone();
        let saved_drop_scopes = fctx.drop_scopes.clone();
        let saved_terminated = fctx.terminated;

        let mut terminated_all = true;
        for (idx, (arm, selected_pattern)) in selected_arms.iter().enumerate() {
            let variant = &layout.variants[idx];
            fctx.vars = saved_scope.clone();
            fctx.drop_scopes = saved_drop_scopes.clone();
            fctx.terminated = false;
            fctx.lines.push(format!("{}:", case_labels[idx]));

            match &selected_pattern.kind {
                ir::PatternKind::Var(binding) => {
                    let ptr = self.new_temp();
                    fctx.lines
                        .push(format!("  {} = alloca {}", ptr, llvm_type(&scrutinee.ty)));
                    fctx.lines.push(format!(
                        "  store {} {}, {}* {}",
                        llvm_type(&scrutinee.ty),
                        scrutinee
                            .repr
                            .clone()
                            .unwrap_or_else(|| default_value(&scrutinee.ty)),
                        llvm_type(&scrutinee.ty),
                        ptr
                    ));
                    fctx.vars.last_mut().expect("scope").insert(
                        binding.clone(),
                        Local {
                            symbol: None,
                            ty: scrutinee.ty.clone(),
                            ptr,
                        },
                    );
                }
                ir::PatternKind::Variant { args, .. } => {
                    if let (Some(payload_ty), Some(binding_pat)) = (&variant.payload, args.first())
                    {
                        match &binding_pat.kind {
                            ir::PatternKind::Var(name) => {
                                let payload = self.new_temp();
                                fctx.lines.push(format!(
                                    "  {} = extractvalue {} {}, {}",
                                    payload,
                                    llvm_type(&scrutinee.ty),
                                    scrutinee
                                        .repr
                                        .clone()
                                        .unwrap_or_else(|| default_value(&scrutinee.ty)),
                                    idx + 1
                                ));
                                let ptr = self.new_temp();
                                fctx.lines.push(format!(
                                    "  {} = alloca {}",
                                    ptr,
                                    llvm_type(payload_ty)
                                ));
                                fctx.lines.push(format!(
                                    "  store {} {}, {}* {}",
                                    llvm_type(payload_ty),
                                    payload,
                                    llvm_type(payload_ty),
                                    ptr
                                ));
                                fctx.vars.last_mut().expect("scope").insert(
                                    name.clone(),
                                    Local {
                                        symbol: None,
                                        ty: payload_ty.clone(),
                                        ptr,
                                    },
                                );
                            }
                            ir::PatternKind::Wildcard => {}
                            _ => {
                                self.diagnostics.push(Diagnostic::error(
                                    "E5017",
                                    "enum payload pattern codegen supports var or wildcard payload",
                                    self.file,
                                    binding_pat.span,
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }

            let arm_value = self.gen_expr_with_expected(&arm.body, expected_ty, fctx);
            let arm_terminated = fctx.terminated;
            if !arm_terminated {
                terminated_all = false;
                if let Some(value) = arm_value {
                    let _ = self.store_match_arm_value(
                        value,
                        arm.span,
                        expected_ty,
                        &mut result_slot,
                        &mut result_ty,
                        fctx,
                    );
                }
                fctx.lines.push(format!("  br label %{}", cont_label));
            }
        }

        fctx.vars = saved_scope;
        fctx.drop_scopes = saved_drop_scopes;
        fctx.terminated = saved_terminated;
        let default_cont_label = self.new_label("match_default_cont");
        fctx.lines.push(format!("{}:", default_label));
        fctx.lines
            .push(format!("  br label %{}", default_cont_label));
        fctx.lines.push(format!("{}:", default_cont_label));
        fctx.lines.push(format!("  br label %{}", cont_label));

        if terminated_all {
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        fctx.lines.push(format!("{}:", cont_label));
        if let (Some(slot), Some(result_ty)) = (result_slot, result_ty) {
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

    pub(super) fn gen_match_tuple(
        &mut self,
        scrutinee: Value,
        layout: &StructLayoutType,
        arms: &[ir::MatchArm],
        expected_ty: Option<&LType>,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if base_type_name(&layout.repr) != TUPLE_INTERNAL_NAME {
            self.diagnostics.push(Diagnostic::error(
                "E5016",
                "tuple match codegen received non-tuple layout",
                self.file,
                crate::span::Span::new(0, 0),
            ));
            return None;
        }
        if let Some(guard) = arms.iter().find_map(|arm| arm.guard.as_ref()) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E5023",
                    "match guards are not supported by LLVM backend yet",
                    self.file,
                    guard.span,
                )
                .with_help("remove the guard or evaluate guard logic outside the match"),
            );
            return None;
        }
        if arms.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5016",
                "tuple match has no arms during codegen",
                self.file,
                crate::span::Span::new(0, 0),
            ));
            return None;
        }

        let cond_labels = (0..arms.len())
            .map(|idx| self.new_label(&format!("match_tuple_cond_{idx}")))
            .collect::<Vec<_>>();
        let arm_labels = (0..arms.len())
            .map(|idx| self.new_label(&format!("match_tuple_arm_{idx}")))
            .collect::<Vec<_>>();
        let default_label = self.new_label("match_tuple_default");
        let cont_label = self.new_label("match_tuple_cont");

        let mut result_ty: Option<LType> = None;
        let mut result_slot: Option<String> = None;

        let saved_scope = fctx.vars.clone();
        let saved_drop_scopes = fctx.drop_scopes.clone();
        let saved_terminated = fctx.terminated;

        fctx.lines.push(format!("  br label %{}", cond_labels[0]));
        for (idx, arm) in arms.iter().enumerate() {
            fctx.lines.push(format!("{}:", cond_labels[idx]));
            let cond = self.pattern_condition_for_tuple_match(&arm.pattern, &scrutinee, fctx)?;
            let next_label = if idx + 1 < arms.len() {
                cond_labels[idx + 1].clone()
            } else {
                default_label.clone()
            };
            fctx.lines.push(format!(
                "  br i1 {}, label %{}, label %{}",
                cond, arm_labels[idx], next_label
            ));

            fctx.vars = saved_scope.clone();
            fctx.drop_scopes = saved_drop_scopes.clone();
            fctx.terminated = false;
            fctx.lines.push(format!("{}:", arm_labels[idx]));
            self.bind_tuple_match_pattern(&arm.pattern, &scrutinee, fctx)?;

            let arm_value = self.gen_expr_with_expected(&arm.body, expected_ty, fctx);
            let arm_terminated = fctx.terminated;
            if !arm_terminated {
                if let Some(value) = arm_value {
                    let _ = self.store_match_arm_value(
                        value,
                        arm.span,
                        expected_ty,
                        &mut result_slot,
                        &mut result_ty,
                        fctx,
                    );
                }
                fctx.lines.push(format!("  br label %{}", cont_label));
            }
        }

        fctx.vars = saved_scope;
        fctx.drop_scopes = saved_drop_scopes;
        fctx.terminated = saved_terminated;

        fctx.lines.push(format!("{}:", default_label));
        if let (Some(slot), Some(result_ty)) = (result_slot.as_ref(), result_ty.as_ref()) {
            fctx.lines.push(format!(
                "  store {} {}, {}* {}",
                llvm_type(result_ty),
                default_value(result_ty),
                llvm_type(result_ty),
                slot
            ));
        }
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        if let (Some(slot), Some(result_ty)) = (result_slot, result_ty) {
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

    pub(super) fn pattern_condition_for_tuple_match(
        &mut self,
        pattern: &ir::Pattern,
        value: &Value,
        fctx: &mut FnCtx,
    ) -> Option<String> {
        match &pattern.kind {
            ir::PatternKind::Wildcard | ir::PatternKind::Var(_) => Some("1".to_string()),
            ir::PatternKind::Int(v) => {
                if !is_integral_type(&value.ty) {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        "tuple match int pattern expects integer value",
                        self.file,
                        pattern.span,
                    ));
                    return Some("0".to_string());
                }
                let reg = self.new_temp();
                let repr = value
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&value.ty));
                fctx.lines.push(format!(
                    "  {} = icmp eq {} {}, {}",
                    reg,
                    llvm_type(&value.ty),
                    repr,
                    v
                ));
                Some(reg)
            }
            ir::PatternKind::Char(v) => {
                if value.ty != LType::Char {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        "tuple match char pattern expects Char value",
                        self.file,
                        pattern.span,
                    ));
                    return Some("0".to_string());
                }
                let reg = self.new_temp();
                let repr = value.repr.clone().unwrap_or_else(|| "0".to_string());
                fctx.lines
                    .push(format!("  {} = icmp eq i32 {}, {}", reg, repr, *v as u32));
                Some(reg)
            }
            ir::PatternKind::String(_) => {
                self.diagnostics.push(Diagnostic::error(
                    "E5017",
                    "tuple match codegen does not support String literal patterns yet",
                    self.file,
                    pattern.span,
                ));
                Some("0".to_string())
            }
            ir::PatternKind::Bool(v) => {
                if value.ty != LType::Bool {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        "tuple match bool pattern expects Bool value",
                        self.file,
                        pattern.span,
                    ));
                    return Some("0".to_string());
                }
                let reg = self.new_temp();
                let repr = value.repr.clone().unwrap_or_else(|| "0".to_string());
                let expected = if *v { "1" } else { "0" };
                fctx.lines
                    .push(format!("  {} = icmp eq i1 {}, {}", reg, repr, expected));
                Some(reg)
            }
            ir::PatternKind::Unit => {
                if value.ty != LType::Unit {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        "tuple match unit pattern expects unit value",
                        self.file,
                        pattern.span,
                    ));
                    return Some("0".to_string());
                }
                Some("1".to_string())
            }
            ir::PatternKind::Or { patterns } => {
                let mut acc: Option<String> = None;
                for part in patterns {
                    let cond = self.pattern_condition_for_tuple_match(part, value, fctx)?;
                    acc = Some(if let Some(prev) = acc {
                        let reg = self.new_temp();
                        fctx.lines
                            .push(format!("  {} = or i1 {}, {}", reg, prev, cond));
                        reg
                    } else {
                        cond
                    });
                }
                Some(acc.unwrap_or_else(|| "0".to_string()))
            }
            ir::PatternKind::Variant { name, args } if name == TUPLE_INTERNAL_NAME => {
                let LType::Struct(tuple_layout) = &value.ty else {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        "tuple pattern codegen expects tuple struct value",
                        self.file,
                        pattern.span,
                    ));
                    return Some("0".to_string());
                };
                if base_type_name(&tuple_layout.repr) != TUPLE_INTERNAL_NAME {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        "tuple pattern codegen expects tuple layout",
                        self.file,
                        pattern.span,
                    ));
                    return Some("0".to_string());
                }
                if args.len() != tuple_layout.fields.len() {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        format!(
                            "tuple pattern arity mismatch: expected {}, found {}",
                            tuple_layout.fields.len(),
                            args.len()
                        ),
                        self.file,
                        pattern.span,
                    ));
                    return Some("0".to_string());
                }
                let mut acc = "1".to_string();
                for (idx, arg) in args.iter().enumerate() {
                    let field_value = self.extract_tuple_field_value(
                        value,
                        idx,
                        &tuple_layout.fields[idx].ty,
                        fctx,
                    );
                    let cond = self.pattern_condition_for_tuple_match(arg, &field_value, fctx)?;
                    let merged = self.new_temp();
                    fctx.lines
                        .push(format!("  {} = and i1 {}, {}", merged, acc, cond));
                    acc = merged;
                }
                Some(acc)
            }
            ir::PatternKind::Variant { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5017",
                    "tuple match codegen does not support enum-style patterns in tuple branches",
                    self.file,
                    pattern.span,
                ));
                Some("0".to_string())
            }
            ir::PatternKind::Struct { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5017",
                    "tuple match codegen does not support struct destructuring patterns yet",
                    self.file,
                    pattern.span,
                ));
                Some("0".to_string())
            }
        }
    }

    pub(super) fn bind_tuple_match_pattern(
        &mut self,
        pattern: &ir::Pattern,
        value: &Value,
        fctx: &mut FnCtx,
    ) -> Option<()> {
        match &pattern.kind {
            ir::PatternKind::Wildcard
            | ir::PatternKind::Int(_)
            | ir::PatternKind::Char(_)
            | ir::PatternKind::String(_)
            | ir::PatternKind::Bool(_)
            | ir::PatternKind::Unit => Some(()),
            ir::PatternKind::Var(binding) => {
                let ptr = self.new_temp();
                fctx.lines
                    .push(format!("  {} = alloca {}", ptr, llvm_type(&value.ty)));
                fctx.lines.push(format!(
                    "  store {} {}, {}* {}",
                    llvm_type(&value.ty),
                    value
                        .repr
                        .clone()
                        .unwrap_or_else(|| default_value(&value.ty)),
                    llvm_type(&value.ty),
                    ptr
                ));
                fctx.vars.last_mut().expect("scope").insert(
                    binding.clone(),
                    Local {
                        symbol: None,
                        ty: value.ty.clone(),
                        ptr,
                    },
                );
                Some(())
            }
            ir::PatternKind::Or { patterns } => {
                if let Some(first) = patterns.first() {
                    self.bind_tuple_match_pattern(first, value, fctx)
                } else {
                    Some(())
                }
            }
            ir::PatternKind::Variant { name, args } if name == TUPLE_INTERNAL_NAME => {
                let LType::Struct(tuple_layout) = &value.ty else {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        "tuple pattern binding expects tuple struct value",
                        self.file,
                        pattern.span,
                    ));
                    return None;
                };
                if base_type_name(&tuple_layout.repr) != TUPLE_INTERNAL_NAME {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        "tuple pattern binding expects tuple layout",
                        self.file,
                        pattern.span,
                    ));
                    return None;
                }
                if args.len() != tuple_layout.fields.len() {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        format!(
                            "tuple pattern arity mismatch: expected {}, found {}",
                            tuple_layout.fields.len(),
                            args.len()
                        ),
                        self.file,
                        pattern.span,
                    ));
                    return None;
                }
                for (idx, arg) in args.iter().enumerate() {
                    let field_value = self.extract_tuple_field_value(
                        value,
                        idx,
                        &tuple_layout.fields[idx].ty,
                        fctx,
                    );
                    self.bind_tuple_match_pattern(arg, &field_value, fctx)?;
                }
                Some(())
            }
            ir::PatternKind::Variant { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5017",
                    "tuple match binding does not support non-tuple variant patterns",
                    self.file,
                    pattern.span,
                ));
                None
            }
            ir::PatternKind::Struct { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5017",
                    "tuple match binding does not support struct destructuring patterns yet",
                    self.file,
                    pattern.span,
                ));
                None
            }
        }
    }

    pub(super) fn extract_tuple_field_value(
        &mut self,
        value: &Value,
        index: usize,
        field_ty: &LType,
        fctx: &mut FnCtx,
    ) -> Value {
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, {}",
            reg,
            llvm_type(&value.ty),
            value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&value.ty)),
            index
        ));
        Value {
            ty: field_ty.clone(),
            repr: Some(reg),
        }
    }

    pub(super) fn select_bool_pattern<'p>(
        &self,
        pattern: &'p ir::Pattern,
        value: bool,
    ) -> Option<&'p ir::Pattern> {
        match &pattern.kind {
            ir::PatternKind::Bool(v) if *v == value => Some(pattern),
            ir::PatternKind::Wildcard | ir::PatternKind::Var(_) => Some(pattern),
            ir::PatternKind::Or { patterns } => patterns
                .iter()
                .find_map(|part| self.select_bool_pattern(part, value)),
            _ => None,
        }
    }

    pub(super) fn select_enum_pattern<'p>(
        &self,
        pattern: &'p ir::Pattern,
        variant_name: &str,
    ) -> Option<&'p ir::Pattern> {
        match &pattern.kind {
            ir::PatternKind::Wildcard | ir::PatternKind::Var(_) => Some(pattern),
            ir::PatternKind::Variant { name, .. } if name == variant_name => Some(pattern),
            ir::PatternKind::Or { patterns } => patterns
                .iter()
                .find_map(|part| self.select_enum_pattern(part, variant_name)),
            _ => None,
        }
    }

    pub(super) fn bind_bool_match_pattern(
        &mut self,
        pattern: &ir::Pattern,
        value: bool,
        fctx: &mut FnCtx,
    ) {
        if let ir::PatternKind::Var(binding) = &pattern.kind {
            let ptr = self.new_temp();
            fctx.lines.push(format!("  {} = alloca i1", ptr));
            let bit = if value { "1" } else { "0" };
            fctx.lines.push(format!("  store i1 {}, i1* {}", bit, ptr));
            fctx.vars.last_mut().expect("scope").insert(
                binding.clone(),
                Local {
                    symbol: None,
                    ty: LType::Bool,
                    ptr,
                },
            );
        }
    }

    pub(super) fn evaluate_all_consts(&mut self) {
        let names = self.const_defs.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let mut stack = Vec::new();
            let _ = self.evaluate_const_by_name(&name, &mut stack);
        }
    }

    pub(super) fn evaluate_const_by_name(
        &mut self,
        name: &str,
        stack: &mut Vec<String>,
    ) -> Option<ConstValue> {
        if let Some(value) = self.const_values.get(name).cloned() {
            return Some(value);
        }
        if self.const_failures.contains(name) {
            return None;
        }
        if stack.iter().any(|entry| entry == name) {
            self.diagnostics.push(Diagnostic::error(
                "E5023",
                format!(
                    "cyclic const dependency detected during codegen: {} -> {}",
                    stack.join(" -> "),
                    name
                ),
                self.file,
                crate::span::Span::new(0, 0),
            ));
            self.const_failures.insert(name.to_string());
            return None;
        }

        let Some(def) = self.const_defs.get(name).cloned() else {
            self.const_failures.insert(name.to_string());
            return None;
        };
        let Some(init) = def.init.clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E5023",
                format!("const '{}' is missing an initializer during codegen", name),
                self.file,
                def.span,
            ));
            self.const_failures.insert(name.to_string());
            return None;
        };

        stack.push(name.to_string());
        let evaluated = self.eval_const_expr(name, &init, stack);
        stack.pop();

        let Some(value) = evaluated else {
            self.const_failures.insert(name.to_string());
            return None;
        };

        if let Some(expected_ty) = self.parse_type_repr(&def.declared_ty, def.span) {
            let actual_ty = self.const_value_ty(&value);
            if actual_ty != expected_ty {
                self.diagnostics.push(Diagnostic::error(
                    "E5007",
                    format!(
                        "const '{}' codegen type mismatch: expected '{}', found '{}'",
                        name,
                        render_type(&expected_ty),
                        render_type(&actual_ty)
                    ),
                    self.file,
                    def.span,
                ));
            }
        }

        self.const_values.insert(name.to_string(), value.clone());
        Some(value)
    }

    pub(super) fn eval_const_expr(
        &mut self,
        const_name: &str,
        expr: &ir::Expr,
        stack: &mut Vec<String>,
    ) -> Option<ConstValue> {
        match &expr.kind {
            ir::ExprKind::Int(v) => Some(ConstValue::Int(*v)),
            ir::ExprKind::Float(v) => Some(ConstValue::Float(*v)),
            ir::ExprKind::Bool(v) => Some(ConstValue::Bool(*v)),
            ir::ExprKind::Char(v) => Some(ConstValue::Char(*v)),
            ir::ExprKind::String(v) => Some(ConstValue::String(v.clone())),
            ir::ExprKind::Unit => Some(ConstValue::Unit),
            ir::ExprKind::Var(name) => {
                if self.const_defs.contains_key(name) || self.const_values.contains_key(name) {
                    self.evaluate_const_by_name(name, stack)
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        "E5001",
                        format!(
                            "const '{}' references unknown constant '{}'",
                            const_name, name
                        ),
                        self.file,
                        expr.span,
                    ));
                    None
                }
            }
            ir::ExprKind::Unary { op, expr: inner } => {
                let value = self.eval_const_expr(const_name, inner, stack)?;
                self.eval_const_unary(const_name, *op, value, expr.span)
            }
            ir::ExprKind::Binary { op, lhs, rhs } => {
                let lhs = self.eval_const_expr(const_name, lhs, stack)?;
                let rhs = self.eval_const_expr(const_name, rhs, stack)?;
                self.eval_const_binary(const_name, *op, lhs, rhs, expr.span)
            }
            ir::ExprKind::Call { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported function call in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::Closure { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported closure expression in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::If { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported `if` expression in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::While { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported `while` expression in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::Loop { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported `loop` expression in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::Break { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported `break` expression in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::Continue => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported `continue` expression in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::Match { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported `match` expression in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::Borrow { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported borrow expression in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::Await { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported `await` expression in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::Try { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported `?` expression in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::UnsafeBlock { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported `unsafe` block in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::StructInit { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported struct construction in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
            ir::ExprKind::FieldAccess { .. } => {
                self.diagnostics.push(Diagnostic::error(
                    "E5023",
                    format!(
                        "const '{}' initializer uses unsupported field access in LLVM backend",
                        const_name
                    ),
                    self.file,
                    expr.span,
                ));
                None
            }
        }
    }

    pub(super) fn eval_const_unary(
        &mut self,
        const_name: &str,
        op: UnaryOp,
        value: ConstValue,
        span: crate::span::Span,
    ) -> Option<ConstValue> {
        match (op, value) {
            (UnaryOp::Neg, ConstValue::Int(v)) => Some(ConstValue::Int(v.wrapping_neg())),
            (UnaryOp::Neg, ConstValue::Float(v)) => Some(ConstValue::Float(-v)),
            (UnaryOp::Not, ConstValue::Bool(v)) => Some(ConstValue::Bool(!v)),
            (UnaryOp::BitNot, ConstValue::Int(v)) => Some(ConstValue::Int(!v)),
            (op, value) => {
                self.diagnostics.push(Diagnostic::error(
                    "E5002",
                    format!(
                        "const '{}' unary operator '{}' does not support '{}'",
                        const_name,
                        unary_op_name(op),
                        const_value_name(&value)
                    ),
                    self.file,
                    span,
                ));
                None
            }
        }
    }

    pub(super) fn eval_const_binary(
        &mut self,
        const_name: &str,
        op: BinOp,
        lhs: ConstValue,
        rhs: ConstValue,
        span: crate::span::Span,
    ) -> Option<ConstValue> {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => match (&lhs, &rhs) {
                (ConstValue::Int(a), ConstValue::Int(b)) => match op {
                    BinOp::Add => Some(ConstValue::Int(a.wrapping_add(*b))),
                    BinOp::Sub => Some(ConstValue::Int(a.wrapping_sub(*b))),
                    BinOp::Mul => Some(ConstValue::Int(a.wrapping_mul(*b))),
                    BinOp::Div => {
                        if *b == 0 {
                            self.diagnostics.push(Diagnostic::error(
                                "E5006",
                                format!("const '{}' divides by zero in initializer", const_name),
                                self.file,
                                span,
                            ));
                            None
                        } else {
                            Some(ConstValue::Int(a.wrapping_div(*b)))
                        }
                    }
                    BinOp::Mod => {
                        if *b == 0 {
                            self.diagnostics.push(Diagnostic::error(
                                "E5006",
                                format!(
                                    "const '{}' computes modulo by zero in initializer",
                                    const_name
                                ),
                                self.file,
                                span,
                            ));
                            None
                        } else {
                            Some(ConstValue::Int(a.wrapping_rem(*b)))
                        }
                    }
                    _ => unreachable!(),
                },
                (ConstValue::Float(a), ConstValue::Float(b)) if !matches!(op, BinOp::Mod) => {
                    match op {
                        BinOp::Add => Some(ConstValue::Float(*a + *b)),
                        BinOp::Sub => Some(ConstValue::Float(*a - *b)),
                        BinOp::Mul => Some(ConstValue::Float(*a * *b)),
                        BinOp::Div => Some(ConstValue::Float(*a / *b)),
                        _ => unreachable!(),
                    }
                }
                _ => {
                    self.diagnostics.push(Diagnostic::error(
                        "E5006",
                        format!(
                            "const '{}' binary operator '{}' does not support '{}' and '{}'",
                            const_name,
                            binary_op_name(op),
                            const_value_name(&lhs),
                            const_value_name(&rhs)
                        ),
                        self.file,
                        span,
                    ));
                    None
                }
            },
            BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::BitXor
            | BinOp::Shl
            | BinOp::Shr
            | BinOp::Ushr => match (&lhs, &rhs) {
                (ConstValue::Int(a), ConstValue::Int(b)) => {
                    let out = match op {
                        BinOp::BitAnd => a & b,
                        BinOp::BitOr => a | b,
                        BinOp::BitXor => a ^ b,
                        BinOp::Shl => {
                            let shift = (*b as u64 & 63) as u32;
                            a.wrapping_shl(shift)
                        }
                        BinOp::Shr => {
                            let shift = (*b as u64 & 63) as u32;
                            a.wrapping_shr(shift)
                        }
                        BinOp::Ushr => {
                            let shift = (*b as u64 & 63) as u32;
                            ((*a as u64).wrapping_shr(shift)) as i64
                        }
                        _ => unreachable!(),
                    };
                    Some(ConstValue::Int(out))
                }
                _ => {
                    self.diagnostics.push(Diagnostic::error(
                        "E5006",
                        format!(
                            "const '{}' binary operator '{}' does not support '{}' and '{}'",
                            const_name,
                            binary_op_name(op),
                            const_value_name(&lhs),
                            const_value_name(&rhs)
                        ),
                        self.file,
                        span,
                    ));
                    None
                }
            },
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                match (&lhs, &rhs) {
                    (ConstValue::Int(a), ConstValue::Int(b)) => {
                        let result = match op {
                            BinOp::Eq => a == b,
                            BinOp::Ne => a != b,
                            BinOp::Lt => a < b,
                            BinOp::Le => a <= b,
                            BinOp::Gt => a > b,
                            BinOp::Ge => a >= b,
                            _ => unreachable!(),
                        };
                        Some(ConstValue::Bool(result))
                    }
                    (ConstValue::Float(a), ConstValue::Float(b)) => {
                        let result = match op {
                            BinOp::Eq => a == b,
                            BinOp::Ne => a != b,
                            BinOp::Lt => a < b,
                            BinOp::Le => a <= b,
                            BinOp::Gt => a > b,
                            BinOp::Ge => a >= b,
                            _ => unreachable!(),
                        };
                        Some(ConstValue::Bool(result))
                    }
                    (ConstValue::Char(a), ConstValue::Char(b)) => {
                        let result = match op {
                            BinOp::Eq => a == b,
                            BinOp::Ne => a != b,
                            BinOp::Lt => a < b,
                            BinOp::Le => a <= b,
                            BinOp::Gt => a > b,
                            BinOp::Ge => a >= b,
                            _ => unreachable!(),
                        };
                        Some(ConstValue::Bool(result))
                    }
                    (ConstValue::Bool(a), ConstValue::Bool(b))
                        if matches!(op, BinOp::Eq | BinOp::Ne) =>
                    {
                        let result = if matches!(op, BinOp::Eq) {
                            a == b
                        } else {
                            a != b
                        };
                        Some(ConstValue::Bool(result))
                    }
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E5006",
                            format!(
                                "const '{}' binary operator '{}' does not support '{}' and '{}'",
                                const_name,
                                binary_op_name(op),
                                const_value_name(&lhs),
                                const_value_name(&rhs)
                            ),
                            self.file,
                            span,
                        ));
                        None
                    }
                }
            }
            BinOp::And | BinOp::Or => match (&lhs, &rhs) {
                (ConstValue::Bool(a), ConstValue::Bool(b)) => {
                    let result = if matches!(op, BinOp::And) {
                        *a && *b
                    } else {
                        *a || *b
                    };
                    Some(ConstValue::Bool(result))
                }
                _ => {
                    self.diagnostics.push(Diagnostic::error(
                        "E5006",
                        format!(
                            "const '{}' binary operator '{}' does not support '{}' and '{}'",
                            const_name,
                            binary_op_name(op),
                            const_value_name(&lhs),
                            const_value_name(&rhs)
                        ),
                        self.file,
                        span,
                    ));
                    None
                }
            },
        }
    }

    pub(super) fn const_value_ty(&self, value: &ConstValue) -> LType {
        match value {
            ConstValue::Int(_) => LType::Int,
            ConstValue::Float(_) => LType::Float,
            ConstValue::Bool(_) => LType::Bool,
            ConstValue::Char(_) => LType::Char,
            ConstValue::Unit => LType::Unit,
            ConstValue::String(_) => LType::String,
        }
    }

    pub(super) fn runtime_value_from_const(
        &mut self,
        value: &ConstValue,
        fctx: &mut FnCtx,
    ) -> Value {
        match value {
            ConstValue::Int(v) => Value {
                ty: LType::Int,
                repr: Some(v.to_string()),
            },
            ConstValue::Float(v) => Value {
                ty: LType::Float,
                repr: Some(llvm_float_literal(*v)),
            },
            ConstValue::Bool(v) => Value {
                ty: LType::Bool,
                repr: Some(if *v { "1".to_string() } else { "0".to_string() }),
            },
            ConstValue::Char(v) => Value {
                ty: LType::Char,
                repr: Some((*v as u32).to_string()),
            },
            ConstValue::Unit => Value {
                ty: LType::Unit,
                repr: None,
            },
            ConstValue::String(v) => self.string_literal(v, fctx),
        }
    }

    pub(super) fn eval_struct_field_default(
        &mut self,
        struct_name: &str,
        field_name: &str,
        expr: &ir::Expr,
        expected_ty: &LType,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let context = format!("default value for field '{}.{}'", struct_name, field_name);
        let value = self.eval_const_expr(&context, expr, &mut Vec::new())?;
        let actual_ty = self.const_value_ty(&value);
        if &actual_ty != expected_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5004",
                format!(
                    "default value for field '{}.{}' expects '{}', found '{}'",
                    struct_name,
                    field_name,
                    render_type(expected_ty),
                    render_type(&actual_ty)
                ),
                self.file,
                expr.span,
            ));
            return None;
        }
        Some(self.runtime_value_from_const(&value, fctx))
    }

    pub(super) fn normalize_type_repr(
        &mut self,
        ty: &str,
        span: crate::span::Span,
    ) -> Option<String> {
        self.normalize_type_repr_with_stack(ty, span, &mut BTreeSet::new())
    }

    pub(super) fn normalize_type_repr_with_stack(
        &mut self,
        ty: &str,
        span: crate::span::Span,
        visiting: &mut BTreeSet<String>,
    ) -> Option<String> {
        let ty = ty.trim();
        let base = base_type_name(ty);
        let has_args = extract_generic_args(ty).is_some();
        let raw_args = extract_generic_args(ty).unwrap_or_default();
        let mut normalized_args = Vec::new();
        for arg in raw_args {
            normalized_args.push(self.normalize_type_repr_with_stack(&arg, span, visiting)?);
        }

        if let Some(alias) = self.type_aliases.get(base).cloned() {
            if !visiting.insert(base.to_string()) {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!("cyclic type alias '{}' cannot be lowered in codegen", base),
                    self.file,
                    span,
                ));
                return None;
            }

            let expanded = if alias.generics.is_empty() {
                if !normalized_args.is_empty() {
                    self.diagnostics.push(Diagnostic::error(
                        "E5019",
                        format!(
                            "generic arity mismatch for type alias '{}': expected 0, found {}",
                            base,
                            normalized_args.len()
                        ),
                        self.file,
                        alias.span,
                    ));
                    visiting.remove(base);
                    return None;
                }
                alias.target
            } else {
                if normalized_args.len() != alias.generics.len() {
                    self.diagnostics.push(Diagnostic::error(
                        "E5019",
                        format!(
                            "generic arity mismatch for type alias '{}': expected {}, found {}",
                            base,
                            alias.generics.len(),
                            normalized_args.len()
                        ),
                        self.file,
                        alias.span,
                    ));
                    visiting.remove(base);
                    return None;
                }
                let mut bindings = BTreeMap::new();
                for (generic, arg) in alias.generics.iter().zip(normalized_args.iter()) {
                    bindings.insert(generic.clone(), arg.clone());
                }
                substitute_type_vars(&alias.target, &bindings)
            };

            let resolved = self.normalize_type_repr_with_stack(&expanded, span, visiting);
            visiting.remove(base);
            return resolved;
        }

        if has_args {
            Some(render_applied_type_from_parts(base, &normalized_args))
        } else {
            Some(base.to_string())
        }
    }

    pub(super) fn type_from_id(
        &mut self,
        id: ir::TypeId,
        span: crate::span::Span,
    ) -> Option<LType> {
        let Some(repr) = self.type_map.get(&id).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                "E5018",
                format!("unknown type id {} in codegen", id.0),
                self.file,
                span,
            ));
            return None;
        };
        let concrete = if let Some(bindings) = &self.active_type_bindings {
            substitute_type_vars(&repr, bindings)
        } else {
            repr.clone()
        };
        match self.parse_type_repr(&concrete, span) {
            Some(ty) => Some(ty),
            None => {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!("unsupported type '{}' in codegen MVP", concrete),
                    self.file,
                    span,
                ));
                None
            }
        }
    }

    pub(super) fn parse_type_repr(&mut self, repr: &str, span: crate::span::Span) -> Option<LType> {
        let normalized = self.normalize_type_repr(repr, span)?;
        let repr = normalized.trim();
        match repr {
            "Int" => return Some(LType::Int),
            "ISize" => return Some(LType::ISize),
            "USize" | "UInt" => return Some(LType::USize),
            "Int8" => return Some(LType::Int8),
            "Int16" => return Some(LType::Int16),
            "Int32" => return Some(LType::Int32),
            "Int64" => return Some(LType::Int64),
            "Int128" => return Some(LType::Int128),
            "UInt8" => return Some(LType::UInt8),
            "UInt16" => return Some(LType::UInt16),
            "UInt32" => return Some(LType::UInt32),
            "UInt64" => return Some(LType::UInt64),
            "UInt128" => return Some(LType::UInt128),
            "Float" => return Some(LType::Float),
            "Bool" => return Some(LType::Bool),
            "Char" => return Some(LType::Char),
            "String" => return Some(LType::String),
            "()" => return Some(LType::Unit),
            _ => {}
        }

        if let Some(trait_name) = repr.strip_prefix("dyn ").map(str::trim) {
            if trait_name.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    "dyn type must name a trait",
                    self.file,
                    span,
                ));
                return None;
            }
            return Some(LType::DynTrait(trait_name.to_string()));
        }

        let base = base_type_name(repr);
        let arg_texts = extract_generic_args(repr).unwrap_or_default();

        if base == "Fn" {
            if arg_texts.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    "Fn type must declare at least a return type",
                    self.file,
                    span,
                ));
                return None;
            }
            let args = arg_texts
                .iter()
                .map(|text| self.parse_type_repr(text, span))
                .collect::<Option<Vec<_>>>()?;
            let mut params = args;
            let ret = params.pop()?;
            return Some(LType::Fn(FnLayoutType {
                repr: render_applied_type("Fn", &{
                    let mut all = params.clone();
                    all.push(ret.clone());
                    all
                }),
                params,
                ret: Box::new(ret),
            }));
        }

        if base == "Async" {
            if arg_texts.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!(
                        "generic arity mismatch for Async: expected 1, found {}",
                        arg_texts.len()
                    ),
                    self.file,
                    span,
                ));
                return None;
            }
            let inner = self.parse_type_repr(&arg_texts[0], span)?;
            return Some(LType::Async(Box::new(inner)));
        }

        if base == TUPLE_INTERNAL_NAME {
            let args = arg_texts
                .iter()
                .map(|text| self.parse_type_repr(text, span))
                .collect::<Option<Vec<_>>>()?;
            let fields = args
                .iter()
                .enumerate()
                .map(|(idx, ty)| StructFieldType {
                    name: idx.to_string(),
                    ty: ty.clone(),
                })
                .collect::<Vec<_>>();
            return Some(LType::Struct(StructLayoutType {
                repr: render_applied_type(base, &args),
                fields,
            }));
        }

        if let Some(template) = self.struct_templates.get(base).cloned() {
            if template.generics.len() != arg_texts.len() {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!(
                        "generic arity mismatch for struct '{}': expected {}, found {}",
                        base,
                        template.generics.len(),
                        arg_texts.len()
                    ),
                    self.file,
                    span,
                ));
                return None;
            }

            let args = arg_texts
                .iter()
                .map(|text| self.parse_type_repr(text, span))
                .collect::<Option<Vec<_>>>()?;

            let mut bindings = BTreeMap::new();
            for (generic, arg) in template.generics.iter().zip(args.iter()) {
                bindings.insert(generic.clone(), render_type(arg));
            }

            let mut fields = Vec::new();
            for (name, field_ty) in template.fields {
                let concrete = substitute_type_vars(&field_ty, &bindings);
                let ty = self.parse_type_repr(&concrete, span)?;
                fields.push(StructFieldType { name, ty });
            }

            return Some(LType::Struct(StructLayoutType {
                repr: render_applied_type(base, &args),
                fields,
            }));
        }

        if let Some(template) = self.enum_templates.get(base).cloned() {
            if template.generics.len() != arg_texts.len() {
                self.diagnostics.push(Diagnostic::error(
                    "E5019",
                    format!(
                        "generic arity mismatch for enum '{}': expected {}, found {}",
                        base,
                        template.generics.len(),
                        arg_texts.len()
                    ),
                    self.file,
                    span,
                ));
                return None;
            }

            let args = arg_texts
                .iter()
                .map(|text| self.parse_type_repr(text, span))
                .collect::<Option<Vec<_>>>()?;

            let mut bindings = BTreeMap::new();
            for (generic, arg) in template.generics.iter().zip(args.iter()) {
                bindings.insert(generic.clone(), render_type(arg));
            }

            let mut variants = Vec::new();
            for (name, payload) in template.variants {
                let payload_ty = if let Some(raw) = payload {
                    let concrete = substitute_type_vars(&raw, &bindings);
                    Some(self.parse_type_repr(&concrete, span)?)
                } else {
                    None
                };
                variants.push(EnumVariantType {
                    name,
                    payload: payload_ty,
                });
            }

            return Some(LType::Enum(EnumLayoutType {
                repr: render_applied_type(base, &args),
                variants,
            }));
        }

        None
    }

    pub(super) fn string_literal(&mut self, s: &str, fctx: &mut FnCtx) -> Value {
        let id = self.string_counter;
        self.string_counter += 1;
        let name = format!("@.str.{}", id);
        let (bytes, len_with_nul) = escape_c_string_bytes(s);
        let len = len_with_nul.saturating_sub(1) as i64;
        let const_text = format!(
            "{} = private unnamed_addr constant [{} x i8] c\"{}\"",
            name, len_with_nul, bytes
        );
        self.globals.push(const_text);

        let ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = getelementptr inbounds [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            ptr, len_with_nul, len_with_nul, name
        ));

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
            len
        ));
        Value {
            ty,
            repr: Some(reg2),
        }
    }

    pub(super) fn alloc_entry_slot(&mut self, ty: &LType, fctx: &mut FnCtx) -> String {
        let ptr = self.new_temp();
        let line = format!("  {} = alloca {}", ptr, llvm_type(ty));
        let mut insert_at = 1usize;
        while insert_at < fctx.lines.len() {
            let text = fctx.lines[insert_at].trim_start();
            if !text.starts_with('%') || !text.contains("alloca") {
                break;
            }
            insert_at += 1;
        }
        fctx.lines.insert(insert_at, line);
        ptr
    }

    pub(super) fn string_parts(
        &mut self,
        value: &Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String)> {
        if value.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "expected String value in codegen string ABI path",
                self.file,
                span,
            ));
            return None;
        }
        let repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            ptr,
            llvm_type(&value.ty),
            repr
        ));
        let repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 1",
            len,
            llvm_type(&value.ty),
            repr
        ));
        let repr = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let cap = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 2",
            cap,
            llvm_type(&value.ty),
            repr
        ));
        Some((ptr, len, cap))
    }

    pub(super) fn bytes_data_value(
        &mut self,
        value: &Value,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Struct(layout) = &value.ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Bytes"),
                self.file,
                span,
            ));
            return None;
        };
        if !base_type_name(&layout.repr).ends_with("Bytes")
            || layout.fields.len() != 1
            || layout.fields[0].name != "data"
            || layout.fields[0].ty != LType::String
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Bytes"),
                self.file,
                span,
            ));
            return None;
        }
        let data_reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = extractvalue {} {}, 0",
            data_reg,
            llvm_type(&value.ty),
            value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&value.ty))
        ));
        Some(Value {
            ty: LType::String,
            repr: Some(data_reg),
        })
    }

    pub(super) fn bytes_parts(
        &mut self,
        value: &Value,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<(String, String, String)> {
        let data = self.bytes_data_value(value, context, span, fctx)?;
        self.string_parts(&data, span, fctx)
    }

    pub(super) fn build_bytes_value_from_data(
        &mut self,
        bytes_ty: &LType,
        data_value: Value,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Struct(layout) = bytes_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Bytes payload"),
                self.file,
                span,
            ));
            return None;
        };
        if !base_type_name(&layout.repr).ends_with("Bytes")
            || layout.fields.len() != 1
            || layout.fields[0].name != "data"
            || layout.fields[0].ty != LType::String
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Bytes payload"),
                self.file,
                span,
            ));
            return None;
        }
        self.build_struct_value(layout, &[data_value], span, fctx)
    }

    pub(super) fn span_line_col(&self, span: crate::span::Span) -> (u64, u64) {
        if let Some(source_map) = &self.source_map {
            source_map.line_col(span.start)
        } else {
            (0, 0)
        }
    }

    pub(super) fn emit_panic_call(
        &mut self,
        ptr: &str,
        len: &str,
        cap: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) {
        let (line, column) = self.span_line_col(span);
        let mut call = format!(
            "  call void @aic_rt_panic(i8* {}, i64 {}, i64 {}, i64 {}, i64 {})",
            ptr, len, cap, line, column
        );
        if let (Some(scope), Some(debug)) = (fctx.debug_scope, self.debug.as_mut()) {
            let location = debug.new_location(line, column, scope);
            call.push_str(&format!(", !dbg !{location}"));
        }
        fctx.lines.push(call);
    }

    pub(super) fn flush_deferred_fn_defs(&mut self) {
        if self.deferred_fn_defs.is_empty() {
            return;
        }
        for def in self.deferred_fn_defs.drain(..) {
            self.out.extend(def);
            self.out.push(String::new());
        }
    }

    pub(super) fn new_temp(&mut self) -> String {
        let n = self.temp_counter;
        self.temp_counter += 1;
        format!("%t{}", n)
    }

    pub(super) fn new_label(&mut self, prefix: &str) -> String {
        let n = self.label_counter;
        self.label_counter += 1;
        format!("{}_{}", prefix, n)
    }
}
