// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that uses constant generics.

const CONST_N: usize = 3;

/// A generic struct type
#[allow(unused)]
struct Foo<'a, const N: usize> {
    arr: [u32; N],
    ptr: &'a u32
}

fn new_foo<'a, const N: usize>(elem: u32, ptr: &'a u32) -> Foo<'a, N> {
    Foo {
        arr: [elem; N],
        ptr
    }
}

fn const_arith() {
    const M: usize = 3 + 2;
    const N: usize = M + 3; 
    let x = 2;
    let _foo = new_foo::<N>(x, &x);
}

fn const_global() {
    let x = 2;
    let _foo = new_foo::<CONST_N>(x, &x);
}

fn main() {
    let _foo = new_foo::<3>(2, &2);
    const_arith();
    const_global();
}