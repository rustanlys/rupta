// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! Builds the Pointer Assignment Graph (PAG) for a single function.
//!
//! The Function PAG is part of the PAG for the whole program.

use log::*;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Result};
use std::rc::Rc;

use rustc_hir::def::DefKind;
use rustc_hir::def_id::DefId;
use rustc_index::IndexVec;
use rustc_middle::mir;
use rustc_middle::mir::interpret::{GlobalAlloc, Scalar};
use rustc_middle::ty;
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{Const, GenericArgsRef, Ty, TyCtxt, TyKind};
use rustc_span::source_map::Spanned;
use rustc_target::abi::FieldIdx;

use crate::builder::{call_graph_builder, special_function_handler};
use crate::graph::func_pag::FuncPAG;
use crate::graph::pag::PAGEdgeEnum;
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::call_site::CallSite;
use crate::mir::function::{FuncId, FunctionReference};
use crate::mir::path::{Path, PathEnum, PathSelector, PathSupport, ProjectionElems};
use crate::util::{self, type_util};

use super::substs_specializer::SubstsSpecializer;

/// A visitor that traverses the MIR associated with a particular function's body and
/// build the function's pointer assignment graph.
pub struct FuncPAGBuilder<'pta, 'tcx, 'compilation> {
    pub(crate) acx: &'pta mut AnalysisContext<'tcx, 'compilation>,
    pub(crate) func_id: FuncId,
    pub(crate) func_ref: Rc<FunctionReference<'tcx>>,
    pub(crate) mir: &'tcx mir::Body<'tcx>,
    /// Pointer Assignment Graph for this function.
    pub(crate) fpag: &'pta mut FuncPAG,

    /// For specializing the generic type in the function.
    substs_specializer: SubstsSpecializer<'tcx>,

    /// Caches the path for each place visited in this function
    path_cache: HashMap<mir::Place<'tcx>, Rc<Path>>,
}

impl<'pta, 'tcx, 'compilation> Debug for FuncPAGBuilder<'pta, 'tcx, 'compilation> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        "FuncPAGBuilder".fmt(f)
    }
}

impl<'pta, 'tcx, 'compilation> FuncPAGBuilder<'pta, 'tcx, 'compilation> {
    pub fn new(
        acx: &'pta mut AnalysisContext<'tcx, 'compilation>,
        func_id: FuncId,
        mir: &'tcx mir::Body<'tcx>,
        fpag: &'pta mut FuncPAG,
    ) -> FuncPAGBuilder<'pta, 'tcx, 'compilation> {
        let func_ref = acx.get_function_reference(func_id);
        debug!("Building FuncPAG for {:?}: {}", func_id, func_ref.to_string());

        // if func_ref.promoted.is_none() {
        //     util::pretty_print_mir(acx.tcx, func_ref.def_id);
        // }
        let substs_specializer = SubstsSpecializer::new(acx.tcx, func_ref.generic_args.clone());
        let aux_local_index = mir.local_decls.len();
        acx.aux_local_indexer.insert(func_id, aux_local_index);
        FuncPAGBuilder {
            acx,
            func_id,
            func_ref,
            mir,
            fpag,
            substs_specializer,
            path_cache: HashMap::new(),
        }
    }

    #[inline]
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.acx.tcx
    }

    #[inline]
    fn def_id(&self) -> DefId {
        self.func_ref.def_id
    }

    /// Returns true if this function corresponds to an initialization procedure
    /// for a promoted constant.
    #[inline]
    fn is_promoted(&self) -> bool {
        self.func_ref.promoted.is_some()
    }

    /// Returns true if this function corresponds to an initialization procedure
    /// for a static item.
    #[inline]
    fn is_static(&self) -> bool {
        self.acx.tcx.is_static(self.def_id())
    }

    /// Returns true if this function corresponds to an initialization procedure
    /// for a const item.
    #[inline]
    fn is_const(&self) -> bool {
        matches!(self.tcx().def_kind(self.def_id()), DefKind::Const)
    }

    /// Builds the PAG.
    pub fn build(&mut self) {
        self.visit_body();

        // Add extra edges between the return value and the promoted_path/static_path
        // if the function body corresponds to a promoted body or a static's body
        if let Some(promoted) = self.func_ref.promoted {
            let ret_path = Path::new_return_value(self.func_id);
            let ret_type = self
                .acx
                .get_path_rustc_type(&ret_path)
                .expect("Unresolved result type");
            let promoted_path = Path::new_promoted(self.def_id(), promoted.into());
            self.acx.set_path_rustc_type(promoted_path.clone(), ret_type);
            self.add_internal_edges(ret_path, ret_type, promoted_path, ret_type);
        } else if self.is_static() || self.is_const() {
            let ret_path = Path::new_return_value(self.func_id);
            let ret_type = self
                .acx
                .get_path_rustc_type(&ret_path)
                .expect("Unresolved result type");
            let static_variable = Path::new_static_variable(self.def_id());
            self.acx.set_path_rustc_type(static_variable.clone(), ret_type);
            self.add_internal_edges(ret_path, ret_type, static_variable, ret_type);
        }
    }

    pub fn visit_body(&mut self) {
        for bb in self.mir.basic_blocks.indices() {
            self.visit_basic_block(bb);
        }
    }

    fn visit_basic_block(&mut self, bb: mir::BasicBlock) {
        let mir::BasicBlockData {
            ref statements,
            ref terminator,
            ..
        } = &self.mir[bb];
        let mut location = bb.start_location();
        let terminator_index = statements.len();

        while location.statement_index < terminator_index {
            self.visit_statement(location, &statements[location.statement_index]);
            location.statement_index += 1;
        }

        if let Some(mir::Terminator {
            ref source_info,
            ref kind,
        }) = *terminator
        {
            self.visit_terminator(location, kind, *source_info);
        }
    }

    /// Calls a specialized visitor for each kind of statement.
    fn visit_statement(&mut self, _location: mir::Location, statement: &mir::Statement<'tcx>) {
        // debug!("Visiting statement: {:?}", statement);
        let mir::Statement { kind, source_info: _ } = statement;
        match kind {
            mir::StatementKind::Assign(box (place, rvalue)) => self.visit_assign(place, rvalue),
            mir::StatementKind::FakeRead(..) => (),
            mir::StatementKind::SetDiscriminant { place, variant_index } => {
                self.visit_set_discriminant(place, *variant_index)
            }
            mir::StatementKind::Deinit(box place) => self.visit_deinit(place),
            mir::StatementKind::StorageLive(local) => self.visit_storage_live(*local),
            mir::StatementKind::StorageDead(local) => self.visit_storage_dead(*local),
            mir::StatementKind::Retag(retag_kind, place) => self.visit_retag(*retag_kind, place),
            mir::StatementKind::PlaceMention(..) => (),
            mir::StatementKind::AscribeUserType(..) => (),
            mir::StatementKind::Coverage(..) => (),
            mir::StatementKind::Intrinsic(box non_diverging_intrinsic) => {
                self.visit_non_diverging_intrinsic(non_diverging_intrinsic);
            }
            mir::StatementKind::ConstEvalCounter => (),
            mir::StatementKind::Nop => (),
        }
    }

    /// An assignment statement writes the RHS Rvalue to the LHS Place.
    fn visit_assign(&mut self, place: &mir::Place<'tcx>, rvalue: &mir::Rvalue<'tcx>) {
        let (lh_path, lh_type) = self.get_path_and_type_for_place(place);

        // Skip this assignment if the destination path is not pointer and does not
        // contain pointer type fields.
        // The lh type maybe a opaque type, we need to determine the actual type
        // according to the rh type.
        if !lh_type.is_any_ptr() && self.acx.get_pointer_projections(lh_type).is_empty() {
            return;
        }

        self.visit_rvalue(lh_path.clone(), rvalue);

        // If this assignment writes to a field or subfield of a union, add edges
        // between the union fields that share the same memory offset.
        self.cast_between_union_fields(&lh_path);
    }

    /// Denotes a call to the intrinsic function copy_nonoverlapping, where `src` and `dst` denotes the
    /// memory being read from and written to and size indicates how many bytes are being copied over.
    /// `src` and `dst` must each be a reference, pointer, or `Box` pointing to the same type T.
    /// A copy_nonoverlapping statement can be regarded as a statement like `*dst = *src`.
    fn visit_copy_non_overlapping(&mut self, copy_info: &mir::CopyNonOverlapping<'tcx>) {
        let mut get_ptr_path = |operand: &mir::Operand<'tcx>| -> Rc<Path> {
            match operand {
                mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                    let (path, ty) = self.get_path_and_type_for_place(place);
                    match ty.kind() {
                        TyKind::Ref(..) | TyKind::RawPtr(..) => path,
                        TyKind::Adt(def, _args) if def.is_box() => {
                            self.get_box_pointer_field(path, ty.boxed_ty())
                        }
                        _ => unreachable!("CopyNonOverlapping is called on non-ptr operands."),
                    }
                }
                mir::Operand::Constant(..) => unreachable!(),
            }
        };
        let src_ptr = get_ptr_path(&copy_info.src);
        let dst_ptr = get_ptr_path(&copy_info.dst);

        // convert it to `` let aux = *src_ptr; *dst_ptr = aux ``
        let deref_ty = type_util::get_dereferenced_type(self.acx.get_path_rustc_type(&src_ptr).unwrap());
        let aux = self.create_aux_local(deref_ty);
        let src_deref = Path::new_deref(src_ptr);
        self.acx.set_path_rustc_type(src_deref.clone(), deref_ty);
        self.add_internal_edges(src_deref, deref_ty, aux.clone(), deref_ty);
        let dst_deref = Path::new_deref(dst_ptr);
        self.acx.set_path_rustc_type(dst_deref.clone(), deref_ty);
        self.add_internal_edges(aux, deref_ty, dst_deref, deref_ty);
    }

    /// Writes the discriminant for a variant to the enum Place.
    fn visit_set_discriminant(
        &mut self,
        _place: &mir::Place<'tcx>,
        _variant_index: rustc_target::abi::VariantIdx,
    ) {
    }

    /// Deinitializes the place. This writes `uninit` bytes to the entire place.
    fn visit_deinit(&mut self, _place: &mir::Place<'tcx>) {}

    /// Start a live range for the storage of the local.
    fn visit_storage_live(&mut self, _local: mir::Local) {}

    /// End the current live range for the storage of the local.
    fn visit_storage_dead(&mut self, _local: mir::Local) {}

    /// Retag references in the given place, ensuring they got fresh tags.  This is
    /// part of the Stacked Borrows model. These statements are currently only interpreted
    /// by miri and only generated when "-Z mir-emit-retag" is passed.
    /// See <https://internals.rust-lang.org/t/stacked-borrows-an-aliasing-model-for-rust/8153/>
    /// for more details.
    fn visit_retag(&self, _retag_kind: mir::RetagKind, _place: &mir::Place<'tcx>) {
        // This seems to be an intermediate artifact of MIR generation and is related to aliasing.
        // We currently simply ignore this.
    }

    /// Denotes a call to an intrinsic that does not require an unwind path and always returns.
    fn visit_non_diverging_intrinsic(
        &mut self,
        visit_non_diverging_intrinsic: &mir::NonDivergingIntrinsic<'tcx>,
    ) {
        match visit_non_diverging_intrinsic {
            mir::NonDivergingIntrinsic::CopyNonOverlapping(copy_info) => {
                self.visit_copy_non_overlapping(copy_info);
            }
            mir::NonDivergingIntrinsic::Assume(_operand) => {}
        }
    }

    /// Terminator for a basic block.
    /// We only analyze the call statements in a flow-insensitive pointer analysis.
    fn visit_terminator(
        &mut self,
        location: mir::Location,
        kind: &mir::TerminatorKind<'tcx>,
        _source_info: mir::SourceInfo,
    ) {
        match kind {
            mir::TerminatorKind::Call {
                func,
                args,
                destination,
                target: _,
                unwind: _,
                call_source: _,
                fn_span: _,
            } => self.visit_call(func, args, destination, location),
            mir::TerminatorKind::InlineAsm {
                template: _,
                operands: _,
                destination: _,
                ..
            } => {}
            _ => {}
        }
    }

    /// Block ends with the call of a function.
    ///
    /// #Arguments
    /// * `func` - The function that’s being called
    /// * `args` - Arguments the function is called with. These are owned by the callee, which is
    /// free to modify them. This allows the memory occupied by "by-value" arguments to be reused
    /// across function calls without duplicating the contents.
    /// * `destination` - Destination for the return value. If some, the call returns a value.
    fn visit_call(
        &mut self,
        func: &mir::Operand<'tcx>,
        args: &Vec<Spanned<mir::Operand<'tcx>>>,
        destination: &mir::Place<'tcx>,
        location: mir::Location,
    ) {
        match func {
            mir::Operand::Constant(box constant) => match constant.ty().kind() {
                TyKind::Closure(callee_def_id, gen_args)
                | TyKind::FnDef(callee_def_id, gen_args)
                | TyKind::Coroutine(callee_def_id, gen_args) => {
                    self.resolve_call(callee_def_id, gen_args, args, destination, location)
                }
                TyKind::FnPtr(_) => {
                    let fnptr = self.visit_const_operand(constant);
                    debug!("Constant function pointer: {:?}", fnptr);
                    let args = self.visit_args(args);
                    let destination = self.get_path_for_place(destination);
                    let callsite = self.new_callsite(self.func_id, location, args, destination);
                    self.fpag.add_fnptr_callsite(fnptr, callsite);
                }
                _ => {
                    error!("Unexpected call: {:?}, type: {:?}", constant, constant.ty());
                }
            },
            mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                let (fn_item, fn_item_ty) = self.get_path_and_type_for_place(place);
                assert!(fn_item_ty.is_fn());
                match fn_item_ty.kind() {
                    TyKind::FnDef(callee_def_id, gen_args) => {
                        self.resolve_call(callee_def_id, gen_args, args, destination, location)
                    }
                    TyKind::FnPtr(..) => {
                        let args = self.visit_args(args);
                        let destination = self.get_path_for_place(destination);
                        let callsite = self.new_callsite(self.func_id, location, args, destination);
                        self.fpag.add_fnptr_callsite(fn_item, callsite);
                    }
                    _ => {
                        unreachable!();
                    }
                }
            }
        }
    }

    fn visit_args(&mut self, args: &Vec<Spanned<mir::Operand<'tcx>>>) -> Vec<Rc<Path>> {
        let mut args_paths = Vec::<Rc<Path>>::with_capacity(args.len());
        for arg in args {
            match &arg.node {
                mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                    args_paths.push(self.get_path_for_place(place));
                }
                mir::Operand::Constant(const_op) => {
                    args_paths.push(self.visit_const_operand(const_op));
                }
            }
        }
        args_paths
    }

    /// Calls a specialized visitor for each kind of Rvalue.
    fn visit_rvalue(&mut self, lh_path: Rc<Path>, rvalue: &mir::Rvalue<'tcx>) {
        match rvalue {
            mir::Rvalue::Use(operand) => {
                self.visit_use(lh_path, operand);
            }
            mir::Rvalue::Repeat(operand, count) => {
                self.visit_repeat(lh_path, operand, count);
            }
            mir::Rvalue::Ref(_, _, place) | mir::Rvalue::AddressOf(_, place) => {
                self.visit_ref_or_address_of(lh_path, place);
            }
            mir::Rvalue::ThreadLocalRef(_def_id) => {}
            mir::Rvalue::Len(_place) => {}
            mir::Rvalue::Cast(cast_kind, operand, ty) => {
                let specialized_ty = self.substs_specializer.specialize_generic_argument_type(*ty);
                self.visit_cast(lh_path, *cast_kind, operand, specialized_ty);
            }
            mir::Rvalue::BinaryOp(bin_op, box (left_operand, right_operand)) => {
                self.visit_binary_op(lh_path, *bin_op, left_operand, right_operand);
            }
            mir::Rvalue::CheckedBinaryOp(_bin_op, box (_left_operand, _right_operand)) => {}
            mir::Rvalue::NullaryOp(..) | mir::Rvalue::UnaryOp(..) | mir::Rvalue::Discriminant(..) => {}
            mir::Rvalue::Aggregate(aggregate_kind, operands) => {
                self.visit_aggregate(lh_path, aggregate_kind, operands);
            }
            mir::Rvalue::ShallowInitBox(operand, ty) => {
                self.visit_shallow_init_box(lh_path, operand, *ty);
            }
            mir::Rvalue::CopyForDeref(place) => {
                // A CopyForDeref is equivalent to a read from a place at the codegen level,
                // but is treated specially by drop elaboration. When such a read happens,
                // it is guaranteed (via nature of the mir_opt Derefer in
                // rustc_mir_transform/src/deref_separator) that the only use of the returned
                // value is a deref operation, immediately followed by one or more projections.
                self.visit_copy_or_move(lh_path, place);
            }
        }
    }

    /// `path = x` (either a move or copy, depending on type of `x`), or `path = constant`.
    fn visit_use(&mut self, lh_path: Rc<Path>, operand: &mir::Operand<'tcx>) {
        match operand {
            // Currently we do not seperate copy and move cases.
            mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                self.visit_copy_or_move(lh_path, place);
            }
            mir::Operand::Constant(const_op) => {
                self.visit_constant_assign(lh_path, const_op.borrow());
            }
        }
    }

    fn visit_copy_or_move(&mut self, lh_path: Rc<Path>, place: &mir::Place<'tcx>) {
        let lh_type = self
            .acx
            .get_path_rustc_type(&lh_path)
            .expect("Unresolved lh type");
        let (rh_path, rh_type) = self.get_path_and_type_for_place(place);

        // Update lh_type if it is a opaque type
        if lh_type.is_impl_trait() {
            // debug!("Update lh opaque type with {:?}", rh_type);
            if rh_type.is_impl_trait() {
                error!("Rh type {:?} is a opaque type", rh_type);
            }
            self.acx.set_path_rustc_type(lh_path.clone(), rh_type);
        }

        // An assignment of format: (*lbase).elem = (*rbase).elem
        if lh_path.is_deref_path() && rh_path.is_deref_path() {
            debug!(
                "Assignment: (*lbase).elem = (*rbase).elem: {:?} = {:?}",
                lh_path, rh_path
            );
            let aux = self.create_aux_local(rh_type);
            self.add_internal_edges(rh_path, rh_type, aux.clone(), rh_type);
            self.add_internal_edges(aux, rh_type, lh_path, lh_type);
            return;
        }

        self.add_internal_edges(rh_path, rh_type, lh_path, lh_type);
    }

    fn visit_constant_assign(&mut self, lh_path: Rc<Path>, const_op: &mir::ConstOperand<'tcx>) {
        let lh_type = self
            .acx
            .get_path_rustc_type(&lh_path)
            .expect("Unresolved lh type.");
        if !lh_type.is_any_ptr() && self.acx.get_pointer_projections(lh_type).is_empty() {
            return;
        }
        let rh_path = self.visit_const_operand(const_op);
        self.add_const_assign_edge(lh_path, rh_path);
    }

    fn add_const_assign_edge(&mut self, lh_path: Rc<Path>, rh_path: Rc<Path>) {
        if let Some(rh_type) = self.acx.get_path_rustc_type(&rh_path) {
            let lh_type = self
                .acx
                .get_path_rustc_type(&lh_path)
                .expect("Unresolved lh type");
            self.add_internal_edges(rh_path, rh_type, lh_path, lh_type);
        };
    }

    /// Returns a value that corresponds to the given literal.
    fn visit_const_operand(&mut self, const_op: &mir::ConstOperand<'tcx>) -> Rc<Path> {
        let mir::ConstOperand { const_, .. } = const_op;
        match const_ {
            // This constant came from the type system
            mir::Const::Ty(c) => self.visit_const(c),
            // An unevaluated mir constant which is not part of the type system.
            mir::Const::Unevaluated(c, ty) => self.visit_unevaluated_const(c, *ty),
            // This constant contains something the type system cannot handle (e.g. pointers).
            mir::Const::Val(v, ty) => self.visit_const_value(*v, *ty),
        }
    }

    /// Synthesizes a constant value from a RustC constant as used in the type system.
    fn visit_const(&mut self, c: &ty::Const<'tcx>) -> Rc<Path> {
        debug!("Visiting constant came from the type system: {c:?}");
        Path::new_constant()
    }

    /// Synthesizes a constant value from an unevaluated mir constant which is not part of the type system.
    fn visit_unevaluated_const(
        &mut self,
        unevaluated: &mir::UnevaluatedConst<'tcx>,
        ty: Ty<'tcx>,
    ) -> Rc<Path> {
        debug!("Visiting unevaluated constant: {unevaluated:?} {ty:?}");
        if let Some(promoted) = unevaluated.promoted {
            let promoted = Path::new_promoted(self.def_id(), promoted.index());
            self.acx.set_path_rustc_type(promoted.clone(), ty);
            return promoted;
        }
        let mut def_id = unevaluated.def;
        let def_ty = self.tcx().type_of(def_id);
        let args = self.substs_specializer.specialize_generic_args(unevaluated.args);
        debug!("resolving unevaluated def_id {:?} {:?}", def_id, def_ty);
        if !args.is_empty() {
            let param_env = rustc_middle::ty::ParamEnv::reveal_all();
            if let Ok(Some(instance)) =
                rustc_middle::ty::Instance::resolve(self.tcx(), param_env, def_id, args)
            {
                def_id = instance.def.def_id();
            }
        }
        if self.tcx().is_mir_available(def_id) {
            let static_variable = Path::new_static_variable(def_id);
            let static_variable_ty = self.tcx().type_of(def_id).skip_binder();
            self.acx
                .set_path_rustc_type(static_variable.clone(), static_variable_ty);
            self.fpag.add_static_variables_involved(static_variable.clone());
            return static_variable;
        }
        Path::new_constant()
    }

    /// This represents things the type system cannot handle (e.g. pointers), as well as
    /// everything else.
    fn visit_const_value(&mut self, val: mir::ConstValue<'tcx>, ty: Ty<'tcx>) -> Rc<Path> {
        debug!("Visiting constant value: {val:?} {ty:?}");
        match val {
            // A pointer.
            // We also store the size of the pointer, such that a `Scalar` always knows how big it is.
            // The size is always the pointer size of the current target, but this is not information
            // that we always have readily available.
            mir::ConstValue::Scalar(Scalar::Ptr(ptr, _size)) => {
                debug!("Visiting scalar pointer {ptr:?}");
                match self.tcx().try_get_global_alloc(ptr.provenance.alloc_id()) {
                    Some(GlobalAlloc::Memory(_alloc)) => {
                        // Todo: The alloc ID points to memory.
                        // We currently ignore the pointed-to memory of the constant.
                        let aux = self.create_aux_local(ty);
                        aux
                    }
                    Some(GlobalAlloc::Static(def_id)) => {
                        // the global alloc is a pointer to a static variable
                        let static_variable = Path::new_static_variable(def_id);
                        let static_variable_ty = self.tcx().type_of(def_id).skip_binder();
                        self.acx
                            .set_path_rustc_type(static_variable.clone(), static_variable_ty);
                        self.fpag.add_static_variables_involved(static_variable.clone());

                        // create an auxiliary variable for representing the global alloc const
                        let aux = self.create_aux_local(ty);
                        self.add_addr_edge(static_variable, aux.clone());
                        aux
                    }
                    _ => Path::new_constant(),
                }
            }
            mir::ConstValue::ZeroSized => match ty.kind() {
                TyKind::Closure(..) => self.new_closure_path(ty),
                TyKind::FnDef(def_id, args) => self.visit_function_reference(*def_id, args),
                _ => Path::new_constant(),
            },
            mir::ConstValue::Scalar(Scalar::Int(..))
            | mir::ConstValue::Slice { .. }
            | mir::ConstValue::Indirect { .. } => Path::new_constant(),
        }
    }

    /// Creates an array where each element is the value of the operand.
    /// Corresponds to source code like `[x; 32]`.
    fn visit_repeat(&mut self, lh_path: Rc<Path>, operand: &mir::Operand<'tcx>, _count: &Const<'tcx>) {
        let lh_type = self.acx.get_path_rustc_type(&lh_path).unwrap();
        if let TyKind::Array(elem_ty, _) = lh_type.kind() {
            let index_path = Path::new_index(lh_path.clone());
            self.acx.set_path_rustc_type(index_path.clone(), *elem_ty);
            self.visit_use(index_path, operand);
        }
    }

    /// Analyzes the `ref` and `address_of` assignments.
    ///
    /// Ref: Creates a reference of the indicated kind to the place. e.g. `path = &x` or `&mut x`
    /// AddressOf: Creates a pointer with the indicated mutability to the place.
    ///            This is generated by pointer casts like `&v` as `*const _` or raw address of
    ///            expressions like `&raw v` or `addr_of!(v)`.
    fn visit_ref_or_address_of(&mut self, lh_path: Rc<Path>, place: &mir::Place<'tcx>) {
        // debug!("Ref/AddressOf Assignment");
        let rh_path = self.get_path_for_place(place);

        // If the lh_path is a deref path, we need to add a temporary local variable,
        // e.g. `(*_1).2 = &rh_path;` ==> `_TMP = &rh_path; (*_1).2 = _TMP`;
        let lh_path = if lh_path.is_deref_path() {
            let lh_type = self.acx.get_path_rustc_type(&lh_path).unwrap();
            let aux = self.create_aux_local(lh_type);
            self.add_store_edge(aux.clone(), lh_path);
            aux
        } else {
            lh_path
        };

        if self.is_promoted() {
            let rh_type = self.acx.get_path_rustc_type(&rh_path).unwrap();
            if type_util::is_argumentv1_array(rh_type) {
                let argv1_arr_path = Path::new_argumentv1_arr();
                if self.acx.get_path_rustc_type(&argv1_arr_path).is_none() {
                    self.acx.set_path_rustc_type(argv1_arr_path.clone(), rh_type);
                }
                self.add_addr_edge(argv1_arr_path, lh_path);
                return;
            }
            if type_util::is_str_ref_array(rh_type) {
                let str_ref_arr_path = Path::new_str_ref_arr();
                if self.acx.get_path_rustc_type(&str_ref_arr_path).is_none() {
                    self.acx.set_path_rustc_type(str_ref_arr_path.clone(), rh_type);
                }
                self.add_addr_edge(str_ref_arr_path, lh_path);
                return;
            }
        }

        match &rh_path.value {
            PathEnum::Parameter { .. } | PathEnum::LocalVariable { .. } | PathEnum::ReturnValue { .. } => {
                self.add_addr_edge(rh_path, lh_path);
            }
            PathEnum::QualifiedPath { base, projection } => {
                // 1. If the rh_path is a dereference of a pointer or reference, add a direct edge from
                //    the base_value of the rh_path to the lh_path,
                //    e.g. _1 = &(*_2); // It is equivelant to _1 = _2;
                // 2. If the first projection element is Deref, and the length of projection is larger
                //    than 1, add a gep edge from the rh_path to lh_path,
                //    e.g. _1 = &((*_2).1); // _1 points to the field of the _2's referent
                // 3. If the first projection element is not Deref, add an addr_edge from the rh_path to
                //    the lh_path, e.g. _1 = &(_1.2);
                match projection[0] {
                    PathSelector::Deref if projection.len() == 1 => {
                        let base = base.clone();
                        self.add_direct_edge(base, lh_path);
                    }
                    PathSelector::Deref => {
                        self.add_gep_edge(rh_path, lh_path);
                    }
                    _ => {
                        self.add_addr_edge(rh_path, lh_path);
                    }
                };
            }
            _ => {
                unreachable!("Unexpected path type of rh_path in Ref/AddressOf assignment.");
            }
        }
    }

    /// path = operand as ty.
    fn visit_cast(
        &mut self,
        lh_path: Rc<Path>,
        cast_kind: mir::CastKind,
        operand: &mir::Operand<'tcx>,
        ty: Ty<'tcx>,
    ) {
        let lh_type = self
            .acx
            .get_path_rustc_type(&lh_path)
            .expect("Unresolved lh type");
        let lh_path = if lh_path.is_deref_path() {
            // Create an auxiliary `aux`, add a cast edge from src to aux first, then store aux into dst.
            let aux = self.create_aux_local(lh_type);
            self.add_internal_edges(aux.clone(), lh_type, lh_path, lh_type);
            aux
        } else {
            lh_path
        };
        match cast_kind {
            // An exposing pointer to address cast. A cast between a pointer and an
            // integer type, or between a function pointer and an integer type.
            // See the docs on expose_addr for more details.
            mir::CastKind::PointerExposeAddress
            // An address-to-pointer cast that picks up an exposed provenance.
            // See the docs on from_exposed_addr for more details.
            | mir::CastKind::PointerFromExposedAddress => {}
            // Primitive casts
            mir::CastKind::IntToInt
            | mir::CastKind::FloatToInt
            | mir::CastKind::FloatToFloat
            | mir::CastKind::IntToFloat => {}
            // Cast into a dyn* object.
            mir::CastKind::DynStar
            // Go from a mut raw pointer to a const raw pointer.
            | mir::CastKind::PointerCoercion(PointerCoercion::MutToConstPointer)
            // Go from a safe fn pointer to an unsafe fn pointer.
            | mir::CastKind::PointerCoercion(PointerCoercion::UnsafeFnPointer) => {
                // These kinds of pointer casts do not re-interpret the bits of the input as a
                // different type. We simply treat them as direct assignments.
                let rh_path = match operand {
                    mir::Operand::Move(place) | mir::Operand::Copy(place) => {
                        self.get_path_for_place(place)
                    }
                    mir::Operand::Constant(box const_op) => {
                        debug!("
                            DynStar/MutToConstPointer/UnsafeFnPointer cast from a const operand!"
                        );
                        self.visit_const_operand(const_op)
                    }
                };
                self.add_direct_edge(rh_path, lh_path);
            }
            // Go from a fn-item type to a fn-pointer type.
            // For example: ``` p = foo as fn(i32) -> i32 (Pointer(ReifyFnPointer)); ```
            // The operand should be a constant of a function instance or a place of FnDef type
            mir::CastKind::PointerCoercion(PointerCoercion::ReifyFnPointer) => {
                let rh_path = match operand {
                    mir::Operand::Move(place) | mir::Operand::Copy(place) => {
                        let mut rh_path = self.get_path_for_place(place);
                        let rh_ty = self
                            .acx
                            .get_path_rustc_type(&rh_path)
                            .expect("Expect a FnDef type");
                        if let TyKind::FnDef(def_id, substs) = rh_ty.kind() {
                            rh_path = self.visit_function_reference(*def_id, substs);
                        } else {
                            unreachable!("Unexpected type of operand in ReifyFnPointer cast!");
                        }
                        rh_path
                    }
                    mir::Operand::Constant(box const_op) => {
                        // the rh_path must be a function item
                        let rh_path = self.visit_const_operand(const_op);
                        assert!(matches!(rh_path.value, PathEnum::Function(..)));
                        rh_path
                    }
                };
                self.add_fnptr_cast_edge(lh_path, rh_path, ty);
            }
            // Go from a non-capturing closure to an fn pointer or an unsafe fn pointer.
            // It cannot convert a closure that requires unsafe.
            // Closures capturing the environments cannot be converted to fn pointer as well.
            // The operand should be a place of a closure instance.
            mir::CastKind::PointerCoercion(PointerCoercion::ClosureFnPointer(..)) => {
                let rh_path = match operand {
                    mir::Operand::Move(place) | mir::Operand::Copy(place) => {
                        self.get_path_for_place(place)
                    }
                    mir::Operand::Constant(const_op) => {
                        // the rh_path must be a closure
                        self.visit_const_operand(const_op)
                    }
                };
                let ty = self
                    .acx
                    .get_path_rustc_type(&rh_path)
                    .expect("Expect a closure type");
                assert!(matches!(ty.kind(), TyKind::Closure(..)));

                self.add_fnptr_cast_edge(lh_path, rh_path, ty);
            }
            // Unsize a pointer/reference value, e.g., &[T; n] to &[T]. Note that the source could
            // be a thin or fat pointer. This will do things like convert thin pointers to fat
            // pointers, or convert structs containing thin pointers to structs containing fat
            // pointers, or convert between fat pointers. We don’t store the details of how the
            // transform is done (in fact, we don’t know that, because it might depend on the
            // precise type parameters). We just store the target type. Codegen backends and miri
            // figure out what has to be done based on the precise source/target type at hand.
            // Example of casting a struct containing thin pointers to a struct containing
            // fat pointers:
            // ```
            //  let a = Box::<[i32; 3]>::new([1, 2, 3]);
            //  let b: Box::<[i32]> = a;
            // ```
            mir::CastKind::PointerCoercion(PointerCoercion::Unsize) => {
                match operand {
                    mir::Operand::Move(place) | mir::Operand::Copy(place) => {
                        let (rh_path, rh_type) = self.get_path_and_type_for_place(place);
                        debug!("Unsize pointer cast: {:?} -> {:?}", rh_path, lh_path);
                        // We need to call transmute_pointers here to make the source pointer and
                        // destination pointer point to different types.
                        self.copy_and_transmute(rh_path, rh_type, lh_path, lh_type);
                    }
                    mir::Operand::Constant(const_op) => {
                        // The operand of a Unsize pointer cast statement can be a constant in rare cases.
                        let const_path = self.visit_const_operand(const_op);
                        if let Some(const_ty) = self.acx.get_path_rustc_type(&const_path) {
                            if ty.is_any_ptr() {
                                self.copy_and_transmute(const_path, const_ty, lh_path, lh_type);
                            }
                        }
                    }
                }
            }
            // Go from *const [T; N] to *const T
            // In practice, we find that most casts from *const [T; N] to *const T are classified
            // as CastKind::PtrToPtr
            mir::CastKind::PointerCoercion(PointerCoercion::ArrayToPointer)
            | mir::CastKind::PtrToPtr
            // Cast a function pointer to another pointer type
            // e.g. ``` let p = fp as *const (); ```
            | mir::CastKind::FnPtrToPtr => {
                if let mir::Operand::Copy(place) | mir::Operand::Move(place) = operand {
                    let (rh_path, rh_type) = self.get_path_and_type_for_place(place);
                    if lh_type.is_any_ptr() && rh_type.is_any_ptr() {
                        let src_path = if rh_path.is_deref_path() {
                            // Load the value of rh_path to an auxiliary variable, then add a cast
                            // edge from aux to dst.
                            let aux = self.create_aux_local(rh_type);
                            self.add_load_edge(rh_path, aux.clone());
                            aux
                        } else {
                            rh_path
                        };
                        // The lh path or the rh path might be a reference to a transparent wrapper struct.
                        // Therefore we cast the pointers by transmuting between them.
                        self.transmute_pointers(src_path, rh_type, lh_path, lh_type)
                    }
                }
            }
            mir::CastKind::Transmute => {
                debug!("Visiting transmute cast statement {:?} -> {:?}", operand, lh_path);
                if let mir::Operand::Copy(place) | mir::Operand::Move(place) = operand {
                    let (rh_path, rh_type) = self.get_path_and_type_for_place(place);
                    self.copy_and_transmute(rh_path, rh_type, lh_path, lh_type);
                }
            }
        }
    }

    /// Apply the given binary operator to the two operands and assign result to path.
    fn visit_binary_op(
        &mut self,
        lh_path: Rc<Path>,
        bin_op: mir::BinOp,
        left_operand: &mir::Operand<'tcx>,
        _right_operand: &mir::Operand<'tcx>,
    ) {
        match bin_op {
            mir::BinOp::Offset => {
                match left_operand {
                    mir::Operand::Move(place) | mir::Operand::Copy(place) => {
                        let rh_path = self.get_path_for_place(place);
                        self.add_offset_edge(rh_path, lh_path);
                    }
                    mir::Operand::Constant(_const_op) => {
                        error!("Unexpected left operand in an Offset BinaryOp.");
                    }
                };
            }
            _ => {}
        }
    }

    /// Creates an aggregate value, like a tuple or struct.
    fn visit_aggregate(
        &mut self,
        lh_path: Rc<Path>,
        aggregate_kind: &mir::AggregateKind<'tcx>,
        operands: &IndexVec<FieldIdx, mir::Operand<'tcx>>,
    ) {
        match aggregate_kind {
            mir::AggregateKind::Array(ty) => {
                let index_path = Path::new_index(lh_path.clone());
                let index_ty = self.substs_specializer.specialize_generic_argument_type(*ty);
                self.acx.set_path_rustc_type(index_path.clone(), index_ty);
                for (_i, operand) in operands.iter().enumerate() {
                    self.visit_use(index_path.clone(), operand);
                }
            }
            mir::AggregateKind::Tuple => {
                let lh_ty = self.acx.get_path_rustc_type(&lh_path).unwrap();
                let types = if let TyKind::Tuple(types) = lh_ty.kind() {
                    types.as_slice()
                } else {
                    &[]
                };
                for (i, operand) in operands.iter().enumerate() {
                    let index_path = Path::new_field(lh_path.clone(), i);
                    if let Some(ty) = types.get(i) {
                        self.acx.set_path_rustc_type(index_path.clone(), *ty);
                    };
                    self.visit_use(index_path, operand);
                }
            }
            mir::AggregateKind::Adt(def, variant_idx, args, _, case_index) => {
                // The second field is the variant index. It’s equal to 0 for struct and union expressions.
                // The last field is the active field number and is present only for union expressions
                // – e.g., for a union expression SomeUnion { c: .. }, the active field index would identity
                // the field c
                let mut path = lh_path;
                let adt_def = self.tcx().adt_def(def);
                let variant_def = &adt_def.variants()[*variant_idx];
                // let adt_ty = self.tcx().type_of(def).skip_binder();
                let args = self.substs_specializer.specialize_generic_args(args);
                if adt_def.is_union() {
                    let case_index = case_index.unwrap_or(0usize.into());
                    let field_path = Path::new_union_field(path, case_index.into());
                    let field = &variant_def.fields[case_index];
                    let field_ty = type_util::field_ty(self.tcx(), field, args);
                    self.acx.set_path_rustc_type(field_path.clone(), field_ty);
                    self.visit_use(field_path.clone(), &operands[0usize.into()]);

                    self.cast_between_union_fields(&field_path);
                    return;
                } else if adt_def.is_enum() {
                    path = Path::new_downcast(path, variant_idx.as_usize());
                }

                for (i, field) in variant_def.fields.iter().enumerate() {
                    let field_path = Path::new_field(path.clone(), i);
                    let field_ty = type_util::field_ty(self.tcx(), field, args);
                    self.acx.set_path_rustc_type(field_path.clone(), field_ty);
                    if let Some(operand) = operands.get(i.into()) {
                        self.visit_use(field_path, operand);
                    } else {
                        debug!("variant has more fields than was serialized {:?}", variant_def);
                    }
                }
            }
            mir::AggregateKind::Closure(_def_id, _args) | mir::AggregateKind::Coroutine(_def_id, _args) => {
                for (i, operand) in operands.iter().enumerate() {
                    let base_ty = self.acx.get_path_rustc_type(&lh_path).unwrap();
                    let field_path = Path::new_field(lh_path.clone(), i);
                    let field_ty = type_util::get_field_type(self.tcx(), base_ty, i);
                    self.acx.set_path_rustc_type(field_path.clone(), field_ty);
                    self.visit_use(field_path, operand);
                }
            }
        }
    }

    /// Transmutes a `*mut u8` into a shallow-initialized `Box<T>`.
    ///
    /// This is different from a normal transmute because dataflow analysis will treat the box
    /// as initialized but its content as uninitialized.
    fn visit_shallow_init_box(&mut self, lh_path: Rc<Path>, operand: &mir::Operand<'tcx>, ty: Ty<'tcx>) {
        // Box.0 = Unique, Unique.0 = NonNull, NonNull.0: *const T = source thin pointer
        let box_ptr_field = self.get_box_pointer_field(lh_path, ty);
        // Treat this statement as a cast statement that casts heap object from u8 type to T type.
        let box_ptr_type = self.acx.get_path_rustc_type(&box_ptr_field).unwrap();
        let source_path = match operand {
            mir::Operand::Move(place) | mir::Operand::Copy(place) => self.get_path_for_place(place),
            _ => {
                unreachable!(
                    "The operand of shallow_init_box statement is supposed to be a move|copy place."
                );
            }
        };
        let source_ptr_type = self.acx.get_path_rustc_type(&source_path).unwrap();
        self.transmute_pointers(source_path, source_ptr_type, box_ptr_field, box_ptr_type)
    }

    /// Try to resolve a function calls.
    fn resolve_call(
        &mut self,
        callee_def_id: &DefId,
        gen_args: &GenericArgsRef<'tcx>,
        args: &Vec<Spanned<mir::Operand<'tcx>>>,
        destination: &mir::Place<'tcx>,
        location: mir::Location,
    ) {
        // Specialize callee's substs from known generic types
        let gen_args = self.substs_specializer.specialize_generic_args(gen_args);
        let args = self.visit_args(args);
        let destination = self.get_path_for_place(destination);
        debug!("Call func {:?}, generic_args: {:?}", callee_def_id, gen_args);

        if special_function_handler::handled_as_special_function_call(
            self,
            callee_def_id,
            &gen_args,
            &args,
            &destination,
            location,
        ) {
            let callsite = self.new_callsite(self.func_id, location, args, destination);
            let (callee_def_id, gen_args) =
                match call_graph_builder::try_to_devirtualize(self.tcx(), *callee_def_id, gen_args) {
                    Some((callee_def_id, gen_args)) => (callee_def_id, gen_args),
                    None => (*callee_def_id, gen_args),
                };
            let callee_func_id = self.acx.get_func_id(callee_def_id, gen_args);
            self.fpag.add_special_callsite(callsite, callee_func_id);
            self.acx.add_special_function(callee_func_id);
            return;
        }

        if self.acx.is_std_ops_fntrait_call(*callee_def_id) {
            // Fn*::call*
            self.resolve_fntrait_call(callee_def_id, &gen_args, args, destination, location);
            return;
        }

        if !util::is_trait_method(self.tcx(), *callee_def_id) {
            // Static functions or methods or associated functions not declared on a trait.
            let callsite = self.new_callsite(self.func_id, location, args, destination);
            let callee_func_id = self.acx.get_func_id(*callee_def_id, gen_args);
            self.fpag.add_static_dispatch_callsite(callsite, callee_func_id);
        } else if let Some((callee_def_id, callee_substs)) =
            call_graph_builder::try_to_devirtualize(self.tcx(), *callee_def_id, gen_args)
        {
            // Methods or associated functions declared on a trait.
            // The called instance can be resolved at compile time.
            let callsite = self.new_callsite(self.func_id, location, args, destination);
            debug!(
                "Devirtualize to func {:?}, substs: {:?}",
                callee_def_id, callee_substs
            );
            let callee_func_id = self.acx.get_func_id(callee_def_id, callee_substs);
            self.fpag.add_static_dispatch_callsite(callsite, callee_func_id);
        } else if util::is_dynamic_call(self.tcx(), *callee_def_id, gen_args) {
            // trait method calls where the first argument is of dynamic type
            let receiver = args[0].clone();
            let callsite = self.new_callsite(self.func_id, location, args, destination);
            self.acx
                .add_dyn_callsite(callsite.clone().into(), *callee_def_id, gen_args);
            self.fpag.add_dynamic_dispatch_callsite(receiver, callsite);
        } else {
            warn!("Could not resolve function: {:?}, {:?}", callee_def_id, gen_args);
        }
    }

    fn resolve_fntrait_call(
        &mut self,
        callee_def_id: &DefId,
        gen_args: &GenericArgsRef<'tcx>,
        args: Vec<Rc<Path>>,
        destination: Rc<Path>,
        location: mir::Location,
    ) {
        // The fn_traits feature allows for implementation of the Fn* traits for
        // creating custom closure-like types. We first try to devirtualize the callee function
        // <https://doc.rust-lang.org/beta/unstable-book/library-features/fn-traits.html>
        let param_env = rustc_middle::ty::ParamEnv::reveal_all();
        // Instance::resolve panics if try_normalize_erasing_regions returns an error.
        // It is hard to figure out exactly when this will be the case.
        if self
            .tcx()
            .try_normalize_erasing_regions(param_env, *gen_args)
            .is_err()
        {
            warn!(
                "Could not resolve fntrait call: {:?}, {:?}",
                callee_def_id, gen_args
            );
            return;
        }
        let resolved_instance =
            rustc_middle::ty::Instance::resolve(self.tcx(), param_env, *callee_def_id, gen_args);
        if let Ok(Some(instance)) = resolved_instance {
            let resolved_def_id = instance.def.def_id();

            // Specially handlings for closures, function items and function pointers.
            // When the Fn* trait object is specialized to a closure, the resolved_def_id
            // corresponds to the def id of the closure. We still handle it along with function
            // items and function pointers.
            if self.tcx().is_closure_or_coroutine(resolved_def_id) {
                self.inline_indirectly_called_function(callee_def_id, gen_args, args, destination, location);
                return;
            }

            let has_mir = self.tcx().is_mir_available(resolved_def_id);
            if !has_mir {
                // ops::function::Fn*::call* for FnDef, FnPtr, Dynamic... types are unavailable
                // Try to inline the indirect call for these types
                if self.acx.def_in_ops_func_namespace(resolved_def_id) {
                    self.inline_indirectly_called_function(
                        callee_def_id,
                        gen_args,
                        args,
                        destination,
                        location,
                    );
                } else {
                    warn!("Unavailable mir for def_id: {:?}", resolved_def_id);
                }
                return;
            }

            // Programmers can implement the `Fn|FnOnce|FnMut` for a customized type.
            //
            // Rust compiler automatically implements the `Fn|FnOnce|FnMut` trait for a reference
            // type if its underlying type has implemented `Fn`, and implements `FnMut` and
            // `FnOnce` trait for a reference type if its underlying type has implemented `FnMut`
            // See https://doc.rust-lang.org/src/core/ops/function.rs.html#76.
            //
            // For example, if we implement the `Fn` trait for a struct type A, it automatically
            // implements the `Fn|FnOnce|FnMut` trait for `&A`, `&&A`, `&&&A`, ...
            //
            // When calling the function `<&&&A as Fn>::call()`, functions `<&&A as Fn>::call()`,
            // `<&A as Fn>::call()` and `<A as Fn>::call()` are called layer-by-layer.
            //
            // The mirs for the automatic implementations are also available and can be analyzed
            // directly.
            let instance_args = instance.args;
            debug!(
                "Devirtualize to func {:?}, substs: {:?}",
                resolved_def_id, instance_args
            );
            let callsite = self.new_callsite(self.func_id, location, args, destination);
            let callee_func_id = self.acx.get_func_id(resolved_def_id, instance_args);
            self.fpag.add_static_dispatch_callsite(callsite, callee_func_id);
        } else {
            warn!("Could not resolve function: {:?}, {:?}", callee_def_id, gen_args);
        }
    }

    /// `Fn::call`, `FnMut::call_mut`, `FnOnce::call_once` all receive two arguments:
    /// 1. An operand of any type that implements `Fn`|`FnMut`|`FnOnce`, including function items,
    ///    function pointers and closures.
    /// 2. A tuple of argument values for the call.
    /// The tuple is unpacked and the callee is then invoked with its normal function signature.
    /// In the case of calling a closure, the closure is included as the first argument.
    ///
    /// All of this happens in code that is not encoded as MIR, so we need built in support for it.
    pub fn inline_indirectly_called_function(
        &mut self,
        callee_def_id: &DefId,
        gen_args: &GenericArgsRef<'tcx>,
        args: Vec<Rc<Path>>,
        destination: Rc<Path>,
        location: mir::Location,
    ) {
        assert_eq!(args.len(), 2);
        // Parse the actual arguments from the second argument.
        let args_tuple_path = args[1].clone();
        // Unpack the type of the second argument, which should be a tuple.
        // The argument can be a constant tuple `const ()`, in which case we may fail to get its type
        let mut actual_arg_types: Vec<Ty<'tcx>> = if args_tuple_path.is_constant() {
            vec![]
        } else {
            if let TyKind::Tuple(tuple_types) = self.acx.get_path_rustc_type(&args_tuple_path).unwrap().kind()
            {
                tuple_types.iter().collect()
            } else {
                unreachable!("The second argument is expected to be a tuple");
            }
        };

        // Unpack the second argument, which should be a tuple
        let mut actual_args: Vec<Rc<Path>> = actual_arg_types
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                let proj_elems = vec![PathSelector::Field(i)];
                let arg = Path::new_qualified(args_tuple_path.clone(), proj_elems);
                self.acx.set_path_rustc_type(arg.clone(), *ty);
                arg
            })
            .collect();

        // If the first substution is a closure or FnDef, we can inline the closure call directly.
        // The substs should have been specialized when added to the type cache.
        let first_subst_ty = gen_args
            .types()
            .next()
            .expect("Expect type substition in Fn* invocation");
        match first_subst_ty.kind() {
            TyKind::FnDef(def_id, substs) => {
                // Fn*::call* itself cannot be the first argument as it is a trait method without
                // a implementation, therefore we do not need to worry about the recursive std_ops_func_call.
                let (def_id, substs) = call_graph_builder::resolve_fn_def(self.tcx(), *def_id, substs);
                let callee_func_id = self.acx.get_func_id(def_id, substs);
                // Set up a callsite
                let callsite = self.new_callsite(self.func_id, location, actual_args, destination);
                self.fpag.add_static_dispatch_callsite(callsite, callee_func_id);
            }
            TyKind::Closure(def_id, substs) | TyKind::Coroutine(def_id, substs) => {
                // Prepend the callee closure/generator/function to the unpacked arguments vector
                // if the called function actually expects it.
                actual_args.insert(0, args[0].clone());
                actual_arg_types.insert(0, first_subst_ty);

                // call_once consumes its callee argument. If the callee does not,
                // we have to provide it with a reference.
                // Sadly, the easiest way to get hold of the type of the first parameter
                // of the callee is to look at its MIR body. If there is no body, we wont
                // be executing it and the type of the first argument is immaterial, so this
                // does not cause problems.
                let mir = self.tcx().optimized_mir(def_id);
                let first_arg_type = self.acx.get_path_rustc_type(&args[0]).unwrap();
                if let Some(decl) = mir.local_decls.get(mir::Local::from(1usize)) {
                    if decl.ty.is_ref() && !first_arg_type.is_ref() {
                        let closure_path = args[0].clone();
                        // create a reference path to to this closure
                        let closure_ref_ty =
                            Ty::new_mut_ref(self.tcx(), self.tcx().lifetimes.re_static, first_subst_ty);
                        let closure_ref_path = self.create_aux_local(closure_ref_ty);
                        self.add_addr_edge(closure_path, closure_ref_path.clone());
                        actual_args[0] = closure_ref_path;
                        // decl.ty is not type specialized
                        actual_arg_types[0] = closure_ref_ty;
                    }
                }

                // Set up a callsite
                let callsite = self.new_callsite(self.func_id, location, actual_args, destination);
                let callee_func_id = self.acx.get_func_id(*def_id, substs);
                self.fpag.add_static_dispatch_callsite(callsite, callee_func_id);
            }
            TyKind::FnPtr(..) => {
                // Add the first argument and the callsite to fpag's fnptr_callsite
                let callsite = self.new_callsite(self.func_id, location, actual_args, destination);
                // If the first argument is a reference to a function pointer
                let first_arg_type = self.acx.get_path_rustc_type(&args[0]).unwrap();
                let fn_ptr = if !first_arg_type.is_fn_ptr() && first_arg_type.is_any_ptr() {
                    let aux = self.create_aux_local(type_util::get_dereferenced_type(first_arg_type));
                    let deref_path = self.create_dereference(args[0].clone(), first_arg_type);
                    self.add_load_edge(deref_path, aux.clone());
                    aux
                } else {
                    args[0].clone()
                };
                self.fpag.add_fnptr_callsite(fn_ptr, callsite);
            }
            // For dynamic substution, resolve on the fly
            // e.g. &dyn FnMut(..)
            TyKind::Dynamic(..) => {
                // Add the first argument and the callsite to fpag's std_ops_callsites
                // Use the original args instead of the actual args
                let dyn_fn_obj = args[0].clone();
                let dyn_callsite = self.new_callsite(self.func_id, location, args, destination);
                self.acx
                    .add_dyn_callsite(dyn_callsite.clone().into(), *callee_def_id, gen_args);
                // This call maybe a dyn FnOnce call, in which case the dyn_fn_obj would be
                // of dyn FnOnce type instead a reference type (occurs for a function call
                // via a Box<dyn FnOnce> object). In this case, the first argument would be a
                // dereference value, e.g. (*_1). We need to cache the dynamic callsite with
                // the reference value, e.g. _1, to make our solver be able to determine the
                // call target based on the pointed-to objects of the reference value.
                let first_arg_type = self.acx.get_path_rustc_type(&dyn_fn_obj).unwrap();
                if !first_arg_type.is_any_ptr() {
                    if let PathEnum::QualifiedPath { base, projection } = &dyn_fn_obj.value {
                        if projection[0] == PathSelector::Deref && projection.len() == 1 {
                            self.fpag.add_dynamic_fntrait_callsite(base.clone(), dyn_callsite);
                        }
                    }
                } else {
                    self.fpag.add_dynamic_fntrait_callsite(dyn_fn_obj, dyn_callsite);
                }
            }
            _ => {
                error!("Unexpected argument type in Dn* trait call!");
            }
        }
    }

    /// If the source path and the destination path are both of pointer types, add a direct edge between them.
    /// Otherwise, get their pointer type fields if exist and add internal edges between these fields.
    pub fn add_internal_edges(
        &mut self,
        src_path: Rc<Path>,
        src_type: Ty<'tcx>,
        dst_path: Rc<Path>,
        dst_type: Ty<'tcx>,
    ) {
        if type_util::equal_types(self.tcx(), src_type, dst_type) {
            if src_type.is_any_ptr() {
                self.add_edge_between_ptrs(src_path, dst_path);
            } else {
                let ptr_projs = unsafe {
                    &*(self.acx.get_pointer_projections(src_type) as *const Vec<(ProjectionElems, Ty<'tcx>)>)
                };
                for (ptr_proj, ptr_ty) in ptr_projs {
                    let src_field = Path::append_projection(&src_path, ptr_proj);
                    self.acx.set_path_rustc_type(src_field.clone(), *ptr_ty);
                    let dst_field = Path::append_projection(&dst_path, ptr_proj);
                    self.acx.set_path_rustc_type(dst_field.clone(), *ptr_ty);
                    self.add_edge_between_ptrs(src_field, dst_field);
                }
            }
        } else {
            warn!(
                "Unmatched types: {:?}({:?}) -> {:?}({:?})",
                src_path, src_type, dst_path, dst_type
            );
        }
    }

    fn add_edge_between_ptrs(&mut self, src: Rc<Path>, dst: Rc<Path>) {
        match (src.is_deref_path(), dst.is_deref_path()) {
            (false, false) => self.add_direct_edge(src, dst),
            (true, false) => self.add_load_edge(src, dst),
            (false, true) => self.add_store_edge(src, dst),
            (true, true) => unreachable!("Unexpected types of lh_path and rh_path."),
        }
    }

    /// Adds edges between the union fields that share the same memory offset
    fn cast_between_union_fields(&mut self, path: &Rc<Path>) {
        let retrieve_union_fields = |path: &Rc<Path>| -> Vec<(Rc<Path>, usize)> {
            let mut ret = Vec::new();
            match &path.value {
                PathEnum::QualifiedPath { projection, .. } => {
                    for (i, selector) in projection.iter().enumerate() {
                        if let PathSelector::UnionField(index) = *selector {
                            let union_base = Path::truncate_projection_elems(&path, i);
                            ret.push((union_base, index));
                        }
                    }
                }
                _ => {}
            }
            ret
        };

        let union_fields = retrieve_union_fields(path);
        if !union_fields.is_empty() {
            for (union_path, field_index) in union_fields {
                let union_type = self
                    .acx
                    .get_path_rustc_type(&union_path)
                    .expect("Uncached union path");
                if let TyKind::Adt(def, substs) = union_type.kind() {
                    let source_field = def.all_fields().nth(field_index).unwrap();
                    let source_type =
                        self.substs_specializer
                            .specialize_generic_argument_type(type_util::field_ty(
                                self.tcx(),
                                source_field,
                                substs,
                            ));
                    let source_path = Path::new_union_field(union_path.clone(), field_index);
                    self.acx.set_path_rustc_type(source_path.clone(), source_type);
                    for (i, field) in def.all_fields().enumerate() {
                        if i == field_index {
                            continue;
                        }
                        let target_type = self
                            .substs_specializer
                            .specialize_generic_argument_type(type_util::field_ty(self.tcx(), field, substs));
                        let target_path = Path::new_union_field(union_path.clone(), i);
                        self.acx.set_path_rustc_type(target_path.clone(), target_type);
                        self.copy_and_transmute(source_path.clone(), source_type, target_path, target_type);
                    }
                } else {
                    unreachable!("the base path of a union field is not a union");
                }
            }
        }
    }

    /// Adds internal edge for ReifyFnPointer or ClosureFnPointer casts, where the rh_path is a function item (
    /// parsed from FnDef or Closure) and the lh_path is a function pointer, to enable the function pointer
    /// pointing to the function item.
    /// Note that the lh_path can also be a dereferenced value, if so, we need to introduce an auxiliary local
    /// variable.
    /// For exmaple, given the ReifyFnPointer cast: `(*_2) = times2 as fn(i32) -> i32 (Pointer(ReifyFnPointer));`
    /// We create an auxiliary local variable `aux` to split this statement into two statements:
    /// `aux = times2 as fn(i32) -> i32 (Pointer(ReifyFnPointer));` and `(*2) = aux`.
    fn add_fnptr_cast_edge(&mut self, lh_path: Rc<Path>, rh_path: Rc<Path>, ty: Ty<'tcx>) {
        match &lh_path.value {
            PathEnum::QualifiedPath { base: _, projection } if projection[0] == PathSelector::Deref => {
                match ty.kind() {
                    TyKind::FnPtr(..) => {
                        let aux = self.create_aux_local(ty);
                        self.add_addr_edge(rh_path, aux.clone());
                        self.add_store_edge(aux, lh_path);
                    }
                    _ => {
                        unreachable!("Unexpected cast type in ReifyFnPointer cast!");
                    }
                }
            }
            _ => {
                self.add_addr_edge(rh_path, lh_path);
            }
        }
    }

    /// Creates an auxiliary local variable with the given type.
    #[inline]
    pub fn create_aux_local(&mut self, ty: Ty<'tcx>) -> Rc<Path> {
        self.acx.create_aux_local(self.func_id, ty)
    }

    /// Creates a dereference path for the given pointer or reference path.
    #[allow(unused)]
    fn create_dereference(&mut self, ptr_path: Rc<Path>, ptr_ty: Ty<'tcx>) -> Rc<Path> {
        let deref_path = if let PathEnum::QualifiedPath { .. } = ptr_path.value {
            let aux = self.create_aux_local(ptr_ty);
            self.add_direct_edge(ptr_path, aux.clone());
            Path::new_deref(aux)
        } else {
            Path::new_deref(ptr_path)
        };
        self.acx
            .set_path_rustc_type(deref_path.clone(), type_util::get_dereferenced_type(ptr_ty));
        deref_path
    }

    /// Returns the parameter environment for the current function.
    pub fn get_param_env(&self) -> rustc_middle::ty::ParamEnv<'tcx> {
        let def_id = self.def_id();
        let env_def_id = if self.tcx().is_closure_or_coroutine(def_id) {
            self.tcx().typeck_root_def_id(def_id)
        } else {
            def_id
        };
        self.tcx().param_env(env_def_id)
    }

    /// Copy the value at `source_path` to a value at `target_path`.
    /// If the type of `source_path` is different from that at `target_path`, the value is transmuted.
    pub fn copy_and_transmute(
        &mut self,
        source_path: Rc<Path>,
        source_rustc_type: Ty<'tcx>,
        target_path: Rc<Path>,
        target_rustc_type: Ty<'tcx>,
    ) {
        debug!(
            "Copy and transmute from {:?}({:?}) to {:?}({:?})",
            source_path, source_rustc_type, target_path, target_rustc_type
        );
        let param_env = self.get_param_env();
        let src_flattened_fields =
            type_util::flatten_fields(self.tcx(), param_env, source_path, source_rustc_type);
        debug!("flattened fields of source value: {:?}", src_flattened_fields);

        let tgt_flattened_fields =
            type_util::flatten_fields(self.tcx(), param_env, target_path, target_rustc_type);
        debug!("flattened fields of target value: {:?}", tgt_flattened_fields);

        self.copy_flattened_fields(src_flattened_fields, tgt_flattened_fields);
    }

    fn copy_flattened_fields(
        &mut self,
        src_flattened_fields: Vec<(usize, Rc<Path>, Ty<'tcx>)>,
        tgt_flattened_fields: Vec<(usize, Rc<Path>, Ty<'tcx>)>,
    ) {
        let src_len = src_flattened_fields.len();
        let tgt_len = tgt_flattened_fields.len();
        let mut src_field_index = 0;
        let mut tgt_field_index = 0;
        while tgt_field_index < tgt_len && src_field_index < src_len {
            // Both the src_type and tgt_type should have been specialized.
            let (tgt_offset, tgt_field, tgt_type) = &tgt_flattened_fields[tgt_field_index];
            let (src_offset, src_field, src_type) = &src_flattened_fields[src_field_index];
            if *tgt_offset < *src_offset {
                tgt_field_index += 1;
                continue;
            } else if *tgt_offset > *src_offset {
                src_field_index += 1;
                continue;
            }

            // if source type and target type are any kind of primitive pointer type (reference, raw pointer, fn pointer).
            if src_type.is_any_ptr() && tgt_type.is_any_ptr() {
                self.acx.set_path_rustc_type(src_field.clone(), *src_type);
                self.acx.set_path_rustc_type(tgt_field.clone(), *tgt_type);
                self.transmute_pointers(src_field.clone(), *src_type, tgt_field.clone(), *tgt_type);
            }
            tgt_field_index += 1;
            src_field_index += 1;
        }
    }

    // Transmute from one pointer to another pointer.
    // If the source and target pointers are of equivalent pointer types, add
    // a direct edge between them, otherwise add a cast edge between them.
    fn transmute_pointers(
        &mut self,
        source_path: Rc<Path>,
        source_ptr_type: Ty<'tcx>,
        target_path: Rc<Path>,
        target_ptr_type: Ty<'tcx>,
    ) {
        assert!(source_ptr_type.is_any_ptr() && target_ptr_type.is_any_ptr());
        debug!(
            "Transmuting from pointer {:?} to pointer {:?}",
            source_path, target_path
        );

        // A cast edge requires that the source path and the target path are not dereference paths.
        let source_path = if source_path.is_deref_path() {
            let aux = self.create_aux_local(source_ptr_type);
            self.add_load_edge(source_path, aux.clone());
            aux
        } else {
            source_path
        };
        let target_path = if target_path.is_deref_path() {
            let aux = self.create_aux_local(target_ptr_type);
            self.add_store_edge(aux.clone(), target_path);
            aux
        } else {
            target_path
        };

        if type_util::equivalent_ptr_types(self.tcx(), source_ptr_type, target_ptr_type) {
            self.add_direct_edge(source_path, target_path);
        } else {
            self.add_cast_edge(source_path.clone(), target_path.clone());
        }
    }

    // Returns a Function path for the given `def_id` and `gen_args`, no matter if the corresponding mir
    // is unavailable.
    // If the function refers to a specific implementation of a trait method, devirtualize it.
    fn visit_function_reference(&mut self, def_id: DefId, gen_args: GenericArgsRef<'tcx>) -> Rc<Path> {
        // Specialize substs from current generic types
        let substs = self.substs_specializer.specialize_generic_args(gen_args);
        let (def_id, substs) = call_graph_builder::resolve_fn_def(self.tcx(), def_id, substs);
        let func_id = self.acx.get_func_id(def_id, substs);
        let path = Path::new_function(func_id);
        self.acx
            .set_path_rustc_type(path.clone(), Ty::new_fn_def(self.tcx(), def_id, substs));
        return path;
    }

    /// Returns a Path representing the given closure instance
    fn new_closure_path(&mut self, closure_ty: Ty<'tcx>) -> Rc<Path> {
        let closure_ty = self
            .substs_specializer
            .specialize_generic_argument_type(closure_ty);
        if let TyKind::Closure(def_id, args) = closure_ty.kind() {
            let func_id = self.acx.get_func_id(*def_id, args);
            let path = Path::new_function(func_id);
            self.acx.set_path_rustc_type(path.clone(), closure_ty);
            path
        } else {
            unreachable!("Unexpected type for creating a new closure path.");
        }
    }

    /// Returns a (Path, Type) pair that corresponds to the given Place instance
    fn get_path_and_type_for_place(&mut self, place: &mir::Place<'tcx>) -> (Rc<Path>, Ty<'tcx>) {
        let path = self.get_path_for_place(place);
        let ty = self
            .acx
            .get_path_rustc_type(&path)
            .expect("Failed to get the rustc type");
        (path, ty)
    }

    /// Returns a `Path` instance that resembles the `Place` instance.
    fn get_path_for_place(&mut self, place: &mir::Place<'tcx>) -> Rc<Path> {
        if let Some(path) = self.path_cache.get(place) {
            return path.clone();
        }
        let base_path: Rc<Path> =
            Path::new_local_parameter_or_result(self.func_id, place.local.as_usize(), self.mir.arg_count);
        let local_ty = self
            .substs_specializer
            .specialize_generic_argument_type(self.mir.local_decls[place.local].ty);
        self.acx.set_path_rustc_type(base_path.clone(), local_ty);
        if place.projection.is_empty() {
            self.path_cache.insert(*place, base_path.clone());
            base_path
        } else {
            let (path, ty) = self.visit_projection(base_path, local_ty, place.projection);
            self.acx.set_path_rustc_type(path.clone(), ty);
            self.path_cache.insert(*place, path.clone());
            path
        }
    }

    /// Returns a path that is qualified by the selector corresponding to the projection.elem.
    /// If projection has a base, the give base_path is first qualified with the base.
    fn visit_projection(
        &mut self,
        base_path: Rc<Path>,
        base_ty: Ty<'tcx>,
        projection: &[mir::PlaceElem<'tcx>],
    ) -> (Rc<Path>, Ty<'tcx>) {
        let mut ty = base_ty;
        let mut base_path = base_path;
        let mut selectors = ProjectionElems::with_capacity(projection.len());
        for elem in projection.iter() {
            let selector = self.visit_projection_elem(ty, elem);
            match elem {
                // We don't need to specialize the type during iteration, as the type must be specific
                // enough when it has projections.
                mir::ProjectionElem::Deref => {
                    if ty.is_box() {
                        // Deref the pointer at field 0 of the NonNull pointer at field 0
                        // of the Unique pointer at field 0 of the box
                        // Create an auxiliary variable to represent this sub-field.
                        // `(*_1);` ==> `aux = _1.0.0.0; *aux`
                        let box_ptr_field = self.get_box_pointer_field(base_path, ty.boxed_ty());
                        let box_ptr_ty = self
                            .acx
                            .get_path_rustc_type(&box_ptr_field)
                            .expect("Box pointer type");
                        let aux = self.create_aux_local(box_ptr_ty);
                        self.add_direct_edge(box_ptr_field, aux.clone());
                        base_path = aux;
                    }
                    ty = type_util::get_dereferenced_type(ty);
                }
                mir::ProjectionElem::Field(_, field_ty) => {
                    // Cache the base path if it is union type
                    if ty.is_union() {
                        let union_path = if selectors.is_empty() {
                            base_path.clone()
                        } else {
                            Path::new_qualified(base_path.clone(), selectors.clone())
                        };
                        let union_ty = self.substs_specializer.specialize_generic_argument_type(ty);
                        self.acx.set_path_rustc_type(union_path, union_ty);
                    }
                    ty = self
                        .substs_specializer
                        .specialize_generic_argument_type(*field_ty);
                }
                mir::ProjectionElem::Index(..) | mir::ProjectionElem::ConstantIndex { .. } => {
                    ty = type_util::get_element_type(self.tcx(), ty);
                }
                mir::ProjectionElem::Downcast(_, variant_idx) => {
                    ty = type_util::get_downcast_type(self.tcx(), ty, *variant_idx);
                }
                mir::ProjectionElem::Subslice { .. } => {
                    continue;
                }
                mir::ProjectionElem::OpaqueCast(..) | mir::ProjectionElem::Subtype(..) => {
                    // Todo
                    continue;
                }
            }
            selectors.push(selector);
        }
        let result = if selectors.len() == 0 {
            base_path
        } else {
            Path::new_qualified(base_path, selectors)
        };
        (result, ty)
    }

    /// Returns a PathSelector instance that resembles the ProjectionElem instance.
    fn visit_projection_elem(
        &mut self,
        base_ty: Ty<'tcx>,
        projection_elem: &mir::ProjectionElem<mir::Local, Ty<'tcx>>,
    ) -> PathSelector {
        match projection_elem {
            mir::ProjectionElem::Deref => PathSelector::Deref,
            mir::ProjectionElem::Field(field, _ty) => {
                if let TyKind::Adt(def, _) = base_ty.kind() {
                    if def.is_union() {
                        return PathSelector::UnionField(field.index());
                    }
                }
                PathSelector::Field(field.index())
            }
            mir::ProjectionElem::Index(_) | mir::ProjectionElem::ConstantIndex { .. } => PathSelector::Index,
            mir::ProjectionElem::Downcast(_name, index) => PathSelector::Downcast(index.as_usize()),
            mir::ProjectionElem::Subslice { from, to, from_end } => PathSelector::Subslice {
                from: *from,
                to: *to,
                from_end: *from_end,
            },
            mir::ProjectionElem::OpaqueCast(ty) | mir::ProjectionElem::Subtype(ty) => {
                PathSelector::Cast(self.acx.get_type_index(ty))
            }
        }
    }

    /// Returns the raw pointer field of a `Box` value.
    fn get_box_pointer_field(&mut self, box_path: Rc<Path>, ty: Ty<'tcx>) -> Rc<Path> {
        // Box.0 = Unique, Unique.0 = NonNull, NonNull.0 = source thin pointer
        let projection = vec![
            PathSelector::Field(0),
            PathSelector::Field(0),
            PathSelector::Field(0),
        ];
        let value_path = Path::append_projection(&box_path, &projection);
        if self.acx.get_path_rustc_type(&value_path).is_none() {
            let deref_ty = self.substs_specializer.specialize_generic_argument_type(ty);
            let ty = self
                .tcx()
                .mk_ty_from_kind(TyKind::RawPtr(rustc_middle::ty::TypeAndMut {
                    ty: deref_ty,
                    mutbl: rustc_middle::mir::Mutability::Not,
                }));
            self.acx.set_path_rustc_type(value_path.clone(), ty);
        }
        value_path
    }

    #[inline]
    pub fn add_addr_edge(&mut self, src: Rc<Path>, dst: Rc<Path>) {
        self.add_edge(src, dst, PAGEdgeEnum::AddrPAGEdge);
    }

    #[inline]
    pub fn add_direct_edge(&mut self, src: Rc<Path>, dst: Rc<Path>) {
        self.add_edge(src, dst, PAGEdgeEnum::DirectPAGEdge);
    }

    /// Adds a store edge from `src` to `dst`.
    /// Given a store statement ```(*p).f1.f2...fn = q```, a store edge of format `q --STORE(f1.f2...fn)--> p` is added.
    #[inline]
    pub fn add_store_edge(&mut self, src: Rc<Path>, dst: Rc<Path>) {
        if let PathEnum::QualifiedPath { base, projection } = &dst.value {
            assert_eq!(projection[0], PathSelector::Deref);
            let store_proj = Vec::from_iter(projection[1..].iter().cloned());
            self.add_edge(src, base.clone(), PAGEdgeEnum::StorePAGEdge(store_proj));
        } else {
            unreachable!();
        }
    }

    /// Adds a load edge from `src` to `dst`.
    /// Given a load statement ```p = (*q).f1.f2...fn```, a Load edge `q --LOAD(f1.f2...fn)--> p` is added.
    #[inline]
    pub fn add_load_edge(&mut self, src: Rc<Path>, dst: Rc<Path>) {
        if let PathEnum::QualifiedPath { base, projection } = &src.value {
            assert_eq!(projection[0], PathSelector::Deref);
            let load_proj = Vec::from_iter(projection[1..].iter().cloned());
            self.add_edge(base.clone(), dst, PAGEdgeEnum::LoadPAGEdge(load_proj));
        } else {
            unreachable!();
        }
    }

    /// Adds a gep edge from `src` to `dst`.
    /// Given a gep statement ```p = &((*q).f1.f2...fn)```, a gep edge `q --GEP(f1.f2...fn)--> p` is added.
    #[inline]
    pub fn add_gep_edge(&mut self, src: Rc<Path>, dst: Rc<Path>) {
        if let PathEnum::QualifiedPath { base, projection } = &src.value {
            assert_eq!(projection[0], PathSelector::Deref);
            assert!(projection.len() > 1);
            let gep_proj = Vec::from_iter(projection[1..].iter().cloned());
            self.add_edge(base.clone(), dst, PAGEdgeEnum::GepPAGEdge(gep_proj));
        } else {
            unreachable!();
        }
    }

    #[inline]
    pub fn add_cast_edge(&mut self, src: Rc<Path>, dst: Rc<Path>) {
        self.add_edge(src, dst, PAGEdgeEnum::CastPAGEdge);
    }

    #[inline]
    pub fn add_offset_edge(&mut self, src: Rc<Path>, dst: Rc<Path>) {
        self.add_edge(src, dst, PAGEdgeEnum::OffsetPAGEdge);
    }

    /// Adds an internal edge from `src` to `dst` of `kind` to the function pag.
    pub fn add_edge(&mut self, src: Rc<Path>, dst: Rc<Path>, kind: PAGEdgeEnum) {
        self.fpag.add_internal_edge(src, dst, kind);
    }

    /// Creates a new callsite.
    fn new_callsite(
        &mut self,
        func_id: FuncId,
        location: rustc_middle::mir::Location,
        args: Vec<Rc<Path>>,
        destination: Rc<Path>,
    ) -> Rc<CallSite> {
        Rc::new(CallSite::new(func_id, location, args, destination))
    }
}
