// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Write};

use rustc_hir::def_id::DefId;

use crate::graph::call_graph::CallGraph;
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::call_site::{BaseCallSite, CSBaseCallSite, CallType};
use crate::mir::function::{CSFuncId, FuncId};

pub fn ci_call_graph_stat<W: Write>(
    acx: &AnalysisContext,
    call_graph: &CallGraph<FuncId, BaseCallSite>,
    stat_writer: &mut BufWriter<W>,
) {
    let num_reach_funcs = call_graph.reach_funcs.len();
    let num_call_graph_edges = call_graph.graph.edge_count();
    // statically resolved calls
    let mut num_statically_resolved_calls = 0;
    // dynamically resolved calls
    let mut num_dynmically_resolved_calls = 0;
    let mut num_dynmically_resolved_call_edges = 0;
    let mut num_dynamic_dispatch_calls = 0;
    let mut num_dynamic_dispatch_call_edges = 0;
    let mut num_fnptr_calls = 0;
    let mut num_fnptr_call_edges = 0;
    let mut num_dynamic_fntrait_calls = 0;
    let mut num_dynamic_fntrait_call_edges = 0;

    // Count reachable functions with distinct defid
    let mut reach_funcs_defids: HashSet<DefId> = HashSet::new();
    for func_id in call_graph.reach_funcs.iter() {
        let func_ref = acx.get_function_reference(*func_id);
        reach_funcs_defids.insert(func_ref.def_id);
    }
    let num_reach_funcs_defids = reach_funcs_defids.len();
    let avg_substs = num_reach_funcs as f32 / num_reach_funcs_defids as f32;

    // We create different callsites for a dynamic Fn* trait callsite since the new callsites will have
    // different arguments. Therefore we count all the callsites representing for the same dyn_fn_trait_call
    // as one callsite.
    let mut dynamic_fntrait_calls: HashSet<BaseCallSite> = HashSet::new();
    let mut resolved_calls: HashSet<BaseCallSite> = HashSet::new();

    for (callsite, call_edges) in &call_graph.callsite_to_edges {
        let callsite_type = call_graph.get_callsite_type(callsite).unwrap();
        resolved_calls.insert(*callsite);
        match callsite_type {
            CallType::StaticDispatch => {
                num_statically_resolved_calls += 1;
            }
            CallType::DynamicDispatch => {
                num_dynamic_dispatch_calls += 1;
                num_dynmically_resolved_calls += 1;
                num_dynamic_dispatch_call_edges += call_edges.len();
                num_dynmically_resolved_call_edges += call_edges.len();
            }
            CallType::FnPtr => {
                num_fnptr_calls += 1;
                num_dynmically_resolved_calls += 1;
                num_fnptr_call_edges += call_edges.len();
                num_dynmically_resolved_call_edges += call_edges.len();
            }
            CallType::DynamicFnTrait => {
                if dynamic_fntrait_calls.insert(*callsite) {
                    num_dynamic_fntrait_calls += 1;
                    num_dynmically_resolved_calls += 1;
                }
                num_dynamic_fntrait_call_edges += call_edges.len();
                num_dynmically_resolved_call_edges += call_edges.len();
            }
        }
    }

    stat_writer
        .write_all("Call Graph Statistics: \n".as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(format!("#Reachable functions: {}\n", num_reach_funcs).as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(
            format!(
                "#Reachable unmonomorphized functions: {}\n",
                num_reach_funcs_defids
            )
            .as_bytes(),
        )
        .expect("Unable to write data");
    stat_writer
        .write_all(format!("#Avg substs: {}\n", avg_substs).as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(format!("#Call graph edges: {}\n", num_call_graph_edges).as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(format!("#Statically resolved calls: {}\n", num_statically_resolved_calls).as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(
            format!(
                "#Dynamically resolved calls: {}, #call graph edges: {}\n",
                num_dynmically_resolved_calls, num_dynmically_resolved_call_edges
            )
            .as_bytes(),
        )
        .expect("Unable to write data");
    stat_writer
        .write_all(
            format!(
                "\t#Dynamic dispatch calls: {}, #call graph edges: {}\n",
                num_dynamic_dispatch_calls, num_dynamic_dispatch_call_edges
            )
            .as_bytes(),
        )
        .expect("Unable to write data");
    stat_writer
        .write_all(
            format!(
                "\t#Fnptr calls: {}, #call graph edges: {}\n",
                num_fnptr_calls, num_fnptr_call_edges
            )
            .as_bytes(),
        )
        .expect("Unable to write data");
    stat_writer
        .write_all(
            format!(
                "\t#Dynamic Fn* trait calls: {}, #call graph edges: {}\n",
                num_dynamic_fntrait_calls, num_dynamic_fntrait_call_edges
            )
            .as_bytes(),
        )
        .expect("Unable to write data");
}

pub fn cs_call_graph_stat<W: Write>(
    acx: &AnalysisContext,
    call_graph: &CallGraph<CSFuncId, CSBaseCallSite>,
    stat_writer: &mut BufWriter<W>,
) {
    let num_cs_reach_funcs = call_graph.reach_funcs.len();
    let num_cs_call_graph_edges = call_graph.graph.edge_count();
    // statically resolved calls
    let mut num_statically_resolved_calls = 0;
    // dynamically resolved calls
    let mut num_dynmically_resolved_calls = 0;
    let mut num_dynmically_resolved_call_edges = 0;
    let mut num_dynamic_dispatch_calls = 0;
    let mut num_dynamic_dispatch_call_edges = 0;
    let mut num_fnptr_calls = 0;
    let mut num_fnptr_call_edges = 0;
    let mut num_dynamic_fntrait_calls = 0;
    let mut num_dynamic_fntrait_call_edges = 0;

    // Count reachable functions with distinct defid
    let mut ci_reach_funcs: HashSet<FuncId> = HashSet::new();
    // let mut reach_funcs_defids: HashSet<DefId> = HashSet::new();
    for func in call_graph.reach_funcs.iter() {
        let ci_func_id = func.func_id;
        ci_reach_funcs.insert(ci_func_id);
        // let func_ref = acx.get_function_reference(ci_func_id);
        // reach_funcs_defids.insert(func_ref.def_id);
    }
    // let num_reach_funcs_defids = reach_funcs_defids.len();
    let num_reach_funcs_defids = acx.overall_metadata.func_metadata.len();

    let num_ci_reach_funcs = ci_reach_funcs.len();

    let mut ci_call_edges: HashMap<BaseCallSite, HashSet<FuncId>> = HashMap::new();
    for (callsite, call_edges) in &call_graph.callsite_to_edges {
        let ci_callsite = callsite.into();
        let callees = ci_call_edges.entry(ci_callsite).or_default();
        for edge_id in call_edges {
            let callee_id = call_graph.get_callee_id_of_edge(*edge_id).unwrap();
            let ci_callee = callee_id.func_id;
            callees.insert(ci_callee);
        }
    }

    let mut num_ci_call_graph_edges = 0;
    // We may create multiple callsites for a dynamic Fn* trait callsite since the new callsites may have
    // different arguments. We treat all the callsites created from the same dynamic Fn* trait callsite
    // as one callsite.
    let mut dynamic_fntrait_calls: HashSet<BaseCallSite> = HashSet::new();
    for (callsite, callees) in &ci_call_edges {
        num_ci_call_graph_edges += callees.len();
        let callsite_type = call_graph.get_callsite_type(callsite).unwrap();
        match callsite_type {
            CallType::StaticDispatch => {
                num_statically_resolved_calls += 1;
            }
            CallType::DynamicDispatch => {
                num_dynamic_dispatch_calls += 1;
                num_dynmically_resolved_calls += 1;
                num_dynamic_dispatch_call_edges += callees.len();
                num_dynmically_resolved_call_edges += callees.len();
            }
            CallType::FnPtr => {
                num_fnptr_calls += 1;
                num_dynmically_resolved_calls += 1;
                num_fnptr_call_edges += callees.len();
                num_dynmically_resolved_call_edges += callees.len();
            }
            CallType::DynamicFnTrait => {
                if dynamic_fntrait_calls.insert(*callsite) {
                    num_dynamic_fntrait_calls += 1;
                    num_dynmically_resolved_calls += 1;
                }
                num_dynamic_fntrait_call_edges += callees.len();
                num_dynmically_resolved_call_edges += callees.len();
            }
        }
    }

    stat_writer
        .write_all("Call Graph Statistics: \n".as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(format!("#Reachable functions (CS): {}\n", num_cs_reach_funcs).as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(format!("#Reachable functions (CI): {}\n", num_ci_reach_funcs).as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(
            format!(
                "#Reachable unmonomorphized functions (CI): {}\n",
                num_reach_funcs_defids
            )
            .as_bytes(),
        )
        .expect("Unable to write data");
    stat_writer
        .write_all(format!("#Call graph edges (CS): {}\n", num_cs_call_graph_edges).as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(format!("#Call graph edges (CI): {}\n", num_ci_call_graph_edges).as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(format!("#Statically resolved calls: {}\n", num_statically_resolved_calls).as_bytes())
        .expect("Unable to write data");
    stat_writer
        .write_all(
            format!(
                "#Dynamically resolved calls: {}, #call graph edges: {}\n",
                num_dynmically_resolved_calls, num_dynmically_resolved_call_edges
            )
            .as_bytes(),
        )
        .expect("Unable to write data");
    stat_writer
        .write_all(
            format!(
                "\t#Dynamic dispatch calls: {}, #call graph edges: {}\n",
                num_dynamic_dispatch_calls, num_dynamic_dispatch_call_edges
            )
            .as_bytes(),
        )
        .expect("Unable to write data");
    stat_writer
        .write_all(
            format!(
                "\t#Fnptr calls: {}, #call graph edges: {}\n",
                num_fnptr_calls, num_fnptr_call_edges
            )
            .as_bytes(),
        )
        .expect("Unable to write data");
    stat_writer
        .write_all(
            format!(
                "\t#Dynamic Fn* trait calls: {}, #call graph edges: {}\n",
                num_dynamic_fntrait_calls, num_dynamic_fntrait_call_edges
            )
            .as_bytes(),
        )
        .expect("Unable to write data");
}
