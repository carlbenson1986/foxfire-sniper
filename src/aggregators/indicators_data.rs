use crate::aggregators::every_tick_indicators::{
    EveryTickIndicatorsCache, EveryTickIndicatorsValue,
};
use crate::aggregators::period_indicators::{TickBarValue, TickBarsCache};
use yata::core::{PeriodType, ValueType};

pub struct IndicatorsData {
    every_tick_indicators: EveryTickIndicatorsCache,
    tick_bars: TickBarsCache,
}

impl IndicatorsData {
    pub fn new(bar_sizes: &[PeriodType], lengths: &[PeriodType]) -> Self {
        Self {
            every_tick_indicators: EveryTickIndicatorsCache::new(lengths),
            tick_bars: TickBarsCache::new(bar_sizes),
        }
    }

    pub fn next_tick(
        &mut self,
        price: &ValueType,
        volume: &ValueType,
    ) -> Vec<EveryTickIndicatorsValue> {
        self.every_tick_indicators.next(price, volume)
    }

    pub fn next_bars(&mut self, price: &ValueType, volume: &ValueType) -> Vec<TickBarValue> {
        self.tick_bars.next(price, volume)
    }
}
