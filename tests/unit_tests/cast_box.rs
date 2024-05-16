// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that casts a box into its unwrapped type.
// In old versions of Rust MIR, a box variable can be dereferenced 
// directly in MIR, e.g. *b. But in the new version, the dereference 
// of a box is disassembled into two operations: (1) accessing the 
// box's pointer field and (2) dereferencing the pointer.
// For example, `q = *b;` is translated into
// ptr = (((b.0: std::ptr::Unique<T>).0: std::ptr::NonNull<T>).0: *const T);
// q = *ptr;    

#[allow(unused)]
#[derive(Copy, Clone)]
struct A<'a> {
    a: &'a u32,
}

#[allow(unused)]
fn main() {
    let mut x = 2;
    
    let a = A { a: &x };

    let b = Box::new(a);
    let p1 = &*b as *const A;
    
    let a = unsafe { *p1 };
}