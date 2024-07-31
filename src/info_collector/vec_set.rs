#![allow(unused)]
/**
 * 项目现需要一种数据结构，该数据结构能够像数组一样随机访问，但是又能保证元素的唯一性。
 * 因此，我们可以考虑使用哈希表结合向量来实现这个数据结构。
 */
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Index;
use std::rc::Rc;
use serde::ser::SerializeSeq;
use serde::Serialize;

pub struct VecSet<T> {
    pub data: Vec<Rc<T>>,
    included: HashMap<Rc<T>, usize>,
}

impl<T> Default for VecSet<T> {
    fn default() -> Self {
        Self { data: Default::default(), included: Default::default() }
    }
}

impl<T: Eq + Hash + Serialize> Serialize for VecSet<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        let mut seq = serializer.serialize_seq(Some(self.data.len()))?;
        for item in self.data.iter().map(|x| x.as_ref()) {
            seq.serialize_element(item)?;
        }

        seq.end()
    }
}

impl<T: Eq + Hash> Index<usize> for VecSet<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

impl<T: Eq + Hash> VecSet<T> {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            included: HashMap::new(),
        }
    }

    pub fn insert(&mut self, value: T) -> usize {
        let new_data = Rc::new(value);
        let idx_of_new_data = self.data.len();
        match self.included.entry(Rc::clone(&new_data)) {
            std::collections::hash_map::Entry::Occupied(oe) => *oe.get(),
            std::collections::hash_map::Entry::Vacant(ve) => {
                self.data.push(new_data);
                *ve.insert(idx_of_new_data)
            }
        }
    }
}
