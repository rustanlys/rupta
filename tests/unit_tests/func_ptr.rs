// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of function pointers.

trait MyTrait { 
    fn f(&self); 
    fn g(&self) {}
}

struct MyStruct;

impl MyStruct {
    /// Associated function
    fn foo() {}

    /// Associated method
    fn bar(&self) {} 

    /// Generic method
    fn use_generic<T>(&self, t: T) -> T {
        t
    }
}

impl MyTrait for MyStruct {
    /// Trait method
    fn f(&self) {}
}

/// Normal function
fn times2(value: i32) -> i32 {
    2 * value
}

/// Generic function
fn id<T>(t: T) -> T {
    t
}

fn main() {
    let fp1: fn(i32) -> i32 = times2;
    fp1(2);

    let fp2: fn(i32) -> i32 = id;
    fp2(2);

    let fp3: fn() = MyStruct::foo;
    fp3();

    let m = MyStruct{};

    let fp4: fn(&MyStruct) = MyStruct::bar;
    fp4(&m);

    let fp5: fn(&MyStruct, i32) -> i32 = MyStruct::use_generic;
    fp5(&m, 2);

    let fp6: fn(&MyStruct) = MyTrait::f;
    fp6(&m);

    let fp7: fn(&MyStruct) = MyTrait::g;
    fp7(&m);

    let fp8 = |x: i32| x;
    fp8(2);
}
