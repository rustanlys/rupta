use rustc_hir::def_id::DefId;
use rustc_hir::lang_items::LangItem;
use rustc_middle::mir;
use rustc_middle::mir::interpret::{GlobalAlloc, Scalar};
use rustc_middle::ty::{Ty, TyCtxt, TyKind, GenericArgsRef};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_span::source_map::Spanned;

use log::*;
use std::borrow::Borrow;
use std::collections::HashSet;

use crate::builder::{call_graph_builder, special_function_handler};
use crate::builder::substs_specializer::SubstsSpecializer;
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::call_site::BaseCallSite;
use crate::mir::function::FuncId;
use crate::mir::known_names::KnownNames;
use crate::util::{self, type_util};

use super::rta::RapidTypeAnalysis;

pub struct BodyVisitor<'a, 'rta, 'tcx, 'compilation> {
    pub(crate) rta: &'rta mut RapidTypeAnalysis<'a, 'tcx, 'compilation>,
    pub(crate) func_id: FuncId,
    pub mir: &'tcx mir::Body<'tcx>,

    /// For specializing the generic type in the method.
    substs_specializer: SubstsSpecializer<'tcx>,
    encountered_statics: HashSet<DefId>,
}

impl<'a, 'rta, 'tcx, 'compilation> BodyVisitor<'a, 'rta, 'tcx, 'compilation> {
    pub fn new(
        rta: &'rta mut RapidTypeAnalysis<'a, 'tcx, 'compilation>, 
        func_id: FuncId,
        mir: &'tcx mir::Body<'tcx>,
    ) -> BodyVisitor<'a, 'rta, 'tcx, 'compilation> {
        let func_ref = rta.acx.get_function_reference(func_id);
        debug!("Processing function {:?} {}", func_id, func_ref.to_string());
        let substs_specializer = SubstsSpecializer::new(
            rta.acx.tcx, 
            func_ref.generic_args.clone()
        );

        BodyVisitor {
            rta,
            func_id,
            mir,
            substs_specializer,
            encountered_statics: HashSet::new(),
        }
    }

    #[inline]
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.rta.acx.tcx
    }

    #[inline]
    fn acx(&mut self) -> &mut AnalysisContext<'tcx, 'compilation> {
        self.rta.acx
    }

    pub fn visit_body(&mut self) {
        for bb in self.mir.basic_blocks.indices() {
            self.visit_baisc_block(bb);
        }
    }

    fn visit_baisc_block(&mut self, bb: mir::BasicBlock,) {
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

        self.rta.num_stmts += statements.len() + 1;
    }

    /// Calls a specialized visitor for each kind of statement.
    fn visit_statement(&mut self, _location: mir::Location, statement: &mir::Statement<'tcx>) {
        // debug!("Visiting statement: {:?}", statement);
        let mir::Statement {kind, source_info: _} = statement;
        match kind {
            mir::StatementKind::Assign(box (place, rvalue)) => {
                self.visit_assign(place, rvalue)
            }
            _ => (),
        }   
    }

    fn visit_assign(&mut self, _place: &mir::Place<'tcx>, rvalue: &mir::Rvalue<'tcx>) {
        match rvalue {
            mir::Rvalue::Cast(cast_kind, operand, ty) => {
                let specialized_ty = self.substs_specializer.specialize_generic_argument_type(
                    *ty
                );
                let source_ty = self.get_rustc_type_for_operand(operand);
                match specialized_ty.kind() {
                    TyKind::RawPtr(rustc_middle::ty::TypeAndMut {ty, ..}) 
                    | TyKind::Ref(_, ty, _) => {
                        if matches!(ty.kind(), TyKind::Dynamic(..)) {
                            let src_deref_type = type_util::get_dereferenced_type(source_ty);
                            if matches!(src_deref_type.kind(), TyKind::Dynamic(..)) {
                                self.rta.add_trait_upcasting_relation(src_deref_type, *ty);
                            } else {
                                debug!("Casting type {:?} to {:?}", src_deref_type, ty);
                                self.rta.add_possible_concrete_type(*ty, src_deref_type);
                            }
                        }
                    }
                    TyKind::FnPtr(..) => {
                        match source_ty.kind() {
                            TyKind::FnDef(..)
                            | TyKind::Closure(..)
                            | TyKind::Coroutine(..) => {
                                debug!("Casting type {:?} to {:?}", source_ty, specialized_ty);
                                self.rta.add_possible_fnptr_target(specialized_ty, source_ty);
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        // An unsize pointer cast can also convert structs containing thin pointers to structs 
                        // containing fat pointers, e.g., Box<MyStruct> -> Box<dyn MyTrait>, and 
                        // NonNull<MyStruct> -> NonNull<dyn MyTrait>
                        if matches!(cast_kind, mir::CastKind::PointerCoercion(PointerCoercion::Unsize)) {
                            if let TyKind::Adt(_def, tgt_generic_args) = specialized_ty.kind() {
                                if let TyKind::Adt(_def, src_generic_args) = source_ty.kind() {
                                    for (tgt_generic_arg, src_generic_arg) in 
                                        tgt_generic_args.iter().zip(src_generic_args.iter()) 
                                    {
                                        if let Some(tgt_generic_ty) = tgt_generic_arg.as_type() {
                                            if matches!(tgt_generic_ty.kind(), TyKind::Dynamic(..)) {
                                                if let Some(src_generic_ty) = src_generic_arg.as_type() {
                                                    if matches!(src_generic_ty.kind(), TyKind::Dynamic(..)) {
                                                        info!("trait_upcasting coercion from {:?} to {:?}", src_generic_ty, tgt_generic_ty);
                                                        self.rta.add_trait_upcasting_relation(src_generic_ty, tgt_generic_ty);
                                                    } else {
                                                        debug!("Casting type {:?} to {:?}", src_generic_ty, tgt_generic_ty);
                                                        self.rta.add_possible_concrete_type(tgt_generic_ty, src_generic_ty);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                };
            }
            _ => {}
        }
    }

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
            } => {
            },
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
        _destination: &mir::Place<'tcx>,
        location: mir::Location
    ) {
        match func {
            mir::Operand::Constant(box constant) => {
                match constant.ty().kind() {
                    TyKind::Closure(callee_def_id, gen_args)
                    | TyKind::FnDef(callee_def_id, gen_args)
                    | TyKind::Coroutine(callee_def_id, gen_args) => {
                        self.resolve_call(callee_def_id, gen_args, location, args)
                    }
                    _ => {
                        error!("Unexpected call: {:?}", constant);
                    }
                }
            }
            mir::Operand::Copy(place)
            | mir::Operand::Move(place) => {
                let fn_item_ty = self.get_rustc_type_for_place(place);
                assert!(fn_item_ty.is_fn());
                match fn_item_ty.kind() {
                    TyKind::FnDef(callee_def_id, callee_substs) => {
                        self.resolve_call(callee_def_id, callee_substs, location, args)
                    }
                    TyKind::FnPtr(..) => {
                        // cannot handle function pointers
                        let callsite = BaseCallSite::new(self.func_id, location);
                        self.rta.add_fnptr_callsite(callsite, fn_item_ty);
                    }
                    _ => {
                        unreachable!();
                    }
                }
            }
        }
    }

    fn resolve_call(
        &mut self, 
        callee_def_id: &DefId, 
        gen_args: &GenericArgsRef<'tcx>,
        location: mir::Location,
        _args: &Vec<Spanned<mir::Operand<'tcx>>>,
    ) {
        // Specialize callee's substs from known generic types
        let gen_args = self.substs_specializer.specialize_generic_args(gen_args);
        debug!("Call func {:?}, generic_args: {:?}", callee_def_id, gen_args);

        if special_function_handler::is_specially_handled_function(self.acx(), *callee_def_id) {
            let callsite = BaseCallSite::new(self.func_id, location);
            
            // Special handlings for thread spawn functions
            if matches!(self.acx().get_known_name_for(*callee_def_id), KnownNames::StdThreadBuilderSpawnUnchecked) {
                let mut new_location = location;
                new_location.statement_index += 1;
                let fn_once_defid = self.tcx().require_lang_item(LangItem::FnOnce, None);
                self.inline_indirectly_called_function(&fn_once_defid, &gen_args, new_location);
            }

            let (callee_def_id, gen_args) = match call_graph_builder::try_to_devirtualize(
                self.tcx(), *callee_def_id, gen_args
            ) {
                Some((callee_def_id, gen_args)) => (callee_def_id, gen_args),
                None => (*callee_def_id, gen_args),
            };
            let callee_func_id = self.acx().get_func_id(callee_def_id, gen_args);
            self.rta.add_static_callsite(callsite);
            self.rta.add_call_edge(callsite, callee_func_id);
            self.rta.specially_handled_functions.insert(callee_func_id);

            return;
        }

        if self.acx().is_std_ops_fntrait_call(*callee_def_id) {
            // Fn*::call*
            self.resolve_fntrait_call(callee_def_id, &gen_args, location);
            return;
        }

        if !util::is_trait_method(self.tcx(), *callee_def_id) 
        {
            // Static functions or methods or associated functions not declared on a trait.
            let callee_func_id = self.acx().get_func_id(*callee_def_id, gen_args);
            let callsite = BaseCallSite::new(self.func_id, location);
            self.rta.add_static_callsite(callsite);
            self.rta.add_call_edge(callsite, callee_func_id);
        } else if let Some((callee_def_id, callee_substs)) = 
            call_graph_builder::try_to_devirtualize(self.tcx(), *callee_def_id, gen_args) 
        {
            // Methods or associated functions declared on a trait.
            // The called instance can be resolved at compile time.
            let callee_func_id = self.acx().get_func_id(callee_def_id, callee_substs);
            let callsite = BaseCallSite::new(self.func_id, location);
            self.rta.add_static_callsite(callsite);
            self.rta.add_call_edge(callsite, callee_func_id);
        } else if util::is_dynamic_call(self.tcx(), *callee_def_id, gen_args) {
            // trait method calls where the first argument is of dynamic type
            let dyn_callsite = BaseCallSite::new(self.func_id, location);
            self.rta.add_dyn_callsite(dyn_callsite, *callee_def_id, gen_args);
        } else {
            warn!("Could not resolve function: {:?}, {:?}", callee_def_id, gen_args);
        }
    }

    fn resolve_fntrait_call(
        &mut self, 
        callee_def_id: &DefId, 
        gen_args: &GenericArgsRef<'tcx>,
        location: mir::Location,
    ) {
        // The fn_traits feature allows for implementation of the Fn* traits for 
        // creating custom closure-like types. We first try to devirtualize the callee function
        // https://doc.rust-lang.org/beta/unstable-book/library-features/fn-traits.html
        let param_env = rustc_middle::ty::ParamEnv::reveal_all();
        // Instance::resolve panics if try_normalize_erasing_regions returns an error.
        // It is hard to figure out exactly when this will be the case.
        if self.tcx().try_normalize_erasing_regions(param_env, *gen_args).is_err() {
            warn!("Could not resolve fntrait call: {:?}, {:?}", callee_def_id, gen_args);
            return;
        }
        let resolved_instance = rustc_middle::ty::Instance::resolve(
            self.tcx(),
            param_env,
            *callee_def_id,
            gen_args,
        );
        if let Ok(Some(instance)) = resolved_instance {
            let resolved_def_id = instance.def.def_id();

            // If it is a call to a closure, inline the closure call.
            if self.tcx().is_closure_or_coroutine(resolved_def_id) {
                self.inline_indirectly_called_function(
                    callee_def_id,
                    gen_args,
                    location,
                );
                return;
            }

            // Visit the implementation if its mir is available
            let has_mir = self.tcx().is_mir_available(resolved_def_id);
            if !has_mir {
                // ops::function::Fn*::call* for FnDef, FnPtr, Dynamic... types are unavailable
                // Try to inline the indirect call for these types
                if self.acx().def_in_ops_func_namespace(resolved_def_id) {
                    self.inline_indirectly_called_function(
                        callee_def_id,
                        gen_args,
                        location,
                    );
                } else {
                    warn!("Unavailable mir for def_id: {:?}", resolved_def_id);
                }
                return;
            }
            let instance_args = instance.args;
            let callsite = BaseCallSite::new(self.func_id, location);
            let callee_func_id = self.acx().get_func_id(resolved_def_id, instance_args);
            self.rta.add_static_callsite(callsite);
            self.rta.add_call_edge(callsite, callee_func_id);
        } else {
            warn!("Could not resolve function: {:?}, {:?}", callee_def_id, gen_args);
        }
    }

    /// Fn::call, FnMut::call_mut, FnOnce::call_once all receive two arguments:
    /// 1. Operand of any type that implements Fn|FnMut|FnOnce, a function pointer or closure instance for most cases.
    /// 2. A tuple of argument values for the call.
    /// The tuple is unpacked and the callee is then invoked with its normal function signature.
    /// In the case of calling a closure, the closure signature includes the closure as the first argument.
    ///
    /// All of this happens in code that is not encoded as MIR, so we need built in support for it.
    fn inline_indirectly_called_function(
        &mut self, 
        callee_def_id: &DefId, 
        gen_args: &GenericArgsRef<'tcx>,
        location: mir::Location,
    ) {
        // If the first substution is a closure or FnDef, we can inline the closure call directly.
        // The substs should have been specialized when added to the type cache.
        let first_subst_ty = gen_args.types().next().expect("Expect type substition in Fn* invocation");
        match first_subst_ty.kind() {
            TyKind::FnDef(def_id, substs) => {
                // Fn*::call* itself cannot be the first argument as it is a trait method without 
                // a implementation, therefore we do not need to worry about the recursive std_ops_func_call.
                let (def_id, substs) = call_graph_builder::resolve_fn_def(self.tcx(), *def_id, substs);
                let callee_func_id = self.acx().get_func_id(def_id, substs);
                // Set up a callsite
                let callsite = BaseCallSite::new(self.func_id, location);
                self.rta.add_static_callsite(callsite);
                self.rta.add_call_edge(callsite, callee_func_id);
            }
            TyKind::Closure(def_id, substs)
            | TyKind::Coroutine(def_id, substs) => {        
                // Set up a callsite
                let callsite = BaseCallSite::new(self.func_id, location);
                let callee_func_id = self.acx().get_func_id(*def_id, substs);
                self.rta.add_static_callsite(callsite);
                self.rta.add_call_edge(callsite, callee_func_id);
            }
            TyKind::FnPtr(..) => {
                // Add the first argument and the callsite to mpag's fnptr_callsite
                let callsite = BaseCallSite::new(self.func_id, location);
                self.rta.add_fnptr_callsite(callsite, first_subst_ty);
            }
            // e.g. &dyn FnMut(..)
            TyKind::Dynamic(..) => {
                let callsite = BaseCallSite::new(self.func_id, location);
                self.rta.add_dyn_fntrait_callsite(callsite, *callee_def_id, gen_args);
            }
            _ => {
                error!("Unexpected first argument type in std::ops::call!");
            }
        }
    }

    fn get_rustc_type_for_operand(&mut self, operand: &mir::Operand<'tcx>) -> Ty<'tcx> {
        let ty = match operand {
            mir::Operand::Copy(place)
            | mir::Operand::Move(place) => {
                self.get_rustc_type_for_place(place)
            }
            mir::Operand::Constant(const_op) => {
                let mir::ConstOperand { const_, .. } = const_op.borrow();
                let ty = match const_ {
                    // This constant came from the type system
                    mir::Const::Ty(_c) => const_.ty(),
                    // An unevaluated mir constant which is not part of the type system.
                    mir::Const::Unevaluated(c, ty) => {
                        self.visit_unevaluated_const(c, *ty);
                        *ty
                    }
                    // This constant contains something the type system cannot handle (e.g. pointers).
                    mir::Const::Val(v, ty) => {
                        self.visit_const_value(*v);
                        *ty
                    }
                };
                self.substs_specializer.specialize_generic_argument_type(
                    ty
                )
            }
        };
        match ty.kind() {
            TyKind::FnDef(def_id, gen_args) => {
                // let gen_args = self.substs_specializer.specialize_generic_args(gen_args);
                let (def_id, gen_args) = call_graph_builder::resolve_fn_def(self.tcx(), *def_id, gen_args);
                Ty::new_fn_def(self.tcx(), def_id, gen_args)
            }
            _ => ty,
        }
    }

    fn visit_unevaluated_const(
        &mut self,
        unevaluated: &mir::UnevaluatedConst<'tcx>,
        ty: Ty<'tcx>,
    ) {
        debug!("Visiting unevaluated constant: {unevaluated:?} {ty:?}");
        if let Some(_promoted) = unevaluated.promoted {
            return;
        }

        let mut def_id = unevaluated.def;
        let args = self.substs_specializer.specialize_generic_args(unevaluated.args);
        if !args.is_empty() {
            let param_env = rustc_middle::ty::ParamEnv::reveal_all();
            if let Ok(Some(instance)) =
                rustc_middle::ty::Instance::resolve(self.tcx(), param_env, def_id, args)
            {
                def_id = instance.def.def_id();
            }
            if self.tcx().is_mir_available(def_id) {
                self.encountered_statics.insert(def_id);
            }
        }
    }

    fn visit_const_value(&mut self, val: mir::ConstValue<'tcx>) {
        match val {
            mir::ConstValue::Scalar(Scalar::Ptr(ptr, _size)) => {
                match self.tcx().try_get_global_alloc(ptr.provenance.alloc_id()) {
                    Some(GlobalAlloc::Static(def_id)) => {
                        self.encountered_statics.insert(def_id);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn get_rustc_type_for_place(&self, place: &mir::Place<'tcx>) -> Ty<'tcx> {
        let local_ty = self.substs_specializer.specialize_generic_argument_type(
            self.mir.local_decls[place.local].ty
        );

        if place.projection.is_empty() {
            local_ty
        } else {
            self.visit_projection_type(local_ty, place.projection)
        }
    }

    fn visit_projection_type(
        &self,
        base_ty: Ty<'tcx>,
        projection: &[mir::PlaceElem<'tcx>],
    ) -> Ty<'tcx> {
        let mut ty = base_ty;
        for elem in projection.iter() {
            match elem {
                // We don't need to specialize the type during iteration, as the type must be specific 
                // enough when it has projections.
                mir::ProjectionElem::Deref => {
                    ty = type_util::get_dereferenced_type(ty);
                }
                mir::ProjectionElem::Field(_, field_ty) => {
                    // ty = *field_ty;
                    ty = self.substs_specializer.specialize_generic_argument_type(*field_ty);
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
                mir::ProjectionElem::OpaqueCast(_new_ty) 
                | mir::ProjectionElem::Subtype(_new_ty) => {
                    // Todo
                    continue;
                }
            }
        }
        ty
    }

}