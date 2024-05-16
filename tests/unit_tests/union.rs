// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of unions.

union U<'u> {
    x: &'u i8,
    y: &'u u32,
}

fn union_set<'a, 'b>(u: &'a mut U<'a>, y: &'b u32)
where 'b: 'a
{
    u.y = y;
}

fn main() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;

    let mut u1 = U { x: &a };
    let mut u2 = U { x: &b };
    
    union_set(&mut u1, &c);
    union_set(&mut u2, &d);
}

