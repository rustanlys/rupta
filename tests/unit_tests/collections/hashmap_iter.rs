// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of hashmap.

use std::collections::HashMap;

#[allow(unused)]
#[derive(Copy, Clone)]
struct Foo<'f> {
    f: &'f u32,
}

fn main() {
    let a = 1;
    let b = 2;
    let c = 3;

    let x = Foo { f: &a };
    let y = Foo { f: &b };
    let z = Foo { f: &c };

    let mut map = HashMap::new();
    map.insert(&a, x);
    map.insert(&b, y);
    map.insert(&c, z);

    for (key, value) in map.iter() {
        let _k = *key;
        let _v = *value;
    }
}