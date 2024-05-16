// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test that uses a box containing a dynamic FnOnce object.

fn foo(f: Box<dyn FnOnce(u32) -> u32>) {
    f(2);
}

fn main() {    
    let c = |x: u32| -> u32 { x * 2 };
    let b: Box<dyn FnOnce(u32) -> u32> = Box::new(c);
    foo(b);
}
