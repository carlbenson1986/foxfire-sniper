use once_cell::sync::Lazy;
use solana_sdk::commitment_config::CommitmentLevel as RpcCommitmentLevel;
use std::str::FromStr;
use yellowstone_grpc_proto::geyser::CommitmentLevel as GeyserCommitmentLevel;

// Messaging channels, huge number is used to avoid blocking for Geyser feed which can be very intense
pub const ENGINE_MESSAGE_CHANNEL_CAPACITY: usize = 16384;
pub const GEYSER_TX_FEED_BUFFER_CAPACITY: usize = 65536;
pub const CACHED_TX_SIGNATURES_BUFFER_CAPACITY: usize = 1024;
// The polling rate for the strategy manager to check for new strategies
pub const NEW_STRATEGY_POLLING_FREQUENCY_MS: u64 = 100;

pub const RT_FEE_ROLLING_AVERAGE_SIZE: usize = 2048;
pub const RT_FEE_PERCENTILE_CAPACITY: usize = 2048;
pub const RT_FEE_PERCENTILE: f64 = 80.0;

pub const MAX_TRANSFERS_IN_ONE_TX: usize = 12;
pub const ACTION_EXPIRY_S: u64 = 1000;

// IF REDIS IS USED ONLY!
// Expiration of the swap cache in seconds
pub const REDIS_SWAP_CACHE_EXPIRES_S: u64 = 600;

pub const TIMEOUT_FOR_ACTION_EXECUTION_HBS: u64 = 200;
pub const RETRIES_IF_ERROR_OR_TIMEOUT: i64 = 2;

pub const COOLDOWN_BETWEEN_RETRIES_HBS: u64 = 5;
pub const BASE_TX_FEE_SOL: u64 = 5000;
pub const TRANSFER_PRIORITY_FEE_SOL: u64 = 10000;
pub const BALANCE_CHANGE_THRESHOLD_SOL: u64 = 1000;
// solana rent 165
pub const RENT_EXEMPTION_THRESHOLD_SOL: u64 = 2039280;
pub const NEW_ACCOUNT_THRESHOLD_SOL: u64 = 890880;

pub const RAYDIUM_SWAP_FEE: f64 = 0.0005;
pub const SIMULATION_RETRIES: usize = 1;
pub const DELAY_BETWEEN_SIMULATION_RETRIES_MS: u64 = 100;
pub const REDIS_POOLS_KEYS: &str = "solana_pools_keys";
pub const REDIS_LP_MINT_KEYS: &str = "solana_lp_mint_keys";
pub const REDIS_SWAP_CACHE_PREFIX: &str = "solana_action_swap_keys";
pub const REDIS_POOLS_DETAILS: &str = "solana_pools_details";

pub const REDIS_USERS: &str = "solana_bot_users";

pub const RPC_COMMITMENT_LEVEL: RpcCommitmentLevel = RpcCommitmentLevel::Processed;
pub const GRPC_FEED_COMMITMENT_LEVEL: GeyserCommitmentLevel = GeyserCommitmentLevel::Processed;
pub const TX_SIMULATION_COMMITMENT_LEVEL: RpcCommitmentLevel = RpcCommitmentLevel::Processed;
//lower thant confirmed doesn't make sense since the transaction details can't be queried from the node
pub const WS_FEED_COMMITMENT_LEVEL: RpcCommitmentLevel = RpcCommitmentLevel::Confirmed;
