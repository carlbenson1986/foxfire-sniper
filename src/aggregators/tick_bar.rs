// candle.rs

use std::collections::HashMap;
use std::ops::Add;
use serde_derive::Serialize;
use yata::core::{PeriodType, ValueType};
use yata::prelude::Candle;

#[derive(Debug, Clone, Default, Serialize)]
pub struct TickBar {
    candle: Candle,
    tick_count: PeriodType,
    forming_period: PeriodType,
}

impl TickBar {
    pub fn new(period: PeriodType, price: &ValueType, volume: &ValueType) -> Self {
        Self {
            forming_period: period,
            ..Default::default()
        }
    }

    fn update_with_tick(&mut self, price: &ValueType, volume: &ValueType) {
        self.candle.add((*price, *price, *price, *price, *volume));
        self.tick_count += 1;
    }

    fn is_complete(&self) -> bool {
        self.tick_count >= self.forming_period
    }

    fn reset(&mut self, period: u32) {
        *self = Self {
            forming_period: period,
            ..Default::default()
        };
    }

    pub fn get_candle(&self) -> Candle {
        self.candle
    }

    pub(crate) fn next(&mut self, price: &ValueType, volume: &ValueType) -> Option<Candle> {
        self.update_with_tick(price, volume);
        if self.is_complete() {
            let candle = self.get_candle();
            self.reset(self.forming_period);
            Some(candle)
        } else {
            None
        }
    }
}
