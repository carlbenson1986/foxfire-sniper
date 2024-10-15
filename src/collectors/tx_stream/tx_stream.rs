use crate::collectors::tx_stream::geyser_feed::geyser_feed;
use crate::collectors::tx_stream::types::{GeyserFeedEvent, TransactionPretty};
use crate::config::app_context::AppContext;
use crate::solana::geyser_pool::GeyserClientPool;
use crate::solana::rpc_pool::RpcClientPool;
use crate::solana::ws_pool::PubsubClientPool;
use crate::storage::cache::RedisPool;
use crate::types::engine::EventStream;
use crate::utils::circular_buffer_w_rev::CircularBufferWithLookupByValue;
use crate::{solana, utils};
use anyhow::Result;
use chrono::Utc;
use futures::stream::Stream;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::Transaction;
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;
use tracing::field::debug;
use tracing::{debug, info, instrument, trace};
use tracing_subscriber::layer::Context;
use crate::config::constants::GEYSER_TX_FEED_BUFFER_CAPACITY;

// this one is aggregated stream - chooses the fastest from the available  geyser(s) and websocket(s)
pub async fn blockchain_stream(
    context: &AppContext,
) -> Result<EventStream<'static, GeyserFeedEvent>> {
    let mut tx_buffer = CircularBufferWithLookupByValue::new(GEYSER_TX_FEED_BUFFER_CAPACITY);
    let (tx, mut rx) = mpsc::unbounded_channel();
    // Initialize Geyser streams
    for geyser in &context.geyser_pool.clients {
        let geyser_client = geyser.1.clone();
        let tx = tx.clone();
        let context_clone = context.clone();
        tokio::spawn(async move {
            geyser_feed(context_clone, geyser_client, tx.clone()).await;
        });
    }
    let stream_with_dupes_removed =
        StreamExt::filter_map(UnboundedReceiverStream::new(rx), move |e| match &e {
            GeyserFeedEvent::Transaction(tx) => {
                let signature = tx.clone().transaction.map(|tx|
                    Signature::try_from(tx.signature.as_slice()).ok()).flatten()?;
                if tx_buffer.contains_key(&signature) {
                    return None;
                }
                tx_buffer.insert(signature, Utc::now().timestamp_millis());
                trace!("tx: {:?}", tx);
                Some(e)
            }
            GeyserFeedEvent::TxStatusUpdate(tx) => {
                let sig = Signature::try_from(tx.signature.as_slice()).expect("valid signature");
                if tx_buffer.contains_key(&sig) {
                    return None;
                }
                tx_buffer.insert(sig, Utc::now().timestamp_millis());
                trace!("tx status update: {:?}", tx);
                Some(e)
            }
            GeyserFeedEvent::Account(i) => {
                trace!("account: {:?}", i);
                Some(e)
            }
        });

    Ok(Box::pin(stream_with_dupes_removed))
}
