// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use std::collections::HashSet;
use std::fmt::{Debug, Formatter, Result};
use std::rc::Rc;
use std::time::Duration;

use itertools::Itertools;
use log::*;
use rustc_middle::ty::TyCtxt;

use super::*;
use super::strategies::context_strategy::{ContextStrategy, KObjectSensitive};
use super::strategies::stack_filtering::StackFilter;
use super::propagator::propagator::Propagator;
use super::PointerAnalysis;
use crate::graph::func_pag::FuncPAG;
use crate::graph::call_graph::CSCallGraph;
use crate::mir::call_site::{AssocCallGroup, CSCallSite, CallSite, CallType};
use crate::mir::context::{Context, ContextId};
use crate::mir::function::{FuncId, CSFuncId};
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::path::{Path, CSPath, PathEnum};
use crate::rta::rta::RapidTypeAnalysis;
use crate::util::pta_statistics::ContextSensitiveStat;
use crate::util::{self, chunked_queue, results_dumper};

pub type CallSiteSensitivePTA<'pta, 'tcx, 'compilation> = ContextSensitivePTA<'pta, 'tcx, 'compilation, KCallSiteSensitive>;
/// The object-sensitive pointer analysis for Rust has not been throughly evaluated so far.
pub type ObjectSensitivePTA<'pta, 'tcx, 'compilation> = ContextSensitivePTA<'pta, 'tcx, 'compilation, KObjectSensitive>;

pub struct ContextSensitivePTA<'pta, 'tcx, 'compilation, S: ContextStrategy> {
    /// The analysis context
    pub(crate) acx: &'pta mut AnalysisContext<'tcx, 'compilation>,
    /// Points-to data
    pub(crate) pt_data: DiffPTDataTy,
    /// Pointer Assignment Graph
    pub(crate) pag: PAG<Rc<CSPath>>,
    /// Call graph
    pub call_graph: CSCallGraph,

    /// Records the functions that have been processed
    pub(crate) processed_funcs: HashSet<CSFuncId>,

    /// Iterator for reachable functions
    rf_iter: chunked_queue::IterCopied<CSFuncId>,

    /// Iterator for address_of edges in pag
    addr_edge_iter: chunked_queue::IterCopied<EdgeId>,

    // Inter-procedure edges created for dynamic calls, which will be iterated
    // as initial constraints in propagator
    pub(crate) inter_proc_edges_queue: chunked_queue::ChunkedQueue<EdgeId>,

    assoc_calls: AssocCallGroup<NodeId, CSFuncId, Rc<CSPath>>,

    ctx_strategy: S,

    pub stack_filter: Option<StackFilter<CSFuncId>>,
    pub pre_analysis_time: Duration,
}

impl<'pta, 'tcx, 'compilation, S: ContextStrategy> Debug for ContextSensitivePTA<'pta, 'tcx, 'compilation, S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        "ContextSensitivePTA".fmt(f)
    }
}

/// Constructor
impl<'pta, 'tcx, 'compilation, S: ContextStrategy> ContextSensitivePTA<'pta, 'tcx, 'compilation, S> {
    pub fn new(acx: &'pta mut AnalysisContext<'tcx, 'compilation>, ctx_strategy: S) -> Self {
        let call_graph = CSCallGraph::new();
        let rf_iter = call_graph.reach_funcs_iter();
        let pag = PAG::new();
        let addr_edge_iter = pag.addr_edge_iter();
        ContextSensitivePTA {
            acx,
            pt_data: DiffPTDataTy::new(),
            pag,
            call_graph,
            processed_funcs: HashSet::new(),
            rf_iter,
            addr_edge_iter,
            inter_proc_edges_queue: chunked_queue::ChunkedQueue::new(),
            assoc_calls: AssocCallGroup::new(),
            ctx_strategy,
            stack_filter: None,
            pre_analysis_time: Duration::ZERO,
        }
    }

    #[inline]
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.acx.tcx
    }

    #[inline]
    pub fn get_context_id(&mut self, context: &Rc<Context<S::E>>) -> ContextId {
        self.ctx_strategy.get_context_id(context)
    }

    #[inline]
    pub fn get_context_by_id(&self, context_id: ContextId) -> Rc<Context<S::E>> {
        self.ctx_strategy.get_context_by_id(context_id)
        
    }
    #[inline]
    pub fn get_empty_context_id(&mut self) -> ContextId {
        self.ctx_strategy.get_empty_context_id()
    }
    
    /// Process statements in reachable functions.
    fn process_reach_funcs(&mut self) {
        while let Some(func) = self.rf_iter.next() {
            if !self.processed_funcs.contains(&func) {
                let func_ref = self.acx.get_function_reference(func.func_id);
                info!(
                    "Processing function {:?} {}, context: {:?}",
                    func.func_id,
                    func_ref.to_string(),
                    self.get_context_by_id(func.cid),
                );
                if self.pag.build_func_pag(self.acx, func.func_id) {
                    self.add_fpag_edges(func);
                    self.process_calls_in_fpag(func);
                }
            }
        }
    }

    /// Adds internal edges of a function pag to the whole program's pag.
    /// The function pag for the given def_id should be built before calling this function.
    pub fn add_fpag_edges(&mut self, func: CSFuncId) {
        if self.processed_funcs.contains(&func) {
            return;
        }

        let fpag = unsafe { &*(self.pag.func_pags.get(&func.func_id).unwrap() as *const FuncPAG) };
        let edges_iter = fpag.internal_edges_iter();
        for (src, dst, kind) in edges_iter {
            let cs_src = self.mk_cs_path(src, func.cid);
            let cs_dst = self.mk_cs_path(dst, func.cid);
            if let Some(edge_id) = self.pag.add_edge(&cs_src, &cs_dst, kind.clone()) {
                if cs_src.path.is_promoted_constant() || cs_src.path.is_static_variable() {
                    self.inter_proc_edges_queue.push(edge_id);
                }
            }
        }

        // add edges in the promoted functions
        // We do not analyze the promoted functions context sensitively
        if let Some(promoted_funcs) = self.pag.promoted_funcs_map.get(&func.func_id) {
            let promoted_funcs = unsafe { &*(promoted_funcs as *const HashSet<FuncId>) };
            for promoted_func in promoted_funcs {
                let cs_promtoted_func = CSFuncId::new(self.get_empty_context_id(), *promoted_func);
                self.add_fpag_edges(cs_promtoted_func);
            }
        }
        // add edges in the related static functions
        // We do not analyze the static functions context sensitively
        if let Some(static_funcs) = self.pag.involved_static_funcs_map.get(&func.func_id) {
            let static_funcs = unsafe { &*(static_funcs as *const HashSet<FuncId>) };
            for static_func in static_funcs {
                let cs_static_func = CSFuncId::new(self.get_empty_context_id(), *static_func);
                self.add_fpag_edges(cs_static_func);
            }
        }

        self.processed_funcs.insert(func);
    }

    fn process_calls_in_fpag(&mut self, func: CSFuncId) {
        let fpag = unsafe { &*(self.pag.get_func_pag(&func.func_id).unwrap() as *const FuncPAG) };
        // For static dispatch callsites, the call target can be resolved directly.
        for (callsite, callee) in &fpag.static_dispatch_callsites {
            let cs_callsite = self.mk_cs_callsite(callsite, func.cid);
            self.process_new_call(&cs_callsite, callee);
            self.call_graph.set_callsite_type(callsite.into(), CallType::StaticDispatch);
        }

        // For special callsites, we have summary the effects. Therefore we only add call edge
        // for the callsite without adding arg --> param and ret --> dst edges.
        for (callsite, callee) in &fpag.special_callsites {
            let cs_callsite = self.mk_cs_callsite(callsite, func.cid);
            // Do not add contexts for the special callees
            let empty_cid = self.special_callsite_context(&cs_callsite, callee);
            let cs_callee = self.mk_cs_func(*callee, empty_cid);
            self.call_graph.add_edge(cs_callsite.into(), func, cs_callee);
            // This may classify some special dynamic calls into static calls
            self.call_graph.set_callsite_type(callsite.into(), CallType::StaticDispatch);
        }

        // For std::ops::call, dynamic and fnptr callsites, add them to the dynamic_calls and fnptr_calls maps.
        for (dyn_fn_obj, callsite) in &fpag.dynamic_fntrait_callsites {
            let cs_dyn_fn_obj = self.mk_cs_path(dyn_fn_obj, func.cid);
            let cs_callsite = self.mk_cs_callsite(callsite, func.cid);
            let dyn_node_id = self.dyn_node_id(&cs_dyn_fn_obj);
            self.assoc_calls.add_dynamic_fntrait_call(dyn_node_id, cs_callsite);
            self.call_graph.set_callsite_type(callsite.into(), CallType::DynamicFnTrait);
        }
        for (dyn_var, callsite) in &fpag.dynamic_dispatch_callsites {
            let cs_dyn_var = self.mk_cs_path(dyn_var, func.cid);
            let cs_callsite = self.mk_cs_callsite(callsite, func.cid);
            let dyn_node_id = self.dyn_node_id(&cs_dyn_var);
            self.assoc_calls.add_dynamic_dispatch_call(dyn_node_id, cs_callsite);
            self.call_graph.set_callsite_type(callsite.into(), CallType::DynamicDispatch);
        }
        for (fn_ptr, callsite) in &fpag.fnptr_callsites {
            let cs_fn_ptr = self.mk_cs_path(fn_ptr, func.cid);
            let cs_callsite = self.mk_cs_callsite(callsite, func.cid);
            self.assoc_calls.add_fnptr_call(self.pag.get_or_insert_node(&cs_fn_ptr), cs_callsite);
            self.call_graph.set_callsite_type(callsite.into(), CallType::FnPtr);
        }
    }

    fn dyn_node_id(&mut self, dyn_obj: &Rc<CSPath>) -> NodeId {
        self.pag.get_or_insert_node(dyn_obj)
    }

    /// Process a resolved call according to the call type
    fn process_new_call(&mut self, callsite: &Rc<CSCallSite>, callee: &FuncId) {
        let callee_def_id = self.acx.get_function_reference(*callee).def_id;
        // an instance call
        if util::has_self_parameter(self.tcx(), callee_def_id) {
            // borrow self (&self or &mut self)
            if util::has_self_ref_parameter(self.tcx(), callee_def_id) {
                // the instance should be the pointed-to object of the self pointer
                if let Some(callee_cid) = self.ctx_strategy.new_instance_call_context(callsite, None) {
                    let cs_callee = CSFuncId::new(callee_cid, *callee);
                    self.add_call_edge(callsite, &cs_callee);
                }
                let self_ref: &Rc<CSPath> = callsite.args.get(0).expect("invalid arguments");
                let self_ref_id = self.pag.get_or_insert_node(self_ref);
                self.assoc_calls.add_static_dispatch_instance_call(self_ref_id, callsite.clone(), *callee);
            } else { // move self
                let instance = callsite.args.get(0).expect("invalid arguments");
                if let Some(callee_cid) = self.ctx_strategy.new_instance_call_context(callsite, Some(instance)) {
                    let cs_callee = CSFuncId::new(callee_cid, *callee);
                    self.add_call_edge(callsite, &cs_callee);
                }
            } 
        } else {
            let callee_cid = self.ctx_strategy.new_static_call_context(callsite);
            let cs_callee = CSFuncId::new(callee_cid, *callee);
            self.add_call_edge(callsite, &cs_callee);
        }
    }

    fn special_callsite_context(&mut self, callsite: &Rc<CSCallSite>, _callee: &FuncId) -> ContextId {
        // Currently we treat all special callsites as statical callsites
        self.ctx_strategy.new_static_call_context(callsite)
    }

    // Add new call edges to pag
    fn process_new_calls(&mut self, new_calls: &Vec<(Rc<CSCallSite>, FuncId)>) {
        for (callsite, callee_id) in new_calls {
            self.process_new_call(callsite, callee_id);
        }
        self.process_reach_funcs();
    }

    fn process_new_call_instances(&mut self, new_call_instances: &Vec<(Rc<CSCallSite>, Rc<CSPath>, FuncId)>) {
        for (callsite, instance, callee_id) in new_call_instances {
            if let Some(callee_cid) = self.ctx_strategy.new_instance_call_context(callsite, Some(instance)) {
                let cs_callee = CSFuncId::new(callee_cid, *callee_id);
                self.add_call_edge(callsite, &cs_callee);
            }
        }
        self.process_reach_funcs();
    }

    fn add_call_edge(&mut self, callsite: &Rc<CSCallSite>, callee: &CSFuncId) {
        let caller = callsite.func;
        if !self.call_graph.add_edge(callsite.into(), caller, *callee) {
            return;
        }
        let new_inter_proc_edges = self.pag.add_inter_procedural_edges(self.acx, callsite, *callee);
        for edge in new_inter_proc_edges {
            self.inter_proc_edges_queue.push(edge);
            self.add_page_edge_func(edge, callsite.func);
        }
    }


    fn mk_cs_path(&mut self, path: &Rc<Path>, cid: ContextId) -> Rc<CSPath> {
        match path.value() {
            PathEnum::Parameter { .. }
            | PathEnum::LocalVariable { .. }
            | PathEnum::ReturnValue { .. } 
            | PathEnum::Auxiliary { .. }
            | PathEnum::QualifiedPath { .. }
            | PathEnum::OffsetPath { .. } => {
                CSPath::new_cs_path(cid, path.clone())
            }
            PathEnum::HeapObj { .. } => {
                // Directly use the context of the method for the heap objects 
                CSPath::new_cs_path(cid, path.clone())
            }
            PathEnum::Constant
            | PathEnum::StaticVariable { .. }
            | PathEnum::PromotedConstant { .. }
            | PathEnum::Function(..) 
            | PathEnum::PromotedStrRefArray
            | PathEnum::PromotedArgumentV1Array
            | PathEnum::Type(..) => {
                // Context insensitive for these kinds of path
                let empty_cid = self.get_empty_context_id();
                CSPath::new_cs_path(empty_cid, path.clone())
            }
        }
    }

    fn mk_cs_func(&mut self, func_id: FuncId, cid: ContextId) -> CSFuncId {
        CSFuncId { cid, func_id }
    }

    fn mk_cs_callsite(&mut self, callsite: &Rc<CallSite>, cid: ContextId) -> Rc<CSCallSite> {
        Rc::new(CSCallSite::new(
            CSFuncId { cid, func_id: callsite.func },
            callsite.location,
            callsite
                .args
                .iter()
                .map(|arg| self.mk_cs_path(arg, cid))
                .collect_vec(),
            self.mk_cs_path(&callsite.destination, cid),
        ))
    }

    fn add_page_edge_func(&mut self, edge: EdgeId, func: CSFuncId) {
        if let Some(sf) = &mut self.stack_filter {
            sf.add_pag_edge_in_func(edge, func);
        }
    }

    #[inline]
    pub fn get_pt_data(&self) -> &DiffPTDataTy {
        &self.pt_data
    }

}

impl<'pta, 'tcx, 'compilation, S: ContextStrategy> PointerAnalysis<'tcx, 'compilation>
    for ContextSensitivePTA<'pta, 'tcx, 'compilation, S>
{
    fn pre_analysis(&mut self) {
        if !self.acx.analysis_options.stack_filtering {
            return;
        }
        info!("Start pre-analysis");
        let mut rta = RapidTypeAnalysis::new(&mut self.acx);
        rta.analyze();
        self.pre_analysis_time += rta.analysis_time;
        self.stack_filter = Some(StackFilter::new(rta.call_graph));
        self.ctx_strategy.with_stack_filter(self.stack_filter.as_mut().unwrap());
        self.pre_analysis_time += self.stack_filter.as_ref().unwrap().fra_time();
        println!("Pre-analysis time {}", 
            humantime::format_duration(self.pre_analysis_time).to_string()
        );
    }

    /// Initialize the analysis.
    fn initialize(&mut self) {
        // add the entry point to the call graph
        let entry_point = self.acx.entry_point;
        let empty_context_id = self.get_empty_context_id();
        let entry_func_id = self.acx.get_func_id(entry_point, self.tcx().mk_args(&[]));
        self.call_graph.add_node(CSFuncId::new(empty_context_id, entry_func_id));

        // process statements of reachable functions
        self.process_reach_funcs();
    }

    /// Solve the worklist problem using Propagator.
    fn propagate(&mut self) {
        let mut iter_proc_edge_iter = self.inter_proc_edges_queue.iter_copied();
        // Solve until no new call relationship is found.
        loop {
            let mut new_calls: Vec<(Rc<CSCallSite>, FuncId)> = Vec::new();
            let mut new_call_instances: Vec<(Rc<CSCallSite>, Rc<CSPath>, FuncId)> = Vec::new();
            let mut propagator = Propagator::new(
                self.acx,
                &mut self.pt_data,
                &mut self.pag,
                &mut new_calls,
                &mut new_call_instances,
                &mut self.addr_edge_iter,
                &mut iter_proc_edge_iter,
                &mut self.assoc_calls,
                self.stack_filter.as_mut(),
            );
            propagator.solve_worklist();

            if new_calls.is_empty() && new_call_instances.is_empty() {
                break;
            } else {
                self.process_new_calls(&new_calls);
                self.process_new_call_instances(&new_call_instances);
            }
        }
    }

    /// Finalize the analysis.
    fn finalize(&self) {
        // dump call graph, points-to results
        results_dumper::dump_results(self.acx, &self.call_graph, &self.pt_data, &self.pag);
        
        // dump pta statistics
        let pta_stat = ContextSensitiveStat::new(self);
        pta_stat.dump_stats();
    }
}
