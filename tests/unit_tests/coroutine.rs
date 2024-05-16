// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of coroutine.

#![feature(coroutines, coroutine_trait)]

use std::ops::Coroutine;
use std::pin::Pin;

fn main() {
    let ret = "foo";
    let a = 2;
    let p = &a;
    let mut coroutine = move || {
        yield *p;
        return ret
    };

    Pin::new(&mut coroutine).resume(());
    Pin::new(&mut coroutine).resume(());
}