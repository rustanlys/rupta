// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that explicitly calls fn_once.

#![feature(fn_traits)]

fn id(x: &i32) -> &i32 {
    x
}

fn main() {
    let x = 2;
    let fn_once = std::ops::FnOnce::call_once;
    let p = fn_once(&id, (&x, ));
}