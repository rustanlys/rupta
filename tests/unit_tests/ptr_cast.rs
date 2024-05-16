// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of pointer casts.

#![feature(allocator_api)]

use std::alloc::Allocator;

fn foo() {
    let alloc = std::alloc::Global;
    let result = alloc.allocate(std::alloc::Layout::from_size_align(4, 2).unwrap());

    if let Ok(ptr) = result {
        let p: std::ptr::NonNull<String> = ptr.cast();
    }
}

fn bar() {
    let alloc = std::alloc::Global;
    let result = alloc.allocate(std::alloc::Layout::from_size_align(4, 2).unwrap());
    
    if let Ok(ptr) = result {
        let p: std::ptr::NonNull<u32> = ptr.cast();
    }
}

fn main() {
    foo();
    bar();
}