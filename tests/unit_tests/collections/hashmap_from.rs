// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
//
// A test of hashmap.

use std::collections::HashMap;

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

    let map = HashMap::from([
        (&a, x),
        (&b, y),
        (&c, z),
    ]);

    let value = map.get(&&a).unwrap();
    let _f = value.f;
}