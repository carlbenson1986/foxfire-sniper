use crate::solana::constants::WSOL_MINT_PUBKEY;
use crate::solana::rpc_pool::RpcClientPool;
use crate::storage::cache::RedisPool;
use crate::utils::decimals;
use anyhow::{anyhow, Result};
use borsh::BorshDeserialize;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_farm_client::error::FarmClientError;
use solana_farm_client::raydium_sdk::LiquidityStateV4;
use solana_farm_sdk::math;
use solana_sdk::pubkey::Pubkey;
use spl_token::solana_program::program_option::COption;
use spl_token::solana_program::program_pack::Pack;
use spl_token::state::Mint;
use tokio::time;
use tokio::time::sleep;
use tracing::{error, info};

const BALANCE_QUERY_RETRIES: u8 = 10;
const BALANCE_QUERY_DELAY: u64 = 200;

/// Returns native SOL balance
pub async fn get_account_sol_balance_f64(
    rpc_client: &RpcClient,
    wallet_address: &Pubkey,
) -> Result<f64> {
    Ok(decimals::tokens_to_ui_amount_with_decimals_f64(
        rpc_client.get_balance(wallet_address).await?,
        spl_token::native_mint::DECIMALS,
    ))
}

pub async fn get_token_balance_u64(
    rpc_client: &RpcClient,
    sniper: &Pubkey,
    token_mint_address: &Pubkey,
) -> Result<u64> {
    let ata =
        spl_associated_token_account::get_associated_token_address(sniper, token_mint_address);
    for _ in 0..BALANCE_QUERY_RETRIES {
        match rpc_client.get_token_account_balance(&ata).await {
            Ok(balance) => return Ok(balance.amount.parse::<u64>().unwrap()),
            Err(e) => {
                error!("get_token_balance: Error: {e}, token_mint_address: {token_mint_address}, sniper: {sniper}");
                sleep(time::Duration::from_millis(BALANCE_QUERY_DELAY));
            }
        }
    }
    Ok(0)
}

pub async fn get_token_balance_f64(
    rpc_client: &RpcClient,
    sniper: &Pubkey,
    token_mint_address: &Pubkey,
    token_decimals: u8,
) -> Result<f64> {
    Ok(decimals::tokens_to_ui_amount_with_decimals_f64(
        get_token_balance_u64(rpc_client, sniper, token_mint_address).await?,
        token_decimals,
    ))
}