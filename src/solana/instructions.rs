use crate::solana::constants::WSOL_MINT_PUBKEY;
use once_cell::sync::Lazy;
use solana_client::rpc_client::RpcClient;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use spl_associated_token_account::{get_associated_token_address, instruction};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, warn};

pub fn get_token_balance(
    client: &RpcClient,
    sniper: &Pubkey,
    base_mint: &Pubkey,
) -> anyhow::Result<u64> {
    let sniper_ata = get_associated_token_address(sniper, base_mint);
    // Check if the ATA exists
    match client.get_account(&sniper_ata) {
        Ok(_) => {
            // ATA exists, get the balance
            match client.get_token_account_balance(&sniper_ata) {
                Ok(balance) => Ok(balance.amount.parse::<u64>().unwrap_or(0)),
                Err(e) => Err(anyhow::anyhow!(
                    "Failed to get token account balance: {:?}",
                    e
                )),
            }
        }
        Err(_) => {
            // ATA doesn't exist, return zero balance
            Ok(0)
        }
    }
}
