// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test that checks dynamic allocations.

#![feature(allocator_api)]

use std::alloc::Allocator;
use std::ptr::NonNull;

fn std_alloc() -> *mut u8 {
    let p = unsafe {
        std::alloc::alloc(std::alloc::Layout::from_size_align(4, 2).unwrap())
    };
    p
}

fn global_alloc() -> NonNull<[u8]> {
    let g = std::alloc::Global;
    let p = g.allocate(std::alloc::Layout::from_size_align(4, 2).unwrap())
        .expect("");
    p
}

fn main() {
    let p = std_alloc();
    let q = global_alloc();
}

