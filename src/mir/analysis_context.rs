// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use log::*;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::{DefId, DefIndex};
use rustc_index::IndexVec;
use rustc_middle::mir::Promoted;
use rustc_middle::ty::{GenericArgsRef, Ty, TyCtxt};
use rustc_session::Session;

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::info_collector::OverallMetadata;
use crate::mir::call_site::{BaseCallSite, CalleeIdentifier};
use crate::mir::function::{FuncId, FunctionReference, GenericArgE};
use crate::mir::known_names::{KnownNames, KnownNamesCache};
use crate::mir::path::Path;
use crate::mir::path::{PathEnum, ProjectionElems};
use crate::util;
use crate::util::options::AnalysisOptions;
use crate::util::type_util::{self, FieldByteOffsetCache, PathCastCache, PointerProjectionsCache, TypeCache};

/// Global information of the analysis
pub struct AnalysisContext<'tcx, 'compilation> {
    /// The central data structure of the compiler.
    pub tcx: TyCtxt<'tcx>,

    /// Represents the data associated with a compilation session for a single crate.
    pub session: &'compilation Session,

    /// The entry function of the analysis.
    pub entry_point: DefId,

    /// Options of the analysis.
    pub analysis_options: AnalysisOptions,

    pub functions: IndexVec<FuncId, Rc<FunctionReference<'tcx>>>,
    pub func_id_map: HashMap<Rc<FunctionReference<'tcx>>, FuncId>,
    pub func_name_cache: HashMap<FuncId, Box<str>>,

    /// Provides a way to refer to a  `rustc_middle::ty::Ty` via a handle that does not have
    /// a life time specifier.
    pub type_cache: TypeCache<'tcx>,

    /// Record the original type for each object.
    pub path_ty_cache: HashMap<Rc<Path>, Ty<'tcx>>,
    /// Record the memory size for each stack and heap object.
    pub path_memory_size: HashMap<Rc<Path>, usize>,

    /// Manage the cast types for each object.
    pub path_cast_cache: PathCastCache<'tcx>,

    /// Cache all the pointer type fields' projections for each type.
    pub ptr_projs_cache: PointerProjectionsCache<'tcx>,
    /// Cache the byte offset for each field of type.
    pub field_byte_offset_cache: FieldByteOffsetCache<'tcx>,

    pub dyn_callsite_cache: HashMap<BaseCallSite, CalleeIdentifier<'tcx>>,

    /// Functions specially handled in special_function_handler.
    pub special_functions: HashSet<FuncId>,

    /// Heap objects that have been cast to a concretized type.
    pub concretized_heap_objs: HashMap<Rc<Path>, Ty<'tcx>>,

    /// Record the max index of the auxiliary local variable for each function instance.
    pub(crate) aux_local_indexer: HashMap<FuncId, usize>,

    pub known_names_cache: KnownNamesCache,

    /// 存储所有元数据
    pub overall_metadata: OverallMetadata,
    /// 工作目录
    pub working_dir: std::path::PathBuf,
}

impl<'tcx, 'compilation> AnalysisContext<'tcx, 'compilation> {
    /// 想方设法地找到入口函数（因为这是构造自身所必须的），然后构造自身。
    pub fn new(
        session: &'compilation Session,
        tcx: TyCtxt<'tcx>,
        analysis_options: AnalysisOptions,
    ) -> Option<Self> {
        info!("Initializing AnalysisContext");
        let mut entry_fn_def_id: Option<DefId> = None;

        // Find the DefId for the entry point according to the function name
        if !analysis_options.entry_func.is_empty() {
            let entr_func = analysis_options.entry_func.clone();
            for local_def_id in tcx.hir().body_owners() {
                let def_kind = tcx.def_kind(local_def_id);
                if def_kind == DefKind::Fn || def_kind == DefKind::AssocFn {
                    let item_name = tcx.item_name(local_def_id.to_def_id());
                    if item_name.to_string() == *entr_func {
                        entry_fn_def_id = Some(local_def_id.to_def_id());
                    }
                }
            }
        }

        if entry_fn_def_id.is_none() {
            // If `entry_def_id` flag is provided, find entry point according to the index
            entry_fn_def_id = if let Some(entry_def_id) = analysis_options.entry_def_id {
                Some(DefId::local(DefIndex::from_u32(entry_def_id)))
            } else {
                // If no entry point specified, use the default entry
                if let Some((def_id, _)) = tcx.entry_fn(()) {
                    Some(def_id)
                } else {
                    None
                }
            }
        }

        if let Some(entry_def_id) = entry_fn_def_id {
            let entry_name = tcx.item_name(entry_def_id);
            // Entry Point: "main", DefId: DefId(0:4 ~ example_crate[6a34]::main)
            info!("Entry Point: {:?}, DefId: {:?}", entry_name, entry_def_id);
            // tcx.def_path_str = main
            info!("tcx.def_path_str = {}", tcx.def_path_str(entry_def_id));
            Some(Self {
                tcx,
                session,
                entry_point: entry_def_id,
                analysis_options,
                functions: IndexVec::new(),
                func_id_map: HashMap::new(),
                func_name_cache: HashMap::new(),
                type_cache: TypeCache::new(),
                path_ty_cache: HashMap::new(),
                path_cast_cache: PathCastCache::new(),
                path_memory_size: HashMap::new(),
                ptr_projs_cache: PointerProjectionsCache::new(),
                field_byte_offset_cache: FieldByteOffsetCache::new(),
                dyn_callsite_cache: HashMap::new(),
                special_functions: HashSet::new(),
                aux_local_indexer: HashMap::new(),
                concretized_heap_objs: HashMap::new(),
                known_names_cache: KnownNamesCache::create_cache_from_language_items(),
                overall_metadata: OverallMetadata::default(),
                working_dir: std::env::current_dir().unwrap(),
            })
        } else {
            error!("Entry point not found");
            None
        }
    }

    /// Records the type of `path`.
    pub fn set_path_rustc_type(&mut self, path: Rc<Path>, ty: Ty<'tcx>) {
        let erase_regions_ty = self.tcx.erase_regions_ty(ty);
        if let Some(t) = self.path_ty_cache.get(&path) {
            if *t == erase_regions_ty {
                return;
            } else if !ty.is_impl_trait() {
                // An impl trait type maybe updated to a concrete type later
            }
        }
        self.path_ty_cache.insert(path, erase_regions_ty);
    }

    pub fn get_path_rustc_type(&self, path: &Rc<Path>) -> Option<Ty<'tcx>> {
        if let Some(ty) = self.path_ty_cache.get(path) {
            return Some(*ty);
        }
        None
    }

    /// Records the size of `path``.
    pub fn set_path_memory_size(&mut self, path: Rc<Path>, ty: Ty<'tcx>) {
        let max_size = 10000;
        match path.value {
            PathEnum::HeapObj { .. } => {
                self.path_memory_size.insert(path, max_size);
            }
            PathEnum::Function(..) | PathEnum::Type(..) => {
                self.path_memory_size.insert(path, 0);
            }
            _ => {
                let param_env = rustc_middle::ty::ParamEnv::reveal_all();
                let size = type_util::size_of(self.tcx, param_env, ty);
                self.path_memory_size.insert(path.clone(), size);
            }
        }
    }

    pub fn get_path_memory_size(&self, path: &Rc<Path>) -> Option<usize> {
        if let Some(size) = self.path_memory_size.get(path) {
            return Some(*size);
        }
        None
    }

    pub fn get_type_index(&mut self, ty: &Ty<'tcx>) -> usize {
        let erase_regions_ty = self.tcx.erase_regions_ty(*ty);
        self.type_cache.get_index(&erase_regions_ty)
    }

    pub fn get_type_by_index(&self, index: usize) -> Option<Ty<'tcx>> {
        self.type_cache.get_type(index)
    }

    /// Creates a path cast from the given path.
    pub fn cast_to(&mut self, path: &Rc<Path>, ty: Ty<'tcx>) -> Option<Rc<Path>> {
        let mut path_cast_cache = std::mem::take(&mut self.path_cast_cache);
        let res = path_cast_cache.cast_to(self, path, ty);
        std::mem::swap(&mut self.path_cast_cache, &mut path_cast_cache);
        res
    }

    /// Returns the type variant of the given path, returns `None` if the path has not been cast to `ty`.
    pub fn get_type_variant(&mut self, path: &Rc<Path>, ty: Ty<'tcx>) -> Option<Rc<Path>> {
        let mut path_cast_cache = std::mem::take(&mut self.path_cast_cache);
        let res = path_cast_cache.get_type_variant(self, path, ty);
        std::mem::swap(&mut self.path_cast_cache, &mut path_cast_cache);
        res
    }

    /// Different paths may refer to the same memory location, we can regularize these path to a base path
    /// e.g. `a.0.0`, `a.0`, `a.cast#T'` and `a` will all be represented by one path.
    pub fn get_regularized_path(&mut self, path: Rc<Path>) -> Rc<Path> {
        PathCastCache::get_regularized_path(self, path)
    }

    /// Returns the types that a path have been cast to.
    pub fn get_cast_types(&self, path: &Rc<Path>) -> Option<&HashSet<Ty<'tcx>>> {
        self.path_cast_cache.get_cast_types(path)
    }

    /// Get the pointer type fields' projections.
    pub fn get_pointer_projections(&mut self, ty: Ty<'tcx>) -> &Vec<(ProjectionElems, Ty<'tcx>)> {
        self.ptr_projs_cache.get_pointer_projections(self.tcx, ty)
    }

    /// Get the byte offset of a specific field.
    pub fn get_field_byte_offset(&mut self, base_ty: Ty<'tcx>, proj: &ProjectionElems) -> usize {
        self.field_byte_offset_cache
            .get_field_byte_offset(self.tcx, base_ty, proj)
    }

    pub fn get_or_add_function_reference(&mut self, func_ref: Rc<FunctionReference<'tcx>>) -> FuncId {
        match self.func_id_map.entry(func_ref.clone()) {
            Entry::Occupied(o) => o.get().to_owned(),
            Entry::Vacant(v) => {
                // 可见这个id: FuncId其实就是这个func_ref在self.functions中的索引
                let id = self.functions.push(func_ref.clone());
                self.func_name_cache
                    .insert(id, func_ref.to_string().into_boxed_str());
                *v.insert(id)
            }
        }
    }

    pub fn get_function_reference(&self, func_id: FuncId) -> Rc<FunctionReference<'tcx>> {
        self.functions.get(func_id).unwrap().clone()
    }

    pub fn get_func_id(&mut self, def_id: DefId, gen_args: GenericArgsRef<'tcx>) -> FuncId {
        let generic_types = util::customize_generic_args(self.tcx, gen_args);
        let func_ref = FunctionReference::new_function_reference(def_id, generic_types);
        self.get_or_add_function_reference(func_ref)
    }

    pub fn get_promoted_id(
        &mut self,
        def_id: DefId,
        gen_args: Vec<GenericArgE<'tcx>>,
        promoted: Promoted,
    ) -> FuncId {
        let func_ref = FunctionReference::new_promoted_reference(def_id, gen_args, promoted);
        self.get_or_add_function_reference(func_ref)
    }

    pub fn add_dyn_callsite(
        &mut self,
        callsite: BaseCallSite,
        callee_id: DefId,
        gen_args: GenericArgsRef<'tcx>,
    ) {
        self.dyn_callsite_cache.insert(callsite, (callee_id, gen_args));
    }

    pub fn get_dyn_callee_identifier(&self, callsite: &BaseCallSite) -> Option<&CalleeIdentifier<'tcx>> {
        self.dyn_callsite_cache.get(callsite)
    }

    pub fn add_special_function(&mut self, func_id: FuncId) {
        self.special_functions.insert(func_id);
    }

    pub fn is_std_ops_fntrait_call(&mut self, def_id: DefId) -> bool {
        let known_name = self.get_known_name_for(def_id);
        match known_name {
            KnownNames::StdOpsFunctionFnCall
            | KnownNames::StdOpsFunctionFnMutCallMut
            | KnownNames::StdOpsFunctionFnOnceCallOnce => true,
            _ => false,
        }
    }

    pub fn def_in_ops_func_namespace(&mut self, def_id: DefId) -> bool {
        let known_name = self.get_known_name_for(def_id);
        match known_name {
            KnownNames::StdOpsFunctionImpls
            | KnownNames::StdOpsFunctionFnCall
            | KnownNames::StdOpsFunctionFnMutCallMut
            | KnownNames::StdOpsFunctionFnOnceCallOnce => true,
            _ => false,
        }
    }

    pub fn get_known_name_for(&mut self, def_id: DefId) -> KnownNames {
        self.known_names_cache.get(self.tcx, def_id)
    }

    /// Creates an auxiliary local variable with the given type for the given `func_id`.
    /// Returns the path of the auxiliary local variable.
    ///
    /// Auxiliary local variables are introduced for breaking donw complex statements into
    /// simple assignments.
    pub fn create_aux_local(&mut self, func_id: FuncId, ty: Ty<'tcx>) -> Rc<Path> {
        let aux_local_index = *self.aux_local_indexer.get(&func_id).expect("aux_local_index");
        debug!(
            "Creating aux local variable {:?} with ty: {:?} for {:?}",
            aux_local_index, ty, func_id
        );
        let aux = Path::new_aux(func_id, aux_local_index);
        self.set_path_rustc_type(aux.clone(), ty);
        // update aux_local_indexer
        self.aux_local_indexer.insert(func_id, aux_local_index + 1);
        aux
    }
}
