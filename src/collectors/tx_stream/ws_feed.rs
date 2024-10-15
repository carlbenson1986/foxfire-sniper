use futures_util::StreamExt;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{
    RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter,
};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::UiTransactionEncoding;
use std::str::FromStr;
use std::sync::Arc;

use crate::collectors::tx_stream::types::TransactionPretty;
use crate::config::constants::WS_FEED_COMMITMENT_LEVEL;
use crate::config::settings::ProviderName;
use crate::solana::rpc_pool::RpcClientPool;
use crate::solana::ws_pool::WsNamedClient;
use crate::storage;
use tokio::sync::{mpsc, Mutex};
use tracing::field::debug;
use tracing::warn;
use tracing::{debug, error};
use yellowstone_grpc_client::{GeyserGrpcClient, InterceptorXToken};

pub async fn ws_feed(
    ws_client: WsNamedClient,
    rpc_client_pool: RpcClientPool,
    transmitter: mpsc::UnboundedSender<TransactionPretty>,
) {
    let (mut stream, _) = match ws_client
        .1
        .logs_subscribe(
            RpcTransactionLogsFilter::All,
            RpcTransactionLogsConfig {
                commitment: Some(CommitmentConfig {
                    commitment: WS_FEED_COMMITMENT_LEVEL,
                }),
            },
        )
        .await
    {
        Ok(stream) => stream,
        Err(e) => {
            panic!("Failed to subscribe to ws logs: {:?}", e);
        }
    };
    while let Some(logs) = stream.next().await {
        let transmitter = transmitter.clone();
        let rpc_client_pool_clone = rpc_client_pool.clone();
        let signature_str = logs.value.signature.clone();
        let signature = Signature::from_str(&signature_str).unwrap();
        debug!(
            "New pool tx detected, signature: {:?}, querying",
            signature_str
        );
        if let Ok(tx) = rpc_client_pool_clone
            .get_transaction_with_config(&signature)
            .await
        {
            transmitter.send(TransactionPretty::from(tx)).unwrap()
        }
    }
}
