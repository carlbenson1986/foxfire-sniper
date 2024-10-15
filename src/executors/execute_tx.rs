use crate::config::app_context::AppContext;
use crate::config::settings::Geyser;
use crate::solana::bloxroute::BloxRoute;
use crate::solana::geyser_pool::GeyserClientPool;
use crate::solana::rpc_pool::RpcClientPool;
use crate::utils::keys::clone_keypair;
use anyhow::{anyhow, bail, Result};
use futures_util::future::select_all;
use solana_client::rpc_client::{RpcClient, SerializableTransaction};
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::instruction::Instruction;
use solana_sdk::signature::{Keypair, Signature, Signer};
use solana_sdk::transaction::Transaction;
use solana_transaction_status::TransactionConfirmationStatus;
use spinners::{Spinner, Spinners};
use spl_token::solana_program::hash::Hash;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::join;
use tracing::field::debug;
use tracing::{debug, error, info, trace, warn};

pub async fn execute_tx(
    context: &AppContext,
    recent_blockhash: Hash,
    sender: &Keypair,
    fee_payer: &Keypair,
    instructions: &[Instruction],
) -> Result<Signature> {
    debug!(
        "Executing tx with {} instructions",
        instructions.len()
    );
    let mut tx = Transaction::new_with_payer(instructions, Some(&fee_payer.pubkey()));
    let senders = if sender.pubkey() != fee_payer.pubkey() {
        debug!("Adding fee payer to the transaction");
        vec![sender, fee_payer]
    } else {
        vec![sender]
    };
    tx.sign(&senders, recent_blockhash);
    // simulating transaction before signing

    let signature = *tx.get_signature();

    let mut handles: Vec<Pin<Box<dyn Future<Output=Result<()>> + Send>>> = vec![];

    if context.bloxroute.use_bloxroute_trader_api {
        let fut = Box::pin(context.bloxroute.add_bx_tip_and_send_tx(
            &recent_blockhash,
            sender,
            fee_payer,
            instructions,
        ));
        handles.push(fut);
    };
    handles.push(Box::pin(context.rpc_pool.send_tx_to_all_providers(&tx)));

    // this returns on the first successful send and fails if allways to send tx fail
    let mut all_errors = vec![];
    while !handles.is_empty() {
        let (result, _index, remaining) = select_all(handles).await;
        handles = remaining;
        match result {
            Ok(_) => {
                debug!("Transaction sent to all providers, signature `{:?}`", signature);
                return Ok(signature);
            }
            Err(e) => all_errors.push(e),
        }
    }
    bail!("All ways to send the tx failed");
}
