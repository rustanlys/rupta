// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of generic functions.
// A generic function is monomorphized when lowering MIR to a lower-level IR.
// In our analysis, we analyze a generic function separately for different concrete types.

#[allow(unused)]
struct Foo<'a> {
    f: &'a i32,
}

fn generic_id<T>(t: T) -> T {
    t
}

fn generic_id_wrapper<T>(t: T) -> T {
    generic_id(t)
}

#[allow(unused)]
fn multi_generics<T, U>(t: T, u: U) {
    let x = generic_id(t);
    let y = generic_id(u);
}

fn main() {
    let x = 32;
    let p = generic_id_wrapper(&x);

    let f = Foo { f: &x };
    let q = generic_id_wrapper(f);

    multi_generics(p, q);
}