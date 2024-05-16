// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of vector.

struct Foo<'foo> {
    f: &'foo u32
}

struct Bar<'bar> {
    f: &'bar i32
}

fn foo() {
    let x = 1;
    let y = 2;
    let z = 3;
    let f1 = Foo { f: &x };
    let f2 = Foo { f: &y };
    let f3 = Foo { f: &z };

    let mut v = Vec::new();
    v.push(f1);
    v.push(f2);
    v.push(f3);

    let _f = v.get(1).unwrap().f;
}

fn bar() {
    let x = 1;
    let y = 2;
    let z = 3;
    let b1 = Bar { f: &x };
    let b2 = Bar { f: &y };
    let b3 = Bar { f: &z };

    let mut v = Vec::new();
    v.push(b1);
    v.push(b2);
    v.push(b3);
    
    let _f = v.get(1).unwrap().f;
}

fn main() {
    foo();
    bar();
}
