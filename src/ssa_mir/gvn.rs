use std::collections::{HashMap, HashSet};
use crate::phi::{Phi, Block};
use crate::path::{Path, PathEnum, Undef};

#[derive(Default)]
pub struct GlobalValueNumbering {
    pub current_def: HashMap<String, HashMap<Block, Path>>,
    pub incomplete_phis: HashMap<Block, HashMap<String, Phi>>,
    pub sealed_blocks: HashSet<Block>,
}

impl GlobalValueNumbering {
    pub fn write_variable(&mut self, variable: String, block: Block, value: Path) {
        self.current_def.entry(variable).or_default().insert(block, value);
    }

    pub fn read_variable(&self, variable: &str, block: &Block) -> Option<Path> {
        if let Some(block_map) = self.current_def.get(variable) {
            return block_map.get(block).cloned();
        } else {
            return self.read_variable_recursive(variable, block)
        }
    }

    fn read_variable_recursive(&mut self, variable: &str, block: &Block) -> Option<Path> {
        if !self.sealed_blocks.contains(block) {
            // Incomplete CFG: create a phi function and register it
            let val = Phi::new(block.clone());
            self.incomplete_phis
                .entry(block.clone())
                .or_default()
                .insert(variable.to_string(), val.clone());
            Some(Path {
                value: PathEnum::Undef,
            })
        } else if block.preds.len() == 1 {
            // Single predecessor
            self.read_variable(variable, &block.preds[0])
        } else {
            // Multiple predecessors
            let mut phi = Phi::new(block.clone());
            self.write_variable(variable.to_string(), block.clone(), Path {
                value: PathEnum::Undef, // Create a placeholder for the Phi
            });

            let phi = self.add_phi_operands(variable, phi);
            self.write_variable(variable.to_string(), block.clone(), Path {
                value: PathEnum::Undef, // Update with final Phi value
            });

            Some(Path {
                value: PathEnum::Undef,
            })
        }
    }


    fn add_phi_operands(&mut self, variable: &str, mut phi: Phi) -> Phi {
        for pred in &phi.block.preds {
            if let Some(value) = self.read_variable(variable, pred) {
                phi.append_operand(value);
            }
        }
        self.try_remove_trivial_phi(&mut phi);
        return phi
    }

    fn try_remove_trivial_phi(&mut self, phi: &mut Phi) -> Path {
        let mut same: Option<Path> = None;
        
        for op in &phi.operands {
            if let Some(existing) = same {
                if op == &existing || op == &phi {
                    continue;
                }
            }
            
            if !same.is_none() {
                return phi.clone();      // Non-trivial Phi; The phi merges at least two values
            }
            same = Some(op.clone());
            
        }
    
        same = same.unwrap_or(Path {
            value: PathEnum::Undef,
        });
    }
    // ASK about users!
}