// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of opaque types.

trait T {
    fn foo(&self);
}

#[allow(unused)]
struct A<'a> {
    a: &'a u32,
}

impl<'a> T for A<'a> {
    fn foo(&self) {}
}

#[allow(unused)]
struct B<'b> {
    b: &'b f32,
}

impl<'b> T for B<'b> {
    fn foo(&self) {}
}

fn opaque(t: impl T) -> impl T {
    return t;
}

fn opaque_wrapper(t: impl T) -> impl T {
    return opaque(t);
}

fn main() {
    let x = 2;
    let y = 2.0;
    let a = A { a: &x };
    let b = B { b: &y };
    
    let t1 = opaque(a);
    t1.foo();
    let t2 = opaque(b);
    t2.foo();

    let t3 = opaque_wrapper(t1);
    t3.foo();
    let t4 = opaque_wrapper(t2);
    t4.foo();
}
