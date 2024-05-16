// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.
// 
// A test that checks the specialization of generic argument types in traits.

trait MyTrait<S> { 
    fn trait_method<T>(&self, t: T); 
    fn trait_func<Q>(_q: Q) {}
}

#[allow(unused)]
struct Foo<A, B> {
    a: A,
    b: B,
}

#[allow(unused)]
impl<B, A> MyTrait<A> for Foo<A, B> {
    fn trait_method<V>(&self, t: V) {
        Self::trait_func(t);
    }
    fn trait_func<Q>(q: Q) {}
} 

#[allow(unused)]
struct Bar<A, B> {
    a: A,
    b: B,
}

#[allow(unused)]
impl<C> MyTrait<C> for Bar<i32, C> {
    fn trait_method<T>(&self, t: T) {
        Self::trait_func(t);
    }
} 

fn main() {
    let foo = Foo { a: 1, b: 2.0 };
    let bar = Bar { a: 2, b: 3.0 };
    foo.trait_method(true);
    bar.trait_method(foo);
}