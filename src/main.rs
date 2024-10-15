#![feature(async_closure)]
#![allow(unused)]
#![feature(core_intrinsics)]
#![feature(let_chains)]
#![feature(trivial_bounds)]
#![feature(more_qualified_paths)]
#[cfg_attr(
    feature = "nightly-error-messages",
    rustc_on_unimplemented(
        message = "Cannot deserialize a value of the database type `{A}` as `{Self}`",
        note = "Double check your type mappings via the documentation of `{A}`"
    )
)]
extern crate core;

mod aggregators;
mod collectors;
mod config;
mod engine;
mod executors;
mod schema;
mod solana;
mod storage;
mod strategies;
mod tg_bot;
mod types;
mod utils;

use crate::config::constants::ENGINE_MESSAGE_CHANNEL_CAPACITY;
use crate::config::settings;
use crate::config::settings::Mode;
use crate::engine::Engine;
use crate::types::actions::SolanaAction;
use crate::types::events::BotEvent;
use ::solana_sdk::commitment_config::CommitmentConfig;
use anyhow::Result;
use base64::{engine::general_purpose, Engine as DecodeEngine};
use diesel::associations::HasTable;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use dotenv::dotenv;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_client::RpcClient;
use solana_farm_client::client::FarmClient;
use solana_sdk::signature::Signer;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::field::debug;
use tracing::log::trace;
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::{filter, prelude::*};
use types::events::BlockchainEvent;
use crate::types::engine::StrategyManager;
use crate::types::sniping_strategy::{NewSnipingStrategyInstance, SnipingStrategyInstance};


#[tokio::main]
async fn main() -> Result<()> {
    // Load settings from config.toml and check if it's valid, panics in case of error
    let context = config::app_context::AppContext::new("config").await;
    let settings = context.get_settings().await;
    info!("Starting the Solana bot with settings: {settings:?}");

    let solana_strat_manager =
        Arc::new(crate::strategies::SolanaStrategyManager::new(&context).await?);
    /// Initialize engine with message handlers.

    info!("Initializing engine..");
    let mut engine: Engine<BotEvent, Arc<Mutex<SolanaAction>>> = Engine::new(solana_strat_manager.clone())
        .with_event_channel_capacity(ENGINE_MESSAGE_CHANNEL_CAPACITY)
        .with_action_channel_capacity(ENGINE_MESSAGE_CHANNEL_CAPACITY);

    /// adding raydium pool collector getting new pools and prices
    let raydium_pool_collector =
        collectors::raydium_pool_update_collector::RaydiumPriceCollector::new(&context);
    engine.add_collector(Box::new(raydium_pool_collector));

    let pool_events_collector =
        collectors::realtime_feed_events_collector::RealtimeFeedEventsCollector::new(&context).await?;
    engine.add_collector(Box::new(pool_events_collector));
    /// adding ticks (default is 1s, configured in the settings)
    let tick_collector = collectors::heartbeat_collector::HeartbeatCollector::new(&context).await;
    engine.add_collector(Box::new(tick_collector));

    if let Some(_) = settings.collector.poll_node_for_tx_confirmations_ms {
        let tx_confirmation_collector =
            collectors::poll_tx_confirmation_collector::PollRpcForTxConfirmationsCollector::new(
                &context,
            );
        engine.add_collector(Box::new(tx_confirmation_collector));
    }
    /// adding aggregators - currently these are indicators, T-EMA, and T-RSI
    let tick_indicator_producer =
        aggregators::tick_indicators_aggregator::TickIndicatorsAggregator::new(&context).await;
    engine.add_aggregator(Box::new(tick_indicator_producer));

    match settings.engine.mode {
        Mode::Live => {
            let executor = executors::SolanaExecutor::new(&context).await;
            engine.add_executor(Arc::new(executor));
        }
        Mode::PaperTrading => {
            let executor = executors::PaperExecutor::new(&context).await;
            engine.add_executor(Arc::new(executor));
        }
        _ => {}
    }

    match context.bloxroute.start_fee_ws_stream().await {
        Ok(_) => info!("Started fee ws stream"),
        Err(_e) => warn!("Bloxroute optmial fee stream disabled"),
    }

    if context.tg_bot.is_some() {
        context.start_telegram_bot(solana_strat_manager.clone()).await;
    }

    /// Add startup strategies here
    if let Some(sniping_strategy_config) = settings.get_sniping_strategy_config() {
        let sniping_strategy_instance = storage::persistent::save_new_sniping_strategy_to_db(
            context.db_pool.clone(), NewSnipingStrategyInstance::try_from(&sniping_strategy_config).unwrap(),
        ).await?;
        let sniping_strategy = strategies::sniper_strategy::SniperStrategy::new(&context, &sniping_strategy_instance).await?;
        solana_strat_manager.start_strategy(Box::new(sniping_strategy)).await;
    }

    solana_strat_manager.start_strategy(Box::new(
        strategies::LoggerInterceptorStrategy::new(&context),
    )).await;
    /// Start engine.
    info!("Engine started");
    if let Ok(mut set) = engine.run().await {
        while let Some(res) = set.join_next().await {
            info!("res: {:?}", res);
        }
    }
    /// Profit!
    Ok(())
}
