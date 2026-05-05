//! A fixed-capacity ring buffer with circular indexing.
//!
//! Backed by a single `Vec<T>` plus a head pointer. Pushing into a full
//! buffer overwrites the oldest element and returns it, which lets
//! streaming statistics maintain incremental sums cheaply.

use crate::error::{CryptoTuiError, Result};

/// Fixed-capacity ring buffer.
///
/// Items are written at the *head*; once the buffer fills, each
/// subsequent push overwrites the oldest element. [`Self::iter`] yields
/// the elements in oldest-to-newest order regardless of where the head
/// currently sits.
///
/// # Examples
///
/// ```
/// use cryptotui::indicators::RingBuffer;
///
/// let mut rb = RingBuffer::<i32>::new(3).expect("capacity > 0");
/// rb.push(1);
/// rb.push(2);
/// rb.push(3);
/// // Pushing into a full buffer evicts the oldest element.
/// assert_eq!(rb.push(4), Some(1));
/// let snapshot: Vec<i32> = rb.iter().copied().collect();
/// assert_eq!(snapshot, vec![2, 3, 4]);
/// ```
#[derive(Debug, Clone)]
pub struct RingBuffer<T> {
    buffer: Vec<T>,
    head: usize,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    /// Create a new ring buffer with the given fixed capacity.
    ///
    /// Returns [`CryptoTuiError::InvalidConfig`] if `capacity == 0`.
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(CryptoTuiError::InvalidConfig(
                "RingBuffer capacity must be > 0".into(),
            ));
        }
        Ok(Self {
            buffer: Vec::with_capacity(capacity),
            head: 0,
            capacity,
        })
    }

    /// Push `item` into the buffer.
    ///
    /// If the buffer was full before the call, returns the evicted
    /// oldest element. Otherwise returns `None`.
    pub fn push(&mut self, item: T) -> Option<T> {
        if self.buffer.len() < self.capacity {
            self.buffer.push(item);
            None
        } else {
            // self.head is always < self.capacity by maintenance, and
            // buffer.len() == capacity here, so the index is in bounds.
            let old = std::mem::replace(&mut self.buffer[self.head], item);
            self.head = (self.head + 1) % self.capacity;
            Some(old)
        }
    }

    /// Number of elements currently stored.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether the buffer holds zero elements.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Whether the buffer has reached its fixed capacity.
    pub fn is_full(&self) -> bool {
        self.buffer.len() == self.capacity
    }

    /// Fixed capacity supplied at construction time.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Iterate over elements in oldest-to-newest order.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        let len = self.buffer.len();
        let start = if len < self.capacity { 0 } else { self.head };
        let cap = self.capacity;
        let buf = &self.buffer;
        (0..len).map(move |i| &buf[(start + i) % cap])
    }

    /// Remove every element, leaving the buffer empty.
    /// Allocated capacity is preserved.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.head = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_zero_capacity() {
        assert!(RingBuffer::<i32>::new(0).is_err());
    }

    #[test]
    fn fresh_buffer_is_empty() {
        let rb = RingBuffer::<i32>::new(3).unwrap();
        assert_eq!(rb.len(), 0);
        assert!(rb.is_empty());
        assert!(!rb.is_full());
        assert_eq!(rb.capacity(), 3);
    }

    #[test]
    fn fills_to_capacity_without_eviction() {
        let mut rb = RingBuffer::new(3).unwrap();
        assert_eq!(rb.push(1), None);
        assert_eq!(rb.push(2), None);
        assert_eq!(rb.push(3), None);
        assert!(rb.is_full());
        assert_eq!(rb.len(), 3);
    }

    #[test]
    fn evicts_oldest_when_full() {
        let mut rb = RingBuffer::new(3).unwrap();
        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.push(4), Some(1));
        assert_eq!(rb.push(5), Some(2));
        assert_eq!(rb.push(6), Some(3));
    }

    #[test]
    fn iter_yields_oldest_to_newest_partial() {
        let mut rb = RingBuffer::new(5).unwrap();
        rb.push(10);
        rb.push(20);
        rb.push(30);
        let v: Vec<i32> = rb.iter().copied().collect();
        assert_eq!(v, vec![10, 20, 30]);
    }

    #[test]
    fn iter_yields_oldest_to_newest_after_one_wrap() {
        let mut rb = RingBuffer::new(3).unwrap();
        rb.push(1);
        rb.push(2);
        rb.push(3);
        rb.push(4); // evicts 1
        rb.push(5); // evicts 2
        let v: Vec<i32> = rb.iter().copied().collect();
        assert_eq!(v, vec![3, 4, 5]);
    }

    #[test]
    fn iter_yields_oldest_to_newest_after_many_wraps() {
        let mut rb = RingBuffer::new(4).unwrap();
        for i in 1..=10 {
            rb.push(i);
        }
        let v: Vec<i32> = rb.iter().copied().collect();
        assert_eq!(v, vec![7, 8, 9, 10]);
    }

    #[test]
    fn clear_resets_state_then_accepts_new_pushes() {
        let mut rb = RingBuffer::new(3).unwrap();
        rb.push(1);
        rb.push(2);
        rb.clear();
        assert!(rb.is_empty());
        assert_eq!(rb.iter().count(), 0);
        rb.push(99);
        assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![99]);
    }

    #[test]
    fn iter_lifetime_does_not_keep_buffer_borrowed_after_collect() {
        // Sanity check that the borrow released after collect lets us mutate.
        let mut rb = RingBuffer::new(2).unwrap();
        rb.push(1);
        rb.push(2);
        let snapshot: Vec<i32> = rb.iter().copied().collect();
        rb.push(3); // evicts 1
        assert_eq!(snapshot, vec![1, 2]);
        assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![2, 3]);
    }
}
