// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use std::collections::{HashSet, HashMap};
use std::rc::Rc;

use rustc_hir::def_id::DefId;
use rustc_middle::mir::Location;
use rustc_middle::ty::GenericArgsRef;

use crate::mir::function::{FuncId, CSFuncId};
use crate::mir::path::{Path, CSPath};


#[derive(Clone, PartialEq, Eq, Hash, Debug)]
/// The type of a call graph edge
pub enum CallType {
    // Calls resolved by static dispatch, including static Fn* trait calls
    StaticDispatch,
    // Calls resolved by dynamic dispatch, excluding dynamic Fn* trait calls
    DynamicDispatch,
    // Fn* trait calls resolved by dynamic dispatch
    DynamicFnTrait,
    // function pointer calls
    FnPtr,
}

pub type BaseCallSite = BaseCallSiteS<FuncId>;
pub type CSBaseCallSite = BaseCallSiteS<CSFuncId>;
pub type CallSite = CallSiteS<FuncId, Rc<Path>>;
pub type CSCallSite = CallSiteS<CSFuncId, Rc<CSPath>>;

pub type CalleeIdentifier<'tcx> = (DefId, GenericArgsRef<'tcx>);

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct BaseCallSiteS<F> {
    pub func: F,
    pub location: Location,
}

impl<F> BaseCallSiteS<F> {
    pub fn new(func: F, location: Location) -> Self {
        BaseCallSiteS { func, location }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CallSiteS<F, P> {
    pub func: F,
    pub location: Location,
    pub args: Vec<P>,
    pub destination: P,
}

impl<F, P> CallSiteS<F, P> {
    pub fn new(func: F, location: Location, args: Vec<P>, destination: P) -> Self {
        CallSiteS {
            func,
            location,
            args,
            destination,
        }
    }
}


#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ExtCallSiteS<'tcx, F, P> {
    pub callsite: Rc<CallSiteS<F, P>>,
    pub callee_def_id: DefId,
    pub callee_substs: GenericArgsRef<'tcx>,
}

impl<'tcx, F, P> ExtCallSiteS<'tcx, F, P> {
    pub fn new(callsite: Rc<CallSiteS<F, P>>, callee_def_id: DefId, callee_substs: GenericArgsRef<'tcx>) -> Self {
        ExtCallSiteS {
            callsite,
            callee_def_id,
            callee_substs,
        }
    }
}

impl<F: Copy + Into<FuncId>, P> From<Rc<CallSiteS<F, P>>> for BaseCallSite {
    fn from(callsite: Rc<CallSiteS<F, P>>) -> Self {
        BaseCallSiteS {
            func: callsite.func.into(),
            location: callsite.location,
        }
    }
}

impl<F: Copy + Into<FuncId>, P> From<&Rc<CallSiteS<F, P>>> for BaseCallSite {
    fn from(callsite: &Rc<CallSiteS<F, P>>) -> Self {
        BaseCallSiteS {
            func: callsite.func.into(),
            location: callsite.location,
        }
    }
}

impl From<CSBaseCallSite> for BaseCallSite {
    fn from(callsite: CSBaseCallSite) -> Self {
        BaseCallSiteS {
            func: callsite.func.into(),
            location: callsite.location,
        }
    }
}

impl From<&CSBaseCallSite> for BaseCallSite {
    fn from(callsite: &CSBaseCallSite) -> Self {
        BaseCallSiteS {
            func: callsite.func.into(),
            location: callsite.location,
        }
    }
}


impl From<Rc<CSCallSite>> for CSBaseCallSite {
    fn from(callsite: Rc<CSCallSite>) -> Self {
        BaseCallSiteS {
            func: callsite.func,
            location: callsite.location,
        }
    }
}

impl From<&Rc<CSCallSite>> for CSBaseCallSite {
    fn from(callsite: &Rc<CSCallSite>) -> Self {
        BaseCallSiteS {
            func: callsite.func,
            location: callsite.location,
        }
    }
}

/// Function calls associated with an instance, a dynamic object or a function pointer 
pub struct AssocCallGroup<I, F, P> {
    // Pairs of the self pointer and the associated function callsites that can be statically dispatched.
    pub(crate) static_dispatch_instance_calls: HashMap<I, HashSet<(Rc<CallSiteS<F, P>>, FuncId)>>,
    // Pairs of the dynamic trait object and its callsites.
    pub(crate) dynamic_dispatch_calls: HashMap<I, HashSet<Rc<CallSiteS<F, P>>>>,
    // Pairs of the dynamic trait object and the callsite of std::ops::Fn|FnOnce|FnMuT calls.
    pub(crate) dynamic_fntrait_calls: HashMap<I, HashSet<Rc<CallSiteS<F, P>>>>,
    // Pairs of the function pointer and its callsites.
    pub(crate) fnptr_calls: HashMap<I, HashSet<Rc<CallSiteS<F, P>>>>,
}

impl<I, F, P> AssocCallGroup<I, F, P> where 
    I: std::cmp::Eq + std::hash::Hash,
    F: std::cmp::Eq + std::hash::Hash,
    P: std::cmp::Eq + std::hash::Hash,
{
    pub fn new() -> Self {
        Self {
            static_dispatch_instance_calls: HashMap::new(),
            dynamic_fntrait_calls: HashMap::new(),
            dynamic_dispatch_calls: HashMap::new(),
            fnptr_calls: HashMap::new(),
        }
    }

    pub fn add_static_dispatch_instance_call(
        &mut self,
        self_ref: I,
        instance_callsite: Rc<CallSiteS<F, P>>,
        callee: FuncId
    ) {
        self.static_dispatch_instance_calls
            .entry(self_ref)
            .or_default()
            .insert((instance_callsite, callee));
    }

    pub fn add_dynamic_dispatch_call(
        &mut self,
        dyn_var: I,
        dyn_callsite: Rc<CallSiteS<F, P>>,
    ) {
        self.dynamic_dispatch_calls
            .entry(dyn_var)
            .or_default()
            .insert(dyn_callsite.clone());
    }

    pub fn add_dynamic_fntrait_call(
        &mut self,
        dyn_fn_obj: I,
        dyn_fntrait_callsite: Rc<CallSiteS<F, P>>,
    ) {
        self.dynamic_fntrait_calls
            .entry(dyn_fn_obj)
            .or_default()
            .insert(dyn_fntrait_callsite);
    }

    pub fn add_fnptr_call(&mut self, fn_ptr: I, callsite: Rc<CallSiteS<F, P>>) {
        self.fnptr_calls
            .entry(fn_ptr)
            .or_default()
            .insert(callsite.clone());
    }

}