// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use rustc_hir::def_id::DefId;
use rustc_middle::mir;
use rustc_middle::ty::{GenericArgsRef, TyCtxt, TyKind};
use std::io::Write;
use std::rc::Rc;

use crate::mir::function::GenericArgE;
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::path::{Path, PathEnum, PathSelector};

pub mod bit_vec;
pub mod call_graph_stat;
pub mod chunked_queue;
pub mod dot;
pub mod index_tree;
pub mod mem_watcher;
pub mod options;
pub mod pta_statistics;
pub mod results_dumper;
pub mod type_util;
pub mod unsafe_statistics;


/// Returns the location of the rust system binaries that are associated with this build of rust-pta.
/// The location is obtained by looking at the contents of the environmental variables that were
/// set at the time rust-pta was compiled. If the rust compiler was installed by rustup, the variables
/// `RUSTUP_HOME` and `RUSTUP_TOOLCHAIN` are used and these are set by the compiler itself.
/// If the rust compiler was compiled and installed in some other way, for example from a source
/// enlistment, then the `RUST_SYSROOT` variable must be set in the environment from which rust-pta
/// is compiled.
/// 简而言之，如果工具链是通过rustup安装的，那么利用环境变量$RUSTUP_HOME和$RUSTUP_TOOLCHAIN合成出sysroot。
/// 否则，尝试直接读取环境变量$RUST_SYSROOT的值。还不行，就开摆。
pub fn find_sysroot() -> String {
    let home = option_env!("RUSTUP_HOME");
    let toolchain = option_env!("RUSTUP_TOOLCHAIN");
    match (home, toolchain) {
        (Some(home), Some(toolchain)) => format!("{home}/toolchains/{toolchain}"),
        _ => match option_env!("RUST_SYSROOT") {
            None => {
                panic!(
                    "Could not find sysroot. Specify the RUST_SYSROOT environment variable, \
                    or use rustup to set the compiler to use for Mirai",
                )
            }
            Some(sys_root) => sys_root.to_owned(),
        },
    }
}

/// Dumps a human readable MIR redendering of the function with the given DefId to standard output.
pub fn pretty_print_mir(tcx: TyCtxt<'_>, def_id: DefId) {
    if !matches!(
        tcx.def_kind(def_id),
        rustc_hir::def::DefKind::Struct | rustc_hir::def::DefKind::Variant
    ) {
        let mut stdout = std::io::stdout();
        stdout.write_fmt(format_args!("{:?}", def_id)).unwrap();
        rustc_middle::mir::write_mir_pretty(tcx, Some(def_id), &mut stdout).unwrap();
        let _ = stdout.flush();
    }
}

/// Returns true if the function identified by `def_id` is defined as part of a trait.
#[inline]
pub fn is_trait_method(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    if tcx.trait_of_item(def_id).is_some() {
        true
    } else {
        false
    }
}

/// Returns true if the function identified by `def_id` is defined in the Rust Standard Library.
pub fn is_std_lib_func(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    let crate_name = tcx.crate_name(def_id.krate);
    match crate_name.as_str() {
        "alloc" | "core" | "std" => true,
        _ => false,
    }
}

/// Returns true if the function has an explicit `self` (either `self` or `&(mut) self`) as its first
/// parameter, allowing method calls.
#[inline]
pub fn has_self_parameter(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    if !tcx.is_mir_available(def_id) {
        return false;
    }
    if let Some(associated_item) = tcx.opt_associated_item(def_id) {
        associated_item.fn_has_self_parameter
    } else {
        false
    }
}

/// Returns true if the function has an explicit `&(mut) self` as its first parameter, allowing method calls.
pub fn has_self_ref_parameter(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    if has_self_parameter(tcx, def_id) {
        let mir = tcx.optimized_mir(def_id);
        if let Some(decl) = mir.local_decls.get(mir::Local::from(1usize)) {
            decl.ty.is_ref()
        } else {
            false
        }
    } else {
        false
    }
}

/// Returns true if the call to (`callee_def_id`, `callee_substs`) is a dynamic call.
#[inline]
pub fn is_dynamic_call<'tcx>(
    tcx: TyCtxt<'tcx>,
    callee_def_id: DefId,
    callee_substs: GenericArgsRef<'tcx>,
) -> bool {
    if !is_trait_method(tcx, callee_def_id) {
        return false;
    }
    let arg0_ty = callee_substs
        .types()
        .next()
        .expect("Expect `Self` substition in trait method invocation");
    if matches!(arg0_ty.kind(), TyKind::Dynamic(..)) {
        true
    } else {
        false
    }
}


#[inline]
pub fn customize_generic_args<'tcx>(tcx: TyCtxt<'tcx>, generic_args: GenericArgsRef<'tcx>) -> Vec<GenericArgE<'tcx>> {
    generic_args
        .iter()
        .map(|t| match t.unpack() {
            // If the const generic cannot be evaluated, we repalce it with Const 1
            rustc_middle::ty::GenericArgKind::Const(c) => {
                if let Some(val) = c.try_eval_target_usize(tcx, rustc_middle::ty::ParamEnv::reveal_all()) {
                    GenericArgE::Const(rustc_middle::ty::Const::from_target_usize(tcx, val))
                } else {
                    GenericArgE::Const(rustc_middle::ty::Const::from_target_usize(tcx, 1))
                }
            }
            _ => GenericArgE::from(&t),
        })
        .collect()
}

/// Returns an `offset_path` equivalent to the `qualified_path`.
pub fn qualified_path_to_offset_path(acx: &mut AnalysisContext, path: Rc<Path>) -> Rc<Path> {
    if let PathEnum::QualifiedPath { base, projection } = &path.value {
        let base_ty = acx.get_path_rustc_type(base).unwrap();
        match projection[0] {
            PathSelector::Deref => {
                if projection.len() > 1 {
                    let deref_path = Path::new_deref(base.clone());
                    let deref_ty = type_util::get_dereferenced_type(base_ty);
                    let offset = acx.get_field_byte_offset(deref_ty, &projection[1..].to_vec());
                    Path::new_offset(deref_path, offset)
                } else {
                    path
                }
            }
            _ => {
                let offset = acx.get_field_byte_offset(base_ty, &projection);
                Path::new_offset(base.clone(), offset)
            }
        }
    } else {
        path
    }
}
