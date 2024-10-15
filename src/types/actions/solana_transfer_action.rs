use serde_derive::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use crate::types::keys::KeypairClonable;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Amount {
    Exact(u64),
    // No more than this amount
    ExactWithFees(u64),
    // Max of everything but leave some SOL for ONE transaction which is a transfer of either SOL or a token, or both 
    MaxButLeaveForTransfer,
    //Sweep either SOL or token completely, account is not closed (rent exempt is kept on the ATA)
    Max,
    // Sweep and close, valid for token only - ATA is closed and SOL reimbursed to the main wallet, 
    // for SOL works as Max but it uses main_wallet for fees
    MaxAndClose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Asset {
    Sol,
    Token(Pubkey),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolanaTransferActionPayload {
    //can be few transfers in one transaction
    pub asset: Asset,
    pub receiver: Pubkey,
    pub amount: Amount,
}
