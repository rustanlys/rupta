// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of vector.

fn main() {
    let a = 1;
    let b = 2;
    let c = 3;
    let v = vec![&a, &b, &c];

    let d = v.get(2);
    let _e = *d.unwrap();
}