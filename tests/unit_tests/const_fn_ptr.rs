// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that uses const function pointers.

const CONST_TIMES2: fn(i32) -> i32 = times2;
const CONST_FOO_I32: fn(i32) -> i32 = foo::<i32>;

fn times2(x: i32) -> i32 {
    x * 2
}

fn foo<T>(v: T) -> T {
    v
}

fn main() {
    CONST_TIMES2(2);
    CONST_FOO_I32(3);
}
