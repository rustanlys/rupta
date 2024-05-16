// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

use super::points_to::PointsToSet;
use crate::util::bit_vec::Idx;

/// Basic points-to data structure
/// Given a key (variable/condition variable), return its points-to data (pts/condition pts)
/// It is designed flexible for different context, heap and path sensitive analysis
/// Context Insensitive			   Key --> Variable, DataSet --> PointsTo
/// Context sensitive:  			   Key --> CondVar,  DataSet --> PointsTo
/// Heap sensitive:     			   Key --> Variable  DataSet --> CondPointsToSet
/// Context and heap sensitive:     Key --> CondVar,  DataSet --> CondPointsToSet
///
/// K  (Key):     "owning" variable of a points-to set.
/// KS (KeySet):  collection of keys.
/// D  (Data):    elements in points-to sets.
/// DS (DataSet): the points-to set; a collection of Data.
pub struct BasePTData<K, KS, D, DS> {
    pts_map: HashMap<K, DS>,
    rev_pts_map: HashMap<D, KS>,
}

impl<K, KS, D, DS> fmt::Debug for BasePTData<K, KS, D, DS> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "BasePTData".fmt(f)
    }
}

impl<K, D, DS> BasePTData<K, HashSet<K>, D, DS>
where
    K: Hash + Eq + Copy,
    D: Idx,
    DS: PointsToSet<D>,
{
    pub fn new() -> BasePTData<K, HashSet<K>, D, DS> {
        BasePTData {
            pts_map: HashMap::new(),
            rev_pts_map: HashMap::new(),
        }
    }

    /// Return Points-to map
    #[inline]
    pub fn get_pts_map(&self) -> &HashMap<K, DS> {
        &self.pts_map
    }

    #[inline]
    pub fn clear(&mut self) {
        self.pts_map.clear();
        self.rev_pts_map.clear();
    }

    /// Get points-to set of a var.
    #[inline]
    pub fn get_pts(&self, var: K) -> Option<&DS> {
        self.pts_map.get(&var)
    }

    #[inline]
    pub fn get_mut_pts(&mut self, var: K) -> Option<&mut DS> {
        self.pts_map.get_mut(&var)
    }

    /// Get reverse points-to set of a elem.
    #[inline]
    pub fn get_rev_pts(&self, elem: D) -> Option<&HashSet<K>> {
        self.rev_pts_map.get(&elem)
    }

    /// Adds element to the points-to set associated with var.
    pub fn add_pts(&mut self, var: K, elem: D) -> bool {
        self.rev_pts_map.entry(elem).or_default().insert(var);
        self.pts_map.entry(var).or_insert(DS::new()).insert(elem)
    }

    /// Performs pts(dst_var) = pts(dst_var) U pts(src_var).
    pub fn union_pts(&mut self, dst_var: K, src_var: K) -> bool {
        if self.get_pts(src_var).is_some() {
            let dst_ds = unsafe { &mut *(self.pts_map.entry(dst_var).or_insert(DS::new()) as *mut DS) };
            let src_ds = unsafe { &*(self.pts_map.get(&src_var).unwrap() as *const DS) };
            self.add_rev_pts(src_ds, dst_var);
            dst_ds.union(src_ds)
        } else {
            false
        }
    }

    /// Performs pts(dst_var) = pts(dst_var) U src_dataset.
    pub fn union_pts_to(&mut self, dst_var: K, src_ds: &DS) -> bool {
        self.add_rev_pts(src_ds, dst_var);
        let dst_ds = self.pts_map.entry(dst_var).or_insert(DS::new());
        dst_ds.union(src_ds)
    }

    /// Removes element from the points-to set of var.
    pub fn remove_pts_elem(&mut self, var: K, elem: D) -> bool {
        // Remove var from rev_pts_map[elem]
        if let Some(vars) = self.rev_pts_map.get_mut(&elem) {
            vars.remove(&var);
        }
        if let Some(pts) = self.pts_map.get_mut(&var) {
            pts.remove(elem)
        } else {
            false
        }
    }

    /// Fully clears the points-to set of var.
    pub fn clear_pts(&mut self, var: K) {
        if let Some(pts) = self.pts_map.get_mut(&var) {
            for elem in pts.iter() {
                self.rev_pts_map.get_mut(&elem).unwrap().remove(&var);
            }
            pts.clear();
        }
    }

    /// Dump stored keys and points-to sets.
    pub fn dump_pt_data(&self) {
        unimplemented!("implement if/when necessary");
    }

    /// Internal union/add points-to helper methods
    /// Add `var` to the reversed pts set for each data in `data_set`.
    #[inline]
    fn add_rev_pts(&mut self, data_set: &DS, var: K) {
        for elem in data_set.iter() {
            self.rev_pts_map.entry(elem).or_default().insert(var);
        }
    }
}

/// Diff points-to data.
/// This is an optimisation on top of the base points-to data structure.
/// The points-to information is propagated incrementally only for the different parts.
pub struct DiffPTData<K, D, DS> {
    /// Diff points-to to be propagated.
    pub(crate) diff_pts_map: HashMap<K, DS>,
    /// Points-to already propagated.
    pub(crate) propa_pts_map: HashMap<K, DS>,

    marker: PhantomData<D>,
}

impl<K, D, DS> fmt::Debug for DiffPTData<K, D, DS> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "DiffPTData".fmt(f)
    }
}

impl<K, D, DS> DiffPTData<K, D, DS>
where
    K: Hash + Eq + Copy,
    D: Idx,
    DS: PointsToSet<D> + Clone + fmt::Debug,
    for<'a> &'a DS: IntoIterator<Item = D>,
{
    pub fn new() -> DiffPTData<K, D, DS> {
        DiffPTData {
            diff_pts_map: HashMap::new(),
            propa_pts_map: HashMap::new(),
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.diff_pts_map.clear();
        self.propa_pts_map.clear();
    }

    /// Adds element to the points-to set associated with var.
    /// Returns false if elem is already in this set
    #[inline]
    pub fn add_pts(&mut self, var: K, elem: D) -> bool {
        if let Some(propa) = self.propa_pts_map.get(&var) {
            if propa.contains(elem) {
                return false;
            }
        }
        let diff = self.diff_pts_map.entry(var).or_insert(DS::new());
        diff.insert(elem)
    }

    /// diff_pts(dst_var) = diff_pts(dst_var) U (diff_pts(src_var) - propa_pts(dst_var)).
    pub fn union_diff_pts(&mut self, dst_var: K, src_var: K) -> bool {
        if dst_var == src_var {
            return false;
        }
        let mut changed = false;
        if let Some(diff) = self.diff_pts_map.get(&src_var) {
            let src_ds = diff.clone();
            changed |= self.union_pts_to(dst_var, &src_ds);
        }
        changed
    }

    /// diff_pts(dst_var) = diff_pts(dst_var) U (pts(src_var) - propa_pts(dst_var)).
    #[inline]
    pub fn union_pts(&mut self, dst_var: K, src_var: K) -> bool {
        if dst_var == src_var {
            return false;
        }
        let mut changed = false;
        if let Some(diff) = self.diff_pts_map.get(&src_var) {
            let src_ds = diff.clone();
            changed |= self.union_pts_to(dst_var, &src_ds);
        }
        if let Some(propa) = self.propa_pts_map.get(&src_var) {
            let src_ds = propa.clone();
            changed |= self.union_pts_to(dst_var, &src_ds);
        }
        changed
    }

    /// Performs diff_pts(dst_var) = diff_pts(dst_var) U (src_ds - propa_pts(dst_var)).
    #[inline]
    pub fn union_pts_to(&mut self, dst_var: K, src_ds: &DS) -> bool {
        let diff = self.diff_pts_map.entry(dst_var).or_insert(DS::new());
        let propa = self.propa_pts_map.entry(dst_var).or_insert(DS::new());
        let mut new = src_ds.clone();
        new.subtract(propa);
        diff.union(&new)
    }

    /// Removes element from the points-to set of var.
    #[inline]
    pub fn remove_pts_elem(&mut self, var: K, elem: D) -> bool {
        let diff = self.diff_pts_map.entry(var).or_insert(DS::new());
        let propa = self.propa_pts_map.entry(var).or_insert(DS::new());
        diff.remove(elem) | propa.remove(elem)
    }

    /// Get diff points to.
    #[inline]
    pub fn get_diff_pts(&self, var: K) -> Option<&DS> {
        self.diff_pts_map.get(&var)
    }

    /// Returns a mutable reference to the diff points to set.
    #[inline]
    pub fn get_mut_diff_pts(&mut self, var: K) -> Option<&mut DS> {
        self.diff_pts_map.get_mut(&var)
    }

    /// Get propagated points to.
    #[inline]
    pub fn get_propa_pts(&self, var: K) -> Option<&DS> {
        self.propa_pts_map.get(&var)
    }

    /// Returns a mutable reference to the propa points to set.
    #[inline]
    pub fn get_mut_propa_pts(&mut self, var: K) -> Option<&mut DS> {
        self.propa_pts_map.get_mut(&var)
    }

    /// Sets all diff elems to propa elems.
    pub fn flush(&mut self, var: K) {
        if !self.diff_pts_map.contains_key(&var) {
            return;
        }

        let diff = self.diff_pts_map.get_mut(&var).unwrap();
        let propa = self.propa_pts_map.entry(var).or_insert(DS::new());
        propa.union(diff);
        diff.clear();
    }

    /// Fully clears the points-to set of var.
    #[inline]
    pub fn clear_pts(&mut self, var: K) {
        if let Some(diff) = self.diff_pts_map.get_mut(&var) {
            diff.clear();
        }
        if let Some(propa) = self.propa_pts_map.get_mut(&var) {
            propa.clear();
        }
    }

    /// Clear propagated points-to set of var.
    pub fn clear_diff_pts(&mut self, var: K) {
        if let Some(diff) = self.diff_pts_map.get_mut(&var) {
            diff.clear()
        }
    }

    /// Clear propagated points-to set of var.
    pub fn clear_propa_pts(&mut self, var: K) {
        if let Some(propa) = self.propa_pts_map.get_mut(&var) {
            propa.clear()
        }
    }

    /// Dump stored keys and points-to sets.
    #[inline]
    pub fn dump_pt_data(&self) {
        unimplemented!("implement if/when necessary");
    }
}
