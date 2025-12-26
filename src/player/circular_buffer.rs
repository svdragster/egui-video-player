use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

/// Thread-safe circular buffer that overwrites oldest data when full.
/// Unlike non-overwriting ring buffers, push operations never block.
pub struct CircularBuffer<T> {
    inner: Mutex<VecDeque<T>>,
    capacity: usize,
}

impl<T> CircularBuffer<T> {
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        })
    }

    /// Push a single item. Drops oldest if at capacity.
    #[allow(dead_code)]
    pub fn push(&self, item: T) {
        let mut buf = self.inner.lock();
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(item);
    }

    /// Push multiple items. Drops oldest as needed.
    /// Uses batch operations for O(1) amortized complexity.
    pub fn push_slice(&self, items: &[T])
    where
        T: Clone,
    {
        let mut buf = self.inner.lock();
        let items_len = items.len();

        if items_len >= self.capacity {
            // New data exceeds capacity - just keep last `capacity` items
            buf.clear();
            buf.extend(items[items_len - self.capacity..].iter().cloned());
        } else {
            // Make room by draining oldest items if needed
            let available = self.capacity - buf.len();
            if items_len > available {
                let to_remove = items_len - available;
                buf.drain(..to_remove);
            }
            buf.extend(items.iter().cloned());
        }
    }

    /// Try to pop the oldest item.
    pub fn try_pop(&self) -> Option<T> {
        self.inner.lock().pop_front()
    }

    /// Clear all items.
    pub fn clear(&self) {
        self.inner.lock().clear();
    }

    /// Check if empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }

    /// Current number of items.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }
}
