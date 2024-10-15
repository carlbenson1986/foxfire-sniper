use crate::aggregators::every_tick_indicators::{EveryTickIndicators, EveryTickIndicatorsValue};
use crate::aggregators::period_indicators::{TickBarValue, TickBarWithPeriod};
use crate::schema::*;
use crate::types::actions::SolanaAction;
use crate::types::pool::{RaydiumPool, RaydiumPoolPriceUpdate, RaydiumSwapEvent};
use crate::collectors::tx_stream::types::AccountPretty;
use crate::utils::serdealizers::JsonbWrapper;
use chrono::{DateTime, Utc};
use diesel::Insertable;
use serde::{Deserialize, Serialize};
use solana_farm_client::raydium_sdk::{
    get_associated_authority, LiquidityPoolKeys, LiquidityStateV4,
};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::TransactionError;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::Arc;
use anyhow::anyhow;
use serde::ser::SerializeStruct;
use strum_macros::Display;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::Instant;
use uuid::Uuid;
use yata::core::{IndicatorResult, PeriodType, ValueType};
use yata::methods::TEMA;
use yata::prelude::Candle;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::sql_types::*;

/// Convenience enum containing all the events that can be emitted by collectors.
type TokenBurned = u64;
pub type TickSizeMs = u64;
#[derive(Debug, Clone, Serialize)]
pub enum BlockchainEvent {
    AccountUpdate(AccountPretty),
    Deposit(String, Pubkey, u64),
    Withdrawal(String, Pubkey, u64),
    ExecutionReceipt(ExecutionReceipt),
    RaydiumHeartbeatPriceUpdate(RaydiumPoolPriceUpdate),
    RaydiumSwapEvent(RaydiumSwapEvent),
    RaydiumNewPoolEvent(RaydiumPool, RaydiumPoolPriceUpdate),
    //below not implemented
    PumpFunCurveUpdate(RaydiumSwapEvent),
    PumpFunSwapDetails(RaydiumSwapEvent),
    LiquidityRemoved,
    PumpFunTokenDeployedTo,
    RaydiumLiquidityTokensBurnedOn,
}
#[derive(Debug, Clone, Serialize)]
pub enum DerivedEvent {
    TickIndicatorEvent(Pubkey, EveryTickIndicatorsValue),
    TickBarEvent(Pubkey, TickBarValue),
}

#[derive(Debug, Clone)]
pub struct BarEvent {
    pub pool_id: Pubkey,
    pub period: PeriodType,
    pub bar: Candle,
}


//todo todo move that to tailored Error types
#[derive(Error, Debug, Clone, Serialize)]
pub enum ExecutionError {
    #[error("Action is too old to execute")]
    ActionTooOld,
    #[error("Nothng to execute")]
    NoInstructionsGenerated,
    #[error("Currently only one token per tx is supported")]
    SeveralTokensInOneTx,
    #[error("SOL balance is zero")]
    ZeroSolBalance,
    #[error("Not enough SOL, {0} SOL required, but balance is only {1} SOL")]
    NotEnoughSolBalance(u64, u64),
    #[error("Not enough token balance to make transfers {0}, but balance is only {1}")]
    NotEnoughTokenBalance(u64, u64),
    #[error("Unsupported pair: base_mint: {0} , quote_mint: {1}")]
    UnsupportedPool(String, String),
    #[error("SimulationFailed: {0}")]
    SimulationFailed(String),
    #[error("Failed to build instructions: {0}")]
    Other(String),
}

#[derive(Debug, Clone, Serialize)]
pub enum ExecutionResult {
    Sent,
    ExecutionError(ExecutionError),
}

#[derive(Debug, Clone, Serialize)]
pub enum SystemEvent {
    DestroyStrategy(i32),
    Stop,
}

#[derive(Debug, Clone)]
pub enum BotEvent {
    HeartBeat(TickSizeMs, Instant),
    BlockchainEvent(BlockchainEvent),
    DerivedEvent(DerivedEvent),
    //todo todo move that to Error types
    ExecutionResult(Uuid, Arc<Mutex<SolanaAction>>, ExecutionResult),
    SystemEvent(SystemEvent),
}


impl Serialize for BotEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("BotEvent", 2)?;
        match self {
            BotEvent::HeartBeat(tick_size_ms, instant) => {
                state.serialize_field("type", "HeartBeat")?;
                state.serialize_field("tick_size_ms", tick_size_ms)?;
                state.serialize_field("instant", &instant.elapsed().as_millis())?;
            }
            BotEvent::BlockchainEvent(event) => {
                state.serialize_field("type", "BlockchainEvent")?;
                state.serialize_field("event", event)?;
            }
            BotEvent::DerivedEvent(event) => {
                state.serialize_field("type", "DerivedEvent")?;
                state.serialize_field("event", event)?;
            }
            BotEvent::ExecutionResult(action_uuid, _, result) => {
                state.serialize_field("type", "ExecutionResult")?;
                state.serialize_field("action_uuid", action_uuid)?;
                state.serialize_field("result", result)?;
            }
            BotEvent::SystemEvent(event) => {
                state.serialize_field("type", "SystemEvent")?;
                state.serialize_field("event", event)?;
            }
        }
        state.end()
    }
}

#[derive(Debug, Clone, Serialize, Insertable)]
#[diesel(table_name = bot_events)]
pub struct BotEventModel {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    #[diesel(serialize_as = JsonbWrapper<BotEvent>)]
    pub event_data: BotEvent,
}

impl From<BotEvent> for BotEventModel {
    fn from(event: BotEvent) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type: format!("{:?}", event),
            event_data: event,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    //todo mov all that to Arc<Mutex<SolanaAction>>
    pub action_uuid: Uuid,
    pub transaction_signature: Signature,
    pub err: Option<TransactionError>,
    pub status_changed_at: chrono::DateTime<Utc>,
}

impl ExecutionReceipt {
    pub fn new(
        action_uuid: Uuid,
        transaction_signature: Signature,
        err: Option<TransactionError>,
    ) -> Self {
        Self {
            action_uuid,
            transaction_signature,
            err,
            status_changed_at: Utc::now(),
        }
    }
}
