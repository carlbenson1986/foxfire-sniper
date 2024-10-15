use crate::config::constants::{BASE_TX_FEE_SOL, CACHED_TX_SIGNATURES_BUFFER_CAPACITY, RT_FEE_PERCENTILE, RT_FEE_PERCENTILE_CAPACITY, RT_FEE_ROLLING_AVERAGE_SIZE};
use crate::types::pool::{RaydiumPool, RaydiumPoolPriceUpdate};
use crate::types::bot_user::BotUser;
use crate::utils::fee_metrics::FeeMetrics;
use anyhow::{anyhow, Result};
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::Arc;
use lru::LruCache;
use solana_sdk::account::Account;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, trace};
use uuid::Uuid;
use crate::collectors::tx_stream::types::AccountPretty;
use crate::types::actions::SolanaAction;
use crate::utils::circular_buffer::CircularBuffer;
use crate::utils::circular_buffer_w_rev::CircularBufferWithLookupByValue;

#[derive(Clone)]
pub struct OperationalCache {
    pub agent_tx_signatures: Arc<RwLock<CircularBufferWithLookupByValue<Uuid, String>>>,
    pub all_system_actions: Arc<RwLock<CircularBuffer<Uuid, Arc<Mutex<SolanaAction>>>>>,
    pub processed_signatures: Arc<RwLock<CircularBufferWithLookupByValue<Uuid, String>>>,
    pub optimal_fee: Arc<RwLock<FeeMetrics>>,
    // pool_id, pool
    pub target_pools: Arc<RwLock<HashMap<Pubkey, RaydiumPool>>>,
    // token_id, pool_id
    pub target_tokens: Arc<Mutex<LruCache<Pubkey,Pubkey>>>,
    pub target_pools_prices: Arc<Mutex<HashMap<Pubkey, RaydiumPoolPriceUpdate>>>,
    pub accounts: Arc<Mutex<HashMap<Pubkey, Option<AccountPretty>>>>,
}

impl OperationalCache {
    pub fn new(
        target_pools: HashMap<Pubkey, RaydiumPool>,
        target_pools_prices: HashMap<Pubkey, RaydiumPoolPriceUpdate>,
    ) -> Self {
        Self {
            agent_tx_signatures: Arc::new(RwLock::new(CircularBufferWithLookupByValue::new(
                CACHED_TX_SIGNATURES_BUFFER_CAPACITY,
            ))),
            all_system_actions: Arc::new(RwLock::new(CircularBuffer::new(CACHED_TX_SIGNATURES_BUFFER_CAPACITY))),
            processed_signatures: Arc::new(RwLock::new(CircularBufferWithLookupByValue::new(
                CACHED_TX_SIGNATURES_BUFFER_CAPACITY,
            ))),
            optimal_fee: Arc::new(RwLock::new(FeeMetrics::new(
                RT_FEE_ROLLING_AVERAGE_SIZE,
                RT_FEE_PERCENTILE_CAPACITY,
            ))),

            target_pools: Arc::new(RwLock::new(target_pools)),
            target_tokens: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::try_from(CACHED_TX_SIGNATURES_BUFFER_CAPACITY).unwrap()))),
            target_pools_prices: Arc::new(Mutex::new(target_pools_prices)),
            accounts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn register_action(&self, action: SolanaAction) -> Arc<Mutex<SolanaAction>> {
        let uuid = action.uuid;
        let mut write = self.all_system_actions.write().await;
        let action_am = Arc::new(Mutex::new(action));
        write.insert(uuid, action_am.clone());
        action_am
    }

    pub async fn get_action_by_uuid(&self, uuid: Uuid) -> Option<Arc<Mutex<SolanaAction>>> {
        let actions = self.all_system_actions.read().await;
        actions.get_by_key(&uuid).cloned()
    }

    // Signature management
    pub async fn add_agent_tx(&self, uuid: Uuid, signature: String) {
        {
            let mut write = self.agent_tx_signatures.write().await;
            write.insert(uuid, signature);
        }
        let signatures_to_monitor = self.agent_tx_signatures.read().await;
        debug!("Added transaction signature to monitor, monitoring: {:?}", signatures_to_monitor.get_all_values());
    }

    pub async fn get_uuid_by_signature(&self, signature: &str) -> Option<Uuid> {
        self.agent_tx_signatures.read().await.get_by_value(&signature.to_string()).cloned()
    }

    pub async fn get_signature_by_uuid(&self, uuid: Uuid) -> Option<String> {
        self.agent_tx_signatures.read().await.get_by_key(&uuid).cloned()
    }

    pub async fn pop_front(&self) -> Option<(Uuid, String)> {
        self.agent_tx_signatures.write().await.pop_front()
    }

    pub async fn if_agent_signature(&self, signature: &str) -> bool {
        self.agent_tx_signatures.read().await.contains_value(&signature.to_string())
    }

    pub async fn get_all_unprocessed_tx_signatures(&self) -> Vec<String> {
        let processed_signatures = self.processed_signatures.read().await;
        self.agent_tx_signatures.read().await.get_all_values().iter()
            .filter(|signature| processed_signatures.contains_value(&signature.to_string()))
            .cloned()
            .collect()
    }

    pub async fn mark_signature_as_processed(&self, uuid: Uuid, signature: String) {
        let mut write = self.processed_signatures.write().await;
        write.insert(uuid, signature);
    }

    pub async fn is_signature_processed(&self, signature: &str) -> bool {
        self.processed_signatures.read().await.contains_value(&signature.to_string())
    }

    pub async fn update_optimal_fee(&self, fee: u64) {
        self.optimal_fee.write().await.add_fee(fee);
    }

    pub async fn get_optimal_fee(&self) -> u64 {
        let fee = self
            .optimal_fee
            .read()
            .await
            .get_percentile(RT_FEE_PERCENTILE)
            .unwrap_or(0);
        debug!("Getting optimal tx fee (price per cu): {fee}");
        fee
    }

    // don't flatten here
    // - if it's None - not monitoring
    // - if it's Some(None) - monitoring, but no data
    // - if it's Some(Some(data)) - monitoring and data is present
    pub async fn get_account(&self, acc: &Pubkey) -> Option<Option<AccountPretty>> {
        self.accounts.lock().await.get(acc).cloned()
    }
    
    pub async fn get_accounts_count(&self) -> usize {
        self.accounts.lock().await.len()
    }

    pub async fn get_accounts(&self) -> Vec<Pubkey> {
        self.accounts.lock().await.keys().cloned().collect()
    }


    pub async fn monitor_with_geyser(&self, acc: Pubkey) {
        let mut balances = self.accounts.lock().await;
        balances.insert(acc, None);
    }


    pub async fn update_account(&self, acc: Pubkey, account: Option<AccountPretty>) {
        let mut balances = self.accounts.lock().await;
        balances.insert(acc, account);
    }

    pub async fn drop_account_monitoring(&self, acc: &Pubkey) {
        let mut balances = self.accounts.lock().await;
        balances.remove(acc);
    }
}
