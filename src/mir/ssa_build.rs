use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Formatter, Result};
use rustc_middle::mir::{Body, BasicBlock, TerminatorKind};


type ControlFlowGraph = HashMap<BasicBlock, Vec<BasicBlock>>;  // Basic block to its successors
type DominatorTree = HashMap<BasicBlock, HashSet<BasicBlock>>; // Block -> Dominators

pub struct SSABuilder<'tcx> {
    pub(crate) mir: &'tcx Body<'tcx>
}

impl<'mir> SSABuilder<'mir> {
    pub fn new(mir: &'mir Body<'mir>) -> Self {
        SSABuilder { mir }
    }

    pub fn ssa_build(&mut self) {
        let cfg = self.build_ssa_cfg();
        let dom_tree = self.compute_dominator_tree(&cfg);
        self.print_dominator_tree(&dom_tree);
    }

    // Build SSA CFG for a single function (one MIR)
    fn build_ssa_cfg(&mut self) -> ControlFlowGraph {
        let mut cfg: ControlFlowGraph = HashMap::new();
        
        for (bb, block_data) in self.mir.basic_blocks.iter_enumerated() {
            let mut successors: Vec<BasicBlock> = Vec::new();
            
            match &block_data.terminator().kind {
                TerminatorKind::Goto { target } => {
                    successors.push(*target);
                }
                TerminatorKind::Return => {
                    // Return doesn't have successors, ends the function
                }
                _ => {
                }
            }
            
            cfg.insert(bb, successors);
        }
        
        cfg
    }

    // Compute dominator tree for SSA CFG
    fn compute_dominator_tree(&self, cfg: &ControlFlowGraph) -> DominatorTree {
        let _n = cfg.len();
        let mut dom: HashMap<BasicBlock, HashSet<BasicBlock>> = HashMap::new();
        let mut semi: HashMap<BasicBlock, BasicBlock> = HashMap::new();
        let mut ancestor: HashMap<BasicBlock, BasicBlock> = HashMap::new();
        let mut parent: HashMap<BasicBlock, BasicBlock> = HashMap::new();
        let mut label: HashMap<BasicBlock, BasicBlock> = HashMap::new();
        let mut dfs: Vec<BasicBlock> = Vec::new();
        
        // Initialize the DFS and dominance relations
        for &bb in cfg.keys() {
            dom.insert(bb, HashSet::new());
            semi.insert(bb, bb); // Initially each block dominates itself
            parent.insert(bb, bb);
            label.insert(bb, bb);
        }

        // DFS traversal to set the DFS ordering
        let mut visited: HashSet<BasicBlock> = HashSet::new();
        let mut dfs_stack = vec![cfg.keys().next().unwrap().clone()]; // Start from the first block

        while let Some(curr) = dfs_stack.pop() {
            if visited.insert(curr) {
                dfs.push(curr);
                if let Some(successors) = cfg.get(&curr) {
                    for &succ in successors {
                        if !visited.contains(&succ) {
                            dfs_stack.push(succ);
                        }
                    }
                }
            }
        }

        // Step 1: Compute the dominator tree using Lengauer-Tarjan
        for &v in dfs.iter().rev() {
            for &w in cfg.get(&v).unwrap_or(&vec![]).iter() {
                if semi.get(&w).cloned().unwrap_or(v) != v {
                    let mut u = semi[&w];
                    while semi.get(&u).cloned().unwrap_or(v) != v {
                        u = semi[&u];
                    }
                    semi.insert(w, u);
                }
            }
            if v != dfs[0] {
                ancestor.insert(v, v);
            }
        }

        // Step 2: Propagate the dominator tree
        for v in dfs.iter().rev() {
            if let Some(&p) = parent.get(v) {
                if let Some(&a) = ancestor.get(v) {
                    let mut u = *semi.get(&a).unwrap_or(&p);
                    while let Some(&w) = ancestor.get(&u) {
                        if *semi.get(&w).unwrap_or(&p) != p {
                            u = w;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        // Convert into dominator tree structure
        let mut dom_tree = DominatorTree::new();
        for &v in dfs.iter() {
            for &w in cfg.get(&v).unwrap_or(&vec![]).iter() {
                if semi.get(&w).cloned().unwrap_or(v) == v {
                    dom_tree.entry(v).or_insert_with(HashSet::new).insert(w);
                }
            }
        }

        dom_tree
    }

    fn print_dominator_tree(&self, dom_tree: &DominatorTree) {
        for (block, dominators) in dom_tree.iter() {
            let dominators: Vec<String> = dominators.iter().map(|x| format!("{:?}", x)).collect();
            println!("Block {:?}: Dominators -> [{}]", block, dominators.join(", "));
        }
    }

    // Placeholder for adding a phi node to a block (to be implemented later)
    fn add_phi_node_to_block(&mut self, _block: BasicBlock) {
        unimplemented!()
    }

    // Placeholder for checking if a block contains a phi node (to be implemented later)
    fn frontier_block_contains_phi(&self, _block: BasicBlock) -> bool {
        unimplemented!()
    }
}

impl<'mir> Debug for SSABuilder<'mir> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "SSABuilder for MIR: {:?}", self.mir)
    }
}