// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of generic functions with `impl traits` types.

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

fn draw_shape(s: &impl Shape) {
    s.draw();
}

fn draw_shape_two(s1: &impl Shape, s2: &impl Shape) {
    s1.draw();
    s2.draw();
}

fn main() {
    let c = Circle;
    let r = Rectangle;
    
    draw_shape(&c);
    draw_shape_two(&c, &r);
}