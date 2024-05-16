// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test for validating the effectiveness of object sensitivity.

struct A {}

impl A {
    fn id<'a>(&self, x: &'a u32) -> &'a u32 {
        x
    }

    fn id2<'a>(&self, x: &'a u32) -> &'a u32 {
        self.id(x)
    }

    fn id3<'a>(&self, x: &'a u32) -> &'a u32 {
        self.id2(x)
    }
}

fn bar() {
    let a = A {};
    let x = 2;
    let y = 3;

    // Object-sensitive PTA will conclude that both p1 and p2 point to {x, y}
    let _p1 = a.id(&x);
    let _p2 = a.id(&y);
}

fn baz() {
    let a1 = A {};
    let a2 = A {};
    let x = 2;
    let y = 3;

    // Object-sensitive PTA will conclude that p1 and p2 point to {x} and {y} respectively.
    let _p1 = a1.id3(&x);
    let _p2 = a2.id3(&y);
}

fn main() {
    bar();
    baz();
}