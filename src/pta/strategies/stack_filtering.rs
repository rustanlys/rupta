use std::collections::{HashMap, VecDeque};
use std::io::{BufWriter, Write};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use rustc_hir::def::DefKind;

use crate::graph::call_graph::CallGraph;
use crate::mir::call_site::BaseCallSite;
use crate::mir::function::CSFuncId;
use crate::util::bit_vec::BitVec;
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::path::PathEnum;
use crate::graph::pag::PAGPath;
use crate::pta::{FuncId, PointsTo, EdgeId, NodeId};
use crate::pts_set::points_to::PointsToSet;

use super::context_strategy::{KCallSiteSensitive, ContextStrategy};


pub trait RowRelation: std::fmt::Debug + std::clone::Clone + std::marker::Send {
    fn new_empty() -> Self;
    fn with_capacity(capacity: usize) -> Self;
    fn contains(&self, elem: usize) -> bool;
    fn insert(&mut self, elem: usize);
    fn count(&self) -> usize;
}

impl RowRelation for Vec<bool> {
    #[inline]
    fn new_empty() -> Self {
        Vec::new()
    }

    #[inline]
    fn with_capacity(capacity: usize) -> Self {
        vec![false; capacity]
    }

    #[inline]
    fn contains(&self, elem: usize) -> bool {
        self[elem]
    }

    #[inline]
    fn insert(&mut self, elem: usize) {
        self[elem] = true;
    }

    #[inline]
    fn count(&self) -> usize {
        self.iter().filter(|&n| *n).count()
    }
}

impl RowRelation for BitVec<usize> {
    #[inline]
    fn new_empty() -> Self {
        BitVec::new_empty()
    }

    #[inline]
    fn with_capacity(capacity: usize) -> Self {
        BitVec::with_capacity(capacity)
    }

    #[inline]
    fn contains(&self, elem: usize) -> bool {
        self.contains(elem)
    }

    #[inline]
    fn insert(&mut self, elem: usize) {
        self.insert(elem);
    }

    #[inline]
    fn count(&self) -> usize {
        self.count()
    }
}

#[derive(Debug)]
pub struct ReachabilityRelation<T: RowRelation> {
    pub num_nodes: usize,
    pub relation_matrix: Vec<T>,
}

impl<T: RowRelation> ReachabilityRelation<T> {
    pub fn new(num_nodes: usize) -> Self {
        let mut relation_matrix = Vec::with_capacity(num_nodes);
        for _i in 0..num_nodes {
            relation_matrix.push(T::with_capacity(num_nodes));
        }
        Self {
            num_nodes,
            relation_matrix, 
        }
    }

    #[inline]
    pub fn is_reachable(&self, from: usize, to: usize) -> bool {
        if from >= self.num_nodes || to >= self.num_nodes {
            false
        } else {
            self.relation_matrix[from].contains(to)
        }
    }

    #[inline]
    pub fn add_reachable(&mut self, from: usize, to: usize) {
        if from >= self.num_nodes || to >= self.num_nodes {
            panic!("Set reachable relation for nodes out of index");
        } else {
            self.relation_matrix[from].insert(to);
        }
    }

    pub fn num_reach_relations(&self) -> usize {
        let mut num = 0;
        for reachable_nodes in &self.relation_matrix {
            num += reachable_nodes.count();
        }
        num
    }
}

pub struct FunctionReachabilityAnalysis;

impl FunctionReachabilityAnalysis {
    pub fn compute_func_reach_relations<T: RowRelation>(
        call_graph: &CallGraph<FuncId, BaseCallSite>
    ) -> ReachabilityRelation<T> {
        let mut reach_relation = ReachabilityRelation::new(call_graph.graph.node_count());
        for func in call_graph.reach_funcs_iter() {
            let func_node_id = *call_graph.func_nodes.get(&func).unwrap();
            Self::compute_func_reach_relations_(func_node_id, &call_graph, &mut reach_relation);
        }
        reach_relation
    }

    fn compute_func_reach_relations_<T: RowRelation>(
        func: NodeId, 
        call_graph: &CallGraph<FuncId, BaseCallSite>, 
        reach_relation: &mut ReachabilityRelation<T>
    ) {
        let mut worklist = VecDeque::new();
        for neighbor in call_graph.graph.neighbors(func) {
            worklist.push_back(neighbor);
        }
        while !worklist.is_empty() {
            let reach_node = worklist.pop_back().unwrap();
            if !reach_relation.is_reachable(func.index(), reach_node.index()) {
                reach_relation.add_reachable(func.index(), reach_node.index());
                for neighbor_node in call_graph.graph.neighbors(reach_node) {
                    worklist.push_back(neighbor_node);
                }
            }
        }
    }

    pub fn compute_func_reach_relations_mt<T: RowRelation>(
        call_graph: &CallGraph<FuncId, BaseCallSite>
    ) -> ReachabilityRelation<T> {
        let node_count = call_graph.graph.node_count();
        let relation_matrix = Arc::new(Mutex::new(vec![T::new_empty(); node_count]));
        
        let graph = &call_graph.graph;
        (0..node_count).into_par_iter().for_each(|node_index| {
            let mut reachable_funcs = T::with_capacity(node_count);
            let mut worklist = VecDeque::new();
            for neighbor in graph.neighbors(NodeId::new(node_index)) {
                worklist.push_back(neighbor);
            }
            while !worklist.is_empty() {
                let reach_node = worklist.pop_back().unwrap();
                if !reachable_funcs.contains(reach_node.index()) {
                    reachable_funcs.insert(reach_node.index());
                    for neighbor_node in graph.neighbors(reach_node) {
                        worklist.push_back(neighbor_node);
                    }
                }
            }
            let mut matrix_lock = relation_matrix.lock().unwrap();
            matrix_lock[node_index] = reachable_funcs;
        });
        
        let mutex = Arc::try_unwrap(relation_matrix).unwrap();
        let reach_relation = ReachabilityRelation {
            num_nodes: node_count,
            relation_matrix: mutex.into_inner().unwrap()
        };
        reach_relation
    }

}

pub struct StackFilter<F> {
    pub(crate) call_graph: CallGraph<FuncId, BaseCallSite>,
    pub(crate) reach_relation: ReachabilityRelation<BitVec<usize>>,
    pub(crate) pag_edge_to_func: HashMap<EdgeId, F>,
    pub(crate) with_kcs_context: bool,
    pub(crate) kcs_context_strategy: NonNull<KCallSiteSensitive>,
    pub(crate) collect_filtered_pts: bool,
    pub(crate) filtered_pts: HashMap<EdgeId, PointsTo<NodeId>>,
    // Function reachability analysis time
    pub(crate) fra_time: Duration,
}

impl<F> StackFilter<F> where 
    F: Copy + Into<FuncId> + std::cmp::Eq + std::hash::Hash,
{
    pub fn new(call_graph: CallGraph<FuncId, BaseCallSite>) -> Self {
        let now = Instant::now();
        let reach_relation = 
            FunctionReachabilityAnalysis::compute_func_reach_relations_mt(&call_graph);
        let fra_time = now.elapsed();
        StackFilter {
            call_graph,
            reach_relation,
            pag_edge_to_func: HashMap::new(),
            with_kcs_context: false,
            kcs_context_strategy: NonNull::dangling(),
            collect_filtered_pts: false,
            filtered_pts: HashMap::new(),
            fra_time,
        }
    }

    pub fn with_kcs_context_strategy(&mut self, kcs_context_strategy: &mut KCallSiteSensitive) {
        self.with_kcs_context = true;
        // This is safe because we will not mutate kcs_context_strategy.
        unsafe {
            self.kcs_context_strategy = NonNull::new_unchecked(
                kcs_context_strategy as *mut KCallSiteSensitive
            )
        };
    }

    pub fn get_filtered_pts_of_edge_mut(&mut self, edge_id: EdgeId) -> &mut PointsTo<NodeId> {
        self.filtered_pts.entry(edge_id).or_insert(PointsTo::new())
    }

    pub fn add_filtered_pts(&mut self, edge_id: EdgeId, filtered_pointee: NodeId) {
        self.filtered_pts.entry(edge_id).or_insert(PointsTo::new()).insert(filtered_pointee);
    }

    pub fn add_pag_edge_in_func(&mut self, edge_id: EdgeId, func: F) {
        self.pag_edge_to_func.insert(edge_id, func);
    }

    pub fn add_pag_edges_in_func(&mut self, edges: Vec<EdgeId>, func: F) {
        edges.iter()
            .for_each(|edge| self.add_pag_edge_in_func(*edge, func));
    }

    pub fn get_container_func_of_edge(&self, edge_id: &EdgeId) -> Option<&F> {
        self.pag_edge_to_func.get(edge_id)
    }

    pub fn collect_filtered_pts(&mut self, edge_id: EdgeId, filtered_pointee: NodeId) {
        if self.collect_filtered_pts {
            self.add_filtered_pts(edge_id, filtered_pointee);
        }
    }

    pub fn fra_time(&self) -> Duration {
        self.fra_time
    }

}

impl<F> StackFilter<F> where 
    F: Copy + Into<FuncId> + std::cmp::Eq + std::hash::Hash + SFReachable,
{
    pub fn is_potentially_alive<P: PAGPath<FuncTy = F>>(
        &self, 
        acx: &AnalysisContext, 
        current_func: F, 
        target_path: &P
    ) -> bool {
        match target_path.value() {
            PathEnum::HeapObj { .. } => { return true; }
            PathEnum::QualifiedPath { base, .. }
            | PathEnum::OffsetPath { base, .. } => {
                if matches!(base.value, PathEnum::HeapObj { .. }) {
                    return true;
                }
            }
            _ => {}
        }

        // A static variable does not have a container function, 
        // therefore a static object.
        if let Some(path_container_func) = target_path.get_containing_func() {
            if path_container_func == current_func {
                return true;
            }
            let func_ref = acx.get_function_reference(path_container_func.into());
            if func_ref.promoted.is_some() {
                return true;
            }
            if matches!(acx.tcx.def_kind(func_ref.def_id), DefKind::Const) {
                return true;
            }

            return current_func.is_reachable_from(&path_container_func, self);
        } 
        return true;
    }

    pub fn naive_reachability_relation(&self, from: F, to: F) -> bool {
        let from_id = self.call_graph.func_nodes.get(&from.into()).unwrap();
        let to_id = self.call_graph.func_nodes.get(&to.into()).unwrap();
        if from_id == to_id {
            true
        } else {
            if self.reach_relation.is_reachable(from_id.index(), to_id.index()) {
                true
            } else {
                false
            }
        }
    }
}


impl<F> StackFilter<F> {
    pub fn dump_stack_filter_stat<W: Write>(&self, stat_writer: &mut BufWriter<W>) {
        if self.collect_filtered_pts {
            let mut num_filtered_relations = 0;
            for (_edge_id, filtered_pts) in &self.filtered_pts {
                num_filtered_relations += filtered_pts.count();
            }
            stat_writer
                .write_all(format!("#Directly filtered points-to relations: {}\n", 
                    num_filtered_relations).as_bytes()
                )
                .expect("Unable to write data");
        }
    }
}

pub trait SFReachable where Self: Sized {
    fn is_reachable_from(&self, from: &Self, stack_filter: &StackFilter<Self>) -> bool;

    fn is_reachable_to(&self, to: &Self, stack_filter: &StackFilter<Self>) -> bool {
        to.is_reachable_from(self, stack_filter)
    }
}

impl SFReachable for FuncId {
    fn is_reachable_from(&self, from: &FuncId, stack_filter: &StackFilter<FuncId>) -> bool {
        stack_filter.naive_reachability_relation(*from, *self)
    }
}

impl SFReachable for CSFuncId {
    fn is_reachable_from(&self, from: &CSFuncId, stack_filter: &StackFilter<CSFuncId>) -> bool {
        if stack_filter.with_kcs_context {
            // To determine whether a call-site sensitive function f1: <f, [c1, c2, ..., cn]> 
            // can be reachable from f2: <f', [c1', c2', ..., cn']>, 
            // we perform the following two steps:
            // 1) We construct the function call chain for the two functions, 
            //    like f <-- f_c1 <-- f_c2 <-- ... <-- f_cn and f' <-- f_c1' <-- f_c2' <-- ... <-- f_cn' 
            //    (where f_cn represents the container function of the call site cn). 
            //    f1 is reachable from f2 if there is an overlapping between f1's call chain's suffix  
            //    and f2's call chain's prefix.
            // 2) If the first condition does not hold, we further check if f_cn is reachable from f'.
            let (from_ctxt, to_ctxt) = unsafe { 
                (stack_filter.kcs_context_strategy.as_ref().get_context_by_id(from.cid),
                stack_filter.kcs_context_strategy.as_ref().get_context_by_id(self.cid))
            };
            if from_ctxt.len() > 0 {
                let mut from_call_chain: Vec<FuncId> = Vec::with_capacity(from_ctxt.len() + 1);
                from_call_chain.push((*from).into());
                from_ctxt.context_elems.iter().for_each(|callsite| from_call_chain.push(callsite.func));

                let mut to_call_chain: Vec<FuncId> = Vec::with_capacity(to_ctxt.len() + 1);
                to_call_chain.push((*self).into());
                to_ctxt.context_elems.iter().for_each(|callsite| to_call_chain.push(callsite.func));

                if match_suffix_and_prefix(&to_call_chain, &from_call_chain) {
                    return true;
                }
            }

            let from_id = stack_filter.call_graph.func_nodes.get(&(*from).into()).unwrap();
            
            let to_id =  if let Some(last_callsite) = to_ctxt.last_context_element() {
                stack_filter.call_graph.func_nodes.get(&last_callsite.func).unwrap()
            } else {
                stack_filter.call_graph.func_nodes.get(&(*self).into()).unwrap()
            };
            if from_id == to_id {
                true
            } else {
                if stack_filter.reach_relation.is_reachable(from_id.index(), to_id.index()) {
                    true
                } else {
                    false
                }
            }
        } else {
            stack_filter.naive_reachability_relation((*from).into(), *self)
        } 
    }
}

fn match_suffix_and_prefix<E: std::cmp::PartialEq>(v1: &Vec<E>, v2: &Vec<E>) -> bool {
    let len1 = v1.len();
    let len2 = v2.len();
    for i in 1..std::cmp::min(len1, len2)+1 {
        if v1.as_slice()[len1 - i..].iter().zip(v2).all(|(e1, e2)| e1 == e2) {
            return true;
        }
    }
    return false;
}
