use crate::tg_bot::volume_strategy_config_args::VolumeStrategyConfigArgs;
use crate::types::volume_strategy::VolumeStrategyInstance;
use config::{Config, ConfigError, File, Map};
use serde_derive::{Deserialize, Serialize};
use solana_sdk::commitment_config::CommitmentLevel;
use solana_sdk::pubkey::Pubkey;
use spl_associated_token_account::solana_program::example_mocks::solana_sdk::signature::Keypair;
use std::sync::Arc;
use tokio::sync::RwLock;
use yata::core::PeriodType;
use crate::tg_bot::sniping_strategy_config_args::SnipingStrategyConfigArgs;

pub type ProviderName = String;

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub enum Mode {
    BackTesting,
    PaperTrading,
    Live,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct Rpc {
    pub(crate) uri: String,
    //todo implement this one to avoid post-throttling timeouts and to query the next throttling-free endpoint
    pub(crate) throttling: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct Geyser {
    pub(crate) uri: String,
    pub(crate) x_key: Option<String>,
    pub(crate) timeout_s: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct WebSocket {
    pub(crate) uri: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct CollectorConfig {
    pub(crate) raydium_pool_polling_interval_ms: u64,
    pub(crate) same_price_threshold: f64,
    pub(crate) heartbeat_frequency_ms: u64,
    pub(crate) poll_node_for_tx_confirmations_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct AggregatorConfig {
    pub(crate) indicator_periods_in_ticks: Vec<PeriodType>,
    pub(crate) tick_bar_sizes: Vec<PeriodType>,
}

#[derive(Clone, Deserialize)]
#[allow(unused)]
pub struct ExecutorConfig {
    pub(crate) private_keys: Vec<String>,
    pub(crate) solana_execution_rpc_uris_https: Vec<ProviderName>,
    pub(crate) use_bloxroute_trader_api: bool,
    pub(crate) use_bloxroute_optimal_fee: bool,
    pub(crate) bloxroute_auth_header: String,
    pub(crate) bloxroute_fee_percentile: u8,
    pub(crate) bloxroute_tip: u64,
    pub(crate) flat_fee_if_bloxroute_is_not_used: u64,
    pub(crate) simulate_execution: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum StrategyConfig {
    Volume(VolumeStrategyConfigArgs),
    Sniping(SnipingStrategyConfigArgs),
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct LoggerConfig {
    pub(crate) level: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct StorageConfig {
    pub database_uri: String,
    pub redis_uri: String,
}
#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct EngineConfig {
    pub mode: Mode,
    pub bot_wallet: Option<String>,
    pub bot_fee: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct TgBotConfig {
    pub telegram_token: String,
    pub whitelisted_chat_ids: Option<Vec<i64>>,
    pub admin_chat_ids: Option<Vec<i64>>,
    pub minimum_deposit_sol: f64,
    pub bot_fee_percentage_taken_from_deposit: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct Settings {
    pub rpcs: Map<ProviderName, Rpc>,
    pub geysers: Map<ProviderName, Geyser>,
    pub websockets: Option<Map<ProviderName, WebSocket>>,
    pub collector: CollectorConfig,
    pub aggregator: AggregatorConfig,
    pub strategies: Map<String, StrategyConfig>,
    pub executor: ExecutorConfig,
    pub logger: LoggerConfig,
    pub storage: StorageConfig,
    pub engine: EngineConfig,
    pub tgbot: Option<TgBotConfig>,
}

impl std::fmt::Debug for ExecutorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutorConfig")
            .field(
                "solana_execution_rpc_uris_https",
                &self.solana_execution_rpc_uris_https,
            )
            .field("bloxroute_fee_percentile", &self.bloxroute_fee_percentile)
            .field("use_bloxroute_trader_api", &self.use_bloxroute_trader_api)
            .field("bloxroute_tip", &self.bloxroute_tip)
            .field("private_keys", &"<hidden>")
            .field("bloxroute_auth_header", &"<hidden>")
            .finish()
    }
}

impl Settings {
    pub fn new(config_filename: &str) -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(File::with_name(config_filename))
            .build()?;
        s.try_deserialize()
    }

    pub fn get_volume_strategy_config(&self) -> Option<VolumeStrategyConfigArgs> {
        match self.strategies.get("mm") {
            Some(StrategyConfig::Volume(config)) => Some(config.clone()),
            _ => None,
        }
    }

    pub fn get_sniping_strategy_config(&self) -> Option<SnipingStrategyConfigArgs> {
        match self.strategies.get("trading") {
            Some(StrategyConfig::Sniping(config)) => Some(config.clone()),
            _ => None,
        }
    }
}
