// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of a vector of dynamic trait objects.

trait T {
    fn foo(&self) {}
}

#[allow(unused)]
struct A<'a> {
    f: &'a u32
}
#[allow(unused)]
struct B<'b> {
    f: &'b u32
}

impl<'a> T for A<'a> {
    fn foo(&self) {
    }
}

impl<'a> T for B<'a> {
    fn foo(&self) {
    }
}

fn main() {
    let x = 1;
    let y = 2;
    let z = 3;

    let a1 = A { f: &x };
    let a2 = A { f: &y };
    let b1 = B { f: &z };

    let p1: &dyn T = &a1;
    let p2: &dyn T = &a2;
    let p3: &dyn T = &b1;

    let mut v: Vec<&dyn T> = Vec::new();  
    v.push(p1);
    v.push(p2);
    v.push(p3);
    
    let t = v.get(1).unwrap();
    t.foo();
}
