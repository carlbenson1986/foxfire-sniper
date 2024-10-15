use crate::collectors::tx_stream::blockchain_stream;
use crate::collectors::tx_stream::types::GeyserFeedEvent::{Account, Transaction, TxStatusUpdate};
use crate::collectors::tx_stream::types::{AccountPretty, GeyserFeedEvent, TransactionPretty};
use crate::config::app_context::AppContext;
use crate::config::settings::Mode;
use crate::solana::constants;
use crate::solana::rpc_pool::RpcClientPool;
use crate::solana::tx_parser::{is_tx_a_sol_transfer, is_tx_a_token_transfer, parse_tx_for_set_compute_unit_price, parse_tx_for_swaps};
use crate::storage::cache::RedisPool;
use crate::storage::persistent::DbPool;
use crate::types::engine::{Collector, EventStream};
use crate::types::events::{BlockchainEvent, BlockchainEvent::{AccountUpdate, Deposit, Withdrawal}, BotEvent, ExecutionReceipt};
use crate::types::pool::{RaydiumPool, RaydiumPoolPriceUpdate, RaydiumSwapEvent, TradeDirection};
use crate::utils::decimals;
use crate::{solana, storage, utils};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use futures::ready;
use futures_util::stream;
use futures_util::StreamExt;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_client::RpcClient;
use solana_farm_client::client::FarmClient;
use solana_sdk::pubkey::Pubkey;
use solana_transaction_status::{
    EncodedTransaction, UiInstruction, UiMessage, UiParsedInstruction, UiParsedMessage,
    UiPartiallyDecodedInstruction,
};
use std::collections::{HashMap, HashSet};
use std::future::ready;
use std::hash::Hash;
use std::sync::Arc;
use solana_transaction_status::option_serializer::OptionSerializer;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{debug, error, info, trace, warn};
use url::quirks::hash;
use yellowstone_grpc_proto::prelude::{SubscribeUpdateTransaction, SubscribeUpdateTransactionStatus};
use crate::config::constants::{BALANCE_CHANGE_THRESHOLD_SOL, BASE_TX_FEE_SOL};
use crate::solana::pool::extract_pool_from_init_tx;

/// A collector that listens to raydium pool events logs based on a [Filter](Filter),
/// and generates a stream of [events](Log).
pub struct RealtimeFeedEventsCollector {
    pub(crate) context: AppContext,
}

impl RealtimeFeedEventsCollector {
    pub async fn new(context: &AppContext) -> Result<Self> {
        Ok(Self {
            context: context.clone(),
        })
    }

    pub async fn parse_cached_executions(&self, tx: &TransactionPretty) -> Option<BotEvent> {
        match self.context.get_settings().await.engine.mode {
            Mode::Live => {
                match self
                    .context
                    .cache
                    .get_uuid_by_signature(&tx.signature.to_string())
                    .await
                {
                    Some(swap_uuid) => {
                        trace!(
                            "Marking swap {} as seen onchain with signature {}",
                            swap_uuid, tx.signature
                        );
                        self.context
                            .cache
                            .mark_signature_as_processed(swap_uuid, tx.signature.to_string())
                            .await;
                        Some(BotEvent::BlockchainEvent(
                            BlockchainEvent::ExecutionReceipt(ExecutionReceipt {
                                action_uuid: swap_uuid.clone(),
                                transaction_signature: tx.signature,
                                err: match &tx.tx.meta {
                                    Some(meta) => meta.err.clone(),
                                    None => None,
                                },
                                status_changed_at: Utc::now(),
                            }),
                        ))
                    }
                    None => None,
                }
            }
            Mode::PaperTrading => {
                if let Some(pop) = self.context.cache.pop_front().await {
                    let (swap_uuid, signature) = pop;
                    Some(BotEvent::BlockchainEvent(
                        BlockchainEvent::ExecutionReceipt(ExecutionReceipt {
                            action_uuid: swap_uuid.clone(),
                            transaction_signature: Default::default(),
                            err: None,
                            status_changed_at: Utc::now(),
                        }),
                    ))
                } else {
                    None
                }
            }
            Mode::BackTesting => {
                panic!("Not supported for this collector")
            }
        }
    }

    pub async fn parse_status_upate_transaction(&self, tx_status: SubscribeUpdateTransactionStatus) -> Option<Vec<BotEvent>> {
        None
    }

    pub async fn parse_transaction(&self, tx_update: SubscribeUpdateTransaction) -> Option<Vec<BotEvent>> {
        let tx: TransactionPretty = tx_update.clone().into();
        //failing early
        if tx.is_vote {
            return None;
        }
        let mut events: Vec<BotEvent> = Vec::new();
        //one routine to parse all the possible event, high-load since all solana transactions are getting here

        // 1. confirming sent transactions
        if let Some(bot_tx_confirmation_event) = self.parse_cached_executions(&tx).await {
            events.push(bot_tx_confirmation_event);
        }

        if let Some(tx_meta) = &tx.tx.meta && tx.is_successful() {
            if let OptionSerializer::Some(cu_consumed) = tx_meta.compute_units_consumed {
                if cu_consumed > 0 && tx_meta.fee > BASE_TX_FEE_SOL {
                    self.context.cache.update_optimal_fee((tx_meta.fee - BASE_TX_FEE_SOL) / cu_consumed).await;
                }
            }
        }

        // 2. parse swap events for monitored pools
        if let Some(parsed_swaps) = parse_tx_for_swaps(&tx.tx) {
            //2. Updating fee to get the most recent fees for swaps
            for parsed_swap in parsed_swaps {
                //3. check if this is our pool swap
                if let Some(client_pool) = self
                    .context
                    .cache
                    .target_pools
                    .read()
                    .await
                    .get(&parsed_swap.pool_id)
                {
                    let mut prices_mutex_guard =
                        self.context.cache.target_pools_prices.lock().await;
                    let pre_swap_pool_state = prices_mutex_guard.get_mut(&parsed_swap.pool_id)?;
                    let base_amount_ui = decimals::tokens_to_ui_amount_with_decimals_f64(
                        parsed_swap.base_amount,
                        client_pool.base_decimals,
                    );
                    let quote_amount_ui = decimals::tokens_to_ui_amount_with_decimals_f64(
                        parsed_swap.quote_amount,
                        client_pool.quote_decimals,
                    );
                    let mut updated_base_reserve_ui = match parsed_swap.trade_direction {
                        TradeDirection::Buy => pre_swap_pool_state.base_reserve - base_amount_ui,
                        TradeDirection::Sell => pre_swap_pool_state.base_reserve + base_amount_ui,
                    };
                    let mut updated_quote_reserve_ui = match parsed_swap.trade_direction {
                        TradeDirection::Buy => pre_swap_pool_state.quote_reserve + quote_amount_ui,
                        TradeDirection::Sell => pre_swap_pool_state.quote_reserve - quote_amount_ui,
                    };
                    let price_update = RaydiumPoolPriceUpdate {
                        pool: client_pool.id,
                        price: updated_quote_reserve_ui / updated_base_reserve_ui,
                        base_reserve: updated_base_reserve_ui,
                        quote_reserve: updated_quote_reserve_ui,
                        created_at: chrono::Utc::now().naive_utc(),
                    };
                    pre_swap_pool_state.base_reserve = updated_base_reserve_ui;
                    pre_swap_pool_state.quote_reserve = updated_quote_reserve_ui;
                    drop(prices_mutex_guard);
                    // can happen due to the price convertion, theorietically, still can't afford panic
                    if updated_base_reserve_ui < 0.0 {
                        error!(
    "{} base reserve is negative {}, pool is dead",
    client_pool.id, updated_base_reserve_ui
    );
                        return None;
                    }
                    if updated_quote_reserve_ui < 0.0 {
                        error!(
    "{} quote reserve is negative {}, pool is dead",
    client_pool.id, updated_quote_reserve_ui
    );
                        return None;
                    }
                    info!("Pool update wih Geyser: {:?}", price_update);
                    events.push(BotEvent::BlockchainEvent(
                        BlockchainEvent::RaydiumSwapEvent(RaydiumSwapEvent {
                            price_update,
                            signature: tx.signature,
                            pool: client_pool.id,
                            trade_direction: parsed_swap.trade_direction,
                            base_amount: base_amount_ui,
                            quote_amount: quote_amount_ui,
                            price: quote_amount_ui / base_amount_ui,
                            volume: quote_amount_ui,
                            created_at: chrono::Utc::now(),
                        }),
                    ));
                }
            }
        }

        // 3. parse new pool creation
        if let Some((new_pool, price_update)) = extract_pool_from_init_tx(&tx_update) {
            let mut tokens = self.context.cache.target_tokens.lock().await;
            if !tokens.contains(&new_pool.base_mint) {
                tokens.put(new_pool.base_mint, new_pool.id);
                events.push(BotEvent::BlockchainEvent(BlockchainEvent::RaydiumNewPoolEvent(new_pool, price_update)));
            } else {
                warn!("Double deployment, skipping, pool: {:?}", new_pool);
            }
        }
        Some(events).filter(|events| !events.is_empty())
    }

    pub async fn parse_account(&self, acc: AccountPretty) -> Option<Vec<BotEvent>> {
        let prev_balance = self.context.cache.get_account(&acc.pubkey).await.flatten().map(|a| a.lamports).unwrap_or(0);
        self.context.cache.update_account(acc.pubkey, Some(acc.clone())).await;
        let mut events = vec![BotEvent::BlockchainEvent(AccountUpdate(acc.clone()))];
        // checking deposit - incoming is not from the trading wallets
        if !self.context.cache.if_agent_signature(&acc.txn_signature).await {
            if acc.lamports > prev_balance && acc.lamports - prev_balance > BALANCE_CHANGE_THRESHOLD_SOL {
                events.push(BotEvent::BlockchainEvent(Deposit(
                    acc.txn_signature,
                    acc.pubkey,
                    acc.lamports - prev_balance,
                )));
            } else if acc.lamports < prev_balance && prev_balance - acc.lamports > BALANCE_CHANGE_THRESHOLD_SOL {
                events.push(BotEvent::BlockchainEvent(Withdrawal(
                    acc.txn_signature,
                    acc.pubkey,
                    prev_balance - acc.lamports,
                )));
            }
        }
        Some(events)
    }
}

/// Implementation of the [Collector](Collector) trait for the [LogCollector](LogCollector).
/// This implementation uses the [PubsubClient](PubsubClient) to subscribe to new logs.
#[async_trait]
impl Collector<BotEvent> for RealtimeFeedEventsCollector {
    async fn get_event_stream(&self) -> Result<EventStream<'_, BotEvent>> {
        info!("Initializing PoolEventsCollector event stream");
        let geyser_stream = blockchain_stream(&self.context).await?;
        let event_stream = geyser_stream
            .filter_map(move |e| {
                let e_clone = e.clone();
                async move {
                    match e_clone {
                        Transaction(tx_update) => self.parse_transaction(tx_update).await,
                        Account(acc) => self.parse_account(acc).await,
                        TxStatusUpdate(tx_status) => self.parse_status_upate_transaction(tx_status).await,
                    }
                }
            })
            .flat_map(futures::stream::iter)
            .map(|event| {
                debug!("Event: {:?}", event);
                event
            });

        Ok(Box::pin(event_stream))
    }
}
