// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that casts a struct containing a option field.

#[allow(unused)]
struct A<'a> {
    opt: Option<&'a u32>,
}

fn main() {
    let x = 3;
    let y = 5;
    let mut a = A { opt: Some(&x) };
    let ap = &mut a as *mut A;
    let bp = ap as *mut Option<&u32>;
    let b = unsafe { &mut *bp };
    if let Some(v) = b {
        *v = &y;
    }
}