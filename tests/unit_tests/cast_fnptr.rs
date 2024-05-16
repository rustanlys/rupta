// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that uses casts related with function poiters, including 
// ReifyFnPointer, ClosureFnPointer, UnsafeFnPointer and FnPtrToPtr casts.

fn times2(value: i32) -> i32 {
    2 * value
}

// PointerCoercion(ReifyFnPointer)
// Go from a fn-item type to a fn-pointer type.
fn reify_fn_pointer() {
    let f: fn(i32) -> i32 = times2;
    f(2);
}

// PointerCoercion(ClosureFnPointer)
// Go from a non-capturing closure to an fn pointer or an unsafe fn pointer.
fn closure_fn_pointer() {
    let f: fn(i32) -> i32 = |x| x * 2;
    f(2);
}

// FnPtrToPtr
fn fnptr_to_ptr() {
    // PointerCoercion(ReifyFnPointer)
    let p: fn(i32) -> i32 = times2;
    // FnPtrToPtr
    let q = p as *const ();
    // Transmute
    let f = unsafe {
        std::mem::transmute::<*const (), fn(i32) -> i32>(q)
    };
    f(2);
}

// PointerCoercion(UnsafeFnPointer)
fn unsafe_fn_pointer() {
    let p: fn(i32) -> i32 = times2;
    let q = p as unsafe fn(i32) -> i32;
    unsafe { q(2); }
}

fn main() {
    reify_fn_pointer();
    closure_fn_pointer();
    fnptr_to_ptr();
    unsafe_fn_pointer();
}