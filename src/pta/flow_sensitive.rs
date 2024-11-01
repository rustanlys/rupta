use std::collections::HashSet;
use std::fmt::{Debug, Formatter, Result};
use std::rc::Rc;
use std::time::Instant;

use flow_strategy::FlowStrategy;
use itertools::Itertools;
use log::*;
use rustc_middle::ty::TyCtxt;


use super::propagator::propagator::Propagator;
use super::PointerAnalysis;
use crate::graph::func_pag::FuncPAG;
use crate::graph::pag::*;
use crate::graph::call_graph::CSCallGraph;
use crate::mir::call_site::{AssocCallGroup, CSCallSite, CallSite, CallType};
use crate::mir::context::{Context, ContextId};
use crate::mir::function::{FuncId, CSFuncId};
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::path::{Path, CSPath, PathEnum};
use crate::pta::*;
use crate::util::{self, chunked_queue, results_dumper};


pub struct FlowSensitvePTA<'pta, 'tcx, 'compilation, S: FlowStrategy> {
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
}

