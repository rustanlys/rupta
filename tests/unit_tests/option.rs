// Copyright (c) 2024 <Wei Li>. 
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of Option.

fn main() {
    let a = 2;
    let o: Option<&u32> = Some(&a);

    if o.is_some() {
        let _x = o.unwrap();
    }
} 
