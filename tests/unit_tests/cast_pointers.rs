// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that uses PtrToPtr casts.

// Const raw pointer to mut raw pointer
// PtrToPtr
#[allow(unused)]
fn const_to_mut() {
    let x = 2;
    let p1 = &x as *const i32;
    let p2 = p1 as *mut i32;
}

// Mut raw pointer to const raw pointer
// PointerCoercion(MutToConstPointer)
#[allow(unused)]
fn mut_to_const() {
    let mut x = 2;
    let p1 = &mut x as *mut i32;
    let p2 = p1 as *const i32;
}

// PtrToPtr
#[allow(unused)]
fn tuple_to_array() {
    let x = 2;
    let y = 3;
    let z = 4;
    let mut t = (&x, &y);
    let p1 = &mut t as *mut (&i32, &i32);
    let p2 = p1 as *mut [&u32; 2];
    unsafe { (*p2)[1] = &z };
}

// PtrToPtr
#[allow(unused)]
fn array_to_tuple() {
    let x = 2;
    let y = 3;
    let z = 4;
    let mut arr = [&x, &y];
    let p1 = &mut arr as *mut [&i32; 2];
    let p2 = p1 as *mut (&u32, &u32);
    unsafe { (*p2).0 = &z };
}

fn main() {
    const_to_mut();
    mut_to_const();
    tuple_to_array();
    array_to_tuple();
}