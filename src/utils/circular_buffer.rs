use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// A circular buffer that combines the functionality of a HashMap and VecDeque.
#[derive(Default, Debug, Clone)]
pub struct CircularBuffer<K, V> {
    map: HashMap<K, V>,
    deque: VecDeque<K>,
    capacity: usize,
}

impl<K, V> CircularBuffer<K, V>
where
    K: Eq + Clone + Hash,
{
    /// Creates a new CircularBuffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            deque: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Inserts a key-value pair into the buffer.
    /// If the buffer is full, the oldest entry is removed.
    pub fn insert(&mut self, key: K, value: V) {
        // If the buffer is full, remove the oldest entry from the map and deque
        if self.deque.len() == self.capacity {
            if let Some(old_key) = self.deque.pop_front() {
                self.map.remove(&old_key);
            }
        }

        // Add the new key-value pair
        self.deque.push_back(key.clone());
        self.map.insert(key, value);
    }

    /// Checks if the buffer contains the given key.
    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// Gets the current size of the buffer.
    pub fn len(&self) -> usize {
        self.deque.len()
    }

    /// Checks if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.deque.is_empty()
    }

    /// Gets the value associated with the given key.
    pub fn get_by_key(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    /// Removes and returns the oldest key-value pair from the buffer.
    pub fn pop_front(&mut self) -> Option<(K, V)> {
        if let Some(key) = self.deque.pop_front() {
            if let Some(value) = self.map.remove(&key) {
                return Some((key, value));
            }
        }
        None
    }

    /// Gets all the values in the buffer in order.
    pub fn get_all_values(&self) -> Vec<&V> {
        self.deque.iter().filter_map(|key| self.map.get(key)).collect()
    }
}
