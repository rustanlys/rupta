// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that casts an array from *const [T; N] to *const T.
// Although a specific kind of pointer cast `PointerCoercion::ArrayToPointer`
// that casts from `*const [T; N]` to `*const T`, is defined in Rust MIR, 
// the cast in this test is still translated into a a `CastKind::PtrToPtr`
// cast currently.
// https://doc.rust-lang.org/beta/nightly-rustc/rustc_middle/ty/adjustment/enum.PointerCoercion.html

#[allow(unused)]
fn main() {
    let mut x = 2;
    let mut y = 3;
    
    let arr = [&x, &y];
    let p1 = &arr as *const [&i32; 2];
    let p2 = p1 as *const &i32;
    let e = unsafe { *p2 };
}  