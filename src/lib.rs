// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

#![feature(
    rustc_private,             // for rustc internals
    box_patterns,              // for conciseness
    associated_type_defaults,  // for crate::indexed::Indexed
    min_specialization,        // for rustc_index::newtype_index
    type_alias_impl_trait,     // for impl Trait in trait definition, eg crate::mir::utils 
    trait_alias,
)]
#![allow(
    clippy::single_match,
    clippy::needless_lifetimes,
    clippy::needless_return,
    clippy::len_zero
)]

extern crate rustc_borrowck;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_interface;
extern crate rustc_macros;
extern crate rustc_middle;
extern crate rustc_serialize;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_target;

pub mod builder;
pub mod graph;
pub mod mir;
pub mod pta;
pub mod rta;
pub mod pts_set;
pub mod util;
