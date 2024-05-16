// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test that uses a box containing dynamic trait objects.

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
    let mut s: Box<dyn Shape>;
    let c = Circle;
    let r = Rectangle;
    
    s = Box::new(c);
    s.draw();
    s = Box::new(r);
    s.draw();
}