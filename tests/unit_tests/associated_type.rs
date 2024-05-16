// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that checks the specialization of associated types in traits.

trait Foo {
    fn foo(&self) {}
}

trait Bar { 
    type N;
    type M: Foo;

    // The concrete type of N and M need to be derived based on the 
    // specialized type of self.
    fn bar(&self, _n: &Self::N, m: &Self::M) {
        m.foo();
    } 

    fn baz(&self, _n: &Self::N, m: &Self::M);
}

struct A;

impl Foo for A {}

struct B;

impl Bar for B {
    type N = i32;
    type M = A;

    fn baz(&self, _n: &Self::N, m: &Self::M) {
        m.foo();
    }
} 

fn main() {
    let a = A;
    let b = B;
    let x = 2;
    b.bar(&x, &a);
    b.baz(&x, &a);
}