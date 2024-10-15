use crate::aggregators::period_indicators::TickBarWithPeriod;
use crate::aggregators::tick_bar::TickBar;
use redis::Value;
use std::collections::HashMap;
use std::sync::Arc;
use serde_derive::Serialize;
use yata::core::{IndicatorInstanceDyn, IndicatorResult, PeriodType, Source, ValueType};
use yata::helpers::MA;
use yata::indicators::{
    BollingerBands, BollingerBandsInstance, RelativeStrengthIndex, RelativeStrengthIndexInstance,
    MACD, RSI,
};
use yata::methods::{EMA, TEMA, TR};
use yata::prelude::{Candle, IndicatorConfig, Method};

#[derive(Debug, Clone, Serialize)]
pub struct EveryTickIndicatorsValue {
    pub t_tema: ValueType,
    pub t_ema: ValueType,
    pub t_rsi: IndicatorResult,
    pub t_bollinger: IndicatorResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct EveryTickIndicators {
    pub t_tema: TEMA,
    pub t_ema: EMA,
    pub t_rsi: RelativeStrengthIndexInstance,
    pub t_bollinger: BollingerBandsInstance,
}

#[derive(Debug, Clone, Serialize)]
pub struct EveryTickIndicatorsCache {
    lengths: Vec<PeriodType>,
    every_tick_indicators: HashMap<PeriodType, EveryTickIndicators>,
}

impl EveryTickIndicators {
    pub fn new(length: &PeriodType, price: &ValueType, volume: &ValueType) -> Self {
        let first_candle = Candle::from(&(*price, *price, *price, *price, *volume));
        Self {
            t_tema: TEMA::new(*length, price).unwrap(),
            t_ema: EMA::new(*length, price).unwrap(),
            t_rsi: RelativeStrengthIndex {
                ma: MA::RMA(*length),
                zone: 0.3,
                source: Source::Close,
            }
            .init(&first_candle)
            .unwrap(),
            t_bollinger: BollingerBands {
                avg_size: *length,
                sigma: 2.0,
                source: Source::Close,
            }
            .init(&first_candle)
            .unwrap(),
        }
    }

    pub fn next(&mut self, price: &ValueType, volume: &ValueType) -> EveryTickIndicatorsValue {
        let next_tick_candle = Candle::from(&(*price, *price, *price, *price, *volume));
        EveryTickIndicatorsValue {
            t_tema: self.t_tema.next(price),
            t_ema: self.t_ema.next(price),
            t_rsi: self.t_rsi.next(&next_tick_candle),
            t_bollinger: self.t_bollinger.next(&next_tick_candle),
        }
    }
}

impl EveryTickIndicatorsCache {
    pub fn new(lengths: &[PeriodType]) -> Self {
        Self {
            lengths: Vec::from(lengths),
            every_tick_indicators: HashMap::new(),
        }
    }

    pub fn next(&mut self, price: &ValueType, volume: &ValueType) -> Vec<EveryTickIndicatorsValue> {
        let mut indicator_events = vec![];
        for length in self.lengths.iter() {
            let mut indicators = self.every_tick_indicators.get_mut(length);
            if indicators.is_none() {
                self.every_tick_indicators
                    .insert(*length, EveryTickIndicators::new(length, &price, &volume));
            } else {
                //todo aggregate tick bars
                let mut indicators = indicators.unwrap();
                indicator_events.push(indicators.next(&price, &volume));
            }
        }
        indicator_events
    }
}
