// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that uses copy_nonoverlapping.
// The `std::ptr::copy_nonoverlapping` function copies `count` * `size_of::<T>()` 
// bytes from src to dst.
// The `copy_nonoverlapping` statement is used in the function.

fn copy_array() {
    let mut x = 2;
    let mut y = 3;
    let mut m = 4;
    let mut n = 5;
    
    let a1 = [&x, &y];
    let mut a2 = [&m, &n];

    let p1 = &a1 as *const [&i32; 2];
    let p2 = &mut a2 as *mut [&i32; 2];
    
    unsafe { std::ptr::copy_nonoverlapping(p1, p2, 1); }
}

fn copy_elem() {
    let mut x = 2;
    let mut y = 3;
    let mut m = 4;
    let mut n = 5;
    
    let a1 = [&x, &y];
    let mut a2 = [&m, &n];

    let p1 = &a1 as *const [&i32; 2] as *const &i32;
    let p2 = &mut a2 as *mut [&i32; 2] as *mut &i32;

    unsafe { std::ptr::copy_nonoverlapping(p1, p2, 1); }
}

fn main() {
    copy_array();
    copy_elem();
}