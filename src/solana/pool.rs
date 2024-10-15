use crate::solana::constants::{RAYDIUM_V4_PROGRAM_ID, WSOL_MINT_ADDRESS};
use crate::types::pool::{RaydiumPool, RaydiumPoolPriceUpdate};
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;
use solana_transaction_status::option_serializer::OptionSerializer;
use solana_transaction_status::parse_instruction::ParsedInstruction;
use solana_transaction_status::{Encodable, EncodableWithMeta, EncodedTransaction, EncodedTransactionWithStatusMeta, TransactionStatusMeta, UiInnerInstructions, UiInstruction, UiMessage, UiParsedInstruction, UiTransactionEncoding, UiTransactionTokenBalance};
use std::str::FromStr;
use regex::Regex;
use solana_account_decoder::parse_token::token_amount_to_ui_amount;
use solana_sdk::transaction::TransactionVersion;
use spl_token::solana_program::message::v0::LoadedAddresses;
use tracing::{debug, error, info, trace};
use yellowstone_grpc_proto::convert_from::{create_loaded_addresses, create_meta_inner_instructions, create_token_balances, create_tx_meta};
use yellowstone_grpc_proto::prelude::SubscribeUpdateTransaction;
use crate::collectors::tx_stream::types::TransactionPretty;
use crate::utils::decimals::{lamports_to_sol, tokens_to_ui_amount_with_decimals_f64};
// this only works with  EncodedTransaction::Json variant

pub(crate) fn extract_pool_from_init_tx(
    tx_update: &SubscribeUpdateTransaction,
) -> Option<(RaydiumPool, RaydiumPoolPriceUpdate)> {
    let tx_pretty: TransactionPretty = tx_update.clone().into();
    let transaction = tx_pretty.tx;
    let initialize_log = match &transaction.clone().meta?.log_messages {
        OptionSerializer::Some(log_messages) => {
            let log_message = log_messages.iter().find(|m| m.contains("initialize2"))?;

            // Define the regex pattern
            let re = Regex::new(r"initialize2: InitializeInstruction2 \{ nonce: (\d+), open_time: (\d+), init_pc_amount: (\d+), init_coin_amount: (\d+) }").ok()?;

            // Apply the regex pattern to the log message
            if let Some(captures) = re.captures(log_message) {
                let nonce = captures.get(1)?.as_str().parse::<u8>().ok()?;
                let open_time = captures.get(2)?.as_str().parse::<u64>().ok()?;
                let init_pc_amount = captures.get(3)?.as_str().parse::<u64>().ok()?;
                let init_coin_amount = captures.get(4)?.as_str().parse::<u64>().ok()?;
                Some((nonce, open_time, init_pc_amount, init_coin_amount))
            } else {
                None
            }
        }
        _ => {
            return None;
        }
    }?;
    debug!("New Pool deployment signature detected: {:?}", tx_pretty.signature);
    let mut transaction_clone = transaction.clone();
    let json = match &transaction_clone.transaction {
        EncodedTransaction::Json(t) => Some(t),
        _ => {
            trace!("transaction: {:#?}",transaction_clone);
            let t = transaction_clone.transaction.decode();
            let ui_meta = transaction_clone.meta?;
            let meta = tx_update.clone().transaction?.meta?;
            let tx_meta = create_tx_meta(meta).ok()?;
            let encoded_tx = t
                .map(|t| t.encode_with_meta(UiTransactionEncoding::JsonParsed, &tx_meta));
            transaction_clone.transaction = encoded_tx?;
            match &transaction_clone.transaction {
                EncodedTransaction::Json(t) => Some(t),
                _ => None
            }
        }
    }?;


    let message = match &json.message {
        UiMessage::Parsed(m) => Some(m),
        _ => {
            error!("UiMessage::Parsed not found tx: {:#?}",tx_update);
            None
        }
    }?;

    trace!("signature {:?}, tx: {:?}, message: {:?}",tx_pretty.signature, json, message);

    let initialize_tx = message.instructions.iter()
        .find(|i| {
            if let UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(instruction_parsed)) = i {
                instruction_parsed.program_id == RAYDIUM_V4_PROGRAM_ID
            } else {
                false
            }
        })?;

    let accounts = if let UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(instruction_parsed)) = initialize_tx {
        &instruction_parsed.accounts
    } else {
        return None;
    };

    let mut base_mint = Pubkey::from_str(&accounts[8]).unwrap();
    let mut base_vault = Pubkey::from_str(&accounts[10]).unwrap();
    let mut quote_mint = Pubkey::from_str(&accounts[9]).unwrap();
    let mut quote_vault = Pubkey::from_str(&accounts[11]).unwrap();

    let reversed_pool = base_mint.to_string() == WSOL_MINT_ADDRESS;

    if reversed_pool {
        std::mem::swap(&mut base_mint, &mut quote_mint);
        std::mem::swap(&mut base_vault, &mut quote_vault);
    };

    let lp_mint = Pubkey::from_str(&accounts[7]).unwrap();

    let (sol_reserve, token_reserve) = if reversed_pool {
        (initialize_log.3, initialize_log.2)
    } else {
        (initialize_log.2, initialize_log.3)
    };


    let pre_token_balance = extract_token_balance_from_pre_or_post_token_balances(
        &transaction.meta.unwrap().pre_token_balances, base_mint.to_string().as_str(),
    )?;

    let base_reserve = tokens_to_ui_amount_with_decimals_f64(
        token_reserve,
        pre_token_balance.ui_token_amount.decimals,
    );
    let quote_reserve = lamports_to_sol(sol_reserve);

    Some((
        RaydiumPool {
            id: Pubkey::from_str(&accounts[4]).unwrap(),
            base_mint,
            quote_mint,
            lp_mint: Pubkey::from_str(&accounts[7]).unwrap(),
            base_decimals: pre_token_balance.ui_token_amount.decimals,
            quote_decimals: 9,
            lp_decimals: 0,
            version: 4,
            program_id: Pubkey::from_str(RAYDIUM_V4_PROGRAM_ID).unwrap(),
            authority: Pubkey::from_str(&accounts[5]).unwrap(),
            open_orders: Pubkey::from_str(&accounts[6]).unwrap(),
            target_orders: Pubkey::from_str(&accounts[13]).unwrap(),
            base_vault,
            quote_vault,
            withdraw_queue: Pubkey::from_str("11111111111111111111111111111111").unwrap(),
            lp_vault: Pubkey::from_str(&accounts[12]).unwrap(),
            market_version: 3,
            market_program_id: Pubkey::from_str(&accounts[15]).unwrap(),
            market_id: Pubkey::from_str(&accounts[16]).unwrap(),
            lp_reserve: 0,
            open_time: 0,
            reverse_pool: reversed_pool,
            freeze_authority: None,
        },
        RaydiumPoolPriceUpdate {
            pool: Pubkey::from_str(&accounts[4]).ok()?,
            price: quote_reserve / base_reserve,
            base_reserve,
            quote_reserve,
            created_at: chrono::Utc::now().naive_utc(),
        },
    ))
}

pub fn extract_token_balance_from_pre_or_post_token_balances(
    token_balances: &OptionSerializer<Vec<UiTransactionTokenBalance>>,
    token_mint_address: &str,
) -> Option<UiTransactionTokenBalance> {
    if let OptionSerializer::Some(balances) = token_balances.as_ref() {
        for balance in balances {
            if balance.mint == token_mint_address {
                return Some(balance.clone());
            }
        }
    }
    None
}

fn get_inner_instruction_by_type_field(
    inner_instructions: &[UiInnerInstructions],
    address: Pubkey,
    type_field_value: &str,
    info_field: &str,
) -> Option<ParsedInstruction> {
    for inner in inner_instructions {
        for instruction in &inner.instructions {
            if let UiInstruction::Parsed(UiParsedInstruction::Parsed(instruct)) = instruction {
                let data: &Value = &instruct.parsed;
                if let Some(type_field) = extract_type_field(data) {
                    if type_field == type_field_value {
                        if let Some(info) = data
                            .get("info")
                            .and_then(|info| info.get(info_field))
                            .and_then(Value::as_str)
                            .map(String::from)
                        {
                            if info == address.to_string() {
                                return Some(instruct.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_transfer_instruction_in_inner_instructions_by_destination(
    inner_instructions: &Vec<UiInnerInstructions>,
    destination_account: &str,
) -> Option<ParsedInstruction> {
    for inner in inner_instructions {
        for instruction in &inner.instructions {
            if let UiInstruction::Parsed(parsed_instruction) = instruction {
                match &parsed_instruction {
                    UiParsedInstruction::Parsed(instruct) => {
                        let data: &Value = &instruct.parsed;
                        if extract_type_field(data).unwrap() == "transfer".to_string()
                            && data
                            .get("info")
                            .and_then(|info| info.get("destination"))
                            .and_then(Value::as_str)
                            .map(String::from)
                            .unwrap()
                            == *destination_account
                            && instruct.program_id == *RAYDIUM_V4_PROGRAM_ID
                        {
                            return Some(instruct.clone());
                        }
                    }
                    &UiParsedInstruction::Parsed(_) => {}
                    _ => {}
                }
            }
        }
    }
    None
}

fn extract_field<F, T>(data: &Value, fields: &[&str], extract_fn: F) -> Option<T>
where
    F: FnOnce(&Value) -> Option<T>,
{
    let mut value = data;
    for &field in fields {
        value = value.get(field)?;
        if value.is_null() {
            return None;
        }
    }
    extract_fn(value)
}

fn extract_type_field(data: &Value) -> Option<String> {
    data.get("type").and_then(Value::as_str).map(String::from)
}

fn get_info_amount(base_instruction: Option<ParsedInstruction>) -> Option<u64> {
    let data: &Value = &base_instruction?.parsed;
    data.get("info")
        .and_then(|info| info.get("amount"))
        .and_then(Value::as_str)
        .and_then(|v| u64::from_str(v).ok())
}

fn find_transfer_amount(
    inner_instructions: &[UiInnerInstructions],
    vault_address: &str,
) -> Option<u64> {
    for inner in inner_instructions {
        for instruction in &inner.instructions {
            if let UiInstruction::Parsed(UiParsedInstruction::Parsed(ref instruct)) = instruction {
                if instruct.program_id == spl_token::id().to_string() {
                    let parsed = &instruct.parsed;
                    if let Some(type_field) = extract_type_field(parsed) {
                        if type_field == "transfer" {
                            if let Some(destination) =
                                extract_field(parsed, &["info", "destination"], |v| {
                                    v.as_str().map(String::from)
                                })
                            {
                                if destination == vault_address {
                                    return extract_field(parsed, &["info", "amount"], |v| {
                                        v.as_str().and_then(|s| u64::from_str(s).ok())
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
