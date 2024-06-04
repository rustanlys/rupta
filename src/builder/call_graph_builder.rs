// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! This module provides essential functions for resolving call targets.

use rustc_hir::def_id::DefId;
use rustc_middle::ty::{GenericArgsRef, TyCtxt, TyKind};

use crate::util;

/// Try to resolve the function with `def_id` and `gen_args`. 
/// 
/// If the function is not a trait method, (`def_id`, `gen_args`) is returned
/// directly. Otherwise, the function is devirtualized to a specific implementation.
pub fn resolve_fn_def<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: DefId,
    gen_args: GenericArgsRef<'tcx>,
) -> (DefId, GenericArgsRef<'tcx>) {
    if tcx.is_mir_available(def_id) && !util::is_trait_method(tcx, def_id) {
        (def_id, gen_args)
    } else if let Some((resolved_def_id, resolved_substs)) =
        try_to_devirtualize(tcx, def_id, gen_args)
    {
        (resolved_def_id, resolved_substs)
    } else {
        // if the function cannot be resolved, 
        // return the original (def_id, gen_args) pair directly.
        (def_id, gen_args)
    }
}

/// Try to devirtualize a trait method with `def_id` and `gen_args`. 
/// 
/// Returns `None` if the given `def_id` does not correspond to a trait method or 
/// we cannot resolve the trait method to a specific instance. For example, the 
/// first gen_arg is a dynamic type.
pub fn try_to_devirtualize<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: DefId,
    gen_args: GenericArgsRef<'tcx>,
) -> Option<(DefId, GenericArgsRef<'tcx>)> {
    if !util::is_trait_method(tcx, def_id) {
        return None;
    }

    // A trait method cannot be devirtualized when the first gen_arg corresponds
    // to a dynamic type.
    let arg0_ty = gen_args
        .types()
        .next()
        .expect("Expect `Self` substition in trait method invocation");
    if matches!(arg0_ty.kind(), TyKind::Dynamic(..)) {
        return None;
    }

    let param_env = rustc_middle::ty::ParamEnv::reveal_all();
    let abi = tcx
        .type_of(def_id)
        .skip_binder()
        .fn_sig(tcx)
        .abi();    
    let resolved_instance = if abi == rustc_target::spec::abi::Abi::Rust {
        // Instance::resolve panics if try_normalize_erasing_regions returns an error.
        // It is difficult to determine exactly when this error will occur.
        if tcx.try_normalize_erasing_regions(param_env, gen_args).is_err() {
            None
        } else {
            Some(rustc_middle::ty::Instance::resolve(
                tcx,
                param_env,
                def_id,
                gen_args,
            ))
        }
    } else {
        None
    };
    if let Some(Ok(Some(instance))) = resolved_instance {
        let resolved_def_id = instance.def.def_id();
        return Some((resolved_def_id, instance.args));
    }
    None
}
