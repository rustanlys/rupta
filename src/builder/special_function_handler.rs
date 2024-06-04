// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! Provides special handling for a set of functions.

use lazy_static::lazy_static;
use log::*;
use std::collections::HashSet;
use std::rc::Rc;

use rustc_hir::def_id::DefId;
use rustc_hir::lang_items::LangItem;
use rustc_middle::mir;
use rustc_middle::ty::{List, GenericArgsRef, Ty, TyCtxt, TyKind};

use crate::builder::fpag_builder::FuncPAGBuilder;
use crate::mir::analysis_context::AnalysisContext;
use crate::mir::known_names::KnownNames;
use crate::mir::path::{Path, PathEnum, PathSelector};
use crate::util::type_util;

lazy_static! {
    static ref SPECIALLY_HANDLED_FUNCTIONS: HashSet<KnownNames> = {
        let mut set = HashSet::new();
        set.insert(KnownNames::StdIntrinsicsTransmute);
        set.insert(KnownNames::StdIntrinsicsOffset);
        set.insert(KnownNames::StdIntrinsicsArithOffset);
        set.insert(KnownNames::StdPtrConstPtrCast);
        set.insert(KnownNames::StdPtrConstPtrAdd);
        set.insert(KnownNames::StdPtrConstPtrSub);
        set.insert(KnownNames::StdPtrConstPtrOffset);
        set.insert(KnownNames::StdPtrConstPtrByteAdd);
        set.insert(KnownNames::StdPtrConstPtrByteSub);
        set.insert(KnownNames::StdPtrConstPtrByteOffset);
        set.insert(KnownNames::StdPtrConstPtrWrappingAdd);
        set.insert(KnownNames::StdPtrConstPtrWrappingSub);
        set.insert(KnownNames::StdPtrConstPtrWrappingOffset);
        set.insert(KnownNames::StdPtrConstPtrWrappingByteAdd);
        set.insert(KnownNames::StdPtrConstPtrWrappingByteSub);
        set.insert(KnownNames::StdPtrConstPtrWrappingByteOffset);
        set.insert(KnownNames::StdPtrMutPtrCast);
        set.insert(KnownNames::StdPtrMutPtrAdd);
        set.insert(KnownNames::StdPtrMutPtrSub);
        set.insert(KnownNames::StdPtrMutPtrOffset);
        set.insert(KnownNames::StdPtrMutPtrByteAdd);
        set.insert(KnownNames::StdPtrMutPtrByteSub);
        set.insert(KnownNames::StdPtrMutPtrByteOffset);
        set.insert(KnownNames::StdPtrMutPtrWrappingAdd);
        set.insert(KnownNames::StdPtrMutPtrWrappingSub);
        set.insert(KnownNames::StdPtrMutPtrWrappingOffset);
        set.insert(KnownNames::StdPtrMutPtrWrappingByteAdd);
        set.insert(KnownNames::StdPtrMutPtrWrappingByteSub);
        set.insert(KnownNames::StdPtrMutPtrWrappingByteOffset);
        set.insert(KnownNames::AllocRawVecAllocateIn);
        set.insert(KnownNames::StdThreadBuilderSpawnUnchecked);
        set.insert(KnownNames::StdPtrNonNullAsPtr);
        set.insert(KnownNames::StdPtrUniqueNewUnchecked);
        set.insert(KnownNames::StdResultMapErr);
        set.insert(KnownNames::RustAlloc);
        set.insert(KnownNames::RustAllocZeroed);
        set.insert(KnownNames::StdAllocAlloc);
        set.insert(KnownNames::StdAllocAllocZeroed);
        set.insert(KnownNames::StdAllocExchangeMalloc);
        set.insert(KnownNames::StdAllocAllocatorAllocate);
        set.insert(KnownNames::StdAllocAllocatorAllocateZeroed);
        set.insert(KnownNames::RustRealloc);
        set.insert(KnownNames::StdAllocRealloc);
        set.insert(KnownNames::StdAllocAllocatorGrow);
        set.insert(KnownNames::StdAllocAllocatorGrowZeroed);
        set.insert(KnownNames::StdAllocAllocatorShrink);
        set.insert(KnownNames::RustDealloc);
        set.insert(KnownNames::RustAllocErrorHandler);
        set.insert(KnownNames::StdAllocDealloc);
        set.insert(KnownNames::StdAllocBoxFree);
        set.insert(KnownNames::StdAllocHandleAllocError);
        set.insert(KnownNames::StdAllocAllocatorDeallocate);
        set
    };
}

/// Returns true if the function with `def_id` is specially handled.
pub fn is_specially_handled_function(acx: &mut AnalysisContext, def_id: DefId) -> bool {
    let known_name = acx.get_known_name_for(def_id);
    SPECIALLY_HANDLED_FUNCTIONS.contains(&known_name)
}

/// Handling calls to special functions.
/// 
/// Returns true if this callee function is handled as a special function.
/// If the return result is false, we need to continue with the normal logic.
pub fn handled_as_special_function_call<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    callee_def_id: &DefId,
    gen_args: &GenericArgsRef<'tcx>,
    args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
    location: mir::Location,
) -> bool {
    let callee_known_name = fpb.acx.get_known_name_for(*callee_def_id);
    match callee_known_name {
        KnownNames::StdIntrinsicsTransmute => {
            handle_transmute(fpb, gen_args, args, destination);
            return true;
        }
        KnownNames::StdIntrinsicsOffset
        | KnownNames::StdIntrinsicsArithOffset
        | KnownNames::StdPtrConstPtrAdd
        | KnownNames::StdPtrConstPtrSub
        | KnownNames::StdPtrConstPtrOffset
        | KnownNames::StdPtrConstPtrByteAdd
        | KnownNames::StdPtrConstPtrByteSub
        | KnownNames::StdPtrConstPtrByteOffset
        | KnownNames::StdPtrConstPtrWrappingAdd
        | KnownNames::StdPtrConstPtrWrappingSub
        | KnownNames::StdPtrConstPtrWrappingOffset
        | KnownNames::StdPtrConstPtrWrappingByteAdd
        | KnownNames::StdPtrConstPtrWrappingByteSub
        | KnownNames::StdPtrConstPtrWrappingByteOffset
        | KnownNames::StdPtrMutPtrAdd
        | KnownNames::StdPtrMutPtrSub
        | KnownNames::StdPtrMutPtrOffset
        | KnownNames::StdPtrMutPtrByteAdd
        | KnownNames::StdPtrMutPtrByteSub
        | KnownNames::StdPtrMutPtrByteOffset
        | KnownNames::StdPtrMutPtrWrappingAdd
        | KnownNames::StdPtrMutPtrWrappingSub
        | KnownNames::StdPtrMutPtrWrappingOffset
        | KnownNames::StdPtrMutPtrWrappingByteAdd
        | KnownNames::StdPtrMutPtrWrappingByteSub
        | KnownNames::StdPtrMutPtrWrappingByteOffset => {
            handle_offset(fpb, args, destination);
            return true;
        }
        KnownNames::StdPtrConstPtrCast 
        | KnownNames::StdPtrMutPtrCast  => {
            handle_ptr_cast(fpb, args, destination);
            return true;
        }
        KnownNames::AllocRawVecAllocateIn => {
            handle_raw_vec_allocate_in(fpb, gen_args, args, destination, location);
            return true;
        }
        KnownNames::StdThreadBuilderSpawnUnchecked => {
            handle_thread_builder_spawn_unchecked(fpb, gen_args, args, destination, location);
            return true;
        }
        KnownNames::StdPtrNonNullAsPtr => {
            handle_non_null_as_ptr(fpb, args, destination);
            return true;
        }
        KnownNames::StdPtrUniqueNewUnchecked => {
            handle_unique_new_unchecked(fpb, args, destination);
            return true;
        }
        KnownNames::StdResultMapErr => {
            handle_result_map_err(fpb, gen_args, args, destination);
            return true;
        }
        KnownNames::StdConvertInto => {
            let tcx = fpb.acx.tcx;
            let generic_types = gen_args.into_type_list(tcx);
            assert!(generic_types.len() >= 2);
            if is_std_ptr_unique(tcx, generic_types[0]) && is_std_ptr_nonnull(tcx, generic_types[1]) {
                handle_unique_into_nonnull(fpb, args, destination);
                return true;
            }
            return false;
        }
        _ => {
            return handle_alloc(fpb, callee_known_name, args, destination, location);
        }
    }
}


/// Handles the call to the intrinsics `Transmute` function.
fn handle_transmute<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    gen_args: &GenericArgsRef<'tcx>,
    args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
) {
    let source_path = args[0].clone();
    let source_rustc_type = gen_args.get(0).expect("rustc type error").expect_ty();
    let target_path = destination.clone();
    let target_rustc_type = fpb
        .acx
        .get_path_rustc_type(&target_path)
        .expect("rustc type error");
    fpb.copy_and_transmute(source_path, source_rustc_type, target_path, target_rustc_type);
}


/// Handles the call to the `offset` function, such as `std::ptr::mut_ptr::offset(_1: *mut T, _2: isize)`.
/// The offset function returns the address computed from the based address and the offset, and is commonly 
/// used in vector's read/write operations.
fn handle_offset<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
) {
    // Adds an offset edge from the source path to the destination path.
    let source_path = args[0].clone();
    fpb.add_offset_edge(source_path, destination.clone());
}

/// `core::ptr::const_ptr::cast()` and `core::ptr::mut_ptr::cast()`.
/// 
/// The cast functions significantly impacts the analysis precision and efficiency 
/// when analyzed context-insensitively.
fn handle_ptr_cast<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
) {
    // Adds a cast edge from the source path to the destination path.
    let source_path = args[0].clone();
    fpb.add_cast_edge(source_path, destination.clone());
}


/// ```fn allocate_in(capacity: usize, init: AllocInit, alloc: A) -> Self```.
/// ```RawVec<T, A: Allocator = Global> { ptr: Unique<T>, cap: usize, alloc: A, }```
fn handle_raw_vec_allocate_in<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    gen_args: &GenericArgsRef<'tcx>,
    _args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
    location: mir::Location,
) {
    let tcx = fpb.acx.tcx;
    let heap_object_path = Path::new_heap_obj(fpb.fpag.func_id, location);
    fpb
        .acx
        .set_path_rustc_type(heap_object_path.clone(), tcx.types.u8);

    let generic_type = gen_args.get(0).expect("rustc type error").expect_ty();
    fpb
        .acx
        .concretized_heap_objs
        .insert(heap_object_path.clone(), generic_type);
    let cast_heap_object_path = fpb
        .acx
        .cast_to(&heap_object_path, generic_type)
        .expect("Cast Error");

    // dst.0 = Unique, Unique.0 = NonNull, NonNull.0 = source thin pointer
    let projection = vec![
        PathSelector::Field(0),
        PathSelector::Field(0),
        PathSelector::Field(0),
    ];
    let dst_ptr_path = Path::new_qualified(destination.clone(), projection);
    let const_ptr_type = const_rawptr_type(tcx, generic_type);
    fpb
        .acx
        .set_path_rustc_type(dst_ptr_path.clone(), const_ptr_type);
    // Instead of inserting an address_of address from heap_object to dst_ptr_path,
    // we create a auxiliary path as an intermediary
    // ```let aux: *const T = &heap_object;  dst.0.0.0 = aux;```
    let aux = fpb
        .acx
        .create_aux_local(fpb.fpag.func_id, const_ptr_type);
    fpb.add_addr_edge(cast_heap_object_path, aux.clone());
    fpb.add_direct_edge(aux, dst_ptr_path);
}


/// ```fn spawn_unchecked<'a, F, T>(self, f: F) -> io::Result<JoinHandle<T>>```.
/// This function starts a new thread by calling external C function.
/// Instead of calling this function, we indirect the call to the thread closure f.
/// We can call `inline_indirectly_called_function` in fpb directly to resolve this call.
fn handle_thread_builder_spawn_unchecked<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    gen_args: &GenericArgsRef<'tcx>,
    args: &Vec<Rc<Path>>,
    _destination: &Rc<Path>,
    location: mir::Location,
) {
    let fn_once_defid = fpb.acx.tcx.require_lang_item(LangItem::FnOnce, None);
    let dst_ty = gen_args.get(1).expect("rustc type error").expect_ty();
    // FnOnce call requires two arguments, the first argument is the fn item that implements FnOnce trait,
    // and the second argument is the actual arguments list, an empty tuple in this case.
    let aux_arg = fpb.create_aux_local(fpb.acx.tcx.mk_ty_from_kind(TyKind::Tuple(List::empty())));
    let new_args = vec![args[1].clone(), aux_arg];
    let aux_dst = fpb.create_aux_local(dst_ty);
    let mut new_location = location;
    new_location.statement_index += 1;
    fpb.inline_indirectly_called_function(
        &fn_once_defid,
        gen_args,
        new_args,
        aux_dst,
        new_location,
    );

    // Todo: Add edges from `aux_dst` to `destination`, to do so, we need to allocate a heap memory for the packet field.
    // Destination type: io::Result<JoinHandle<T>>, where struct JoinHandle<T>(JoinInner<'static, T>);
    // struct JoinInner<'scope, T> {
    //     native: imp::Thread,
    //     thread: Thread,
    //     packet: Arc<Packet<'scope, T>>,
    // }
    // struct Packet<'scope, T> {
    //     scope: Option<Arc<scoped::ScopeData>>,
    //     result: UnsafeCell<Option<Result<T>>>,
    //     _marker: PhantomData<Option<&'scope scoped::ScopeData>>,
    // }
}

fn handle_non_null_as_ptr<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
) {
    // Adds an direct edge from the source path's first field to the destination path .
    let source_path = args[0].clone();
    let field_path = Path::new_field(source_path, 0);
    let ty = fpb.acx.get_path_rustc_type(destination).unwrap();
    fpb.acx.set_path_rustc_type(field_path.clone(), ty);
    fpb.add_direct_edge(field_path, destination.clone());
}

/// ```fn std::ptr::Unique::<T>::new_unchecked(_1: *mut T) -> std::ptr::Unique<T>```
fn handle_unique_new_unchecked<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
) {
    // Adds an direct edge from args[0] to dst.0.0
    let dst_field_path = Path::new_qualified(
        destination.clone(),
        vec![PathSelector::Field(0), PathSelector::Field(0)],
    );
    fpb.add_direct_edge(args[0].clone(), dst_field_path);
}

/// ```fn std::result::Result::<T, E>::map_err(_1: std::result::Result<T, E>, _2: O) 
///    -> std::result::Result<T, F>
/// ```
/// Handles as an assignment from `param_1.as_variant#0.0` to `ret.as_variant#0.0`.
fn handle_result_map_err<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    gen_args: &GenericArgsRef<'tcx>,
    args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
) {
    assert!(!matches!(args[0].value, PathEnum::QualifiedPath { .. }));
    let source_path = Path::new_qualified(
        args[0].clone(),
        vec![PathSelector::Downcast(0), PathSelector::Field(0)],
    );
    let source_rustc_type = gen_args.get(0).expect("rustc type error").expect_ty();
    let target_path = Path::new_qualified(
        destination.clone(),
        vec![PathSelector::Downcast(0), PathSelector::Field(0)],
    );
    let target_rustc_type = source_rustc_type;
    fpb.add_internal_edges(
        source_path,
        source_rustc_type,
        target_path,
        target_rustc_type,
    );
}

#[allow(unused)]
fn handle_slice_index_index<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
) -> bool {
    let slice_path = args[1].clone();
    let slice_ty = fpb
        .acx
        .get_path_rustc_type(&slice_path)
        .expect("rustc type error");
    let dst_ty = fpb
        .acx
        .get_path_rustc_type(destination)
        .expect("rustc type error");

    if slice_ty == dst_ty {
        fpb.add_internal_edges(
            slice_path,
            slice_ty,
            destination.clone(),
            dst_ty,
        );
        return true;
    }
    return false;
}

fn handle_unique_into_nonnull(
    fpb: &mut FuncPAGBuilder,
    args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
) {
    assert!(!matches!(args[0].value, PathEnum::QualifiedPath { .. }));
    let source_path = Path::new_field(args[0].clone(), 0);
    let source_rustc_type = type_util::try_eval_path_type(fpb.acx, &source_path).unwrap();
    let target_rustc_type = fpb
        .acx
        .get_path_rustc_type(destination)
        .expect("rustc type error");
    info!(
        "Add edge from {:?}({:?}) to {:?}({:?})",
        source_path, source_rustc_type, destination, target_rustc_type
    );
    fpb.add_internal_edges(
        source_path,
        source_rustc_type,
        destination.clone(),
        target_rustc_type,
    );
}

fn handle_alloc<'tcx>(
    fpb: &mut FuncPAGBuilder<'_, 'tcx, '_>,
    callee_known_name: KnownNames,
    args: &Vec<Rc<Path>>,
    destination: &Rc<Path>,
    location: mir::Location,
) -> bool {
    let tcx = fpb.acx.tcx;
    match callee_known_name {
        // Allocates memory on the heap and returns the address as `*mut u8`
        KnownNames::RustAlloc
        | KnownNames::RustAllocZeroed
        | KnownNames::StdAllocAlloc
        | KnownNames::StdAllocAllocZeroed
        | KnownNames::StdAllocExchangeMalloc => {
            let heap_object_path = Path::new_heap_obj(fpb.fpag.func_id, location);
            fpb
                .acx
                .set_path_rustc_type(heap_object_path.clone(), tcx.types.u8);
            fpb.add_addr_edge(heap_object_path, destination.clone());
            true
        }
        // Allocates memory on the heap and returns a result of Result<NonNull<[u8]>, AllocError> type.
        // If the allocation is successful, the result would be Result::Ok<NonNull<[u8]>>, Result::Err<AllocError> otherwise.
        KnownNames::StdAllocAllocatorAllocate | KnownNames::StdAllocAllocatorAllocateZeroed => {
            let heap_object_path = Path::new_heap_obj(fpb.fpag.func_id, location);
            fpb
                .acx
                .set_path_rustc_type(heap_object_path.clone(), tcx.types.u8);
            let cast_heap_object_path = fpb
                .acx
                .cast_to(&heap_object_path, Ty::new_slice(fpb.acx.tcx, tcx.types.u8))
                .expect("Cast Error");
            // (dst as Ok).0: NonNull<[u8]>, ((dst as Ok).0).0: *const [u8]
            let projection = vec![
                PathSelector::Downcast(0),
                PathSelector::Field(0),
                PathSelector::Field(0),
            ];
            let qualified_path = Path::new_qualified(destination.clone(), projection);
            fpb
                .acx
                .set_path_rustc_type(qualified_path.clone(), const_u8_rawptr_type(tcx));
            // Instead of inserting an address_of address from heap_object to ((dst as Ok).0).0,
            // we create a auxiliary path as an intermediary
            // ```let aux: *const u8 = &heap_object;  ((dst as Ok).0).0 = aux;```
            let aux = fpb
                .acx
                .create_aux_local(fpb.fpag.func_id, const_u8_rawptr_type(tcx));
            fpb.add_addr_edge(cast_heap_object_path, aux.clone());
            fpb.add_direct_edge(aux, qualified_path);
            true
        }
        // Reallocate memory on the heap and returns the address as `*mut u8`
        KnownNames::RustRealloc | KnownNames::StdAllocRealloc => {
            // Instead of creating a new heap object path, we return the original heap object directly.
            // Therefore we add an direct edge from the source heap object to the target heap object.
            fpb.add_direct_edge(args[0].clone(), destination.clone());
            true
        }
        // Reallocates memory on the heap and returns a result of `Result<NonNull<[u8]>, AllocError>` type.
        KnownNames::StdAllocAllocatorGrow
        | KnownNames::StdAllocAllocatorGrowZeroed
        | KnownNames::StdAllocAllocatorShrink => {
            // Similar to RustRealloc, we add an direct edge from the source pointer to the destination pointer
            // Note: source arg type: NonNull<u8>, destination type: Result<NonNull<[u8]>, AllocError>
            // we need to cast from type *const u8 (arg[1].0) to type *const [u8] (ret.downcast(0).0.0)
            let src_ptr_path = Path::new_qualified(args[1].clone(), vec![PathSelector::Field(0)]);

            // (dst as Ok).0: NonNull<[u8]>, ((dst as Ok).0).0: *const [u8]
            let projection = vec![
                PathSelector::Downcast(0),
                PathSelector::Field(0),
                PathSelector::Field(0),
            ];
            let dst_ptr_path = Path::new_qualified(destination.clone(), projection);
            fpb
                .acx
                .set_path_rustc_type(dst_ptr_path.clone(), const_u8_rawptr_type(tcx));
            fpb.add_cast_edge(src_ptr_path, dst_ptr_path);
            true
        }
        KnownNames::RustDealloc
        | KnownNames::RustAllocErrorHandler
        | KnownNames::StdAllocDealloc
        | KnownNames::StdAllocBoxFree
        | KnownNames::StdAllocHandleAllocError
        | KnownNames::StdAllocAllocatorDeallocate => true,
        _ => false,
    }
}

fn is_std_ptr_unique<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(def, _) => {
            let def_path_str = tcx.def_path_str(def.did());
            def_path_str == "std::ptr::Unique"
        }
        _ => false
    }
}

fn is_std_ptr_nonnull<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(def, _) => {
            let def_path_str = tcx.def_path_str(def.did());
            def_path_str == "std::ptr::NonNull"
        }
        _ => false
    }
}

fn const_rawptr_type<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Ty<'tcx> {
    tcx.mk_ty_from_kind(TyKind::RawPtr(rustc_middle::ty::TypeAndMut {
        ty,
        mutbl: rustc_middle::mir::Mutability::Not,
    }))
}

fn const_u8_rawptr_type(tcx: TyCtxt) -> Ty {
    tcx.mk_ty_from_kind(TyKind::RawPtr(rustc_middle::ty::TypeAndMut {
        ty: tcx.mk_ty_from_kind(TyKind::Slice(tcx.types.u8)),
        mutbl: rustc_middle::mir::Mutability::Not,
    }))
}
