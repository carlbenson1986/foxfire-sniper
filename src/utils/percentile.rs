use std::collections::BTreeSet;

pub struct Percentile {
    data: BTreeSet<u64>,
    capacity: usize,
}

impl Percentile {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            data: BTreeSet::new(),
            capacity,
        }
    }

    pub(crate) fn add(&mut self, value: u64) {
        if self.data.len() == self.capacity {
            if let Some(&min_value) = self.data.iter().next() {
                self.data.remove(&min_value);
            }
        }
        self.data.insert(value);
    }

    pub(crate) fn get_percentile(&self, percentile: f64) -> Option<u64> {
        if self.data.is_empty() {
            return None;
        }
        let k = ((percentile / 100.0) * (self.data.len() as f64 - 1.0)).round() as usize;
        self.data.iter().nth(k).copied()
    }
}
