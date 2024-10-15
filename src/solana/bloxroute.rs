use anyhow::{bail, Result};
use base64;
use bincode;
use futures_util::future;
use futures_util::future::select_ok;
use futures_util::{SinkExt, StreamExt};
use generic_array::typenum::U64;
use generic_array::GenericArray;
use reqwest::Client;
use serde_derive::Deserialize;
use serde_json::json;
use solana_client::rpc_client::RpcClient;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature, Signer};
use solana_sdk::system_instruction::transfer;
use solana_sdk::transaction::Transaction;
use spl_memo::solana_program::hash::Hash;
use spl_memo::ID;
use spl_memo::{build_memo, id};
use spl_token::instruction::TokenInstruction::Transfer;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tungstenite::protocol::Message;
use tungstenite::WebSocket;

const BLOXROUTE_TIP_WALLET: &str = "HWEoBxYs7ssKuudEjzjmpfJVX7Dvi7wescFsVx2L5yoY";
const BLOXROUTE_URI: [&str; 2] = ["uk.solana.dex.blxrbdn.com", "ny.solana.dex.blxrbdn.com"];
const BLOXROUTE_WS: &str = "wss://uk.solana.dex.blxrbdn.com/ws";

#[derive(Deserialize, Debug)]
struct FeeStreamResult {
    project: String,
    percentile: f64,
    fee_at_percentile: String,
}

#[derive(Deserialize, Debug)]
struct FeeStreamParams {
    subscription: String,
    result: FeeStreamResult,
}

#[derive(Deserialize, Debug)]
struct FeeStreamEvent {
    method: String,
    params: FeeStreamParams,
    json_rpc: String,
}
#[derive(Default, Debug, Clone)]
pub struct BloxRoute {
    bloxroute_auth_header: String,
    tip: u64,
    priority_fee: Arc<RwLock<u64>>,
    fee_percentile: u8,
    use_bloxroute_optimal_fee: bool,
    pub use_bloxroute_trader_api: bool,
}

impl BloxRoute {
    pub fn new(bloxroute_auth_header: &str) -> Self {
        Self {
            bloxroute_auth_header: bloxroute_auth_header.to_string(),
            tip: 0,
            priority_fee: Arc::new(RwLock::new(0)),
            fee_percentile: 70,
            use_bloxroute_optimal_fee: false,
            use_bloxroute_trader_api: false,
        }
    }

    pub fn with_tip(mut self, tip: u64) -> Self {
        self.tip = tip;
        self
    }

    pub fn with_bloxroute_optimal_fee(mut self, use_bloxroute_optimal_fee: bool) -> Self {
        self.use_bloxroute_optimal_fee = use_bloxroute_optimal_fee;
        self
    }

    pub fn with_bloxroute_trader_api(mut self, use_bloxroute_trader_api: bool) -> Self {
        self.use_bloxroute_trader_api = use_bloxroute_trader_api;
        self
    }

    pub fn with_fee_percentile(mut self, fee_percentile: u8) -> Self {
        self.fee_percentile = fee_percentile;
        self
    }

    pub async fn start_fee_ws_stream(&self) -> Result<()> {
        if !self.use_bloxroute_optimal_fee {
            bail!("Bloxroute optimal fee is disabled");
        }
        let percentile = self.fee_percentile;
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "subscribe",
            "params": ["GetPriorityFeeStream", {"project": "P_RAYDIUM", "percentile": percentile}]
        });

        let priority_fee = Arc::clone(&self.priority_fee);
        tokio::spawn(async move {
            let (ws_stream, _) = tokio_tungstenite::connect_async(BLOXROUTE_WS)
                .await
                .expect("Failed to connect");
            let (mut write, mut read) = ws_stream.split();

            // Send subscription request
            write
                .send(Message::Text(request.to_string()))
                .await
                .expect("Failed to send request");
            // Spawn a task to keep the connection alive
            let keep_alive_interval = tokio_stream::wrappers::IntervalStream::new(
                tokio::time::interval(tokio::time::Duration::from_secs(10)),
            );
            let write_arc_mutex = Arc::new(tokio::sync::Mutex::new(write));
            let write_arc_mutex_clone = write_arc_mutex.clone();
            tokio::spawn(async move {
                let mut interval = keep_alive_interval;
                while let Some(_) = interval.next().await {
                    write_arc_mutex_clone
                        .lock()
                        .await
                        .send(Message::Ping(vec![]))
                        .await
                        .expect("Failed to send ping");
                }
            });

            // Process incoming messages
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(event) = serde_json::from_str::<FeeStreamEvent>(&text) {
                            if event.method == "subscribe" {
                                let fee = event
                                    .params
                                    .result
                                    .fee_at_percentile
                                    .parse::<u64>()
                                    .unwrap_or(0);
                                *priority_fee.write().await = fee;
                            }
                        }
                    }
                    Ok(Message::Ping(ping)) => {
                        write_arc_mutex
                            .lock()
                            .await
                            .send(Message::Pong(ping))
                            .await
                            .expect("Failed to send pong");
                    }
                    Ok(Message::Close(_)) => break,
                    _ => (),
                }
            }
        });
        Ok(())
    }

    pub async fn get_priority_fee(&self) -> Option<u64> {
        if self.use_bloxroute_optimal_fee {
            Some(*self.priority_fee.read().await)
        } else {
            None
        }
    }

    pub async fn add_bx_tip_and_send_tx(
        &self,
        recent_blockhash: &Hash,
        sender: &Keypair,
        fee_payer: &Keypair,
        instructions: &[Instruction],
    ) -> Result<()> {
        // Convert the tip wallet address to a Pubkey
        let bloxroute_tip_wallet = Pubkey::from_str(BLOXROUTE_TIP_WALLET).unwrap();

        let tip_transfer_instruction = transfer(&fee_payer.pubkey(), &bloxroute_tip_wallet, self.tip);

        // Create the memo instruction

        let memo_instruction = Instruction {
            program_id: Pubkey::from_str("HQ2UUt18uJqKaQFJhgV9zaTdQxUZjNrsKFgoEDquBkcx").unwrap(),
            accounts: vec![AccountMeta::new_readonly(fee_payer.pubkey(), true)],
            data: "Powered by bloXroute Trader Api".as_bytes().to_vec(),
        };

        // Append the tip instruction to the instructions vector
        let mut all_instructions = instructions.to_vec();
        all_instructions.push(tip_transfer_instruction);
        all_instructions.push(memo_instruction);

        // Get a recent blockhash
        // Create and sign the transaction
        let senders = if sender.pubkey() != fee_payer.pubkey() {
            vec![sender, fee_payer]
        } else {
            vec![sender]
        };
        let mut transaction = Transaction::new_with_payer(&all_instructions, Some(&fee_payer.pubkey()));
        transaction.sign(&senders, *recent_blockhash);

        // Serialize the transaction to raw bytes using bincode
        let serialized_transaction = bincode::serialize(&transaction)?;

        // Encode the serialized transaction to base64
        let tx_base64 = base64::encode(serialized_transaction);

        // Bloxroute API URL

        // Prepare the payload
        let payload = serde_json::json!({
             "transaction": {
                "content": tx_base64,
                "isCleanup": false
            },
            "frontRunningProtection": false,
             "useStakedRPCs": true,
        });

        let client = Client::new();
        let futures = BLOXROUTE_URI.iter().map(|uri| {
            let client = client.clone();
            let auth_header = self.bloxroute_auth_header.clone();
            let payload = payload.clone();
            Box::pin(async move {
                let response = client
                    .post(*uri)
                    .header("Content-Type", "application/json")
                    .header("Authorization", &auth_header)
                    .json(&payload)
                    .send()
                    .await?;

                if response.status().is_success() {
                    let response_json: serde_json::Value = response.json().await?;
                    if let Some(signature) = response_json["signature"].as_str() {
                        let signature = Signature::from_str(signature)?;
                        Ok(signature)
                    } else {
                        bail!("Transaction submission failed: {:?}", response_json);
                    }
                } else {
                    bail!(
                        "HTTP error: {}: {}",
                        response.status(),
                        response.text().await?
                    );
                }
            })
        });

        let first_successful_result = select_ok(futures).await;

        match first_successful_result {
            Ok((signature, _)) => Ok(()),
            Err(e) => bail!("Failed to send transaction: {:?}", e),
        }
    }
}
