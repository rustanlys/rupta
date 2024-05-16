// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of structures' layouts.
// The layout of an adt type is not guaranteed to be identical to the definition
// of the type.

#[derive(Copy, Clone)]
struct B {
    m: bool,
    n: u16,
    p: u32,
}

struct C {
    b: B,
    f: f64,
}


fn main() {
    let b = B { m: true, n: 3, p: 23 };
    let c = C { b, f: 32.0};

    let mut d = unsafe { std::mem::transmute::<B, [u8; 8]>(b) };
    let mut e = unsafe { std::mem::transmute::<C, [u8; 16]>(c) };
}