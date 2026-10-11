//! General `match` lowering.
//!
//! Arms are tested in order. Each arm's pattern compiles to a chain of tests
//! on the scrutinee value: integer and char literals compare with `icmp`,
//! strings through `aic_rt_string_compare`, booleans directly, enum variants
//! on the tag, and tuples, structs and variant payloads recurse into their
//! fields with `extractvalue`. A failed test jumps to the next arm. Bindings
//! live in entry-block slots, so every alternative of an or-pattern can
//! store into the same slot. Guards run after the bindings are in scope.
//!
//! The checker proves exhaustiveness before code generation, so falling off
//! the last arm is unreachable; it panics with `match_failure` instead of
//! running undefined code if that invariant is ever broken.

use super::*;

/// One binding introduced by a pattern: the name and its entry-block slot.
struct PatternBinding {
    name: String,
    ty: LType,
    ptr: String,
}

impl<'a> Generator<'a> {
    /// Whether the specialized `Bool`/enum/tuple lowerings handle every arm:
    /// patterns must be flat (wildcards, bindings, bool literals, variants
    /// whose payload is a binding or wildcard, tuples of those).
    pub(super) fn match_needs_general_lowering(
        &self,
        scrutinee_ty: &LType,
        arms: &[ir::MatchArm],
    ) -> bool {
        let supported_scrutinee = match scrutinee_ty {
            LType::Bool | LType::Enum(_) => true,
            LType::Struct(layout) => base_type_name(&layout.repr) == TUPLE_INTERNAL_NAME,
            _ => false,
        };
        !supported_scrutinee
            || arms
                .iter()
                .any(|arm| !pattern_is_flat(&arm.pattern, matches!(scrutinee_ty, LType::Struct(_))))
    }

    pub(super) fn gen_match_general(
        &mut self,
        scrutinee: Value,
        arms: &[ir::MatchArm],
        expected_ty: Option<&LType>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let cont_label = self.new_label("match_cont");
        let mut result_ty: Option<LType> = None;
        let mut result_slot: Option<String> = None;
        let saved_scope = fctx.vars.clone();
        let saved_drop_scopes = fctx.drop_scopes.clone();
        let saved_terminated = fctx.terminated;
        let mut terminated_all = true;

        for arm in arms {
            let next_label = self.new_label("match_next");
            fctx.vars = saved_scope.clone();
            fctx.drop_scopes = saved_drop_scopes.clone();
            fctx.terminated = false;

            let mut bindings = Vec::new();
            self.gen_pattern_test(&arm.pattern, &scrutinee, &next_label, &mut bindings, fctx)?;
            fctx.vars.push(BTreeMap::new());
            for binding in bindings {
                fctx.vars.last_mut().expect("arm scope").insert(
                    binding.name,
                    Local {
                        symbol: None,
                        ty: binding.ty,
                        ptr: binding.ptr,
                    },
                );
            }

            if let Some(guard) = &arm.guard {
                let cond = self.gen_expr(guard, fctx)?;
                if cond.ty != LType::Bool {
                    self.diagnostics.push(Diagnostic::error(
                        "E5016",
                        "match guard must lower to Bool",
                        self.file,
                        guard.span,
                    ));
                    return None;
                }
                let body_label = self.new_label("match_guard_ok");
                fctx.lines.push(format!(
                    "  br i1 {}, label %{}, label %{}",
                    cond.repr.unwrap_or_else(|| "0".to_string()),
                    body_label,
                    next_label
                ));
                fctx.lines.push(format!("{body_label}:"));
                fctx.current_label = body_label;
            }

            let arm_value = self.gen_expr_with_expected(&arm.body, expected_ty, fctx);
            if !fctx.terminated {
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
                fctx.lines.push(format!("  br label %{cont_label}"));
            }
            fctx.lines.push(format!("{next_label}:"));
            fctx.current_label = next_label;
        }

        fctx.vars = saved_scope;
        fctx.drop_scopes = saved_drop_scopes;
        fctx.terminated = saved_terminated;
        // Exhaustiveness is checked before codegen; reaching this block means
        // that invariant was broken, so fail loudly instead of continuing.
        self.emit_static_panic("match_failure: no match arm matched the value", span, fctx);

        if terminated_all {
            fctx.terminated = true;
            return Some(Value {
                ty: LType::Unit,
                repr: None,
            });
        }

        fctx.lines.push(format!("{cont_label}:"));
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

    /// Emits the tests for `pattern` against `value`. On a mismatch control
    /// jumps to `fail_label`; on success it continues in the current block
    /// with every binding stored in `bindings`.
    fn gen_pattern_test(
        &mut self,
        pattern: &ir::Pattern,
        value: &Value,
        fail_label: &str,
        bindings: &mut Vec<PatternBinding>,
        fctx: &mut FnCtx,
    ) -> Option<()> {
        match &pattern.kind {
            ir::PatternKind::Wildcard | ir::PatternKind::Unit => Some(()),
            ir::PatternKind::Var(name) => {
                let ptr = self.binding_slot(name, &value.ty, bindings, fctx);
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
                Some(())
            }
            ir::PatternKind::Bool(expected) => {
                let repr = value.repr.clone().unwrap_or_else(|| "0".to_string());
                let cond = if *expected {
                    repr
                } else {
                    let negated = self.new_temp();
                    fctx.lines
                        .push(format!("  {negated} = xor i1 {repr}, true"));
                    negated
                };
                self.branch_on(&cond, fail_label, fctx);
                Some(())
            }
            ir::PatternKind::Int(literal, ..) => {
                if !is_integral_type(&value.ty) {
                    return self.pattern_type_error(pattern, "integer", &value.ty);
                }
                let text = pattern
                    .int_literal_metadata()
                    .and_then(|meta| {
                        super::generator_core::parse_raw_int_literal_magnitude(
                            &meta.raw_literal_text,
                        )
                    })
                    .map(|magnitude| magnitude.to_string())
                    .unwrap_or_else(|| literal.to_string());
                let cond = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = icmp eq {} {}, {}",
                    cond,
                    llvm_type(&value.ty),
                    value.repr.clone().unwrap_or_else(|| "0".to_string()),
                    text
                ));
                self.branch_on(&cond, fail_label, fctx);
                Some(())
            }
            ir::PatternKind::Char(expected) => {
                if value.ty != LType::Char {
                    return self.pattern_type_error(pattern, "char", &value.ty);
                }
                let cond = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = icmp eq {} {}, {}",
                    cond,
                    llvm_type(&value.ty),
                    value.repr.clone().unwrap_or_else(|| "0".to_string()),
                    *expected as u32
                ));
                self.branch_on(&cond, fail_label, fctx);
                Some(())
            }
            ir::PatternKind::String(expected) => {
                if value.ty != LType::String {
                    return self.pattern_type_error(pattern, "string", &value.ty);
                }
                let literal = self.string_literal(expected, fctx);
                let cond = self.gen_binary(
                    BinOp::Eq,
                    value.clone(),
                    literal,
                    DivisorFact::Unknown,
                    fctx,
                    pattern.span,
                )?;
                self.branch_on(
                    &cond.repr.unwrap_or_else(|| "0".to_string()),
                    fail_label,
                    fctx,
                );
                Some(())
            }
            ir::PatternKind::Or { patterns } => {
                // Every alternative binds the same names (checked earlier), so
                // they share entry-block slots.
                let success_label = self.new_label("match_or_ok");
                for (idx, alternative) in patterns.iter().enumerate() {
                    let last = idx + 1 == patterns.len();
                    let alt_fail = if last {
                        fail_label.to_string()
                    } else {
                        self.new_label("match_or_next")
                    };
                    self.gen_pattern_test(alternative, value, &alt_fail, bindings, fctx)?;
                    fctx.lines.push(format!("  br label %{success_label}"));
                    if !last {
                        fctx.lines.push(format!("{alt_fail}:"));
                        fctx.current_label = alt_fail;
                    }
                }
                fctx.lines.push(format!("{success_label}:"));
                fctx.current_label = success_label;
                Some(())
            }
            ir::PatternKind::Variant { name, args } if name == TUPLE_INTERNAL_NAME => {
                let LType::Struct(layout) = &value.ty else {
                    return self.pattern_type_error(pattern, "tuple", &value.ty);
                };
                let layout = layout.clone();
                for (idx, (arg, field)) in args.iter().zip(layout.fields.iter()).enumerate() {
                    let element = self.extract_field(value, idx, &field.ty, fctx);
                    self.gen_pattern_test(arg, &element, fail_label, bindings, fctx)?;
                }
                Some(())
            }
            ir::PatternKind::Variant { name, args } => {
                let LType::Enum(layout) = &value.ty else {
                    return self.pattern_type_error(pattern, "enum", &value.ty);
                };
                let layout = layout.clone();
                let short = name.rsplit(['.', ':']).next().unwrap_or(name);
                let Some(index) = layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == short || variant.name == *name)
                else {
                    self.diagnostics.push(Diagnostic::error(
                        "E5017",
                        format!(
                            "unknown variant '{}' for '{}' in match codegen",
                            name, layout.repr
                        ),
                        self.file,
                        pattern.span,
                    ));
                    return None;
                };
                let tag = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = extractvalue {} {}, 0",
                    tag,
                    llvm_type(&value.ty),
                    value
                        .repr
                        .clone()
                        .unwrap_or_else(|| default_value(&value.ty))
                ));
                let cond = self.new_temp();
                fctx.lines
                    .push(format!("  {cond} = icmp eq i32 {tag}, {index}"));
                self.branch_on(&cond, fail_label, fctx);
                if let (Some(payload_ty), Some(payload_pat)) =
                    (layout.variants[index].payload.clone(), args.first())
                {
                    let payload = self.extract_field(value, index + 1, &payload_ty, fctx);
                    self.gen_pattern_test(payload_pat, &payload, fail_label, bindings, fctx)?;
                }
                Some(())
            }
            ir::PatternKind::Struct { name, fields, .. } => {
                let LType::Struct(layout) = &value.ty else {
                    return self.pattern_type_error(pattern, "struct", &value.ty);
                };
                let layout = layout.clone();
                for field_pattern in fields {
                    let Some(index) = layout
                        .fields
                        .iter()
                        .position(|field| field.name == field_pattern.name)
                    else {
                        self.diagnostics.push(Diagnostic::error(
                            "E5017",
                            format!(
                                "unknown field '{}' of '{}' in match codegen",
                                field_pattern.name, name
                            ),
                            self.file,
                            field_pattern.pattern.span,
                        ));
                        return None;
                    };
                    let field_ty = layout.fields[index].ty.clone();
                    let field_value = self.extract_field(value, index, &field_ty, fctx);
                    self.gen_pattern_test(
                        &field_pattern.pattern,
                        &field_value,
                        fail_label,
                        bindings,
                        fctx,
                    )?;
                }
                Some(())
            }
        }
    }

    /// The entry-block slot for binding `name`, reusing the slot an earlier
    /// or-pattern alternative created for the same name.
    fn binding_slot(
        &mut self,
        name: &str,
        ty: &LType,
        bindings: &mut Vec<PatternBinding>,
        fctx: &mut FnCtx,
    ) -> String {
        if let Some(existing) = bindings.iter().find(|binding| binding.name == name) {
            return existing.ptr.clone();
        }
        let ptr = self.alloc_entry_slot(ty, fctx);
        bindings.push(PatternBinding {
            name: name.to_string(),
            ty: ty.clone(),
            ptr: ptr.clone(),
        });
        ptr
    }

    fn extract_field(
        &mut self,
        value: &Value,
        index: usize,
        ty: &LType,
        fctx: &mut FnCtx,
    ) -> Value {
        if *ty == LType::Unit {
            return Value {
                ty: LType::Unit,
                repr: None,
            };
        }
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
            ty: ty.clone(),
            repr: Some(reg),
        }
    }

    /// Continues in a fresh block when `cond` holds, else jumps to `fail_label`.
    fn branch_on(&mut self, cond: &str, fail_label: &str, fctx: &mut FnCtx) {
        let ok_label = self.new_label("match_test_ok");
        fctx.lines.push(format!(
            "  br i1 {cond}, label %{ok_label}, label %{fail_label}"
        ));
        fctx.lines.push(format!("{ok_label}:"));
        fctx.current_label = ok_label;
    }

    fn pattern_type_error(&mut self, pattern: &ir::Pattern, kind: &str, ty: &LType) -> Option<()> {
        self.diagnostics.push(Diagnostic::error(
            "E5017",
            format!(
                "{kind} pattern does not fit a value of type '{}' in match codegen",
                render_type(ty)
            ),
            self.file,
            pattern.span,
        ));
        None
    }
}

/// Patterns the specialized lowerings support. Tuple elements allow only
/// wildcards, bindings and bool literals.
fn pattern_is_flat(pattern: &ir::Pattern, in_tuple: bool) -> bool {
    match &pattern.kind {
        ir::PatternKind::Wildcard | ir::PatternKind::Var(_) | ir::PatternKind::Bool(_) => true,
        ir::PatternKind::Or { patterns } => patterns.iter().all(|p| pattern_is_flat(p, in_tuple)),
        ir::PatternKind::Variant { name, args } if name == TUPLE_INTERNAL_NAME => {
            args.iter().all(|arg| {
                matches!(
                    arg.kind,
                    ir::PatternKind::Wildcard | ir::PatternKind::Var(_) | ir::PatternKind::Bool(_)
                )
            })
        }
        ir::PatternKind::Variant { args, .. } => {
            !in_tuple
                && args.iter().all(|arg| {
                    matches!(
                        arg.kind,
                        ir::PatternKind::Wildcard | ir::PatternKind::Var(_)
                    )
                })
        }
        ir::PatternKind::Int(_, ..)
        | ir::PatternKind::Char(_)
        | ir::PatternKind::String(_)
        | ir::PatternKind::Unit
        | ir::PatternKind::Struct { .. } => false,
    }
}
