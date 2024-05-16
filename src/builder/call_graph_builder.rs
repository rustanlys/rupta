// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use rustc_hir::def_id::DefId;
use rustc_middle::ty::{GenericArgsRef, TyCtxt, TyKind};

use crate::util;

/// Try to resolve the given FnDef, devirtualize the callee function if possible.
pub fn resolve_fn_def<'tcx>(
    tcx: TyCtxt<'tcx>,
    callee_def_id: DefId,
    gen_args: GenericArgsRef<'tcx>,
) -> (DefId, GenericArgsRef<'tcx>) {
    if tcx.is_mir_available(callee_def_id) && !util::is_trait_method(tcx, callee_def_id) {
        (callee_def_id, gen_args)
    } else if let Some((callee_def_id, callee_substs)) =
        try_to_devirtualize(tcx, callee_def_id, gen_args)
    {
        (callee_def_id, callee_substs)
    } else {
        // if the mir is unavailable or the callee cannot be resolved, return the callee_def_id directly
        (callee_def_id, gen_args)
    }
}

pub fn try_to_devirtualize<'tcx>(
    tcx: TyCtxt<'tcx>,
    callee_def_id: DefId,
    gen_args: GenericArgsRef<'tcx>,
) -> Option<(DefId, GenericArgsRef<'tcx>)> {
    if !util::is_trait_method(tcx, callee_def_id) {
        return None;
    }

    let arg0_ty = gen_args
        .types()
        .next()
        .expect("Expect `Self` substition in trait method invocation");
    if matches!(arg0_ty.kind(), TyKind::Dynamic(..)) {
        return None;
    }

    let param_env = rustc_middle::ty::ParamEnv::reveal_all();
    let abi = tcx
        .type_of(callee_def_id)
        .skip_binder()
        .fn_sig(tcx)
        .abi();    
    let resolved_instance = if abi == rustc_target::spec::abi::Abi::Rust {
        // Instance::resolve panics if try_normalize_erasing_regions returns an error.
        // It is hard to figure out exactly when this will be the case.
        if tcx.try_normalize_erasing_regions(param_env, gen_args).is_err() {
            None
        } else {
            Some(rustc_middle::ty::Instance::resolve(
                tcx,
                param_env,
                callee_def_id,
                gen_args,
            ))
        }
    } else {
        None
    };
    if let Some(Ok(Some(instance))) = resolved_instance {
        let resolved_def_id = instance.def.def_id();
        let has_mir = tcx.is_mir_available(resolved_def_id);
        if !has_mir {
            return None;
        }
        return Some((resolved_def_id, instance.args));
    }
    None
}
