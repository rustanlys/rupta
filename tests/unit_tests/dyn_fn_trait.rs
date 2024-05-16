// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of calls on dynamic `Fn*` trait objects.
// Currently we can only make calls via reference to dynamic `Fn` and 
// `FnMut` trait objects. Calls on a reference to a dynamic `FnOnce`
// trait object would lead to a compilation error. For example, if we 
// have a call `f(x)`, where f: &dyn FnOnce(u32) -> u32, the compiler 
// will raise an error like the following:
// error[E0161]: cannot move a value of type `dyn FnOnce(u32) -> u32`.
// To make a call on a dynamic FnOnce object, we can use Box<dyn FnOnce>
// instead.


#![feature(unboxed_closures)]
#![feature(fn_traits)]

#[derive(Copy, Clone)]
struct Foo {
    a: u32
}

impl FnOnce<(&u32, )> for Foo {
    type Output = u32;
    extern "rust-call" fn call_once(self, b: (&u32, )) -> Self::Output {
        self.a + *b.0
    }
}

impl FnMut<(&u32, )> for Foo { 
    extern "rust-call" fn call_mut(&mut self, b: (&u32, )) -> Self::Output {
        // We must invoke `call_once` on the dereferecened self variable
        // Calling `self.call_once(b)` will cause an infinite recursion 
        // because it will call the automatically generated `call_once` function 
        // for &mut Foo
        (*self).call_once(b)
    }
}

fn times2(x: &u32) -> u32 {
    2 * (*x)
}

fn dyn_fn_call(f: &dyn Fn(&u32) -> u32, x: &u32) {
    f(x);
}

fn dyn_fnmut_call(f: &mut dyn FnMut(&u32) -> u32, x: &u32) {
    f(x);
}

fn dyn_fn_item() {
    let x = &1;
    dyn_fn_call(&times2, x);
    dyn_fnmut_call(&mut times2, x);
}

fn dyn_fn_ptr() {
    let x = &2;
    let mut fp: fn(&u32) -> u32 = times2;
    dyn_fn_call(&fp, x);
    dyn_fnmut_call(&mut fp, x);
}

fn dyn_closure() {
    let mut c = |x: &u32| (*x) * 2;
    let x = &3;
    dyn_fn_call(&c, x);
    dyn_fnmut_call(&mut c, x);
}

fn dyn_dyn_fn() {
    let x = &4;
    let f: &dyn Fn(&u32) -> u32 = &times2;
    let mut f2: &dyn Fn(&u32) -> u32 = &f;
    dyn_fn_call(&f2, x);
    dyn_fnmut_call(&mut f2, x);
}

fn dyn_foo() {
    let x = &5;
    let mut foo = Foo { a: 3 };
    dyn_fnmut_call(&mut foo, x);
}

fn main() {
    dyn_fn_item();
    dyn_fn_ptr();
    dyn_closure();
    dyn_dyn_fn();
    dyn_foo();
}
