// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of vector.

struct A<'a> {
    x: &'a u32,
}

fn main() {
    let x = 1;
    let y = 2;
    let z = 3;
    let a = A {x: &x};
    let b = A {x: &y};
    let c = A {x: &z};
    
    let mut v = Vec::new();
    v.push(a);
    v.push(b);
    v.push(c);

    for e in v.iter() {
        let _f = e.x;
    }
}