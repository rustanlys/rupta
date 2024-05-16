// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test that uses an array containing reference type elements.

fn main() {
    let a = 1;
    let b = 2;
    let c = 3;
    let arr = [&a, &b, &c];
}