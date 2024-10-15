use crate::collectors::prices_heartbeat_streamer::start_prices_stream;
use crate::config::app_context::AppContext;
use crate::solana::rpc_pool::RpcClientPool;
use crate::types::engine::{Collector, EventStream};
use crate::types::events::{BlockchainEvent, BotEvent};
use anyhow::Result;
use async_trait::async_trait;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_client::RpcClient;
use solana_farm_client::client::FarmClient;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;
use tracing::error;

/// A collector that listens to raydium pool events logs based on a [Filter](Filter),
/// and generates a stream of [events](Log).
pub struct RaydiumPriceCollector {
    pub(crate) context: AppContext,
}

impl RaydiumPriceCollector {
    pub fn new(context: &AppContext) -> Self {
        Self {
            context: context.clone(),
        }
    }
}

/// Implementation of the [Collector](Collector) trait for the [LogCollector](LogCollector).
/// This implementation uses the [PubsubClient](PubsubClient) to subscribe to new logs.
#[async_trait]
impl Collector<BotEvent> for RaydiumPriceCollector {
    async fn get_event_stream(&self) -> Result<EventStream<'_, BotEvent>> {
        let (tx, rx) = mpsc::unbounded_channel();

        start_prices_stream(&self.context, tx.clone()).await;

        let stream =
            UnboundedReceiverStream::new(rx).filter_map(|signal_result| match signal_result {
                Ok(signal) => Some(signal),
                Err(err) => {
                    error!("Error receiving result: {:?}", err);
                    None
                }
            });

        Ok(Box::pin(stream))
    }
}
