//! Utilities of resolving visibility issues.

use rustc_hir::def::DefKind;
use rustc_hir::def_id::DefId;
use rustc_middle::ty::TyCtxt;
use std::collections::HashSet;

pub(crate) fn is_reachable(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    match def_id.as_local() {
        Some(def_id) => tcx.effective_visibilities(()).is_reachable(def_id),
        None => false,
    }
}

pub(crate) fn lib_entry_funcs<'tcx>(tcx: TyCtxt<'tcx>) -> HashSet<DefId> {
    let mut set = HashSet::new();
    for item in tcx.hir_crate_items(()).items() {
        let def_id = item.owner_id.def_id.to_def_id();
        match tcx.def_kind(def_id) {
            // XXX: make sure those cover all possible entries for call graph construction
            DefKind::AssocFn | DefKind::Fn => {
                if is_reachable(tcx, def_id) {
                    set.insert(def_id);
                }
            }
            _ => {}
        }
    }
    set
}
