use petgraph::graph::{DefaultIx, NodeIndex};
use petgraph::Graph;
use rustc_middle::mir;
use rustc_hir::def_id::DefId;
use rustc_middle::ty::{Ty, TyKind, GenericArgsRef};
use std::rc::Rc;
use petgraph::visit::EdgeRef;
use std::collections::{HashSet, VecDeque, HashMap};
use crate::builder::substs_specializer::SubstsSpecializer;
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::function::FuncId;
use rustc_middle::mir::Location;
use crate::pre_analysis::rta::rta::RapidTypeAnalysis;
use crate::mir::path::PathEnum;
use rustc_span::source_map::Spanned;


use crate::mir::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DFGNode {
    pub path: Rc<Path>,
}

impl DFGNode {
    pub fn new(path: Rc<Path>) -> Self {
        DFGNode { path }
    }

    /// Returns the path of the node.
    pub fn path(&self) -> &Rc<Path> {
        &self.path
    }

}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PFGEdge {
    pub kind: PFGEdgeEnum,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PFGEdgeEnum {
    IntraPFGEdge,
    CallPFGEdge(Location)
}

// === Pointer Flow Graph for a function ===
#[derive(Clone, Debug)]
pub struct FuncPFG {
    pub(crate) graph: Graph<DFGNode, PFGEdge>,
    edges: HashSet<(NodeIndex<DefaultIx>, NodeIndex<DefaultIx>)>,
    pub func_id: FuncId,
    // whether there is any argument-to-return flow (precision critical)
    pub has_arg_to_return_flow: bool, 
    // call-site to (arg paths, return path) mapping
    pub callsite_to_locals: HashMap<Location, (Vec<(usize, Rc<Path>)>, Rc<Path>)>,
    pub static_callsites: HashSet<Location>,
    // record of function pointer (FnPtr) and definition (FnDef) call-sites
    pub fn_ptr_def_callsites: HashSet<Location>,
    // record of closure and dynamic dispatch call-sites
    pub closure_dyn_callsites: HashSet<Location>, 
    // call-sites with arg->return path
    pub cs_callsites: HashSet<Location>, 
    // parameter with flow to return
    pub param_with_flow: HashSet<usize>,
}


impl FuncPFG {
    pub fn new(func_id: FuncId) -> Self {
        FuncPFG {
            graph: Graph::new(),
            edges: HashSet::new(),
            func_id,
            has_arg_to_return_flow: false,
            callsite_to_locals: HashMap::new(),
            static_callsites: HashSet::new(),
            fn_ptr_def_callsites: HashSet::new(),
            closure_dyn_callsites: HashSet::new(),
            cs_callsites: HashSet::new(),
            param_with_flow: HashSet::new(),
        }
    }

    pub fn add_node(&mut self, path: Rc<Path>) -> NodeIndex<DefaultIx> {
        self.graph.add_node(DFGNode::new(path))
    }

    pub fn get_or_insert_node(&mut self, path: Rc<Path>) -> NodeIndex<DefaultIx> {
        if let Some(node_index) = self.graph.node_indices().find(|&n| self.graph[n].path == <Path as Clone>::clone(&(*path)).into()) {
            node_index
        } else {
            self.add_node(path)
        }
    }

    pub fn add_edge(&mut self, src: Rc<Path>, dst: Rc<Path>, kind: PFGEdgeEnum) -> bool {
        let src_index = self.get_or_insert_node(src);
        let dst_index = self.get_or_insert_node(dst);
        if self.edges.insert((src_index, dst_index)) {
            self.graph.add_edge(src_index, dst_index, PFGEdge { kind: kind.clone() });
            return true;
        }
        false
    }

    pub fn get_node(&self, path: &Path) -> Option<&DFGNode> {
        let node = self.graph.node_indices().find(|&n| self.graph[n].path.as_ref() == path);
        if let Some(node_index) = node {
            Some(&self.graph[node_index])
        } else {
            None
        }
    }
    pub fn nodes(&self) -> impl Iterator<Item = &DFGNode> {
        self.graph.node_weights()
    }

    // check loc ⇒ 𝑓 in CTXFunc
    pub fn is_cs_callsite(&self, loc: &Location) -> bool {
        self.cs_callsites.contains(loc)
    }

    /// Solve the reachability in the pointer flow graph.
    pub fn solve_graph_reachability(&mut self) {
        // self.print_graph();
        self.has_arg_to_return_flow = false;
        self.cs_callsites.clear();
       
        let param_nodes: Vec<NodeIndex> = self
            .graph
            .node_indices()
            .filter(|&n| matches!(self.graph[n].path.value, PathEnum::Parameter{..}))
            .collect();
        
        let return_idx = self.get_or_insert_node(Path::new_return_value(self.func_id));

        // --- nodes that can reach return (reverse BFS) ---
        let mut reaches_ret: HashSet<NodeIndex> = HashSet::new();
        {
            let mut q = VecDeque::new();
            q.push_back(return_idx);
            while let Some(n) = q.pop_front() {
                if !reaches_ret.insert(n) { continue; }
                for e in self.graph.edges_directed(n, petgraph::Direction::Incoming) {
                    q.push_back(e.source());
                }
            }
        }


        // --- F: nodes reachable from any parameter (forward BFS) ---
        let mut from_param: HashSet<NodeIndex> = HashSet::new();
        {
            let mut q: VecDeque<NodeIndex> = param_nodes.iter().copied().collect();
            while let Some(n) = q.pop_front() {
                if !from_param.insert(n) { continue; }
                for v in self.graph.edges_directed(n, petgraph::Direction::Outgoing) {
                    q.push_back(v.target());
                }
            }
        }
        
        // --- Any arg → return path? ---
        let has_arg_to_return_flow = param_nodes.iter().any(|p| reaches_ret.contains(p));
        self.has_arg_to_return_flow = has_arg_to_return_flow;

        if !has_arg_to_return_flow {
            self.cs_callsites.clear();
            return;
        }

        // --- Collect only parameters that lie on arg→return path ---
        self.param_with_flow.clear();
        for &p in &param_nodes {
            if reaches_ret.contains(&p) {
                let path = &self.graph[p].path;
                let idx = match path.value {
                    PathEnum::Parameter { ordinal, .. } => ordinal,
                    _ => continue,
                };
                self.param_with_flow.insert(idx);
            }
        }

        // --- Collect only call-sites that lie on some arg→return path ---
        for e in self.graph.edge_references() {
            let u = e.source();
            let v = e.target();
            if from_param.contains(&u) && reaches_ret.contains(&v) {
                if let PFGEdgeEnum::CallPFGEdge(loc) = &e.weight().kind {
                    self.cs_callsites.insert(*loc);
                }
            }
        }
    }

    pub fn print_graph(&self) {
        println!("--- Pointer Flow Graph for function {:?} ---", self.func_id);
        for n in self.graph.node_indices() {
            let node = &self.graph[n];
            println!("  Node {:?}", node.path);
            // println!("  Node {:?}: from_non_param={:?}", node.path, node.from_non_param);
        }
        for e in self.graph.edge_references() {
            let src = &self.graph[e.source()];
            let dst = &self.graph[e.target()];
            println!(
                "  {:?} --{:?}--> {:?}",
                src.path,
                e.weight().kind,
                dst.path
            );
        }
    }
}


pub struct FuncPointerFlowAnalysis<'a, 'rta, 'tcx, 'compilation> {
    pub(crate) rta: &'rta mut RapidTypeAnalysis<'a, 'tcx, 'compilation>,
    pub(crate) func_id: FuncId,
    pub(crate) mir: &'tcx mir::Body<'tcx>,
    pub (crate) substs_specializer: SubstsSpecializer<'tcx>,
    pub pfg: FuncPFG,
}

impl<'a, 'rta, 'tcx, 'compilation> FuncPointerFlowAnalysis<'a, 'rta, 'tcx, 'compilation> {
    pub fn new(
        rta: &'rta mut RapidTypeAnalysis<'a, 'tcx, 'compilation>, 
        func_id: FuncId,
        mir: &'tcx mir::Body<'tcx>,
    ) -> FuncPointerFlowAnalysis<'a, 'rta, 'tcx, 'compilation> {
        let func_ref = rta.acx.get_function_reference(func_id);
        let substs_specializer = SubstsSpecializer::new(
            rta.acx.tcx, 
            func_ref.generic_args.clone()
        );

        FuncPointerFlowAnalysis {
            rta,
            func_id,
            mir,
            substs_specializer,
            pfg: FuncPFG::new(func_id),
        }
    }

    #[inline]
    fn acx(&mut self) -> &mut AnalysisContext<'tcx, 'compilation> {
        self.rta.acx
    }

    /// Construct the intra dataflow graph and calculate the dataflow information.
    pub fn calculate_intra_pointer_flow(&mut self) -> FuncPFG {

        // Check if any argument or return type contains pointer or reference
        let mut arg_has_ptr = false;
        for arg in self.mir.args_iter() {
            let decl = &self.mir.local_decls[arg];
            let arg_ty = self.substs_specializer.specialize_generic_argument_type(decl.ty);
            if self.type_contains_pointer_or_ref(arg_ty) {
                arg_has_ptr = true;
                break;
            }
        }
        let return_local =  &self.mir.local_decls[mir::Local::from_u32(0)];
        let return_ty = self.substs_specializer.specialize_generic_argument_type(return_local.ty);
        let return_has_ptr = self.type_contains_pointer_or_ref(return_ty);
        // we construct the pointer flow graph only if there exist pointer/ref in args and return type
        if arg_has_ptr && return_has_ptr {
            self.visit_body();
            self.solve_graph_initial();
        } else {
            self.pfg.has_arg_to_return_flow = false;
            self.pfg.cs_callsites.clear();
        }

        return self.pfg.clone();
    }


    fn solve_graph_initial(&mut self) {
        let ret_path = Path::new_local_parameter_or_result(self.func_id, 0, self.mir.arg_count);
        let _return_idx = self.pfg.get_or_insert_node(ret_path);
        self.pfg.solve_graph_reachability();

    }

    fn visit_body(&mut self){
        for bb in self.mir.basic_blocks.indices() {
            self.visit_basic_block(bb);
        }
    }

    fn visit_basic_block(&mut self, bb: mir::BasicBlock,) {
        let mir::BasicBlockData {
            ref statements,
            ref terminator,
            ..
        } = &self.mir[bb];
        let mut location = bb.start_location();
        let terminator_index = statements.len();

        while location.statement_index < terminator_index {
            self.visit_statement(location, &statements[location.statement_index]);
            location.statement_index += 1;
        }

        if let Some(mir::Terminator {
            ref source_info,
            ref kind,
        }) = *terminator
        {
            self.visit_terminator(location, kind, *source_info);
        }
    }

    /// Calls a specialized visitor for each kind of statement.
    fn visit_statement(&mut self, _location: mir::Location, statement: &mir::Statement<'tcx>) {
        let mir::Statement {kind, source_info: _} = statement;
        match kind {
            mir::StatementKind::Assign(box (place, rvalue)) => {
                
                let place_ty = self.substs_specializer.specialize_generic_argument_type(
                    self.mir.local_decls[place.local].ty
                );
                
                if self.type_contains_pointer_or_ref(place_ty) {
                    self.visit_assign(place, rvalue);
                }
                
            }
            _ => (),
        }   
    }

    fn visit_assign(&mut self, lplace: &mir::Place<'tcx>, rvalue: &mir::Rvalue<'tcx>) {
        match rvalue {
            mir::Rvalue::Use(operand) | mir::Rvalue::Repeat(operand, _) => {
                self.visit_use(lplace, operand);
            }
            mir::Rvalue::Ref(_, _, place) | mir::Rvalue::AddressOf(_, place) => {
                self.add_ref_pfg_edge(lplace, place);
            }
            mir::Rvalue::Cast(_cast_kind, operand, _ty) => {
                self.visit_use(lplace, operand);
            }
            mir::Rvalue::Aggregate(_ ,operands) => {
                for (_i, operand) in operands.iter().enumerate() {
                    self.visit_use(lplace, operand);
                }
            }
            mir::Rvalue::BinaryOp(_, box (loperand, _roperand)) => {
                self.visit_use(lplace, loperand);
            }
            mir::Rvalue::ShallowInitBox(operand, _) => {
                self.visit_use(lplace, operand);
            }
            mir::Rvalue::CopyForDeref(place) => {
                self.add_ref_pfg_edge(lplace, place);
            }
            _ => {
                // println!("Skipping rvalue kind: {:?}", rvalue);
            }
        }
    }

    fn visit_use(&mut self, lplace: &mir::Place<'tcx>, operand: &mir::Operand<'tcx>) {
        match operand {
            mir::Operand::Copy(place) | mir::Operand::Move(place) => {

                // For Copy and Move operands,
                // we add an edge from rplace to lplace only if rplace contains pointer or reference.
                let local_decls = &self.mir.local_decls;
                let place_ty = place.ty(local_decls, self.acx().tcx);
                let place_ty = self.substs_specializer.specialize_generic_argument_type(place_ty.ty);

                if self.type_contains_pointer_or_ref(place_ty) {
                    let lpath = Path::new_local_parameter_or_result(self.func_id, lplace.local.as_usize(), self.mir.arg_count); 
                    let rpath = Path::new_local_parameter_or_result(self.func_id, place.local.as_usize(), self.mir.arg_count);
                    let edge_kind = PFGEdgeEnum::IntraPFGEdge;
                    self.pfg.add_edge(rpath, lpath, edge_kind);
                }
            }
            _ => {} // We do not consider constant operands for pointer flow analysis.
        }

    }

    fn add_ref_pfg_edge(&mut self, lplace: &mir::Place<'tcx>, rplace: &mir::Place<'tcx>) {
        // For Ref and AddressOf rvalues, 
        // we add an edge from rplace to lplace only if rplace contains pointer or reference.
        let rplace_ty = self.substs_specializer.specialize_generic_argument_type(
                    self.mir.local_decls[rplace.local].ty
        );

        if self.type_contains_pointer_or_ref(rplace_ty) {
            let lpath = Path::new_local_parameter_or_result(self.func_id, lplace.local.as_usize(), self.mir.arg_count);
            let rpath = Path::new_local_parameter_or_result(self.func_id, rplace.local.as_usize(), self.mir.arg_count);
            let edge_kind = PFGEdgeEnum::IntraPFGEdge;
            self.pfg.add_edge(rpath, lpath, edge_kind);
        }
    }
    
    fn visit_terminator(
        &mut self,
        location: mir::Location,
        kind: &mir::TerminatorKind<'tcx>,
        _source_info: mir::SourceInfo,
    ) {
        match kind {
            mir::TerminatorKind::Call {
                func, 
                args,
                destination,
                target: _,
                unwind: _,
                call_source: _,
                fn_span: _,
            } => self.visit_call(func, args, destination, location),
            mir::TerminatorKind::InlineAsm { 
                template: _,
                operands: _,
                destination: _, 
                .. 
            } => {}
            _ => {}
        }
    }


    /// visit call for collecting callsite information
    fn visit_call(
        &mut self,
        func: &mir::Operand<'tcx>,
        args: &Vec<Spanned<mir::Operand<'tcx>>>,
        destination: &mir::Place<'tcx>,
        location: mir::Location,
    ) {
        let destination_ty = self.substs_specializer.specialize_generic_argument_type(
            self.mir.local_decls[destination.local].ty
        );
        if !self.type_contains_pointer_or_ref(destination_ty) {
            return;
        }
        // collect callsite information
        let destination_path = Path::new_local_parameter_or_result(self.func_id, destination.local.as_usize(), self.mir.arg_count);
        let args_paths = self.visit_args(args).into_iter().collect();

        self.pfg.callsite_to_locals.insert(location, (args_paths, destination_path.clone()));
        match func {
            mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                let fn_item_ty = self.substs_specializer.specialize_generic_argument_type(
                    self.mir.local_decls[place.local].ty
                );
                match fn_item_ty.kind() {
                    TyKind::Closure(callee_def_id, gen_args)
                    | TyKind::FnDef(callee_def_id, gen_args)
                    | TyKind::Coroutine(callee_def_id, gen_args) => {
                        self.resolve_call(callee_def_id, gen_args, args, destination, location)
                    }
                    TyKind::FnPtr(_) => {
                        // consider as static call for PAG only
                        self.pfg.static_callsites.insert(location);
                    }
                    _ => {}
                }
            }
            mir::Operand::Constant(box constant) => {
                match constant.ty().kind() {
                    TyKind::Closure(callee_def_id, gen_args)
                    | TyKind::FnDef(callee_def_id, gen_args)
                    | TyKind::Coroutine(callee_def_id, gen_args) => {
                        self.resolve_call(callee_def_id, gen_args, args, destination, location)
                    }
                    TyKind::FnPtr(_) => {
                        // consider as static call for PAG only
                        self.pfg.static_callsites.insert(location);
                    }
                    _ => {}
                }
                
            }
        }

    }


    /// collecting all dynamic dispatch callsites information
    fn resolve_call(
        &mut self,
        callee_def_id: &DefId,
        gen_args: &GenericArgsRef<'tcx>,
        _args: &Vec<Spanned<mir::Operand<'tcx>>>,
        _destination: &mir::Place<'tcx>,
        location: mir::Location,
    ) {

        if !self.acx().is_std_ops_fntrait_call(*callee_def_id) {
            self.pfg.static_callsites.insert(location);
            return;
        } 

        let mut first_subst_ty = match gen_args.types().next() {
            Some(ty) => ty,
            None => return,
        };
        first_subst_ty = self.substs_specializer.specialize_generic_argument_type(first_subst_ty);
        // Determine the callsite type based on the first generic argument type
        match first_subst_ty.kind() {
            TyKind::FnPtr(_) => {
                self.pfg.fn_ptr_def_callsites.insert(location);
            }
            TyKind::FnDef(_, _) => {
                self.pfg.fn_ptr_def_callsites.insert(location);
            }
            TyKind::Closure(_, _) | TyKind::Coroutine(_, _) => {
                // The dispatch callee of Closure and Coroutine contains 
                // reference to the closure/coroutine as first argument.
                self.pfg.closure_dyn_callsites.insert(location);
            }
            TyKind::Dynamic(_, _, _) => {
                // The dispatch callee of Dynamic contains self reference as first argument.  
                self.pfg.closure_dyn_callsites.insert(location);
            }
            _ => {}
        }
    }

    // Collect argument paths
    fn visit_args(&mut self, args: &Vec<Spanned<mir::Operand<'tcx>>>,) -> Vec<(usize, Rc<Path>)> {
        let mut idx = 0;
        let mut args_paths = Vec::new();
        for arg in args {
            idx += 1;
            match &arg.node {
                mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                    let arg_ty = self.substs_specializer.specialize_generic_argument_type(
                        self.mir.local_decls[place.local].ty
                    );

                    if self.type_contains_pointer_or_ref(arg_ty) {
                        let arg_path = Path::new_local_parameter_or_result(self.func_id, place.local.as_usize(), self.mir.arg_count);
                        args_paths.push((idx, arg_path));
                    }
                }
                _ => {}
            }
        }
        args_paths
    }
 
    fn type_contains_pointer_or_ref(&mut self, ty: Ty<'tcx>) -> bool {
        if ty.is_any_ptr() {
            return true;
        } else {
            let ptr_projs = self.acx().get_pointer_projections(ty);
            if !ptr_projs.is_empty() {
                return true;
            } else {
                return false;
            }
        }
    }

}