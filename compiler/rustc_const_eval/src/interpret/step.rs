//! This module contains the `InterpCx` methods for executing a single step of the interpreter.
//!
//! The main entry point is the `step` method.

use either::Either;

use rustc_index::IndexSlice;
use rustc_middle::mir;
use rustc_middle::ty::layout::LayoutOf;
use rustc_middle::{bug, span_bug};
use rustc_target::abi::{FieldIdx, FIRST_VARIANT};

use super::{
    ImmTy, Immediate, InterpCx, InterpResult, Machine, MemPlaceMeta, PlaceTy, Projectable, Scalar,
};
use crate::util;

impl<'mir, 'tcx: 'mir, M: Machine<'mir, 'tcx>> InterpCx<'mir, 'tcx, M> {
    /// Returns `true` as long as there are more things to do.
    ///
    /// This is used by [priroda](https://github.com/oli-obk/priroda)
    ///
    /// This is marked `#inline(always)` to work around adversarial codegen when `opt-level = 3`
    #[inline(always)]
    pub fn step(&mut self) -> InterpResult<'tcx, bool> {
        if self.stack().is_empty() {
            return Ok(false);
        }

        let Either::Left(loc) = self.frame().loc else {
            // We are unwinding and this fn has no cleanup code.
            // Just go on unwinding.
            trace!("unwinding: skipping frame");
            self.pop_stack_frame(/* unwinding */ true)?;
            return Ok(true);
        };
        let basic_block = &self.body().basic_blocks[loc.block];

        if let Some(stmt) = basic_block.statements.get(loc.statement_index) {
            let old_frames = self.frame_idx();
            self.statement(stmt)?;
            // Make sure we are not updating `statement_index` of the wrong frame.
            assert_eq!(old_frames, self.frame_idx());
            // Advance the program counter.
            self.frame_mut().loc.as_mut().left().unwrap().statement_index += 1;
            return Ok(true);
        }

        M::before_terminator(self)?;

        let terminator = basic_block.terminator();
        self.terminator(terminator)?;
        Ok(true)
    }

    /// Runs the interpretation logic for the given `mir::Statement` at the current frame and
    /// statement counter.
    ///
    /// This does NOT move the statement counter forward, the caller has to do that!
    pub fn statement(&mut self, stmt: &mir::Statement<'tcx>) -> InterpResult<'tcx> {
        info!("{:?}", stmt);

        use rustc_middle::mir::StatementKind::*;

        match &stmt.kind {
            Assign(box (place, rvalue)) => self.eval_rvalue_into_place(rvalue, *place)?,

            SetDiscriminant { place, variant_index } => {
                let dest = self.eval_place(**place)?;
                self.write_discriminant(*variant_index, &dest)?;
            }

            Deinit(place) => {
                let dest = self.eval_place(**place)?;
                self.write_uninit(&dest)?;
            }

            // Mark locals as alive
            StorageLive(local) => {
                self.storage_live(*local)?;
            }

            // Mark locals as dead
            StorageDead(local) => {
                self.storage_dead(*local)?;
            }

            // No dynamic semantics attached to `FakeRead`; MIR
            // interpreter is solely intended for borrowck'ed code.
            FakeRead(..) => {}

            // Stacked Borrows.
            Retag(kind, place) => {
                let dest = self.eval_place(**place)?;
                M::retag_place_contents(self, *kind, &dest)?;
            }

            Intrinsic(box intrinsic) => self.emulate_nondiverging_intrinsic(intrinsic)?,

            // Evaluate the place expression, without reading from it.
            PlaceMention(box place) => {
                let _ = self.eval_place(*place)?;
            }

            // This exists purely to guide borrowck lifetime inference, and does not have
            // an operational effect.
            AscribeUserType(..) => {}

            // Currently, Miri discards Coverage statements. Coverage statements are only injected
            // via an optional compile time MIR pass and have no side effects. Since Coverage
            // statements don't exist at the source level, it is safe for Miri to ignore them, even
            // for undefined behavior (UB) checks.
            //
            // A coverage counter inside a const expression (for example, a counter injected in a
            // const function) is discarded when the const is evaluated at compile time. Whether
            // this should change, and/or how to implement a const eval counter, is a subject of the
            // following issue:
            //
            // FIXME(#73156): Handle source code coverage in const eval
            Coverage(..) => {}

            ConstEvalCounter => {
                M::increment_const_eval_counter(self)?;
            }

            // Defined to do nothing. These are added by optimization passes, to avoid changing the
            // size of MIR constantly.
            Nop => {}
        }

        Ok(())
    }

    /// Evaluate an assignment statement.
    ///
    /// There is no separate `eval_rvalue` function. Instead, the code for handling each rvalue
    /// type writes its results directly into the memory specified by the place.
    pub fn eval_rvalue_into_place(
        &mut self,
        rvalue: &mir::Rvalue<'tcx>,
        place: mir::Place<'tcx>,
    ) -> InterpResult<'tcx> {
        let dest = self.eval_place(place)?;
        // FIXME: ensure some kind of non-aliasing between LHS and RHS?
        // Also see https://github.com/rust-lang/rust/issues/68364.

        use rustc_middle::mir::Rvalue::*;
        match *rvalue {
            ThreadLocalRef(did) => {
                let ptr = M::thread_local_static_pointer(self, did)?;
                self.write_pointer(ptr, &dest)?;
            }

            Use(ref operand) => {
                // Avoid recomputing the layout
                let op = self.eval_operand(operand, Some(dest.layout))?;
                self.copy_op(&op, &dest)?;
            }

            CopyForDeref(place) => {
                let op = self.eval_place_to_op(place, Some(dest.layout))?;
                self.copy_op(&op, &dest)?;
            }

            BinaryOp(bin_op, box (ref left, ref right)) => {
                let layout = util::binop_left_homogeneous(bin_op).then_some(dest.layout);
                let left = self.read_immediate(&self.eval_operand(left, layout)?)?;
                let layout = util::binop_right_homogeneous(bin_op).then_some(left.layout);
                let right = self.read_immediate(&self.eval_operand(right, layout)?)?;
                self.binop_ignore_overflow(bin_op, &left, &right, &dest)?;
            }

            CheckedBinaryOp(bin_op, box (ref left, ref right)) => {
                // Due to the extra boolean in the result, we can never reuse the `dest.layout`.
                let left = self.read_immediate(&self.eval_operand(left, None)?)?;
                let layout = util::binop_right_homogeneous(bin_op).then_some(left.layout);
                let right = self.read_immediate(&self.eval_operand(right, layout)?)?;
                self.binop_with_overflow(bin_op, &left, &right, &dest)?;
            }

            UnaryOp(un_op, ref operand) => {
                // The operand always has the same type as the result.
                let val = self.read_immediate(&self.eval_operand(operand, Some(dest.layout))?)?;
                let val = self.wrapping_unary_op(un_op, &val)?;
                assert_eq!(val.layout, dest.layout, "layout mismatch for result of {un_op:?}");
                self.write_immediate(*val, &dest)?;
            }

            Aggregate(box ref kind, ref operands) => {
                self.write_aggregate(kind, operands, &dest)?;
            }

            Repeat(ref operand, _) => {
                self.write_repeat(operand, &dest)?;
            }

            Len(place) => {
                let src = self.eval_place(place)?;
                let len = src.len(self)?;
                self.write_scalar(Scalar::from_target_usize(len, self), &dest)?;
            }

            Ref(_, borrow_kind, place) => {
                let src = self.eval_place(place)?;
                let place = self.force_allocation(&src)?;
                let val = ImmTy::from_immediate(place.to_ref(self), dest.layout);
                // A fresh reference was created, make sure it gets retagged.
                let val = M::retag_ptr_value(
                    self,
                    if borrow_kind.allows_two_phase_borrow() {
                        mir::RetagKind::TwoPhase
                    } else {
                        mir::RetagKind::Default
                    },
                    &val,
                )?;
                self.write_immediate(*val, &dest)?;
            }

            AddressOf(_, place) => {
                // Figure out whether this is an addr_of of an already raw place.
                let place_base_raw = if place.is_indirect_first_projection() {
                    let ty = self.frame().body.local_decls[place.local].ty;
                    ty.is_unsafe_ptr()
                } else {
                    // Not a deref, and thus not raw.
                    false
                };

                let src = self.eval_place(place)?;
                let place = self.force_allocation(&src)?;
                let mut val = ImmTy::from_immediate(place.to_ref(self), dest.layout);
                if !place_base_raw {
                    // If this was not already raw, it needs retagging.
                    val = M::retag_ptr_value(self, mir::RetagKind::Raw, &val)?;
                }
                self.write_immediate(*val, &dest)?;
            }

            NullaryOp(ref null_op, ty) => {
                let ty = self.instantiate_from_current_frame_and_normalize_erasing_regions(ty)?;
                let layout = self.layout_of(ty)?;
                if let mir::NullOp::SizeOf | mir::NullOp::AlignOf = null_op
                    && layout.is_unsized()
                {
                    span_bug!(
                        self.frame().current_span(),
                        "{null_op:?} MIR operator called for unsized type {ty}",
                    );
                }
                let val = match null_op {
                    mir::NullOp::SizeOf => {
                        let val = layout.size.bytes();
                        Scalar::from_target_usize(val, self)
                    }
                    mir::NullOp::AlignOf => {
                        let val = layout.align.abi.bytes();
                        Scalar::from_target_usize(val, self)
                    }
                    mir::NullOp::OffsetOf(fields) => {
                        let val = layout.offset_of_subfield(self, fields.iter()).bytes();
                        Scalar::from_target_usize(val, self)
                    }
                    mir::NullOp::UbChecks => Scalar::from_bool(self.tcx.sess.ub_checks()),
                };
                self.write_scalar(val, &dest)?;
            }

            ShallowInitBox(ref operand, _) => {
                let src = self.eval_operand(operand, None)?;
                let v = self.read_immediate(&src)?;
                self.write_immediate(*v, &dest)?;
            }

            Cast(cast_kind, ref operand, cast_ty) => {
                let src = self.eval_operand(operand, None)?;
                let cast_ty =
                    self.instantiate_from_current_frame_and_normalize_erasing_regions(cast_ty)?;
                self.cast(&src, cast_kind, cast_ty, &dest)?;
            }

            Discriminant(place) => {
                let op = self.eval_place_to_op(place, None)?;
                let variant = self.read_discriminant(&op)?;
                let discr = self.discriminant_for_variant(op.layout.ty, variant)?;
                self.write_immediate(*discr, &dest)?;
            }
        }

        trace!("{:?}", self.dump_place(&dest));

        Ok(())
    }

    /// Writes the aggregate to the destination.
    #[instrument(skip(self), level = "trace")]
    fn write_aggregate(
        &mut self,
        kind: &mir::AggregateKind<'tcx>,
        operands: &IndexSlice<FieldIdx, mir::Operand<'tcx>>,
        dest: &PlaceTy<'tcx, M::Provenance>,
    ) -> InterpResult<'tcx> {
        self.write_uninit(dest)?; // make sure all the padding ends up as uninit
        let (variant_index, variant_dest, active_field_index) = match *kind {
            mir::AggregateKind::Adt(_, variant_index, _, _, active_field_index) => {
                let variant_dest = self.project_downcast(dest, variant_index)?;
                (variant_index, variant_dest, active_field_index)
            }
            mir::AggregateKind::RawPtr(..) => {
                // Pointers don't have "fields" in the normal sense, so the
                // projection-based code below would either fail in projection
                // or in type mismatches. Instead, build an `Immediate` from
                // the parts and write that to the destination.
                let [data, meta] = &operands.raw else {
                    bug!("{kind:?} should have 2 operands, had {operands:?}");
                };
                let data = self.eval_operand(data, None)?;
                let data = self.read_pointer(&data)?;
                let meta = self.eval_operand(meta, None)?;
                let meta = if meta.layout.is_zst() {
                    MemPlaceMeta::None
                } else {
                    MemPlaceMeta::Meta(self.read_scalar(&meta)?)
                };
                let ptr_imm = Immediate::new_pointer_with_meta(data, meta, self);
                let ptr = ImmTy::from_immediate(ptr_imm, dest.layout);
                self.copy_op(&ptr, dest)?;
                return Ok(());
            }
            _ => (FIRST_VARIANT, dest.clone(), None),
        };
        if active_field_index.is_some() {
            assert_eq!(operands.len(), 1);
        }
        for (field_index, operand) in operands.iter_enumerated() {
            let field_index = active_field_index.unwrap_or(field_index);
            let field_dest = self.project_field(&variant_dest, field_index.as_usize())?;
            let op = self.eval_operand(operand, Some(field_dest.layout))?;
            self.copy_op(&op, &field_dest)?;
        }
        self.write_discriminant(variant_index, dest)
    }

    /// Repeats `operand` into the destination. `dest` must have array type, and that type
    /// determines how often `operand` is repeated.
    fn write_repeat(
        &mut self,
        operand: &mir::Operand<'tcx>,
        dest: &PlaceTy<'tcx, M::Provenance>,
    ) -> InterpResult<'tcx> {
        let src = self.eval_operand(operand, None)?;
        assert!(src.layout.is_sized());
        let dest = self.force_allocation(&dest)?;
        let length = dest.len(self)?;

        if length == 0 {
            // Nothing to copy... but let's still make sure that `dest` as a place is valid.
            self.get_place_alloc_mut(&dest)?;
        } else {
            // Write the src to the first element.
            let first = self.project_index(&dest, 0)?;
            self.copy_op(&src, &first)?;

            // This is performance-sensitive code for big static/const arrays! So we
            // avoid writing each operand individually and instead just make many copies
            // of the first element.
            let elem_size = first.layout.size;
            let first_ptr = first.ptr();
            let rest_ptr = first_ptr.offset(elem_size, self)?;
            // No alignment requirement since `copy_op` above already checked it.
            self.mem_copy_repeatedly(
                first_ptr,
                rest_ptr,
                elem_size,
                length - 1,
                /*nonoverlapping:*/ true,
            )?;
        }

        Ok(())
    }

    /// Evaluate the given terminator. Will also adjust the stack frame and statement position accordingly.
    fn terminator(&mut self, terminator: &mir::Terminator<'tcx>) -> InterpResult<'tcx> {
        info!("{:?}", terminator.kind);

        self.eval_terminator(terminator)?;
        if !self.stack().is_empty() {
            if let Either::Left(loc) = self.frame().loc {
                info!("// executing {:?}", loc.block);
            }
        }
        Ok(())
    }
}
