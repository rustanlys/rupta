// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use petgraph::graph::{DefaultIx, EdgeIndex, NodeIndex};
use petgraph::Graph;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Debug};
use std::hash::Hash;

use crate::mir::analysis_context::AnalysisContext;
use crate::mir::call_site::{BaseCallSite, CallType, CSBaseCallSite};
use crate::mir::function::{FuncId, CSFuncId};
use crate::util::chunked_queue::{self, ChunkedQueue};
use crate::util::dot::Dot;

/// Unique identifiers for call graph nodes.
pub type CGNodeId = NodeIndex<DefaultIx>;
/// Unique identifiers for call graph edges.
pub type CGEdgeId = EdgeIndex<DefaultIx>;
// Context-sensitive call graph.
pub type CSCallGraph = CallGraph<CSFuncId, CSBaseCallSite>;


pub trait CGFunction: Copy + Clone + PartialEq + Eq + Hash + Debug {
    fn dot_fmt(&self, acx: &AnalysisContext, f: &mut fmt::Formatter) -> fmt::Result;
}

impl CGFunction for FuncId {
    fn dot_fmt(&self, acx: &AnalysisContext, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!(
            "{}",
            acx.get_function_reference(*self).to_string()
        ))
    }
}

impl CGFunction for CSFuncId {
    fn dot_fmt(&self, acx: &AnalysisContext, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!(
            "{}",
            acx.get_function_reference(self.func_id).to_string(),
        ))
    }
}

pub trait CGCallSite: Copy + Clone + PartialEq + Eq + Hash + Debug {
    fn dot_fmt(&self, f: &mut fmt::Formatter) -> fmt::Result;
}

impl CGCallSite for BaseCallSite {
    fn dot_fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{:?}", self.location))
    }
}

impl CGCallSite for CSBaseCallSite {
    fn dot_fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{:?}", self.location))
    }
}

#[derive(Debug)]
pub struct CallGraphNode<F: CGFunction> {
    pub(crate) func: F,
}

impl<F: CGFunction> CallGraphNode<F> {
    pub fn new(func: F) -> Self {
        CallGraphNode { func }
    }
}

#[derive(Debug)]
pub struct CallGraphEdge<S: CGCallSite> {
    pub(crate) callsite: S,
}

impl<S: CGCallSite> CallGraphEdge<S> {
    pub fn new(callsite: S) -> Self {
        CallGraphEdge { callsite }
    }
}

pub struct CallGraph<F: CGFunction, S: CGCallSite> {
    /// The graph structure capturing call relationships.
    pub graph: Graph<CallGraphNode<F>, CallGraphEdge<S>>,
    /// A map from functions to their corresponding call graph nodes.
    pub func_nodes: HashMap<F, CGNodeId>,
    /// A map from call sites to call graph edges.
    pub callsite_to_edges: HashMap<S, HashSet<CGEdgeId>>,
    /// Record the type of each call.
    pub(crate) callsite_to_type: HashMap<BaseCallSite, CallType>,
    /// A queue of reachable ndoes.
    pub(crate) reach_funcs: ChunkedQueue<F>,
}

impl<F: CGFunction, S: CGCallSite> CallGraph<F, S> {
    pub fn new() -> Self {
        CallGraph {
            graph: Graph::<CallGraphNode<F>, CallGraphEdge<S>>::new(),
            func_nodes: HashMap::new(),
            callsite_to_edges: HashMap::new(),
            callsite_to_type: HashMap::new(),
            reach_funcs: ChunkedQueue::new(),
        }
    }

    /// Add a new node to the call graph.
    pub fn add_node(&mut self, func: F) {
        if let Entry::Vacant(e) = self.func_nodes.entry(func) {
            let node = CallGraphNode::new(func);
            let node_id = self.graph.add_node(node);
            e.insert(node_id);
            self.add_reach_func(func);
        }
    }

    /// Helper function to get a node or insert a new
    /// node if it does not exist in the map.
    fn get_or_insert_node(&mut self, func: F) -> CGNodeId {
        match self.func_nodes.entry(func) {
            Entry::Occupied(o) => o.get().to_owned(),
            Entry::Vacant(v) => {
                // Add the def_id the the reachble functions queue.
                self.reach_funcs.push(func);
                let node_id = self.graph.add_node(CallGraphNode::new(func));
                *v.insert(node_id)
            }
        }
    }

    pub fn set_callsite_type(&mut self, callsite: BaseCallSite, call_type: CallType) {
        self.callsite_to_type.insert(callsite, call_type);
    }

    pub fn get_callsite_type(&self, callsite: &BaseCallSite) -> Option<&CallType> {
        self.callsite_to_type.get(&callsite)
    }

    pub fn get_callee_id_of_edge(&self, edge_id: EdgeIndex) -> Option<F> {
        if let Some((_, callee_node)) = self.edge_endpoints(edge_id) {
            if let Some(node) = self.graph.node_weight(callee_node) {
                return Some(node.func);
            }
            return None;
        }
        return None;
    }

    /// 给定一条边的EdgeIndex，以`(CGNodeId, CGNodeId)`的形式返回这条边的起点和终点。
    pub fn edge_endpoints(&self, edge_id: EdgeIndex) -> Option<(CGNodeId, CGNodeId)> {
        self.graph.edge_endpoints(edge_id)
    }

    /// 从一个Callsite中获取其调用的所有函数，即所有callee
    pub fn get_callees(&self, callsite: &S) -> HashSet<F> {
        if let Some(edges) = self.callsite_to_edges.get(callsite) {
            edges
                .iter()
                .filter_map(|edge_id| match self.graph.edge_endpoints(*edge_id) {
                    Some((_, target)) => Some(self.graph.node_weight(target).unwrap().func),
                    None => None,
                })
                .collect::<HashSet<F>>()
        } else {
            HashSet::new()
        }
    }

    /// Returns true if an edge to the callee already existed for the callsite.
    pub fn has_edge(&self, callsite: &S, callee_id: F) -> bool {
        let callees = self.get_callees(callsite);
        callees.contains(&callee_id)
    }

    /// Adds a new edge to the call graph.
    /// The edge is a call from `caller_id` to `callee_id` at `callsite`.
    /// Returns false if the edge already existed, and true otherwise.
    pub fn add_edge(&mut self, callsite: S, caller_id: F, callee_id: F) -> bool {
        // 既然要加边，那么肯定是要先保证边的端点确实在图中
        let caller_node = self.get_or_insert_node(caller_id);
        let callee_node = self.get_or_insert_node(callee_id);

        // 如果调用图中没有这条边，才会增加这条边，
        // 否则啥也不干并返回false
        let callees = self.get_callees(&callsite);
        if !callees.contains(&callee_id) {
            //? 新建边啦！！
            let edge = CallGraphEdge::new(callsite);
            //? 通过这里可以看出来，是先有边CallGraphEdge，然后才申请的边编号EdgeIdx
            let edge_id = self.graph.add_edge(caller_node, callee_node, edge);
            self.callsite_to_edges
                .entry(callsite)
                .or_default()
                .insert(edge_id);
            true
        } else {
            false
        }
    }

    /// Add the def_id into the reachable functions queue.
    pub fn add_reach_func(&mut self, func: F) {
        self.reach_funcs.push(func);
    }

    /// Return a iterator for the reachable functions.
    pub fn reach_funcs_iter(&self) -> chunked_queue::IterCopied<F> {
        self.reach_funcs.iter_copied()
    }

    /// Produce a dot file representation of the call graph
    /// for displaying with Graphviz.
    pub fn to_dot(&self, acx: &AnalysisContext, dot_path: &std::path::Path) {
        let node_fmt = |node: &CallGraphNode<F>, f: &mut fmt::Formatter| -> fmt::Result {
            node.func.dot_fmt(acx, f)
        };
        let edge_fmt = |edge: &CallGraphEdge<S>, f: &mut fmt::Formatter| -> fmt::Result {
            edge.callsite.dot_fmt(f)
        };

        let output = format!(
            "{:?}",
            Dot::with_graph_fmt(&self.graph, &[], &node_fmt, &edge_fmt)
        );
        match std::fs::write(dot_path, output) {
            Ok(_) => (),
            Err(e) => panic!("Failed to write dot file output: {:?}", e),
        };
    }
}
