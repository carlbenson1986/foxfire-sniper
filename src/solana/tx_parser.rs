use crate::solana::constants;
use crate::storage;
use crate::storage::cache::RedisPool;
use crate::types::events::ExecutionReceipt;
use crate::types::pool::{RaydiumPool, RaydiumPoolPriceUpdate, TradeDirection};
use crate::utils::decimals::lamports_to_sol;
use anyhow::{bail, Result};
use base64::Engine;
use borsh::BorshDeserialize;
use core::panic;
use std::default::Default;
use serde::Deserialize;
use serde_json::to_string;
use solana_sdk::message::VersionedMessage;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::TransactionError;
use solana_sdk::{bs58, pubkey::Pubkey, transaction::Transaction};
use solana_transaction_status::{option_serializer::OptionSerializer, Encodable, EncodableWithMeta, EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, EncodedTransactionWithStatusMeta, TransactionBinaryEncoding, TransactionStatusMeta, UiCompiledInstruction, UiInnerInstructions, UiInstruction, UiMessage, UiParsedInstruction, UiParsedMessage, UiPartiallyDecodedInstruction, UiRawMessage, UiTransactionEncoding};
use spl_associated_token_account::get_associated_token_address;
use spl_associated_token_account::solana_program::message::Message;
use spl_token::instruction::TokenInstruction;
use std::str::FromStr;
use serde_derive::Serialize;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use spl_token::solana_program::system_program;
use tracing::{debug, info, instrument, trace};
use uuid::Uuid;
use crate::solana::constants::{RAYDIUM_V4_PROGRAM_ID, WSOL_MINT_ADDRESS};
use crate::solana::pool::{extract_pool_from_init_tx, extract_token_balance_from_pre_or_post_token_balances};

#[derive(Debug)]
pub struct Swap {
    pub signature: Signature,
    pub pool_id: Pubkey,
    pub quote_amount: u64,
    pub base_amount: u64,
    pub trade_direction: TradeDirection,
}

pub fn parse_mint(tx: &EncodedTransactionWithStatusMeta) -> Result<String> {
    let instructions = self::parse_instructions(tx)?;
    for instruction in instructions {
        if let UiInstruction::Parsed(ix) = instruction {
            match ix {
                UiParsedInstruction::Parsed(ix) => {
                    if ix.program == "spl-associated-token-account" {
                        // TODO this might panic, might be handled more gracefully
                        let mint = ix.parsed["info"]["mint"].as_str().unwrap().to_string();
                        return Ok(mint);
                    }
                }
                UiParsedInstruction::PartiallyDecoded(_) => (),
            }
        }
    }
    bail!("Mint not found in tx")
}

pub fn parse_accounts(
    tx: &EncodedTransactionWithStatusMeta,
) -> Result<String, Box<dyn std::error::Error>> {
    let instructions = self::parse_instructions(tx)?;
    for instruction in instructions {
        if let UiInstruction::Parsed(ix) = instruction {
            match ix {
                UiParsedInstruction::Parsed(ix) => {
                    if ix.program == "spl-associated-token-account" {
                        // TODO this might panic, might be handled more gracefully
                        let mint = ix.parsed["info"]["mint"].as_str().unwrap().to_string();
                        return Ok(mint);
                    }
                }
                UiParsedInstruction::PartiallyDecoded(_) => (),
            }
        }
    }
    Err("Mint not found in tx".into())
}

pub fn parse_notional(
    tx: &EncodedConfirmedTransactionWithStatusMeta,
) -> Result<u64, Box<dyn std::error::Error>> {
    if let Some(meta) = &tx.transaction.meta {
        let max_sol = std::iter::zip(&meta.pre_balances, &meta.post_balances)
            .map(|(a, b)| (*a as f64 - *b as f64) as u64)
            .max()
            .unwrap();
        return Ok(max_sol);
    }
    Err("could not parse notional".into())
}

pub fn deserialize<T: Clone>(item: &OptionSerializer<T>) -> Option<T> {
    match item {
        OptionSerializer::Some(val) => Some(val.clone()),
        _ => None,
    }
}

pub fn is_tx_a_sol_transfer(tx: &EncodedTransactionWithStatusMeta) -> bool {
    let instructions = self::parse_instructions(tx).unwrap();
    for instruction in instructions {
        if let UiInstruction::Parsed(ix) = instruction {
            match ix {
                UiParsedInstruction::Parsed(ix) => {
                    if ix.program == system_program::id().to_string() && ix.parsed["type"] == "transfer" {
                        return true;
                    }
                }
                UiParsedInstruction::PartiallyDecoded(_) => (),
            }
        }
    }
    false
}

pub fn parse_tx_for_set_compute_unit_price(
    tx: &EncodedTransactionWithStatusMeta,
) -> Result<u64, Box<dyn std::error::Error>> {
    let instructions = self::parse_instructions(tx)?;
    for instruction in instructions {
        if let UiInstruction::Parsed(ix) = instruction {
            match ix {
                UiParsedInstruction::Parsed(_) => (),
                UiParsedInstruction::PartiallyDecoded(compute_budget_ix) => {
                    if compute_budget_ix.program_id == constants::COMPUTE_BUDGET {
                        let data = bs58::decode(&compute_budget_ix.data).into_vec()?;
                        // let value =   borsh::BorshDeserialize::try_from_slice(&data)
                        //     .map_err(|e| e.into())
                        //     .and_then(|ix| match ix {
                        //         ComputeBudgetInstruction::SetComputeUnitPrice(price) => Ok(price),
                        //         _ => Err("not a SetComputeUnitPrice instruction".into()),
                        //     })?;
                        return Ok(0);
                    }
                }
            }
        }
    }
    Err("could not parse compute unit price".into())
}


pub fn is_tx_a_token_transfer(tx: &EncodedTransactionWithStatusMeta) -> bool {
    let instructions = self::parse_instructions(tx).unwrap();
    for instruction in instructions {
        if let UiInstruction::Parsed(ix) = instruction {
            match ix {
                UiParsedInstruction::Parsed(ix) => {
                    if ix.program == constants::TOKEN_PROGRAM_ID && ix.parsed["type"] == "transfer" {
                        return true;
                    }
                }
                UiParsedInstruction::PartiallyDecoded(_) => (),
            }
        }
    }
    false
}

pub fn parse_tx_for_swaps(tx: &EncodedTransactionWithStatusMeta) -> Option<Vec<Swap>> {
    let mut swaps = vec![];
    // From EncodedTransactionWithStatusMeta we need to extract the following:
    // instruction that has Raydium program as a program_id - this is a swap instruction
    // from that instruction we need to extract Input Accounts and Inner Instructions (which are two transfers)
    let versioned_tx = &tx.transaction.decode().unwrap();
    let signature = versioned_tx.signatures[0];
    let signature_str = signature.to_string();
    let ui_message = match &versioned_tx.message {
        VersionedMessage::Legacy(message) => message.encode(UiTransactionEncoding::JsonParsed),
        VersionedMessage::V0(message) => {
            //only legacy for now
            return None;
        }
    };

    let ui_parsed_message = match ui_message {
        UiMessage::Parsed(ui_parsed_message) => ui_parsed_message,
        UiMessage::Raw(_) => return None,
    };

    let tx_accounts = &ui_parsed_message
        .account_keys
        .iter()
        .map(|k| k.pubkey.to_owned())
        .collect::<Vec<String>>();
    for ix in ui_parsed_message.instructions {
        let partially_decoded_ix = match &ix {
            UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(partially_decoded_ix)) => {
                partially_decoded_ix
            }
            _ => return None,
        };
        let instruction_accounts = &partially_decoded_ix.accounts;

        // Raydium swap instruction, contains 2 inner instructions - to/from pool transfers of tokens/wsol
        if partially_decoded_ix.program_id == constants::RAYDIUM_V4_PROGRAM_ID {
            let authority = instruction_accounts[instruction_accounts.len() - 1].clone();
            let mut quote_amount = 0;
            let mut base_amount = 0;
            let mut trade_direction: TradeDirection = TradeDirection::Buy;
            let sol_ata = get_associated_token_address(
                &Pubkey::from_str(&*authority).unwrap(),
                &constants::WSOL_MINT_PUBKEY,
            )
                .to_string();
            if let Some(meta) = &tx.meta {
                if meta.err.is_some() {
                    return None;
                }
                let all_inner_ixs = self::deserialize(&meta.inner_instructions)?;
                for inner_ixs in all_inner_ixs {
                    // might also be identified based on static index 5 but
                    // that would be even more brittle than this
                    if inner_ixs.instructions.len() == 2 {
                        let mut parially_decoded = vec![];
                        for ui_i in inner_ixs.instructions {
                            let parsed = parse_ui_instruction(&ui_i, tx_accounts);
                            if let UiParsedInstruction::PartiallyDecoded(part_decoded_ix) = parsed {
                                let destination_account = &part_decoded_ix.accounts[1];
                                trade_direction = if destination_account == &sol_ata {
                                    TradeDirection::Sell
                                } else {
                                    TradeDirection::Buy
                                };
                                parially_decoded.push(part_decoded_ix);
                            }
                        }
                        for part_decoded_ix in parially_decoded {
                            let data = bs58::decode(&part_decoded_ix.data).into_vec().ok()?;
                            const TRANSFER_INSTRUCTION: u8 = 3;

                            // this is either from Raydium to the user or vise versa
                            if part_decoded_ix.program_id == constants::TOKEN_PROGRAM_ID
                                && !data.is_empty()
                                && data[0] == TRANSFER_INSTRUCTION
                            {
                                // Deserialize the data to get the transfer amount
                                let token_instruction = TokenInstruction::unpack(&data).ok()?;
                                let amount = match token_instruction {
                                    TokenInstruction::Transfer { amount } => amount,
                                    _ => return None,
                                };
                                let source_account = &part_decoded_ix.accounts[0];
                                let destination_account = &part_decoded_ix.accounts[1];
                                let authority_account = &part_decoded_ix.accounts[2];

                                // 2 options
                                // Sell
                                // instruction 1: authority: Raydium, destination: sol_ata, amount  => raydium is sending sol to user, meaning this is a sell
                                // this is a quote_amount
                                // instruction 2: then another intruction is sending token to raydium
                                // this is a base_amount
                                // Buy
                                // instruction 1: authority: not raydium (user), destination: raydium, amount => user is sending token to raydium, meaning this is a buy
                                if authority_account == constants::RAYDIUM_V4_AUTHORITY {
                                    match trade_direction {
                                        TradeDirection::Buy => {
                                            // raydium is sending token to the user
                                            base_amount = amount;
                                        }
                                        TradeDirection::Sell => {
                                            // raydium is sending wsol to the user
                                            quote_amount = amount;
                                        }
                                    }
                                } else {
                                    match trade_direction {
                                        TradeDirection::Buy => {
                                            // user is sending token to raydium
                                            quote_amount = amount;
                                        }
                                        TradeDirection::Sell => {
                                            // user is sending wsol to raydium
                                            base_amount = amount;
                                        }
                                    }
                                };
                            }
                        }
                    }
                }
            }
            let swap = Swap {
                signature,
                pool_id: Pubkey::from_str(&instruction_accounts[1]).unwrap(),
                quote_amount,
                base_amount,
                trade_direction,
            };
            trace!("Parsed swap: {:#?}", swap);
            swaps.push(swap);
        }
    }
    Some(swaps)
}

fn parse_ui_message(ui_msg: &UiMessage) -> Vec<UiInstruction> {
    match ui_msg {
        UiMessage::Parsed(msg) => msg.instructions.clone(),
        UiMessage::Raw(raw_msg) => {
            let instructions = raw_msg
                .instructions
                .iter()
                .map(|ix| {
                    UiInstruction::Compiled(UiCompiledInstruction {
                        program_id_index: ix.program_id_index,
                        accounts: ix.accounts.clone(),
                        data: bs58::encode(&ix.data).into_string(),
                        stack_height: None,
                    })
                })
                .collect();
            instructions
        }
    }
}

pub fn parse_instructions(tx: &EncodedTransactionWithStatusMeta) -> Result<Vec<UiInstruction>> {
    match &tx.transaction {
        EncodedTransaction::Json(ui_tx) => match &ui_tx.message {
            UiMessage::Parsed(msg) => Ok(msg.instructions.clone()),
            UiMessage::Raw(raw_msg) => {
                let instructions = raw_msg
                    .instructions
                    .iter()
                    .map(|ix| {
                        UiInstruction::Compiled(UiCompiledInstruction {
                            program_id_index: ix.program_id_index,
                            accounts: ix.accounts.clone(),
                            data: bs58::encode(&ix.data).into_string(),
                            stack_height: None,
                        })
                    })
                    .collect();
                Ok(instructions)
            }
        },
        _ => bail!("Only EncodedTransaction::Json txs are supported"),
    }
}

pub fn parse_ui_instruction(
    ui_instruction: &UiInstruction,
    account_keys: &[String],
) -> UiParsedInstruction {
    match ui_instruction {
        UiInstruction::Compiled(c) => {
            UiParsedInstruction::PartiallyDecoded(parse_ui_compiled_instruction(c, account_keys))
        }
        UiInstruction::Parsed(p) => match p {
            UiParsedInstruction::PartiallyDecoded(pd) => {
                UiParsedInstruction::PartiallyDecoded(UiPartiallyDecodedInstruction {
                    program_id: pd.program_id.clone(),
                    accounts: pd.accounts.clone(),
                    data: pd.data.clone(),
                    stack_height: None,
                })
            }
            _ => panic!("Unsupported instruction encoding"),
        },
    }
}

pub fn parse_ui_compiled_instruction(
    c: &UiCompiledInstruction,
    account_keys: &[String],
) -> UiPartiallyDecodedInstruction {
    let program_id = account_keys[c.program_id_index as usize].clone();
    let accounts = c
        .accounts
        .iter()
        .map(|i| account_keys[*i as usize].clone())
        .collect();
    UiPartiallyDecodedInstruction {
        program_id,
        accounts,
        data: c.data.clone(),
        stack_height: None,
    }
}
