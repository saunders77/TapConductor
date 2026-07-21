//! A small bounded SPSC queue.
//!
//! `Producer` and `Consumer` are deliberately not clonable. Requiring `&mut
//! self` for each operation enforces the one-producer/one-consumer contract in
//! safe Rust while the shared ring itself remains lock-free.

use core::cell::UnsafeCell;
use core::fmt;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Ring<T, const N: usize> {
    // Allocated once during setup so large real-time capacities do not consume
    // the Windows UI thread's comparatively small stack.
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl<T, const N: usize> Ring<T, N> {
    fn new() -> Self {
        assert!(N > 0, "an SPSC queue needs at least one slot");
        Self {
            slots: (0..N)
                .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
}

// SAFETY: A Producer has exclusive write access to the tail slot and a
// Consumer has exclusive read access to the head slot. Release/acquire
// publication prevents either side from observing a slot in transition.
unsafe impl<T: Send, const N: usize> Sync for Ring<T, N> {}
unsafe impl<T: Send, const N: usize> Send for Ring<T, N> {}

impl<T, const N: usize> Drop for Ring<T, N> {
    fn drop(&mut self) {
        let mut head = *self.head.get_mut();
        let tail = *self.tail.get_mut();
        while head != tail {
            let index = head % N;
            // SAFETY: Every position between head and tail was initialized by
            // the producer and has not yet been consumed.
            unsafe { self.slots[index].get_mut().assume_init_drop() };
            head = head.wrapping_add(1);
        }
    }
}

pub struct Producer<T, const N: usize> {
    ring: Arc<Ring<T, N>>,
}

pub struct Consumer<T, const N: usize> {
    ring: Arc<Ring<T, N>>,
}

pub fn spsc_channel<T, const N: usize>() -> (Producer<T, N>, Consumer<T, N>) {
    let ring = Arc::new(Ring::new());
    (
        Producer {
            ring: Arc::clone(&ring),
        },
        Consumer { ring },
    )
}

impl<T, const N: usize> Producer<T, N> {
    /// Attempts to publish without blocking or allocating.
    pub fn try_push(&mut self, value: T) -> Result<(), QueueFull<T>> {
        let tail = self.ring.tail.load(Ordering::Relaxed);
        let head = self.ring.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) >= N {
            return Err(QueueFull(value));
        }
        let index = tail % N;
        // SAFETY: This is the sole Producer; the capacity check established
        // that the Consumer no longer owns this slot.
        unsafe { (*self.ring.slots[index].get()).write(value) };
        self.ring
            .tail
            .store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn is_full(&self) -> bool {
        let tail = self.ring.tail.load(Ordering::Relaxed);
        let head = self.ring.head.load(Ordering::Acquire);
        tail.wrapping_sub(head) >= N
    }
}

impl<T, const N: usize> Consumer<T, N> {
    /// Attempts to consume without blocking or allocating.
    pub fn try_pop(&mut self) -> Option<T> {
        let head = self.ring.head.load(Ordering::Relaxed);
        let tail = self.ring.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let index = head % N;
        // SAFETY: This is the sole Consumer and the acquire load observed the
        // Producer's initialized slot.
        let value = unsafe { (*self.ring.slots[index].get()).assume_init_read() };
        self.ring
            .head
            .store(head.wrapping_add(1), Ordering::Release);
        Some(value)
    }

    pub fn is_empty(&self) -> bool {
        self.ring.head.load(Ordering::Relaxed) == self.ring.tail.load(Ordering::Acquire)
    }
}

pub struct QueueFull<T>(pub T);

impl<T> QueueFull<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for QueueFull<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("QueueFull(..)")
    }
}

impl<T> fmt::Display for QueueFull<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("real-time queue is full")
    }
}

impl<T: fmt::Debug> std::error::Error for QueueFull<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_fifo_and_reports_full() {
        let (mut producer, mut consumer) = spsc_channel::<u32, 2>();
        producer.try_push(10).unwrap();
        producer.try_push(20).unwrap();
        assert_eq!(producer.try_push(30).unwrap_err().into_inner(), 30);
        assert_eq!(consumer.try_pop(), Some(10));
        producer.try_push(30).unwrap();
        assert_eq!(consumer.try_pop(), Some(20));
        assert_eq!(consumer.try_pop(), Some(30));
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn drops_unconsumed_values() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct CountDrop<'a>(&'a AtomicUsize);
        impl Drop for CountDrop<'_> {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let drops = AtomicUsize::new(0);
        {
            let (mut producer, _consumer) = spsc_channel::<CountDrop<'_>, 2>();
            producer.try_push(CountDrop(&drops)).unwrap();
            producer.try_push(CountDrop(&drops)).unwrap();
        }
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    }
}
