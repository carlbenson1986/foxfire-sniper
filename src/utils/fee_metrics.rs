use std::collections::BTreeSet;
use tracing::{debug, trace};

struct RollingAverage {
    buffer: Vec<u64>,
    size: usize,
    index: usize,
    sum: u64,
}

impl RollingAverage {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0; size],
            size,
            index: 0,
            sum: 0,
        }
    }

    fn add(&mut self, value: u64) {
        self.sum = self.sum.wrapping_sub(self.buffer[self.index]);
        self.buffer[self.index] = value;
        self.sum = self.sum.wrapping_add(value);
        self.index = (self.index + 1) % self.size;
    }

    fn get_average(&self) -> u64 {
        self.sum / self.size as u64
    }
}

struct Percentile {
    data: BTreeSet<u64>,
    capacity: usize,
}

impl Percentile {
    fn new(capacity: usize) -> Self {
        Self {
            data: BTreeSet::new(),
            capacity,
        }
    }

    fn add(&mut self, value: u64) {
        if self.data.len() == self.capacity {
            if let Some(&min_value) = self.data.iter().next() {
                self.data.remove(&min_value);
            }
        }
        self.data.insert(value);
    }

    fn get_percentile(&self, percentile: f64) -> Option<u64> {
        if self.data.is_empty() {
            return None;
        }
        let k = ((percentile / 100.0) * (self.data.len() as f64 - 1.0)).round() as usize;
        self.data.iter().nth(k).copied()
    }
}

pub struct FeeMetrics {
    rolling_average: RollingAverage,
    percentile: Percentile,
}

impl Default for FeeMetrics {
    fn default() -> Self {
        Self::new(10, 100)
    }
}

impl FeeMetrics {
    pub(crate) fn new(ra_size: usize, percentile_capacity: usize) -> Self {
        Self {
            rolling_average: RollingAverage::new(ra_size),
            percentile: Percentile::new(percentile_capacity),
        }
    }

    pub(crate) fn add_fee(&mut self, fee: u64) {
        self.rolling_average.add(fee);
        self.percentile.add(fee);
        trace!("Updated optimal 75 percentile transfer fee: {:?}", self.percentile.get_percentile(75.0).unwrap_or(0));
    }

    fn get_rolling_average(&self) -> u64 {
        self.rolling_average.get_average()
    }

    pub(crate) fn get_percentile(&self, p: f64) -> Option<u64> {
        self.percentile.get_percentile(p)
    }
}
