// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of slices.

fn main() {
    let s = String::from("hello");

    let len = s.len();

    let _slice1 = &s[3..len];
    let _slice2 = &s[3..];

    let a = [1, 2, 3, 4, 5];
    let _slice3 = &a[1..3];
}