use crate::{Path, AnalysisFlow};
use std::collections::HashSet;

/// Represents a Phi node in SSA form
#[derive(Clone, Debug)]
pub struct Phi {
    pub block: Block,              // Block to which this Phi belongs
    pub operands: Vec<Path>,      // Operands of the Phi node (paths)
    pub users: HashSet<PhiUser>,  // Users of this Phi node (other Phi nodes or operations)
}


impl Phi {
    // Tries to remove trivial Phi nodes, i.e., nodes that merge the same value or have a single operand.
    pub fn try_remove_trivial(&mut self, analysis_flow: &mut AnalysisFlow) -> Option<Path> {
        let mut same: Option<Path> = None;
        let mut users: HashSet<PhiUser> = HashSet::new(); // To track users of the Phi node

        // Traverse operands of the Phi node
        for op in &self.operands {
            // Skip trivial cases: if op is the same or self-reference (phi itself)
            if op == &same.unwrap_or(Path::default()) || op == &self.to_path() {
                continue;
            }

            if same.is_some() {
                // If `same` is already set, the Phi merges at least two values: not trivial
                return Some(self.to_path());
            }

            same = Some(op.clone()); // Set `same` to the first non-trivial operand
        }

        if same.is_none() {
            // If no operands were found, set `same` to `Undef`, representing an unreachable value
            same = Some(Path::Undef);
        }

        // Remove the users of the Phi and replace it with `same`
        for user in &self.users {
            if let Some(user_phi) = user.as_phi() {
                analysis_flow.try_remove_trivial_phi(user_phi);
            }
        }

        // Replace all uses of `phi` with `same`
        self.replace_by(same.unwrap_or(Path::Undef));

        // Return the simplified value, which is either `same` or `Undef`
        same
    }

    pub fn to_path(&self) -> Path {
        Path::Phi(self.clone())
    }

    pub fn replace_by(&mut self, path: Path) {
        self.operands.clear();
        self.operands.push(path);
    }
}

#[derive(Clone, Debug)]
pub struct PhiUser {
    // Users of the Phi node: other Phi nodes or general operations
    pub phi: Option<Phi>,
}

impl PhiUser {
    pub fn as_phi(&self) -> Option<&Phi> {
        self.phi.as_ref()
    }
}