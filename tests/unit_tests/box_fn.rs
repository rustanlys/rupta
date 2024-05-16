// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test that uses a box containing a dynamic Fn object.

fn foo(f: Box<dyn Fn(u32) -> u32>) {
    f(2);
}

fn main() {    
    let fp: fn(u32) -> u32 = |x: u32| x * 2;
    let b: Box<dyn Fn(u32) -> u32> = Box::new(fp);
    foo(b);
}
