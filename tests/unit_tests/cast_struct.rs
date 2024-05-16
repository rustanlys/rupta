// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that casts a struct.

#[allow(unused)]
struct Foo<'a, 'b> {
    a: &'a i32,
    b: &'b i32,
}

#[allow(unused)]
fn cast_struct() {
    let x = 2;
    let y = 3;
    let foo = Foo {
        a: &x,
        b: &y,
    };
    let p = &foo as *const Foo;
    let q = p as *const (&i32, &i32);
    let e = unsafe { (*q).0 };
}

#[allow(unused)]
fn cast_struct_ref() {
    let x = 2;
    let y = 3;
    let foo = Foo {
        a: &x,
        b: &y,
    };
    let p = &&foo as *const &Foo;
    let q = p as *const &(&i32, &i32);
    let e = unsafe { (**q).0 };
}

fn main() {
    cast_struct();
    cast_struct_ref();
}