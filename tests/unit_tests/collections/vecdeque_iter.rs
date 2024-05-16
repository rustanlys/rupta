// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of VecDeque.

use std::collections::VecDeque;

pub fn main() {
    let a = 1;
    let b = 2;
    let c = 3;
    let mut v: VecDeque<&i32> = VecDeque::new();
    v.push_back(&a);
    v.push_back(&b);
    v.push_back(&c);

    for d in v.iter() {
        let _e = *d;
    }
}
