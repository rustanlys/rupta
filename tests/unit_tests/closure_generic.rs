// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that checks closure with generics.

trait Getter {
    fn get(&self) -> &i32;
}

struct Foo<'f> {
    f: &'f i32
}

struct Bar<'b> {
    b: &'b i32
}

impl<'f> Getter for Foo<'f> {
    fn get(&self) -> &i32 { self.f }
}

impl<'b> Getter for Bar<'b> {
    fn get(&self) -> &i32 { self.b }
}

fn generic_closure_call<T: Getter>(g: &impl Getter, t: &T) -> i32 {
    // This closure captures a generic variable g and accepts a generic parameter x
    let f = |x: &T| *g.get() + *x.get();
    f(t)
}

fn main() {
    let x = 1;
    let y = 2;
    let foo = Foo { f: &x };
    let bar = Bar { b: &y };
    generic_closure_call(&foo, &bar);
}