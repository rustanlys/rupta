// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that checks reference and dereference.

#[allow(unused)]
fn main() {
    let a = 1;
    let r = &&a;
    let d = *r;
}