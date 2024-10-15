use async_trait::async_trait;
use diesel::internal::derives::multiconnection::chrono::Utc;
use futures::Stream;
use futures_util::{SinkExt, StreamExt};
use log::trace;
use maplit::hashmap;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{
    RpcBlockSubscribeConfig, RpcBlockSubscribeFilter, RpcTransactionConfig,
    RpcTransactionLogsConfig, RpcTransactionLogsFilter,
};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::{Transaction, VersionedTransaction};
use solana_transaction_status::{EncodedTransactionWithStatusMeta};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use yellowstone_grpc_client::{
    GeyserGrpcClient, GeyserGrpcClientError, Interceptor, InterceptorXToken,
};
use yellowstone_grpc_proto::prelude::{
    geyser_client::GeyserClient, CommitmentLevel, GetBlockHeightRequest, GetBlockHeightResponse,
    GetLatestBlockhashRequest, GetLatestBlockhashResponse, GetSlotRequest, GetSlotResponse,
    GetVersionRequest, GetVersionResponse, IsBlockhashValidRequest, IsBlockhashValidResponse,
    PingRequest, PongResponse, SubscribeRequest, SubscribeRequestFilterAccounts,
    SubscribeRequestFilterBlocks, SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterSlots,
    SubscribeRequestFilterTransactions, SubscribeRequestPing, SubscribeUpdate,
    SubscribeUpdateTransaction,
};

use crate::collectors::tx_stream::types::{AccountPretty, GeyserFeedEvent, TransactionPretty};
use crate::config::app_context::AppContext;
use crate::config::settings::ProviderName;
use crate::solana::geyser_pool::GeyserNamedClient;
use crate::storage;
use crate::storage::cache::RedisPool;
use crate::types::engine::{Collector, EventStream};
use crate::types::events::BlockchainEvent;
use tokio::sync::{mpsc, watch, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::warn;
use tracing::{debug, error, info, instrument};
use yellowstone_grpc_proto::geyser::SubscribeRequestAccountsDataSlice;
use yellowstone_grpc_proto::prelude::subscribe_update::UpdateOneof;
use yellowstone_grpc_proto::prost::bytes::Bytes;


// Helper function to get updated account filters
async fn get_account_filters(context: &AppContext) -> HashMap<String, SubscribeRequestFilterAccounts> {
    let accounts: Vec<String> = context
        .cache
        .get_accounts()
        .await
        .iter()
        .map(|u| u.to_string())
        .collect();

    if !accounts.is_empty() {
        hashmap! {
            "client".to_owned() => SubscribeRequestFilterAccounts {
                account: accounts,
                owner: vec![],
                filters: vec![],
            },
        }
    } else {
        HashMap::default()
    }
}


pub async fn geyser_feed(
    context: AppContext,
    geyser_client: Arc<Mutex<GeyserGrpcClient<InterceptorXToken>>>,
    transmitter: mpsc::UnboundedSender<GeyserFeedEvent>,
) {
    let mut average_delay = 0.0;
    let mut number_of_blocks = 0;
    let mut last_block_time = 0;

    let account_tx = context.geyser_resubscribe_account_tx_notify.clone();
    let mut account_rx = account_tx.subscribe();


    let blocks = hashmap! {
        "client".to_owned() => SubscribeRequestFilterBlocks::default()
    };

    let _blocks_meta = hashmap! {
        "client".to_owned() => SubscribeRequestFilterBlocksMeta {},
    };

    let transactions = hashmap! {
        "client".to_owned() => SubscribeRequestFilterTransactions {
            vote: Some(false),
            ..Default::default()
        }
    };

    let transactions_status = hashmap! {
            "".to_owned() => SubscribeRequestFilterTransactions {
                vote: Some(false),
                ..Default::default()
            } 
    };

    let mut users_count = context.cache.get_accounts_count().await;
    let user_accounts: Vec<String> = context
        .cache
        .get_accounts()
        .await
        .iter()
        .map(|u| u.to_string())
        .collect();

    let accounts = if user_accounts.len() > 0 {
        hashmap! { "client".to_owned() => SubscribeRequestFilterAccounts {
            account: user_accounts,
            owner: vec ! [],
            filters: vec ! [],
            },
        }
    } else {
        HashMap::default()
    };

    info!("starting_stream");
    let blocks_clone = blocks.clone();
    let (mut subscribe_tx, mut stream) = geyser_client
        .lock()
        .await
        .subscribe_with_request(Some(SubscribeRequest {
            accounts,
            slots: Default::default(),
            transactions: transactions.clone(),
            transactions_status: transactions_status.clone(),
            blocks: Default::default(),
            blocks_meta: HashMap::default(),
            entry: Default::default(),
            commitment: None,
            accounts_data_slice: vec![],
            ping: None,
        }))
        .await
        .unwrap();


    let subscribe_tx = Arc::new(Mutex::new(subscribe_tx));

    let subscribe_tx_clone = Arc::clone(&subscribe_tx);
    // Watch for changes in the accounts and update the subscription immediately
    let mut cached_accounts = context.cache.get_accounts().await;
    cached_accounts.sort();
    tokio::spawn(async move {
        while account_rx.changed().await.is_ok() {
            let mut accs = context.cache.get_accounts().await;
            accs.sort();
            if accs == cached_accounts {
                trace!("Accounts have not changed, skipping subscription update, monitoring: {:?}", accs);
                continue;
            }
            trace!("Accounts have changed, updating subscription, monitoring: {:?}", accs);
            cached_accounts = accs;
            let transactions = transactions.clone();
            let transactions_status = transactions_status.clone();
            // Dynamically send the updated subscription request when accounts change
            let updated_accounts = get_account_filters(&context).await;
            subscribe_tx_clone.lock().await
                .send(SubscribeRequest {
                    accounts: updated_accounts,
                    slots: Default::default(),
                    transactions,
                    transactions_status,
                    blocks: Default::default(),
                    ..Default::default()
                })
                .await
                .map_err(GeyserGrpcClientError::SubscribeSendError)
                .unwrap();
        }
    });


    while let Some(message) = stream.next().await {
        match message {
            Ok(msg) => {
                match msg.update_oneof {
                    Some(UpdateOneof::Block(block)) => {
                        let current_time = Utc::now().timestamp_millis();
                        let block_time = block.block_time.unwrap();
                        number_of_blocks += 1;
                        let delay_ms = current_time - block_time.timestamp * 1000;
                        average_delay = (average_delay * (number_of_blocks - 1) as f64
                            + delay_ms as f64)
                            / number_of_blocks as f64;
                        trace!(
                            "Block: {:?}, Block Time: {}, delay: {}, average: {}",
                            block.block_height.unwrap(),
                            block_time.timestamp,
                            delay_ms,
                            average_delay
                            );
                        continue;
                    }

                    Some(UpdateOneof::Account(account)) => {
                        trace!(
                                "new account update: filters {:?}, account: {:?}",
                                msg.filters,
                                account
                                );
                        let acc: AccountPretty = account.into();
                        transmitter.send(GeyserFeedEvent::Account(acc)).unwrap();

                        continue;
                    }
                    Some(UpdateOneof::Transaction(tx)) => {
                        // info!(
                        //     "new transaction update: filters {:?}, transaction: {:#?}",
                        //     msg.filters, tx
                        // );
                        transmitter.send(GeyserFeedEvent::Transaction(tx)).unwrap();
                        continue;
                    }
                    Some(UpdateOneof::TransactionStatus(status)) => {
                        // info!(
                        //     "new transaction update: filters {:?}, transaction status: {:?}",
                        //     msg.filters, status
                        // );
                        transmitter.send(GeyserFeedEvent::TxStatusUpdate(status)).unwrap();
                        continue;
                    }
                    Some(UpdateOneof::Ping(_)) => {
                        // This is necessary to keep load balancers that expect client pings alive. If your load balancer doesn't
                        // require periodic client pings then this is unnecessary
                        subscribe_tx.lock().await
                            .send(SubscribeRequest {
                                ping: Some(SubscribeRequestPing { id: 1 }),
                                ..Default::default()
                            })
                            .await
                            .unwrap();
                    }
                    _ => {}
                }
                // info!("new message: {msg:?}")
            }
            Err(error) => {
                error!("error: {error:?}");
                break;
            }
        }
    }
    info!("stream closed");
}
