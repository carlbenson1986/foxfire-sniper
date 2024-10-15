use std::collections::HashSet;
use std::sync::Arc;
use futures::stream::{self, StreamExt};
use solana_sdk::instruction::Instruction;
use tokio::sync::Mutex;
use anyhow::{anyhow, bail, Result};
use solana_farm_client::raydium_sdk::{make_swap_fixed_in_instruction, LiquiditySwapFixedInInstructionParamsV4, UserKeys};
use solana_sdk::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use spl_associated_token_account::instruction::create_associated_token_account;
use spl_token::solana_program::system_instruction;
use tracing::{debug, error, trace};
use tracing::field::debug;
use crate::config::app_context::AppContext;
use crate::config::constants::{BASE_TX_FEE_SOL, NEW_ACCOUNT_THRESHOLD_SOL, RENT_EXEMPTION_THRESHOLD_SOL};
use crate::solana;
use crate::solana::constants::WSOL_MINT_PUBKEY;
use crate::types::actions::{Amount, Asset, SolanaAction, SwapMethod, SolanaActionPayload, SolanaSwapActionPayload, SolanaTransferActionPayload, Balance};
use crate::types::events::ExecutionError;

// todo currently only one token per sniper is supported,
pub async fn build_instructions(context: &AppContext, action: &Arc<Mutex<SolanaAction>>, price_per_cu_microlamports: u64, compute_units_per_tx: u32) -> Result<(Balance, Vec<Instruction>)> {
    let action_guard = action.lock().await;
    // accounts involved
    let sniper_pubkey = action_guard.sniper.pubkey();
    let fee_payer = action_guard.fee_payer.pubkey();

    // Estimate SOL and inventory before the action
    // 1. Transfer/Swap normally (leave rent exemption threshold)
    // 2. Transfer only and sweep completely, separate fee payer

    // sol_balance is the balance that's going to be reduced by the action
    let sol_balance = solana::get_balance(context, &action_guard.sniper.pubkey()).await.unwrap_or(0);
    if sol_balance == 0 {
        bail!(ExecutionError::ZeroSolBalance);
    }
    let tokens: HashSet<Pubkey> = get_tokens_used_in_tx(&action_guard).await;

    if tokens.len() > 1 {
        bail!(ExecutionError::SeveralTokensInOneTx);
    }
    let token_mint = tokens.iter().next().cloned().unwrap_or_default();
    let token_balance = solana::get_token_balance(context, &sniper_pubkey, &token_mint).await.unwrap_or(0);
    let sniper_token_ata = spl_associated_token_account::get_associated_token_address(&sniper_pubkey, &token_mint);
    let sniper_wsol_ata = spl_associated_token_account::get_associated_token_address(&sniper_pubkey, &WSOL_MINT_PUBKEY);

    // Calculate the SOL amount to spend
    // 1. Transfer: (5000 base fee + priority fee) = optimal fee
    // 1.a. transfer SOL:  if transfer Max amount, then fee payer is the main wallet, also sweeping from the token ATA
    // 1.b. transfer SOL: if transfer to the newly created account (not exists in the system) then add account creation fee (rent exemption threshold)
    // 1.c. transfer TOKEN: threshold is different for newly created account, should create ATA and make it rent exempt.
    // 2. Swap (5000 base fee + priority fee) = optimal swap fee
    // 2.a. swap SOL: if swap Max amount, then less fees, keep an
    // 2.b. swap TOKEN:
    let tx_fee = BASE_TX_FEE_SOL + (price_per_cu_microlamports * compute_units_per_tx as u64) / 1_000_000 + 1;
    debug!("Fee to use: {}", tx_fee);

    // Minimum balance to keep:
    // Transfer SOL
    // 1. If Max and closing account: 0, but add account closure instruction.
    // 2. Else (either Max with keeping account or exact amount still keep account): account_creation_threshold (which is basically rent exempt for 128 bytes for SOL only) + transfer fee to send this SOL later on. (if there's tokens on ATA, always keep + one more transfer fee as well)
    // Transfer TOKEN
    // 3. If Max and closing account: 0, but add account closure instruction.
    // 4. Keep token rent_exempt account on ATA account (!)
    // 1.a. ... + account_creation_threshold (normal case)
    // 1.b. ... + 0 (if Max and sweeping, main_wallet_is the payer)
    // 1.c. ... + account_creation_threshold (if transfer to the new account)

    // checking before processing to subtract fees
    if sol_balance < tx_fee {
        bail!(ExecutionError::NotEnoughSolBalance(tx_fee, sol_balance));
    }

    let balance_before = Balance {
        sol: sol_balance,
        //todo update this for a few tokens for each wallet
        token: tokens.iter().map(|t| (*t, token_balance)).collect(),
    };

    // Prepare instructions for transfers
    let mut action_ixs = vec![];
    let mut sol_balance_pointer = sol_balance;
    let mut tx_fee_pointer = tx_fee;
    let mut token_balance_pointer = token_balance;
    let mut min_sol_to_keep_after_pointer = 0;
    // optimal_fee should already include BASE_FEE
    for (i, step) in action_guard.action_payload.iter().enumerate() {
        // step_ixs is the list of instructions for the step
        // sol_cost is the amount of SOL to spend in this step, EXCLUDING fees - fees are the feature of the tx, not the step
        // rent exempt is part of sol_cost in created in the step
        if let Some((step_ixs, sol_cost, token_cost, min_sol_to_keep_after)) = match step {
            SolanaActionPayload::SolanaTransferActionPayload(transfer) => {
                match transfer.asset {
                    Asset::Sol => {
                        match transfer.amount {
                            Amount::Exact(amount) => if amount > 0 {
                                Some((vec![solana_sdk::system_instruction::transfer(&sniper_pubkey, &transfer.receiver, amount)],
                                      amount + tx_fee_pointer, 0, 0))
                            } else { None },
                            Amount::ExactWithFees(amount) => if amount > 0 {
                                let amt_to_transfer = amount - tx_fee_pointer;
                                Some((
                                    vec![solana_sdk::system_instruction::transfer(&sniper_pubkey, &transfer.receiver, amt_to_transfer)],
                                    amt_to_transfer + tx_fee_pointer, 0, 0))
                            } else { None },
                            Amount::Max | Amount::MaxAndClose =>
                                Some((
                                    // todo double check if this works for a few instructions in a tx,
                                    // sol_balance_pointer doesn't work here although some transfers already completed sol_balance
                                    vec![solana_sdk::system_instruction::transfer(&sniper_pubkey, &transfer.receiver, sol_balance)],
                                    sol_balance_pointer - tx_fee_pointer, 0, 0)),
                            Amount::MaxButLeaveForTransfer => if sol_balance_pointer > tx_fee + tx_fee_pointer {
                                let amt_to_transfer = sol_balance_pointer - tx_fee - tx_fee_pointer;
                                Some((
                                    vec![solana_sdk::system_instruction::transfer(&sniper_pubkey, &transfer.receiver, amt_to_transfer)],
                                    amt_to_transfer + tx_fee_pointer, 0, tx_fee))
                            } else { None },
                        }
                    }
                    Asset::Token(token_pubkey) => {
                        let receiver_ata = spl_associated_token_account::get_associated_token_address(&transfer.receiver, &token_mint);
                        let mut token_amt = match transfer.amount {
                            Amount::Exact(amount) | Amount::ExactWithFees(amount) | Amount::ExactWithFees(amount) =>
                                amount,
                            Amount::MaxButLeaveForTransfer | Amount::Max | Amount::MaxAndClose => token_balance_pointer
                        };
                        if token_amt > 0 {
                            let mut sol_to_budget = 0;
                            let mut token_transfer_ixs = vec![];
                            // create an account if doensn't exist
                            if !solana::is_account_exist(context, &receiver_ata).await {
                                debug!("Creating ATA for the receiver, fee payer: {:?}, ata: {:?}", fee_payer, receiver_ata);
                                // token_transfer_ixs.push(solana_sdk::system_instruction::transfer(&fee_payer, &receiver_ata, RENT_EXEMPTION_THRESHOLD_SOL));
                                token_transfer_ixs.push(create_associated_token_account(
                                    &fee_payer, // The account that will fund the ATA creation
                                    &transfer.receiver,   // The account that will own the ATA
                                    &token_pubkey,  // The mint of the token
                                    &spl_token::ID, // TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
                                ));

                                sol_to_budget += RENT_EXEMPTION_THRESHOLD_SOL;
                            }

                            token_transfer_ixs.push(spl_token::instruction::transfer(
                                &spl_token::ID,
                                &sniper_token_ata,
                                &receiver_ata,
                                &sniper_pubkey,
                                &[],
                                token_amt,
                            )?);

                            if transfer.amount == Amount::MaxAndClose {
                                token_transfer_ixs.push(spl_token::instruction::close_account(
                                    &spl_token::ID,
                                    &sniper_token_ata,
                                    &fee_payer,
                                    &sniper_pubkey,
                                    &[],
                                )?);
                            }

                            Some((token_transfer_ixs, sol_to_budget, token_amt, 0))
                        } else { None }
                    }
                }
            }
            SolanaActionPayload::SolanaSwapActionPayload(swap) => {
                if !(swap.keys.base_mint != *WSOL_MINT_PUBKEY && swap.keys.quote_mint == *WSOL_MINT_PUBKEY) {
                    bail!(ExecutionError::UnsupportedPool(swap.keys.base_mint.to_string(), swap.keys.quote_mint.to_string()));
                }
                // swapping
                let user_keys = match swap.swap_method {
                    SwapMethod::BuyTokensForExactSol => {
                        // reversed pool
                        if swap.keys.base_mint == *WSOL_MINT_PUBKEY {
                            UserKeys::new(
                                sniper_token_ata,
                                sniper_wsol_ata,
                                sniper_pubkey,
                            )
                        } else {
                            UserKeys::new(
                                sniper_wsol_ata,
                                sniper_token_ata,
                                sniper_pubkey,
                            )
                        }
                    }
                    SwapMethod::SellExactTokensForSol => {
                        if swap.keys.base_mint == *WSOL_MINT_PUBKEY {
                            UserKeys::new(
                                sniper_wsol_ata,
                                sniper_token_ata,
                                sniper_pubkey,
                            )
                        } else {
                            UserKeys::new(
                                sniper_token_ata,
                                sniper_wsol_ata,
                                sniper_pubkey,
                            )
                        }
                    }
                };

                match swap.swap_method {
                    SwapMethod::BuyTokensForExactSol => {
                        let ata_creation_fee = if !solana::is_account_exist(context, &sniper_token_ata).await {
                            RENT_EXEMPTION_THRESHOLD_SOL
                        } else { 0 };
                        debug!("ata_creation_fee: {}", ata_creation_fee);
                        let wsol_ata_creation_fee_reimbursed = if !solana::is_account_exist(context, &sniper_wsol_ata).await {
                            RENT_EXEMPTION_THRESHOLD_SOL
                        } else { 0 };
                        debug!("wsol_ata_creation_fee_reimbursed: {}", wsol_ata_creation_fee_reimbursed);
                        let mut swap_sol_amount_in = match swap.amount_in {
                            // just swapping what is in the payload
                            Amount::Exact(amount) => amount,
                            // swapping what is in the payload less fees for the swap, and fees for the further transfer
                            // wrapping sol required another RENT_EXEMPTION_THRESHOLD_SOL but it's reimbursed on closing wsol with the same txs
                            Amount::ExactWithFees(amount) => {
                                let fees = tx_fee_pointer + ata_creation_fee + wsol_ata_creation_fee_reimbursed;
                                if amount > sol_balance_pointer {
                                    bail!(ExecutionError::NotEnoughSolBalance(amount, sol_balance_pointer));
                                }
                                amount - fees
                            }
                            Amount::Max => {
                                let amt_needed = tx_fee_pointer + ata_creation_fee + wsol_ata_creation_fee_reimbursed;
                                if amt_needed > sol_balance_pointer {
                                    error!(" Amount::Max amt_needed > sol_balance_pointer check failed, amt_needed: {}, sol_balance_pointer: {}", amt_needed, sol_balance_pointer);
                                    bail!(ExecutionError::NotEnoughSolBalance(amt_needed, sol_balance_pointer));
                                }
                                sol_balance_pointer - amt_needed
                            }
                            Amount::MaxButLeaveForTransfer => {
                                let amt_needed = tx_fee_pointer + tx_fee + ata_creation_fee + wsol_ata_creation_fee_reimbursed;
                                if amt_needed > sol_balance_pointer {
                                    error!("Amount::MaxButLeaveForTransfer amt_needed > sol_balance_pointer check failed, amt_needed: {}, sol_balance_pointer: {}, amt_needed = tx_fee_pointer {tx_fee_pointer} + tx_fee {tx_fee} +ata_creation_fee {ata_creation_fee} + wsol_ata_creation_fee_reimbursed {wsol_ata_creation_fee_reimbursed}", amt_needed, sol_balance_pointer);
                                    bail!(ExecutionError::NotEnoughSolBalance(amt_needed, sol_balance_pointer));
                                }
                                sol_balance_pointer - amt_needed
                            }
                            // feepayer is paying for the fees, so no need to keep extra
                            Amount::MaxAndClose => sol_balance_pointer,
                        };
                        debug!("amount_in_sol: {}", swap_sol_amount_in);
                        if swap_sol_amount_in > 0 {
                            let mut sol_to_budget = 0;
                            let mut token_transfer_ixs = vec![];
                            // create an account if doensn't exist
                            if !solana::is_account_exist(context, &sniper_token_ata).await {
                                token_transfer_ixs.push(create_associated_token_account(
                                    &fee_payer, // The account that will fund the ATA creation
                                    &sniper_pubkey,   // The account that will own the ATA
                                    &token_mint,  // The mint of the token
                                    &spl_token::ID, // TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
                                ));
                                sol_to_budget += RENT_EXEMPTION_THRESHOLD_SOL;
                            }
                            // creating wsol if doesn't exist
                            if !solana::is_account_exist(context, &sniper_wsol_ata).await {
                                token_transfer_ixs.push(create_associated_token_account(
                                    &fee_payer, // The account that will fund the ATA creation
                                    &sniper_pubkey,   // The account that will own the ATA
                                    &WSOL_MINT_PUBKEY,  // The mint of the token
                                    &spl_token::ID, // TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
                                ));
                                // we dont' increase sol_to_budget here, because it's reimbursed on closing the account
                            }

                            // wrapping sol to wsol, swapping, unwrapping what's left
                            token_transfer_ixs.extend_from_slice(&[
                                system_instruction::transfer(&sniper_pubkey, &sniper_wsol_ata, swap_sol_amount_in),
                                spl_token::instruction::sync_native(&spl_token::ID, &sniper_wsol_ata)?,
                                make_swap_fixed_in_instruction(
                                    LiquiditySwapFixedInInstructionParamsV4::new(
                                        swap.keys.clone(),
                                        user_keys,
                                        swap_sol_amount_in,
                                        // todo having min_amount_out 0 is generally a bad idea
                                        0,
                                    ), swap.keys.version),
                                spl_token::instruction::close_account(
                                    &spl_token::ID,
                                    &sniper_wsol_ata,
                                    &fee_payer,
                                    &sniper_pubkey,
                                    &[],
                                )?
                            ]);

                            Some((token_transfer_ixs, swap_sol_amount_in + sol_to_budget, 0, RENT_EXEMPTION_THRESHOLD_SOL))
                        } else { None }
                    }
                    SwapMethod::SellExactTokensForSol => {
                        // it all doesn't make much sense for the token - either balance or the amount provided
                        let mut amount_in = match swap.amount_in {
                            Amount::Exact(amount) => amount,
                            Amount::ExactWithFees(amount) => amount,
                            Amount::Max => token_balance_pointer,
                            Amount::MaxButLeaveForTransfer => token_balance_pointer,
                            Amount::MaxAndClose => token_balance_pointer,
                        };
                        if amount_in > 0 {
                            let mut token_transfer_ixs = vec![];
                            // creating wsol if doesn't exist
                            if !solana::is_account_exist(context, &sniper_wsol_ata).await {
                                token_transfer_ixs.push(create_associated_token_account(
                                    &fee_payer, // The account that will fund the ATA creation
                                    &sniper_pubkey,   // The account that will own the ATA
                                    &WSOL_MINT_PUBKEY,  // The mint of the token
                                    &spl_token::ID, // TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
                                ));
                                // we dont' increase sol_to_budget here, because it's reimbursed on closing the account
                            }
                            // swapping

                            // wrapping sol to wsol, swapping, unwrapping what's left
                            token_transfer_ixs.extend_from_slice(&[
                                spl_token::instruction::sync_native(&spl_token::ID, &sniper_wsol_ata)?,
                                make_swap_fixed_in_instruction(
                                    LiquiditySwapFixedInInstructionParamsV4::new(
                                        swap.keys.clone(),
                                        user_keys,
                                        amount_in,
                                        // todo having min_amount_out 0 is generally a bad idea
                                        swap.min_amount_out,
                                    ), swap.keys.version),
                                spl_token::instruction::close_account(
                                    &spl_token::ID,
                                    &sniper_wsol_ata,
                                    &fee_payer,
                                    &sniper_pubkey,
                                    &[],
                                )?
                            ]);


                            Some((token_transfer_ixs, 0, amount_in, 0))
                        } else { None }
                    }
                }
            }
        } {
            // if tx_fee > sol_balance_pointer {
            //     error!(" tx_fee > sol_balance_pointer check failed, tx_fee: {}, sol_balance_pointer: {}", tx_fee, sol_balance_pointer);
            //     bail!(ExecutionError::NotEnoughSolBalance(tx_fee, sol_balance_pointer));
            // }
            // if sol_cost + tx_fee + min_sol_to_keep_after_pointer > sol_balance_pointer {
            //     error!(" sol_cost + tx_fee + min_sol_to_keep_after_pointer > sol_balance_pointer check failed, sol_cost: {}, tx_fee: {}, min_sol_to_keep_after_pointer: {}, sol_balance_pointer: {}", sol_cost, tx_fee, min_sol_to_keep_after_pointer, sol_balance_pointer);
            //     bail!(ExecutionError::NotEnoughSolBalance(sol_cost + tx_fee + min_sol_to_keep_after, sol_balance_pointer));
            // }
            if token_cost > token_balance_pointer {
                error!(" token_cost > token_balance_pointer check failed, token_cost: {}, token_balance_pointer: {}", token_cost, token_balance_pointer);
                bail!(ExecutionError::NotEnoughTokenBalance(token_cost + token_balance_pointer,
                token_balance));
            }
            action_ixs.extend(step_ixs);
            // Simulating fee deduction
            // we just ignore add if for some reason zero amount is transferred or swapped {
            // we use fee only once during the first iteration
            if tx_fee_pointer > 0 {
                tx_fee_pointer = 0;
                sol_balance_pointer -= tx_fee;
            }
            if sol_cost < sol_balance_pointer {
                sol_balance_pointer -= sol_cost;
            } else {
                sol_balance_pointer = 0;
            }
            token_balance_pointer -= token_cost;
            min_sol_to_keep_after_pointer += min_sol_to_keep_after;
            trace!("queued ix {} of {} {:#?}: post isx sol_balance: {}, token_balance: {}, min_sol_to_keep_after_pointer: {}, ix step: {:?}", i,  action_guard.action_payload.len(), step,sol_balance_pointer, token_balance_pointer, min_sol_to_keep_after_pointer,step);
        }
    }
    Ok((balance_before, action_ixs))
}


async fn get_tokens_used_in_tx(action_guard: &SolanaAction) -> HashSet<Pubkey> {
    action_guard.action_payload.iter()
        .filter_map(|s| {
            match s {
                SolanaActionPayload::SolanaTransferActionPayload(solana_transfer_action_payload) => {
                    match solana_transfer_action_payload.asset {
                        Asset::Token(token_pubkey) => Some(token_pubkey),
                        _ => None,
                    }
                }
                SolanaActionPayload::SolanaSwapActionPayload(solana_swap_action_payload) => {
                    if solana_swap_action_payload.keys.base_mint != *WSOL_MINT_PUBKEY {
                        Some(solana_swap_action_payload.keys.base_mint)
                    } else {
                        Some(solana_swap_action_payload.keys.quote_mint)
                    }
                }
            }
        })
        .collect()
}

pub async fn estimate_cu_per_tx(context: &AppContext, action: &Arc<Mutex<SolanaAction>>) -> u32 {
    const ESTIMATE_CU_PER_COMPUTE_BUDGET_INSTRUCTION: u32 = 300;
    const ESTIMATE_CU_SOL_TRANSFER: u32 = 150;
    const ESTIMATE_CU_SPL_TOKEN_TRANSFER: u32 = 35000;
    const ESTIMATE_CU_BUY_SPL_RAYDIUM_V4: u32 = 80000;
    const ESTIMATE_CU_SELL_SPL_RAYDIUM_V4: u32 = 80000;

    action.lock().await.action_payload.iter().map(|s| {
        match s {
            SolanaActionPayload::SolanaTransferActionPayload(solana_transfer_action_payload) => {
                match solana_transfer_action_payload.asset {
                    Asset::Sol => ESTIMATE_CU_SOL_TRANSFER,
                    Asset::Token(_) => ESTIMATE_CU_SPL_TOKEN_TRANSFER,
                }
            }
            SolanaActionPayload::SolanaSwapActionPayload(solana_swap_action_payload) => {
                match solana_swap_action_payload.swap_method {
                    SwapMethod::BuyTokensForExactSol => ESTIMATE_CU_BUY_SPL_RAYDIUM_V4 + ESTIMATE_CU_SPL_TOKEN_TRANSFER,
                    SwapMethod::SellExactTokensForSol => ESTIMATE_CU_BUY_SPL_RAYDIUM_V4,
                }
            }
        }
    }).sum::<u32>() + ESTIMATE_CU_PER_COMPUTE_BUDGET_INSTRUCTION
}
