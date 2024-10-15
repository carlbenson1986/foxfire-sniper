use crate::config::constants::GRPC_FEED_COMMITMENT_LEVEL;
use crate::config::settings::{Geyser, ProviderName};
use anyhow::{bail, Result};
use config::Map;
use futures::channel::mpsc;
use futures::channel::mpsc::UnboundedSender;
use futures::{Sink, Stream};
use futures_util::{SinkExt, TryFutureExt};
use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, warn};
use yellowstone_grpc_client::{GeyserGrpcClient, GeyserGrpcClientResult, InterceptorXToken};
use yellowstone_grpc_proto::geyser::CommitmentLevel as GeyserCommitmentLevel;
use yellowstone_grpc_proto::geyser::{SubscribeRequest, SubscribeUpdate};
use yellowstone_grpc_proto::prelude::GetLatestBlockhashResponse;
use yellowstone_grpc_proto::tonic::service::Interceptor;
use yellowstone_grpc_proto::tonic::{Status, Streaming};

pub type GeyserNamedClient = (
    ProviderName,
    Arc<Mutex<GeyserGrpcClient<InterceptorXToken>>>,
);
#[derive(Default, Clone)]
pub struct GeyserClientPool {
    pub(crate) clients: Map<ProviderName, Arc<Mutex<GeyserGrpcClient<InterceptorXToken>>>>,
}

impl Debug for GeyserClientPool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeyserClientPool")
            .field("clients", &self.clients.keys())
            .finish()
    }
}

impl GeyserClientPool {
    #[tracing::instrument]
    pub async fn new(
        uris: &Map<ProviderName, Geyser>,
        commitment_level: GeyserCommitmentLevel,
    ) -> Self {
        let connection_futures = uris.iter().map(|(provider_name, geyser_endpoint)| {
            let provider_name = provider_name.clone();
            async move {
                debug!(
                    "Connecting to {} Geyser gRPC endpoint at {}...",
                    provider_name, geyser_endpoint.uri
                );
                let client = GeyserGrpcClient::build_from_shared(geyser_endpoint.uri.to_owned())
                    .unwrap()
                    .x_token(geyser_endpoint.x_key.to_owned())
                    .unwrap()
                    .connect_timeout(Duration::from_secs(geyser_endpoint.timeout_s.unwrap_or(10)))
                    .timeout(Duration::from_secs(geyser_endpoint.timeout_s.unwrap_or(10)))
                    .connect()
                    .await
                    .unwrap();

                (provider_name, Arc::new(Mutex::new(client)))
            }
        });
        let results = futures::future::join_all(connection_futures).await;
        Self {
            clients: results.into_iter().collect::<Map<ProviderName, Arc<_>>>(),
        }
    }
    
    pub async fn get_latest_blockhash(&self) -> Result<GetLatestBlockhashResponse> {
        while let Some((provider_name, client)) = self.clients.iter().next() {
            let commitment = GeyserCommitmentLevel::Finalized;
            match client
                .lock()
                .await
                .get_latest_blockhash(Some(commitment))
                .await
            {
                Ok(response) => {
                    return Ok(response);
                }
                Err(e) => {
                    warn!(
                        "Error getting latest blockhash from {}: {:?}",
                        provider_name, e
                    );
                }
            }
        }
        bail!("No Geyser clients available")
    }
}
