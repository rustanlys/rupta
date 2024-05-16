// Copied from mirai (https://github.com/facebookexperimental/MIRAI)
// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use rustc_hir::def_id::DefId;
use rustc_hir::definitions::{DefPathData, DisambiguatedDefPathData};
use rustc_middle::ty::TyCtxt;
use std::collections::HashMap;

/// Well known definitions (language provided items) that are treated in special ways.
#[derive(Clone, Copy, Debug, Eq, PartialOrd, PartialEq, Hash, Ord)]
pub enum KnownNames {
    /// This is not a known name
    None,
    AllocRawVecMinNonZeroCap,

    // These are the magic symbols to call the global allocator.  rustc generates
    // them to call `__rg_alloc` etc. if there is a `#[global_allocator]` attribute
    // (the code expanding that attribute macro generates those functions), or to call
    // the default implementations in libstd otherwise.
    RustAlloc,             // fn __rust_alloc(size: usize, align: usize) -> *mut u8;
    RustAllocZeroed,       // fn __rust_alloc_zeroed(size: usize, align: usize) -> *mut u8;
    RustDealloc,           // fn __rust_dealloc(ptr: *mut u8, size: usize, align: usize);
    RustRealloc,           // fn __rust_realloc(ptr: *mut u8, old_size: usize, align: usize, new_size: usize) -> *mut u8;
    RustAllocErrorHandler, // fn __rust_alloc_error_handler(size: usize, align: usize) -> !;

    // Allocate|Deallocate memory with the global allocator. Wrappers of `__rust_alloc`, `__rust_alloc_zeroed`...
    StdAllocAlloc,            // fn alloc(Layout) -> *mut u8
    StdAllocAllocZeroed,      // fn alloc_zeroed(layout: Layout) -> *mut u8
    StdAllocDealloc,          // fn dealloc(ptr: *mut u8, layout: Layout)
    StdAllocRealloc,          // fn realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8
    StdAllocExchangeMalloc,   // fn exchange_malloc(size: usize, align: usize) -> *mut u8
    StdAllocBoxFree,          // fn box_free(ptr: Unique<T>, alloc: A)
    StdAllocHandleAllocError, // fn handle_alloc_error(layout: Layout) -> !

    // Implementations of functions in trait Allocator, the default implementation type of Allocator in Box and Vec... is alloc::Global,
    // which is a wrapper of alloc::alloc|alloc::alloc_zeroed
    StdAllocAllocatorAllocate,       // fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>;
    StdAllocAllocatorAllocateZeroed, // fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>
    StdAllocAllocatorDeallocate,     // fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);
    StdAllocAllocatorGrow,           // fn grow(&self, ptr: NonNull<u8>, layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError>;
    StdAllocAllocatorGrowZeroed,     // fn grow_zeroed(&self, ptr: NonNull<u8>, layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError>;
    StdAllocAllocatorShrink,         // fn shrink(&self, ptr: NonNull<u8>, layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError>;

    AllocRawVecCurrnetMemory, // fn alloc::raw_vec::RawVec::<T, A>::current_memory(_1: &alloc::raw_vec::RawVec<T, A>)
                              //     -> std::option::Option<(std::ptr::NonNull<u8>, std::alloc::Layout)>
    AllocRawVecGrowAmortized, // alloc::raw_vec::RawVec::<T, A>::grow_amortized(_1: &mut alloc::raw_vec::RawVec<T, A>, _2: usize, _3: usize)
                              //     -> std::result::Result<(), std::collections::TryReserveError>
    AllocRawVecGrowExact,     // fn alloc::raw_vec::RawVec::<T, A>::grow_exact(_1: &mut alloc::raw_vec::RawVec<T, A>, _2: usize, _3: usize)
                              //     -> std::result::Result<(), std::collections::TryReserveError>
    AllocRawVecShrink,        // fn alloc::raw_vec::RawVec::<T, A>::shrink(_1: &mut alloc::raw_vec::RawVec<T, A>, _2: usize)
                              //     -> std::result::Result<(), std::collections::TryReserveError>
    AllocRawVecFinishGrow,    // fn alloc::raw_vec::finish_grow(_1: std::result::Result<std::alloc::Layout, std::alloc::LayoutError>,
                              //                                _2: std::option::Option<(std::ptr::NonNull<u8>, std::alloc::Layout)>, _3: &mut A)
                              //     -> std::result::Result<std::ptr::NonNull<[u8]>, std::collections::TryReserveError>
    AllocRawVecAllocateIn,    // alloc::raw_vec::{impl#1}::allocate_in<T, A>
    StdResultMapErr,          // fn std::result::Result::<T, E>::map_err(_1: std::result::Result<T, E>, _2: O) -> std::result::Result<T, F>
    
    StdCloneClone,
    StdFutureFromGenerator,
    StdIntrinsicsArithOffset,
    StdIntrinsicsBitreverse,
    StdIntrinsicsBswap,
    StdIntrinsicsCeilf32,
    StdIntrinsicsCeilf64,
    StdIntrinsicsCopy,
    StdIntrinsicsCopyNonOverlapping,
    StdIntrinsicsCopysignf32,
    StdIntrinsicsCopysignf64,
    StdIntrinsicsCosf32,
    StdIntrinsicsCosf64,
    StdIntrinsicsCtlz,
    StdIntrinsicsCtlzNonzero,
    StdIntrinsicsCtpop,
    StdIntrinsicsCttz,
    StdIntrinsicsCttzNonzero,
    StdIntrinsicsDiscriminantValue,
    StdIntrinsicsExp2f32,
    StdIntrinsicsExp2f64,
    StdIntrinsicsExpf32,
    StdIntrinsicsExpf64,
    StdIntrinsicsFabsf32,
    StdIntrinsicsFabsf64,
    StdIntrinsicsFaddFast,
    StdIntrinsicsFdivFast,
    StdIntrinsicsFloorf32,
    StdIntrinsicsFloorf64,
    StdIntrinsicsFmulFast,
    StdIntrinsicsFremFast,
    StdIntrinsicsFsubFast,
    StdIntrinsicsLog10f32,
    StdIntrinsicsLog10f64,
    StdIntrinsicsLog2f32,
    StdIntrinsicsLog2f64,
    StdIntrinsicsLogf32,
    StdIntrinsicsLogf64,
    StdIntrinsicsMaxnumf32,
    StdIntrinsicsMaxnumf64,
    StdIntrinsicsMinAlignOfVal,
    StdIntrinsicsMinnumf32,
    StdIntrinsicsMinnumf64,
    StdIntrinsicsMulWithOverflow,
    StdIntrinsicsNearbyintf32,
    StdIntrinsicsNearbyintf64,
    StdIntrinsicsNeedsDrop,
    StdIntrinsicsOffset,
    StdIntrinsicsPowf32,
    StdIntrinsicsPowf64,
    StdIntrinsicsPowif32,
    StdIntrinsicsPowif64,
    StdIntrinsicsRawEq,
    StdIntrinsicsRintf32,
    StdIntrinsicsRintf64,
    StdIntrinsicsRoundf32,
    StdIntrinsicsRoundf64,
    StdIntrinsicsSinf32,
    StdIntrinsicsSinf64,
    StdIntrinsicsSizeOf,
    StdIntrinsicsSizeOfVal,
    StdIntrinsicsSqrtf32,
    StdIntrinsicsSqrtf64,
    StdIntrinsicsTransmute,
    StdIntrinsicsTruncf32,
    StdIntrinsicsTruncf64,
    StdIntrinsicsWriteBytes,
    StdMarkerPhantomData,
    StdMemReplace,

    // Indirect function calls via Fn::call|FnOnce::call_once|FnMut::call_mut
    StdOpsFunctionImpls,
    StdOpsFunctionFnCall,
    StdOpsFunctionFnMutCallMut,
    StdOpsFunctionFnOnceCallOnce,

    StdPanickingAssertFailed,
    StdPanickingBeginPanic,
    StdPanickingBeginPanicFmt,

    StdPtrSwapNonOverlapping,
    StdPtrNonNullAsPtr,
    StdPtrNonNullAsRef,
    StdPtrNonNullAsMut,
    StdPtrNonNullCast,
    StdPtrUniqueNewUnchecked,
    StdPtrConstPtrCast,
    StdPtrConstPtrAdd,
    StdPtrConstPtrSub,
    StdPtrConstPtrOffset,
    StdPtrConstPtrByteAdd,
    StdPtrConstPtrByteSub,
    StdPtrConstPtrByteOffset,
    StdPtrConstPtrWrappingAdd,
    StdPtrConstPtrWrappingSub,
    StdPtrConstPtrWrappingOffset,
    StdPtrConstPtrWrappingByteAdd,
    StdPtrConstPtrWrappingByteSub,
    StdPtrConstPtrWrappingByteOffset,
    StdPtrMutPtrCast,
    StdPtrMutPtrAdd,
    StdPtrMutPtrSub,
    StdPtrMutPtrOffset,
    StdPtrMutPtrByteAdd,
    StdPtrMutPtrByteSub,
    StdPtrMutPtrByteOffset,
    StdPtrMutPtrWrappingAdd,
    StdPtrMutPtrWrappingSub,
    StdPtrMutPtrWrappingOffset,
    StdPtrMutPtrWrappingByteAdd,
    StdPtrMutPtrWrappingByteSub,
    StdPtrMutPtrWrappingByteOffset,

    StdSliceCmpMemcmp,
    StdSliceIndexIndex, // slice::index::{impl#3-8}::index<T>(_1: std::ops::Range*<usize>, _2: &[T]) -> &[T]
    StdSliceIndexIndexMut, // slice::index::{impl#3-8}::index_mut<T>(_1: std::ops::Range*<usize>, _2: &mut [T]) -> &mut [T]

    StdThreadBuilderSpawnUnchecked,
    StdThreadBuilderSpawnUnchecked_, // This function starts a new thread by invoking a function through the passed function closure

    StdConvertInto,
}

/// An analysis lifetime cache that contains a map from def ids to known names.
pub struct KnownNamesCache {
    name_cache: HashMap<DefId, KnownNames>,
}

type Iter<'a> = std::slice::Iter<'a, rustc_hir::definitions::DisambiguatedDefPathData>;

impl KnownNamesCache {
    /// Create an empty known names cache.
    /// This cache is re-used by every successive MIR visitor instance.
    pub fn create_cache_from_language_items() -> KnownNamesCache {
        let name_cache = HashMap::new();
        KnownNamesCache { name_cache }
    }

    /// Get the well known name for the given def id and cache the association.
    /// I.e. the first call for an unknown def id will be somewhat costly but
    /// subsequent calls will be cheap. If the def_id does not have an actual well
    /// known name, this returns KnownNames::None.
    pub fn get(&mut self, tcx: TyCtxt<'_>, def_id: DefId) -> KnownNames {
        *self
            .name_cache
            .entry(def_id)
            .or_insert_with(|| Self::get_known_name_for(tcx, def_id))
    }

    /// Uses information obtained from tcx to figure out which well known name (if any)
    /// this def id corresponds to.
    pub(crate) fn get_known_name_for(tcx: TyCtxt<'_>, def_id: DefId) -> KnownNames {
        use DefPathData::*;

        let def_path = &tcx.def_path(def_id);
        let def_path_data_iter = def_path.data.iter();

        // helper to get next elem from def path and return its name, if it has one
        let get_path_data_elem_name =
            |def_path_data_elem: Option<&rustc_hir::definitions::DisambiguatedDefPathData>| {
                def_path_data_elem.and_then(|ref elem| {
                    let DisambiguatedDefPathData { data, .. } = elem;
                    match &data {
                        TypeNs(name) | ValueNs(name) => Some(*name),
                        _ => None,
                    }
                })
            };

        let is_foreign_module =
            |def_path_data_elem: Option<&rustc_hir::definitions::DisambiguatedDefPathData>| {
                if let Some(elem) = def_path_data_elem {
                    let DisambiguatedDefPathData { data, .. } = elem;
                    matches!(&data, ForeignMod)
                } else {
                    false
                }
            };

        let path_data_elem_as_disambiguator =
            |def_path_data_elem: Option<&rustc_hir::definitions::DisambiguatedDefPathData>| {
                def_path_data_elem.map(|DisambiguatedDefPathData { disambiguator, .. }| *disambiguator)
            };

        let get_known_name_for_alloc_namespace = |mut def_path_data_iter: Iter<'_>| {
            let def_path_data = def_path_data_iter.next();
            if is_foreign_module(def_path_data) {
                get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "RawVec" => KnownNames::RustAlloc,
                        "__rust_alloc_zeroed" => KnownNames::RustAllocZeroed,
                        "__rust_dealloc" => KnownNames::RustDealloc,
                        "__rust_realloc" => KnownNames::RustRealloc,
                        "__rust_alloc_error_handler" => KnownNames::RustAllocErrorHandler,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None)
            } else {
                get_path_data_elem_name(def_path_data)
                    .map(|n| match n.as_str() {
                        "alloc" => KnownNames::StdAllocAlloc,
                        "alloc_zeroed" => KnownNames::StdAllocAllocZeroed,
                        "dealloc" => KnownNames::StdAllocDealloc,
                        "realloc" => KnownNames::StdAllocRealloc,
                        "exchange_malloc" => KnownNames::StdAllocExchangeMalloc,
                        "handle_alloc_error" => KnownNames::StdAllocHandleAllocError,
                        "box_free" => KnownNames::StdAllocBoxFree,
                        "Allocator" => get_path_data_elem_name(def_path_data_iter.next())
                            .map(|n| match n.as_str() {
                                "allocate" => KnownNames::StdAllocAllocatorAllocate,
                                "allocate_zeroed" => KnownNames::StdAllocAllocatorAllocateZeroed,
                                "deallocate" => KnownNames::StdAllocAllocatorDeallocate,
                                "grow" => KnownNames::StdAllocAllocatorGrow,
                                "grow_zeroed" => KnownNames::StdAllocAllocatorGrowZeroed,
                                "shrink" => KnownNames::StdAllocAllocatorShrink,
                                _ => KnownNames::None,
                            })
                            .unwrap_or(KnownNames::None),
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None)
            }
        };

        let get_known_name_for_clone_trait = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "clone" => KnownNames::StdCloneClone,
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_clone_namespace = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "Clone" => get_known_name_for_clone_trait(def_path_data_iter),
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_future_namespace = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "from_generator" => KnownNames::StdFutureFromGenerator,
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_instrinsics_foreign_namespace =
            |mut def_path_data_iter: Iter<'_>| {
                get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "arith_offset" => KnownNames::StdIntrinsicsArithOffset,
                        "bitreverse" => KnownNames::StdIntrinsicsBitreverse,
                        "bswap" => KnownNames::StdIntrinsicsBswap,
                        "ceilf32" => KnownNames::StdIntrinsicsCeilf32,
                        "ceilf64" => KnownNames::StdIntrinsicsCeilf64,
                        "compare_bytes" => KnownNames::StdSliceCmpMemcmp,
                        "copysignf32" => KnownNames::StdIntrinsicsCopysignf32,
                        "copysignf64" => KnownNames::StdIntrinsicsCopysignf64,
                        "cosf32" => KnownNames::StdIntrinsicsCosf32,
                        "cosf64" => KnownNames::StdIntrinsicsCosf64,
                        "ctlz" => KnownNames::StdIntrinsicsCtlz,
                        "ctlz_nonzero" => KnownNames::StdIntrinsicsCtlzNonzero,
                        "ctpop" => KnownNames::StdIntrinsicsCtpop,
                        "cttz" => KnownNames::StdIntrinsicsCttz,
                        "cttz_nonzero" => KnownNames::StdIntrinsicsCttzNonzero,
                        "discriminant_value" => KnownNames::StdIntrinsicsDiscriminantValue,
                        "exp2f32" => KnownNames::StdIntrinsicsExp2f32,
                        "exp2f64" => KnownNames::StdIntrinsicsExp2f64,
                        "expf32" => KnownNames::StdIntrinsicsExpf32,
                        "expf64" => KnownNames::StdIntrinsicsExpf64,
                        "fabsf32" => KnownNames::StdIntrinsicsFabsf32,
                        "fabsf64" => KnownNames::StdIntrinsicsFabsf64,
                        "fadd_fast" => KnownNames::StdIntrinsicsFaddFast,
                        "fdiv_fast" => KnownNames::StdIntrinsicsFdivFast,
                        "floorf32" => KnownNames::StdIntrinsicsFloorf32,
                        "floorf64" => KnownNames::StdIntrinsicsFloorf64,
                        "fmul_fast" => KnownNames::StdIntrinsicsFmulFast,
                        "frem_fast" => KnownNames::StdIntrinsicsFremFast,
                        "fsub_fast" => KnownNames::StdIntrinsicsFsubFast,
                        "log10f32" => KnownNames::StdIntrinsicsLog10f32,
                        "log10f64" => KnownNames::StdIntrinsicsLog10f64,
                        "log2f32" => KnownNames::StdIntrinsicsLog2f32,
                        "log2f64" => KnownNames::StdIntrinsicsLog2f64,
                        "logf32" => KnownNames::StdIntrinsicsLogf32,
                        "logf64" => KnownNames::StdIntrinsicsLogf64,
                        "maxnumf32" => KnownNames::StdIntrinsicsMaxnumf32,
                        "maxnumf64" => KnownNames::StdIntrinsicsMaxnumf64,
                        "min_align_of_val" => KnownNames::StdIntrinsicsMinAlignOfVal,
                        "minnumf32" => KnownNames::StdIntrinsicsMinnumf32,
                        "minnumf64" => KnownNames::StdIntrinsicsMinnumf64,
                        "mul_with_overflow" => KnownNames::StdIntrinsicsMulWithOverflow,
                        "nearbyintf32" => KnownNames::StdIntrinsicsNearbyintf32,
                        "nearbyintf64" => KnownNames::StdIntrinsicsNearbyintf64,
                        "needs_drop" => KnownNames::StdIntrinsicsNeedsDrop,
                        "offset" => KnownNames::StdIntrinsicsOffset,
                        "powf32" => KnownNames::StdIntrinsicsPowf32,
                        "powf64" => KnownNames::StdIntrinsicsPowf64,
                        "powif32" => KnownNames::StdIntrinsicsPowif32,
                        "powif64" => KnownNames::StdIntrinsicsPowif64,
                        "raw_eq" => KnownNames::StdIntrinsicsRawEq,
                        "rintf32" => KnownNames::StdIntrinsicsRintf32,
                        "rintf64" => KnownNames::StdIntrinsicsRintf64,
                        "roundf32" => KnownNames::StdIntrinsicsRintf32,
                        "roundf64" => KnownNames::StdIntrinsicsRintf64,
                        "sinf32" => KnownNames::StdIntrinsicsSinf32,
                        "sinf64" => KnownNames::StdIntrinsicsSinf64,
                        "size_of" => KnownNames::StdIntrinsicsSizeOf,
                        "size_of_val" => KnownNames::StdIntrinsicsSizeOfVal,
                        "sqrtf32" => KnownNames::StdIntrinsicsSqrtf32,
                        "sqrtf64" => KnownNames::StdIntrinsicsSqrtf64,
                        "transmute" => KnownNames::StdIntrinsicsTransmute,
                        "truncf32" => KnownNames::StdIntrinsicsTruncf32,
                        "truncf64" => KnownNames::StdIntrinsicsTruncf64,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None)
            };

        let get_known_name_for_intrinsics_namespace = |mut def_path_data_iter: Iter<'_>| {
            let current_elem = def_path_data_iter.next();
            match path_data_elem_as_disambiguator(current_elem) {
                Some(0) => {
                    if is_foreign_module(current_elem) {
                        get_known_name_for_instrinsics_foreign_namespace(def_path_data_iter)
                    } else {
                        get_path_data_elem_name(current_elem)
                            .map(|n| match n.as_str() {
                                "copy" => KnownNames::StdIntrinsicsCopy,
                                "copy_nonoverlapping" => {
                                    KnownNames::StdIntrinsicsCopyNonOverlapping
                                }
                                "write_bytes" => KnownNames::StdIntrinsicsWriteBytes,
                                _ => KnownNames::None,
                            })
                            .unwrap_or(KnownNames::None)
                    }
                }
                _ => KnownNames::None,
            }
        };

        let get_known_name_for_marker_namespace = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "PhantomData" => KnownNames::StdMarkerPhantomData,
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_mem_namespace = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "replace" => KnownNames::StdMemReplace,
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_ops_function_namespace = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "Fn" | "FnMut" | "FnOnce" => get_path_data_elem_name(def_path_data_iter.next())
                        .map(|n| match n.as_str() {
                            "call" => KnownNames::StdOpsFunctionFnCall,
                            "call_mut" => KnownNames::StdOpsFunctionFnMutCallMut,
                            "call_once" | "call_once_force" => {
                                KnownNames::StdOpsFunctionFnOnceCallOnce
                            }
                            _ => KnownNames::None,
                        })
                        .unwrap_or(KnownNames::None),
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_ops_namespace = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "function" => get_known_name_for_ops_function_namespace(def_path_data_iter),
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_panicking_namespace = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "assert_failed" => KnownNames::StdPanickingAssertFailed,
                    "begin_panic" | "panic" => KnownNames::StdPanickingBeginPanic,
                    "begin_panic_fmt" | "panic_fmt" => KnownNames::StdPanickingBeginPanicFmt,
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_ptr_mut_ptr_namespace =
            |mut def_path_data_iter: Iter<'_>| match path_data_elem_as_disambiguator(
                def_path_data_iter.next(),
            ) {
                Some(0) => get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "write_bytes" => KnownNames::StdIntrinsicsWriteBytes,
                        "cast" => KnownNames::StdPtrMutPtrCast,
                        "add" => KnownNames::StdPtrMutPtrAdd,
                        "sub" => KnownNames::StdPtrMutPtrSub,
                        "offset" => KnownNames::StdPtrMutPtrOffset,
                        "byte_add" => KnownNames::StdPtrMutPtrByteAdd,
                        "byte_sub" => KnownNames::StdPtrMutPtrByteSub,
                        "byte_offset" => KnownNames::StdPtrMutPtrByteOffset,
                        "wrapping_add" => KnownNames::StdPtrMutPtrWrappingAdd,
                        "wrapping_sub" => KnownNames::StdPtrMutPtrWrappingSub,
                        "wrapping_offset" => KnownNames::StdPtrMutPtrWrappingOffset,
                        "wrapping_byte_add" => KnownNames::StdPtrMutPtrWrappingByteAdd,
                        "wrapping_byte_sub" => KnownNames::StdPtrMutPtrWrappingByteSub,
                        "wrapping_byte_offset" => KnownNames::StdPtrMutPtrWrappingByteOffset,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None),
                _ => KnownNames::None,
            };

        let get_known_name_for_ptr_const_ptr_namespace =
            |mut def_path_data_iter: Iter<'_>| match path_data_elem_as_disambiguator(
                def_path_data_iter.next(),
            ) {
                Some(0) => get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "write_bytes" => KnownNames::StdIntrinsicsWriteBytes,
                        "cast" => KnownNames::StdPtrConstPtrCast,
                        "add" => KnownNames::StdPtrConstPtrAdd,
                        "sub" => KnownNames::StdPtrConstPtrSub,
                        "offset" => KnownNames::StdPtrConstPtrOffset,
                        "byte_add" => KnownNames::StdPtrConstPtrByteAdd,
                        "byte_sub" => KnownNames::StdPtrConstPtrByteSub,
                        "byte_offset" => KnownNames::StdPtrConstPtrByteOffset,
                        "wrapping_add" => KnownNames::StdPtrConstPtrWrappingAdd,
                        "wrapping_sub" => KnownNames::StdPtrConstPtrWrappingSub,
                        "wrapping_offset" => KnownNames::StdPtrConstPtrWrappingOffset,
                        "wrapping_byte_add" => KnownNames::StdPtrConstPtrWrappingByteAdd,
                        "wrapping_byte_sub" => KnownNames::StdPtrConstPtrWrappingByteSub,
                        "wrapping_byte_offset" => KnownNames::StdPtrConstPtrWrappingByteOffset,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None),
                _ => KnownNames::None,
            };

        let get_known_name_for_ptr_non_null_namespace = |mut def_path_data_iter: Iter<'_>| {
            def_path_data_iter.next();
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "as_ptr" => KnownNames::StdPtrNonNullAsPtr,
                    "as_mut" => KnownNames::StdPtrNonNullAsMut,
                    "as_ref" => KnownNames::StdPtrNonNullAsRef,
                    "cast" => KnownNames::StdPtrNonNullCast,
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_ptr_unique_namespace = |mut def_path_data_iter: Iter<'_>| {
            def_path_data_iter.next();
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "new_unchecked" => KnownNames::StdPtrUniqueNewUnchecked,
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_ptr_namespace = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "swap_nonoverlapping" => KnownNames::StdPtrSwapNonOverlapping,
                    "mut_ptr" => get_known_name_for_ptr_mut_ptr_namespace(def_path_data_iter),
                    "const_ptr" => get_known_name_for_ptr_const_ptr_namespace(def_path_data_iter),
                    "non_null" => get_known_name_for_ptr_non_null_namespace(def_path_data_iter),
                    "unique" => get_known_name_for_ptr_unique_namespace(def_path_data_iter),
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let get_known_name_for_slice_cmp_namespace =
            |mut def_path_data_iter: Iter<'_>| match path_data_elem_as_disambiguator(
                def_path_data_iter.next(),
            ) {
                Some(0) => get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "memcmp" => KnownNames::StdSliceCmpMemcmp,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None),
                _ => KnownNames::None,
            };

        let get_known_name_for_slice_index_namespace =
            |mut def_path_data_iter: Iter<'_>| match path_data_elem_as_disambiguator(
                def_path_data_iter.next(),
            ) {
                Some(0) => get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "index" => KnownNames::StdSliceIndexIndex,
                        "index_mut" => KnownNames::StdSliceIndexIndexMut,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None),
                _ => KnownNames::None,
            };

        let get_known_name_for_sync_once_namespace =
            |mut def_path_data_iter: Iter<'_>| match path_data_elem_as_disambiguator(
                def_path_data_iter.next(),
            ) {
                Some(2) => get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "call_once" | "call_once_force" => KnownNames::StdOpsFunctionFnOnceCallOnce,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None),
                _ => KnownNames::None,
            };

        let get_known_name_for_raw_vec_namespace =
            |mut def_path_data_iter: Iter<'_>| match path_data_elem_as_disambiguator(
                def_path_data_iter.next(),
            ) {
                Some(1) => get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "MIN_NON_ZERO_CAP" => KnownNames::AllocRawVecMinNonZeroCap,
                        "allocate_in" => KnownNames::AllocRawVecAllocateIn,
                        "current_memory" => KnownNames::AllocRawVecCurrnetMemory,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None),
                _ => get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "allocate_in" => KnownNames::AllocRawVecAllocateIn,
                        "current_memory" => KnownNames::AllocRawVecCurrnetMemory,
                        "grow_amortized" => KnownNames::AllocRawVecGrowAmortized,
                        "grow_exact" => KnownNames::AllocRawVecGrowExact,
                        "finish_grow" => KnownNames::AllocRawVecFinishGrow,
                        "shrink" => KnownNames::AllocRawVecShrink,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None),
            };

        let get_known_name_for_slice_namespace = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "cmp" => get_known_name_for_slice_cmp_namespace(def_path_data_iter),
                    "index" => get_known_name_for_slice_index_namespace(def_path_data_iter),
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        //get_known_name_for_sync_namespace
        let get_known_name_for_sync_namespace = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "once" => get_known_name_for_sync_once_namespace(def_path_data_iter),
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        //get_known_name_for_sync_namespace
        let get_known_name_for_thread_namespace =
            |mut def_path_data_iter: Iter<'_>| match path_data_elem_as_disambiguator(
                def_path_data_iter.next(),
            ) {
                Some(0) => get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "spawn_unchecked" => KnownNames::StdThreadBuilderSpawnUnchecked,
                        "spawn_unchecked_" => KnownNames::StdThreadBuilderSpawnUnchecked_,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None),
                _ => KnownNames::None,
            };

        // get_known_name_for_result_namespace
        let get_known_name_for_result_namespace =
            |mut def_path_data_iter: Iter<'_>| match path_data_elem_as_disambiguator(
                def_path_data_iter.next(),
            ) {
                _ => get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "map_err" => KnownNames::StdResultMapErr,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None),
            };

        let get_known_name_for_convert_namespace =
            |mut def_path_data_iter: Iter<'_>| match path_data_elem_as_disambiguator(
                def_path_data_iter.next(),
            ) {
                _ => get_path_data_elem_name(def_path_data_iter.next())
                    .map(|n| match n.as_str() {
                        "into" => KnownNames::StdConvertInto,
                        _ => KnownNames::None,
                    })
                    .unwrap_or(KnownNames::None),
            };

        let get_known_name_for_known_crate = |mut def_path_data_iter: Iter<'_>| {
            get_path_data_elem_name(def_path_data_iter.next())
                .map(|n| match n.as_str() {
                    "alloc" => get_known_name_for_alloc_namespace(def_path_data_iter),
                    "clone" => get_known_name_for_clone_namespace(def_path_data_iter),
                    "future" => get_known_name_for_future_namespace(def_path_data_iter),
                    "intrinsics" => get_known_name_for_intrinsics_namespace(def_path_data_iter),
                    "marker" => get_known_name_for_marker_namespace(def_path_data_iter),
                    "mem" => get_known_name_for_mem_namespace(def_path_data_iter),
                    "ops" => get_known_name_for_ops_namespace(def_path_data_iter),
                    "panicking" => get_known_name_for_panicking_namespace(def_path_data_iter),
                    "ptr" => get_known_name_for_ptr_namespace(def_path_data_iter),
                    "raw_vec" => get_known_name_for_raw_vec_namespace(def_path_data_iter),
                    "result" => get_known_name_for_result_namespace(def_path_data_iter),
                    "rt" => get_known_name_for_panicking_namespace(def_path_data_iter),
                    "slice" => get_known_name_for_slice_namespace(def_path_data_iter),
                    "sync" => get_known_name_for_sync_namespace(def_path_data_iter),
                    "thread" => get_known_name_for_thread_namespace(def_path_data_iter),
                    "convert" => get_known_name_for_convert_namespace(def_path_data_iter),
                    _ => KnownNames::None,
                })
                .unwrap_or(KnownNames::None)
        };

        let crate_name = tcx.crate_name(def_id.krate);
        match crate_name.as_str() {
            "alloc" | "core" | "std" => get_known_name_for_known_crate(def_path_data_iter),
            _ => KnownNames::None,
        }
    }
}
