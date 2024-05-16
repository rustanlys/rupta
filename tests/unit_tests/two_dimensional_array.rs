// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of two dimensional array.

fn main() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;

    let arr1 = [&a, &b, &c];
    let arr2 = [&c, &d, &e];
    let arr3 = [arr1, arr2];
    
    let _x = arr3[0];
    let _y = arr3[1];
    let _z = arr3[0][1];
}