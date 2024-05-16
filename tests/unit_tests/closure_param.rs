// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that checks closure with parameters.

fn foo<F>(f: F, a: &i32, b: &i32) -> i32
where
    F: FnOnce(&i32, &i32) -> i32,
{
    f(a, b)
}

fn main() {
    let f = |a: &i32, b: &i32| *a + *b;
    let a = 2;
    let b = 3;
    f(&a, &b);
    foo(f, &a, &b);
}