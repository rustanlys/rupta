// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! Specializes generic types to concrete types.
//! 
//! Adapted primarily from the code in [MIRAI](<https://github.com/facebookexperimental/MIRAI>).
//! 
//! For example:
//!
//! ```no_run
//! fn foo<T>(t: T) {}
//! fn bar<U, V>(u: U, v: V) { foo(u); foo(v); }
//! fn main() { bar(3, 4.0); }
//! ```
//!
//! The function `bar` is invoked in `main` with generic arguments `[i32, f64]`.
//! During analysis, we specialize the types of `u` and `v` in `bar` to `i32` and `f64` respectively.
//! The calls to `foo(u)` and `foo(v)` can therefore be resolved to `foo::<i32>(u)` and 
//! `foo::<f64>(v)` respectively.

use log::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::ops::DerefMut;

use rustc_middle::ty::{GenericArg, GenericArgKind, GenericArgsRef};
use rustc_middle::ty::{
    Const, ConstKind, ExistentialPredicate, ExistentialProjection, ExistentialTraitRef, 
    FnSig, ParamConst, ParamTy, Ty, TyCtxt, TyKind,
};
use rustc_span::def_id::DefId;

use crate::mir::function::GenericArgE;
use crate::util::type_util;


pub struct SubstsSpecializer<'tcx> {
    pub tcx: TyCtxt<'tcx>,
    pub generic_args: Vec<GenericArgE<'tcx>>,
    pub closures_being_specialized: RefCell<HashSet<DefId>>,
}

impl<'tcx> SubstsSpecializer<'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>, generic_args: Vec<GenericArgE<'tcx>>) -> SubstsSpecializer<'tcx> {
        SubstsSpecializer {
            tcx,
            generic_args,
            closures_being_specialized: RefCell::new(HashSet::new()),
        }
    }

    pub fn specialize_generic_args(&self, args: GenericArgsRef<'tcx>) -> GenericArgsRef<'tcx> {
        let specialized_generic_args: Vec<GenericArg<'_>> = args
            .iter()
            .map(|gen_arg| self.specialize_generic_argument(gen_arg))
            .collect();
        self.tcx.mk_args(&specialized_generic_args)
    }

    fn specialize_generic_argument(&self, gen_arg: GenericArg<'tcx>) -> GenericArg<'tcx> {
        match gen_arg.unpack() {
            GenericArgKind::Type(ty) => self.specialize_generic_argument_type(ty).into(),
            GenericArgKind::Const(c) => self.specialize_const(c).into(),
            _ => gen_arg,
        }
    }

    fn specialize_const(&self, constant: Const<'tcx>) -> Const<'tcx> {
        if let ConstKind::Param(ParamConst { index, name: _ }) = constant.kind() {
            match self.generic_args[index as usize] {
                GenericArgE::Const(c) => c,
                _ => {
                    error!("Unmatched constant generic argument: {:?}({:?})", 
                        self.generic_args[index as usize], 
                        constant.kind()
                    );
                    constant
                }
            }
        } else {
            constant
        }
    }

    pub fn specialize_generic_argument_type(&self, gen_arg_type: Ty<'tcx>) -> Ty<'tcx> {
        debug!("Specializing generic arg ty {:?}", gen_arg_type);
        // The projection of an associated type. For example,
        // `<T as Trait<..>>::N`.
        if let TyKind::Alias(rustc_middle::ty::Projection, projection) = gen_arg_type.kind() {
            let specialized_substs = self.specialize_generic_args(projection.args);
            let item_def_id = projection.def_id;
            return if type_util::are_concrete(specialized_substs) {
                let param_env = self
                    .tcx
                    .param_env(self.tcx.associated_item(item_def_id).container_id(self.tcx));
                if let Ok(Some(instance)) = rustc_middle::ty::Instance::resolve(
                    self.tcx, 
                    param_env, 
                    item_def_id, 
                    specialized_substs
                ) {
                    let instance_item_def_id = instance.def.def_id();
                    if item_def_id == instance_item_def_id {
                        // Resolve the concrete type for FnOnce::Output alias type.
                        // It may omit to resolve a closure's output type, in which case 
                        // the resolved instance_item_def_id may correspond to FnOnce::call_once 
                        // instead of FnOnce::Output, leading to item_def_id not equal to instance_item_def_id.
                        if type_util::is_fn_once_output(self.tcx, instance_item_def_id) {
                            if specialized_substs.len() > 0 {
                                if let Some(ty) = specialized_substs[0].as_type() {
                                    match ty.kind() {
                                        TyKind::FnDef(def_id, gen_args) => {
                                            let specialized_type = type_util::function_return_type(self.tcx, *def_id, gen_args);
                                            debug!("FnOnce::Output ({:?}) specialized to {:?}", ty, specialized_type);
                                            return specialized_type;
                                        }
                                        TyKind::Closure(def_id, gen_args) => {
                                            let specialized_type = type_util::closure_return_type(self.tcx, *def_id, gen_args);
                                            debug!("FnOnce::Output ({:?}) specialized to {:?}", ty, specialized_type);
                                            return specialized_type;
                                        }
                                        TyKind::FnPtr(fn_sig) => {
                                            let specialized_type = fn_sig.skip_binder().output();
                                            debug!("FnOnce::Output ({:?}) specialized to {:?}", ty, specialized_type);
                                            return specialized_type;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        return Ty::new_projection(self.tcx, projection.def_id, specialized_substs);
                    }
                    let item_type = self.tcx.type_of(instance_item_def_id).skip_binder();
                    if type_util::is_fn_once_output(self.tcx, item_def_id) && 
                        type_util::is_fn_once_call_once(self.tcx, instance_item_def_id)
                    {
                        if specialized_substs.len() > 0 {
                            if let Some(ty) = specialized_substs[0].as_type() {
                                if let TyKind::Closure(def_id, gen_args) = ty.kind() {
                                    let specialized_type = type_util::closure_return_type(self.tcx, *def_id, gen_args);
                                        debug!("FnOnce::Output ({:?}) specialized to {:?}", ty, specialized_type);
                                        return specialized_type;
                                }
                            }
                        }
                    }
                    let tmp_generic_args = instance.args.iter().map(|t| GenericArgE::from(&t)).collect();
                    let tmp_specializer = SubstsSpecializer::new(self.tcx, tmp_generic_args);
                    tmp_specializer.specialize_generic_argument_type(item_type)
                } else {
                    let projection_trait = Some(self.tcx.parent(item_def_id));
                    if projection_trait == self.tcx.lang_items().pointee_trait() {
                        assert!(!specialized_substs.is_empty());
                        if let GenericArgKind::Type(ty) = specialized_substs[0].unpack() {
                            return ty.ptr_metadata_ty(self.tcx, |ty| ty).0;
                        }
                    } else if projection_trait == self.tcx.lang_items().discriminant_kind_trait() {
                        assert!(!specialized_substs.is_empty());
                        if let GenericArgKind::Type(enum_ty) = specialized_substs[0].unpack() {
                            return enum_ty.discriminant_ty(self.tcx);
                        }
                    }
                    warn!("Could not resolve an associated type with concrete type arguments");
                    gen_arg_type
                }
            } else {
                Ty::new_projection(self.tcx, projection.def_id, specialized_substs)
            };
        }

        // If the type is an opaque type, substitute it with the concrete type.
        // An opaque type is usually from impl Trait in type aliases or function return types
        if let TyKind::Alias(
            rustc_middle::ty::Opaque, 
            rustc_middle::ty::AliasTy { def_id, args, .. }
        ) = gen_arg_type.kind() {
            let gen_args = self
                .specialize_generic_args(args)
                .iter()
                .map(|t| GenericArgE::from(&t))
                .collect();
            let underlying_type = self.tcx.type_of(def_id).skip_binder();
            let specialized_type =
                SubstsSpecializer::new(self.tcx, gen_args).specialize_generic_argument_type(underlying_type);
            // debug!("Opaque type {:?} specialized to {:?}", gen_arg_type, specialized_type);
            return specialized_type;
        }

        match gen_arg_type.kind() {
            TyKind::Adt(def, args) => {
                Ty::new_adt(self.tcx, *def, self.specialize_generic_args(args))
            }
            TyKind::Array(elem_ty, len) => {
                let specialized_elem_ty = self.specialize_generic_argument_type(*elem_ty);
                let specialized_len = self.specialize_const(*len);
                self.tcx
                    .mk_ty_from_kind(TyKind::Array(specialized_elem_ty, specialized_len))
            }
            TyKind::Slice(elem_ty) => {
                let specialized_elem_ty = self.specialize_generic_argument_type(*elem_ty);
                Ty::new_slice(self.tcx, specialized_elem_ty)
            }
            TyKind::RawPtr(rustc_middle::ty::TypeAndMut { ty, mutbl }) => {
                let specialized_ty = self.specialize_generic_argument_type(*ty);
                Ty::new_ptr(
                    self.tcx, 
                    rustc_middle::ty::TypeAndMut {
                        ty: specialized_ty,
                        mutbl: *mutbl,
                    })
            }
            TyKind::Ref(region, ty, mutbl) => {
                let specialized_ty = self.specialize_generic_argument_type(*ty);
                Ty::new_ref(
                    self.tcx, 
                    *region,
                    rustc_middle::ty::TypeAndMut {
                        ty: specialized_ty,
                        mutbl: *mutbl,
                    },
                )
            }
            TyKind::FnDef(def_id, substs) => {
                Ty::new_fn_def(self.tcx, *def_id, self.specialize_generic_args(substs))
            }
            TyKind::FnPtr(fn_sig) => {
                let map_fn_sig = |fn_sig: FnSig<'tcx>| {
                    let specialized_inputs_and_output = self.tcx.mk_type_list_from_iter(
                        fn_sig
                            .inputs_and_output
                            .iter()
                            .map(|ty| self.specialize_generic_argument_type(ty)),
                    );
                    FnSig {
                        inputs_and_output: specialized_inputs_and_output,
                        c_variadic: fn_sig.c_variadic,
                        unsafety: fn_sig.unsafety,
                        abi: fn_sig.abi,
                    }
                };
                let specialized_fn_sig = fn_sig.map_bound(map_fn_sig);
                Ty::new_fn_ptr(self.tcx, specialized_fn_sig)
            }
            TyKind::Dynamic(predicates, region, kind) => {
                let specialized_predicates = predicates.iter().map(
                    |bound_pred: rustc_middle::ty::Binder<'_, ExistentialPredicate<'tcx>>| {
                        bound_pred.map_bound(|pred| match pred {
                            ExistentialPredicate::Trait(ExistentialTraitRef { def_id, args }) => {
                                ExistentialPredicate::Trait(ExistentialTraitRef {
                                    def_id,
                                    args: self.specialize_generic_args(args),
                                })
                            }
                            ExistentialPredicate::Projection(ExistentialProjection {
                                def_id,
                                args,
                                term,
                            }) => {
                                if let Some(ty) = term.ty() {
                                    ExistentialPredicate::Projection(ExistentialProjection {
                                        def_id,
                                        args: self.specialize_generic_args(args),
                                        term: self.specialize_generic_argument_type(ty).into(),
                                    })
                                } else {
                                    ExistentialPredicate::Projection(ExistentialProjection {
                                        def_id,
                                        args: self.specialize_generic_args(args),
                                        term,
                                    })
                                }
                            }
                            ExistentialPredicate::AutoTrait(_) => pred,
                        })
                    },
                );
                Ty::new_dynamic(
                    self.tcx,
                    self.tcx
                        .mk_poly_existential_predicates_from_iter(specialized_predicates),
                    *region,
                    *kind,
                )
            }
            TyKind::Closure(def_id, args) => {
                // Closure types can be part of their own type parameters...
                // so need to guard against endless recursion
                {
                    let mut borrowed_closures_being_specialized =
                        self.closures_being_specialized.borrow_mut();
                    let closures_being_specialized = 
                        borrowed_closures_being_specialized.deref_mut();
                    if !closures_being_specialized.insert(*def_id) {
                        return gen_arg_type;
                    }
                }
                let specialized_closure = 
                    Ty::new_closure(self.tcx, *def_id, self.specialize_generic_args(args));
                let mut borrowed_closures_being_specialized = 
                    self.closures_being_specialized.borrow_mut();
                let closures_being_specialized = borrowed_closures_being_specialized.deref_mut();
                closures_being_specialized.remove(def_id);
                specialized_closure
            }
            TyKind::Coroutine(def_id, args) => Ty::new_coroutine(
                self.tcx,
                *def_id,
                self.specialize_generic_args(args), 
            ),
            TyKind::CoroutineWitness(_def_id, _args) => {
                // Todo: specialize generic arguments for a CoroutineWitness type 
                gen_arg_type
            }
            TyKind::Tuple(types) => Ty::new_tup_from_iter(
                self.tcx,
                types
                    .iter()
                    .map(|ty| self.specialize_generic_argument_type(ty))
            ),
            TyKind::Param(ParamTy { index, name: _ }) => match self.generic_args[*index as usize] {
                GenericArgE::Type(ty) => ty,
                _ => {
                    error!(
                        "Unexpected param type: {:?}({:?})",
                        self.generic_args[*index as usize],
                        gen_arg_type.kind()
                    );
                    unreachable!();
                }
            },
            _ => gen_arg_type,
        }
    }
}
