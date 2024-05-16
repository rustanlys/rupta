// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that tests the assume_init function of MaybeUninit.

use std::mem::MaybeUninit;

trait T {
    fn foo(&self) {}
}

struct A {}
struct B {}
struct C {}

impl T for A {
    fn foo(&self) { println!("A::foo()"); }
}

impl T for B {
    fn foo(&self) { println!("B::foo()"); }
}

impl T for C {
    fn foo(&self) { println!("C::foo()"); }
}

struct E<'e> {
    m: &'e MaybeUninit::<&'e dyn T>,
}

fn main() {
    let a = A {};
    let b = B {};
    let c = C {};
    
    let m1 = MaybeUninit::<&dyn T>::new(&a);
    unsafe {m1.assume_init().foo()};
    
    let mut t1: &dyn T = &b;
    let m2 = unsafe { std::mem::transmute::<&dyn T, MaybeUninit::<&dyn T>>(t1) };
    unsafe {m2.assume_init().foo()};
    
    t1 = &a;
    unsafe {m2.assume_init().foo()};
    
    let m3 = unsafe { std::mem::transmute::<&&dyn T, &MaybeUninit::<&dyn T>>(&t1) };
    unsafe {m3.assume_init().foo()};
    
    let e = unsafe { std::mem::transmute::<&&dyn T, E>(&t1) };
    unsafe {e.m.assume_init().foo()};
     
    t1 = &c;
    unsafe {m3.assume_init().foo()};
    unsafe {e.m.assume_init().foo()};
}