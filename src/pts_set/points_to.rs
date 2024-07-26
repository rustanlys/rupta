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

#[cfg(test)]
mod test {
    use std::collections::HashSet;
    use rand::Rng;
    use crate::pts_set::points_to::{
        HybridPointsToSet, HybridSet, 
        PointsToSet, SMALL_SET_CAPACITY
    };

    fn random_set(len: usize) -> HashSet<u32> {
        let mut rng = rand::thread_rng();
        let mut set = HashSet::new();
        while set.len() < len {
            let x = rng.gen_range(1..1000);
            set.insert(x);
        }
        set
    }

    fn random_value_from_set(set: &HashSet<u32>) -> u32 {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..set.len());
        set.iter().nth(index).cloned().unwrap()
    }

    #[test]
    fn small_set_test() {
        let rand_set = random_set(8);
        let mut small_set = HybridPointsToSet::<u32>::new();
        for x in rand_set.iter() {
            small_set.insert(*x);
        }
        assert_eq!(small_set.count(), 8);
        assert!(matches!(small_set.points_to, HybridSet::SmallSet(_)));
        assert_eq!(
            small_set.iter().collect::<HashSet<_>>(), 
            rand_set
        );

        let rand_val = random_value_from_set(&rand_set);
        assert_eq!(small_set.contains(rand_val), true);
        assert_eq!(small_set.remove(rand_val), true);
        assert_eq!(small_set.contains(rand_val), false);
        assert_eq!(small_set.count(), 7);
    }

    #[test]
    fn large_set_test() {
        let rand_set = random_set(SMALL_SET_CAPACITY + 3);
        let mut large_set = HybridPointsToSet::<u32>::new();
        for x in rand_set.iter() {
            large_set.insert(*x);
        }
        assert_eq!(large_set.count(), SMALL_SET_CAPACITY + 3);
        assert!(matches!(large_set.points_to, HybridSet::LargeSet(_)));
        assert_eq!(
            large_set.iter().collect::<HashSet<_>>(), 
            rand_set
        );

        let rand_val = random_value_from_set(&rand_set);
        assert_eq!(large_set.contains(rand_val), true);
        assert_eq!(large_set.remove(rand_val), true);
        assert_eq!(large_set.contains(rand_val), false);
        assert_eq!(large_set.count(), SMALL_SET_CAPACITY + 2);
    }


    #[test] 
    fn small_set_union_large_set() {
        let rand_small_set = random_set(8);
        let mut small_set = HybridPointsToSet::<u32>::new();
        for x in rand_small_set.iter() {
            small_set.insert(*x);
        }
        let rand_large_set = random_set(SMALL_SET_CAPACITY + 3);
        let mut large_set = HybridPointsToSet::<u32>::new();
        for x in rand_large_set.iter() {
            large_set.insert(*x);
        }

        let mut union_set = small_set.clone();
        union_set.union(&large_set);
        assert_eq!(union_set.superset(&small_set), true);
        assert_eq!(union_set.superset(&large_set), true);
        assert_eq!(
            union_set.iter().collect::<HashSet<_>>(), 
            rand_small_set.union(&rand_large_set)
                .cloned()
                .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn large_set_union_small_set() {
        let rand_small_set = random_set(8);
        let mut small_set = HybridPointsToSet::<u32>::new();
        for x in rand_small_set.iter() {
            small_set.insert(*x);
        }
        let rand_large_set = random_set(SMALL_SET_CAPACITY + 3);
        let mut large_set = HybridPointsToSet::<u32>::new();
        for x in rand_large_set.iter() {
            large_set.insert(*x);
        }

        let mut union_set = large_set.clone();
        union_set.union(&small_set);
        assert_eq!(
            union_set.iter().collect::<HashSet<_>>(), 
            rand_small_set.union(&rand_large_set)
                .cloned()
                .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn large_set_union_large_set() {
        let rand_set1 = random_set(SMALL_SET_CAPACITY + 3);
        let mut large_set1 = HybridPointsToSet::<u32>::new();
        for x in rand_set1.iter() {
            large_set1.insert(*x);
        }
        let rand_set2 = random_set(SMALL_SET_CAPACITY + 3);
        let mut large_set2 = HybridPointsToSet::<u32>::new();
        for x in rand_set2.iter() {
            large_set2.insert(*x);
        }

        let mut union_set = large_set1.clone();
        union_set.union(&large_set2);
        assert_eq!(
            union_set.iter().collect::<HashSet<_>>(), 
            rand_set1.union(&rand_set2)
                .cloned()
                .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn subtract_test() {
        let rand_small_set = random_set(8);
        let mut small_set = HybridPointsToSet::<u32>::new();
        for x in rand_small_set.iter() {
            small_set.insert(*x);
        }
        let mut rand_large_set = random_set(SMALL_SET_CAPACITY + 3);
        for &x in rand_small_set.iter().take(5) {
            rand_large_set.insert(x);
        }
        let mut large_set = HybridPointsToSet::<u32>::new();
        for x in rand_large_set.iter() {
            large_set.insert(*x);
        }
        
        let mut cloned_set = small_set.clone();
        assert_eq!(cloned_set.subtract(&large_set), true);
        assert_eq!(
            cloned_set.iter().collect::<HashSet<_>>(), 
            rand_small_set.difference(&rand_large_set)
                .cloned()
                .collect::<HashSet<_>>()
        );

        cloned_set = large_set.clone();
        assert_eq!(cloned_set.subtract(&small_set), true);
        assert_eq!(
            cloned_set.iter().collect::<HashSet<_>>(), 
            rand_large_set.difference(&rand_small_set)
                .cloned()
                .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn intersect_test() {
        let rand_small_set = random_set(8);
        let mut small_set = HybridPointsToSet::<u32>::new();
        for x in rand_small_set.iter() {
            small_set.insert(*x);
        }
        let mut rand_large_set = random_set(SMALL_SET_CAPACITY + 3);
        for &x in rand_small_set.iter().take(5) {
            rand_large_set.insert(x);
        }
        let mut large_set = HybridPointsToSet::<u32>::new();
        for x in rand_large_set.iter() {
            large_set.insert(*x);
        }
        
        let mut cloned_set = large_set.clone();
        assert_eq!(cloned_set.intersect(&small_set), true);
        assert_eq!(
            cloned_set.iter().collect::<HashSet<_>>(), 
            rand_large_set.intersection(&rand_small_set)
                .cloned()
                .collect::<HashSet<_>>()
        );
        assert!(matches!(cloned_set.points_to, HybridSet::SmallSet(_)));
    }
}
