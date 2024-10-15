use crate::collectors::raydium_pool_update_collector::RaydiumPriceCollector;
use crate::config::app_context::AppContext;
use crate::types::engine::{Collector, EventStream};
use crate::types::events::{BlockchainEvent, BotEvent};
use async_trait::async_trait;
use chrono::Utc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{error, info};

pub struct HeartbeatCollector {
    delay_ms: u64,
}

impl HeartbeatCollector {
    pub async fn new(context: &AppContext) -> Self {
        Self {
            delay_ms: context
                .get_settings()
                .await
                .collector
                .heartbeat_frequency_ms,
        }
    }
}

#[async_trait]
impl Collector<BotEvent> for HeartbeatCollector {
    async fn get_event_stream(&self) -> anyhow::Result<EventStream<'_, BotEvent>> {
        info!("Initializing HeartbeatCollector event stream");
        let (tx, rx) = mpsc::unbounded_channel();
        let delay = self.delay_ms;
        let mut interval = tokio::time::interval(Duration::from_millis(delay));
        tokio::spawn(async move {
            loop {
                interval.tick().await;
                tx.send(BotEvent::HeartBeat(delay, Instant::now())).unwrap();
            }
        });
        let stream = UnboundedReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }
}
