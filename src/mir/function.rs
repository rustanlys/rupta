// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use rustc_hir::def_id::DefId;
use rustc_middle::mir::Promoted;
use rustc_middle::ty::{GenericArg, GenericArgKind};
use rustc_middle::ty::{Const, Ty};

use crate::mir::context::ContextId;
use std::rc::Rc;

rustc_index::newtype_index! {
    /// The unique identifier for each function reference.
    /// Every unique instantiation of a generic function will have a different function_id but the same def_id.
    #[orderable]
    #[debug_format = "FuncId({})"]
    pub struct FuncId {}
}

/// Context-sensitive function consisting of a context_id (cid) and a function id (func_id). 
#[derive(Copy, Clone, Debug, Eq, PartialOrd, PartialEq, Hash, Ord)]
pub struct CSFuncId {
    pub cid: ContextId,
    pub func_id: FuncId,
}

impl CSFuncId {
    pub fn new(cid: ContextId, func_id: FuncId) -> Self {
        Self { cid, func_id }
    }
}

impl From<CSFuncId> for FuncId {
    fn from(f: CSFuncId) -> Self {
        f.func_id
    }
}

/// Information that identifies a function instance.
#[derive(Clone, Debug, Eq, PartialOrd, PartialEq, Hash, Ord)]
pub struct FunctionReference<'tcx> {
    /// The crate specific key that is used to identify the function in the current crate.
    pub def_id: DefId,

    /// The generic argument types with which the referenced function was instantiated, if generic.
    pub generic_args: Vec<GenericArgE<'tcx>>,

    /// Promoteds do not have their own DefId. The body references promoteds by the DefId
    /// and the mir::Promoted index.
    pub promoted: Option<Promoted>,
}

/// We cannot implement the Clone and Hash trait for GenericArgKind,
/// therefore we provide a similar enum type as GenericArgKind
#[derive(Clone, Debug, Eq, PartialOrd, PartialEq, Hash, Ord)]
pub enum GenericArgE<'tcx> {
    Region,
    Const(Const<'tcx>),
    Type(Ty<'tcx>),
}

impl<'tcx> From<&GenericArg<'tcx>> for GenericArgE<'tcx> {
    fn from(ga: &GenericArg<'tcx>) -> GenericArgE<'tcx> {
        match ga.unpack() {
            GenericArgKind::Lifetime(_) => GenericArgE::Region,
            // the only supported Const types are integers, `bool` and `char`
            GenericArgKind::Const(c) => GenericArgE::Const(c),
            GenericArgKind::Type(ty) => GenericArgE::Type(ty),
        }
    }
}

impl<'tcx> FunctionReference<'tcx> {
    pub fn new_function_reference(
        def_id: DefId,
        generic_args: Vec<GenericArgE<'tcx>>,
    ) -> Rc<FunctionReference<'tcx>> {
        Rc::new(FunctionReference {
            def_id,
            generic_args,
            promoted: None,
        })
    }

    pub fn new_promoted_reference(
        def_id: DefId,
        generic_args: Vec<GenericArgE<'tcx>>,
        promoted: Promoted,
    ) -> Rc<FunctionReference<'tcx>> {
        Rc::new(FunctionReference {
            def_id,
            generic_args,
            promoted: Some(promoted),
        })
    }
}

impl<'tcx> ToString for FunctionReference<'tcx> {
    fn to_string(&self) -> String {
        let const_to_str = |c: &Const| -> String {
            if let Some(v) = c.try_to_scalar() {
                return v.to_string();
            }
            return "_".to_string();
        };

        let tmp1 = format!("{:?}", self.def_id);
        let crate_name = &tmp1[tmp1.find("~ ").unwrap() + 2..tmp1.find("[").unwrap()];
        let tmp2 = &tmp1[tmp1.find("::").unwrap() + 2..tmp1.len() - 1];
        let mut tmp3 = "".to_string();
        if !self.generic_args.is_empty() {
            tmp3.push('<');
            let tys = self
                .generic_args
                .iter()
                .filter_map(|t| match t {
                    GenericArgE::Type(ty) => Some(format!("{:?}", ty)),
                    GenericArgE::Const(c) => Some(const_to_str(c)),
                    _ => None,
                })
                .collect::<Vec<String>>();
            tmp3.push_str(&tys.join(", "));
            tmp3.push('>');
        }
        if let Some(promoted) = self.promoted {
            format!("{}::{}::promoted[{}]", crate_name, tmp2, promoted.index())
        } else {
            format!("{}::{}{}", crate_name, tmp2, tmp3)
        }
    }
}
