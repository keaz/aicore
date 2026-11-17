//! Place assignment: `p.x = v`, `p.a.0 = v`, `v[i] = x`, `v[i].x = y` and
//! `self.n = v` in `&mut self` methods.
//!
//! Values are immutable LLVM aggregates, so an assignment rebuilds the
//! path from the root: each struct or tuple level is copied with the new
//! field (`insertvalue`), each `Vec` level is updated with `aic_rt_vec_set`
//! (which copies the buffer, keeping Vec value semantics), and the new root
//! is stored back into the binding's slot. For a `&mut self` receiver the
//! slot is the caller's storage, so the update is visible to the caller.
//!
//! Evaluation order: index expressions (already bound to hidden locals by
//! the IR builder), then the right-hand side, then the store. A rebuilt
//! struct whose type declares an invariant is checked again through the
//! struct's invariant helper, which panics with the contract metadata.

use super::*;

/// Names and symbols of the functions and impl methods with a `&mut self`
/// receiver.
pub(super) fn collect_mutable_receiver_fns(
    program: &ir::Program,
) -> (BTreeSet<String>, BTreeSet<ir::SymbolId>) {
    let has_mutable_receiver =
        |func: &ir::Function| func.params.first().is_some_and(|p| p.mutable_receiver);
    let mut names = BTreeSet::new();
    let mut symbols = BTreeSet::new();
    let mut record = |func: &ir::Function| {
        if has_mutable_receiver(func) {
            names.insert(func.name.clone());
            symbols.insert(func.symbol);
        }
    };
    for item in &program.items {
        match item {
            ir::Item::Function(func) => record(func),
            ir::Item::Impl(impl_def) => impl_def.methods.iter().for_each(&mut record),
            _ => {}
        }
    }
    (names, symbols)
}

/// Prefix of the helper that checks a struct invariant and rebuilds the
/// value; see `contracts::lower_runtime_asserts`.
const INVARIANT_HELPER_PREFIX: &str = "__aic_invariant_ctor_";

impl<'a> Generator<'a> {
    /// Address passed for a `&mut self` receiver: the caller's storage when
    /// known, otherwise a fresh slot holding `value` (a temporary receiver,
    /// whose updates are discarded with it).
    pub(super) fn receiver_storage(
        &mut self,
        storage: Option<&str>,
        value: &Value,
        fctx: &mut FnCtx,
    ) -> String {
        if let Some(ptr) = storage {
            return ptr.to_string();
        }
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
        slot
    }

    /// Address of a place expression rooted in a local (`x`, `x.a.b`), with
    /// its type. Returns `None` for any other expression.
    fn place_address(&mut self, expr: &ir::Expr, fctx: &mut FnCtx) -> Option<(String, LType)> {
        match &expr.kind {
            ir::ExprKind::Var(name) => {
                let local = find_local(&fctx.vars, name)?;
                Some((local.ptr, local.ty))
            }
            ir::ExprKind::FieldAccess { base, field } => {
                let (base_ptr, base_ty) = self.place_address(base, fctx)?;
                let LType::Struct(layout) = &base_ty else {
                    return None;
                };
                let (index, field_layout) = layout
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, info)| info.name == *field)?;
                let ptr = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = getelementptr inbounds {}, {}* {}, i32 0, i32 {}",
                    ptr,
                    llvm_type(&base_ty),
                    llvm_type(&base_ty),
                    base_ptr,
                    index
                ));
                Some((ptr, field_layout.ty.clone()))
            }
            _ => None,
        }
    }

    /// Emits `base.method(args)` when `base` is a place and `method` takes
    /// `&mut self`: the method receives the place's address, so its updates
    /// land in the caller's binding. Returns `None` when this does not
    /// apply and the ordinary method call path should run. `symbol` is the
    /// checker's resolution of the method call.
    pub(super) fn gen_mutable_receiver_method_call(
        &mut self,
        base: &ir::Expr,
        method: &str,
        args: &[ir::Expr],
        symbol: Option<ir::SymbolId>,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let ty = self.place_type_without_codegen(base, fctx)?;
        let (name, symbol) = self.mutable_receiver_method(&ty, method, symbol)?;
        let (ptr, ty) = self.place_address(base, fctx)?;
        let loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            loaded,
            llvm_type(&ty),
            llvm_type(&ty),
            ptr
        ));
        let mut values = Vec::with_capacity(args.len() + 1);
        values.push(Value {
            ty,
            repr: Some(loaded),
        });
        for arg in args {
            let Some(value) = self.gen_expr(arg, fctx) else {
                return Some(None);
            };
            values.push(value);
        }
        Some(self.gen_named_function_call_with_receiver(
            &name,
            symbol,
            values,
            Some(ptr),
            span,
            fctx,
        ))
    }

    /// Emits `Type::method(receiver, args)` for a `&mut self` method: the
    /// receiver's address is passed when it is a place, so the update lands
    /// in the caller's binding. Returns `None` for other functions.
    /// `symbol` is the checker's resolution of the call.
    pub(super) fn gen_mutable_receiver_function_call(
        &mut self,
        name: &str,
        symbol: Option<ir::SymbolId>,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let symbol = self.callee_symbol(name, symbol);
        if !self.has_mutable_receiver(name, symbol) {
            return None;
        }
        let (receiver_arg, rest) = args.split_first()?;
        let mut values = Vec::with_capacity(args.len());
        let storage = match self.place_address(receiver_arg, fctx) {
            Some((ptr, ty)) => {
                let loaded = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = load {}, {}* {}",
                    loaded,
                    llvm_type(&ty),
                    llvm_type(&ty),
                    ptr
                ));
                values.push(Value {
                    ty,
                    repr: Some(loaded),
                });
                Some(ptr)
            }
            None => {
                let Some(value) = self.gen_expr(receiver_arg, fctx) else {
                    return Some(None);
                };
                values.push(value);
                None
            }
        };
        for arg in rest {
            let Some(value) = self.gen_expr(arg, fctx) else {
                return Some(None);
            };
            values.push(value);
        }
        Some(self.gen_named_function_call_with_receiver(name, symbol, values, storage, span, fctx))
    }

    /// Type of a place expression rooted in a local, computed without
    /// emitting code.
    fn place_type_without_codegen(&self, expr: &ir::Expr, fctx: &FnCtx) -> Option<LType> {
        match &expr.kind {
            ir::ExprKind::Var(name) => find_local(&fctx.vars, name).map(|local| local.ty),
            ir::ExprKind::FieldAccess { base, field } => {
                let LType::Struct(layout) = self.place_type_without_codegen(base, fctx)? else {
                    return None;
                };
                layout
                    .fields
                    .iter()
                    .find(|info| info.name == *field)
                    .map(|info| info.ty.clone())
            }
            _ => None,
        }
    }

    /// The `Type::method` name, and the checker's symbol when it names that
    /// method, of a `&mut self` method called on a value of type `ty`, when
    /// there is one.
    fn mutable_receiver_method(
        &self,
        ty: &LType,
        method: &str,
        symbol: Option<ir::SymbolId>,
    ) -> Option<(String, Option<ir::SymbolId>)> {
        let repr = match ty {
            LType::Struct(layout) => &layout.repr,
            LType::Enum(layout) => &layout.repr,
            _ => return None,
        };
        let name = format!("{}::{}", base_type_name(repr), method);
        let symbol = self.callee_symbol(&name, symbol);
        self.has_mutable_receiver(&name, symbol)
            .then_some((name, symbol))
    }

    pub(super) fn gen_place_assign(
        &mut self,
        target: &str,
        path: &[ir::PlaceProjection],
        op: Option<BinOp>,
        expr: &ir::Expr,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) {
        let Some(local) = find_local(&fctx.vars, target) else {
            self.diagnostics.push(Diagnostic::error(
                "E5001",
                format!("unknown local variable '{target}' during assignment codegen"),
                self.file,
                span,
            ));
            return;
        };
        let Some(place_ty) = self.place_ltype(&local.ty, path, target) else {
            return;
        };
        let Some(value) = self.gen_expr_with_expected(expr, Some(&place_ty), fctx) else {
            return;
        };
        if fctx.terminated {
            return;
        }
        let value = match op {
            None => value,
            Some(op) => {
                // `place op= expr`: the right-hand side is evaluated first,
                // then the place is read and combined with it.
                let Some(current) = self.read_place(&local, path, fctx) else {
                    return;
                };
                let divisor = if matches!(op, BinOp::Div | BinOp::Mod) {
                    divisor_facts::divisor_fact(expr, &fctx.divisor_facts)
                } else {
                    DivisorFact::Unknown
                };
                let lhs = if path.is_empty() {
                    OverflowOperand::Local(target)
                } else {
                    OverflowOperand::Unknown
                };
                let proven = current.ty == value.ty
                    && self.arith_overflow_proven(op, lhs, expr, &current.ty, fctx);
                let Some(combined) =
                    self.gen_binary(op, current, value, divisor, proven, fctx, span)
                else {
                    return;
                };
                combined
            }
        };
        let Some(value) = self.coerce_value_to_expected(value, &place_ty, span, fctx) else {
            return;
        };
        if value.ty != place_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5007",
                format!(
                    "assignment codegen type mismatch for '{}': expected '{}', found '{}'",
                    ir::render_place(target, path),
                    render_type(&place_ty),
                    render_type(&value.ty)
                ),
                self.file,
                span,
            ));
            return;
        }

        let root_reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            root_reg,
            llvm_type(&local.ty),
            llvm_type(&local.ty),
            local.ptr
        ));
        let root = Value {
            ty: local.ty.clone(),
            repr: Some(root_reg),
        };
        let Some(updated) = self.rebuild_place(root, path, value, span, fctx) else {
            return;
        };
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(&local.ty),
            updated
                .repr
                .clone()
                .unwrap_or_else(|| default_value(&local.ty)),
            llvm_type(&local.ty),
            local.ptr
        ));
    }

    /// Current value of the place `local.path`.
    fn read_place(
        &mut self,
        local: &Local,
        path: &[ir::PlaceProjection],
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(&local.ty),
            llvm_type(&local.ty),
            local.ptr
        ));
        let mut current = Value {
            ty: local.ty.clone(),
            repr: Some(reg),
        };
        for projection in path {
            current = match projection {
                ir::PlaceProjection::Field { name, span } => {
                    let (index, field_ty) = self.place_field(&current.ty, name, "place", *span)?;
                    let field = self.new_temp();
                    fctx.lines.push(format!(
                        "  {} = extractvalue {} {}, {}",
                        field,
                        llvm_type(&current.ty),
                        current
                            .repr
                            .clone()
                            .unwrap_or_else(|| default_value(&current.ty)),
                        index
                    ));
                    Value {
                        ty: field_ty,
                        repr: Some(field),
                    }
                }
                ir::PlaceProjection::Index { binding, span } => {
                    let index = self.load_place_index(binding, *span, fctx)?;
                    let (elem_ty, _, _) =
                        self.vec_element_info(&current.ty, "index assignment", *span)?;
                    self.vec_element_or_panic(&current, &index, &elem_ty, *span, fctx)?
                }
            };
        }
        Some(current)
    }

    /// Loads the hidden `Int` binding that holds a place index.
    fn load_place_index(
        &mut self,
        binding: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<String> {
        let Some(index_local) = find_local(&fctx.vars, binding) else {
            self.diagnostics.push(Diagnostic::error(
                "E5001",
                "missing index binding during place assignment codegen",
                self.file,
                span,
            ));
            return None;
        };
        if index_local.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "Vec index must be Int, found '{}'",
                    render_type(&index_local.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let index = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", index, index_local.ptr));
        Some(index)
    }

    /// LLVM type of the place `target.path`, or `None` after reporting.
    fn place_ltype(
        &mut self,
        root_ty: &LType,
        path: &[ir::PlaceProjection],
        target: &str,
    ) -> Option<LType> {
        let mut current = root_ty.clone();
        for projection in path {
            current = match projection {
                ir::PlaceProjection::Field { name, span } => {
                    let (_, field_ty) = self.place_field(&current, name, target, *span)?;
                    field_ty
                }
                ir::PlaceProjection::Index { span, .. } => {
                    let (elem_ty, _, _) =
                        self.vec_element_info(&current, "index assignment", *span)?;
                    elem_ty
                }
            };
        }
        Some(current)
    }

    fn place_field(
        &mut self,
        ty: &LType,
        name: &str,
        target: &str,
        span: crate::span::Span,
    ) -> Option<(usize, LType)> {
        let LType::Struct(layout) = ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5005",
                format!(
                    "cannot assign to field '{}' under '{}': '{}' is not a struct",
                    name,
                    target,
                    render_type(ty)
                ),
                self.file,
                span,
            ));
            return None;
        };
        let Some((index, field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == name)
        else {
            self.diagnostics.push(Diagnostic::error(
                "E5005",
                format!("struct '{}' has no field '{}'", layout.repr, name),
                self.file,
                span,
            ));
            return None;
        };
        Some((index, field.ty.clone()))
    }

    /// Returns `current` with the place `path` replaced by `value`.
    fn rebuild_place(
        &mut self,
        current: Value,
        path: &[ir::PlaceProjection],
        value: Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((first, rest)) = path.split_first() else {
            return Some(value);
        };
        match first {
            ir::PlaceProjection::Field {
                name,
                span: field_span,
            } => {
                let (index, field_ty) =
                    self.place_field(&current.ty, name, "place", *field_span)?;
                let aggregate_ty = llvm_type(&current.ty);
                let aggregate = current
                    .repr
                    .clone()
                    .unwrap_or_else(|| default_value(&current.ty));
                let old_field = self.new_temp();
                fctx.lines.push(format!(
                    "  {old_field} = extractvalue {aggregate_ty} {aggregate}, {index}"
                ));
                let new_field = self.rebuild_place(
                    Value {
                        ty: field_ty.clone(),
                        repr: Some(old_field),
                    },
                    rest,
                    value,
                    span,
                    fctx,
                )?;
                let rebuilt = self.new_temp();
                fctx.lines.push(format!(
                    "  {} = insertvalue {} {}, {} {}, {}",
                    rebuilt,
                    aggregate_ty,
                    aggregate,
                    llvm_type(&field_ty),
                    new_field.repr.unwrap_or_else(|| default_value(&field_ty)),
                    index
                ));
                let rebuilt = Value {
                    ty: current.ty.clone(),
                    repr: Some(rebuilt),
                };
                self.recheck_struct_invariant(rebuilt, span, fctx)
            }
            ir::PlaceProjection::Index {
                binding,
                span: index_span,
            } => {
                let index = self.load_place_index(binding, *index_span, fctx)?;
                let (elem_ty, _, _) =
                    self.vec_element_info(&current.ty, "index assignment", *index_span)?;
                let new_elem = if rest.is_empty() {
                    value
                } else {
                    let old_elem =
                        self.vec_element_or_panic(&current, &index, &elem_ty, *index_span, fctx)?;
                    self.rebuild_place(old_elem, rest, value, span, fctx)?
                };
                self.vec_set_or_panic(&current, &index, &new_elem, &elem_ty, *index_span, fctx)
            }
        }
    }

    /// Reads `vec[index]`, panicking when the index is out of bounds.
    fn vec_element_or_panic(
        &mut self,
        vec_value: &Value,
        index: &str,
        elem_ty: &LType,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let (ptr, len, cap) = self.vec_ptr_len_cap_i8(vec_value, span, fctx)?;
        let elem_size = self.vec_elem_size(elem_ty, fctx);
        let out_slot = self.alloc_entry_slot(elem_ty, fctx);
        let out_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = bitcast {}* {} to i8*",
            out_ptr,
            llvm_type(elem_ty),
            out_slot
        ));
        let found = self.new_temp();
        fctx.lines.push(format!(
            "  {found} = call i64 @aic_rt_vec_get(i8* {ptr}, i64 {len}, i64 {cap}, i64 {index}, i64 {elem_size}, i8* {out_ptr})"
        ));
        self.panic_unless_in_bounds(&found, span, fctx);
        let loaded = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            loaded,
            llvm_type(elem_ty),
            llvm_type(elem_ty),
            out_slot
        ));
        Some(Value {
            ty: elem_ty.clone(),
            repr: Some(loaded),
        })
    }

    /// Returns a copy of `vec` with `vec[index] = value`, panicking when the
    /// index is out of bounds.
    fn vec_set_or_panic(
        &mut self,
        vec_value: &Value,
        index: &str,
        value: &Value,
        elem_ty: &LType,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let elem_size = self.vec_elem_size(elem_ty, fctx);
        let value_ptr = self.value_to_i8_ptr(value, fctx);
        let (ptr_slot, len_slot, cap_slot) = self.vec_slots_from_value(vec_value, span, fctx)?;
        let updated = self.new_temp();
        fctx.lines.push(format!(
            "  {updated} = call i64 @aic_rt_vec_set(i8** {ptr_slot}, i64* {len_slot}, i64* {cap_slot}, i64 {index}, i64 {elem_size}, i8* {value_ptr})"
        ));
        self.panic_unless_in_bounds(&updated, span, fctx);
        self.load_vec_from_slots(&vec_value.ty, &ptr_slot, &len_slot, &cap_slot, span, fctx)
    }

    /// Branches to a panic when the runtime status `status` is 0.
    fn panic_unless_in_bounds(&mut self, status: &str, span: crate::span::Span, fctx: &mut FnCtx) {
        let ok = self.new_temp();
        fctx.lines.push(format!("  {ok} = icmp ne i64 {status}, 0"));
        let fail_label = self.new_label("place_index_oob");
        let ok_label = self.new_label("place_index_ok");
        fctx.lines.push(format!(
            "  br i1 {ok}, label %{ok_label}, label %{fail_label}"
        ));
        fctx.lines.push(format!("{fail_label}:"));
        self.emit_static_panic(
            "index_out_of_bounds{op:assign,type:Vec}: Vec index is out of bounds in assignment",
            span,
            fctx,
        );
        fctx.lines.push(format!("{ok_label}:"));
        fctx.current_label = ok_label;
    }

    /// Runs the struct's invariant helper on a rebuilt value when the
    /// struct declares an invariant.
    fn recheck_struct_invariant(
        &mut self,
        value: Value,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Struct(layout) = &value.ty else {
            return Some(value);
        };
        let helper = format!("{INVARIANT_HELPER_PREFIX}{}", base_type_name(&layout.repr));
        if !self.has_callable_name(&helper) {
            return Some(value);
        }
        let aggregate_ty = llvm_type(&value.ty);
        let aggregate = value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(&value.ty));
        let mut fields = Vec::with_capacity(layout.fields.len());
        for (index, field) in layout.fields.iter().enumerate() {
            let reg = self.new_temp();
            fctx.lines.push(format!(
                "  {reg} = extractvalue {aggregate_ty} {aggregate}, {index}"
            ));
            fields.push(Value {
                ty: field.ty.clone(),
                repr: Some(reg),
            });
        }
        // The invariant helper is generated by contract lowering after the
        // checker ran, so no checker symbol exists for this call.
        self.gen_named_function_call_with_values(&helper, None, fields, span, fctx)
    }
}
