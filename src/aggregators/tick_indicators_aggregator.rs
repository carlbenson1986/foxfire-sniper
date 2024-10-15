use crate::aggregators::every_tick_indicators::EveryTickIndicatorsCache;
use crate::aggregators::indicators_data::IndicatorsData;
use crate::aggregators::period_indicators::TickBarsCache;
use crate::aggregators::tick_bar::TickBar;
use crate::config::app_context::AppContext;
use crate::config::settings::AggregatorConfig;
use crate::types::engine::Aggregator;
use crate::types::events::{BarEvent, BlockchainEvent, BotEvent, DerivedEvent, ExecutionReceipt};
use crate::types::pool::RaydiumSwapEvent;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{debug, error, info};
use yata::core::Candle;
use yata::core::{Method, PeriodType, ValueType};
use yata::helpers::{RandomCandles, MA};
use yata::indicators::{AwesomeOscillator, BollingerBands, RelativeStrengthIndex, MACD, RSI};
use yata::methods::{EMA, TEMA, VWMA};
use yata::prelude::*;

pub struct TickIndicatorsAggregator {
    context: AppContext,
    config: AggregatorConfig,
    indicators: HashMap<Pubkey, IndicatorsData>,
}

impl TickIndicatorsAggregator {
    pub async fn new(context: &AppContext) -> Self {
        info!("Initializing TickIndicatorsAggregator");
        let config = context.settings.read().await.aggregator.clone();
        Self {
            context: context.clone(),
            config,
            indicators: HashMap::new(),
        }
    }
}

impl Aggregator<BotEvent> for TickIndicatorsAggregator {
    fn aggregate_event(&mut self, event: BotEvent) -> Vec<BotEvent> {
        let mut derived_events = Vec::new();
        match event {
            BotEvent::BlockchainEvent(BlockchainEvent::RaydiumSwapEvent(swap_event)) => {
                match self.indicators.get_mut(&swap_event.pool) {
                    Some(indicators_data) => {
                        indicators_data
                            .next_tick(&swap_event.price, &swap_event.volume)
                            .iter()
                            .for_each(|tick_indicator| {
                                derived_events.push(BotEvent::DerivedEvent(
                                    DerivedEvent::TickIndicatorEvent(
                                        swap_event.pool,
                                        tick_indicator.clone(),
                                    ),
                                ));
                            });

                        indicators_data
                            .next_bars(&swap_event.price, &swap_event.volume)
                            .iter()
                            .for_each(|period_indicator| {
                                derived_events.push(BotEvent::DerivedEvent(
                                    DerivedEvent::TickBarEvent(
                                        swap_event.pool,
                                        period_indicator.clone(),
                                    ),
                                ));
                            });
                    }
                    None => {
                        let mut indicators_data = IndicatorsData::new(
                            &self.config.indicator_periods_in_ticks,
                            &self.config.tick_bar_sizes,
                        );
                        self.indicators
                            .insert(swap_event.pool.clone(), indicators_data);
                    }
                }
            }
            _ => {}
        }
        derived_events
    }
}
