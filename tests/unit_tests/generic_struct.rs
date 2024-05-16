// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of strcutures with generic arguments.

struct Foo<'a, T> {
    f: &'a T,
}

struct Bar<'a, 'b, T, U> {
    a: &'a T,
    b: &'b U
}

impl<'a, T> Foo<'a, T> {
    fn get_f(&self) -> &'a T {
        self.f
    }
}

impl<'a, 'b, T, U> Bar<'a, 'b, T, U> {
    fn get_ab(&self) -> (&'a T, &'b U) {
        (self.a, self.b)
    }
}

fn foo_field<T>(foo: Foo<T>) -> &T {
    foo.f
}

fn bar_fields<'a, 'b, T, U>(bar: Bar<'a, 'b, T, U>) -> (&'a T, &'b U) {
    (bar.a, bar.b)
}

#[allow(unused)]
fn main() {
    let x = 3;
    let y = 4.0;
    let foo = Foo { f: &x };
    let bar = Bar { a: &x, b: &y };
    let f1 = foo.get_f();
    let f2 = foo_field(foo);
    let (a1, b1) = bar.get_ab();
    let (a2, b2) = bar_fields(bar);
}