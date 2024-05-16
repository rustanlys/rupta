// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of Rc.

use std::rc::Rc;

fn main() {
    let five = Rc::new(5);
    let _five_clone = five.clone();
}