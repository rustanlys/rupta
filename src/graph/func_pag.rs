// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use std::collections::HashSet;
use std::rc::Rc;

use super::pag::PAGEdgeEnum;
use crate::mir::call_site::CallSite;
use crate::mir::function::FuncId;
use crate::mir::path::Path;
use crate::util::chunked_queue::{self, ChunkedQueue};

/// A tuple type consisiting of source path, destination path and path edge type
pub type InternalEdge = (Rc<Path>, Rc<Path>, PAGEdgeEnum);

pub struct FuncPAG {
    pub(crate) func_id: FuncId,
    pub(crate) internal_edges: ChunkedQueue<InternalEdge>,
    pub(crate) static_variables_involved: HashSet<Rc<Path>>,

    // Call sites that can be statically resolved, including the Fn* trait calls that can 
    // be directly resolved. 
    pub(crate) static_dispatch_callsites: Vec<(Rc<CallSite>, FuncId)>,
    // Special calls like alloc() 
    pub(crate) special_callsites: Vec<(Rc<CallSite>, FuncId)>,
    // Pairs of the dynamic type receiver and its corresponding callsite.
    pub(crate) dynamic_dispatch_callsites: Vec<(Rc<Path>, Rc<CallSite>)>,
    // Pairs of the first argument of Fn::call, FnMut::call_mut, FnOnce::call_once call
    // and the callsite that need to be resolved on-the-fly
    pub(crate) dynamic_fntrait_callsites: Vec<(Rc<Path>, Rc<CallSite>)>,
    // Pairs of the function pointer and its corresponding callsite, including the fnptr
    // callsites that are speciallized from a Fn* trait callsite.
    pub(crate) fnptr_callsites: Vec<(Rc<Path>, Rc<CallSite>)>,
}

impl FuncPAG {
    pub fn new(func_id: FuncId) -> Self {
        FuncPAG {
            func_id,
            internal_edges: ChunkedQueue::new(),
            static_variables_involved: HashSet::new(),
            static_dispatch_callsites: Vec::new(),
            special_callsites: Vec::new(),
            dynamic_fntrait_callsites: Vec::new(),
            dynamic_dispatch_callsites: Vec::new(),
            fnptr_callsites: Vec::new(),
        }
    }

    pub fn add_internal_edge(&mut self, src: Rc<Path>, dst: Rc<Path>, kind: PAGEdgeEnum) {
        self.internal_edges.push((src, dst, kind));
    }

    pub fn internal_edges_iter(&self) -> chunked_queue::Iter<InternalEdge> {
        self.internal_edges.iter()
    }

    pub fn add_static_variables_involved(&mut self, static_variable: Rc<Path>) {
        self.static_variables_involved.insert(static_variable);
    }

    pub fn add_static_dispatch_callsite(&mut self, callsite: Rc<CallSite>, callee: FuncId) {
        self.static_dispatch_callsites.push((callsite, callee));
    }

    pub fn add_dynamic_fntrait_callsite(
        &mut self,
        dynamic_fn_obj: Rc<Path>,
        std_ops_callsite: Rc<CallSite>,
    ) {
        self.dynamic_fntrait_callsites.push((dynamic_fn_obj, std_ops_callsite));
    }

    pub fn add_dynamic_dispatch_callsite(
        &mut self,
        dynamic_obj: Rc<Path>,
        dyn_callsite: Rc<CallSite>,
    ) {
        self.dynamic_dispatch_callsites.push((dynamic_obj, dyn_callsite));
    }

    pub fn add_fnptr_callsite(&mut self, fn_ptr: Rc<Path>, callsite: Rc<CallSite>) {
        self.fnptr_callsites.push((fn_ptr, callsite));
    }

    pub fn add_special_callsite(&mut self, callsite: Rc<CallSite>, callee: FuncId) {
        self.special_callsites.push((callsite, callee));
    }
}
