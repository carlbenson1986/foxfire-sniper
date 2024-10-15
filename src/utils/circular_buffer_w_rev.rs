use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// A circular buffer that combines the functionality of a HashMap and VecDeque and also can reverse lookup the key from the value.
#[derive(Default, Debug, Clone)]
pub struct CircularBufferWithLookupByValue<K, V> {
    map: HashMap<K, V>,
    reverse_map: HashMap<V, K>,
    deque: VecDeque<K>,
    reverse_deque: VecDeque<V>,
    capacity: usize,
}

impl<K, V> CircularBufferWithLookupByValue<K, V>
where
    K: Eq + Hash + Clone,
    V: Eq + Hash + Clone, // Ensure that V can be used as a key in reverse_map
{
    /// Creates a new CircularBuffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            reverse_map: HashMap::new(),
            deque: VecDeque::with_capacity(capacity),
            reverse_deque: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Inserts a key-value pair into the buffer.
    /// If the buffer is full, the oldest entry is removed.
    pub fn insert(&mut self, key: K, value: V) {
        // If the buffer is full, remove the oldest entry from both maps
        if self.deque.len() == self.capacity {
            if let Some(old_key) = self.deque.pop_front() {
                if let Some(old_value) = self.map.remove(&old_key) {
                    // Remove from reverse map and reverse deque
                    self.reverse_map.remove(&old_value);
                    self.reverse_deque.pop_front();
                }
            }
        }

        // Add the new key-value pair
        self.deque.push_back(key.clone());
        self.reverse_deque.push_back(value.clone());

        // Update both maps
        self.map.insert(key.clone(), value.clone());
        self.reverse_map.insert(value, key);
    }

    /// Checks if the buffer contains the given key.
    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// Checks if the buffer contains the given value.
    pub fn contains_value(&self, value: &V) -> bool {
        self.reverse_map.contains_key(value)
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

    /// Gets the key associated with the given value.
    pub fn get_by_value(&self, value: &V) -> Option<&K> {
        self.reverse_map.get(value)
    }
    
    pub fn pop_front(&mut self) -> Option<(K, V)> {
        if let Some(key) = self.deque.pop_front() {
            if let Some(value) = self.map.remove(&key) {
                // Remove from reverse map and reverse deque
                self.reverse_map.remove(&value);
                self.reverse_deque.pop_front();
                return Some((key, value));
            }
        }
        None
    }
    
    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.map.remove(key) {
            // Remove from reverse map and reverse deque
            self.reverse_map.remove(&value);
            self.reverse_deque.pop_front();
            return Some(value);
        }
        None
    }
    
    pub fn get_all_values(&self) -> Vec<V> {
        self.reverse_deque.iter().cloned().collect()
    }
}
