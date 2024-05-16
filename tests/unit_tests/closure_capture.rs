// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of closures that capture variables from its surrrounding context.

fn closure_immutable() {
    let a = 2;
    let b = &a;
    let f = |x| *b * x;
    f(2);
}

fn closure_mutable() {
    let mut a = 2;
    let b = &mut a;
    let mut f = |x| {
        *b += 1;
        *b * x
    };
    f(2);
}

fn main() {
    closure_immutable();
    closure_mutable();
}