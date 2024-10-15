use crate::schema::prices;
use crate::solana::tx_parser::Swap;
use chrono::{DateTime, NaiveDateTime, Utc};
use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{IsNull, Output, ToSql};
use diesel::{deserialize, serialize, sql_types, Insertable};
use diesel_derives::{AsExpression, FromSqlRow};
use serde_derive::{Deserialize, Serialize};
use solana_farm_client::raydium_sdk::{get_associated_authority, LiquidityPoolKeys};
use solana_sdk::bs58;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::fmt::Display;
use std::io::Write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradeDirection {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaydiumSwapEvent {
    pub price_update: RaydiumPoolPriceUpdate,
    pub signature: Signature,
    //todo change to Arc<RaydiumPool>
    pub pool: Pubkey,
    pub trade_direction: TradeDirection,
    pub base_amount: f64,
    pub quote_amount: f64,
    pub price: f64,
    //todo currently just a quote_amount but should be denominated in USD
    pub volume: f64,
    pub created_at: chrono::DateTime<Utc>,
}

/// A new block event, containing the block number and hash.
#[derive(Debug, Clone, Serialize, Deserialize, Insertable, Default)]
#[diesel(table_name = prices)]
pub struct RaydiumPoolPriceUpdate {
    // note here pool is String to avoid wrapper aroundPubkey and implementing tosql and fromsql for diesel
    #[diesel(
        serialize_as = crate::utils::serdealizers::PubkeyString,
        deserialize_as = crate::utils::serdealizers::PubkeyString,
    )]
    pub pool: Pubkey,
    pub price: f64,
    pub base_reserve: f64,
    pub quote_reserve: f64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Hash, PartialEq, Eq)]
pub struct RaydiumPool {
    pub id: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub lp_mint: Pubkey,
    pub base_decimals: u8,
    pub quote_decimals: u8,
    pub lp_decimals: u8,
    pub version: u8,
    pub program_id: Pubkey,
    pub authority: Pubkey,
    pub open_orders: Pubkey,
    pub target_orders: Pubkey,
    pub base_vault: Pubkey,
    pub quote_vault: Pubkey,
    pub withdraw_queue: Pubkey,
    pub lp_vault: Pubkey,
    pub market_version: u8,
    pub market_program_id: Pubkey,
    pub market_id: Pubkey,
    pub lp_reserve: u64,
    pub open_time: u64,
    pub reverse_pool: bool,
    pub freeze_authority: Option<Pubkey>,
}

impl RaydiumPool {
    pub fn to_liquidity_keys(&self) -> LiquidityPoolKeys {
        LiquidityPoolKeys {
            id: self.id,
            base_mint: self.base_mint,
            quote_mint: self.quote_mint,
            lp_mint: self.lp_mint,
            base_decimals: self.base_decimals,
            quote_decimals: self.quote_decimals,
            lp_decimals: self.lp_decimals,
            version: self.version,
            program_id: self.program_id,
            authority: self.authority,
            open_orders: self.id,
            target_orders: self.id,
            base_vault: if self.reverse_pool {
                self.quote_vault
            } else {
                self.base_vault
            },
            quote_vault: if self.reverse_pool {
                self.base_vault
            } else {
                self.quote_vault
            },
            withdraw_queue: self.withdraw_queue,
            lp_vault: self.lp_vault,
            market_version: self.market_version,
            market_program_id: self.id,
            market_id: self.id,
            market_authority: self.id,
            // market_authority: get_associated_authority(&self.market_program_id, &self.market_id).unwrap(),
            market_base_vault: self.id,
            market_quote_vault: self.id,
            market_bids: self.id,
            market_asks: self.id,
            market_event_queue: self.id,
        }
    }
}
