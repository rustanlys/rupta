// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test that uses a box containing a struct.

struct Foo<'f> {
    f: &'f u32
}

struct Bar<'b> {
    b: &'b i32
}

#[allow(unused)]
pub fn main() {
    let m: u32 = 2;
    let foo = Foo { f: &m };
    let n: i32 = 3;
    let bar = Bar { b: &n };

    // Box::new is a generic function and will be monomorphized in 
    // our analysis. Even in a context-insensitive configuration,
    // the underlying pointer of b1 and b2 will point to different 
    // heap allocations. 
    let b1 = Box::new(foo);
    let b2 = Box::new(bar);
}