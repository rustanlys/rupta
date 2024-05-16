// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that uses static variables.

static mut STATIC_VAL: u8 = 2;
static mut STATIC_PTR: &u8 = &3;

static mut STATIC_FNPTR: fn(u8) -> u8 = times2;

fn times2(x: u8) -> u8 {
    unsafe { 
        x * STATIC_VAL
    }
}

#[allow(unused)]
fn main() {
    unsafe { 
        STATIC_VAL = 4;
        let a = STATIC_VAL;
        
        STATIC_PTR = &4; 
        let b = STATIC_PTR;
        
        let fp = STATIC_FNPTR;
        fp(2);
    }
}
