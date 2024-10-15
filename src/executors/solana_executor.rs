use crate::config::app_context::AppContext;
use crate::config::constants::{ACTION_EXPIRY_S, BASE_TX_FEE_SOL, RENT_EXEMPTION_THRESHOLD_SOL, SIMULATION_RETRIES};
use crate::config::settings::ExecutorConfig;
use crate::executors::execute_tx::execute_tx;
use crate::solana::bloxroute::BloxRoute;
use crate::solana::geyser_pool::GeyserClientPool;
use crate::solana::rpc_pool::RpcClientPool;
use crate::storage::cache::RedisPool;
use crate::types::actions::{SolanaActionPayload, SolanaAction, Asset, Amount};
use crate::types::engine::{EventStream, Executor};
use crate::types::events::{BlockchainEvent, BotEvent, ExecutionError, ExecutionReceipt, ExecutionResult};
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
use spl_token::solana_program::hash::Hash;
use tokio::sync;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::Instant;
use tokio_stream::wrappers::{BroadcastStream, UnboundedReceiverStream};
use tracing::{debug, error, info, instrument};
use crate::executors::build_instructions::{build_instructions, estimate_cu_per_tx};

//todo there should be ONE signer per executor, not multiple, because the executor gets actions in a serial manner from the engine
pub struct SolanaExecutor {
    context: AppContext,
    tx: broadcast::Sender<BlockchainEvent>,
}

impl SolanaExecutor {
    pub async fn new(context: &AppContext) -> Self {
        let config = context.get_settings().await.executor.clone();

        let mut exec_rpc_pool = context.rpc_pool.clone();
        exec_rpc_pool
            .clients
            .iter_mut()
            .filter(|(provider_name, _)| {
                config
                    .solana_execution_rpc_uris_https
                    .iter()
                    .any(|uri| uri.eq(*provider_name))
            });

        //todo move it to strategy - strategy should check the balance
        // 10% as a cushion
        let signers = config.private_keys.clone();
        // let minimum_amount = (0.01f64 * signers.len() as f64 * 1.1) as u64 + 100000000;
        // info!("Initializing SolSwapExecutor, signers: {:?}, every sniper should have at least {} SOL",signers.len(), minimum_amount as f64 / 1000000000.0);
        // for signer in &signers {
        //     let k = &Keypair::from_base58_string(signer).pubkey();
        //     let sol_balance = exec_rpc_pool.get_balance(k).await.unwrap_or(0);
        //     let wsol_balance =
        //         exec_rpc_pool.get_balance(
        //             &spl_associated_token_account::get_associated_token_address(&k, &spl_token::native_mint::id())
        //         ).await.unwrap_or(0);
        //     if wsol_balance + sol_balance < minimum_amount {
        //         panic!("Sniper {:?} has insufficient balance {}, should have at least {} SOL",
        //                k,
        //                (sol_balance + wsol_balance) as f64 / 1000000000.0,
        //                minimum_amount as f64 / 1000000000.0);
        //     }
        //     info!("Sniper {:?} balance: {:?} SOL, {:?} WSOL", k, sol_balance as f64 / 1000000000.0, wsol_balance as f64 / 1000000000.0);
        // }

        // let signers = signers.into_iter()
        //     .map(|private_key| Keypair::from_base58_string(&private_key))
        //     .map(|keypair| (keypair.pubkey(), keypair))
        //     .collect();

        let (tx, _) = broadcast::channel(512);

        Self {
            context: context.clone(),
            tx,
        }
    }
}

#[async_trait]
impl Executor<Arc<Mutex<SolanaAction>>, BotEvent> for SolanaExecutor {
    #[instrument(skip(self, action))]
    async fn execute(&self, action: Arc<Mutex<SolanaAction>>) -> Result<BotEvent> {
        debug!("Executing action: {:#?}", action);
        if action.lock().await.is_expired() {
            return Ok(BotEvent::ExecutionResult(action.lock().await.uuid, action.clone(), ExecutionResult::ExecutionError(ExecutionError::ActionTooOld)));
        }
        let mut price_per_cu_priority = 3052504;
        // let mut price_per_cu_priority = self.context.cache.get_optimal_fee().await;

        let compute_units_per_tx_estimate = estimate_cu_per_tx(&self.context, &action).await;


        debug!("Estimated compute units per tx: {:?}", compute_units_per_tx_estimate);
        match build_instructions(&self.context, &action, price_per_cu_priority, compute_units_per_tx_estimate).await {
            Ok((balance_before, mut action_itx)) => {
                debug!("{} instructions generated", action_itx.len());
                if action_itx.is_empty() {
                    return Ok(BotEvent::ExecutionResult(action.lock().await.uuid, action.clone(), ExecutionResult::ExecutionError(ExecutionError::NoInstructionsGenerated)));
                }
                // understand compute price per unit to pay, optimal_fee includes both the base fee and the priority fee
                // it's prefinal since ideally after simulation we'd gett the actual CU used and should rebuild the tranfers of MAX amounts to close the account, but now it'll do like that
                let mut prefinal_itxs_with_cu =
                    if price_per_cu_priority > 0 {
                        vec![
                            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
                                compute_units_per_tx_estimate
                            ),
                            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(
                                price_per_cu_priority
                            ),
                        ]
                    } else {
                        vec![]
                    };
                prefinal_itxs_with_cu.append(&mut action_itx);

                // add bloxroute tip if needed
                //build a tx
                let action_guard = action.lock().await;
                let fee_payer = action_guard.fee_payer.clone();
                let mut tx = Transaction::new_with_payer(
                    &prefinal_itxs_with_cu, Some(&fee_payer.pubkey()),
                );
                let recent_blockhash = Hash::from_str(&self.context.geyser_pool.get_latest_blockhash().await?.blockhash)?;
                tx.message.recent_blockhash = recent_blockhash;
                // simulate
                if self.context.settings.read().await.executor.simulate_execution {
                    let mut retries = SIMULATION_RETRIES;
                    loop {
                        match self.context.rpc_pool.simulate_tx(&tx).await {
                            Ok(ex) => {
                                debug!("Simulation result: {:#?}", ex);
                                break;
                            }
                            Err(err) => {
                                error!("Simulation failed: {:?}", err);
                                retries -= 1;
                                if retries == 0 {
                                    return Ok(BotEvent::ExecutionResult(
                                        action.lock().await.uuid,
                                        Arc::clone(&action),
                                        ExecutionResult::ExecutionError(ExecutionError::SimulationFailed(err.to_string())),
                                    ));
                                } else {
                                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                }
                            }
                        };
                    }
                }

                // execute
                let signature = execute_tx(
                    &self.context,
                    recent_blockhash,
                    &action_guard.sniper.get_keypair(),
                    &action_guard.fee_payer.get_keypair(),
                    &prefinal_itxs_with_cu).await?;

                // update the action and the cache
                self.context
                    .cache
                    .add_agent_tx(action_guard.uuid, signature.to_string())
                    .await;

                action.lock().await.sent(
                    balance_before,
                    signature,
                    BASE_TX_FEE_SOL as i64 + price_per_cu_priority as i64 * compute_units_per_tx_estimate as i64,
                );
                debug!("Action executed: {:?}", action);
                Ok(BotEvent::ExecutionResult(action.lock().await.uuid, action.clone(), ExecutionResult::Sent))
            }
            Err(e) => {
                //downcasting to crate::types::events::ExecutionError
                match e.downcast_ref::<ExecutionError>() {
                    Some(ExecutionError::Other(error_text)) => {
                        Ok(BotEvent::ExecutionResult(action.lock().await.uuid, Arc::clone(&action), ExecutionResult::ExecutionError(ExecutionError::Other(error_text.clone()))))
                    }
                    _ => bail!(e),
                }
            }
        }
    }
}
