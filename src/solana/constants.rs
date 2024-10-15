use once_cell::sync::Lazy;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

pub const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const WSOL_MINT_ADDRESS: &str = "So11111111111111111111111111111111111111112";
pub const RAYDIUM_V4_PROGRAM_ID: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
pub const COMPUTE_BUDGET: &str = "ComputeBudget111111111111111111111111111111";
pub const RAYDIUM_V4_AUTHORITY: &str = "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1";
pub const RAYDIUM_POOL_INIT_INSTRUCTION: &str = "initialize2";

pub static WSOL_MINT_PUBKEY: Lazy<Pubkey> =
    Lazy::new(|| Pubkey::from_str(WSOL_MINT_ADDRESS).unwrap());
pub static RAYDIUM_V4_PROGRAM_ID_PUBKEY: Lazy<Pubkey> =
    Lazy::new(|| Pubkey::from_str(RAYDIUM_V4_PROGRAM_ID).unwrap());
