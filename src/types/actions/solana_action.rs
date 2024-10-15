use std::collections::BTreeMap;
use crate::types::keys::KeypairClonable;
use crate::types::pool::RaydiumPool;
use crate::schema::*;
use crate::utils;
use chrono::{DateTime, Utc};
use diesel::{sql_types, Associations, Identifiable, Insertable, Queryable};
use serde_derive::{Deserialize, Serialize};
use solana_farm_client::raydium_sdk::{LiquidityPoolKeys, MarketStateLayoutV3};
use solana_sdk::pubkey::Pubkey;
use std::fmt::{Debug, Display, Formatter};
use std::io::Write;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::sql_types::*;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::deserialize::{self, FromSql};
use diesel::result::Error::SerializationError;
use diesel::sql_types::Jsonb;
use diesel_derives::{AsExpression, FromSqlRow};
use solana_sdk::signature::Signature;
use serde_json::Value as JsonValue;
use uuid::Uuid;
use crate::config::constants::ACTION_EXPIRY_S;
use crate::types::actions::{Amount, Asset};
use crate::types::actions::solana_swap_action::SolanaSwapActionPayload;
use crate::types::actions::solana_transfer_action::SolanaTransferActionPayload;
use crate::utils::serdealizers::{SignatureString, JsonbVec, JsonbWrapper};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, AsExpression)]
#[sql_type = "diesel::sql_types::Jsonb"]
pub enum SolanaActionPayload {
    SolanaSwapActionPayload(SolanaSwapActionPayload),
    SolanaTransferActionPayload(SolanaTransferActionPayload),
}

impl ToSql<diesel::sql_types::Jsonb, Pg> for SolanaActionPayload {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let json = serde_json::to_string(self)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        out.write_all(json.as_bytes())?;
        Ok(IsNull::No)
    }
}

#[derive(Debug, Clone, Serialize, AsExpression)]
#[sql_type = "diesel::sql_types::Text"]
pub enum ActionExecutionStatus {
    NotSent,
    Pending,
    Success,
    TxError { error: String },
    Timeout,
}

impl ToSql<sql_types::Text, Pg> for ActionExecutionStatus {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        out.write_all(format!("{:?}", self).as_bytes())?;
        Ok(IsNull::No)
    }
}

#[derive(Debug, Clone, Serialize, AsExpression)]
#[sql_type = "diesel::sql_types::Jsonb"]
pub struct Balance {
    pub sol: u64,
    pub token: BTreeMap<Pubkey, u64>,
}

impl ToSql<Jsonb, Pg> for Balance {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let json = serde_json::to_string(self)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        out.write_all(json.as_bytes())?;
        Ok(IsNull::No)
    }
}

#[derive(Debug, Clone, Queryable, Insertable, Serialize)]
#[table_name = "solana_actions"]
pub struct SolanaAction {
    #[diesel(serialize_as = String)]
    pub uuid: Uuid,
    #[diesel(serialize_as = String)]
    pub sniper: KeypairClonable,
    // main_wallet is used for fees for Max transfers
    #[diesel(serialize_as = String)]
    pub fee_payer: KeypairClonable,
    pub created_at: DateTime<Utc>,
    #[diesel(serialize_as = JsonbVec<SolanaActionPayload>)]
    pub action_payload: Vec<SolanaActionPayload>,
    pub status: ActionExecutionStatus,
    #[diesel(serialize_as = crate::utils::serdealizers::SignatureString)]
    pub tx_hash: Signature,
    pub balance_before: Option<Balance>,
    pub balance_after: Option<Balance>,
    pub fee: i64,
    pub sent_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

impl Display for SolanaAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for action in &self.action_payload {
            match action {
                SolanaActionPayload::SolanaSwapActionPayload(action) => {
                    write!(
                        f,
                        "Swap, uuid: {}, sniper: {:?}, pool: {}, {:?}, amount_in: {:?}",
                        self.uuid,
                        self.sniper,
                        action.keys.id,
                        action.swap_method,
                        action.amount_in
                    )?;
                }
                SolanaActionPayload::SolanaTransferActionPayload(action) => {
                    write!(
                        f,
                        "Transfer, uuid: {}, {:#?}",
                        self.uuid, action
                    )?;
                }
            }
        }
        Ok(())
    }
}

impl SolanaAction {
    pub fn new(
        sniper: KeypairClonable,
        action_payload: Vec<SolanaActionPayload>,
    ) -> Self {
        SolanaAction::new_with_feepayer(sniper.clone(), sniper, action_payload)
    }

    pub fn new_with_feepayer(
        sniper: KeypairClonable,
        fee_payer: KeypairClonable,
        action_payload: Vec<SolanaActionPayload>,
    ) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            sniper,
            fee_payer,
            created_at: Utc::now(),
            action_payload,
            status: ActionExecutionStatus::NotSent,
            tx_hash: Default::default(),
            balance_before: None,
            balance_after: None,
            fee: 0,
            sent_at: None,
            confirmed_at: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        (Utc::now() - &self.created_at).num_seconds() > ACTION_EXPIRY_S as i64
    }

    pub fn sent(&self, balance_before: Balance, signature: Signature, fee: i64) -> Self {
        Self {
            status: ActionExecutionStatus::Pending,
            tx_hash: signature,
            balance_before: Some(balance_before),
            balance_after: None,
            fee,
            sent_at: Some(Utc::now()),
            confirmed_at: None,
            ..self.clone()
        }
    }
}