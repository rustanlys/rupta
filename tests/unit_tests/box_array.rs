// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

fn len(b: Box<[i32]>) -> usize {
    b.len()
}

#[allow(unused)]
pub fn main() {
    let boxed_array = Box::new([10]);
    let elem = boxed_array[0];
    let len = len(boxed_array);
}