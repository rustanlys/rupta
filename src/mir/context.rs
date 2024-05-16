// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Result};
use std::hash::Hash;
use std::rc::Rc;

use rustc_middle::ty::Ty;
use rustc_index::IndexVec;

use super::call_site::BaseCallSite;
use super::path::Path;

rustc_index::newtype_index! {
    /// The unique identifier for each context.
    #[orderable]
    #[debug_format = "ContextId({})"]
    pub struct ContextId {}
}

pub trait ContextElement: Clone + Eq + PartialEq + Debug + Hash {}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Context<E: ContextElement> {
    pub(crate) context_elems: Vec<E>,
}

impl<E: ContextElement> Debug for Context<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        self.context_elems.fmt(f)
    }
}

impl<E: ContextElement> Context<E> {
    pub fn new_empty() -> Rc<Self> {
        Rc::new(Context {
            context_elems: Vec::new(),
        })
    }

    pub fn new(context_elems: Vec<E>) -> Rc<Self> {
        Rc::new(Context { context_elems })
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.context_elems.len()
    }

    /// Compose a new context from a given context and a new context element.
    /// Discard the last old context element if the length of context exceeds the depth limit  
    pub fn new_k_limited_context(old_ctx: &Rc<Context<E>>, elem: E, k: usize) -> Rc<Self> {
        let mut elems = Vec::with_capacity(k);
        if k > 0 {
            elems.push(elem);
            if old_ctx.len() < k {
                elems.extend_from_slice(&old_ctx.context_elems[..])
            } else {
                elems.extend_from_slice(&old_ctx.context_elems[..k - 1])
            }
        }
        Rc::new(Context { context_elems: elems })
    }

    pub fn k_limited_context(ctx: &Rc<Context<E>>, k: usize) -> Rc<Self> {
        if ctx.len() <= k {
            ctx.clone()
        } else {
            let elems = ctx.context_elems[..k].to_vec();
            Rc::new(Context { context_elems: elems })
        }
    } 
    
    pub fn first_context_element(&self) -> Option<&E> {
        self.context_elems.first()
    }

    pub fn last_context_element(&self) -> Option<&E> {
        self.context_elems.last()
    }

}


#[derive(Debug)]
pub struct ContextCache<E: ContextElement> {
    context_list: IndexVec<ContextId, Rc<Context<E>>>,
    context_to_index_map: HashMap<Rc<Context<E>>, ContextId>,
}

impl<E: ContextElement> Default for ContextCache<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: ContextElement> ContextCache<E> {
    pub fn new() -> ContextCache<E> {
        ContextCache {
            context_list: IndexVec::new(),
            context_to_index_map: HashMap::new(),
        }
    }

    /// Returns a non zero index that can be used to retrieve context via get_context.
    pub fn get_context_id(&mut self, context: &Rc<Context<E>>) -> ContextId {
        if let Some(id) = self.context_to_index_map.get(context) {
            *id
        } else {
            let id = self.context_list.push(context.clone());
            self.context_to_index_map.insert(context.clone(), id);
            id
        }
    }

    /// Returns the type that was stored at this index, or None if index is zero
    /// or greater than the length of the type list.
    pub fn get_context(&self, id: ContextId) -> Option<Rc<Context<E>>> {
        self.context_list.get(id).cloned()
    }

    pub fn context_list(&self) -> &IndexVec<ContextId, Rc<Context<E>>> {
        &self.context_list
    }
}


// Different kinds of context elements supported now
impl ContextElement for BaseCallSite {}

impl ContextElement for Rc<Path> {}

impl ContextElement for Ty<'_> {}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HybridCtxElem {
    CallSite(BaseCallSite),
    Object(Rc<Path>),
}

impl ContextElement for HybridCtxElem {}
