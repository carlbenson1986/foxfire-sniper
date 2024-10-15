use crate::collectors::prices_heartbeat_streamer::start_prices_stream;
use crate::config::app_context::AppContext;
use crate::solana::rpc_pool::RpcClientPool;
use crate::types::engine::{Collector, EventStream};
use crate::types::events::{BlockchainEvent, BotEvent, ExecutionReceipt};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_client::RpcClient;
use solana_farm_client::client::FarmClient;
use solana_sdk::signature::Signature;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;
use tracing::{error, info};

/// A collector that listens to raydium pool events logs based on a [Filter](Filter),
/// and generates a stream of [events](Log).
pub struct PollRpcForTxConfirmationsCollector {
    pub(crate) context: AppContext,
}

impl PollRpcForTxConfirmationsCollector {
    pub fn new(context: &AppContext) -> Self {
        Self {
            context: context.clone(),
        }
    }
}

/// Implementation of the [Collector](Collector) trait for the [LogCollector](LogCollector).
/// This implementation uses the [PubsubClient](PubsubClient) to subscribe to new logs.
#[async_trait]
impl Collector<BotEvent> for PollRpcForTxConfirmationsCollector {
    async fn get_event_stream(&self) -> Result<EventStream<'_, BotEvent>> {
        let (tx, rx) = mpsc::unbounded_channel();

        let solana_farm_client =
            FarmClient::new(&self.context.rpc_pool.get_a_client().unwrap().url());
        let frequency = self
            .context
            .settings
            .read()
            .await
            .collector
            .poll_node_for_tx_confirmations_ms
            .ok_or(anyhow!("Polling frequency not set"))?;
        let context = self.context.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(frequency));
            loop {
                interval.tick().await;
                let signatures_to_poll = context.cache.get_all_unprocessed_tx_signatures().await;
                for signature_str in &signatures_to_poll {
                    let signature = Signature::from_str(&signature_str).unwrap();
                    match context.rpc_pool.get_signature_status(&signature).await {
                        Ok(status) => {
                            if status.is_some() {
                                let uuid = context
                                    .cache
                                    .get_uuid_by_signature(&signature.to_string())
                                    .await
                                    .unwrap();
                                if !context.cache.is_signature_processed(&signature_str).await {
                                    context
                                        .cache
                                        .mark_signature_as_processed(uuid, signature_str.clone())
                                        .await;
                                    tx.send(BotEvent::BlockchainEvent(
                                        BlockchainEvent::ExecutionReceipt(ExecutionReceipt {
                                            action_uuid: uuid,
                                            transaction_signature: signature,
                                            err: match &status.unwrap() {
                                                Ok(_) => {
                                                    info!("Tx confirmed {:?} ", &signature_str);
                                                    None
                                                }
                                                Err(err) => {
                                                    error!(
                                                    "Tx failed {:?} error: {:?}",
                                                    &signature_str, &err
                                                );
                                                    Some(err.clone())
                                                }
                                            },
                                            status_changed_at: Utc::now(),
                                        }),
                                    ));
                                }
                            }
                        }
                        Err(err) => {
                            error!("Error polling for tx confirmation: {:?}", err);
                        }
                    }
                }
            }
        });

        let stream = UnboundedReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }
}
