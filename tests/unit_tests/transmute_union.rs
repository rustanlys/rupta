// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that transmutes unions.

#[allow(unused)]
union U<'u> {
    f1: &'u u32,
    f2: (&'u u32, &'u u32)
}

fn main() {   
    let a = 2;
    let b = 3;
    let u = U { f2: (&a, &b) };
    let _t = unsafe { std::mem::transmute::<U, (&u32, &u32)>(u) };
}