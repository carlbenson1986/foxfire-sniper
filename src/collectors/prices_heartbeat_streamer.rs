use crate::config::app_context::AppContext;
use crate::storage::cache::RedisPool;
use crate::storage::persistent::DbPool;
use crate::types::events::{BlockchainEvent, BotEvent};
use crate::{solana, storage, utils};
use anyhow::Result;
use diesel::internal::derives::multiconnection::chrono::Utc;
use log::trace;
use redis::Commands;
use solana_client::rpc_client::RpcClient;
use solana_farm_client::client::FarmClient;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task;
use tracing::{debug, info};

pub async fn start_prices_stream(
    context: &AppContext,
    transmitter: mpsc::UnboundedSender<Result<BotEvent>>,
) {
    let solana_farm_client = FarmClient::new(&context.rpc_pool.get_a_client().unwrap().url());
    let db = context.db_pool.clone();
    let frequency = context
        .settings
        .read()
        .await
        .collector
        .raydium_pool_polling_interval_ms;
    let context = context.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(frequency));
        loop {
            interval.tick().await;
            let trading_pairs = context
                .cache
                .target_pools
                .read()
                .await
                .iter()
                .map(|(_, pool)| pool.id)
                .collect::<Vec<Pubkey>>();
            for pool in trading_pairs.iter() {
                match context.rpc_pool.get_pool_price(&pool).await {
                    Ok(pool_price_update) => {
                        let db = db.clone();
                        let _ = transmitter.send(Ok(BotEvent::BlockchainEvent(
                            BlockchainEvent::RaydiumHeartbeatPriceUpdate(pool_price_update.clone()),
                        )));
                        context
                            .cache
                            .target_pools_prices
                            .lock()
                            .await
                            .insert(pool_price_update.pool, pool_price_update.clone());
                        trace!("Price update using node polling: {:#?}", pool_price_update);
                    }
                    Err(e) => {
                        continue;
                    }
                };
            }
        }
    });
}
