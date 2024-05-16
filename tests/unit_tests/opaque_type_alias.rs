// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of opaque type alias.

#![feature(type_alias_impl_trait)]

mod m {
    pub type Seq<T> = impl IntoIterator<Item = T>;

    pub fn produce_singleton<T>(t: T) -> Seq<T> {
        vec![t]
    }

    #[allow(unused)]
    pub fn produce_doubleton<T>(t: T, u: T) -> Seq<T> {
        vec![t, u]
    }
}

fn is_send<T: Send>(_: &T) {}

pub fn main() {
    let elems = m::produce_singleton(22);

    is_send(&elems);
}