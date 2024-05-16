// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that transmutes structs.

use std::marker::PhantomData;

#[allow(unused)]
struct A<'a> {
    x: &'a u32,
    arr: [&'a u32; 2],
    tup: (&'a u32, &'a u32),
}

#[allow(unused)]
struct B<'b> {
    placeholder: (),
    phantom: PhantomData<u32>,
    x: &'b u32,
}

fn transmute_struct_a() {
    let x = 2;
    let y = 3;
    let z = 4;
    let m = 5;
    let n = 6;
    
    let a = A {
        x: &x,
        arr: [&y, &z],
        tup: (&m, &n),
    };
    
    let b = unsafe { std::mem::transmute::<A, [&mut u32; 5]>(a) };
    let _x = *b[0];
}


fn transmute_struct_b() {
    let x = 2;
    let b = B {
        placeholder: (),
        phantom: PhantomData,
        x: &x,
    };

    let _x = unsafe { std::mem::transmute::<B, &u32>(b) };
}

fn main() {
    transmute_struct_a();
    transmute_struct_b();
}
