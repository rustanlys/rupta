// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that uses enums.

pub enum Foo<'a> {
    A(&'a i32),
    B(&'a i32),
}

fn enum_match(foo: Foo) -> &i32 {
    match foo {
        Foo::A(x) => {
            x
        }
        Foo::B(x) => {
            x
        },
    }
}

fn enum_ref_match<'f, 'a>(foo: &'f Foo<'a>) -> &'a i32 {
    match foo {
        Foo::A(x) => {
            x
        }
        Foo::B(x) => {
            x
        },
    }
}

#[allow(unused)]
pub fn main() {
    let a = 2;
    let b = 3;
    let f1 = Foo::A(&a);
    let f2 = Foo::B(&b);
    
    let x1 = enum_ref_match(&f1);
    let x2 = enum_ref_match(&f2);
    let x3 = enum_match(f1);
    let x4 = enum_match(f2);
}