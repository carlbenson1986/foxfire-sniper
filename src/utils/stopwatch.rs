use std::time::Duration;
use tokio::time::Instant;
use crate::types::events::TickSizeMs;

#[derive(Debug, Clone)]
pub struct Stopwatch {
    lap_ticks: u64,
    start: Instant,
}

impl Default for Stopwatch {
    fn default() -> Self {
        Stopwatch {
            start: Instant::now(),
            lap_ticks: 0,
        }
    }
}

impl Stopwatch {
    pub fn new(lap_ticks: u64) -> Self {
        Stopwatch {
            start: Instant::now(),
            lap_ticks,
        }
    }

    pub fn start(&mut self, lap_ticks: u64) {
        self.lap_ticks = lap_ticks;
        self.start = Instant::now();
    }

    pub fn turn_off(&mut self) {
        self.lap_ticks = 0;
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub fn ticks_elapsed(&self, tick_size_ms: TickSizeMs) -> u64 {
        self.elapsed().as_millis() as u64 / tick_size_ms
    }

    pub fn is_time_elapsed(&self, tick_size_ms: TickSizeMs) -> bool {
        self.lap_ticks > 0 && self.ticks_elapsed(tick_size_ms) >= self.lap_ticks
    }

    pub fn ticks_left(&self, tick_size_ms: TickSizeMs) -> i64 {
        self.lap_ticks as i64 - self.ticks_elapsed(tick_size_ms) as i64
    }
}
