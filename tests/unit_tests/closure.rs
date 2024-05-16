// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that checks closure calls.
// 
// A closure call in translated into a Fn* trait call in Rust MIR by default. 

fn closure1() {
    let f = |x| 2 * x;
    f(2);

    let g = &&f;
    g(2);
}

fn closure2() {
    let f = || &2;
    f();

    let g = &&f;
    g();
}

fn main() {
    closure1();
    closure2();
}