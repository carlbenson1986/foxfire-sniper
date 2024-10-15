use crate::aggregators::every_tick_indicators::EveryTickIndicators;
use crate::aggregators::tick_bar::TickBar;
use redis::Value;
use std::collections::HashMap;
use std::sync::Arc;
use serde_derive::Serialize;
use yata::core::{Candle, IndicatorResult, PeriodType, ValueType};
use yata::helpers::MA;
use yata::indicators::{BollingerBands, RelativeStrengthIndex};
use yata::methods::TEMA;
use yata::prelude::Method;

#[derive(Debug, Clone, Serialize)]
pub struct TickBarValue {
    pub period: PeriodType,
    pub t_bar: Candle,
}

#[derive(Debug, Clone, Serialize)]
pub struct TickBarWithPeriod {
    pub period: PeriodType,
    pub t_bar: TickBar,
}

impl TickBarWithPeriod {
    pub fn new(period: PeriodType, price: &ValueType, volume: &ValueType) -> Self {
        Self {
            period,
            t_bar: TickBar::new(period, price, volume),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TickBarsCache {
    bar_sizes: Vec<PeriodType>,
    tick_bars: HashMap<PeriodType, TickBarWithPeriod>,
}

impl TickBarsCache {
    pub fn new(bar_sizes: &[PeriodType]) -> Self {
        Self {
            bar_sizes: Vec::from(bar_sizes.clone()),
            tick_bars: HashMap::new(),
        }
    }

    pub fn next(&mut self, price: &ValueType, volume: &ValueType) -> Vec<TickBarValue> {
        let mut indicator_events = vec![];
        for period in self.bar_sizes.iter() {
            let period = *period;
            let mut indicators = self.tick_bars.get_mut(&period);
            if indicators.is_none() {
                self.tick_bars.insert(
                    period,
                    TickBarWithPeriod {
                        period,
                        t_bar: TickBar::new(period, price, volume),
                    },
                );
            } else {
                let mut indicators = indicators.unwrap();
                if let Some(candle) = indicators.t_bar.next(&price, &volume) {
                    indicator_events.push(TickBarValue {
                        period,
                        t_bar: indicators.t_bar.get_candle(),
                    });
                }
            }
        }
        indicator_events
    }
}
