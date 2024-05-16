// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use log::*;
use petgraph::visit::EdgeRef;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::rc::Rc;

use crate::graph::pag::{PAGNodeId, PAG, PAGPath};
use crate::graph::call_graph::{CallGraph, CGFunction, CGCallSite, CSCallGraph};
use crate::mir::call_site::{BaseCallSite, CallType};
use crate::mir::context::{Context, ContextId};
use crate::mir::function::FuncId;
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::path::PathEnum;
use crate::pta::DiffPTDataTy;
use crate::pta::context_strategy::ContextStrategy;
use crate::pts_set::points_to::PointsToSet;
use crate::util;

pub fn dump_results<P: PAGPath, F, S>(
    acx: &AnalysisContext, 
    call_graph: &CallGraph<F, S>, 
    pt_data: &DiffPTDataTy, 
    pag: &PAG<P>, 
) where
    F: CGFunction + Into<FuncId>,
    S: CGCallSite + Into<BaseCallSite>,
    <P as PAGPath>::FuncTy: Ord + std::fmt::Debug + Into<FuncId> + Copy
{
    // dump points-to results
    if let Some(pts_output) = &acx.analysis_options.pts_output {
        info!("Dumping points-to results...");
        dump_ci_pts(acx, pt_data, pag, pts_output);
        // dump_pts(pt_data, pag, pts_output);
    }

    // dump call graph
    if let Some(cg_output) = &acx.analysis_options.call_graph_output {
        let cg_path = std::path::Path::new(cg_output);
        info!("Dumping call graph...");
        dump_call_graph(acx, call_graph, cg_path);
    }

    // dump mir for reachable functions
    if let Some(mir_output) = &acx.analysis_options.mir_output {
        info!("Dumping functions' mir...");
        dump_mir(acx, call_graph, mir_output);
    }

    // dump type indices
    // Note: the type indices map is not used to store all the types.
    if let Some(ti_output) = &acx.analysis_options.type_indices_output {
        let ti_path = std::path::Path::new(ti_output);
        info!("Dumping type indices...");
        dump_type_index(acx, ti_path);
    }

    // dump dynamically resolved calls
    if let Some(dyn_calls_output) = &acx.analysis_options.dyn_calls_output {
        info!("Dumping dynamically resolved calls...");
        dump_dyn_calls(acx, call_graph, dyn_calls_output);
    }
}


pub fn dump_call_graph<F, S>(
    acx: &AnalysisContext, 
    call_graph: &CallGraph<F, S>, 
    dot_path: &std::path::Path
) where 
    F: CGFunction + Into<FuncId>,
    S: CGCallSite + Into<BaseCallSite>,
{
    let ci_call_graph = to_ci_call_graph(call_graph);
    ci_call_graph.to_dot(acx, dot_path);
}

pub fn dump_type_index(acx: &AnalysisContext, index_path: &std::path::Path) {
    let mut output = String::new();
    for (i, ty) in acx.type_cache.type_list().iter().enumerate() {
        output.push_str(&format!("{}: {:?}\n", i, ty));
    }
    match std::fs::write(index_path, output) {
        Ok(_) => (),
        Err(e) => panic!("Failed to write index file: {:?}", e),
    };
}

pub fn dump_pts<P: PAGPath>(pt_data: &DiffPTDataTy, pag: &PAG<P>, pts_path: &String) {
    let pts_map = &pt_data.propa_pts_map;
    let mut pts_writer = BufWriter::new(match &pts_path[..] {
        "stdout" => Box::new(std::io::stdout()) as Box<dyn Write>,
        _ => Box::new(File::create(pts_path).expect("Unable to create file")) as Box<dyn Write>,
    });
    for (node, pts) in pts_map {
        if pts.is_empty() {
            continue;
        }
        let var = pag.node_path(*node);
        pts_writer
            .write_all(format!("{:?} ==> {{ ", var).as_bytes())
            .expect("Unable to write data");
        for pointee in pts {
            pts_writer
                .write_all(format!("{:?} ", pag.node_path(pointee)).as_bytes())
                .expect("Unable to write data");
        }
        pts_writer
            .write_all("}\n".as_bytes())
            .expect("Unable to write data");
    }
}

pub fn dump_pts_for<P: PAGPath>(pt_data: &DiffPTDataTy, pag: &PAG<P>, node_id: PAGNodeId) {
    let path = pag.node_path(node_id);
    println!("Processing node: {:?}, {:?}", node_id, path);
    let pts = pt_data.propa_pts_map.get(&node_id);
    if pts.is_some() {
        let pts = pts.unwrap();
        let mut str = String::new();
        for node in pts {
            str.push_str(&format!("{:?}, ", pag.node_path(node)));
        }
        println!("Points-to: {}", str);
    }
}


pub fn dump_ci_pts<P: PAGPath>(acx: &AnalysisContext, pt_data: &DiffPTDataTy, pag: &PAG<P>, grouped_pts_path: &String) {
    let mut grouped_pts: BTreeMap<FuncId, HashMap<&PathEnum, HashSet<&PathEnum>>> = BTreeMap::new();
    let pts_map = &pt_data.propa_pts_map;
    let mut pts_writer = BufWriter::new(match &grouped_pts_path[..] {
        "stdout" => Box::new(std::io::stdout()) as Box<dyn Write>,
        _ => Box::new(File::create(grouped_pts_path).expect("Unable to create file")) as Box<dyn Write>,
    });
    for (node, pts) in pts_map {
        if pts.is_empty() {
            continue;
        }
        let var = pag.node_path(*node);
        let value = var.value();
        if let Some(func_id) = path_func_id(value) {
            let pts_map = grouped_pts.entry(func_id).or_default();
            let tmp_pts = pts_map.entry(value).or_default();
            for pointee in pts {
                tmp_pts.insert(pag.node_path(pointee).value());
            }
        }
    }
    for (func_id, pts_map) in grouped_pts {
        pts_writer
            .write_all(format!("{:?} - {:?}\n", func_id, acx.get_function_reference(func_id).to_string()).as_bytes())
            .expect("Unable to write data");
        for (pt, pts) in pts_map {
            pts_writer
                .write_all(format!("\t{:?} ({:?}) ==> {{ ", pt, pts.len()).as_bytes())
                .expect("Unable to write data");
            for pointee in pts {
                pts_writer
                    .write_all(format!("{:?} ", pointee).as_bytes())
                    .expect("Unable to write data");
            }
            pts_writer
                .write_all("}\n".as_bytes())
                .expect("Unable to write data");
        }
    }
}

pub fn dump_mir<F: CGFunction + Into<FuncId>, S: CGCallSite>(
    acx: &AnalysisContext, 
    call_graph: &CallGraph<F, S>, 
    mir_path: &String
) {
    // let mut mir_writer = Box::new(File::create(mir_path).expect("Unable to create file")) as Box<dyn Write>;
    let mut mir_writer = match &mir_path[..] {
        "stdout" => Box::new(std::io::stdout()) as Box<dyn Write>,
        _ => Box::new(File::create(mir_path).expect("Unable to create file")) as Box<dyn Write>,
    };
    let mut visited_func = HashSet::new();
    for func in call_graph.reach_funcs_iter() {
        let func_id = func.into();
        if visited_func.contains(&func_id) {
            continue;
        }
        visited_func.insert(func_id);
        let def_id = acx.get_function_reference(func_id).def_id;
        let func_name = acx.get_function_reference(func_id).to_string();
        mir_writer
            .write_all(format!("[{:?} - {:?}]\n", func_id, func_name).as_bytes())
            .expect("Unable to write data");
        if !acx.tcx.is_mir_available(def_id) {
            mir_writer.write_all(("Mir is unavailable\n").as_bytes()).expect("Unable to write data");
        } else {
            rustc_middle::mir::write_mir_pretty(acx.tcx, Some(def_id), mir_writer.as_mut()).unwrap();
        }
        mir_writer.write_all("\n".as_bytes()).expect("Unable to write data");
    }
}

pub fn dump_dyn_calls<F: CGFunction, S: CGCallSite>(
    acx: &AnalysisContext, 
    call_graph: &CallGraph<F, S>, 
    dyn_calls_path: &String
) where
    F: Into<FuncId>,
    S: Into<BaseCallSite>,
{
    let mut dyn_dispatch_calls: HashMap<BaseCallSite, HashSet<FuncId>> = HashMap::new();
    let mut fnptr_calls: HashMap<BaseCallSite, HashSet<FuncId>> = HashMap::new();
    let mut dyn_fntrait_calls: HashMap<BaseCallSite, HashSet<FuncId>> = HashMap::new();
    for (callsite, call_edges) in &call_graph.callsite_to_edges {
        let callsite_type = call_graph.get_callsite_type(&(*callsite).into()).unwrap();
        match callsite_type {
            CallType::DynamicDispatch => {
                let callees = dyn_dispatch_calls.entry((*callsite).into()).or_default();
                for edge_id in call_edges {
                    let callee_id = call_graph.get_callee_id_of_edge(*edge_id).unwrap();
                    callees.insert(callee_id.into());
                }
            }
            CallType::FnPtr => {
                let callees = fnptr_calls.entry((*callsite).into()).or_default();
                for edge_id in call_edges {
                    let callee_id = call_graph.get_callee_id_of_edge(*edge_id).unwrap();
                    callees.insert(callee_id.into());
                }
            }
            CallType::DynamicFnTrait => {
                let callees = dyn_fntrait_calls.entry((*callsite).into()).or_default();
                for edge_id in call_edges {
                    let callee_id = call_graph.get_callee_id_of_edge(*edge_id).unwrap();
                    callees.insert(callee_id.into());
                }
            }
            _ => {}
        }
    }
    dump_dyn_calls_(
        acx,
        dyn_dispatch_calls,
        fnptr_calls,
        dyn_fntrait_calls,
        dyn_calls_path,
    );
}

fn dump_dyn_calls_(
    acx: &AnalysisContext,
    dyn_dispatch_calls: HashMap<BaseCallSite, HashSet<FuncId>>,
    fnptr_calls: HashMap<BaseCallSite, HashSet<FuncId>>,
    dyn_fntrait_calls: HashMap<BaseCallSite, HashSet<FuncId>>,
    dyn_calls_path: &String,
) {
    let mut dyn_calls_writer = BufWriter::new(match &dyn_calls_path[..] {
        "stdout" => Box::new(std::io::stdout()) as Box<dyn Write>,
        _ => Box::new(File::create(dyn_calls_path).expect("Unable to create file")) as Box<dyn Write>,
    });

    dyn_calls_writer
        .write_all(format!("#Dynamic dispatch calls:\n").as_bytes())
        .expect("Unable to write data");
    for (callsite, callees) in dyn_dispatch_calls {
        let caller_func_ref = acx.get_function_reference(callsite.func);
        dyn_calls_writer
            .write_all(
                format!(
                    "\tcallsite: {:?}, {:?}, callee: \n",
                    caller_func_ref.to_string(),
                    callsite.location
                )
                .as_bytes(),
            )
            .expect("Unable to write data");
        for callee in callees {
            dyn_calls_writer
                .write_all(format!("\t\t{:?}\n", acx.get_function_reference(callee).to_string()).as_bytes())
                .expect("Unable to write data");
        }
    }
    dyn_calls_writer
        .write_all(format!("#Fnptr calls:\n").as_bytes())
        .expect("Unable to write data");
    for (callsite, callees) in fnptr_calls {
        let caller_func_ref = acx.get_function_reference(callsite.func);
        dyn_calls_writer
            .write_all(
                format!(
                    "\tcallsite: {:?}, {:?}, callee: \n",
                    caller_func_ref.to_string(),
                    callsite.location
                )
                .as_bytes(),
            )
            .expect("Unable to write data");
        for callee in callees {
            dyn_calls_writer
                .write_all(format!("\t\t{:?}\n", acx.get_function_reference(callee).to_string()).as_bytes())
                .expect("Unable to write data");
        }
    }
    dyn_calls_writer
        .write_all(format!("#Dynamic Fn* Trait calls:\n").as_bytes())
        .expect("Unable to write data");
    for (callsite, callees) in dyn_fntrait_calls {
        let caller_func_ref = acx.get_function_reference(callsite.func);
        dyn_calls_writer
            .write_all(
                format!(
                    "\tcallsite: {:?}, {:?}, callee: \n",
                    caller_func_ref.to_string(),
                    callsite.location
                )
                .as_bytes(),
            )
            .expect("Unable to write data");
        for callee in callees {
            dyn_calls_writer
                .write_all(format!("\t\t{:?}\n", acx.get_function_reference(callee).to_string()).as_bytes())
                .expect("Unable to write data");
        }
    }
}

pub fn dump_func_contexts(acx: &AnalysisContext, call_graph: &CSCallGraph, ctx_strategy: &impl ContextStrategy, func_ctxts_path: &String) {
    let mut func_ctxts_writer = BufWriter::new(match &func_ctxts_path[..] {
        "stdout" => Box::new(std::io::stdout()) as Box<dyn Write>,
        _ => Box::new(File::create(func_ctxts_path).expect("Unable to create file")) as Box<dyn Write>,
    });

    let mut func_ctxts_map: HashMap<FuncId, HashSet<ContextId>> = HashMap::new();
    for cs_func in call_graph.reach_funcs_iter() {
        func_ctxts_map.entry(cs_func.func_id).or_default().insert(cs_func.cid);
    }
    
    // Sort and print the func_ctxts_map
    let mut sorted_func_ctxts: Vec<(&FuncId, &HashSet<ContextId>)> = func_ctxts_map.iter().collect();
    sorted_func_ctxts.sort_by(|a, b| a.1.len().cmp(&b.1.len()));
    for (func_id, ctxts) in sorted_func_ctxts {
        let func_ref = acx.get_function_reference(*func_id);
        let has_self_parameter = util::has_self_parameter(acx.tcx, func_ref.def_id);
        let has_self_ref_parameter = util::has_self_ref_parameter(acx.tcx, func_ref.def_id);
        let ctxts: HashSet<Rc<Context<_>>> = ctxts.iter().map(|ctxt_id| ctx_strategy.get_context_by_id(*ctxt_id)).collect();
        func_ctxts_writer
            .write_all(
                format!(
                    "{:?}, has_self_param: {:?}, has_self_ref_param: {:?}, #ctxts: {:?} \n",
                    func_ref.to_string(),
                    has_self_parameter,
                    has_self_ref_parameter,
                    ctxts.len()
                )
                .as_bytes(),
            )
            .expect("Unable to write data");
        func_ctxts_writer.write_all(format!("\t{:?}\n", ctxts).as_bytes()).expect("Unable to write data");
    }
}

pub fn dump_most_called_funcs<W: Write>(acx: &AnalysisContext, call_graph: &CallGraph<FuncId, BaseCallSite>, stat_writer: &mut BufWriter<W>) {
    let edge_references = call_graph.graph.edge_references();
    let mut call_times_map: HashMap<FuncId, u32> = HashMap::new();
    for edge_ref in edge_references {
        let target = edge_ref.target();
        let callee_id = call_graph.graph.node_weight(target).unwrap().func;
        let count = call_times_map.entry(callee_id).or_insert(0);
        *count += 1;
    }
    let mut vec: Vec<_> = call_times_map.into_iter().collect();
    vec.sort_by_key(|&(_, count)| std::cmp::Reverse(count));

    stat_writer
        .write_all("Top-100 called functions: \n".as_bytes())
        .expect("Unable to write data");
    for i in 0..100 {
        let (func_id, called_times) = vec.get(i).unwrap();
        let func_ref = acx.get_function_reference(*func_id);
        stat_writer
            .write_all(format!("\t{:?}: {:?}\n", func_ref.to_string(), called_times).as_bytes())
            .expect("Unable to write data");
    }
}



fn path_func_id(value: &PathEnum) -> Option<FuncId> {
    match value {
        PathEnum::LocalVariable { func_id, .. } 
        | PathEnum::Parameter { func_id, .. } 
        | PathEnum::ReturnValue { func_id } 
        | PathEnum::Auxiliary { func_id, .. } 
        | PathEnum::HeapObj { func_id, .. } => Some(*func_id),
        PathEnum::Constant 
        | PathEnum::StaticVariable { .. } 
        | PathEnum::PromotedConstant { .. } => {
            None
        }
        PathEnum::QualifiedPath { base, .. } 
        | PathEnum::OffsetPath { base, .. } => path_func_id(&base.value),
        PathEnum::Function(..) 
        | PathEnum::PromotedArgumentV1Array 
        | PathEnum::PromotedStrRefArray 
        | PathEnum::Type(..) => None,
    }
}

fn to_ci_call_graph<F, S>(
    call_graph: &CallGraph<F, S>, 
) -> CallGraph<FuncId, BaseCallSite> where 
    F: CGFunction + Into<FuncId>,
    S: CGCallSite + Into<BaseCallSite>,
{
    let mut ci_call_graph = CallGraph::new();
    for (callsite, edges) in &call_graph.callsite_to_edges {
        let ci_callsite: BaseCallSite = callsite.clone().into();
        for edge in edges {
            let (from_id, to_id) = call_graph.graph.edge_endpoints(*edge).unwrap();
            let from_func = call_graph.graph.node_weight(from_id).unwrap().func;
            let to_func = call_graph.graph.node_weight(to_id).unwrap().func;
            ci_call_graph.add_edge(ci_callsite, from_func.into(), to_func.into());
        }
    }
    ci_call_graph
}