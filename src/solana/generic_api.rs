use std::fmt::Display;
use anyhow::{bail, Result};
use log::trace;
use solana_sdk::pubkey::Pubkey;
use strum_macros::Display;
use tracing::{debug, error, instrument, warn};
use tracing::field::debug;
use thiserror::Error;
use crate::collectors::tx_stream::types::AccountPretty;
use crate::config::app_context::AppContext;

#[derive(Error, Debug)]
pub enum AccountError {
    #[error("Account not found")]
    AccountNotFound,
    #[error("Other: {0}")]
    OtherError(String),
}

pub async fn get_balance(context: &AppContext, pubkey: &Pubkey) -> Result<u64> {
    let update_from_rpc = |pubkey| async move {
        match context.rpc_pool.get_account(pubkey).await {
            Ok(acc) => {
                // cached and exists
                context.cache.update_account(*pubkey, Some(acc.clone().into())).await;
                debug!("Updated cached balance from rpc for {:?}, {}", pubkey, acc.lamports);
                Ok(acc.lamports)
            }
            Err(e) => {
                if e.to_string().contains("AccountNotFound") || e.to_string().contains("Account not found") {
                    // account does not exist, starting monitoring
                    trace!("{:?} does not not exist (rpc query), staring monitoring balance change with Geyser", pubkey);
                    context.cache.monitor_with_geyser(*pubkey).await;
                    context.geyser_resubscribe_account_tx_notify.send(());
                    bail!(AccountError::AccountNotFound)
                } else {
                    trace!("{} get balance error: {:?}", pubkey, e);
                    bail!(AccountError::OtherError(e.to_string()))
                }
            }
        }
    };
    match context.cache.get_account(pubkey).await {
        // account is being watched
        Some(acc_pretty) => {
            // account is exists and cached
            if let Some(acc) = acc_pretty {
                debug!("Using cached balance for {:?}, {}",pubkey, acc.lamports);
                Ok(acc.lamports)
            } else {
                // account is being monitored but does not exists yet
                trace!("Account {:?} is being monitored but no data so far, querying from the chain", pubkey);
                update_from_rpc(pubkey).await
            }
        }
        // account is not cached - but can exist though - fetch from rpc and check
        None => {
            update_from_rpc(pubkey).await
        }
    }
}

pub async fn start_monitoring_account(context: &AppContext, pubkey: &Pubkey) {
    context.cache.monitor_with_geyser(*pubkey).await;
    context.geyser_resubscribe_account_tx_notify.send(());
}

pub async fn start_monitoring_token_account(context: &AppContext, sniper: &Pubkey, token_mint: &Pubkey) {
    let ata = spl_associated_token_account::get_associated_token_address(sniper, token_mint);
    start_monitoring_account(context, &ata).await;
}


pub async fn get_token_balance(context: &AppContext, sniper: &Pubkey, token_mint_address: &Pubkey) -> Result<u64> {
    let query_ata = |ata| async move {
        match context.rpc_pool.get_account(&ata).await {
            Ok(acc_ata) => {
                // cached and exists
                let acc_pretty: AccountPretty = acc_ata.into();
                context.cache.update_account(ata, Some(acc_pretty.clone())).await;
                match acc_pretty.token_unpacked_data {
                    Some(token_account) => {
                        Ok(token_account.amount) 
                    },
                    None => Ok(0)
                }
            }
            Err(e) => {
                if e.to_string().contains("AccountNotFound") || e.to_string().contains("Account not found") {
                    // account does not exist, starting monitoring
                    trace!("Staring monitoring token balance for {:?}, token {:?}, ata {:?}", sniper, token_mint_address, ata);
                    start_monitoring_account(context, &ata).await;
                    bail!(AccountError::AccountNotFound)
                } else {
                    bail!(AccountError::OtherError(e.to_string()))
                }
            }
        }
    };

    let ata = spl_associated_token_account::get_associated_token_address(sniper, token_mint_address);
    match context.cache.get_account(&ata).await {
        // account is being watched
        Some(acc_pretty) => {
            // check if there's data on the account
            if let Some(acc) = acc_pretty {
                match acc.token_unpacked_data {
                    Some(token_account) => {
                        trace!("Using cached token balance for {:?}, token {:?}, balance {}", sniper, token_mint_address, token_account.amount);
                        Ok(token_account.amount)
                    }
                    None => {
                        warn!("Unpacking fails");
                        Ok(0)
                    }
                }
            } else {
                query_ata(ata).await
                // account is being monitored but does not exists yet
            }
        }
        // account is not cached - but can exist though - fetch from rpc and check
        None => {
            query_ata(ata).await
        }
    }
}

pub async fn is_account_exist(context: &AppContext, pubkey: &Pubkey) -> bool {
    match context.cache.get_account(pubkey).await {
        Some(acc_monitored) => {
            if let Some(acc) = acc_monitored {
                // account is exists and cached
                true
            } else {
                // account is being monitored but does not exists yet
                false
            }
        }
        None => {
            // we're assuming it doesn't exist if not in our trading accounts.
            false
        }
    }
}

pub async fn stop_monitoring_account(context: &AppContext, pubkey: &Pubkey) {
    context.cache.drop_account_monitoring(pubkey).await;
    //todo this is unnecessary if periodic automated resubscription is implemented
    context.geyser_resubscribe_account_tx_notify.send(());
}

pub async fn stop_monitoring_token_account(context: &AppContext, sniper: &Pubkey, token_mint: &Pubkey) {
    let ata = spl_associated_token_account::get_associated_token_address(sniper, token_mint);
    stop_monitoring_account(context, &ata).await;
}