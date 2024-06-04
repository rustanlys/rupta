// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use std::fmt::{Debug, Formatter, Result};
use std::rc::Rc;

use log::*;
use rustc_hir::def_id::DefId;
use rustc_middle::mir::Location;
use rustc_middle::ty::Ty;

use crate::graph::pag::PAGPath;
use crate::mir::context::ContextId;
use crate::mir::function::FuncId;
use crate::util::type_util;

use super::function::CSFuncId;
use super::analysis_context::AnalysisContext;

/// Byte offset of metadata in fat pointer
const PTR_METADATA_OFFSET: usize = 8;

/// A non-empty list of projections
pub type ProjectionElems = Vec<PathSelector>;

/// The customized representation for a local variable, heap objects, ...
/// 
/// Resembles the `Place` type in rustc.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Path {
    pub value: PathEnum,
}

impl Debug for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        self.value.fmt(f)
    }
}

/// A path augmented with a context.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CSPath {
    pub cid: ContextId,
    pub path: Rc<Path>,
}

impl CSPath {
    pub fn new_cs_path(cid: ContextId, path: Rc<Path>) -> Rc<CSPath> {
        Rc::new(CSPath {
            cid, path
        })
    }
}

/// Different kinds of `Path` used in our analysis.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum PathEnum {
    /// Locals [arg_count+1..] are the local variables and compiler temporaries.
    LocalVariable {
        func_id: FuncId,
        ordinal: usize,
    },

    /// Locals [1..=arg_count] are the parameters
    Parameter {
        func_id: FuncId,
        ordinal: usize,
    },

    /// Local 0 is the return value temporary
    ReturnValue {
        func_id: FuncId,
    },

    /// Auxiliary local variable created when running pointer analysis
    Auxiliary {
        func_id: FuncId,
        ordinal: usize,
    },

    /// A dynamically allocated memory object.
    HeapObj {
        func_id: FuncId,
        location: Location,
    },

    /// This path points to data that is not used, but exists only to satisfy a static checker
    /// that a generic parameter is actually used.
    // PhantomData,

    Constant,

    StaticVariable {
        def_id: DefId,
    },

    /// The ordinal is an index into a method level table of MIR bodies.
    PromotedConstant {
        def_id: DefId,
        ordinal: usize,
    },

    /// The base denotes some struct, collection or heap_obj.
    /// projection: a non-empty list of projections
    QualifiedPath {
        base: Rc<Path>,
        projection: ProjectionElems,
    },

    OffsetPath {
        base: Rc<Path>,
        offset: usize,
    },

    /// A function instance which can be pointed to by a function pointer.
    Function(FuncId),

    PromotedStrRefArray,

    PromotedArgumentV1Array,

    /// A type instance uniquely identified by the type's index in type cache
    Type(usize),
}

impl Debug for PathEnum {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            PathEnum::LocalVariable { func_id, ordinal } => {
                f.write_fmt(format_args!("{:?}::local_{}", func_id, ordinal))
            }
            PathEnum::Parameter { func_id, ordinal } => {
                f.write_fmt(format_args!("{:?}::param_{}", func_id, ordinal))
            }
            PathEnum::ReturnValue { func_id } => f.write_fmt(format_args!("{:?}::ret", func_id)),
            PathEnum::Auxiliary { func_id, ordinal } => {
                f.write_fmt(format_args!("{:?}::aux_{}", func_id, ordinal))
            }
            PathEnum::HeapObj { func_id, location } => {
                f.write_fmt(format_args!("{:?}::heap_{:?}", func_id, location))
            }
            // PathEnum::PhantomData => f.write_str("phantom_data"),
            PathEnum::Constant => f.write_fmt(format_args!("constant")),
            PathEnum::StaticVariable { def_id } => {
                let def_id_str = format!("{:?}", def_id);
                let mut static_variable_name = def_id_str.split("::").last().unwrap();
                static_variable_name = &static_variable_name[..static_variable_name.len() - 1];
                f.write_fmt(format_args!("static_variable::{}", static_variable_name))
            }
            PathEnum::PromotedConstant { def_id, ordinal } => {
                f.write_fmt(format_args!("{:?}::promoted_{}", def_id, ordinal))
            }
            PathEnum::QualifiedPath { base, projection } => f.write_fmt(format_args!(
                "{:?}.{}",
                base,
                projection
                    .iter()
                    .map(|x| format!("{x:?}"))
                    .collect::<Vec<String>>()
                    .join(".")
            )),
            PathEnum::OffsetPath { base, offset } => f.write_fmt(format_args!("{:?}.ofs({})", base, offset)),
            PathEnum::Function(func_id) => f.write_fmt(format_args!("{:?}", func_id)),
            PathEnum::PromotedArgumentV1Array => f.write_fmt(format_args!("ArgumentV1Arr")),
            PathEnum::PromotedStrRefArray => f.write_fmt(format_args!("StrRefArr")),
            PathEnum::Type(type_id) => f.write_fmt(format_args!("Ty({:?})", type_id)),
        }
    }
}

/// The PathSelector denotes a de-referenced item, field, or element, or slice.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum PathSelector {
    /// Given a path that denotes a reference, select the thing the reference points to.
    Deref,

    /// Select the struct field with the given index.
    Field(usize),

    /// Selects a particular type case from a type union.
    /// all fields of a union share common storage. As a result, writes to one field of
    /// a union can overwrite its other fields, and size of a union is determined by the
    /// size of its largest field.
    UnionField(usize),

    /// For each field of a union, we connect it with a union offset path. Different fields
    /// at the same position connect to the same union offset path. Therefore, when writing
    /// to one field of a union, we can update the other fields at the same offset.
    // UnionOffset(usize),

    /// Index into a slice/array
    Index,

    // These indices are generated by slice patterns. Easiest to explain
    // by example:
    //
    // ```
    // [X, _, .._, _, _] => { offset: 0, min_length: 4, from_end: false },
    // [_, X, .._, _, _] => { offset: 1, min_length: 4, from_end: false },
    // [_, _, .._, X, _] => { offset: 2, min_length: 4, from_end: true },
    // [_, _, .._, _, X] => { offset: 1, min_length: 4, from_end: true },
    // ```
    // ConstantIndex {
    //     /// index or -index (in Python terms), depending on from_end
    //     offset: u64,
    //     /// thing being indexed must be at least this long
    //     min_length: u64,
    //     /// counting backwards from end?
    //     from_end: bool,
    // },
    /// These indices are generated by slice patterns.
    ///
    /// If `from_end` is true `slice[from..slice.len() - to]`.
    /// Otherwise `array[from..to]`.
    Subslice { from: u64, to: u64, from_end: bool },

    /// "Downcast" to a variant of an ADT. Currently, MIR only introduces
    /// this for ADTs with more than one variant. The value is the ordinal of the variant.
    Downcast(usize),

    /// The tag used to indicate which case of an enum is used for a particular enum value.
    Discriminant,

    /// Cast a path into another type.
    /// The most common cases are casting a transparent wrapper tyoe into its inner type or
    /// casting a type into a transparent wrapper type via pointer casting.
    Cast(usize),
}

impl Debug for PathSelector {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            PathSelector::Deref => f.write_str("deref"),
            PathSelector::Discriminant => f.write_str("discr"),
            PathSelector::Field(index) => index.fmt(f),
            PathSelector::UnionField(index) => f.write_fmt(format_args!("union_field#{:?}", index)),
            // PathSelector::UnionOffset(offset) => f.write_fmt(format_args!("union_offset#{:?}", offset)),
            PathSelector::Index => f.write_str("index"),
            // PathSelector::ConstantIndex {
            //     offset,
            //     min_length,
            //     from_end
            // } => f.write_fmt(format_args!("const_index#[{},{},{}]", offset, min_length, from_end)),
            PathSelector::Subslice { from, to, from_end } => {
                f.write_fmt(format_args!("subslice#[{}:{},{}]", from, to, from_end))
            }
            PathSelector::Downcast(index) => f.write_fmt(format_args!("as_variant#{}", *index)),
            PathSelector::Cast(index) => f.write_fmt(format_args!("cast#{}", *index)),
        }
    }
}

impl Path {
    /// Creates a path to the local variable corresponding to the ordinal.
    pub fn new_local(func_id: FuncId, ordinal: usize) -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::LocalVariable { func_id, ordinal },
        })
    }

    /// Creates a path to the parameter corresponding to the ordinal.
    pub fn new_parameter(func_id: FuncId, ordinal: usize) -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::Parameter { func_id, ordinal },
        })
    }

    /// Creates a path to the return value.
    pub fn new_return_value(func_id: FuncId) -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::ReturnValue { func_id },
        })
    }

    /// Creates a path to the local variable, parameter or result local, corresponding to the ordinal.
    pub fn new_local_parameter_or_result(func_id: FuncId, ordinal: usize, argument_count: usize) -> Rc<Path> {
        if ordinal == 0 {
            Self::new_return_value(func_id)
        } else if ordinal <= argument_count {
            Self::new_parameter(func_id, ordinal)
        } else {
            Self::new_local(func_id, ordinal)
        }
    }

    /// Creates a new auxiliary path.
    pub fn new_aux(func_id: FuncId, ordinal: usize) -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::Auxiliary { func_id, ordinal },
        })
    }

    /// Creates a path to the heap object.
    pub fn new_heap_obj(func_id: FuncId, location: Location) -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::HeapObj { func_id, location },
        })
    }

    /// Creates a path to a constant.
    pub fn new_constant() -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::Constant,
        })
    }

    /// Creates a path to a static variable.
    pub fn new_static_variable(def_id: DefId) -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::StaticVariable { def_id },
        })
    }

    /// Creates a path to a promoted constant.
    pub fn new_promoted(def_id: DefId, ordinal: usize) -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::PromotedConstant { def_id, ordinal },
        })
    }

    /// Creates a path to a argumentv1 array.
    pub fn new_argumentv1_arr() -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::PromotedArgumentV1Array,
        })
    }

    /// Creates a path to a &str array.
    pub fn new_str_ref_arr() -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::PromotedStrRefArray,
        })
    }

    /// Creates a path that qualifies the given root path with the given projection.
    pub fn new_qualified(base: Rc<Path>, projection: ProjectionElems) -> Rc<Path> {
        assert!(!matches!(base.value, PathEnum::QualifiedPath { .. }));
        Rc::new(Path {
            value: PathEnum::QualifiedPath { base, projection },
        })
    }

    /// Creates a path that qualifies the given root path with the given offset.
    pub fn new_offset(base: Rc<Path>, offset: usize) -> Rc<Path> {
        if offset == 0 {
            base
        } else {
            Rc::new(Path {
                value: PathEnum::OffsetPath { base, offset },
            })
        }
    }

    /// Creates a path that selects the element at a given index value of the array at the given path.
    pub fn new_index(collection_path: Rc<Path>) -> Rc<Path> {
        Path::append_projection_elem(&collection_path, PathSelector::Index)
    }

    /// Creates a path that selects the given field of the struct at the given path.
    pub fn new_field(base: Rc<Path>, field_index: usize) -> Rc<Path> {
        Self::append_projection_elem(&base, PathSelector::Field(field_index))
    }

    /// Creates a path that selects the given union field of the union at the given path.
    pub fn new_union_field(base: Rc<Path>, field_index: usize) -> Rc<Path> {
        Self::append_projection_elem(&base, PathSelector::UnionField(field_index))
    }
    
    /// Creates a path that selects the given downcast of the enum at the given path.
    pub fn new_downcast(base: Rc<Path>, downcast_variant: usize) -> Rc<Path> {
        Self::append_projection_elem(&base, PathSelector::Downcast(downcast_variant))
    }

    /// Creates a path referring to function item.
    pub fn new_function(func_id: FuncId) -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::Function(func_id),
        })
    }

    /// Creates a path referring to a type item.
    pub fn new_type(index: usize) -> Rc<Path> {
        Rc::new(Path {
            value: PathEnum::Type(index),
        })
    }

    /// Creates a path to the target memory of a reference value.
    pub fn new_deref(address_path: Rc<Path>) -> Rc<Path> {
        assert!(!matches!(address_path.value, PathEnum::QualifiedPath { .. }));
        Rc::new(Path {
            value: PathEnum::QualifiedPath {
                base: address_path,
                projection: vec![PathSelector::Deref],
            },
        })
    }

    /// Creates a path representing the metadata of a dynamic pointer.
    pub fn dyn_ptr_metadata(dyn_ptr_path: &Rc<Path>) -> Rc<Path> {
        Path::add_offset(dyn_ptr_path, PTR_METADATA_OFFSET)
    }

    /// Creates a path by appending the projection elem.
    pub fn append_projection_elem(path: &Rc<Path>, projection_elem: PathSelector) -> Rc<Path> {
        match &path.value {
            PathEnum::QualifiedPath { base, projection } => {
                let mut projection = projection.clone();
                projection.push(projection_elem);
                Path::new_qualified(base.clone(), projection)
            }
            _ => Path::new_qualified(path.clone(), vec![projection_elem]),
        }
    }

    /// Creates a path by appending the projection elems.
    pub fn append_projection(path: &Rc<Path>, projection_elems: &ProjectionElems) -> Rc<Path> {
        if projection_elems.is_empty() {
            return path.clone();
        }
        match &path.value {
            PathEnum::QualifiedPath { base, projection } => {
                let mut projection = projection.clone();
                projection.extend_from_slice(projection_elems);
                Path::new_qualified(base.clone(), projection)
            }
            _ => Path::new_qualified(path.clone(), projection_elems.clone()),
        }
    }

    pub fn add_offset(path: &Rc<Path>, offset: usize) -> Rc<Path> {
        if offset == 0 {
            return path.clone();
        }
        match &path.value {
            PathEnum::OffsetPath {
                base,
                offset: old_offset,
            } => Path::new_offset(base.clone(), old_offset + offset),
            _ => {
                if let PathEnum::QualifiedPath { base: _, projection } = &path.value {
                    assert!(projection.len() == 1 && projection[0] == PathSelector::Deref);
                }
                Path::new_offset(path.clone(), offset)
            }
        }
    }

    /// Creates a path by truncating the projection elems.
    pub fn truncate_projection_elems(path: &Rc<Path>, len: usize) -> Rc<Path> {
        if let PathEnum::QualifiedPath { base, projection } = &path.value {
            if projection.len() < len {
                warn!("The given length is langer than the projection elements length.");
                path.clone()
            } else {
                if len == 0 {
                    base.clone()
                } else {
                    Path::new_qualified(base.clone(), projection[..len].to_vec())
                }
            }
        } else {
            warn!("Truncating a non-qualified path");
            path.clone()
        }
    }

    /// Returns the original path by removing the cast.
    pub fn remove_cast(path: &Rc<Path>) -> Rc<Path> {
        if let PathEnum::QualifiedPath { base: _, projection } = &path.value {
            if let PathSelector::Cast(_) = projection.last().unwrap() {
                Path::truncate_projection_elems(&path, projection.len() - 1)
            } else {
                path.clone()
            }
        } else {
            path.clone()
        }
    }

    pub fn is_constant(&self) -> bool {
        matches!(self.value, PathEnum::Constant)
    } 
}


pub trait PathSupport {
    fn is_field_of(&self, path: &Rc<Path>) -> bool;
    fn is_deref_path(&self) -> bool;
}

impl PathSupport for Rc<Path> {
    /// Returns true if this path is the field of the given path.
    /// e.g. `_1.0.1` and `_1.0`
    fn is_field_of(&self, path: &Rc<Path>) -> bool {
        if let PathEnum::QualifiedPath { base, projection } = &self.value {
            let self_base = base;
            let self_projection = projection;
            match &path.value {
                PathEnum::QualifiedPath { base, projection } => {
                    if self_base == base && self_projection.len() > projection.len() {
                        return self_projection.iter().zip(projection.iter()).all(|(a, b)| a == b);
                    }
                    return false;
                }
                _ => {
                    return self_base == path;
                }
            }
        }
        false
    }

    /// Returns true if this path represents a dereferenced value or a field of a dereferenced value.
    fn is_deref_path(&self) -> bool {
        match &self.value {
            PathEnum::QualifiedPath { base: _, projection } => {
                if projection.len() == 0 {
                    error!("Found incorrect qualified path: {:?}", self);
                }
                if projection[0] == PathSelector::Deref {
                    true
                } else {
                    false
                }
            }
            PathEnum::OffsetPath { base, offset: _ } => base.is_deref_path(),
            _ => false,
        }
    }
}


impl PAGPath for Rc<Path> {
    type FuncTy = FuncId;

    fn new_parameter(func: FuncId, ordinal: usize) -> Self {
        Path::new_parameter(func, ordinal)
    }

    fn new_return_value(func: FuncId) -> Self {
        Path::new_return_value(func)
    }

    fn new_aux_local_path<'tcx>(acx: &mut AnalysisContext<'tcx, '_>, func: FuncId, ty: Ty<'tcx>) -> Self {
        acx.create_aux_local(func, ty)
    }

    fn value(&self) -> &PathEnum {
        &self.value
    }

    fn append_projection(&self, projection_elems: &ProjectionElems) -> Self {
        Path::append_projection(self, projection_elems)
    }

    fn add_offset(&self, offset: usize) -> Self {
        Path::add_offset(self, offset)
    }

    fn dyn_ptr_metadata(&self) -> Self {
        Path::dyn_ptr_metadata(self)
    }

    fn remove_cast(&self) -> Self {
        Path::remove_cast(self)
    }

    fn cast_to<'tcx>(&self, acx: &mut AnalysisContext<'tcx, '_>, ty: Ty<'tcx>) -> Option<Self> {
        acx.cast_to(self, ty)
    }
    
    fn type_variant<'tcx>(&self, acx: &mut AnalysisContext<'tcx, '_>, ty: Ty<'tcx>) -> Option<Self> {
        acx.get_type_variant(self, ty)
    }

    fn regularize(&self, acx: &mut AnalysisContext) -> Self {
        acx.get_regularized_path(self.clone())
    }

    fn try_eval_path_type<'tcx>(&self, acx: &mut AnalysisContext<'tcx, '_>) -> Ty<'tcx> {
        if let Some(ty) = acx.get_path_rustc_type(self) {
            ty
        } else {
            if let Some(ty) = type_util::try_eval_path_type(acx, self) {
                acx.set_path_rustc_type(self.clone(), ty);
                ty
            } else {
                acx.tcx.types.never
            }
        }
    }

    fn set_path_rustc_type<'tcx>(&self, acx: &mut AnalysisContext<'tcx, '_>, ty: Ty<'tcx>) {
        acx.set_path_rustc_type(self.clone(), ty);
    }

    fn has_been_cast(&self, acx: &AnalysisContext) -> bool {
        acx.get_cast_types(self).is_some()
    }

    fn concretized_heap_type<'tcx>(&self, acx: &AnalysisContext<'tcx, '_>) -> Option<Ty<'tcx>> {
        if let Some(ty) = acx.concretized_heap_objs.get(self) {
            Some(*ty)
        } else {
            None
        }
    }

    fn flatten_fields<'tcx>(self, acx: &mut AnalysisContext<'tcx, '_>) -> Vec<(usize, Self, Ty<'tcx>)> {
        let param_env = rustc_middle::ty::ParamEnv::reveal_all();
        let path_ty = self.try_eval_path_type(acx);
        type_util::flatten_fields(acx.tcx, param_env, self, path_ty)
    }

    fn get_containing_func(&self) -> Option<FuncId> {
        match &self.value {
            PathEnum::LocalVariable { func_id, .. } 
            | PathEnum::Parameter { func_id, .. } 
            | PathEnum::ReturnValue { func_id } 
            | PathEnum::Auxiliary { func_id, .. } 
            | PathEnum::HeapObj { func_id, .. } => Some(*func_id),            
            PathEnum::QualifiedPath { base, .. } 
            | PathEnum::OffsetPath { base, .. } => base.get_containing_func(),
            PathEnum::Constant
            | PathEnum::StaticVariable { .. } 
            | PathEnum::PromotedConstant { .. } 
            | PathEnum::Function(..) 
            | PathEnum::PromotedArgumentV1Array 
            | PathEnum::PromotedStrRefArray 
            | PathEnum::Type(..) => None,
        }
    }

}

impl PAGPath for Rc<CSPath> {
    type FuncTy = CSFuncId;

    fn new_parameter(func: CSFuncId, ordinal: usize) -> Self {
        CSPath::new_cs_path(
            func.cid,
            Path::new_parameter(func.func_id, ordinal)
        )
    }

    fn new_return_value(func: CSFuncId) -> Self {
        CSPath::new_cs_path(
            func.cid,
            Path::new_return_value(func.func_id)
        )
    }

    fn new_aux_local_path<'tcx>(acx: &mut AnalysisContext<'tcx, '_>, func: CSFuncId, ty: Ty<'tcx>) -> Self {
        CSPath::new_cs_path(
            func.cid,
            acx.create_aux_local(func.func_id, ty)
        )
    }

    fn value(&self) -> &PathEnum {
        &self.path.value
    }
        
    fn append_projection(&self, projection_elems: &ProjectionElems) -> Self {
        CSPath::new_cs_path(
            self.cid, 
            Path::append_projection(&self.path, projection_elems)
        )
    }

    fn add_offset(&self, offset: usize) -> Self {
        CSPath::new_cs_path(
            self.cid, 
            Path::add_offset(&self.path, offset)
        )
    }

    fn dyn_ptr_metadata(&self) -> Self {
        CSPath::new_cs_path(
            self.cid, 
            Path::dyn_ptr_metadata(&self.path)
        )
    }

    fn remove_cast(&self) -> Self {
        CSPath::new_cs_path(
            self.cid, 
            Path::remove_cast(&self.path)
        )
    }

    fn cast_to<'tcx>(&self, acx: &mut AnalysisContext<'tcx, '_>, ty: Ty<'tcx>) -> Option<Self> {
        if let Some(path) = acx.cast_to(&self.path, ty) {
            Some(
                CSPath::new_cs_path(self.cid, path)
            )
        } else {
            None
        }    
    }

    fn type_variant<'tcx>(&self, acx: &mut AnalysisContext<'tcx, '_>, ty: Ty<'tcx>) -> Option<Self> {
        if let Some(path) = acx.get_type_variant(&self.path, ty) {
            Some(
                CSPath::new_cs_path(self.cid, path)
            )
        } else {
            None
        }   
    }

    fn regularize(&self, acx: &mut AnalysisContext) -> Self {
        CSPath::new_cs_path(
            self.cid, 
            acx.get_regularized_path(self.path.clone())
        )
    }

    fn try_eval_path_type<'tcx>(&self, acx: &mut AnalysisContext<'tcx, '_>) -> Ty<'tcx> {
        self.path.try_eval_path_type(acx)
    }

    fn set_path_rustc_type<'tcx>(&self, acx: &mut AnalysisContext<'tcx, '_>, ty: Ty<'tcx>) {
        acx.set_path_rustc_type(self.path.clone(), ty);
    }
    
    fn has_been_cast(&self, acx: &AnalysisContext) -> bool {
        acx.get_cast_types(&self.path).is_some()
    }

    fn concretized_heap_type<'tcx>(&self, acx: &AnalysisContext<'tcx, '_>) -> Option<Ty<'tcx>> {
        self.path.concretized_heap_type(acx)
    }

    fn flatten_fields<'tcx>(self, acx: &mut AnalysisContext<'tcx, '_>) -> Vec<(usize, Self, Ty<'tcx>)> {
        let fields = self.path.clone().flatten_fields(acx);
        fields
            .into_iter()
            .map(|(offset, path, ty)| (offset, CSPath::new_cs_path(self.cid, path), ty))
            .collect()
    }

    fn get_containing_func(&self) -> Option<CSFuncId> {
        if let Some(func_id) = self.path.get_containing_func() {
            Some(CSFuncId { cid: self.cid, func_id })
        } else {
            None
        }
    }
}
