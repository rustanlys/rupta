// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use log::*;
use rustc_hir::def_id::DefId;
use rustc_middle::ty::{GenericArgKind, GenericArgsRef};
use rustc_middle::ty::{
    Const, ExistentialPredicate, FieldDef, ParamEnv, 
    PolyFnSig, Ty, TyCtxt, TyKind, TypeAndMut
};
use rustc_target::abi::VariantIdx;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::collections::hash_map::Entry;

use crate::builder::substs_specializer::SubstsSpecializer;
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::function::{FuncId, GenericArgE};
use crate::mir::known_names::{KnownNames, KnownNamesCache};
use crate::mir::path::{Path, PathEnum, PathSelector, ProjectionElems};

/// Provides a way to refer to a rustc_middle::ty::Ty via a handle that does not have
/// a life time specifier.
#[derive(Debug)]
pub struct TypeCache<'tcx> {
    type_list: Vec<Ty<'tcx>>,
    type_to_index_map: HashMap<Ty<'tcx>, usize>,
}

impl<'tcx> Default for TypeCache<'tcx> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'tcx> TypeCache<'tcx> {
    pub fn new() -> TypeCache<'tcx> {
        TypeCache {
            type_list: Vec::new(),
            type_to_index_map: HashMap::new(),
        }
    }

    /// Returns a non zero index that can be used to retrieve ty via get_type.
    pub fn get_index(&mut self, ty: &Ty<'tcx>) -> usize {
        if let Some(index) = self.type_to_index_map.get(ty) {
            *index
        } else {
            let index = self.type_list.len();
            self.type_list.push(*ty);
            self.type_to_index_map.insert(*ty, index);
            index
        }
    }

    /// Returns the type that was stored at this index, or None if index is zero
    /// or greater than the length of the type list.
    pub fn get_type(&self, index: usize) -> Option<Ty<'tcx>> {
        self.type_list.get(index).cloned()
    }

    pub fn type_list(&self) -> &Vec<Ty<'tcx>> {
        &self.type_list
    }
}


/// Provides a way to effectively get the pointer type fields of a given type
pub struct PointerProjectionsCache<'tcx> {
    pub(crate) ptr_projs_cache: HashMap<Ty<'tcx>, Vec<(ProjectionElems, Ty<'tcx>)>>,
}

impl<'tcx> Default for PointerProjectionsCache<'tcx> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'tcx> PointerProjectionsCache<'tcx> {
    pub fn new() -> PointerProjectionsCache<'tcx> {
        PointerProjectionsCache {
            ptr_projs_cache: HashMap::new(),
        }
    }

    /// Get or fetch the pointer type fields of the given base ty.
    pub fn get_pointer_projections(
        &mut self,
        tcx: TyCtxt<'tcx>,
        base_ty: Ty<'tcx>
    ) -> &Vec<(ProjectionElems, Ty<'tcx>)> {
        match self.ptr_projs_cache.entry(base_ty) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => v.insert(get_pointer_projections(tcx, base_ty)),
        }
    }
}


/// Provides a way to effectively get the byte offsets of an ADT type's fields
pub struct FieldByteOffsetCache<'tcx> {
    pub(crate) field_byte_offset_cache: HashMap<Ty<'tcx>, HashMap<ProjectionElems, usize>>,
}

impl<'tcx> Default for FieldByteOffsetCache<'tcx> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'tcx> FieldByteOffsetCache<'tcx> {
    pub fn new() -> FieldByteOffsetCache<'tcx> {
        FieldByteOffsetCache {
            field_byte_offset_cache: HashMap::new(),
        }
    }

    /// Get or compute the offset of the given proj of base_ty.
    /// If we cannot obtain the layout of base_ty, the offset would be 0
    pub fn get_field_byte_offset(
        &mut self,
        tcx: TyCtxt<'tcx>,
        base_ty: Ty<'tcx>,
        proj: &ProjectionElems,
    ) -> usize {
        if !self.field_byte_offset_cache.contains_key(&base_ty) {
            self.compute_fields_byte_offsets(tcx, base_ty);
        }
        let fields_byte_offsets = self.field_byte_offset_cache.get(&base_ty).unwrap();
        if let Some(offset) = fields_byte_offsets.get(proj) {
            *offset
        } else {
            0
        }
    }

    /// Compute the byte offset for each field a struct type
    pub fn compute_fields_byte_offsets(&mut self, tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) {
        let mut compute_subfields_offsets =
            |fld_proj: ProjectionElems,
             fld_ty,
             fld_offset,
             fld_byte_offsets: &mut HashMap<ProjectionElems, usize>| {
                if !self.field_byte_offset_cache.contains_key(&fld_ty) {
                    self.compute_fields_byte_offsets(tcx, fld_ty);
                }
                let subflds_offsets = self.field_byte_offset_cache.get(&fld_ty).unwrap();
                for (subfld, suboffset) in subflds_offsets {
                    let mut full_proj = fld_proj.clone();
                    full_proj.extend(subfld);
                    fld_byte_offsets.insert(full_proj, fld_offset + suboffset);
                }
            };

        let param_env = rustc_middle::ty::ParamEnv::reveal_all();
        let mut fields_byte_offsets = HashMap::new();
        match ty.kind() {
            TyKind::Adt(adt_def, args) if adt_def.is_struct() => {
                if let Ok(layout) = layout_of(tcx, param_env, ty) {
                    let layout = layout.layout;
                    let variant = adt_def.variants().iter().next().expect("at least one variant");
                    if let rustc_target::abi::FieldsShape::Arbitrary {
                        offsets,
                        memory_index: _,
                    } = layout.fields()
                    {
                        // offsets: Offsets for the first byte of each field, ordered to
                        // match the source definition order.
                        for (field_idx, offset) in offsets.iter().enumerate() {
                            let field = &variant.fields[field_idx.into()];
                            let field_ty = field_ty(tcx, field, args);
                            let byte_offset = offset.bytes_usize();
                            let proj = vec![PathSelector::Field(field_idx)];
                            fields_byte_offsets.insert(proj.clone(), byte_offset);
                            // analyse the subfield recursively
                            compute_subfields_offsets(proj, field_ty, byte_offset, &mut fields_byte_offsets);
                        }
                    }
                } 
            }
            TyKind::Adt(adt_def, args) if adt_def.is_union() => {
                let variant = adt_def.variants().iter().next().expect("at least one variant");
                // All fields start at no offset.
                for (field_idx, field) in variant.fields.iter().enumerate() {
                    let field_ty = field_ty(tcx, field, args);
                    let byte_offset = 0;
                    let proj = vec![PathSelector::UnionField(field_idx)];
                    fields_byte_offsets.insert(proj.clone(), byte_offset);
                    // analyse the subfield recursively
                    compute_subfields_offsets(proj, field_ty, byte_offset, &mut fields_byte_offsets);
                }
            }
            TyKind::Adt(adt_def, _args) if adt_def.is_enum() => {
                if !adt_def.variants().is_empty() {
                    if let Ok(layout) = layout_of(tcx, param_env, ty) {
                        let layout = layout.layout;
                        // Single enum variant has the same memory layout as structs.
                        // For enums with more than one inhabited variant: each variant comes with a discriminant
                        match layout.variants() {
                            // Todo
                            rustc_target::abi::Variants::Single { index: _ } => {
                            }
                            rustc_target::abi::Variants::Multiple {
                                tag: _,
                                tag_encoding: _,
                                tag_field: _,
                                variants: _,
                            } => {
                            }
                        }
                    }
                }
            }
            TyKind::Array(elem_ty, _) | TyKind::Slice(elem_ty) => {
                let byte_offset = 0;
                let proj = vec![PathSelector::Index];
                fields_byte_offsets.insert(proj.clone(), byte_offset);
                // analyse the subfield recursively
                compute_subfields_offsets(proj, *elem_ty, byte_offset, &mut fields_byte_offsets);
            }
            TyKind::Tuple(tuple_types) => {
                if let Ok(layout) = layout_of(tcx, param_env, ty) {
                    let layout = layout.layout;
                    if let rustc_target::abi::FieldsShape::Arbitrary {
                        offsets,
                        memory_index: _,
                    } = layout.fields()
                    {
                        for (field_idx, offset) in offsets.iter().enumerate() {
                            let field_ty = tuple_types[field_idx];
                            let byte_offset = offset.bytes_usize();
                            let proj = vec![PathSelector::Field(field_idx)];
                            fields_byte_offsets.insert(proj.clone(), byte_offset);
                            // analyse the subfield recursively
                            compute_subfields_offsets(proj, field_ty, byte_offset, &mut fields_byte_offsets);
                        }
                    }
                } else {
                    let fields = projections_and_types(tcx, ty);
                    for (field_proj, _field_ty) in fields {
                        fields_byte_offsets.insert(field_proj, 0);
                    }
                }
            }
            TyKind::Closure(..) => {
                // Closures have no layout guarantees. "https://doc.rust-lang.org/reference/type-layout.html"
                // We compute the byte offset directly based on the order of fields
                let closure_field_types = closure_field_types(ty);
                let mut byte_offset = 0;
                for (i, field_ty) in closure_field_types.iter().enumerate() {
                    let proj = vec![PathSelector::Field(i)];
                    fields_byte_offsets.insert(proj.clone(), byte_offset);
                    if let Ok(layout) = layout_of(tcx, param_env, *field_ty) {
                        compute_subfields_offsets(proj, *field_ty, byte_offset, &mut fields_byte_offsets);
                        byte_offset += layout.size.bytes_usize();
                    } 
                }
            }
            // Todo
            TyKind::Coroutine(..) | TyKind::CoroutineWitness(..) => {}
            _ => {}
        }
        self.field_byte_offset_cache.insert(ty, fields_byte_offsets);
    }
}



/// Manage the type cast for paths
pub struct PathCastCache<'tcx> {
    pub(crate) path_cast_types: HashMap<Rc<Path>, HashSet<Ty<'tcx>>>,
}

impl<'tcx> Default for PathCastCache<'tcx> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'tcx> PathCastCache<'tcx> {
    pub fn new() -> PathCastCache<'tcx> {
        PathCastCache {
            path_cast_types: HashMap::new(),
        }
    }

    /// Returns the types that a path may be cast to
    pub fn get_cast_types(&self, path: &Rc<Path>) -> Option<&HashSet<Ty<'tcx>>> {
        self.path_cast_types.get(path)
    }

    /// Creates a path that casts the given path to a given type
    pub fn cast_to(&mut self, acx: &mut AnalysisContext<'tcx, '_>, path: &Rc<Path>, ty: Ty<'tcx>) -> Option<Rc<Path>> {
        let path = Self::get_regularized_path(acx, path.clone());
        if path.is_constant() {
            return Some(path);
        }

        let original_ty = if let Some(ty) = acx.get_path_rustc_type(&path) {
            ty
        } else {
            let ty = try_eval_path_type(acx, &path).unwrap();
            acx.set_path_rustc_type(path.clone(), ty);
            ty
        };

        // To avoid infinite casts, we do not perfrom cast operations for the path with unknown type
        if original_ty == acx.tcx.types.never {
            return None;
        }
        if equal_types(acx.tcx, original_ty, ty) {
            return Some(path);
        } else {
            // When casting a pointer to a struct to its first field, we return the first field directly
            let fields_at_start_location =
                fields_at_start_location(acx.tcx, path.clone(), original_ty);
            for (field, field_ty) in fields_at_start_location {
                if equal_types(acx.tcx, field_ty, ty) {
                    return Some(field);
                }
            }

            let ty_index = acx.get_type_index(&ty);
            if let PathEnum::QualifiedPath { base: _, projection } = &path.value {
                for elem in projection {
                    if let PathSelector::Cast(index) = elem {
                        if *index == ty_index {
                            warn!(
                                "Warning: Potential recursive cast for casting path {:?} to type_{:?} {:?}",
                                path, ty_index, ty
                            );
                            return None;
                        }
                    }
                }
            }

            self.path_cast_types.entry(path.clone()).or_default().insert(ty);
            let cast_path = Path::append_projection_elem(&path, PathSelector::Cast(ty_index));
            acx.set_path_rustc_type(cast_path.clone(), ty);
            return Some(cast_path);
        }
    }

    // Returns the type variant of the given path, returns none if the path has not been cast to ty
    pub fn get_type_variant(&mut self, acx: &mut AnalysisContext<'tcx, '_>, path: &Rc<Path>, ty: Ty<'tcx>) -> Option<Rc<Path>> {
        let path = Self::get_regularized_path(acx, path.clone());
        let original_ty = if let Some(ty) = acx.get_path_rustc_type(&path) {
            ty
        } else {
            let ty = try_eval_path_type(acx, &path).unwrap();
            acx.set_path_rustc_type(path.clone(), ty);
            ty
        };

        if equal_types(acx.tcx, original_ty, ty) {
            return Some(path);
        } else {
            let fields_at_start_location =
                fields_at_start_location(acx.tcx, path.clone(), original_ty);
            for (field, field_ty) in fields_at_start_location {
                if equal_types(acx.tcx, field_ty, ty) {
                    return Some(field);
                }
            }

            let ty_index = acx.get_type_index(&ty);
            if let Some(cast_types) = self.path_cast_types.get(&path) {
                if cast_types.contains(&ty) {
                    let cast_path = Path::append_projection_elem(&path, PathSelector::Cast(ty_index));
                    return Some(cast_path);
                }
            }
            return None;
        }
    }


    /// Different paths may refer to the same memory location, we can regularize these path to a base path
    /// e.g. a.0.0, a.0, a.cast#T' and a are all represented by one path
    pub fn get_regularized_path(acx: &mut AnalysisContext<'tcx, '_>, path: Rc<Path>) -> Rc<Path> {
        if let PathEnum::QualifiedPath { base: _, projection } = &path.value {
            match projection.last().unwrap() {
                PathSelector::Cast(_) => {
                    // If this path is already a cast path, remove the last path selector
                    // to get the orginal path
                    Self::get_regularized_path(acx, Path::truncate_projection_elems(&path, projection.len() - 1))
                }
                PathSelector::Index | PathSelector::UnionField(..) => {
                    // If this path is an index path of an array, remove the index selector
                    Self::get_regularized_path(acx, Path::truncate_projection_elems(&path, projection.len() - 1))
                }
                PathSelector::Field(f) => {
                    // If this path is a field of a struct and the field's offset is 0,
                    // remove this field to get the base path
                    let struct_ty = if let Some(ty) = acx.get_path_rustc_type(&path) {
                        ty
                    } else {
                        let ty = try_eval_path_type(acx, &path).unwrap();
                        acx.set_path_rustc_type(path.clone(), ty);
                        ty
                    };
                    if acx.get_field_byte_offset(struct_ty, &vec![PathSelector::Field(*f)]) == 0 {
                        Self::get_regularized_path(
                            acx, 
                            Path::truncate_projection_elems(
                                &path,
                                projection.len() - 1
                            )
                        )
                    } else {
                        path
                    }
                }
                PathSelector::Downcast(_) => {
                    // If this path is an downcast path of an enum, remove the downcast selector
                    Self::get_regularized_path(acx, Path::truncate_projection_elems(&path, projection.len() - 1))
                }
                _ => path,
            }
        } else {
            path
        }
    }

}




/// Returns the target type of a reference type.
pub fn get_dereferenced_type(ty: Ty<'_>) -> Ty<'_> {
    match ty.kind() {
        TyKind::RawPtr(ty_and_mut) => ty_and_mut.ty,
        TyKind::Ref(_, t, _) => *t,
        _ => {
            if ty.is_box() {
                ty.boxed_ty()
            } else {
                ty
            }
        }
    }
}

/// Returns the element type of an array or slice type.
pub fn get_element_type<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Ty<'tcx> {
    match &ty.kind() {
        TyKind::Array(t, _) => *t,
        TyKind::RawPtr(TypeAndMut { ty: t, .. }) | TyKind::Ref(_, t, _) => match t.kind() {
            TyKind::Array(t, _) => *t,
            TyKind::Slice(t) => *t,
            TyKind::Str => tcx.types.char,
            _ => *t,
        },
        TyKind::Slice(t) => *t,
        TyKind::Str => tcx.types.char,
        _ => ty,
    }
}

/// Returns the type of the field with the given ordinal.
pub fn get_field_type<'tcx>(tcx: TyCtxt<'tcx>, base_ty: Ty<'tcx>, ordinal: usize) -> Ty<'tcx> {
    if let TyKind::Adt(def, args) = base_ty.kind() {
        if def.is_union() || def.is_struct() {
            let variant = def.variants().iter().next().expect("at least one variant");
            assert!(ordinal < variant.fields.len());
            let field = &variant.fields[ordinal.into()];
            let ft = field_ty(tcx, field, args);
            return ft;
        } else {
            warn!("Getting the field type with ordinal {:?} for a Enum type {:?} ", ordinal, base_ty);
            return tcx.types.never;
        }
    } else if let TyKind::Tuple(tuple_types) = base_ty.kind() {
        assert!(ordinal < tuple_types.len());
        let ft = tuple_types[ordinal];
        return ft;
    } else if base_ty.is_closure() || base_ty.is_coroutine() {
        let closure_field_types = closure_field_types(base_ty);
        debug!("Closure/Coroutine field types: {:?}", closure_field_types);
        assert!(ordinal < closure_field_types.len());
        return closure_field_types[ordinal];
    } else {
        warn!("Getting the field type for an unexpected type {:?} ", base_ty);
        return tcx.types.never;
    }
}

/// Returns the rustc TyKind of the downcast projection
pub fn get_downcast_type<'tcx>(tcx: TyCtxt<'tcx>, base_ty: Ty<'tcx>, variant_idx: VariantIdx) -> Ty<'tcx> {
    if let TyKind::Adt(def, args) = base_ty.kind() {
        let variant = if variant_idx.index() >= def.variants().len() {
            error!(
                "illegally down casting to index {} of {:?}",
                variant_idx.index(),
                base_ty,
            );
            // Return the last variant type of the enum
            &def.variants().iter().last().unwrap()
        } else {
            def.variant(variant_idx)
        };
        let field_tys = variant.fields.iter().map(|field| field_ty(tcx, field, args));
        Ty::new_tup_from_iter(tcx, field_tys)
    } else if let TyKind::Coroutine(def_id, args) = base_ty.kind() {
        let mut tuple_types = args.as_coroutine().state_tys(*def_id, tcx);
        if let Some(field_tys) = tuple_types.nth(variant_idx.index()) {
            return Ty::new_tup_from_iter(tcx, field_tys);
        }
        debug!(
            "illegally down casting to index {} of {:?}",
            variant_idx.index(),
            base_ty,
        );
        tcx.types.never
    } else {
        error!("unexpected type for downcast {:?}", base_ty);
        tcx.types.never
    }
}

pub fn field_ty<'tcx>(tcx: TyCtxt<'tcx>, field: &FieldDef, generic_args: GenericArgsRef<'tcx>) -> Ty<'tcx> {
    // let ft = field.ty(tcx, generic_args);
    let field_ty = tcx.type_of(field.did).skip_binder();
    let substs_specializer =
        SubstsSpecializer::new(tcx, generic_args.iter().map(|t| GenericArgE::from(&t)).collect());
    tcx.erase_regions_ty(substs_specializer.specialize_generic_argument_type(field_ty))
}

/// Returns false if any of the generic arguments are themselves generic
pub fn are_concrete(generic_args: GenericArgsRef<'_>) -> bool {
    for gen_arg in generic_args.iter() {
        if let GenericArgKind::Type(ty) = gen_arg.unpack() {
            if !is_concrete(ty.kind()) {
                return false;
            }
        }
    }
    true
}

/// Determines if the given type is fully concrete.
pub fn is_concrete(ty_kind: &TyKind<'_>) -> bool {
    match ty_kind {
        TyKind::Adt(_, gen_args)
        | TyKind::Closure(_, gen_args)
        | TyKind::FnDef(_, gen_args)
        | TyKind::Coroutine(_, gen_args)
        | TyKind::CoroutineWitness(_, gen_args)
        | TyKind::Alias(_, rustc_middle::ty::AliasTy { args: gen_args, .. }) => {
            are_concrete(gen_args)
        }
        TyKind::Tuple(types) => types.iter().all(|t| is_concrete(t.kind())),
        TyKind::Bound(..)
        | TyKind::Dynamic(..)
        | TyKind::Error(..)
        | TyKind::Infer(..)
        | TyKind::Param(..) => false,
        TyKind::Ref(_, ty, _) => is_concrete(ty.kind()),
        _ => true,
    }
}

/// Returns true if this id corresponds to the fn_trait|fn_mut_trait|fn_once_trait
pub fn is_fn_trait(tcx: TyCtxt<'_>, id: DefId) -> bool {
    let items = tcx.lang_items();
    match Some(id) {
        x if x == items.fn_trait() => true,
        x if x == items.fn_mut_trait() => true,
        x if x == items.fn_once_trait() => true,
        _ => false,
    }
}

/// Returns true if this type is `dyn Fn`, `dyn FnMut` or  `dyn FnOnce`.
pub fn is_dynamic_fn_trait<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    if let TyKind::Dynamic(trait_data, ..) = ty.kind() {
        if let Some(principal) = trait_data.principal() {
            let principal = tcx.normalize_erasing_late_bound_regions(ParamEnv::reveal_all(), principal);
            return is_fn_trait(tcx, principal.def_id);
        }
    }
    return false;
}

pub fn is_fn_once_output<'tcx>(tcx: TyCtxt<'tcx>, id: DefId) -> bool {
    let items = tcx.lang_items();
    match Some(id) {
        x if x == items.fn_once_output() => true,
        _ => false,
    }
}

pub fn is_fn_once_call_once<'tcx>(tcx: TyCtxt<'tcx>, id: DefId) -> bool {
    matches!(
        KnownNamesCache::get_known_name_for(tcx, id), 
        KnownNames::StdOpsFunctionFnOnceCallOnce
    ) 
}

/// Returns true if the given type is a reference (or raw pointer) to a collection type, in which
/// case the reference/pointer independently tracks the length of the collection, thus effectively
/// tracking a slice of the underlying collection.
pub fn is_slice_pointer<'tcx>(ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::RawPtr(TypeAndMut { ty: target, .. }) | TyKind::Ref(_, target, _) => {
            // Pointers to sized arrays are thin pointers.
            matches!(target.kind(), TyKind::Slice(..) | TyKind::Str)
        }
        _ => false,
    }
}

/// Returns true if the given type is a reference (or raw pointer) to a dynamic type
pub fn is_dynamic_pointer<'tcx>(ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::RawPtr(TypeAndMut { ty: target, .. }) | TyKind::Ref(_, target, _) => {
            // Pointers to sized arrays are thin pointers.
            matches!(target.kind(), TyKind::Dynamic(..))
        }
        _ => false,
    }
}

/// Returns true if the given type is a reference (or raw pointer) to a foreign type
pub fn is_foreign_pointer<'tcx>(ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::RawPtr(TypeAndMut { ty: target, .. }) | TyKind::Ref(_, target, _) => {
            // Pointers to sized arrays are thin pointers.
            matches!(target.kind(), TyKind::Foreign(..))
        }
        _ => false,
    }
}

/// Returns whether the type is a primitive type or an array or slice containing basic ty elements
/// e.g. u8, [u8], ()
pub fn is_basic_type(ty: Ty<'_>) -> bool {
    match ty.kind() {
        TyKind::Bool | TyKind::Char | TyKind::Int(_) | TyKind::Uint(_) | TyKind::Float(_) => true,
        TyKind::Str => true,
        TyKind::Array(elem_ty, _) | TyKind::Slice(elem_ty) => is_basic_type(*elem_ty),
        TyKind::Tuple(ty_list) => ty_list.is_empty(),
        _ => false,
    }
}

/// Returns whether the type is a pointer to a basic type
/// e.g. *u8, *[u8], *()
pub fn is_basic_pointer(ty: Ty<'_>) -> bool {
    if !ty.is_any_ptr() {
        false
    } else {
        is_basic_type(get_dereferenced_type(ty))
    }
}

/// repr(transparent) is used on structs with a single non-zero-sized field (there may be
/// additional zero-sized fields).
/// Get the type and field index after removing the transparent wrapper
pub fn remove_transparent_wrapper<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Option<(PathSelector, Ty<'tcx>)> {
    if let TyKind::Adt(def, args) = ty.kind() {
        if def.repr().transparent() {
            if def.is_union() || def.is_struct() {
                let param_env = rustc_middle::ty::ParamEnv::reveal_all();
                let variant = def.variants().iter().next().expect("at least one variant");
                let non_zst_field = variant.fields.iter().enumerate().find(|(_i, field)| {
                    let field_ty = tcx.type_of(field.did).skip_binder();
                    let is_zst = layout_of(tcx, param_env, field_ty)
                        .map_or(false, |layout| layout.is_zst());
                    !is_zst
                });
                if let Some((i, field)) = non_zst_field {
                    if def.is_union() {
                        return Some((PathSelector::UnionField(i), field_ty(tcx, field, args)));
                    } else {
                        return Some((PathSelector::Field(i), field_ty(tcx, field, args)));
                    }
                }
            }
        }
    }
    None
}

pub fn is_transparent_wrapper(ty: Ty) -> bool {
    return if let TyKind::Adt(def, _) = ty.kind() {
        def.repr().transparent()
    } else {
        false
    };
}

pub fn function_return_type<'tcx>(tcx: TyCtxt<'tcx>, def_id: DefId, gen_args: GenericArgsRef<'tcx>) -> Ty<'tcx> {
    let fn_sig = tcx.fn_sig(def_id);
    let ret_type = fn_sig.skip_binder().output().skip_binder();
    let generic_args = gen_args.iter().map(|t| GenericArgE::from(&t)).collect();
    let substs_specializer = SubstsSpecializer::new(tcx, generic_args);
    substs_specializer.specialize_generic_argument_type(ret_type)
}

pub fn closure_return_type<'tcx>(tcx: TyCtxt<'tcx>, _def_id: DefId, gen_args: GenericArgsRef<'tcx>) -> Ty<'tcx> {
    let fn_sig = gen_args.as_closure().sig();
    let ret_type = fn_sig.skip_binder().output();
    let generic_args = gen_args.iter().map(|t| GenericArgE::from(&t)).collect();
    let substs_specializer = SubstsSpecializer::new(tcx, generic_args);
    substs_specializer.specialize_generic_argument_type(ret_type)
}

/// Closures bring enclosed variables with them that are effectively additional parameters.
/// There is no convenient way to look up their types later on. I.e. unlike ordinary parameters
/// whose types can be looked up in mir.local_decls, these extra parameters need their
/// types extracted from the closure type definitions via the tricky logic below.
pub fn closure_field_types<'tcx>(ty: Ty<'tcx>) -> Vec<Ty<'tcx>> {
    match ty.kind() {
        TyKind::Closure(_, args) => {
            return args.as_closure().upvar_tys().iter().collect::<Vec<Ty<'tcx>>>();
        }
        TyKind::Coroutine(_, args) => {
            return args.as_coroutine().prefix_tys().iter().collect::<Vec<Ty<'tcx>>>();
        }
        _ => {
            unreachable!("unexpected type {:?}", ty);
        }
    }
}

/// Returns a vector of field projections paired with their corresponding types contained in the given type 
pub fn projections_and_types<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Vec<(ProjectionElems, Ty<'tcx>)> {
    let mut fields = Vec::new();
    match ty.kind() {
        TyKind::Adt(adt_def, args) if adt_def.is_struct() || adt_def.is_union() => {
            // If this adt is a struct or union, there will be a single variant containing all the fields.
            let variant = adt_def.variants().iter().next().expect("at least one variant");
            for (i, field) in variant.fields.iter().enumerate() {
                let field_ty = field_ty(tcx, field, args);
                let field = if adt_def.is_struct() {
                    PathSelector::Field(i)
                } else {
                    PathSelector::UnionField(i)
                };
                fields.push((vec![field], field_ty));
                // recursively get the subfields of this field
                let subfields = projections_and_types(tcx, field_ty);
                for (mut subfield, subfield_ty) in subfields {
                    subfield.insert(0, field);
                    fields.push((subfield, subfield_ty));
                }
            }
        }
        TyKind::Adt(adt_def, args) if adt_def.is_enum() => {
            if !adt_def.variants().is_empty() {
                adt_def
                    .variants()
                    .iter()
                    .enumerate()
                    .for_each(|(variant_idx, variant)| {
                        let downcast = PathSelector::Downcast(variant_idx);
                        for (i, field) in variant.fields.iter().enumerate() {
                            let field_ty = field_ty(tcx, field, args);
                            let field = PathSelector::Field(i);
                            fields.push((vec![downcast, field], field_ty));
                            // recursively get the subfields of this field
                            let subfields = projections_and_types(tcx, field_ty);
                            for (mut subfield, subfield_ty) in subfields {
                                let mut projection = vec![downcast, field];
                                projection.append(&mut subfield);
                                fields.push((projection, subfield_ty));
                            }
                        }
                    });
            }
        }
        TyKind::Array(elem_ty, _) | TyKind::Slice(elem_ty) => {
            fields.push((vec![PathSelector::Index], *elem_ty));
            // recursively get the pointer type subfields of the array element
            let subfields = projections_and_types(tcx, *elem_ty);
            for (mut subfield, subfield_ty) in subfields {
                subfield.insert(0, PathSelector::Index);
                fields.push((subfield, subfield_ty));
            }
        }
        TyKind::Closure(..) | TyKind::Coroutine(..) => {
            let closure_field_types = closure_field_types(ty);
            for (i, field_ty) in closure_field_types.iter().enumerate() {
                let field = PathSelector::Field(i);
                fields.push((vec![field], *field_ty));
                // recursively get the pointer type subfields of this field
                let subfields = projections_and_types(tcx, *field_ty);
                for (mut subfield, subfield_ty) in subfields {
                    subfield.insert(0, field);
                    fields.push((subfield, subfield_ty));
                }
            }
        }
        TyKind::Tuple(tuple_types) => {
            tuple_types.iter().enumerate().for_each(|(i, field_ty)| {
                fields.push((vec![PathSelector::Field(i)], field_ty));
                // recursively get the pointer type subfields of this field
                let subfields = projections_and_types(tcx, field_ty);
                for (mut subfield, subfield_ty) in subfields {
                    subfield.insert(0, PathSelector::Field(i));
                    fields.push((subfield, subfield_ty));
                }
            });
        }
        TyKind::Alias(kind, ty) => {
            warn!("unnormalized alias type: {:?}({:?})", ty, kind);
        }
        _ => {}
    }
    return fields;
}

/// Returns all the projections of pointer type fields contained in the given type
pub fn get_pointer_projections<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Vec<(ProjectionElems, Ty<'tcx>)> {
    let mut ptr_projs = Vec::new();
    match ty.kind() {
        TyKind::Adt(adt_def, args) if adt_def.is_struct() || adt_def.is_union() => {
            // If this adt is a struct or union, there will be a single variant containing all the fields.
            let variant = adt_def.variants().iter().next().expect("at least one variant");
            for (i, field) in variant.fields.iter().enumerate() {
                let field_ty = field_ty(tcx, field, args);
                let field = if adt_def.is_struct() {
                    PathSelector::Field(i)
                } else {
                    PathSelector::UnionField(i)
                };
                if field_ty.is_any_ptr() {
                    ptr_projs.push((vec![field], field_ty));
                } else {
                    // recursively get the pointer type subfields of this field
                    for (mut subfield, subfield_ty) in get_pointer_projections(tcx, field_ty) {
                        subfield.insert(0, field);
                        ptr_projs.push((subfield, subfield_ty));
                    }
                }
            }
        }
        TyKind::Adt(adt_def, args) if adt_def.is_enum() => {
            if !adt_def.variants().is_empty() {
                adt_def
                    .variants()
                    .iter()
                    .enumerate()
                    .for_each(|(variant_idx, variant)| {
                        let downcast = PathSelector::Downcast(variant_idx);
                        for (i, field) in variant.fields.iter().enumerate() {
                            let field_ty = field_ty(tcx, field, args);
                            let field = PathSelector::Field(i);
                            if field_ty.is_any_ptr() {
                                ptr_projs.push((vec![downcast, field], field_ty));
                            } else {
                                // recursively get the pointer type subfields of this field
                                for (mut subfield, subfield_ty) in get_pointer_projections(tcx, field_ty) {
                                    let mut projection = vec![downcast, field];
                                    projection.append(&mut subfield);
                                    ptr_projs.push((projection, subfield_ty));
                                }
                            }
                        }
                    });
            }
        }
        TyKind::Array(elem_ty, _) | TyKind::Slice(elem_ty) => {
            // Slice is the pointee of an array slice. Written as [T].
            // It doesn't have a size known at compile-time, therefore it must be referenced in program.
            if elem_ty.is_any_ptr() {
                ptr_projs.push((vec![PathSelector::Index], *elem_ty));
            } else {
                // recursively get the pointer type subfields of the array element
                for (mut subfield, subfield_ty) in get_pointer_projections(tcx, *elem_ty) {
                    subfield.insert(0, PathSelector::Index);
                    ptr_projs.push((subfield, subfield_ty));
                }
            }
        }
        TyKind::Closure(..) | TyKind::Coroutine(..) => {
            let closure_field_types = closure_field_types(ty);
            // The generic argments of the closure type should have been specialized, therefore the
            // field_ty don't need to be specialized again.
            for (i, field_ty) in closure_field_types.iter().enumerate() {
                let field = PathSelector::Field(i);
                if field_ty.is_any_ptr() {
                    ptr_projs.push((vec![field], *field_ty));
                } else {
                    // recursively get the pointer type subfields of this field
                    for (mut subfield, subfield_ty) in get_pointer_projections(tcx, *field_ty) {
                        subfield.insert(0, field);
                        ptr_projs.push((subfield, subfield_ty));
                    }
                }
            }
        }
        TyKind::Tuple(tuple_types) => {
            tuple_types.iter().enumerate().for_each(|(i, field_ty)| {
                if field_ty.is_any_ptr() {
                    ptr_projs.push((vec![PathSelector::Field(i)], field_ty));
                } else {
                    // recursively get the pointer type subfields of this field
                    for (mut subfield, subfield_ty) in get_pointer_projections(tcx, field_ty) {
                        subfield.insert(0, PathSelector::Field(i));
                        ptr_projs.push((subfield, subfield_ty));
                    }
                }
            });
        }
        TyKind::Alias(kind, ty) => {
            warn!("unnormalized alias type: {:?}({:?})", ty, kind);
        }
        _ => {}
    }
    ptr_projs
}

#[inline]
pub fn get_array_length<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env: ParamEnv<'tcx>,
    length: &'tcx Const<'tcx>,
) -> usize {
    if let Some(val) = length.try_eval_target_usize(tcx, param_env) {
        val as usize
    } else {
        // if the value cannot be evaluated or doesn’t contain a valid usize,
        // e.g. unevaluated generic const value, we just return 1
        1
    }
}

/// Returns a layout for the given type, if concrete.
#[inline]
pub fn layout_of<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env: ParamEnv<'tcx>,
    ty: Ty<'tcx>,
) -> std::result::Result<
    rustc_middle::ty::layout::TyAndLayout<'tcx>,
    &'tcx rustc_middle::ty::layout::LayoutError<'tcx>,
> {
    tcx.layout_of(param_env.and(ty))
}

/// Returns the size for the given type
#[inline]
pub fn size_of<'tcx>(tcx: TyCtxt<'tcx>, param_env: ParamEnv<'tcx>, ty: Ty<'tcx>) -> usize {
    let layout = layout_of(tcx, param_env, ty)
        .expect("Failed to get the layout of the type.")
        .layout;
    layout.size().bytes_usize()
}

/// Given an object that may contain nested objects, flatten it by extracting all the bottom-level subobjects. 
/// This function returns a vector of tuples, each including a subobject's memory offset from the base object,
/// its path representation and its type.
pub fn flatten_fields<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env: ParamEnv<'tcx>,
    path: Rc<Path>,
    path_ty: Ty<'tcx>,
) -> Vec<(usize, Rc<Path>, Ty<'tcx>)> {
    let mut flattened_fields = Vec::new();
    flatten_fields_recursively(tcx, param_env, path, path_ty, 0, &mut flattened_fields);
    return flattened_fields;
}

fn flatten_fields_recursively<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env: ParamEnv<'tcx>,
    path: Rc<Path>,
    path_ty: Ty<'tcx>,
    base_offset: usize,
    flattened_fields: &mut Vec<(usize, Rc<Path>, Ty<'tcx>)>,
) {
    match path_ty.kind() {
        TyKind::Adt(adt_def, args) => {
            if adt_def.is_enum() {
                // Todo: we currently do not flatten a enum type variable
                flattened_fields.push((base_offset, path, path_ty));
                return;
            }
            if adt_def.is_union() {
                // Todo
                // We currently only push the first non-zero-sized field into the flattened_fields now.
                // This solution is sound for most of the cases, especially for handling transparent union.
                let variant = adt_def.variants().iter().next().expect("at least one variant");
                let non_zst_field = variant.fields.iter().enumerate().find(|(_i, field)| {
                    let field_ty = tcx.type_of(field.did).skip_binder();
                    let is_zst = tcx
                        .layout_of(param_env.and(field_ty))
                        .map_or(false, |layout| layout.is_zst());
                    !is_zst
                });
                if let Some((i, field)) = non_zst_field {
                    let field_path = Path::new_union_field(path.clone(), i);
                    let field_ty = field_ty(tcx, field, args);
                    flatten_fields_recursively(
                        tcx,
                        param_env,
                        field_path,
                        field_ty,
                        base_offset,
                        flattened_fields,
                    );
                }
                return;
            }
            if !adt_def.variants().is_empty() { // Struct
                // The layout of an adt type is not guaranteed to be identical to the definition
                // of the type. We need to flatten the fields according to the layout of fields
                if let Ok(layout) = layout_of(tcx, param_env, path_ty) {
                    let layout = layout.layout;
                    let variant = adt_def.variants().iter().next().expect("at least one variant");
                    if let rustc_target::abi::FieldsShape::Arbitrary {
                        offsets,
                        memory_index,
                    } = layout.fields()
                    {
                        for index in memory_index {
                            let index = *index as usize;
                            let field = &variant.fields[index.into()];
                            let field_path = Path::new_field(path.clone(), index);
                            let field_ty = field_ty(tcx, field, args);
                            let offset = offsets[index.into()].bytes_usize() + base_offset;
                            flatten_fields_recursively(
                                tcx,
                                param_env,
                                field_path,
                                field_ty,
                                offset,
                                flattened_fields,
                            );
                        }
                    } 
                } else {
                    // Todo: for a struct we fail to obtain its layout,  we can assume that all its fields 
                    // are stored sequentially in memory.
                    warn!("Failed to get the layout of the adt type: {:?}", path_ty);
                }
            }
        }
        TyKind::Array(elem_ty, length) => {
            let length = get_array_length(tcx, param_env, length);
            let index_path = Path::new_index(path);
            let elem_size = size_of(tcx, param_env, *elem_ty);
            let mut offset = base_offset;
            for _i in 0..length {
                flatten_fields_recursively(
                    tcx,
                    param_env,
                    index_path.clone(),
                    *elem_ty,
                    offset,
                    flattened_fields,
                );
                offset += elem_size;
            }
        }
        TyKind::Tuple(types) => {
            if let Ok(layout) = layout_of(tcx, param_env, path_ty) {
                let layout = layout.layout;
                if let rustc_target::abi::FieldsShape::Arbitrary {
                    offsets,
                    memory_index,
                } = layout.fields()
                {
                    for index in memory_index {
                        let index = *index as usize;
                        let field_path = Path::new_field(path.clone(), index);
                        let field_ty = types[index];
                        let offset = offsets[index.into()].bytes_usize() + base_offset;
                        flatten_fields_recursively(
                            tcx,
                            param_env,
                            field_path,
                            field_ty,
                            offset,
                            flattened_fields,
                        );
                    }
                }
            } else {
                // Todo
                warn!("Failed to get the layout of the tuple type: {:?}", path_ty);
            }
        }
        TyKind::Slice(elem_ty) => {
            // The length of a slice is unknown at compile time
            let index_path = Path::new_index(path);
            flatten_fields_recursively(
                tcx,
                param_env,
                index_path.clone(),
                *elem_ty,
                base_offset,
                flattened_fields,
            );
        }
        _ => {
            // We do not further flatten a fat pointer (pointers to slice, str or dynamic types), which 
            // consists of data pointer and vtable pointer. This does not impact the soundness of the analysis.
            // For example, if we are going to transmute a slice reference type to (*const u32, usize),
            // we can propagate the pointees correctly while ignoring the length metadata.
            flattened_fields.push((base_offset, path, path_ty));
        }
    }
}

pub fn fields_at_start_location<'tcx>(
    tcx: TyCtxt<'tcx>,
    path: Rc<Path>,
    path_ty: Ty<'tcx>,
) -> Vec<(Rc<Path>, Ty<'tcx>)> {
    let param_env = rustc_middle::ty::ParamEnv::reveal_all();
    let mut fields_at_start_location = Vec::new();
    find_fields_at_start_location(tcx, param_env, path, path_ty, &mut fields_at_start_location);
    return fields_at_start_location;
}

fn find_fields_at_start_location<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env: ParamEnv<'tcx>,
    path: Rc<Path>,
    path_ty: Ty<'tcx>,
    fields_at_start_location: &mut Vec<(Rc<Path>, Ty<'tcx>)>,
) {
    match path_ty.kind() {
        TyKind::Adt(adt_def, args) => {
            if adt_def.is_enum() {
                return;
            }
            if adt_def.is_union() {
                // Add all union fields to the vector
                let variant = adt_def.variants().iter().next().expect("at least one variant");
                variant.fields.iter().enumerate().for_each(|(i, field)| {
                    let field_path = Path::new_union_field(path.clone(), i);
                    let field_ty = field_ty(tcx, field, args);
                    fields_at_start_location.push((field_path.clone(), field_ty));
                    find_fields_at_start_location(
                        tcx,
                        param_env,
                        field_path,
                        field_ty,
                        fields_at_start_location,
                    );
                });
                return;
            }
            if !adt_def.variants().is_empty() {
                let variant = adt_def.variants().iter().next().expect("at least one variant");
                if variant.fields.is_empty() {
                    return;
                }
                if let Ok(layout) = layout_of(tcx, param_env, path_ty) {
                    let layout = layout.layout;
                    if let rustc_target::abi::FieldsShape::Arbitrary {
                        offsets,
                        memory_index,
                    } = layout.fields()
                    {
                        // There may be multiple fields at start memeory location as a field can be zero sized.
                        for index in memory_index {
                            let index = *index as usize;
                            let offset = offsets[index.into()].bytes_usize();
                            if offset == 0 {
                                let field = &variant.fields[index.into()];
                                let field_path = Path::new_field(path.clone(), index);
                                let field_ty = field_ty(tcx, field, args);
                                fields_at_start_location.push((field_path.clone(), field_ty));
                                find_fields_at_start_location(
                                    tcx,
                                    param_env,
                                    field_path,
                                    field_ty,
                                    fields_at_start_location,
                                );
                            }
                        }
                    } 
                } else {
                    // Todo
                    // If we cannot obtain the layout of the struct, add the first field directly
                }
            }
        }
        TyKind::Array(elem_ty, _) | TyKind::Slice(elem_ty) => {
            let index_path = Path::new_index(path);
            fields_at_start_location.push((index_path.clone(), *elem_ty));
            find_fields_at_start_location(
                tcx,
                param_env,
                index_path,
                *elem_ty,
                fields_at_start_location,
            );
        }
        TyKind::Tuple(types) => {
            if let Ok(layout) = layout_of(tcx, param_env, path_ty) {
                let layout = layout.layout;
                if let rustc_target::abi::FieldsShape::Arbitrary {
                    offsets,
                    memory_index,
                } = layout.fields()
                {
                    // There may be multiple fields at start memeory location as a field can be zero sized.
                    for index in memory_index {
                        let index = *index as usize;
                        let offset = offsets[index.into()].bytes_usize();
                        if offset == 0 {
                            let field_path = Path::new_field(path.clone(), index);
                            let field_ty = types[index];
                            fields_at_start_location.push((field_path.clone(), field_ty));
                            find_fields_at_start_location(
                                tcx,
                                param_env,
                                field_path,
                                field_ty,
                                fields_at_start_location,
                            );
                        }
                    }
                } 
            } else {
                // Todo
                warn!("Failed to get the layout of the tuple type: {:?}", path_ty);
            }
        }
        _ => {}
    }
}


/// Returns true if the two given types are equal after erasing regions
pub fn equal_types<'tcx>(tcx: TyCtxt<'tcx>, ty1: Ty<'tcx>, ty2: Ty<'tcx>) -> bool {
    let ty1 = tcx.erase_regions_ty(ty1);
    let ty2 = tcx.erase_regions_ty(ty2);
    // Todo: strip_const_generics
    // As we may infer the const generic arguments incorrectly, we should ignore them
    // when comparing the types.
    if let TyKind::Array(elem_ty1, _) = ty1.kind() {
        if let TyKind::Array(elem_ty2, _) = ty2.kind() {
            return equal_types(tcx, *elem_ty1, *elem_ty2);
        }
    }
    if let TyKind::Slice(elem_ty1) = ty1.kind() {
        if let TyKind::Slice(elem_ty2) = ty2.kind() {
            return equal_types(tcx, *elem_ty1, *elem_ty2);
        }
    }
    return ty1 == ty2;
}

/// Returns true if the given two pointer types are equivalent.
/// We suppose that a reference type and a mut/const raw pointer type are equivalent if
/// their dereference types are equivalent.  
/// Pointers of equivalent types can point to the same object.
pub fn equivalent_ptr_types<'tcx>(tcx: TyCtxt<'tcx>, ty1: Ty<'tcx>, ty2: Ty<'tcx>) -> bool {
    if !ty1.is_any_ptr() || !ty2.is_any_ptr() {
        return false;
    }
    // Note: Despite that a function pointer and a function's reference both point to function
    // items, they are not equivalent and they cannot be cast to each other.
    // For example,
    // ```
    // let f = foo;  // f: fn() {foo}
    // let p = &f;   // p: &fn() {foo}
    // let fp: fn() = unsafe { std::mem::transmute(p) };
    // fp();         // Segmentation fault
    // ```
    // If we cast a function's reference into a function pointer and call via the function pointer, 
    // it will lead to a segmentation fault.
    // The equivalence between two function pointers are determined by comparing their signatures.
    if ty1.is_fn_ptr() && ty2.is_fn_ptr() {
        let fn_sig1 = ty1.fn_sig(tcx);
        let fn_sig2 = ty2.fn_sig(tcx);
        return fn_sig1.inputs_and_output() == fn_sig2.inputs_and_output();
    }
    let deref_ty1 = get_dereferenced_type(ty1);
    let deref_ty2 = get_dereferenced_type(ty2);
    if !deref_ty1.is_any_ptr() || !deref_ty2.is_any_ptr() {
        // We don't make cast or do type filtering when we propagate the points-to set to a dyn trait 
        // pointer, therefore we treat a pointer and a dyn trait pointer as equilvalent pointers.
        if deref_ty1.is_trait() || deref_ty2.is_trait() {
            return true;
        } else if deref_ty1.is_closure() && deref_ty2.is_closure() {
            // Todo: two same closure types may be unequal
            return true;
        } else {
            return equal_types(tcx, deref_ty1, deref_ty2);
        }
    } else {
        return equivalent_ptr_types(tcx, deref_ty1, deref_ty2);
    }
}

pub fn eval_local_decl_type<'tcx>(
    acx: &mut AnalysisContext<'tcx, '_>,
    func_id: FuncId,
    ordinal: usize,
) -> Ty<'tcx> {
    let def_id = acx.get_function_reference(func_id).def_id;
    let mir = acx.tcx.optimized_mir(def_id);
    let substs_specializer =
        SubstsSpecializer::new(acx.tcx, acx.get_function_reference(func_id).generic_args.clone());
    substs_specializer.specialize_generic_argument_type(mir.local_decls[ordinal.into()].ty)
}

pub fn try_eval_path_type<'tcx>(acx: &mut AnalysisContext<'tcx, '_>, path: &Rc<Path>) -> Option<Ty<'tcx>> {
    if let Some(ty) = acx.get_path_rustc_type(&path) {
        return Some(ty);
    }

    match &path.value {
        PathEnum::Auxiliary { .. }
        | PathEnum::PromotedConstant { .. }
        | PathEnum::Function(..)
        | PathEnum::Type(..)
        | PathEnum::PromotedArgumentV1Array
        | PathEnum::PromotedStrRefArray => {
            unreachable!(
                "All auxiliary variables, promoted constants and function paths' 
                          types should have been cached when creating the paths."
            );
        }
        PathEnum::OffsetPath { base: _, offset: _ } => {
            // There is no fix type for a offset path since different qualified paths with different types
            // maybe represented by the same offset path
            None
        }
        PathEnum::LocalVariable { func_id, ordinal } | PathEnum::Parameter { func_id, ordinal } => {
            Some(eval_local_decl_type(acx, *func_id, *ordinal))
        }
        PathEnum::ReturnValue { func_id } => Some(eval_local_decl_type(acx, *func_id, 0)),
        PathEnum::HeapObj { .. } => Some(acx.tcx.types.u8),
        PathEnum::Constant => None,
        PathEnum::StaticVariable { def_id } => Some(acx.tcx.type_of(def_id).skip_binder()),
        PathEnum::QualifiedPath { base, projection } => {
            let mut base_ty = try_eval_path_type(acx, base).expect("Unable to evaluate the base type");
            let mut projection = &projection[..];
            while !projection.is_empty() {
                let projection_elem = projection.first().unwrap();
                let projection_ty = match projection_elem {
                    PathSelector::Deref if base_ty.is_any_ptr() => get_dereferenced_type(base_ty),
                    PathSelector::Field(ordinal) | PathSelector::UnionField(ordinal) => {
                        match base_ty.kind() {
                            TyKind::Adt(..)
                            | TyKind::Tuple(..)
                            | TyKind::Closure(..)
                            | TyKind::Coroutine(..) => {
                                let ft = get_field_type(acx.tcx, base_ty, *ordinal);
                                ft
                            }
                            _ => {
                                return Some(acx.tcx.types.never);
                            }
                        }
                    }
                    PathSelector::Index => {
                        match base_ty.kind() {
                            // the type `Str` cannot be indexed
                            TyKind::Array(..) | TyKind::Slice(..) => get_element_type(acx.tcx, base_ty),
                            _ => {
                                unreachable!();
                            }
                        }
                    }
                    PathSelector::Subslice { .. } => base_ty,
                    PathSelector::Downcast(ordinal) => {
                        get_downcast_type(acx.tcx, base_ty, (*ordinal).into())
                    }
                    PathSelector::Cast(type_index) => acx
                        .get_type_by_index(*type_index)
                        .expect("Casted type must have been cached."),
                    _ => {
                        return Some(acx.tcx.types.never);
                    }
                };
                base_ty = projection_ty;
                projection = &projection[1..];
            }
            Some(base_ty)
        }
    }
}

pub fn is_str_ref_array(ty: Ty<'_>) -> bool {
    if let TyKind::Array(elem_ty, _) = ty.kind() {
        if let TyKind::Ref(_, t, _) = elem_ty.kind() {
            if matches!(t.kind(), TyKind::Str) {
                return true;
            }
        }
    }
    return false;
}

pub fn is_argumentv1_array(ty: Ty<'_>) -> bool {
    if let TyKind::Array(elem_ty, _) = ty.kind() {
        if format!("{:?}", elem_ty) == "std::fmt::ArgumentV1" {
            return true;
        }
    }
    return false;
}

pub fn matched_fn_sig<'tcx>(tcx: TyCtxt<'tcx>, fn_sig1: PolyFnSig<'tcx>, fn_sig2: PolyFnSig<'tcx>) -> bool {
    let inputs_and_output1 = fn_sig1.inputs_and_output().skip_binder();
    let inputs_and_output2 = fn_sig2.inputs_and_output().skip_binder();
    if inputs_and_output1.len() != inputs_and_output2.len() {
        return false;
    }
    for i in 0..inputs_and_output1.len() {
        let ty1 = inputs_and_output1[i];
        let ty2 = inputs_and_output2[i];
        if ty1.is_any_ptr() && ty2.is_any_ptr() {
            // continue;
            if equivalent_ptr_types(tcx, ty1, ty2) {
                continue;
            } else if is_foreign_pointer(ty1) || is_foreign_pointer(ty2) {
                continue;
            }
        }
        if matches!(ty1.kind(), TyKind::Foreign(..)) || matches!(ty2.kind(), TyKind::Foreign(..)) {
            continue;
        }
        if matches!(ty1.kind(), TyKind::Alias(..)) || matches!(ty2.kind(), TyKind::Alias(..)) {
            continue;
        }
        if !equal_types(tcx, ty1, ty2) {
            return false;
        }
    }
    return true;
}

// Given a dynamic type like "dyn Trait + Send", return the dynamic type "dyn Trait"
pub fn strip_auto_traits<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Ty<'tcx> {
    if let TyKind::Dynamic(predicates, region, kind) = ty.kind() {
        let new_predicates = predicates.iter().filter(
            |bound_pred: &rustc_middle::ty::Binder<'_, ExistentialPredicate<'tcx>>| {
                match bound_pred.skip_binder() {
                    ExistentialPredicate::AutoTrait(_) => false,
                    _ => true,
                }
            }
        );
        Ty::new_dynamic(
            tcx,
            tcx.mk_poly_existential_predicates_from_iter(new_predicates),
            *region,
            *kind,
        )
    } else {
        ty
    }
}
