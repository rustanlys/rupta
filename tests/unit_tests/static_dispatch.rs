// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that checks static dispatch.

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

fn draw_shape<S: Shape>(s: S) {
    s.draw();
}

fn main() {
    let c = Circle;
    let r = Rectangle;
    
    draw_shape(c);
    draw_shape(r);
}