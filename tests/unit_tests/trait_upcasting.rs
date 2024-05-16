// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of trait upcasting.

#![feature(trait_upcasting)]

trait Foo {
    fn foo(&self);
}

trait Bar: Foo {}

impl<T: Foo + ?Sized> Bar for T {}

struct A {}

impl Foo for A {
    fn foo(&self) {}
}

fn get() -> Box<dyn Bar> {
    return Box::new(A{});
} 

fn dyn_bar() {
    let b: Box<dyn Bar> = get();
    b.foo();
}

fn trait_upcasting() {
    let f: Box<dyn Foo> = get();
    f.foo();
}

fn main() {
    dyn_bar();
    trait_upcasting();
}
