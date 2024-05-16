// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of vector.

fn main() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let mut v = Vec::with_capacity(4);
    v.push(&a);
    v.push(&b);
    v.push(&c);

    let _e = v[1];
    v.push(&d);
    let _f = *v.get(2).unwrap();
}