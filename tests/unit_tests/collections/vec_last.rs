// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of vector.

pub const fn last<'a, 'b>(v: &'a [&'b i32]) -> Option<&'a &'b i32> {
    if let [.., last] = v { Some(last) } else { None }
}

fn main() {
    let a = 1;
    let b = 2;
    let c = 3;
    let v = vec![&a, &b, &c];
    let _elem = *last(&v[..]).unwrap();
}