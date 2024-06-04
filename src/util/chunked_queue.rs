// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use arrayvec::ArrayVec;
use std::fmt::{Debug, Formatter, Result};
use std::marker::PhantomData;
use std::ptr::NonNull;

// The maximum number of elements a chunk can hold.
const CHUNK_CAP: usize = 60;

/// This queue is implemented as a linked list of chunks, where each chunk is a small buffer
/// that can hold a handful of elements.
/// Chunks need to be dynamically allocated as elements get pushed.
/// This queue is supposed to be faster thanthan `LinkedList`.
pub struct ChunkedQueue<T> {
    head: NonNull<Chunk<T>>,
    tail: NonNull<Chunk<T>>,
    len: usize,
    marker: PhantomData<Box<Chunk<T>>>,
}

impl<T: Debug> Debug for ChunkedQueue<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T> Drop for ChunkedQueue<T> {
    fn drop(&mut self) {
        unsafe {
            let mut all_dropped = false;
            while !all_dropped {
                let chunk = Box::from_raw(self.head.as_ptr());
                if chunk.next.is_some() {
                    self.head = chunk.next.unwrap();
                } else {
                    all_dropped = true;
                }
                drop(chunk);
            }
        }
    }
}

pub struct Chunk<T> {
    next: Option<NonNull<Chunk<T>>>,
    prev: Option<NonNull<Chunk<T>>>,
    elems: ArrayVec<T, CHUNK_CAP>,
}

impl<T: Debug> Debug for Chunk<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        self.elems.fmt(f)
    }
}

impl<T> Chunk<T> {
    fn new() -> Self {
        Chunk {
            next: None,
            prev: None,
            elems: ArrayVec::new(),
        }
    }

    fn len(&self) -> usize {
        self.elems.len()
    }

    fn get_elem_ref(&self, index: usize) -> Option<&T> {
        if index < self.elems.len() {
            unsafe { Some(&*self.elems.as_ptr().add(index)) }
        } else {
            None
        }
    }
}

impl<T: Copy> Chunk<T> {
    fn get_elem(&self, index: usize) -> Option<T> {
        if index < self.elems.len() {
            unsafe { Some(*self.elems.as_ptr().add(index)) }
        } else {
            None
        }
    }
}

impl<T> Default for ChunkedQueue<T> {
    /// Creates an empty `ChunkedQueue<T>`.
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ChunkedQueue<T> {
    /// Creates an empty `ChunkedQueue`.
    #[inline]
    pub fn new() -> Self {
        let chunk = Self::new_chunk();
        ChunkedQueue {
            head: chunk,
            tail: chunk,
            len: 0,
            marker: PhantomData,
        }
    }

    /// Returns the length of the `ChunkedQueue`.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the `ChunkedQueue` is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Appends an element to the back of a queue.
    pub fn push(&mut self, elem: T) {
        // If the tail chunk is full, insert a new chunk.
        let is_full = unsafe { (*self.tail.as_ptr()).elems.is_full() };
        if is_full {
            let chunk = Self::new_chunk();
            unsafe {
                (*self.tail.as_ptr()).next = Some(chunk);
                (*chunk.as_ptr()).prev = Some(self.tail);
            }
            self.tail = chunk;
        }
        unsafe {
            let chunk = &mut *self.tail.as_ptr();
            chunk.elems.push(elem);
        }
        self.len += 1;
    }

    /// Provides a forward iterator.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            chunk: self.head,
            index: 0,
            marker: PhantomData,
        }
    }

    /// Create a new chunk.
    #[inline]
    fn new_chunk() -> NonNull<Chunk<T>> {
        let chunk: Box<Chunk<T>> = Box::new(Chunk::new());
        Box::leak(chunk).into()
    }
}

impl<T: Copy> ChunkedQueue<T> {
    /// Provides a forward copied iterator.
    #[inline]
    pub fn iter_copied(&self) -> IterCopied<T> {
        IterCopied {
            chunk: self.head,
            index: 0,
            marker: PhantomData,
        }
    }
}

pub struct Iter<'a, T> {
    /// A pointer to the current chunk.
    chunk: NonNull<Chunk<T>>,

    /// The index of the next element in the chunk.
    index: usize,

    marker: PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        // Reach to the end of the chunk
        if self.index == CHUNK_CAP {
            // Move onto the next chunk if the next chunk is not none.
            if let Some(chunk) = unsafe { (*self.chunk.as_ptr()).next } {
                self.chunk = chunk;
                self.index = 0;
            } else {
                return None;
            }
        }
        let elem = unsafe { (&*self.chunk.as_ptr()).get_elem_ref(self.index) };
        self.index += 1;
        elem
    }
}

/// This Iter supports iterating a dynamically growing queue that contains
/// copyable elements
#[derive(Copy, Clone)]
pub struct IterCopied<T> {
    /// A pointer to the current chunk.
    chunk: NonNull<Chunk<T>>,

    /// The index of the next element in the chunk.
    index: usize,

    marker: PhantomData<T>,
}

impl<T: Copy> Iterator for IterCopied<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        // Reach to the end of the chunk
        if self.index == CHUNK_CAP {
            // Move onto the next chunk if the next chunk is not none.
            if let Some(chunk) = unsafe { (*self.chunk.as_ptr()).next } {
                self.chunk = chunk;
                self.index = 0;
            } else {
                return None;
            }
        }
        let chunk = unsafe { &*self.chunk.as_ptr() };
        if self.index < chunk.len() {
            let elem = chunk.get_elem(self.index);
            self.index += 1;
            elem
        } else {
            None
        }
    }
}
