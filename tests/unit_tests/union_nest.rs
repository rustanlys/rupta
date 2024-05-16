// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of nested unions.

union U<'u> {
    f1: &'u u32,
    f2: (&'u u32, &'u u32)
}

// Manual impl needed to avoid `T: Copy` bound.
impl<'u> Copy for U<'u> {}

// Manual impl needed to avoid `T: Clone` bound.
impl<'u> Clone for U<'u> {
    fn clone(&self) -> Self {
        *self
    }
}

union V<'v, 'u> {
    f1: &'v u32,
    f2: U<'u>,
}


fn main() {
    let a = 2;
    let b = 3;
    let c = 4;
    let u = U { f2: (&a, &b) };
    
    let mut v = V { f2: u };
    v.f2.f1 = &c;
    let _a = unsafe {
        v.f1
    };
}