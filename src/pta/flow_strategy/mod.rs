// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! Flow strategies for flow-sensitive pointer analyses
use std::collections::HashMap;
use std::rc::Rc;

use crate::mir::call_site::{BaseCallSite, CSCallSite};
use crate::mir::context::{Context, ContextCache, ContextElement, ContextId, HybridCtxElem};
use crate::mir::path::{CSPath, Path};
use crate::rustc_index::Idx;

pub trait FlowStrategy {
    type E: ContextElement;
    fn empty_flow_context(&self) -> Rc<Context<Self::E>>;
    fn get_empty_flow_context_id(&mut self) -> ContextId;
    fn get_flow_context_id(&mut self, context: &Rc<Context<Self::E>>) -> ContextId;
    fn get_flow_context_by_id(&self, context_id: ContextId) -> Rc<Context<Self::E>>;
    fn new_instance_flow_context(&mut self, callsite: &Rc<CSCallSite>, receiver: Option<&Rc<CSPath>>, path: &[CSCallSite]) -> Option<ContextId>;
    fn new_static_flow_context(&mut self, callsite: &Rc<CSCallSite>, path: &[CSCallSite]) -> ContextId;
    fn handle_branch(&mut self, condition: bool, callsite: &Rc<CSCallSite>, path: &[CSCallSite]) -> ContextId; // New method for handling branches
}

pub struct FlowInsensitive {}

impl FlowStrategy for FlowInsensitive {
    type E = BaseCallSite;

    fn empty_flow_context(&self) -> Rc<Context<BaseCallSite>> {
        Context::new_empty()
    }

    fn get_empty_flow_context_id(&mut self) -> ContextId {
        ContextId::new(0)
    }
    
    fn get_flow_context_id(&mut self, _context: &Rc<Context<BaseCallSite>>) -> ContextId {
        ContextId::new(0)
    } 

    fn get_flow_context_by_id(&self, _context_id: ContextId) -> Rc<Context<BaseCallSite>> {
        self.empty_flow_context()
    }  

    fn new_instance_flow_context(&mut self, _callsite: &Rc<CSCallSite>, _receiver: Option<&Rc<CSPath>>, _path: &[CSCallSite]) -> Option<ContextId> {
        Some(ContextId::new(0))
    }

    fn new_static_flow_context(&mut self, _callsite: &Rc<CSCallSite>, _path: &[CSCallSite]) -> ContextId {
        ContextId::new(0)
    }

    fn handle_branch(&mut self, _condition: bool, _callsite: &Rc<CSCallSite>, _path: &[CSCallSite]) -> ContextId {
        ContextId::new(0) // Default implementation, can be customized
    }
}

pub struct KFlowSensitive {
    /// Context length limit for methods
    k: usize,
    pub(crate) flow_cache: ContextCache<BaseCallSite>,
    // Track execution paths
    execution_paths: Vec<Vec<CSCallSite>>,
}

impl KFlowSensitive {
    pub fn new(k: usize) -> Self {
        Self {
            k, 
            flow_cache: ContextCache::new(),
            execution_paths: Vec::new(),
        }
    }

    pub fn new_flow_context(&mut self, callsite: &Rc<CSCallSite>, path: &[CSCallSite]) -> ContextId {
        // Track the current path of execution
        self.execution_paths.push(path.to_vec());
        
        let caller_ctx_id = callsite.func.cid;
        let caller_ctx = self.flow_cache.get_context(caller_ctx_id).unwrap();
        let callee_ctx = Context::new_k_limited_context(
            &caller_ctx,
            callsite.into(),
            self.k,
        );
        let callee_ctx_id = self.flow_cache.get_context_id(&callee_ctx);
        callee_ctx_id
    }
}

impl FlowStrategy for KFlowSensitive {
    type E = BaseCallSite;

    fn empty_flow_context(&self) -> Rc<Context<BaseCallSite>> {
        Context::new_empty()
    }
    
    fn get_flow_context_id(&mut self, context: &Rc<Context<BaseCallSite>>) -> ContextId {
        self.flow_cache.get_context_id(context)
    } 

    fn get_flow_context_by_id(&self, context_id: ContextId) -> Rc<Context<BaseCallSite>> {
        self.flow_cache.get_context(context_id).unwrap_or(Context::new_empty())
    }  

    fn get_empty_flow_context_id(&mut self) -> ContextId {
        self.get_flow_context_id(&Context::new_empty())
    }

    fn new_instance_flow_context(&mut self, callsite: &Rc<CSCallSite>, _receiver: Option<&Rc<CSPath>>, path: &[CSCallSite]) -> Option<ContextId> {
        Some(self.new_flow_context(callsite, path))
    }

    fn new_static_flow_context(&mut self, callsite: &Rc<CSCallSite>, path: &[CSCallSite]) -> ContextId {
        self.new_flow_context(callsite, path)
    }

    fn handle_branch(&mut self, condition: bool, callsite: &Rc<CSCallSite>, path: &[CSCallSite]) -> ContextId {
        // Create contexts for both branches
        let branch_context_id = if condition {
            self.new_flow_context(callsite, path) // True branch
        } else {
            self.new_flow_context(callsite, path) // False branch
        };
        branch_context_id
    }
}