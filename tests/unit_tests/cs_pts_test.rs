// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test for validating the effectiveness of call-site sensitivity
// in computing points-to information.

fn id(x: &u32) -> &u32 {
    x
}

fn id2(x: &u32) -> &u32 {
    id(x)
}

#[allow(unused)]
fn for_cs1() {
    let x = 2;
    let p = id(&x);
    let y = 3;
    let q = id(&y);
}

#[allow(unused)]
fn for_cs2() {
    let x = 2;
    let p = id2(&x);
    let y = 3;
    let q = id2(&y);
}

fn main() {
    for_cs1();
    for_cs2();
}
