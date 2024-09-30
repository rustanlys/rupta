use std::collections::{HashSet, HashMap};
use std::time::{Instant, Duration};
use log::*;

use rustc_hir::def_id::DefId;
use rustc_middle::ty::{PolyFnSig, GenericArgsRef, Ty, TyCtxt};

use crate::builder::call_graph_builder;
use crate::graph::call_graph::CallGraph;
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::call_site::{BaseCallSite, CallType};
use crate::mir::function::{FuncId, FunctionReference, GenericArgE};
use crate::util::{type_util, chunked_queue, results_dumper};

use super::body_visitor::BodyVisitor;


pub struct RapidTypeAnalysis<'a, 'tcx, 'compilation> {
    /// The analysis context
    pub(crate) acx: &'a mut AnalysisContext<'tcx, 'compilation>,
    /// Call graph
    pub call_graph: CallGraph<FuncId, BaseCallSite>,

    /// Iterator for reachable functions
    rf_iter: chunked_queue::IterCopied<FuncId>,

    /// Records the functions that have been visited
    pub(crate) visited_functions: HashSet<FuncId>,
    pub(crate) specially_handled_functions: HashSet<FuncId>,

    pub static_callsites: HashSet<BaseCallSite>,
    pub dyn_callsites: HashMap<Ty<'tcx>, HashSet<(BaseCallSite, DefId, GenericArgsRef<'tcx>)>>,
    pub dyn_fntrait_callsites: HashMap<Ty<'tcx>, HashSet<(BaseCallSite, DefId, GenericArgsRef<'tcx>)>>,
    pub fnptr_callsites: HashMap<Ty<'tcx>, HashSet<BaseCallSite>>,

    pub dynamic_to_possible_concrete_types: HashMap<Ty<'tcx>, HashSet<Ty<'tcx>>>,
    pub fnptr_sig_to_possible_targets: HashMap<PolyFnSig<'tcx>, HashSet<Ty<'tcx>>>,
    pub trait_upcasting_relations: HashMap<Ty<'tcx>, HashSet<Ty<'tcx>>>,

    pub num_stmts: usize,

    pub analysis_time: Duration,
}

impl<'a, 'tcx, 'compilation> RapidTypeAnalysis<'a, 'tcx, 'compilation> {
    pub fn new(acx: &'a mut AnalysisContext<'tcx, 'compilation>) -> Self {
        let call_graph = CallGraph::new();
        let rf_iter = call_graph.reach_funcs_iter();
        RapidTypeAnalysis {
            acx,
            call_graph,
            rf_iter,
            visited_functions: HashSet::new(),
            specially_handled_functions: HashSet::new(),
            static_callsites: HashSet::new(),
            dyn_callsites: HashMap::new(),
            dyn_fntrait_callsites: HashMap::new(),
            fnptr_callsites: HashMap::new(),
            dynamic_to_possible_concrete_types: HashMap::new(),
            fnptr_sig_to_possible_targets: HashMap::new(),
            trait_upcasting_relations: HashMap::new(),
            num_stmts: 0,
            analysis_time: Duration::ZERO,
        }
    }

    #[inline]
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.acx.tcx
    }

    pub fn analyze(&mut self) {
        let now = Instant::now();

        // add the entry point to the call graph
        let entry_point = self.acx.entry_point;
        let entry_func_id = self.acx.get_or_add_function_reference(
            FunctionReference::new_function_reference(entry_point, vec![])
        );
        self.call_graph.add_node(entry_func_id);

        // process terminators of reachable functions
        self.iteratively_process_reachable_functions();

        self.analysis_time = now.elapsed();
        println!("Rapid Type Analysis completed.");
        println!(
            "Rapid Type Analysis time: {}", 
            humantime::format_duration(self.analysis_time).to_string()
        );
    }

    fn iteratively_process_reachable_functions(&mut self) {
        while self.process_reachable_functions() {
            self.solve_trait_upcasting();
            self.solve_dyn_callsites();
            self.solve_dyn_fntrait_callsites();
            self.solve_fnptr_callsites();
        }
    }

    /// Process statements of reachable functions.
    fn process_reachable_functions(&mut self) -> bool {
        let mut has_new_functions = false;
        while let Some(func_id) = self.rf_iter.next() {
            has_new_functions = true;
            if !self.visited_functions.contains(&func_id) {
                let func_ref = self.acx.get_function_reference(func_id);
                let def_id = func_ref.def_id;
                let generic_args = &func_ref.generic_args;

                // We don't count specially handled functions as we do not process them in pta
                if self.specially_handled_functions.contains(&func_id) {
                    self.visited_functions.insert(func_id);
                    continue;
                }

                if !self.tcx().is_mir_available(def_id) {
                    warn!("Unavailable mir for def_id: {:?}", def_id);
                    self.visited_functions.insert(func_id);
                    continue;
                }

                self.promote_constants(def_id, generic_args);
                let mir = self.tcx().optimized_mir(def_id);
                let mut bv = BodyVisitor::new(self, func_id, mir);
                bv.visit_body();
                self.visited_functions.insert(func_id);
            }
        }  
        has_new_functions 
    }


    fn solve_trait_upcasting(&mut self) {
        // The algorithm for solving trait upcasting constraits is inefficient. 
        // However, considering that trait upcasting is rarely used in programs, it will not cause efficiency problems.
        let mut changed = true;
        while changed {
            changed = false;
            for (src_dyn_ty, tgt_dyn_ty_set) in &self.trait_upcasting_relations {
                if let Some(src_concrete_types) = self.dynamic_to_possible_concrete_types.get_mut(src_dyn_ty) {
                    let src_concrete_types = src_concrete_types.clone();
                    for tgt_dyn_ty in tgt_dyn_ty_set {
                        let tgt_concrete_types = self.dynamic_to_possible_concrete_types.entry(*tgt_dyn_ty).or_default();
                        for src_concrete_ty in &src_concrete_types {
                            changed |= tgt_concrete_types.insert(*src_concrete_ty);
                        }
                    }

                }
            }
        }
    }

    fn solve_dyn_callsites(&mut self) {
        let dynamic_to_possible_concrete_types = unsafe {
            &*{&self.dynamic_to_possible_concrete_types as *const HashMap<Ty<'tcx>, HashSet<Ty<'tcx>>>}
        };
        let dyn_callsites = unsafe {
            &*{&self.dyn_callsites as *const HashMap<Ty<'tcx>, HashSet<(BaseCallSite, DefId, GenericArgsRef<'tcx>)>>}
        };
        for (dyn_ty, concrete_types) in dynamic_to_possible_concrete_types {
            if let Some(dyn_callsites_tuple) = dyn_callsites.get(dyn_ty) {
                for (callsite, callee_def_id, gen_args) in dyn_callsites_tuple {
                    for concrete_type in concrete_types {
                        let mut replaced_args = gen_args.to_vec();
                        replaced_args[0] = (*concrete_type).into();
                        let replaced_args = self.tcx().mk_args(&replaced_args);
                        // Devirtualize the callee function
                        if let Some((callee_def_id, gen_args)) = call_graph_builder::try_to_devirtualize(
                            self.tcx(),
                            *callee_def_id,
                            replaced_args,
                        ) {
                            let func_id = self.acx.get_func_id(callee_def_id, gen_args);
                            self.add_call_edge(*callsite, func_id);
                        } else {
                            warn!("Could not resolve function: {:?}, {:?}", callee_def_id, replaced_args);
                        }
                    }
                }
            } 
        }
    }

    fn solve_dyn_fntrait_callsites(&mut self) {
        let dynamic_to_possible_concrete_types = unsafe {
            &*{&self.dynamic_to_possible_concrete_types as *const HashMap<Ty<'tcx>, HashSet<Ty<'tcx>>>}
        };
        let dyn_fntrait_callsites = unsafe {
            &*{&self.dyn_fntrait_callsites as *const HashMap<Ty<'tcx>, HashSet<(BaseCallSite, DefId, GenericArgsRef<'tcx>)>>}
        };
        for (dyn_fntrait_ty, callsites_tuple) in dyn_fntrait_callsites {
            if let Some(concrete_types) = dynamic_to_possible_concrete_types.get(dyn_fntrait_ty) {
                for concrete_type in concrete_types {
                    match concrete_type.kind() {
                        rustc_middle::ty::TyKind::FnDef(def_id, substs)
                        | rustc_middle::ty::TyKind::Closure(def_id, substs)
                        | rustc_middle::ty::TyKind::Coroutine(def_id, substs) => {
                            for (callsite, _, _) in callsites_tuple {
                                // try to devirtualize the def_id first
                                let (def_id, substs) = call_graph_builder::resolve_fn_def(self.tcx(), *def_id, substs);
                                let func_id = self.acx.get_func_id(def_id, substs);
                                self.add_call_edge(*callsite, func_id);
                            }
                        }
                        rustc_middle::ty::TyKind::FnPtr(..) => {
                            for (callsite, _, _) in callsites_tuple {
                                self.add_fnptr_callsite(*callsite, *concrete_type)
                            }
                        }
                        _ => {
                            for (callsite, callee_def_id, gen_args) in callsites_tuple {
                                let mut replaced_args = gen_args.to_vec();
                                replaced_args[0] = (*concrete_type).into();
                                let replaced_args = self.tcx().mk_args(&replaced_args);

                                // Devirtualize the callee function
                                let resolved_instance = rustc_middle::ty::Instance::resolve(
                                    self.tcx(),
                                    rustc_middle::ty::ParamEnv::reveal_all(),
                                    *callee_def_id, 
                                    replaced_args,
                                );
                                if let Ok(Some(instance)) = resolved_instance {
                                    let resolved_def_id = instance.def.def_id();
                                    let instance_args = instance.args;
                                    if self.tcx().is_mir_available(resolved_def_id) {
                                        // The pointee type cannot be FnDef, FnPtr, Closure, therefore its mir is supposed to be available
                                        let func_id = self.acx.get_func_id(resolved_def_id, instance_args);
                                        self.add_call_edge(*callsite, func_id);
                                    } else {
                                        warn!("Unavailable mir for def_id: {:?}", resolved_def_id);
                                    }
                                } else {
                                    warn!("Could not resolve function: {:?}, {:?}", callee_def_id, replaced_args);
                                }   
                            }
                        }
                    }
                }
            } else {
                error!("Fail to find concrete types for dyn fn* type: {:?}", dyn_fntrait_ty);
            }
        }
    }

    fn solve_fnptr_callsites(&mut self) {
        let fnptr_sig_to_possible_targets = unsafe {
            &*{&self.fnptr_sig_to_possible_targets as *const HashMap<PolyFnSig<'tcx>, HashSet<Ty<'tcx>>>}
        };
        let fnptr_callsites = unsafe {
            &*{&self.fnptr_callsites as *const HashMap<Ty<'tcx>, HashSet<BaseCallSite>>}
        };
        for (fnptr_type, callsites) in fnptr_callsites {
            if let rustc_middle::ty::TyKind::FnPtr(fn_sig) = fnptr_type.kind() {
                for (fn_sig2, possible_targets) in fnptr_sig_to_possible_targets {
                    if type_util::matched_fn_sig(self.tcx(), fn_sig.clone(), *fn_sig2) {
                        for callsite in callsites {
                            for fn_item_ty in possible_targets {
                                match fn_item_ty.kind() {
                                    rustc_middle::ty::TyKind::FnDef(def_id, substs) 
                                    | rustc_middle::ty::TyKind::Closure(def_id, substs) 
                                    | rustc_middle::ty::TyKind::Coroutine(def_id, substs) => {
                                        let func_id = self.acx.get_func_id(*def_id, substs);
                                        self.add_call_edge(*callsite, func_id);
                                    }
                                    _ => {
                                        unreachable!();
                                    }
                                }
                            }
                        }
                    }
                }
            } 
        }
    }


    pub fn promote_constants(&mut self, def_id: DefId, gen_args: &Vec<GenericArgE<'tcx>>) {
        for (ordinal, constant_mir) in self.tcx().promoted_mir(def_id).iter().enumerate() {
            let func_id = self.acx.get_promoted_id(def_id,  gen_args.clone(), ordinal.into());
            if !self.visited_functions.contains(&func_id) {
                let mut bv = BodyVisitor::new(self, func_id, constant_mir);
                bv.visit_body();
                self.visited_functions.insert(func_id);
            }
        }
    }

    pub fn visit_static(&mut self, def_id: DefId) {
        if !self.tcx().is_mir_available(def_id) {
            return;
        }

        let func_id = self.acx.get_func_id(def_id, self.tcx().mk_args(&[]));
        if !self.visited_functions.contains(&func_id) {
            let def = rustc_middle::ty::InstanceDef::Item(def_id);
            let mir = self.tcx().instance_mir(def);
            let mut bv = BodyVisitor::new(self, func_id, mir);
            bv.visit_body();
            self.visited_functions.insert(func_id);
        }
    }

    pub fn add_static_callsite(&mut self, callsite: BaseCallSite) {
        self.static_callsites.insert(callsite);
        self.set_callsite_type(callsite, CallType::StaticDispatch);
    }

    pub fn add_dyn_callsite(&mut self, callsite: BaseCallSite, callee_def_id: DefId, callee_substs: GenericArgsRef<'tcx>) {
        let dyn_type = type_util::strip_auto_traits(
            self.tcx(), 
            self.tcx().erase_regions_ty(callee_substs[0].expect_ty())
        );
        debug!("Add dyn callsite: {:?}->{:?}", dyn_type, callsite);
        assert!(matches!(dyn_type.kind(), rustc_middle::ty::TyKind::Dynamic(..)));
        self.dyn_callsites.entry(dyn_type).or_default().insert((callsite, callee_def_id, callee_substs));
        self.set_callsite_type(callsite, CallType::DynamicDispatch);
    }

    pub fn add_dyn_fntrait_callsite(&mut self, callsite: BaseCallSite, callee_def_id: DefId, callee_substs: GenericArgsRef<'tcx>) {
        let dyn_fntrait_type = type_util::strip_auto_traits(
            self.tcx(), 
            self.tcx().erase_regions_ty(callee_substs[0].expect_ty())
        );
        debug!("Add dyn_fn_trait callsite: {:?}->{:?}", dyn_fntrait_type, callsite);
        self.dyn_fntrait_callsites.entry(dyn_fntrait_type).or_default().insert((callsite, callee_def_id, callee_substs));
        self.set_callsite_type(callsite, CallType::DynamicFnTrait);
    }

    pub fn add_fnptr_callsite(&mut self, callsite: BaseCallSite, fnptr_type: Ty<'tcx>) {
        let fnptr_type =  self.tcx().erase_regions_ty(fnptr_type);
        debug!("Add fnptr callsite: {:?} -> {:?}", fnptr_type, callsite);
        self.fnptr_callsites.entry(fnptr_type).or_default().insert(callsite);
        self.set_callsite_type(callsite, CallType::FnPtr);
    }


    pub fn add_possible_concrete_type(&mut self, dyn_ty: Ty<'tcx>, concrete_ty: Ty<'tcx>) {
        let dyn_ty = type_util::strip_auto_traits(
            self.tcx(), 
            self.tcx().erase_regions_ty(dyn_ty)
        );
        let concrete_ty = self.tcx().erase_regions_ty(concrete_ty);
        self.dynamic_to_possible_concrete_types.entry(dyn_ty).or_default().insert(concrete_ty);
    }

    pub fn add_possible_fnptr_target(&mut self, fnptr_type: Ty<'tcx>, fn_item_type: Ty<'tcx>) {
        let fnptr_type = self.tcx().erase_regions_ty(fnptr_type);
        let fn_item_type = self.tcx().erase_regions_ty(fn_item_type);
        debug!("Possible target fn item for fnptr type {:?}, {:?}", fnptr_type, fn_item_type);
        if let rustc_middle::ty::TyKind::FnPtr(fnsig) = fnptr_type.kind() {
            self.fnptr_sig_to_possible_targets.entry(*fnsig).or_default().insert(fn_item_type);
            // self.fnptr_possible_targets.insert(fn_item_type);
        } else {
            unreachable!();
        }
    }

    pub fn add_trait_upcasting_relation(&mut self, src_dyn_ty: Ty<'tcx>, tgt_dyn_ty: Ty<'tcx>) {
        let src_dyn_ty = type_util::strip_auto_traits(
            self.tcx(), 
            self.tcx().erase_regions_ty(src_dyn_ty)
        );
        let tgt_dyn_ty = type_util::strip_auto_traits(
            self.tcx(), 
            self.tcx().erase_regions_ty(tgt_dyn_ty)
        );
        if src_dyn_ty != tgt_dyn_ty {
            info!("trait_upcasting coercion from {:?} to {:?}", src_dyn_ty, tgt_dyn_ty);
            self.trait_upcasting_relations.entry(src_dyn_ty).or_default().insert(tgt_dyn_ty);
        }
    }

    pub fn add_call_edge(&mut self, callsite: BaseCallSite, callee_id: FuncId) {
        self.call_graph.add_edge(callsite, callsite.func, callee_id);
    }

    pub fn set_callsite_type(&mut self, callsite: BaseCallSite, call_type: CallType) {
        self.call_graph.set_callsite_type(callsite, call_type);
    }
    
    pub fn dump_call_graph(&self, cg_path: &std::path::Path) {
        results_dumper::dump_call_graph(self.acx, &self.call_graph, cg_path);
    }

}

