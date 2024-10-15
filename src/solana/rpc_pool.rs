use crate::config::constants::{RPC_COMMITMENT_LEVEL, TX_SIMULATION_COMMITMENT_LEVEL};
use crate::config::settings::{ProviderName, Rpc};
use crate::solana::constants::{
    RAYDIUM_V4_AUTHORITY, RAYDIUM_V4_PROGRAM_ID, RAYDIUM_V4_PROGRAM_ID_PUBKEY, WSOL_MINT_PUBKEY,
};
use crate::types::pool::{RaydiumPool, RaydiumPoolPriceUpdate};
use crate::utils::decimals;
use anyhow::{anyhow, bail, Error, Result};
use borsh::BorshDeserialize;
use chrono::Utc;
use config::Map;
use futures_util::future::{join_all, select_all, select_ok};
use futures_util::stream::FuturesUnordered;
use futures_util::{SinkExt, TryFutureExt};
use log::{info, trace, warn};
use solana_account_decoder::parse_token::UiTokenAmount;
use solana_client::client_error::ClientErrorKind::TransactionError;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::{
    RpcSendTransactionConfig, RpcSimulateTransactionConfig, RpcTransactionConfig,
};
use solana_client::rpc_response::Response;
use spl_token::solana_program::program_option::COption;
use spl_token::solana_program::program_pack::Pack;
use spl_token::state::Mint;
use solana_farm_client::raydium_sdk::LiquidityStateV4;
use solana_sdk::account::Account;
use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::transaction;
use solana_sdk::transaction::Transaction;
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding};
use spl_associated_token_account::get_associated_token_address;
use spl_memo::solana_program::clock::Slot;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::ops::Index;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tracing::{debug, error, instrument};
use crate::solana::AccountError;

#[derive(Default, Clone)]
pub struct RpcClientPool {
    //todo add throttling configuration here
    pub(crate) clients: Map<ProviderName, Arc<RpcClient>>,
}

impl Debug for RpcClientPool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcClientPool")
            .field("clients", &self.clients.keys())
            .finish()
    }
}

impl RpcClientPool {
    #[tracing::instrument]
    pub fn new(rpc_client_uris: &Map<ProviderName, Rpc>, commitment: CommitmentLevel) -> Self {
        Self {
            clients: rpc_client_uris.iter().map(|(provider_name, rpc)|
                {
                    (provider_name.to_owned(),
                     Arc::new(
                         solana_client::nonblocking::rpc_client::RpcClient::new_with_commitment(
                             rpc.uri.to_string(),
                             CommitmentConfig {
                                 commitment,
                                 ..CommitmentConfig::default()
                             },
                         )))
                }
            )
                .collect::<Map<ProviderName, Arc<solana_client::nonblocking::rpc_client::RpcClient>>>()
        }
    }

    pub fn get_a_client(&self) -> Option<Arc<RpcClient>> {
        self.clients.values().next().map(|client| client.clone())
    }

    pub async fn execute_rpc_method_consequently_till_first_success<F, Fut, T, E>(
        &self,
        method: F,
    ) -> Result<T>
    where
        F: Fn(Arc<RpcClient>) -> Fut + Send + 'static,
        Fut: Future<Output=std::result::Result<T, E>> + Send,
        T: Send + 'static,
        E: Into<Error> + std::fmt::Display + Send + 'static,
    {
        for (provider_name, client) in &self.clients {
            let client = client.clone();
            match method(client).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    // todo retry on other providers only if there's a communication error
                    trace!("Error executing method on {}: {}", provider_name, e);
                    if e.to_string().contains("AccountNotFound") || e.to_string().contains("Account not found") {
                        // if it's not found, no need to try other providers
                        bail!(AccountError::AccountNotFound)
                    } else {
                        continue;
                    }
                }
            }
        }
        bail!("Failed to execute method on all providers")
    }

    async fn execute_rpc_method_simultaneously_till_first_success<F, Fut, T, E>(
        &self,
        method: F,
    ) -> Result<T>
    where
        F: Fn(Arc<RpcClient>) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output=std::result::Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: Into<Error> + std::fmt::Display + Send + 'static,
    {
        let mut tasks: Vec<JoinHandle<std::result::Result<T, E>>> = Vec::new();

        for (_, client) in &self.clients {
            let method = method.clone();
            let client = client.clone();
            let task: JoinHandle<std::result::Result<T, E>> =
                tokio::spawn(async move { method(client).await });
            tasks.push(task);
        }

        match select_ok(tasks).await {
            Ok((result, _)) => Ok(result.map_err(Into::into)?),
            Err(e) => bail!("Failed to execute method on all providers: {:?}",e),
        }
    }

    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        let pubkey = Arc::new(*pubkey);
        self.execute_rpc_method_consequently_till_first_success(move |client| {
            let pubkey = Arc::clone(&pubkey);
            async move { client.get_balance(&pubkey).await }
        })
            .await
    }

    pub async fn get_balance_ui(&self, pubkey: &Pubkey) -> Result<f64> {
        let balance = self.get_balance(pubkey).await?;
        Ok(decimals::lamports_to_sol(balance))
    }

    pub async fn get_account(&self, pubkey: &Pubkey) -> Result<Account> {
        let pubkey = Arc::new(*pubkey);
        self.execute_rpc_method_consequently_till_first_success(move |client| {
            let pubkey = Arc::clone(&pubkey);
            async move { client.get_account(&pubkey).await }
        })
            .await
    }

    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let pubkey = Arc::new(*pubkey);
        self.execute_rpc_method_consequently_till_first_success(move |client| {
            let pubkey = Arc::clone(&pubkey);
            async move { client.get_account_data(&pubkey).await }
        })
            .await
    }

    pub async fn get_token_account_balance_ui(&self, pubkey: &Pubkey) -> Result<UiTokenAmount> {
        let pubkey = Arc::new(*pubkey);
        self.execute_rpc_method_consequently_till_first_success(move |client| {
            let pubkey = Arc::clone(&pubkey);
            async move { client.get_token_account_balance(&pubkey).await }
        })
            .await
    }

    pub async fn get_token_balance(
        &self,
        sniper_pubkey: &Pubkey,
        token_mint: &Pubkey,
    ) -> Result<u64> {
        let token_ata = get_associated_token_address(sniper_pubkey, &token_mint);
        match self.get_token_account_balance_ui(&token_ata).await {
            Ok(balance) => Ok(balance.amount.parse::<u64>()?),
            Err(e) => Err(e),
        }
    }

    // Note! get_transaction doesn't support commitment level below confirmed!
    pub async fn get_transaction_with_config(
        &self,
        signature: &solana_sdk::signature::Signature,
    ) -> Result<EncodedConfirmedTransactionWithStatusMeta> {
        let signature = signature.clone();
        self.execute_rpc_method_consequently_till_first_success(move |client| {
            let signature = signature.clone();
            async move {
                client
                    .get_transaction_with_config(
                        &signature,
                        RpcTransactionConfig {
                            encoding: Some(UiTransactionEncoding::JsonParsed),
                            // this method doesn't support the commitment below confirmed
                            commitment: Some(CommitmentConfig::confirmed()),
                            max_supported_transaction_version: Some(3),
                        },
                    )
                    .await
            }
        })
            .await
    }

    pub async fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        trace!("Checking if account exists: {:?}", pubkey);
        let pubkey = Arc::new(*pubkey);
        self.execute_rpc_method_consequently_till_first_success(move |client| {
            let pubkey = Arc::clone(&pubkey);
            async move {
                match client.get_account(&pubkey).await {
                    Ok(_) => {
                        debug!("Account found: {:?}", pubkey);
                        Ok(true)
                    }
                    Err(e) => {
                        match e.kind {
                            TransactionError(
                                solana_sdk::transaction::TransactionError::AccountNotFound,
                            ) => {
                                debug!("Account not found: {:?}", pubkey);
                                Ok(false)
                            }
                            _ => {
                                // todo works, but this is awful, should be error type but TransactionError::AccountNotFound doenst' work, fix this later
                                if e.to_string().contains("AccountNotFound") || e.to_string().contains("Account not found") {
                                    trace!("text analysis: Account not found: {:?}", pubkey);
                                    Ok(false)
                                } else {
                                    Err(e)
                                }
                            }
                        }
                    }
                }
            }
        })
            .await
    }


    pub async fn simulate_tx(
        &self,
        tx: &solana_sdk::transaction::Transaction,
    ) -> Result<solana_client::rpc_response::RpcSimulateTransactionResult> {
        let tx = tx.clone();
        let res = self
            .execute_rpc_method_consequently_till_first_success(move |client| {
                let tx = tx.clone();
                async move {
                    client
                        .simulate_transaction_with_config(
                            &tx,
                            RpcSimulateTransactionConfig {
                                sig_verify: false,
                                replace_recent_blockhash: false,
                                commitment: Some(CommitmentConfig {
                                    commitment: TX_SIMULATION_COMMITMENT_LEVEL,
                                }),
                                encoding: Some(UiTransactionEncoding::Base64),
                                accounts: None,
                                min_context_slot: None,
                                inner_instructions: false,
                            },
                        )
                        .await
                }
            })
            .await;

        res.and_then(|res| {
            if let Some(err) = res.value.err {
                bail!(
                    "Simulation failed: {:?} accounts {:?} logs {:?}",
                    err,
                    res.value.accounts,
                    res.value.logs
                );
            } else {
                debug!(
                    "Simulation passed, units consumed {:?}",
                    res.value.units_consumed
                );
                Ok(res.value)
            }
        })
    }

    pub async fn send_tx_to_all_providers(&self, tx: &Transaction) -> Result<()> {
        let mut tasks: Vec<JoinHandle<Result<(), Error>>> = Vec::new();

        for (provider_name, client) in &self.clients {
            let client = client.clone();
            let tx = tx.clone();
            let provider_name = provider_name.clone(); // Move provider_name into async block

            let task: JoinHandle<Result<(), Error>> = tokio::spawn(async move {
                const MAX_RETRIES: usize = 3;
                let mut attempts = 0;
                loop {
                    match client
                        .send_transaction_with_config(
                            &tx,
                            RpcSendTransactionConfig {
                                skip_preflight: true,
                                preflight_commitment: None,
                                encoding: Some(UiTransactionEncoding::Base64),
                                max_retries: None,
                                min_context_slot: None,
                            },
                        )
                        .await
                    {
                        Ok(_) => {
                            trace!("Transaction sent successfully to {}", provider_name);
                            return Ok(());
                        }
                        Err(e) => {
                            attempts += 1;
                            if attempts >= MAX_RETRIES {
                                error!(
                                    "Max retries reached for provider: {} with error: {}",
                                    provider_name, e
                                );
                                return Err(e.into());
                            }
                            warn!(
                                "Retrying transaction for provider: {} due to error: {}",
                                provider_name, e
                            );
                            sleep(Duration::from_millis(100)).await; // backoff before retrying
                        }
                    }
                }
            });
            tasks.push(task);
        }

        // Collect results and handle failures
        let results = join_all(tasks).await;
        for result in results {
            match result {
                Ok(Ok(_)) => return Ok(()),
                Ok(Err(e)) => {
                    println!("Error from task: {}", e);
                    continue;
                }
                Err(e) => {
                    println!("Task panicked: {}", e);
                    continue;
                }
            }
        }

        bail!("Failed to send transaction with all providers")
    }

    pub async fn get_pool_reserves_f64(&self, pool_lp: &Pubkey) -> Result<(f64, f64)> {
        let account = self.get_account(pool_lp).await?;
        let data: Vec<u8> = account.data.clone();
        let market = LiquidityStateV4::try_from_slice(&data)
            .map_err(|e| anyhow!("Failed to parse liquidity state data: {:?}", e))
            .unwrap();

        let (quote_vault, quote_decimal, base_vault, base_decimal) =
            if market.base_mint == *WSOL_MINT_PUBKEY {
                (
                    market.base_vault,
                    market.base_decimal as u8,
                    market.quote_vault,
                    market.quote_decimal as u8,
                )
            } else {
                (
                    market.quote_vault,
                    market.quote_decimal as u8,
                    market.base_vault,
                    market.base_decimal as u8,
                )
            };

        let token_a_balance = self
            .get_token_account_balance_ui(&base_vault)
            .await?
            .amount
            .parse::<u64>()
            .unwrap();

        let token_b_balance = self
            .get_token_account_balance_ui(&quote_vault)
            .await?
            .amount
            .parse::<u64>()
            .unwrap();

        Ok((
            decimals::tokens_to_ui_amount_with_decimals_f64(token_a_balance, base_decimal),
            decimals::tokens_to_ui_amount_with_decimals_f64(token_b_balance, quote_decimal),
        ))
    }

    pub async fn get_pool_price(&self, pool_lp: &Pubkey) -> Result<RaydiumPoolPriceUpdate> {
        let (reserve_base, reserve_quote) = self.get_pool_reserves_f64(pool_lp).await?;
        Ok(RaydiumPoolPriceUpdate {
            pool: *pool_lp,
            price: reserve_quote / reserve_base,
            base_reserve: reserve_base,
            quote_reserve: reserve_quote,
            created_at: Utc::now().naive_utc(),
        })
    }

    pub async fn get_pool_details(&self, pool_pubkey: &Pubkey) -> Result<RaydiumPool> {
        // Fetch account data
        let account_data = self.get_account_data(pool_pubkey).await?;
        let amm_info_data: LiquidityStateV4 =
            borsh::BorshDeserialize::try_from_slice(&account_data).unwrap();

        let program_id = Pubkey::from_str(RAYDIUM_V4_PROGRAM_ID).unwrap();
        let authority = Pubkey::from_str(RAYDIUM_V4_AUTHORITY).unwrap();

        ///todo add more details to the pool, currently we're interested in the vaules basically
        let new = Ok(RaydiumPool {
            id: pool_pubkey.clone(),
            base_mint: amm_info_data.base_mint,
            quote_mint: amm_info_data.quote_mint,
            lp_mint: amm_info_data.lp_mint,
            base_decimals: amm_info_data.base_decimal as u8,
            quote_decimals: amm_info_data.quote_decimal as u8,
            lp_decimals: 0,
            version: 3,
            program_id,
            authority,
            open_orders: amm_info_data.open_orders,
            target_orders: amm_info_data.target_orders,
            base_vault: amm_info_data.base_vault,
            quote_vault: amm_info_data.quote_vault,
            withdraw_queue: amm_info_data.withdraw_queue,
            lp_vault: amm_info_data.lp_vault,
            market_version: 3,
            market_program_id: amm_info_data.market_program_id,
            market_id: amm_info_data.market_id,
            lp_reserve: 0,
            open_time: 0,
            reverse_pool: false,
            freeze_authority: None,
        });
        trace!("Pool details: {:#?}", new);
        new
    }

    pub async fn is_valid_raydium_pool(&self, pubkey: &Pubkey) -> bool {
        match self.get_account(&pubkey).await {
            Ok(account) => {
                let raydium_program_id = *RAYDIUM_V4_PROGRAM_ID_PUBKEY;
                account.owner == raydium_program_id && {
                    match self.get_pool_details(pubkey).await {
                        Ok(pool) => {
                            let (reserve_base, reserve_quote) =
                                self.get_pool_reserves_f64(pubkey).await.unwrap();
                            reserve_base > 0.0 && reserve_quote > 0.0 && {
                                // check if one of the assets is SOL
                                pool.base_mint == *WSOL_MINT_PUBKEY
                                    || pool.quote_mint == *WSOL_MINT_PUBKEY
                            }
                        }
                        Err(_) => false,
                    }
                }
            }
            Err(_) => false,
        }
    }

    pub async fn get_signature_status(
        &self,
        signature: &solana_sdk::signature::Signature,
    ) -> Result<Option<transaction::Result<()>>> {
        let signature = signature.clone();
        self.execute_rpc_method_consequently_till_first_success(move |client| {
            let signature = signature.clone();
            async move { client.get_signature_status(&signature).await }
        })
            .await
    }

    pub async fn get_median_recent_prioritization_fees(&self) -> Result<u64> {
        fn calculate_percentiles(fees: &[u64]) -> HashMap<u8, u64> {
            let mut sorted_fees = fees.to_vec();
            sorted_fees.sort_unstable();
            let len = sorted_fees.len();
            let percentiles = vec![10, 25, 50, 60, 70, 75, 80, 85, 90, 100];
            percentiles
                .into_iter()
                .map(|p| {
                    let index = (p as f64 / 100.0 * len as f64).round() as usize;
                    (p, sorted_fees[index.saturating_sub(1)])
                })
                .collect()
        }
        let mut recent_prioritization_fees = self
            .execute_rpc_method_consequently_till_first_success(move |client| async move {
                client.get_recent_prioritization_fees(&[]).await
            })
            .await?;

        let mut sorted_fees: Vec<_> = recent_prioritization_fees.into_iter().collect();
        sorted_fees.sort_by(|a, b| b.slot.cmp(&a.slot));
        let chunk_size = 150;
        let chunks: Vec<_> = sorted_fees.chunks(chunk_size).take(3).collect();
        let mut percentiles: HashMap<u8, u64> = HashMap::new();
        for (_, chunk) in chunks.iter().enumerate() {
            let fees: Vec<u64> = chunk.iter().map(|fee| fee.prioritization_fee).collect();
            percentiles = calculate_percentiles(&fees);
        }

        // Default to 75 percentile
        let fee = *percentiles.get(&75).unwrap_or(&0);
        Ok(fee)
    }


    pub async fn is_freezable(&self, token_mint: &Pubkey) -> Result<bool> {
        let account_data = self.get_account_data(token_mint).await?;
        let mint =
            Mint::unpack(&account_data).map_err(|e| anyhow!("Failed to unpack Mint data: {:?}", e))?;
        Ok(match mint.freeze_authority {
            COption::Some(authority) => authority != Pubkey::default(),
            COption::None => false,
        })
    }


    pub async fn is_freezable_by_account_data(&self, account_data: &[u8]) -> Result<bool> {
        let mint =
            Mint::unpack(&account_data).map_err(|e| anyhow!("Failed to unpack Mint data: {:?}", e))?;
        Ok(match mint.freeze_authority {
            COption::Some(authority) => authority != Pubkey::default(),
            COption::None => false,
        })
    }
    
    pub async fn is_mintable_by_account_data(&self, account_data: &[u8]) -> Result<bool> {
        let mint =
            Mint::unpack(&account_data).map_err(|e| anyhow!("Failed to unpack Mint data: {:?}", e))?;
        Ok(match mint.mint_authority {
            COption::Some(authority) => authority != Pubkey::default(),
            COption::None => false,
        })
    }


}
