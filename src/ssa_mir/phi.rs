use crate::path::{Path, PathEnum};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Block {
    pub preds: Vec<Block>,
}

#[derive(Debug, Clone)]
pub struct Phi {
    pub block: Block,
    pub operands: Vec<Path>,
    pub users: HashSet<Block>,
}

impl Phi {
    pub fn new(block: Block) -> Self {
        Phi {
            block,
            operands: Vec::new(),
            users: HashSet::new()
        }
    }

    pub fn append_operand(&mut self, operand: Path) {
        self.operands.push(operand);  // Add an opperand to Phi node
    }

    pub fn add_user(&mut self, user: Block) {
        self.users.insert(user);  // Add a user to the Phi's users set
    }

    pub fn remove_user(&mut self, user: &Block) {
        self.users.remove(user);  // Remove a user from the Phi's users set
    }
}