pub struct RollingAverage {
    buffer: Vec<u64>,
    size: usize,
    index: usize,
    sum: u64,
}

impl RollingAverage {
    pub(crate) fn new(size: usize) -> Self {
        Self {
            buffer: vec![0; size],
            size,
            index: 0,
            sum: 0,
        }
    }

    pub(crate) fn add(&mut self, value: u64) {
        self.sum = self.sum.wrapping_sub(self.buffer[self.index]);
        self.buffer[self.index] = value;
        self.sum = self.sum.wrapping_add(value);
        self.index = (self.index + 1) % self.size;
    }

    pub(crate) fn get_average(&self) -> u64 {
        self.sum / self.size as u64
    }
}
