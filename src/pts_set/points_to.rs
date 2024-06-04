// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use std::fmt;
use std::slice;

use crate::util::bit_vec::{BitIter, BitVec, Idx};

const SMALL_SET_CAPACITY: usize = 32;

pub trait PointsToSet<T> {
    type Iter<'a>: Iterator<Item = T>
    where
        Self: 'a;

    fn new() -> Self;
    fn clear(&mut self);
    fn count(&self) -> usize;
    fn contains(&self, elem: T) -> bool;
    fn is_empty(&self) -> bool;
    fn superset(&self, other: &Self) -> bool;
    fn insert(&mut self, elem: T) -> bool;
    fn remove(&mut self, elem: T) -> bool;
    fn union(&mut self, other: &Self) -> bool;
    fn subtract(&mut self, other: &Self) -> bool;
    fn intersect(&mut self, other: &Self) -> bool;
    fn iter<'a>(&'a self) -> Self::Iter<'a>;
}

/// Hybrid implementation of points to set,
/// which uses an explicit array for small sets, and a bit vector for large sets.
#[derive(Clone)]
pub struct HybridPointsToSet<T> {
    points_to: HybridSet<T>,
}

impl<T: Idx> fmt::Debug for HybridPointsToSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.points_to.fmt(f)
    }
}

/// IntoIterator
impl<'a, T: Idx> IntoIterator for &'a HybridPointsToSet<T> {
    type Item = T;
    type IntoIter = HybridIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: Idx> PointsToSet<T> for HybridPointsToSet<T> {
    fn new() -> Self {
        HybridPointsToSet {
            points_to: HybridSet::new(),
        }
    }

    /// Clear all elements.
    fn clear(&mut self) {
        self.points_to.clear();
    }

    /// Count the number of elements in the set.
    fn count(&self) -> usize {
        self.points_to.count()
    }

    /// Returns `true` if `self` contains `elem`.
    fn contains(&self, elem: T) -> bool {
        self.points_to.contains(elem)
    }

    fn is_empty(&self) -> bool {
        self.points_to.is_empty()
    }

    /// Is `self` is a superset of `other`?
    fn superset(&self, other: &HybridPointsToSet<T>) -> bool {
        self.points_to.superset(&other.points_to)
    }

    /// Adds `elem` to this set, returns true if n was not already in this set.
    fn insert(&mut self, elem: T) -> bool {
        self.points_to.insert(elem)
    }

    fn remove(&mut self, elem: T) -> bool {
        self.points_to.remove(elem)
    }

    fn union(&mut self, other: &HybridPointsToSet<T>) -> bool {
        self.points_to.union(&other.points_to)
    }

    fn subtract(&mut self, other: &HybridPointsToSet<T>) -> bool {
        self.points_to.subtract(&other.points_to)
    }

    fn intersect(&mut self, other: &HybridPointsToSet<T>) -> bool {
        self.points_to.intersect(&other.points_to)
    }

    type Iter<'a> = HybridIter<'a, T>;
    fn iter(&self) -> HybridIter<'_, T> {
        self.points_to.iter()
    }
}

#[derive(Clone)]
pub enum HybridSet<T> {
    SmallSet(Vec<T>),
    LargeSet(BitVec<T>),
}

impl<T: Idx> fmt::Debug for HybridSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SmallSet(s) => s.fmt(f),
            Self::LargeSet(s) => s.fmt(f),
        }
    }
}

impl<T: Idx> HybridSet<T> {
    pub fn new() -> Self {
        HybridSet::SmallSet(Vec::with_capacity(SMALL_SET_CAPACITY))
    }

    /// Clear all elements.
    pub fn clear(&mut self) {
        match self {
            HybridSet::SmallSet(small) => small.clear(),
            HybridSet::LargeSet(_) => {
                *self = HybridSet::SmallSet(Vec::with_capacity(SMALL_SET_CAPACITY));
            }
        }
    }

    /// Count the number of elements in the set.
    pub fn count(&self) -> usize {
        match self {
            HybridSet::SmallSet(small) => small.len(),
            HybridSet::LargeSet(large) => large.count(),
        }
    }

    /// Returns `true` if `self` contains `elem`.
    pub fn contains(&self, elem: T) -> bool {
        match self {
            HybridSet::SmallSet(small) => small.contains(&elem),
            HybridSet::LargeSet(large) => large.contains(elem),
        }
    }

    /// Is `self` is a superset of `other`?
    pub fn superset(&self, other: &HybridSet<T>) -> bool {
        match (self, other) {
            (HybridSet::LargeSet(self_large), HybridSet::LargeSet(other_large)) => {
                self_large.superset(&other_large)
            }
            _ => other.iter().all(|elem| self.contains(elem)),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            HybridSet::SmallSet(small) => small.is_empty(),
            HybridSet::LargeSet(large) => large.is_empty(),
        }
    }

    /// Adds `elem` to this set, returns true if n was not already in this set.
    pub fn insert(&mut self, elem: T) -> bool {
        match self {
            HybridSet::SmallSet(small) if small.contains(&elem) => {
                // The set is small and `elem` is not present.
                false
            }
            HybridSet::SmallSet(small) if small.len() < SMALL_SET_CAPACITY => {
                // The set is small and has space for `elem`.
                small.push(elem);
                true
            }
            HybridSet::SmallSet(small) => {
                // The set is small and full. Convert to a large set.
                let mut large = BitVec::new_empty();
                for elem in small {
                    large.insert(*elem);
                }
                let changed = large.insert(elem);
                *self = HybridSet::LargeSet(large);
                changed
            }
            HybridSet::LargeSet(large) => large.insert(elem),
        }
    }

    pub fn remove(&mut self, elem: T) -> bool {
        // Note: we currently don't bother going from Large back to Small.
        match self {
            HybridSet::SmallSet(small) => {
                if let Some(pos) = small.iter().position(|x| *x == elem) {
                    small.swap_remove(pos);
                    true
                } else {
                    false
                }
            }
            HybridSet::LargeSet(large) => large.remove(elem),
        }
    }

    pub fn iter(&self) -> HybridIter<'_, T> {
        match self {
            HybridSet::SmallSet(small) => HybridIter::SmallIter(small.iter()),
            HybridSet::LargeSet(large) => HybridIter::LargeIter(large.iter()),
        }
    }

    pub fn union(&mut self, other: &HybridSet<T>) -> bool {
        match self {
            HybridSet::LargeSet(self_large) => match other {
                HybridSet::LargeSet(other_large) => self_large.union(&other_large),
                HybridSet::SmallSet(other_small) => {
                    let mut changed = false;
                    for elem in other_small.iter() {
                        changed |= self_large.insert(*elem);
                    }
                    changed
                }
            },
            HybridSet::SmallSet(self_small) => {
                match other {
                    HybridSet::LargeSet(other_large) => {
                        // convert self set to a large set
                        let mut self_large = BitVec::new_empty();
                        for elem in self_small.iter() {
                            self_large.insert(*elem);
                        }
                        let changed = self_large.union(&other_large);
                        *self = HybridSet::LargeSet(self_large);
                        changed
                    }
                    HybridSet::SmallSet(other_small) => {
                        let mut changed = false;
                        for &elem in other_small.iter() {
                            changed |= self.insert(elem);
                        }
                        changed
                    }
                }
            }
        }
    }

    pub fn subtract(&mut self, other: &HybridSet<T>) -> bool {
        match self {
            HybridSet::LargeSet(self_large) => match other {
                HybridSet::LargeSet(other_large) => self_large.subtract(&other_large),
                HybridSet::SmallSet(other_small) => {
                    let mut changed = false;
                    for &elem in other_small.iter() {
                        changed |= self_large.remove(elem);
                    }
                    changed
                }
            },
            HybridSet::SmallSet(self_small) => {
                let mut changed = false;
                self_small.retain(|&elem| {
                    let contains = other.contains(elem);
                    if contains {
                        changed = true;
                    }
                    !contains
                });
                changed
            }
        }
    }

    pub fn intersect(&mut self, other: &HybridSet<T>) -> bool {
        match self {
            HybridSet::LargeSet(self_large) => {
                match other {
                    HybridSet::LargeSet(other_large) => self_large.intersect(&other_large),
                    HybridSet::SmallSet(other_small) => {
                        // convert self set to a small set
                        let mut self_small = other_small.clone();
                        let mut changed = false;
                        self_small.retain(|&elem| {
                            let contains = self_large.contains(elem);
                            if !contains {
                                changed = true;
                            }
                            contains
                        });
                        *self = HybridSet::SmallSet(self_small);
                        changed
                    }
                }
            }
            HybridSet::SmallSet(self_small) => {
                let mut changed = false;
                self_small.retain(|&elem| {
                    let contains = other.contains(elem);
                    if !contains {
                        changed = true;
                    }
                    contains
                });
                changed
            }
        }
    }
}

pub enum HybridIter<'a, T: Idx> {
    SmallIter(slice::Iter<'a, T>),
    LargeIter(BitIter<'a, T>),
}

impl<'a, T: Idx> Iterator for HybridIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        match self {
            HybridIter::SmallIter(small) => small.next().copied(),
            HybridIter::LargeIter(large) => large.next(),
        }
    }
}

#[test]
fn hybrid_set_tests() {
    // small set test
    let mut a = HybridPointsToSet::<u32>::new();
    a.insert(1);
    a.insert(3);
    a.insert(5);
    a.insert(3);
    a.insert(11);

    assert_eq!(a.count(), 4);
    assert_eq!(a.contains(3), true);
    assert_eq!(a.contains(7), false);
    assert_eq!(a.iter().collect::<Vec<_>>(), [1, 3, 5, 11]);
    assert!(matches!(a.points_to, HybridSet::SmallSet(_)));

    // large set test
    let mut b = HybridPointsToSet::<u32>::new();
    b.insert(1);
    b.insert(10);
    b.insert(19);
    b.insert(62);
    b.insert(63);
    b.insert(64);
    b.insert(65);
    b.insert(66);
    b.insert(99);
    b.insert(2);
    b.insert(20);
    b.insert(38);
    b.insert(124);
    b.insert(126);
    b.insert(128);
    b.insert(130);
    b.insert(132);
    b.insert(99);
    assert_eq!(b.count(), 17);
    assert_eq!(b.contains(38), true);
    assert_eq!(b.contains(7), false);
    assert_eq!(
        b.iter().collect::<Vec<_>>(),
        [1, 2, 10, 19, 20, 38, 62, 63, 64, 65, 66, 99, 124, 126, 128, 130, 132]
    );
    assert_eq!(b.superset(&a), false);
    assert!(matches!(b.points_to, HybridSet::LargeSet(_)));

    // remove test
    assert_eq!(a.remove(3), true);
    assert_eq!(a.count(), 3);
    assert_eq!(a.contains(3), false);
    assert_eq!(a.remove(17), false);
    a.insert(3);

    assert_eq!(b.remove(10), true);
    assert_eq!(b.count(), 16);
    assert_eq!(b.contains(10), false);
    assert_eq!(b.remove(17), false);
    b.insert(10);

    // union test
    // small set union large set
    let mut c = a.clone();
    c.union(&b);
    assert_eq!(c.count(), 20);
    assert_eq!(c.superset(&a), true);
    assert_eq!(c.superset(&b), true);
    assert_eq!(
        c.iter().collect::<Vec<_>>(),
        [1, 2, 3, 5, 10, 11, 19, 20, 38, 62, 63, 64, 65, 66, 99, 124, 126, 128, 130, 132]
    );
    assert!(matches!(c.points_to, HybridSet::LargeSet(_)));

    // small set union small set
    c = a.clone();
    let mut d = HybridPointsToSet::<u32>::new();
    d.insert(3);
    d.insert(17);
    d.insert(25);
    d.insert(37);
    d.insert(46);
    d.insert(55);
    d.insert(63);
    d.insert(77);
    d.insert(89);
    d.insert(90);
    d.insert(102);
    d.insert(111);
    d.insert(123);
    d.insert(134);
    c.union(&d);
    assert_eq!(c.count(), 17);
    assert_eq!(c.superset(&a), true);
    assert_eq!(
        c.iter().collect::<Vec<_>>(),
        [1, 3, 5, 11, 17, 25, 37, 46, 55, 63, 77, 89, 90, 102, 111, 123, 134]
    );
    assert!(matches!(d.points_to, HybridSet::SmallSet(_)));
    assert!(matches!(c.points_to, HybridSet::LargeSet(_)));

    // large set union small set
    let mut e = b.clone();
    e.union(&a);
    assert_eq!(e.count(), 20);
    assert_eq!(e.superset(&a), true);
    assert_eq!(e.superset(&b), true);
    assert_eq!(
        e.iter().collect::<Vec<_>>(),
        [1, 2, 3, 5, 10, 11, 19, 20, 38, 62, 63, 64, 65, 66, 99, 124, 126, 128, 130, 132]
    );
    assert!(matches!(e.points_to, HybridSet::LargeSet(_)));

    // large set union large set
    let mut f = b.clone();
    f.insert(156);
    f.insert(10001);
    e.union(&f);
    assert_eq!(e.count(), 22);
    assert_eq!(
        e.iter().collect::<Vec<_>>(),
        [1, 2, 3, 5, 10, 11, 19, 20, 38, 62, 63, 64, 65, 66, 99, 124, 126, 128, 130, 132, 156, 10001]
    );

    // subtract test
    c = a.clone();
    assert_eq!(c.subtract(&b), true);
    assert_eq!(c.count(), 3);
    assert_eq!(c.contains(1), false);

    e = b.clone();
    assert_eq!(e.subtract(&a), true);
    assert_eq!(e.count(), 16);
    assert_eq!(e.contains(1), false);

    // intersect test
    c = a.clone();
    assert_eq!(c.intersect(&b), true);
    assert_eq!(c.count(), 1);
    assert_eq!(c.contains(1), true);
    assert!(matches!(c.points_to, HybridSet::SmallSet(_)));

    e = b.clone();
    assert_eq!(e.intersect(&a), true);
    assert_eq!(e.count(), 1);
    assert_eq!(e.contains(1), true);
    assert!(matches!(e.points_to, HybridSet::SmallSet(_)));
}
