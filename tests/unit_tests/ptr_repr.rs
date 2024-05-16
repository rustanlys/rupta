// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test of PtrComponents and PtrRepr, which are widely used in Rust standard libray.

pub struct PtrComponents<T: ?Sized> {
    pub data_address: *const T,
    pub metadata: usize,
}

// Manual impl needed to avoid `T: Copy` bound.
impl<T: ?Sized> Copy for PtrComponents<T> {}

// Manual impl needed to avoid `T: Clone` bound.
impl<T: ?Sized> Clone for PtrComponents<T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Copy, Clone)]
pub union PtrRepr<T: ?Sized> {
    pub const_ptr: *const T,
    pub mut_ptr: *mut T,
    pub components: PtrComponents<T>,
}


pub struct MyStruct<T: ?Sized> {
    pub ptr_repr: PtrRepr<T>,
}

pub fn from_raw_parts<T: ?Sized>(
    data_address: *const T,
    metadata: usize,
) -> MyStruct<T> {
    MyStruct { 
        ptr_repr: PtrRepr { 
            components: PtrComponents { 
                data_address, 
                metadata 
            } 
        } 
    } 
}

fn foo(m: &mut MyStruct<i32>) {
    let v3 = [3, 4, 5, 6];
    let addr3 = &v3 as *const i32;
    // It is dangerout as the ptr will point to a invalid address after the function returns.
    (*m).ptr_repr.const_ptr = addr3; 
}

fn main() {
    let v1 = [1, 2, 3, 4];
    let metadata = 4;
    let addr1 = &v1 as *const i32;

    let mut m = from_raw_parts(addr1, metadata);
    let _const_ptr = unsafe { m.ptr_repr.const_ptr };

    let v2 = [2, 3, 4, 5];
    let addr2 = &v2 as *const i32;
    m.ptr_repr.const_ptr = addr2; 
    let _mut_ptr = unsafe { m.ptr_repr.mut_ptr };

    foo(&mut m);
}