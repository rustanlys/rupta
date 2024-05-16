// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test for validating the effectiveness of call-site sensitivity
// in resolving dynamic call graph edges.

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

fn id(t: Box<&dyn Shape>) -> Box<&dyn Shape> {
    t
}

fn id2(t: Box<&dyn Shape>) -> Box<&dyn Shape> {
    id(t)
}

fn for_cs1() {
    let c = Circle;
    let r = Rectangle;
    let b1 = id(Box::new(&c));
    let b2 = id(Box::new(&r));
    b1.draw();
    b2.draw();
}

fn for_cs2() {
    let c = Circle;
    let r = Rectangle;
    let b1 = id2(Box::new(&c));
    let b2 = id2(Box::new(&r));
    b1.draw();
    b2.draw();
}

fn main() {
    for_cs1();
    for_cs2();
}