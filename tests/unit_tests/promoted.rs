// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of const promotion.
// "Promotion" is the act of splicing a part of a MIR computation out 
// into a separate self-contained MIR body which is evaluated at 
// compile-time like a constant. 

fn main() {
    let a = &2;
}