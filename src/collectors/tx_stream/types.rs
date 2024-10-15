use crate::config::app_context::AppContext;
use crate::config::settings::ProviderName;
use crate::solana::constants;
use spl_token::state::Account as SplTokenAccount;
use solana_sdk::account::Account as SDKTokenAccount;
use solana_sdk::bs58;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, EncodedTransactionWithStatusMeta, UiInnerInstructions, UiInstruction, UiMessage, UiParsedInstruction, UiParsedMessage, UiPartiallyDecodedInstruction, UiTransactionEncoding, UiTransactionStatusMeta};
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;
use serde::ser::SerializeStruct;
use serde::Serialize;
use solana_sdk::account::Account;
use solana_transaction_status::option_serializer::OptionSerializer;
use spl_token::solana_program::program_pack::Pack;
use tokio::sync::{Mutex, RwLock};
use yellowstone_grpc_proto::geyser::SubscribeUpdateTransaction;
use yellowstone_grpc_proto::prelude::{SubscribeUpdateAccount, SubscribeUpdateTransactionStatus};
#[derive(Clone)]
pub enum GeyserFeedEvent {
    Transaction(SubscribeUpdateTransaction),
    TxStatusUpdate(SubscribeUpdateTransactionStatus),
    Account(AccountPretty),
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct TransactionPretty {
    provider_name: ProviderName,
    slot: u64,
    pub(crate) signature: Signature,
    pub(crate) is_vote: bool,
    pub(crate) tx: EncodedTransactionWithStatusMeta,
}

impl TransactionPretty {
    pub fn with_provider_name(mut self, provider_name: ProviderName) -> Self {
        self.provider_name = provider_name;
        self
    }

    pub fn is_successful(&self) -> bool {
        self.tx.meta.as_ref().map_or(false, |meta| meta.status.is_ok())
    }
}

impl fmt::Debug for TransactionPretty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct TxWrap<'a>(&'a EncodedTransactionWithStatusMeta);
        impl<'a> fmt::Debug for TxWrap<'a> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let serialized = serde_json::to_string(self.0).expect("failed to serialize");
                fmt::Display::fmt(&serialized, f)
            }
        }

        f.debug_struct("TransactionPretty")
            .field("slot", &self.slot)
            .field("signature", &self.signature)
            .field("is_vote", &self.is_vote)
            .field("tx", &TxWrap(&self.tx))
            .finish()
    }
}

impl From<SubscribeUpdateTransaction> for TransactionPretty {
    fn from(SubscribeUpdateTransaction { transaction, slot }: SubscribeUpdateTransaction) -> Self {
        let tx = transaction.expect("should be defined");
        Self {
            slot,
            provider_name: "unknown geyser".to_string(),
            signature: Signature::try_from(tx.signature.as_slice()).expect("valid signature"),
            is_vote: tx.is_vote,
            tx: yellowstone_grpc_proto::convert_from::create_tx_with_meta(tx)
                .expect("valid tx with meta")
                .encode(UiTransactionEncoding::Base64, Some(u8::MAX), true)
                .expect("failed to encode"),
        }
    }
}

impl From<EncodedConfirmedTransactionWithStatusMeta> for TransactionPretty {
    fn from(tx: EncodedConfirmedTransactionWithStatusMeta) -> Self {
        let encoded_tx = tx.transaction.transaction;
        let signature_str = match encoded_tx.clone() {
            EncodedTransaction::Json(json_ui_tx) => json_ui_tx.signatures[0].clone(),
            _ => panic!("expected json encoding"),
        };
        Self {
            slot: tx.slot,
            provider_name: "unknown ws".to_string(),
            signature: signature_str.parse().expect("valid signature"),
            is_vote: tx.transaction.meta.is_some(),
            tx: EncodedTransactionWithStatusMeta {
                transaction: encoded_tx,
                meta: tx.transaction.meta,
                version: None,
            },
        }
    }
}

#[derive(Default, Debug, Clone)]
#[allow(dead_code)]
pub struct AccountPretty {
    is_startup: bool,
    slot: u64,
    pub(crate) pubkey: Pubkey,
    pub(crate) lamports: u64,
    owner: Pubkey,
    executable: bool,
    rent_epoch: u64,
    data_encoded: String,
    data: Vec<u8>,
    pub token_unpacked_data: Option<SplTokenAccount>,
    write_version: u64,
    pub(crate) txn_signature: String,
}

impl Serialize for AccountPretty {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("AccountPretty", 10)?;
        state.serialize_field("is_startup", &self.is_startup)?;
        state.serialize_field("slot", &self.slot)?;
        state.serialize_field("pubkey", &self.pubkey.to_string())?;
        state.serialize_field("lamports", &self.lamports)?;
        state.serialize_field("owner", &self.owner.to_string())?;
        state.serialize_field("executable", &self.executable)?;
        state.serialize_field("rent_epoch", &self.rent_epoch)?;
        state.serialize_field("data_encoded", &self.data_encoded)?;
        state.serialize_field("data", &hex::encode(&self.data))?;
        state.serialize_field("token_unpacked_data", &format!("{:?}", self.token_unpacked_data))?;
        state.serialize_field("write_version", &self.write_version)?;
        state.serialize_field("txn_signature", &self.txn_signature)?;
        state.end()
    }
}

impl From<SubscribeUpdateAccount> for AccountPretty {
    fn from(
        SubscribeUpdateAccount {
            is_startup,
            slot,
            account,
        }: SubscribeUpdateAccount,
    ) -> Self {
        let account = account.expect("should be defined");
        Self {
            is_startup,
            slot,
            pubkey: Pubkey::try_from(account.pubkey).expect("valid pubkey"),
            lamports: account.lamports,
            owner: Pubkey::try_from(account.owner).expect("valid pubkey"),
            executable: account.executable,
            rent_epoch: account.rent_epoch,
            data_encoded: hex::encode(account.data.clone()),
            data: account.data.clone(),
            token_unpacked_data: SplTokenAccount::unpack(&account.data).ok(),
            write_version: account.write_version,
            txn_signature: bs58::encode(account.txn_signature.unwrap_or_default()).into_string(),
        }
    }
}


impl From<SDKTokenAccount> for AccountPretty {
    fn from(SDKTokenAccount {
                lamports,
                owner,
                executable,
                rent_epoch,
                data,
            }: SDKTokenAccount) -> Self {
        let token_unpacked_data = SplTokenAccount::unpack(&data).ok();
        let data_encoded = hex::encode(data.clone());
        Self {
            is_startup: false,
            slot: 0,
            pubkey: Pubkey::default(),
            lamports,
            owner,
            executable,
            rent_epoch,
            data_encoded,
            data,
            token_unpacked_data,
            write_version: 0,
            txn_signature: "".to_string(),
        }
    }
}
