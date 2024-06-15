// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! The key component of our pointer analysis. 

use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use log::*;
use rustc_hir::def_id::DefId;
use rustc_middle::mir;
use rustc_middle::ty::{Ty, TyCtxt, TyKind};

use crate::builder::call_graph_builder;
use crate::graph::pag::*;
use crate::mir::call_site::{AssocCallGroup, CallSiteS};
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::path::{PathEnum, PathSelector};
use crate::pta::*;
use crate::pts_set::points_to::PointsToSet;
use crate::util::{self, chunked_queue, type_util};


/// Propagating the points-to information along the PAG edges. 
pub struct Propagator<'pta, 'tcx, 'compilation, F, P: PAGPath> {
    /// The analysis context
    pub(crate) acx: &'pta mut AnalysisContext<'tcx, 'compilation>,
    /// Points-to data
    pub(crate) pt_data: &'pta mut DiffPTDataTy,
    /// Pointer Assignment Graph
    pub(crate) pag: &'pta mut PAG<P>,

    /// New calls and new instance calls to be processed.
    /// Basically, we resolved the following kinds of dynamic calls:
    /// - Dynamic trait call
    /// - Dynamic fntrait call
    /// - Fnptr call
    /// 1. For a dynamic trait call resolved with a concrete object that is pointed to by the dynamic trait object, 
    ///    we add it into `new_call_instances`
    /// 2. For a dynamic fntrait call with a concrete object that is pointed to by the dynamic fntrait object, 
    ///    a) when the concrete object is a function item or a closure, we directly resolve the call target 
    ///       and add it into `new_calls`. The resolved target function may be an associated function that 
    ///       has self or &self parameter, which needs to be cached in the assoc_calls. Specifically, when the 
    ///       first parameter of the resolved function is &self, we also add all the pointed-to objects of self 
    ///       into `new_call_instances` 
    ///    b) when the concrete object is a function pointer, we create a new function pointer callsite, and 
    ///       leave it to be processed by fnptr call process
    ///    c) For other cases, the concrete object should be a type that impls Fn* trait, we resolved the call 
    ///       as a normal dynamic trait call and add it into `new_call_instances`
    /// 3. For a fnptr call resolved with a concrete function item that is pointed to by the function pointer,
    ///    we add it into `new_calls` (The resolved target function may also be an associated function that 
    ///    has self or &self parameter.)
    /// 4. Except for the dynamic calls, For a statically dispatched method call on a receiver, we also dynamically
    ///    add the pointed-to objects of the receiver (the self reference) to `new_call_instances` for the need 
    ///    of object-sensitive pointer analysis
    new_calls: &'pta mut Vec<(Rc<CallSiteS<F, P>>, FuncId)>,
    new_call_instances: &'pta mut Vec<(Rc<CallSiteS<F, P>>, P, FuncId)>,

    /// Iterator for address_of edges in pag
    addr_edge_iter: &'pta mut chunked_queue::IterCopied<EdgeId>,

    /// Iterator for new inter-procedure edges of dynamic calls
    inter_proc_edge_iter: &'pta mut chunked_queue::IterCopied<EdgeId>,

    /// Worklist for resolution
    worklist: VecDeque<NodeId>,

    assoc_calls: &'pta mut AssocCallGroup<NodeId, F, P>,
}

impl<'pta, 'tcx, 'compilation, F, P> Propagator<'pta, 'tcx, 'compilation, F, P> where 
    F: Copy + Into<FuncId> + std::cmp::Eq + std::hash::Hash,
    P: PAGPath<FuncTy = F>,
{
    /// Constructor
    pub fn new(
        acx: &'pta mut AnalysisContext<'tcx, 'compilation>,
        pt_data: &'pta mut DiffPTDataTy,
        pag: &'pta mut PAG<P>,
        new_calls: &'pta mut Vec<(Rc<CallSiteS<F, P>>, FuncId)>,
        new_call_instances: &'pta mut Vec<(Rc<CallSiteS<F, P>>, P, FuncId)>,
        addr_edge_iter: &'pta mut chunked_queue::IterCopied<EdgeId>,
        inter_proc_edge_iter: &'pta mut chunked_queue::IterCopied<EdgeId>,
        assoc_calls: &'pta mut AssocCallGroup<NodeId, F, P>,
    ) -> Self {
        Propagator {
            acx,
            pt_data,
            pag,
            new_calls,
            new_call_instances,
            worklist: VecDeque::new(),
            addr_edge_iter,
            inter_proc_edge_iter,
            assoc_calls,
        }
    }

    #[inline]
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.acx.tcx
    }

    /// Propogate pts data until the worklist is empty.
    pub fn solve_worklist(&mut self) {
        self.init_constraints();
        while !self.worklist.is_empty() {
            let node_id = self.worklist.pop_front().unwrap();
            self.process_node(node_id);
        }
    }

    /// Initialize the worklist, activate new constraints.
    pub fn init_constraints(&mut self) {
        self.process_all_addr_edges();
        self.process_all_inter_proc_edges();
    }

    /// Process address edges.
    fn process_all_addr_edges(&mut self) {
        while let Some(edge_id) = self.addr_edge_iter.next() {
            self.process_addr(edge_id);
        }
    }

    /// Process arg-param edges.
    fn process_all_inter_proc_edges(&mut self) {
        while let Some(edge_id) = self.inter_proc_edge_iter.next() {
            self.propagate(edge_id, false);
        }
    }

    /// Start constraint solving.
    fn process_node(&mut self, node_id: NodeId) {
        self.handle_direct(node_id);
        self.handle_load_and_store(node_id);
        self.handle_gep(node_id);
        self.handle_cast(node_id);
        self.handle_offset(node_id);

        self.handle_static_dispatch_instance_call(node_id);
        self.handle_dynamic_dispatch_call(node_id);
        self.handle_fnptr_call(node_id);
        self.handle_dynamic_fntrait_call(node_id);

        self.pt_data.flush(node_id);
    }

    /// Process the given addr edge.
    fn process_addr(&mut self, addr_edge: EdgeId) {
        let (src, dst) = self.pag.graph().edge_endpoints(addr_edge).unwrap();
        if self.add_pts(dst, src) {
            self.worklist.push_back(dst);
        }
    }

    /// process all outgoing direct edges of the node.
    fn handle_direct(&mut self, node_id: NodeId) {
        if let Some(direct_out_edges) = self.pag.direct_out_edges.get_mut(&node_id) {
            let mut direct_out_edges = std::mem::take(direct_out_edges);

            for edge in &direct_out_edges {
                self.propagate(*edge, true);
            }

            std::mem::swap(
                self.pag.direct_out_edges.get_mut(&node_id).unwrap(),
                &mut direct_out_edges,
            );
        }
    }

    /// process all outgoing gep edges of the node.
    fn handle_gep(&mut self, node_id: NodeId) {
        if let Some(gep_out_edges) = self.pag.gep_out_edges.get_mut(&node_id) {
            let mut gep_out_edges = std::mem::take(gep_out_edges);

            if let Some(diff_pts) = self.get_diff_pts(node_id) {
                let diff_pts = diff_pts.clone();
                for gep_edge in &gep_out_edges {
                    self.process_gep(*gep_edge, &diff_pts);
                }
            }

            std::mem::swap(
                self.pag.gep_out_edges.get_mut(&node_id).unwrap(),
                &mut gep_out_edges,
            );
        }
    }

    /// process all outgoing load edges and incoming store edges of the node.
    fn handle_load_and_store(&mut self, node_id: NodeId) {
        if let Some(diff_pts) = self.get_diff_pts(node_id) {
            let diff_pts = diff_pts.clone();
            if let Some(load_out_edges) = self.pag.load_out_edges.get_mut(&node_id) {
                let mut load_out_edges = std::mem::take(load_out_edges);

                for load_edge in &load_out_edges {
                    self.process_load(*load_edge, &diff_pts);
                }

                std::mem::swap(
                    self.pag.load_out_edges.get_mut(&node_id).unwrap(),
                    &mut load_out_edges,
                );
            }

            if let Some(store_in_edges) = self.pag.store_in_edges.get_mut(&node_id) {
                let mut store_in_edges = std::mem::take(store_in_edges);

                for store_edge in &store_in_edges {
                    self.process_store(*store_edge, &diff_pts);
                }

                std::mem::swap(
                    self.pag.store_in_edges.get_mut(&node_id).unwrap(),
                    &mut store_in_edges,
                );
            }
        }
    }

    /// process all outgoing cast edges of the node.
    fn handle_cast(&mut self, node_id: NodeId) {
        if let Some(cast_out_edges) = self.pag.cast_out_edges.get_mut(&node_id) {
            let mut cast_out_edges = std::mem::take(cast_out_edges);

            for edge in &cast_out_edges {
                self.propagate_cast(*edge, true);   
            }

            std::mem::swap(
                self.pag.cast_out_edges.get_mut(&node_id).unwrap(),
                &mut cast_out_edges,
            );
        }
    }

    /// process all outgoing offset edges of the node.
    fn handle_offset(&mut self, node_id: NodeId) {
        if let Some(offset_out_edges) = self.pag.offset_out_edges.get_mut(&node_id) {
            let mut offset_out_edges = std::mem::take(offset_out_edges);

            for offset_edge in &offset_out_edges {
                self.process_offset(*offset_edge);
            }

            std::mem::swap(
                self.pag.offset_out_edges.get_mut(&node_id).unwrap(),
                &mut offset_out_edges,
            );
        }
    }

    /// If there are some static instance calls on this node
    fn handle_static_dispatch_instance_call(&mut self, node_id: NodeId) {
        if self.assoc_calls.static_dispatch_instance_calls.contains_key(&node_id) {
            let instance_callsites = self.assoc_calls.static_dispatch_instance_calls.get(&node_id).unwrap().clone();

            if let Some(diff_pts) = self.get_diff_pts(node_id) {
                let diff_pts = diff_pts.clone();
                for pointee in &diff_pts {
                    let (pointee_path, _pointee_type) = self.node_path_and_ty(pointee);
                    for (callsite, callee_id) in &instance_callsites {
                        self.add_new_call_instance(callsite, &pointee_path, callee_id);
                    }
                }
            }
        }
    }


    /// If this node is a dynamic trait object, add call edges for the dynamic calls.
    fn handle_dynamic_dispatch_call(&mut self, node_id: NodeId) {
        if self.assoc_calls.dynamic_dispatch_calls.contains_key(&node_id) {
            let dyn_callsites = self.assoc_calls.dynamic_dispatch_calls.get(&node_id).unwrap().clone();

            if let Some(diff_pts) = self.get_diff_pts(node_id) {
                let diff_pts = diff_pts.clone();
                self.process_dynamic_dispatch_call(&dyn_callsites, &diff_pts);
            }
        }
    }

    /// If this node is a fn pointer, add call edges for the fnptr calls.
    fn handle_fnptr_call(&mut self, node_id: NodeId) {
        if self.assoc_calls.fnptr_calls.contains_key(&node_id) {
            let callsites = self.assoc_calls.fnptr_calls.get(&node_id).unwrap().clone();

            if let Some(diff_pts) = self.get_diff_pts(node_id) {
                let diff_pts = diff_pts.clone();
                self.process_fnptr_call(&callsites, &diff_pts);
            }
        }
    }

    /// If this node is the first argument of a Fn::call|FnMut::call_mut|FnOnce::call_once,
    /// resolve the function call.
    fn handle_dynamic_fntrait_call(&mut self, node_id: NodeId) {
        if self.assoc_calls.dynamic_fntrait_calls.contains_key(&node_id) {
            let dynamic_fntrait_callsites =
                self.assoc_calls.dynamic_fntrait_calls.get(&node_id).unwrap().clone();

            if let Some(diff_pts) = self.get_diff_pts(node_id) {
                let diff_pts = diff_pts.clone();
                self.process_dynamic_fntrait_call(&dynamic_fntrait_callsites, &diff_pts);
            }
        }
    }

    /// Process the given load edge.
    /// src --load--> dst:  node \in pts(src) ==> node --direct-->dst
    fn process_load(&mut self, load_edge: EdgeId, base_pts: &PointsTo<NodeId>) {
        let (_src, dst) = self.pag.graph().edge_endpoints(load_edge).unwrap();
        let PAGEdgeEnum::LoadPAGEdge(load_proj) = self.pag.get_edge(load_edge).kind.clone() else { unreachable!() };
        
        let dst_path = self.pag.node_path(dst).clone();
        for pointee in base_pts {
            let pointee_path = self.pag.node_path(pointee).clone();
            let src_path = pointee_path.append_projection(&load_proj);

            if let Some(edge_id) = self.add_direct_edge(&src_path, &dst_path) {
                self.propagate(edge_id, false);
            }
        }
    }

    /// Process the given store edge.
    /// src --store--> dst:  node \in pts(dst) ==> src --direct--> node
    fn process_store(&mut self, store_edge: EdgeId, base_pts: &PointsTo<NodeId>) {
        let (src, _dst) = self.pag.graph().edge_endpoints(store_edge).unwrap();
        let PAGEdgeEnum::StorePAGEdge(store_proj) = self.pag.get_edge(store_edge).kind.clone() else { unreachable!() };
        
        let src_path = self.pag.node_path(src).clone();
        for pointee in base_pts {
            let pointee_path = self.pag.node_path(pointee).clone();
            let dst_path = pointee_path.append_projection(&store_proj);

            if let Some(edge_id) = self.add_direct_edge(&src_path, &dst_path) {
                self.propagate(edge_id, false);
            }
        }
    }

    /// Process the given gep edge.
    fn process_gep(&mut self, gep_edge: EdgeId, base_pts: &PointsTo<NodeId>) {
        let (_src, dst) = self.pag.graph().edge_endpoints(gep_edge).unwrap();
        let PAGEdgeEnum::GepPAGEdge(gep_proj) = self.pag.get_edge(gep_edge).kind.clone() else { unreachable!() };
        
        let mut changed = false;
        for pointee in base_pts {
            let pointee_path = self.pag.node_path(pointee).clone();
            let src_path = pointee_path.append_projection(&gep_proj);

            let src_id = self.pag.get_or_insert_node(&src_path);
            if self.add_pts(dst, src_id) {
                changed = true;
            }
        }
        if changed {
            self.worklist.push_back(dst);
        }
    }

    /// Process the given offset edge.
    /// Currently we treat an offset edge as a direct edge if the source path and destination
    /// path have the same type. It works for most cases as the offset function are usually used
    /// for accessing an element inside an array or a vector heap block (e.g. `offset` called in
    /// `Vec::push` and when iterating a vec).
    fn process_offset(&mut self, offset_edge: EdgeId) {
        // We don't check whether src and dst have the same type here, as the propagate function will
        // ignore the value flow if they are not of the same type.
        self.propagate(offset_edge, true);
    }

    fn process_dynamic_dispatch_call(
        &mut self,
        dyn_callsites: &HashSet<Rc<CallSiteS<F, P>>>,
        dyn_pts: &PointsTo<NodeId>,
    ) {
        for pointee in dyn_pts {
            let (pointee_path, pointee_type) = self.node_path_and_ty(pointee);
            for dyn_callsite in dyn_callsites {
                // Replace the first generic type in generic args with the pointee type.
                let (callee_def_id, gen_args) = self
                    .acx
                    .get_dyn_callee_identifier(&dyn_callsite.into())
                    .expect("Uncached dynamic callsite");
                let mut replaced_args = gen_args.to_vec();
                replaced_args[0] = pointee_type.into();
                let replaced_args = self.tcx().mk_args(&replaced_args);

                // Devirtualize the callee function
                if let Some((callee_def_id, gen_args)) = call_graph_builder::try_to_devirtualize(
                    self.tcx(),
                    *callee_def_id,
                    replaced_args,
                ) {
                    let func_id = self.acx.get_func_id(callee_def_id, gen_args);
                    // self.add_new_call(&dyn_callsite, &func_id);
                    self.add_new_call_instance(&dyn_callsite, &pointee_path, &func_id);
                } else {
                    warn!(
                        "Could not resolve function: {:?}, {:?}",
                        *callee_def_id, replaced_args
                    );
                }
            }
        }
    }

    fn process_fnptr_call(&mut self, callsites: &HashSet<Rc<CallSiteS<F, P>>>, fn_pts: &PointsTo<NodeId>) {
        for fn_item_id in fn_pts {
            // The pointee of a function pointer can be classified into the following three kinds:
            // (a) a variable of FnDef type
            //     e.g. ``` let f = times2;
            //              let fp: fn(i32) -> i32 = f; ```
            //          where the function pointer `fp` points to the local variable `f` of type `fn(i32) -> i32 {times2}`
            // (b) an const operand of FnDef type
            //     e.g. ``` let fp: fn(i32) -> i32 = times2; ```
            //          where the function pointer `fp` points a constant of type FnDef `fn(i32) -> i32 {times2}`
            // (c) a variable of closure type
            //     e.g. ``` let fp: fn(i32) -> i32 = |x| 2 * x; ```
            //          where the function pointer `fp` points to a closure local variable automatically generated in mir.
            // For the first two cases (a) and (b), we create a devirtualized PathEnum::Function path to represent the pointee.
            let (mut fn_item, mut fn_item_ty) = self.node_path_and_ty(fn_item_id);
            // Some function items maybe transmuted to a function pointer type
            if matches!(fn_item_ty.kind(), TyKind::FnPtr(..)) {
                fn_item = fn_item.remove_cast();
                fn_item_ty = fn_item.try_eval_path_type(self.acx);
            }
            match fn_item_ty.kind() {
                // A function pointer can point to a trait-defined function. However, we do not need to 
                // perform static dispatch here as each function item is statically dispatched when initialized.
                TyKind::FnDef(..) => {
                    if let PathEnum::Function(func_id) = fn_item.value() {
                        for callsite in callsites {
                            self.add_new_call(callsite, func_id);
                        }
                    }
                }
                // closures can only be coerced to `fn` types if they do not capture any variables
                TyKind::Closure(def_id, args) | TyKind::Coroutine(def_id, args) => {
                    for callsite in callsites {
                        let closure_callsite = self.create_closure_callsite(
                            callsite.clone(),
                            fn_item.clone(),
                            fn_item_ty,
                            *def_id,
                        );
                        let callee_func_id = self.acx.get_func_id(*def_id, args);
                        self.add_new_call(&closure_callsite, &callee_func_id);
                    }
                }
                _ => {
                    error!("Unexpected type of function pointer's pointee: {:?}", fn_item_ty.kind());
                }
            }
        }
    }

    // The pointer points to an object which implements Fn|FnMut|FnOnce trait.
    fn process_dynamic_fntrait_call(
        &mut self,
        dynamic_fntrait_callsites: &HashSet<Rc<CallSiteS<F, P>>>,
        first_arg_pts: &PointsTo<NodeId>,
    ) {
        let unpack_args_tuple = |args_tuple: &P, tuple_type: Ty| -> Vec<P> {
            if let TyKind::Tuple(tuple_types) = tuple_type.kind() {
                tuple_types
                    .iter()
                    .enumerate()
                    .map(|(i, _t)| {
                        let proj_elems = vec![PathSelector::Field(i)];
                        args_tuple.append_projection(&proj_elems)
                    })
                    .collect()
            } else {
                // The argument may be a constant `()`. We currently did not cache the type for constants.
                vec![]
            }
        };

        for pointee_id in first_arg_pts {
            let (pointee_path, pointee_type) = self.node_path_and_ty(pointee_id);
            match pointee_type.kind() {
                TyKind::FnDef(def_id, args) => {
                    // try to devirtualize the def_id first
                    let (def_id, args) = call_graph_builder::resolve_fn_def(self.tcx(), *def_id, args);
                    let callee_func_id = self.acx.get_func_id(def_id, args);
                    for dynamic_fntrait_callsite in dynamic_fntrait_callsites {
                        let new_callsite = Rc::new(CallSiteS::new(
                            dynamic_fntrait_callsite.func,
                            dynamic_fntrait_callsite.location,
                            unpack_args_tuple(
                                &dynamic_fntrait_callsite.args[1],
                                dynamic_fntrait_callsite.args[1].try_eval_path_type(self.acx),
                            ),
                            dynamic_fntrait_callsite.destination.clone(),
                        ));
                        self.add_new_call(&new_callsite, &callee_func_id);
                    }
                }
                TyKind::Closure(def_id, args) | TyKind::Coroutine(def_id, args) => {
                    // If the function item resolved from the dynamic fntrait object is a
                    // closure, the fntrait must be Fn or FnMut trait. It cannot be a FnOnce trait.
                    // For example, the following code cannot be compiled:
                    // ```
                    // fn foo(f: &dyn FnOnce(u32) -> u32) {
                    //     f(1);
                    // }
                    // ```
                    // The Rust compiler will report the error:
                    //      f(1);
                    //      ^ the size of `dyn FnOnce(u32) -> u32` cannot be statically determined
                    // Therefore, the first argument of the resolved closure must be a reference to the closure.
                    // The only case where a dyn FnOnce object can be used is Box<dyn FnOnce>.
                    // For example, the following code is valid:
                    // ```
                    // let f: Box<dyn FnOnce(i8)> = Box::new(|x| {
                    //      assert!(x == 1);
                    // });
                    // f(1);
                    // ```
                    // The function call `f(1)` will be resolved to the implementation of FnOnce for Box<F, A>,
                    // in which the indirect call is achieved via the code like:
                    // ``` <dyn FnOnce<Args> as std::ops::FnOnce<Args>>::call_once((*_3), move _4) ```
                    // Note that this special case does not affect the handling of dynamic fntrait calls, since
                    // the type of the first argument of this case is `dyn FnOnce` type instead of a dynamic reference
                    // type, which prevents us from inferring the concrete type from the pointee information. Therefore,
                    // this case can only be processed by special handlings.
                    for dynamic_fntrait_callsite in dynamic_fntrait_callsites {
                        let mut closure_args = unpack_args_tuple(
                            &dynamic_fntrait_callsite.args[1],
                            dynamic_fntrait_callsite.args[1].try_eval_path_type(self.acx),
                        );
                        // For Fn and FnMut cases, the first argument should be of &dyn Fn or &dyn FnMut type, and the
                        // first parameter of the closure should be of &[closure] type. Therefore, they are of compatible
                        // types and we can add a direct edge between them. Incompatible value flows can be filtered by
                        // type filter in the propagate function.
                        closure_args.insert(0, dynamic_fntrait_callsite.args[0].clone());
                        let closure_callsite = Rc::new(CallSiteS::new(
                            dynamic_fntrait_callsite.func,
                            dynamic_fntrait_callsite.location,
                            closure_args,
                            dynamic_fntrait_callsite.destination.clone(),
                        ));
                        let callee_func_id = self.acx.get_func_id(*def_id, args);
                        self.add_new_call(&closure_callsite, &callee_func_id);
                    }
                }
                TyKind::FnPtr(..) => {
                    // If the first argument of a std::ops::call refers to a function pointer,
                    // we can add this callsite as a fnptr call, and process with the whole points-to set
                    // of the function pointer.
                    // For pointer analysis with projection-based path representation, we know exactly the
                    // fnptr paths that the dynamic trait object points to, but for offset-based pointer
                    // analysis, we only know that the dynamic trait object refers to a fnptr. Therefore we
                    // take different ways to handle this.
                    let callsites = dynamic_fntrait_callsites
                        .iter()
                        .map(|dynamic_fntrait_callsite| {
                            Rc::new(CallSiteS::new(
                                dynamic_fntrait_callsite.func,
                                dynamic_fntrait_callsite.location,
                                unpack_args_tuple(
                                    &dynamic_fntrait_callsite.args[1],
                                    dynamic_fntrait_callsite.args[1].try_eval_path_type(self.acx),
                                ),
                                dynamic_fntrait_callsite.destination.clone(),
                            ))
                        })
                        .collect::<HashSet<Rc<CallSiteS<F, P>>>>();
                    if let Some(propa) = self.get_propa_pts(pointee_id) {
                        let propa = propa.clone();
                        self.process_fnptr_call(&callsites, &propa);
                    }
                    if let Some(diff) = self.get_diff_pts(pointee_id) {
                        let diff = diff.clone();
                        self.process_fnptr_call(&callsites, &diff);
                    }
                    for callsite in callsites {
                        self.assoc_calls.add_fnptr_call(pointee_id, callsite.clone());
                    }
                }
                _ => {
                    // The first argument of the call is a reference to a object that implements Fn|FnMut|FnOnce trait.
                    // For example:
                    // ```
                    // let fp: fn(i32) -> i32 = times2;
                    // let f = &&fp;
                    // f(2);
                    // ```
                    // The variable `f` is a reference which refers to a reference to the function pointer `fp`.
                    // The call `f(2)` at the third line can be successfully compiled to a Fn*::call*, because rust
                    // automatically implements Fn* Trait for the reference type that refers to a type which impls
                    // Fn* Trait. Since the Fn* Trait is implemented for the function pointer type (as well as FnDef,
                    // and Closure type) by default, it will also be implemented for &FnPtr, &&FnPtr... recursively.
                    // Therefore, the following code can also be compiled, albeit quite odd.
                    // ``` let f = &&&&&&&&&fp; f(2); ```
                    // For this case, we add the pair (pointee_path, callsite) to `dynamic_fntrait_callsite`, and recursively
                    // solve it.
                    for dynamic_fntrait_callsite in dynamic_fntrait_callsites {
                        // replace the first type in callee_susbts with the pointee type
                        let (callee_def_id, gen_args) = self
                            .acx
                            .get_dyn_callee_identifier(&dynamic_fntrait_callsite.into())
                            .expect("Uncached dynamic callsite");
                        let mut replaced_args = gen_args.to_vec();
                        replaced_args[0] = pointee_type.into();
                        let replaced_args = self.tcx().mk_args(&replaced_args);

                        debug!(
                            "Dynamically resolve std::ops::call for {:?}::<{:?}> with replaced generic args {:?}",
                            callee_def_id,
                            gen_args,
                            replaced_args
                        );

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
                                // self.add_new_call(&dynamic_fntrait_callsite, &func_id);
                                self.add_new_call_instance(&dynamic_fntrait_callsite, &pointee_path, &func_id);
                            } else {
                                warn!("Unavailable mir for def_id: {:?}", resolved_def_id);
                            }
                        } else {
                            warn!(
                                "Could not resolve function: {:?}, {:?}",
                                *callee_def_id, replaced_args
                            );
                        }
                    }
                }
            }
        }
    }

    /// Adds a new direct edge from src to dst if it does not exist in the graph
    /// Returns the edge id if this edge is newly added to the graph
    fn add_direct_edge(&mut self, src: &P, dst: &P) -> Option<EdgeId> {
        if !self.pag.has_edge(src, dst, &PAGEdgeEnum::DirectPAGEdge) {
            self.pag.add_direct_edge(src, dst)
        } else {
            None
        }
    }

    /// Process the given direct edge.
    fn propagate(&mut self, direct_edge: EdgeId, propa_diff: bool) {
        let mut changed = false;
        let (src, dst) = self.pag.graph().edge_endpoints(direct_edge).unwrap();
        // If src is a pointer or a reference.
        if self.get_propa_pts(src).is_some() || self.get_diff_pts(src).is_some() {
            // check the type of src and dst
            let (src_path, src_type) = self.node_path_and_ty(src);
            let (dst_path, dst_type) = self.node_path_and_ty(dst);
            // debug!("Propagating from {:?}({:?}) -> {:?}({:?})", src_path, src_type, dst_path, dst_type);
            
            let type_filter_pred = Self::type_filter_pred();

            if !type_util::equivalent_ptr_types(self.tcx(), src_type, dst_type) {
                debug!(
                    "Filtering propagating from {:?}({:?}) to {:?}({:?})",
                    src_path, src_type, dst_path, dst_type
                );
                return;
            }

            let src_deref_type = type_util::get_dereferenced_type(src_type);
            let dst_deref_type = type_util::get_dereferenced_type(dst_type);
            if let Some(diff) = self.pt_data.get_diff_pts(src) {
                for pointee in &diff.clone() {
                    let (_pointee_path, pointee_type) = self.node_path_and_ty(pointee);
                    if type_filter_pred(self.acx, pointee_type, src_deref_type, dst_deref_type) 
                    {   
                        continue;
                    } 

                    changed |= self.add_pts(dst, pointee);
                }
            }

            if !propa_diff {
                if let Some(propa) = self.pt_data.get_propa_pts(src) {
                    for pointee in &propa.clone() {
                        let (_pointee_path, pointee_type) = self.node_path_and_ty(pointee);
                        if type_filter_pred(self.acx, pointee_type, src_deref_type, dst_deref_type) 
                        {
                            continue;
                        }  

                        changed |= self.add_pts(dst, pointee);
                    }
                }
            }

            if changed {
                self.worklist.push_back(dst);
            }
            return;

            // To be optimized
            // if propa_diff {
            //     // Propagate the diff points-to set only.
            //     changed |= self.pt_data.union_diff_pts(dst, src);
            // } else {
            //     // Propagate the whole points-to set.
            //     changed |= self.pt_data.union_pts(dst, src);
            // }
        }
        if changed {
            self.worklist.push_back(dst);
        }
    }

    /// Adds cast edges between src and dst, src --cast--> dst, and dst --cast-->src.
    /// If any of the cast edges is newly added to the graph, propagate along this edge
    fn add_cast_edge_and_propagate(&mut self, src: &P, dst: &P) {
        if let Some(edge_id) = self.add_cast_edge(src, dst) {
            self.propagate_cast(edge_id, false);
        }
        if let Some(edge_id) = self.add_cast_edge(dst, src) {
            self.propagate_cast(edge_id, false);
        }
    }

    /// Adds a new cast edge from src to dst if it does not exist in the graph
    /// Returns the edge id if this edge is newly added to the graph
    fn add_cast_edge(&mut self, src: &P, dst: &P) -> Option<EdgeId> {
        if !self.pag.has_edge(src, dst, &PAGEdgeEnum::CastPAGEdge) {
            self.pag.add_cast_edge(src, dst)
        } else {
            None
        }
    }

    /// Process the given cast edge.
    fn propagate_cast(&mut self, cast_edge: EdgeId, propa_diff: bool) {
        let mut changed = false;
        let (src, dst) = self.pag.graph().edge_endpoints(cast_edge).unwrap();
        let (_src_path, src_ty) = self.node_path_and_ty(src);
        let (_dst_path, dst_ty) = self.node_path_and_ty(dst);
        // debug!("Propagating cast from {:?}({:?}) -> {:?}({:?})", src_path, src_ty, dst_path, dst_ty);
        assert!(src_ty.is_any_ptr() && dst_ty.is_any_ptr());

        let dst_deref_ty = type_util::get_dereferenced_type(dst_ty);
        let src_pts = self.get_cloned_pts(src, propa_diff);

        // Special handling for function pointers. Since the src type and dst type are non-equivalent for
        // cast edges, if the src(dst) type is function pointer type, the dst(src) must not be function
        // type
        if src_ty.is_fn_ptr() {
            for pointee in &src_pts {
                changed |= self.cast_and_add_pts(dst, pointee, dst_deref_ty);
            }
            if changed {
                self.worklist.push_back(dst);
            }
            return;
        }
        if dst_ty.is_fn_ptr() {
            // Casting a function's reference to a function pointer is invalid in Rust, but
            // this cast algorithm supports it. This may introduce infeasible call path, but
            // it would not impact the soundness.
            for pointee in &src_pts {
                changed |= self.uncast_and_add_fnptr_pts(dst, pointee);
            }
            if changed {
                self.worklist.push_back(dst);
            }
            return;
        }

        // Skip casts for certain cases that may significantly impact the efficiency
        if self.acx.analysis_options.cast_constraint && type_util::is_basic_pointer(src_ty) {
            for pointee in &src_pts {
                let pointee_path = self.pag.node_path(pointee).clone();
                // debug!("Pointee of source to be cast: {:?}", pointee_path);
                let regularized_path = pointee_path.regularize(self.acx);

                // Perform cast if the pointee path has not been cast to any other type before
                if !regularized_path.has_been_cast(self.acx) {
                    if let Some(cast_path) = regularized_path.cast_to(self.acx, dst_deref_ty) {
                        let cast_path_id = self.pag.get_or_insert_node(&cast_path);
                        changed |= self.pt_data.add_pts(dst, cast_path_id);
                        continue;
                    }
                }
                // If the pointee has been cast to the dst_deref_ty before
                if let Some(type_variant) = regularized_path.type_variant(self.acx, dst_deref_ty)
                {
                    let type_variant_id = self.pag.get_or_insert_node(&type_variant);
                    changed |= self.pt_data.add_pts(dst, type_variant_id);
                    continue;
                } 
                // If the pointer is cast to another basic pointer ty, e.g. *u8 -> *[u8]
                if type_util::is_basic_pointer(dst_ty) {
                    if let Some(cast_path) = regularized_path.cast_to(self.acx, dst_deref_ty) {
                        let cast_path_id = self.pag.get_or_insert_node(&cast_path);
                        changed |= self.pt_data.add_pts(dst, cast_path_id);
                        continue;
                    }
                } 
                if matches!(regularized_path.value(), PathEnum::HeapObj { .. }) {
                    // For heap objects that have a concretized type, we do not let it been cast from 
                    // a simple type to other incompatible types.
                    if let Some(concre_ty) = regularized_path.concretized_heap_type(self.acx) {
                        let mut compatible_cast = false;
                        match dst_deref_ty.kind() {
                            TyKind::Array(elem_ty, _) | TyKind::Slice(elem_ty) => {
                                if type_util::equal_types(self.tcx(), concre_ty, *elem_ty) {
                                    compatible_cast = true;
                                }
                            }
                            _ => {}
                        }
                        if !compatible_cast {
                            continue;
                        }
                    }
                    if let Some(cast_path) = regularized_path.cast_to(self.acx, dst_deref_ty) {
                        let cast_path_id = self.pag.get_or_insert_node(&cast_path);
                        changed |= self.pt_data.add_pts(dst, cast_path_id);
                        continue;
                    }
                }
            }

            if changed {
                self.worklist.push_back(dst);
            }
            return;
        }

        // let param_env = rustc_middle::ty::ParamEnv::reveal_all();
        for pointee in &src_pts {
            let pointee_path = self.pag.node_path(pointee).clone();
            // debug!("Pointee of source to be cast: {:?}", pointee_path);
            if let Some(cast_path) = pointee_path.cast_to(self.acx, dst_deref_ty) {
                let cast_path_id = self.pag.get_or_insert_node(&cast_path);
                changed |= self.pt_data.add_pts(dst, cast_path_id);

                // flatten both the pointee path and the cast path and add edge between the pointer type fields
                let pointee_flattened_fields = pointee_path.flatten_fields(self.acx);
                let cast_flattened_fields = cast_path.flatten_fields(self.acx);
                self.cast_between_flattened_fields(pointee_flattened_fields, cast_flattened_fields);
            }
        }

        if changed {
            self.worklist.push_back(dst);
        }
    }

    fn cast_between_flattened_fields(
        &mut self,
        src_flattened_fields: Vec<(usize, P, Ty<'tcx>)>,
        tgt_flattened_fields: Vec<(usize, P, Ty<'tcx>)>,
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
                src_field.set_path_rustc_type(self.acx, *src_type);
                tgt_field.set_path_rustc_type(self.acx, *tgt_type);
                if type_util::equivalent_ptr_types(self.tcx(), *src_type, *tgt_type) {
                    if let Some(edge_id) = self.add_direct_edge(src_field, tgt_field) {
                        self.propagate(edge_id, false);
                    }
                    if let Some(edge_id) = self.add_direct_edge(tgt_field, src_field) {
                        self.propagate(edge_id, false);
                    }
                } else {
                    self.add_cast_edge_and_propagate(src_field, tgt_field);
                }
            } else {
                if src_type.is_enum() && src_type == tgt_type {
                    // We do not flatten fields for enum type, therefore if src_type and tgt_type are the same enum types,
                    // we add direct edges between their pointer fields.
                    let ptr_projs = self.acx.get_pointer_projections(*src_type);
                    let ptr_projs = ptr_projs.clone();
                    for (ptr_proj, ptr_ty) in ptr_projs {
                        let src_field = src_field.append_projection(&ptr_proj);
                        let tgt_field = tgt_field.append_projection(&ptr_proj);
                        src_field.set_path_rustc_type(self.acx, ptr_ty);
                        tgt_field.set_path_rustc_type(self.acx, ptr_ty);
                        if let Some(edge_id) = self.add_direct_edge(&src_field, &tgt_field) {
                            self.propagate(edge_id, false);
                        }
                        if let Some(edge_id) = self.add_direct_edge(&tgt_field, &src_field) {
                            self.propagate(edge_id, false);
                        }
                    }
                }
            }
            tgt_field_index += 1;
            src_field_index += 1;
        }
    }

    /// Uncast the pointee and add it to fnptr's pts if it is a function item
    fn uncast_and_add_fnptr_pts(&mut self, fnptr: NodeId, pointee: NodeId) -> bool {
        let pointee_path = self.pag.node_path(pointee);
        let original_path = pointee_path.remove_cast();
        // The type of original path should have been cached
        let original_ty = original_path.try_eval_path_type(self.acx);
        match original_ty.kind() {
            TyKind::FnDef(..) | TyKind::Closure(..) | TyKind::Coroutine(..) => {
                let original_id = self.pag.get_node_id(&original_path).unwrap();
                self.add_pts(fnptr, original_id)
            }
            _ => {
                // warn!("Propagate a non-function item to function pointer!");
                false
            }
        }
    }

    /// Cast the pointee to a given type and add to the pts set of the pointer
    fn cast_and_add_pts(&mut self, pointer: NodeId, pointee: NodeId, cast_ty: Ty<'tcx>) -> bool {
        let pointee_path = self.pag.node_path(pointee);
        if let Some(cast_path) = pointee_path.cast_to(self.acx, cast_ty) {
            let cast_path_id = self.pag.get_or_insert_node(&cast_path);
            self.pt_data.add_pts(pointer, cast_path_id)
        } else {
            false
        }
    }

    /// Union/add points-to.
    fn add_pts(&mut self, pointer: NodeId, pointee: NodeId) -> bool {
        self.pt_data.add_pts(pointer, pointee)
    }

    // Get points-to data
    #[inline]
    pub fn get_pt_data(&self) -> &DiffPTDataTy {
        self.pt_data
    }

    // Get points-to
    #[inline]
    pub fn get_propa_pts(&self, id: NodeId) -> Option<&PointsTo<NodeId>> {
        self.pt_data.get_propa_pts(id)
    }

    #[inline]
    pub fn get_diff_pts(&self, id: NodeId) -> Option<&PointsTo<NodeId>> {
        self.pt_data.get_diff_pts(id)
    }

    /// Returns a node's points-to set cloned from the diff points-to set or
    /// the union of the propa and diff points-to set.
    fn get_cloned_pts(&self, id: NodeId, diff_only: bool) -> PointsTo<NodeId> {
        if let Some(diff) = self.get_diff_pts(id) {
            let mut diff = diff.clone();
            if !diff_only {
                if let Some(propa) = self.get_propa_pts(id) {
                    diff.union(propa);
                }
            }
            return diff;
        } else {
            if !diff_only {
                if let Some(propa) = self.get_propa_pts(id) {
                    return propa.clone();
                }
            }
            return PointsTo::new();
        }
    }

    #[inline]
    pub fn node_path_and_ty(&mut self, id: NodeId) -> (P, Ty<'tcx>) {
        let path = self.pag.node_path(id);
        let ty = path.try_eval_path_type(self.acx);
        (path.clone(), ty)
    }

    /// If a fnptr callsite or a Fn*::call* refers to a closure call, we need to create
    /// a new callsite for the closure call by adding a closure reference variable
    /// to the arguments.
    fn create_closure_callsite(
        &mut self,
        callsite: Rc<CallSiteS<F, P>>,
        closure_path: P,
        closure_ty: Ty<'tcx>,
        closure_def_id: DefId,
    ) -> Rc<CallSiteS<F, P>> {
        assert!(matches!(
            closure_ty.kind(),
            TyKind::Closure(..) | TyKind::Coroutine(..)
        ));
        // Prepend the callee closure/generator/function to the unpacked arguments vector
        // if the called function actually expects it.
        let mut actual_args = callsite.args.clone();
        actual_args.insert(0, closure_path.clone());

        // call_once consumes its callee argument. If the callee does not,
        // we have to provide it with a reference.
        let mir = self.tcx().optimized_mir(closure_def_id);
        if let Some(decl) = mir.local_decls.get(mir::Local::from(1usize)) {
            if decl.ty.is_ref() {
                // create a reference path to this closure
                let closure_ref_ty = Ty::new_mut_ref(self.tcx(), self.tcx().lifetimes.re_static, closure_ty);
                // To optimize. This may introduce redundant aux variables.
                let closure_ref_path = PAGPath::new_aux_local_path(self.acx, callsite.func, closure_ref_ty);
                let addr_edge = self
                    .pag
                    .add_addr_edge(&closure_path, &closure_ref_path)
                    .expect("Expect a newly added address_of edge");
                self.process_addr(addr_edge);
                actual_args[0] = closure_ref_path; 
            }
        }
        // Set up a new callsite
        Rc::new(CallSiteS::new(
            callsite.func,
            callsite.location,
            actual_args,
            callsite.destination.clone(),
        ))
    }

    fn add_new_call(&mut self, callsite: &Rc<CallSiteS<F, P>>, callee_id: &FuncId) {
        self.new_calls.push((callsite.clone(), *callee_id));

        let callee_def_id = self.acx.get_function_reference(*callee_id).def_id;
        if util::has_self_ref_parameter(self.tcx(), callee_def_id) {
            let self_ref: &P = callsite.args.get(0).expect("invalid arguments");
            let self_ref_id = self.pag.get_or_insert_node(self_ref);
            if let Some(propa) = self.get_propa_pts(self_ref_id) {
                let propa = propa.clone();
                for pointee in &propa {
                    let pointee_path = self.pag.node_path(pointee).clone();
                    self.add_new_call_instance(callsite, &pointee_path, callee_id);
                }
            }
        }
    }

    fn add_new_call_instance(&mut self, callsite: &Rc<CallSiteS<F, P>>, instance: &P, callee_id: &FuncId) {
        self.new_call_instances.push((callsite.clone(), instance.clone(), *callee_id))
    }

    fn type_filter_pred() -> impl Fn(&AnalysisContext<'tcx, '_>, Ty<'tcx>, Ty<'tcx>, Ty<'tcx>) -> bool {
        |acx: &AnalysisContext<'tcx, '_>, pointee_ty: Ty<'tcx>, src_deref_type: Ty<'tcx>, dst_deref_ty: Ty<'tcx>| 
            -> bool 
        {
            if src_deref_type.is_trait() && !dst_deref_ty.is_trait() && 
                !type_util::equal_types(acx.tcx, pointee_ty, dst_deref_ty) 
            {
                true
            } else {
                false
            }
        }
    }
}
