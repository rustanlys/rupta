// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that initializes, copies and moves structs.

#[allow(unused)]
struct Foo<'a, 'b> {
    x: &'a i32,
    y: &'b i32,
}

#[allow(unused)]
#[derive(Copy, Clone)]
struct Bar<'a, 'b> {
    x: &'a i32,
    y: &'b i32,
}

impl<'a, 'b> Foo<'a, 'b> {
    fn new(x: &'a i32, y: &'b i32) -> Foo<'a, 'b> {
        Foo { x, y }
    }

    fn foo(&self) {}
}

impl<'a, 'b> Bar<'a, 'b> {
    fn bar(&self) {}
}

fn struct_new() {
    let a = 2;
    let b = 3;
    let _foo = Foo::new(&a, &b);
}

fn struct_move() {
    let a = 2;
    let b = 3;
    let foo1 = Foo { x: &a, y: &b };
    let foo2 = foo1;
    foo2.foo();
}

fn struct_copy() {
    let a = 2;
    let b = 3;
    let bar1 = Bar { x: &a, y: &b };
    let bar2 = bar1;
    bar2.bar();
}

fn struct_ref() {
    let a = 2;
    let b = 3;
    let c = 4;
    let mut foo = Foo { x: &a, y: &b };
    let p = &mut foo;
    (*p).x = &c;
}

fn main() {
    struct_new();
    struct_move();
    struct_copy();
    struct_ref();
}