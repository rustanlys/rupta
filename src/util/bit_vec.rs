//! Our implemention of bit vector is modifed from `rustc_index::bit_set`, see
//! <https://doc.rust-lang.org/stable/nightly-rustc/src/rustc_index/bit_set.rs.html>

use std::fmt;
use std::fmt::Debug;
use std::hash::Hash;
use std::iter;
use std::marker::PhantomData;
use std::mem;
use std::slice;

use rustc_macros::{Decodable, Encodable};

type Word = u64;
const WORD_BYTES: usize = mem::size_of::<Word>();
const WORD_BITS: usize = WORD_BYTES * 8;

/// Represents some newtyped `usize` wrapper.
///
/// Purpose: avoid mixing indexes for different bitvector domains.
pub trait Idx: Copy + 'static + Eq + PartialEq + Debug + Hash {
    fn new(idx: usize) -> Self;

    fn index(self) -> usize;

    fn increment_by(&mut self, amount: usize) {
        *self = self.plus(amount);
    }

    fn plus(self, amount: usize) -> Self {
        Self::new(self.index() + amount)
    }
}

impl Idx for usize {
    #[inline]
    fn new(idx: usize) -> Self {
        idx
    }
    #[inline]
    fn index(self) -> usize {
        self
    }
}

impl Idx for u32 {
    #[inline]
    fn new(idx: usize) -> Self {
        assert!(idx <= u32::MAX as usize);
        idx as u32
    }
    #[inline]
    fn index(self) -> usize {
        self as usize
    }
}

/// A growable bit-vector type with a dense representation.
///
/// `T` is an index type, typically a newtyped `usize` wrapper, but it can also
/// just be `usize`.
#[derive(Eq, PartialEq, Hash, Decodable, Encodable)]
pub struct BitVec<T> {
    words: Vec<Word>,
    marker: PhantomData<T>,
}

impl<T: Idx> BitVec<T> {
    /// Creates a new, empty bitvec with 0 elements.
    #[inline]
    pub fn new_empty() -> BitVec<T> {
        BitVec {
            words: Vec::new(),
            marker: PhantomData,
        }
    }

    /// Creates a new, empty bitvec with a given capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> BitVec<T> {
        let num_words = num_words(capacity);
        BitVec {
            words: vec![0; num_words],
            marker: PhantomData,
        }
    }

    /// Creates a new bit vec from a give vec.
    #[inline]
    pub fn from_vec(from: &Vec<T>) -> BitVec<T> {
        let mut ret = Self::new_empty();
        for elem in from {
            ret.insert(*elem);
        }
        ret
    }

    /// Ensure that the set can hold at least `capacity` elements.
    #[inline]
    pub fn ensure(&mut self, capacity: usize) {
        let min_num_words = num_words(capacity);
        if self.words.len() < min_num_words {
            self.words.resize(min_num_words, 0)
        }
    }

    /// Clear all elements.
    #[inline]
    pub fn clear(&mut self) {
        self.words.fill(0);
    }

    /// Count the number of set bits in the set.
    pub fn count(&self) -> usize {
        self.words.iter().map(|e| e.count_ones() as usize).sum()
    }

    /// Returns `true` if `self` contains `elem`.
    #[inline]
    pub fn contains(&self, elem: T) -> bool {
        if capacity(&self.words) <= elem.index() {
            return false;
        }
        let (word_index, mask) = word_index_and_mask(elem);
        (self.words[word_index] & mask) != 0
    }

    /// Is `self` is a (non-strict) superset of `other`?
    #[inline]
    pub fn superset(&self, other: &BitVec<T>) -> bool {
        let mut tmp_words = self.words.clone();
        tmp_words.resize(other.words.len(), 0);
        tmp_words.iter().zip(&other.words).all(|(a, b)| (a & b) == *b)
    }

    /// Is the set empty?
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|a| *a == 0)
    }

    /// Insert `elem`. Returns whether the set has changed.
    #[inline]
    pub fn insert(&mut self, elem: T) -> bool {
        self.ensure(elem.index() + 1);
        let (word_index, mask) = word_index_and_mask(elem);
        let word_ref = &mut self.words[word_index];
        let word = *word_ref;
        let new_word = word | mask;
        *word_ref = new_word;
        new_word != word
    }

    /// Sets all bits to true.
    pub fn insert_all(&mut self) {
        self.words.fill(!0);
    }

    /// Returns `true` if the set has changed.
    #[inline]
    pub fn remove(&mut self, elem: T) -> bool {
        if capacity(&self.words) <= elem.index() {
            return false;
        }
        let (word_index, mask) = word_index_and_mask(elem);
        let word_ref = &mut self.words[word_index];
        let word = *word_ref;
        let new_word = word & !mask;
        *word_ref = new_word;
        new_word != word
    }

    /// Gets a slice of the underlying words.
    pub fn words(&self) -> &[Word] {
        &self.words
    }

    /// Iterates over the indices of set bits in a sorted order.
    #[inline]
    pub fn iter(&self) -> BitIter<'_, T> {
        BitIter::new(&self.words)
    }

    pub fn union(&mut self, other: &BitVec<T>) -> bool {
        self.ensure(capacity(&other.words));
        bitwise(&mut self.words, &other.words, |a, b| a | b)
    }

    pub fn subtract(&mut self, other: &BitVec<T>) -> bool {
        bitwise(&mut self.words, &other.words, |a, b| a & !b)
    }

    pub fn intersect(&mut self, other: &BitVec<T>) -> bool {
        bitwise(&mut self.words, &other.words, |a, b| a & b)
    }
}

impl<T> Clone for BitVec<T> {
    fn clone(&self) -> Self {
        BitVec {
            words: self.words.clone(),
            marker: PhantomData,
        }
    }

    fn clone_from(&mut self, from: &Self) {
        self.words.clone_from(&from.words);
    }
}

impl<T: Idx> Debug for BitVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T: Idx> ToString for BitVec<T> {
    fn to_string(&self) -> String {
        let mut result = String::new();
        let mut sep = '[';

        // Note: this is a little endian printout of bytes.

        // i tracks how many bits we have printed so far.
        for word in &self.words {
            let mut word = *word;
            for _ in 0..WORD_BYTES {
                // If less than a byte remains, then mask just that many bits.
                let mask = 0xFF;
                let byte = word & mask;
                result.push_str(&format!("{}{:02x}", sep, byte));
                word >>= 8;
                sep = '-';
            }
            sep = '|';
        }
        result.push(']');

        result
    }
}

pub struct BitIter<'a, T: Idx> {
    /// A copy of the current word, but with any already-visited bits cleared.
    /// (This lets us use `trailing_zeros()` to find the next set bit.) When it
    /// is reduced to 0, we move onto the next word.
    word: Word,

    /// The offset (measured in bits) of the current word.
    offset: usize,

    /// Underlying iterator over the words.
    iter: slice::Iter<'a, Word>,

    marker: PhantomData<T>,
}

impl<'a, T: Idx> BitIter<'a, T> {
    #[inline]
    fn new(words: &'a [Word]) -> BitIter<'a, T> {
        // We initialize `word` and `offset` to degenerate values. On the first
        // call to `next()` we will fall through to getting the first word from
        // `iter`, which sets `word` to the first word (if there is one) and
        // `offset` to 0. Doing it this way saves us from having to maintain
        // additional state about whether we have started.
        BitIter {
            word: 0,
            offset: usize::MAX - (WORD_BITS - 1),
            iter: words.iter(),
            marker: PhantomData,
        }
    }
}

impl<'a, T: Idx> Iterator for BitIter<'a, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        loop {
            if self.word != 0 {
                // Get the position of the next set bit in the current word,
                // then clear the bit.
                let bit_pos = self.word.trailing_zeros() as usize;
                let bit = 1 << bit_pos;
                self.word ^= bit;
                return Some(T::new(bit_pos + self.offset));
            }

            // Move onto the next word. `wrapping_add()` is needed to handle
            // the degenerate initial value given to `offset` in `new()`.
            let word = self.iter.next()?;
            self.word = *word;
            self.offset = self.offset.wrapping_add(WORD_BITS);
        }
    }
}

#[inline]
fn capacity(words: &[Word]) -> usize {
    words.len() * WORD_BITS
}

#[inline]
fn num_words<T: Idx>(capacity: T) -> usize {
    (capacity.index() + WORD_BITS - 1) / WORD_BITS
}

#[inline]
fn word_index_and_mask<T: Idx>(elem: T) -> (usize, Word) {
    let elem = elem.index();
    let word_index = elem / WORD_BITS;
    let mask = 1 << (elem % WORD_BITS);
    (word_index, mask)
}

#[inline]
fn bitwise<Op>(out_vec: &mut [Word], in_vec: &[Word], op: Op) -> bool
where
    Op: Fn(Word, Word) -> Word,
{
    let mut changed = 0;
    for (out_elem, in_elem) in iter::zip(out_vec, in_vec) {
        let old_val = *out_elem;
        let new_val = op(old_val, *in_elem);
        *out_elem = new_val;
        // This is essentially equivalent to a != with changed being a bool, but
        // in practice this code gets auto-vectorized by the compiler for most
        // operators. Using != here causes us to generate quite poor code as the
        // compiler tries to go back to a boolean on each loop iteration.
        changed |= old_val ^ new_val;
    }
    changed != 0
}
