// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that checks dynamic call resolution.

trait Shape {
    fn draw(&self) {}
}

struct Circle;
struct Rectangle;

impl Shape for Circle {
    fn draw(&self) {}
}
impl Shape for Rectangle {
    fn draw(&self) {}
}

fn main() {
    let c = Circle;
    let r = Rectangle;
    
    let s1: &dyn Shape = &c;
    s1.draw();
    let s2: &dyn Shape = &r;
    s2.draw();
}