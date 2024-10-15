use crate::config::app_context::AppContext;
use crate::config::constants::{ACTION_EXPIRY_S};
use crate::config::settings::ExecutorConfig;
use crate::executors::execute_tx::execute_tx;
use crate::solana::bloxroute::BloxRoute;
use crate::solana::geyser_pool::GeyserClientPool;
use crate::solana::rpc_pool::RpcClientPool;
use crate::storage::cache::RedisPool;
use crate::types::actions::SolanaAction;
use crate::types::engine::{EventStream, Executor};
use crate::types::events::{BlockchainEvent, BotEvent, ExecutionReceipt};
use crate::types::keys::KeypairClonable;
use crate::utils::keys::clone_keypair;
use crate::{storage, utils};
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use diesel::internal::derives::multiconnection::chrono;
use diesel::internal::derives::multiconnection::chrono::Utc;
use futures::StreamExt;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use solana_sdk::transaction::VersionedTransaction;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use teloxide::Bot;
use tokio::sync;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::sleep;
use tokio_stream::wrappers::{BroadcastStream, UnboundedReceiverStream};
use tracing::{debug, error, info};
use uuid::Uuid;

//todo there should be ONE signer per executor, not multiple, because the executor gets actions in a serial manner from the engine
pub struct PaperExecutor {
    context: AppContext,
}

impl PaperExecutor {
    pub async fn new(context: &AppContext) -> Self {
        Self {
            context: context.clone(),
        }
    }
}

#[async_trait]
impl Executor<Arc<Mutex<SolanaAction>>, BotEvent> for PaperExecutor {
    async fn execute(&self, action: Arc<Mutex<SolanaAction>>) -> Result<BotEvent> {
        let mut action_uuid: Uuid;
        {
            let action_guard = action.lock().await;
            info!("<Paper trading> Mocking execution of {action_guard:#?}");
            self.context
                .cache
                .add_agent_tx(
                    action_guard.uuid,
                    format!("<Paper trading> signature {}", action_guard.uuid),
                )
                .await;
            action_uuid = action_guard.uuid;
        }
        sleep(std::time::Duration::from_millis(100)).await;
        Ok(BotEvent::ExecutionResult(action_uuid, Arc::clone(&action), crate::types::events::ExecutionResult::Sent))
    }
}
