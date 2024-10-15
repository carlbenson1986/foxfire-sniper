use crate::config::settings::{ProviderName, WebSocket};
use anyhow::Result;
use config::Map;
use futures_util::{SinkExt, TryFutureExt};
use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};
use std::sync::Arc;
use tracing::debug;

pub type WsNamedClient = (
    ProviderName,
    Arc<solana_client::nonblocking::pubsub_client::PubsubClient>,
);

#[derive(Debug, Clone)]
pub struct PubsubClientPool {
    pub(crate) clients:
        Map<ProviderName, Arc<solana_client::nonblocking::pubsub_client::PubsubClient>>,
}

impl PubsubClientPool {
    #[tracing::instrument]
    pub async fn new(ws_rpc_uris: &Map<ProviderName, WebSocket>) -> Self {
        let connection_futures = ws_rpc_uris.iter().map(|(provider_name, ws_details)| {
            let provider_name = provider_name.clone();
            let uri = ws_details.uri.clone();
            async move {
                debug!("Connecting to Solana ws endpoint at {}", uri);
                let client = solana_client::nonblocking::pubsub_client::PubsubClient::new(&uri)
                    .await
                    .unwrap();
                (provider_name, Arc::new(client))
            }
        });
        let results = futures::future::join_all(connection_futures).await;
        Self {
            clients: results.into_iter().collect::<Map<ProviderName, Arc<solana_client::nonblocking::pubsub_client::PubsubClient>>>()
        }
    }
}
