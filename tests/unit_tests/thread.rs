// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of thread spawn.

use std::thread;

fn main() {
    let _ = thread::Builder::new().name("thread1".to_string()).spawn(move || {
        println!("Hello, world!");
    });
}