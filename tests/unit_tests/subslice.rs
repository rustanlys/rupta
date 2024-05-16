// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of subslice.

pub const fn split_first<'a, 'b>(v: &'a [&'b i32]) -> Option<(&'a &'b i32, &'a [&'b i32])> {
    if let [first, tail @ ..] =  v { Some((first, tail)) } else { None }
}

fn main() {
    let a = 1;
    let b = 2;
    let c = 3;
    let v = vec![&a, &b, &c];
    let (first, left) = split_first(&v[..]).unwrap();
    let _first_elem = *first;
    let _next = left[0];
}