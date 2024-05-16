// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that uses unsize cast.
// An unsize cast Unsize a pointer/reference value, e.g., `&[T; n]` to `&[T]`.
// This will do things like convert thin pointers to fat pointers, or convert 
// structs containing thin pointers to structs containing fat pointers, or 
// convert between fat pointers. 

trait Shape {
    fn draw(&self) {}
}

#[allow(unused)]
struct Circle { r: f64 }
#[allow(unused)]
struct Rectangle { w: f64, h: f64 }

impl Shape for Circle {
    fn draw(&self) {}
}
impl Shape for Rectangle {
    fn draw(&self) {}
}

fn ref_dyn_unsize() {
    let c = Circle { r: 2.0 };
    let r = Rectangle { w: 1.0, h: 2.0 };
    
    let p = &c as &dyn Shape;
    let q = &r as &dyn Shape;
    p.draw();
    q.draw();
}

fn raw_dyn_unsize() {
    let x = 2.0;
    let p = &x as *const f64;
    let q = p as *const Circle;
    let s = q as *const dyn Shape;
    let s = unsafe {
        &(*s)
    };
    s.draw();
}

#[allow(unused)]
fn slice_unsize() {
    let arr = [1, 2, 3, 4, 5];
    let slice = &arr as &[i32];
}

fn main() {
    slice_unsize();
    ref_dyn_unsize();
    raw_dyn_unsize();
}