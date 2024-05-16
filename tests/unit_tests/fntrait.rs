// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that checks `Fn*` trait calls.
// 
// `Fn`, `FnMut` and `FnOnce` are implemented automatically by closures, function pointers
// and function items. 
// 
// Additionally, for any type `F` that implements `Fn`, `&F` implements `Fn`, too; for any 
// type `F` that implements `FnMut`, `&mut F` implements `FnMut` too.
// 
// `FnOnce` is a supertrait of `FnMut`, and both `FnMut` and FnOnce are supertraits of `Fn`.
// Therefore, any instance of `Fn` can be used as a parameter where a `FnMut` or `FnOnce` 
// is expected; any instance of `FnMut` can be used where a `FnOnce` is expected.

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

fn fn_call<F>(f: F, x: &u32) 
where 
    F: Fn(&u32) -> u32
{
    f(x);
}

fn fnmut_call<F>(mut f: F, x: &u32)
where
    F: FnMut(&u32) -> u32
{
    f(x);
}

fn fnonce_call<F>(mut f: F, x: &u32)
where
    F: FnOnce(&u32) -> u32
{
    f(x);
}

fn fn_item() {
    let x = &1;
    fn_call(times2, x);
    fnmut_call(times2, x);
    fnonce_call(times2, x);
}

fn fn_ptr() {
    let x = &2;
    let fp: fn(&u32) -> u32 = times2;
    fn_call(fp, x);
    fnmut_call(fp, x);
    fnonce_call(fp, x);
}

fn closure() {
    let mut c = |x: &u32| (*x) * 2;
    let x = &3;
    fn_call(&c, x);
    fnmut_call(&mut c, x);
    fnonce_call(c, x);
}

fn ref_fn() {
    let x = &4;
    let f = &times2;
    let f2 = &f;
    fn_call(&f2, x);
    fnmut_call(f2, x);
    fnonce_call(f2, x);
}

fn foo() {
    let x = &5;
    let foo = Foo { a: 3 };
    fnmut_call(foo, x);
    fnonce_call(foo, x);
}

fn main() {
    fn_item();
    fn_ptr();
    closure();
    ref_fn();
    foo();
}
