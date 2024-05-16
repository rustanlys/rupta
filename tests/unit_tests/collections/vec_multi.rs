// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of vector.

fn foo() {
    let a = 1;
    let b = 2;
    let c = 3;
    let mut v1 = vec![&a, &b, &c];

    let d = 1.0;
    let e = 2.0;
    let f = 3.0;
    let mut v2 = vec![&d, &e, &f];

    let m = 4;
    let n = 4.0;
    v1.push(&m);
    v2.push(&n);

    let _x = v1[1];
    let _y = v2[1];
}

fn bar() {
    let a = 1;
    let b = 2;
    let c = 3;
    let mut v1 = Vec::new();
    v1.push(&a);
    v1.push(&b);
    v1.push(&c);

    let d = 1.0;
    let e = 2.0;
    let f = 3.0;
    let mut v2 = Vec::new();
    v2.push(&d);
    v2.push(&e);
    v2.push(&f);

    let _x = v1[1];
    let _y = v2[1];
}

fn main() {
    foo();
    bar();
}