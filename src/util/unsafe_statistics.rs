// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};

use rustc_hir::def_id::DefId;
use rustc_hir::intravisit::Visitor;
use rustc_hir::Unsafety;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::CrateNum;

use crate::graph::call_graph::CallGraph;
use crate::mir::call_site::BaseCallSite;
use crate::mir::function::FuncId;
use crate::mir::analysis_context::AnalysisContext;

pub struct UnsafeStat<'pta, 'tcx, 'compilation> {
    acx: &'pta AnalysisContext<'tcx, 'compilation>,
    #[allow(unused)]
    caller_to_callees_map: HashMap<FuncId, HashSet<FuncId>>,
    callee_to_callers_map: HashMap<FuncId, HashSet<FuncId>>,
    reach_ci_funcs: Vec<FuncId>,
    // def_id_to_func_id_map: HashMap<DefId, HashSet<FuncId>>,
    exclude_unsafe_std: bool,
}

impl<'pta, 'tcx, 'compilation> UnsafeStat<'pta, 'tcx, 'compilation> {
    pub fn new(acx: &'pta AnalysisContext<'tcx, 'compilation>, call_graph: &CallGraph<FuncId, BaseCallSite>) -> Self {
        let mut caller_to_callees_map: HashMap<FuncId, HashSet<FuncId>> = HashMap::new();
        let mut callee_to_callers_map: HashMap<FuncId, HashSet<FuncId>> = HashMap::new();
        let graph = &call_graph.graph;
        for edge in graph.edge_indices() {
            let (caller_id, callee_id) = graph.edge_endpoints(edge).unwrap();
            let caller = graph.node_weight(caller_id).unwrap().func;
            let callee = graph.node_weight(callee_id).unwrap().func;
            caller_to_callees_map.entry(caller).or_default().insert(callee);
            callee_to_callers_map.entry(callee).or_default().insert(caller);
        }

        let mut reach_ci_funcs = Vec::new();
        for ci_func_id in call_graph.reach_funcs_iter() {
            reach_ci_funcs.push(ci_func_id);
        }

        UnsafeStat {
            acx,
            caller_to_callees_map,
            callee_to_callers_map,
            reach_ci_funcs,
            // def_id_to_func_id_map,
            exclude_unsafe_std: true,
        }
    }

    pub fn dump_unsafe_functions(&mut self, stat_path: &String) {
        let mut stat_writer = BufWriter::new(match &stat_path[..] {
            "stdout" => Box::new(std::io::stdout()) as Box<dyn Write>,
            _ => Box::new(File::create(stat_path).expect("Unable to create file")) as Box<dyn Write>,
        });

        stat_writer
            .write_all(
                "Conservative results (Both compiler-generated unsafety & user-provided unsafety included): \n"
                    .as_bytes(),
            )
            .expect("Unable to write data");
        self.count_unsafe_functions(true, &mut stat_writer);
        stat_writer
            .write_all("----------------------------------------------------------\n".as_bytes())
            .expect("Unable to write data");
        stat_writer
            .write_all("Optimistic results (Only user-provided unsafety included): \n".as_bytes())
            .expect("Unable to write data");
        self.count_unsafe_functions(false, &mut stat_writer);
    }

    pub fn count_unsafe_functions(
        &mut self,
        conservative: bool,
        stat_writer: &mut BufWriter<Box<dyn Write>>,
    ) {
        let explicit_unsafe_functions = self.collect_explicit_unsafe_functions(conservative);
        let possible_unsafe_functions = self.collect_possibile_unsafe_functrions(&explicit_unsafe_functions);

        let explicit_unsafe_defids: HashSet<DefId> = explicit_unsafe_functions
            .iter()
            .map(|func_id| self.acx.get_function_reference(*func_id).def_id)
            .collect();
        let possible_unsafe_defids: HashSet<DefId> = possible_unsafe_functions
            .iter()
            .map(|func_id| self.acx.get_function_reference(*func_id).def_id)
            .collect();

        let mut crate_to_funcids: HashMap<CrateNum, HashSet<FuncId>> = HashMap::new();
        let mut crate_to_defids: HashMap<CrateNum, HashSet<DefId>> = HashMap::new();
        self.reach_ci_funcs.iter().for_each(|func_id| {
            let def_id = self.acx.get_function_reference(*func_id).def_id;
            let crate_num = def_id.krate;
            crate_to_funcids.entry(crate_num).or_default().insert(*func_id);
            crate_to_defids.entry(crate_num).or_default().insert(def_id);
        });

        let mut crate_to_explicit_unsafe_funcids = HashMap::new();
        let mut crate_to_possible_unsafe_funcids = HashMap::new();
        for (crate_num, funcids) in &crate_to_funcids {
            let mut explicit_funcids: HashSet<FuncId> = HashSet::new();
            let mut possible_funcids: HashSet<FuncId> = HashSet::new();
            funcids.iter().for_each(|funcid| {
                if explicit_unsafe_functions.contains(funcid) {
                    explicit_funcids.insert(*funcid);
                }
                if possible_unsafe_functions.contains(funcid) {
                    possible_funcids.insert(*funcid);
                }
            });
            crate_to_explicit_unsafe_funcids.insert(*crate_num, explicit_funcids);
            crate_to_possible_unsafe_funcids.insert(*crate_num, possible_funcids);
        }

        let mut crate_to_explicit_unsafe_defids = HashMap::new();
        let mut crate_to_possible_unsafe_defids = HashMap::new();
        for (crate_num, defids) in &crate_to_defids {
            let mut explicit_defids: HashSet<DefId> = HashSet::new();
            let mut possible_defids: HashSet<DefId> = HashSet::new();
            defids.iter().for_each(|defid| {
                if explicit_unsafe_defids.contains(defid) {
                    explicit_defids.insert(*defid);
                }
                if possible_unsafe_defids.contains(defid) {
                    possible_defids.insert(*defid);
                }
            });
            crate_to_explicit_unsafe_defids.insert(*crate_num, explicit_defids);
            crate_to_possible_unsafe_defids.insert(*crate_num, possible_defids);
        }

        stat_writer
            .write_all(
                format!(
                    "#Explicit unsafe funcids: {}, defids: {}\n",
                    explicit_unsafe_functions.len(),
                    explicit_unsafe_defids.len()
                )
                .as_bytes(),
            )
            .expect("Unable to write data");
        stat_writer
            .write_all(
                format!(
                    "#Possible unsafe funcids: {}, defids: {}\n",
                    possible_unsafe_functions.len(),
                    possible_unsafe_defids.len()
                )
                .as_bytes(),
            )
            .expect("Unable to write data");

        for (crate_num, defids) in crate_to_defids {
            let crate_name = self.acx.tcx.crate_name(crate_num);
            let num_funcids = crate_to_funcids.get(&crate_num).unwrap().len();
            let num_defids = defids.len();
            stat_writer
                .write_all(
                    format!(
                        "crate: {:?}, num_all_funcids: {:?}, num_all_defids: {:?}\n",
                        crate_name, num_funcids, num_defids
                    )
                    .as_bytes(),
                )
                .expect("Unable to write data");
            if let Some(explicit_unsafe_defids) = crate_to_explicit_unsafe_defids.get(&crate_num) {
                let explicit_unsafe_funcids = crate_to_explicit_unsafe_funcids.get(&crate_num).unwrap();
                stat_writer
                    .write_all(
                        format!(
                            "\texplicit unsafe funcids: {:?}, defids: {:?}\n",
                            explicit_unsafe_funcids.len(),
                            explicit_unsafe_defids.len()
                        )
                        .as_bytes(),
                    )
                    .expect("Unable to write data");
                for defid in explicit_unsafe_defids {
                    stat_writer
                        .write_all(format!("\t\t{:?}\n", defid).as_bytes())
                        .expect("Unable to write data");
                }
            } else {
                stat_writer
                    .write_all(format!("\texplicit unsafe defids: 0\n").as_bytes())
                    .expect("Unable to write data");
            }

            if let Some(possible_unsafe_defids) = crate_to_possible_unsafe_defids.get(&crate_num) {
                let possible_unsafe_funcids = crate_to_possible_unsafe_funcids.get(&crate_num).unwrap();
                stat_writer
                    .write_all(
                        format!(
                            "\tpossible unsafe funcids: {:?}, defids: {:?}\n",
                            possible_unsafe_funcids.len(),
                            possible_unsafe_defids.len()
                        )
                        .as_bytes(),
                    )
                    .expect("Unable to write data");
                for defid in possible_unsafe_defids {
                    stat_writer
                        .write_all(format!("\t\t{:?}\n", defid).as_bytes())
                        .expect("Unable to write data");
                }
            } else {
                stat_writer
                    .write_all(format!("\tpossible unsafe defids: 0\n").as_bytes())
                    .expect("Unable to write data");
            }
        }
    }

    fn collect_possibile_unsafe_functrions(
        &self,
        explicit_unsafe_functions: &HashSet<FuncId>,
    ) -> HashSet<FuncId> {
        let mut possible_unsafe_func = HashSet::default();

        let mut worklist: VecDeque<FuncId> = VecDeque::new();
        for unsafe_func in explicit_unsafe_functions {
            worklist.push_back(*unsafe_func);
        }

        while !worklist.is_empty() {
            let unsafe_func = worklist.pop_front().unwrap();
            if let Some(callers) = self.callee_to_callers_map.get(&unsafe_func) {
                for caller in callers {
                    if explicit_unsafe_functions.contains(caller) {
                        continue;
                    }
                    let caller_func_ref = self.acx.get_function_reference(*caller);
                    let caller_def_id = caller_func_ref.def_id;
                    if self.exclude_unsafe_std && is_library_crate(self.acx.tcx, caller_def_id) {
                        continue;
                    }
                    if possible_unsafe_func.insert(*caller) {
                        worklist.push_back(*caller);
                    }
                }
            }
        }

        possible_unsafe_func
    }

    fn collect_explicit_unsafe_functions(&self, conservative: bool) -> HashSet<FuncId> {
        let mut explicit_unsafe_func = HashSet::new();

        for func_id in self.reach_ci_funcs.iter() {
            let func_ref = self.acx.get_function_reference(*func_id);
            let def_id = func_ref.def_id;
            if self.exclude_unsafe_std && is_library_crate(self.acx.tcx, def_id) {
                continue;
            }
            let fn_ty = self.acx.tcx.type_of(def_id).skip_binder();
            let is_declared_unsafe = match fn_ty.kind() {
                rustc_middle::ty::FnDef(..) => {
                    let sig = fn_ty.fn_sig(self.acx.tcx);
                    sig.unsafety() == Unsafety::Unsafe
                }
                rustc_middle::ty::Closure(_, substs) => {
                    let sig = substs.as_closure().sig();
                    sig.unsafety() == Unsafety::Unsafe
                }
                _ => false,
            };

            if is_declared_unsafe {
                explicit_unsafe_func.insert(*func_id);
                continue;
            }

            let mut contains_unsafe_block = false;
            let hir_map = self.acx.tcx.hir();
            if let Some(local_def_id) = def_id.as_local() {
                if let Some(body_id) = hir_map.maybe_body_owned_by(local_def_id) {
                    let body = hir_map.body(body_id);
                    let mut bv = BodyVisitor {
                        contains_unsafe_block,
                        conservative,
                    };
                    bv.visit_body(body);
                    contains_unsafe_block = bv.contains_unsafe_block;
                }
            }
            if contains_unsafe_block {
                explicit_unsafe_func.insert(*func_id);
            }
        }

        explicit_unsafe_func
    }
}

struct BodyVisitor {
    contains_unsafe_block: bool,
    conservative: bool,
}

impl<'tcx> rustc_hir::intravisit::Visitor<'tcx> for BodyVisitor {
    fn visit_block(&mut self, b: &'tcx rustc_hir::Block) {
        match b.rules {
            rustc_hir::BlockCheckMode::DefaultBlock => {}
            rustc_hir::BlockCheckMode::UnsafeBlock(unsafe_source) => {
                if self.conservative {
                    self.contains_unsafe_block = true;
                } else {
                    match unsafe_source {
                        rustc_hir::UnsafeSource::UserProvided => {
                            self.contains_unsafe_block = true;
                        }
                        rustc_hir::UnsafeSource::CompilerGenerated => {}
                    }
                }
            }
        }
        //count all the blocks, including the compiler generated ones
        rustc_hir::intravisit::walk_block(self, b);
    }
}

fn is_library_crate(tcx: TyCtxt, def_id: DefId) -> bool {
    let crate_name = tcx.crate_name(def_id.krate);
    crate_name.as_str() == "alloc" || crate_name.as_str() == "std" || crate_name.as_str() == "core"
}
