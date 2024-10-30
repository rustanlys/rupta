// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! Flow strategies for flow-sensitive pointer analyses
use std::rc::Rc;

use crate::mir::call_site::{BaseCallSite, CSCallSite};
use crate::mir::context::{Context, ContextCache, ContextElement, ContextId, HybridCtxElem};
use crate::mir::path::{CSPath, Path};
use crate::rustc_index::Idx;

pub trait ContextStrategy {
    type E: ContextElement;
    fn empty_context(&self) -> Rc<Context<Self::E>>;
    fn get_empty_context_id(&mut self) -> ContextId;
    fn get_context_id(&mut self, context: &Rc<Context<Self::E>>) -> ContextId;
    fn get_context_by_id(&self, context_id: ContextId) -> Rc<Context<Self::E>>;
    fn new_instance_call_context(&mut self, callsite: &Rc<CSCallSite>, receiver: Option<&Rc<CSPath>>) -> Option<ContextId>;
    fn new_static_call_context(&mut self, callsite: &Rc<CSCallSite>) -> ContextId;
}

pub struct FlowInsetive {}


impl ContextStrategy for ContextInsensitive {
    type E = BaseCallSite;

    fn empty_context(&self) -> Rc<Context<BaseCallSite>> {
        Context::new_empty()
    }

    fn get_empty_context_id(&mut self) -> ContextId {
        ContextId::new(0)
    }
    
    fn get_context_id(&mut self, _context: &Rc<Context<BaseCallSite>>) -> ContextId {
        ContextId::new(0)
    } 

    fn get_context_by_id(&self, _context_id: ContextId) -> Rc<Context<BaseCallSite>> {
        self.empty_context()
    }  

    fn new_instance_call_context(&mut self, _callsite: &Rc<CSCallSite>, _receiver: Option<&Rc<CSPath>>) -> Option<ContextId> {
        Some(ContextId::new(0))
    }

    fn new_static_call_context(&mut self, _callsite: &Rc<CSCallSite>) -> ContextId {
        ContextId::new(0)
    }
}

pub struct KCallSiteSensitive {
    /// Context length limit for methods
    k: usize,
    pub(crate) ctx_cache: ContextCache<BaseCallSite>,
}

impl KCallSiteSensitive {
    pub fn new(k: usize) -> Self {
        Self {
            k, 
            ctx_cache: ContextCache::new(),
        }
    }

    pub fn new_context(&mut self, callsite: &Rc<CSCallSite>) -> ContextId {
        let caller_ctx_id = callsite.func.cid;
        let caller_ctx = self.ctx_cache.get_context(caller_ctx_id).unwrap();
        let callee_ctx = Context::new_k_limited_context(
            &caller_ctx,
            callsite.into(),
            self.k,
        );
        let callee_ctx_id = self.ctx_cache.get_context_id(&callee_ctx);
        callee_ctx_id
    }
}

impl ContextStrategy for KCallSiteSensitive {
    type E = BaseCallSite;

    fn empty_context(&self) -> Rc<Context<BaseCallSite>> {
        Context::new_empty()
    }
    
    fn get_context_id(&mut self, context: &Rc<Context<BaseCallSite>>) -> ContextId {
        self.ctx_cache.get_context_id(context)
    } 

    fn get_context_by_id(&self, context_id: ContextId) -> Rc<Context<BaseCallSite>> {
        self.ctx_cache.get_context(context_id).unwrap_or(Context::new_empty())
    }  

    fn get_empty_context_id(&mut self) -> ContextId {
        self.get_context_id(&Context::new_empty())
    }

    fn new_instance_call_context(&mut self, callsite: &Rc<CSCallSite>, _receiver: Option<&Rc<CSPath>>) -> Option<ContextId> {
        Some(self.new_context(callsite))
    }

    fn new_static_call_context(&mut self, callsite: &Rc<CSCallSite>) -> ContextId {
        self.new_context(callsite)
    }
}


pub struct KObjectSensitive {
    /// Context length limit for methods
    k: usize,
    pub(crate) ctx_cache: ContextCache<Rc<Path>>,
}

impl KObjectSensitive {
    pub fn new(k: usize) -> Self {
        Self {
            k, 
            ctx_cache: ContextCache::new(),
        }
    }

    pub fn new_context(&mut self, receiver: Rc<CSPath>) -> ContextId {
        let receiver_ctx_id = receiver.cid;
        let receiver_ctx = self.ctx_cache.get_context(receiver_ctx_id).unwrap();
        let callee_ctx = Context::new_k_limited_context(
            &receiver_ctx,
            receiver.path.clone(),
            self.k,
        );
        let callee_ctx_id = self.ctx_cache.get_context_id(&callee_ctx);
        callee_ctx_id
    }
}

impl ContextStrategy for KObjectSensitive {
    type E = Rc<Path>;

    fn empty_context(&self) -> Rc<Context<Rc<Path>>> {
        Context::new_empty()
    }
    
    fn get_context_id(&mut self, context: &Rc<Context<Rc<Path>>>) -> ContextId {
        self.ctx_cache.get_context_id(context)
    } 

    fn get_context_by_id(&self, context_id: ContextId) -> Rc<Context<Rc<Path>>> {
        self.ctx_cache.get_context(context_id).unwrap_or(Context::new_empty())
    }  

    fn get_empty_context_id(&mut self) -> ContextId {
        self.get_context_id(&Context::new_empty())
    }

    fn new_instance_call_context(&mut self, _callsite: &Rc<CSCallSite>, receiver: Option<&Rc<CSPath>>) -> Option<ContextId> {
        if let Some(cs_path) = receiver {
            Some(self.new_context(cs_path.clone()))
        } else {
            None
        }
    }

    fn new_static_call_context(&mut self, callsite: &Rc<CSCallSite>) -> ContextId {
        // use the same context as the caller function
        callsite.func.cid
    }
}


// A simple hybrid context sensitive approach, which analyzes instance-invoked methods in a object-sensitive way 
// and statically invoked functions in a callsite-sensitive way
pub struct SimpleHybridContextSensitive {
    /// Context length limit for methods
    k: usize,
    pub(crate) ctx_cache: ContextCache<HybridCtxElem>,
}

impl SimpleHybridContextSensitive {
    pub fn new(k: usize) -> Self {
        Self {
            k, 
            ctx_cache: ContextCache::new(),
        }
    }

    pub fn new_instance_call_context(&mut self, receiver: Rc<CSPath>) -> ContextId {
        let receiver_ctx_id = receiver.cid;
        let receiver_ctx = self.ctx_cache.get_context(receiver_ctx_id).unwrap();
        let callee_ctx = Context::new_k_limited_context(
            &receiver_ctx,
            HybridCtxElem::Object(receiver.path.clone()),
            self.k,
        );
        let callee_ctx_id = self.ctx_cache.get_context_id(&callee_ctx);
        callee_ctx_id
    }

    pub fn new_static_call_context(&mut self, callsite: &Rc<CSCallSite>) -> ContextId {
        let caller_ctx_id = callsite.func.cid;
        let caller_ctx = self.ctx_cache.get_context(caller_ctx_id).unwrap();
        let callee_ctx = Context::new_k_limited_context(
            &caller_ctx,
            HybridCtxElem::CallSite(callsite.into()),
            self.k,
        );
        let callee_ctx_id = self.ctx_cache.get_context_id(&callee_ctx);
        callee_ctx_id
    }
}

impl ContextStrategy for SimpleHybridContextSensitive {
    type E = HybridCtxElem;

    fn empty_context(&self) -> Rc<Context<HybridCtxElem>> {
        Context::new_empty()
    }
    
    fn get_context_id(&mut self, context: &Rc<Context<HybridCtxElem>>) -> ContextId {
        self.ctx_cache.get_context_id(context)
    } 

    fn get_context_by_id(&self, context_id: ContextId) -> Rc<Context<HybridCtxElem>> {
        self.ctx_cache.get_context(context_id).unwrap_or(Context::new_empty())
    }  

    fn get_empty_context_id(&mut self) -> ContextId {
        self.get_context_id(&Context::new_empty())
    }

    fn new_instance_call_context(&mut self, _callsite: &Rc<CSCallSite>, receiver: Option<&Rc<CSPath>>) -> Option<ContextId> {
        if let Some(cs_path) = receiver {
            Some(self.new_instance_call_context(cs_path.clone()))
        } else {
            None
        }
    }

    fn new_static_call_context(&mut self, callsite: &Rc<CSCallSite>) -> ContextId {
        // use the same context as the caller function
        self.new_static_call_context(callsite)
    }
}