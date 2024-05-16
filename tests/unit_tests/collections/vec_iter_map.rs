// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of vector.

fn main() {
    let mut c = 0;

    let _v: Vec<(&char, i32)> = ['a', 'b', 'c'].into_iter()
        .map(
            |letter| { c += 1; (letter, c)}
        )
        .rev()
        .collect();
}