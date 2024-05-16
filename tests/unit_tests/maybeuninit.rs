// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that uses MaybeUninit.
// MaybeUninit is transparent struct type that is widely used in the standard library.
// A MaybeUninit<T> struct object can be directly cast to its inner type T.

use std::mem::MaybeUninit;

fn cast(m: *const MaybeUninit<&i32>) -> &i32 {
    let ret = m as *const &i32;
    unsafe { *ret }
}

fn main() {
    let a1 = 2;
    let a2 = 3; 
    let b1 = &a1;
    let b2 = &a2;
    
    let c1 = &b1 as *const &i32;
    let m1 = c1 as *const MaybeUninit<&i32>;
    
    let c2 = MaybeUninit::<&i32>::new(b2);
    let m2 = &c2 as *const MaybeUninit<&i32>;
    
    let _d1 = cast(m1);
    let _d2 = cast(m2);
}