use crate::config::app_context::AppContext;
use crate::config::settings::StrategyConfig;
use crate::schema::volumestrategyinstances::completed_at;
use crate::schema::volumestrategyinstances::dsl::volumestrategyinstances;
use crate::schema::*;
use crate::types::actions::{SolanaAction, SwapMethod};
use crate::types::engine::{Strategy, StrategyStatus};
use crate::types::events::{BlockchainEvent, BotEvent, BotEventModel};
use crate::types::keys::KeypairClonable;
use crate::types::pool::{RaydiumPool, RaydiumPoolPriceUpdate, RaydiumSwapEvent};
use crate::types::bot_user::Trader;
use crate::types::volume_strategy::VolumeStrategyInstance;
use crate::utils::helpers::{max_time, zero_time};
use crate::{solana, storage, utils};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::sql_types::*;
use diesel_derives::{Associations, Identifiable, Insertable, Queryable, Selectable};
use futures::stream::{self, StreamExt};
use futures_util::future::join_all;
use once_cell::sync::Lazy;
use serde_derive::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use statig::awaitable::{InitializedStateMachine, IntoStateMachineExt, StateMachine};
use std::any::Any;
use std::collections::{BTreeMap, VecDeque};
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use std::{env, mem};
use std::fmt::{Debug, Formatter};
use log::trace;
use maplit::hashmap;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::field::debug;
use tracing::{debug, error, info, instrument, Event};
use crate::strategies::volume_strategy::VolumeStrategyStateMachine;

// strategy basically manages a collection of position state machines,
// this struct is just a message filter
// The logic is handled on the state machine level, see StrategyState (or PositionState in the case of the sniping strategy)
#[derive(Clone)]
pub struct LoggerInterceptorStrategy {
    context: AppContext,
}

impl Debug for LoggerInterceptorStrategy {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "DbLoggerInterceptor")
    }
}

impl LoggerInterceptorStrategy {
    /// Create a new instance of the strategy.
    pub fn new(context: &AppContext) -> Self {
        Self { context: context.clone() }
    }
}

#[async_trait]
impl Strategy<BotEvent, Arc<Mutex<SolanaAction>>> for LoggerInterceptorStrategy {
    /// Initialize the strategy. This is called once at startup
    async fn sync_state(&mut self) -> Result<()> {
        // collect amounts from all traders to their main wallets
        Ok(())
    }

    // Process incoming signals2
    // #[instrument(skip(self))]
    async fn process_event(&mut self, event: BotEvent) -> Vec<Arc<Mutex<SolanaAction>>> {
        if let Err(e) = match &event {
            BotEvent::HeartBeat(..) => { Ok(()) }
            BotEvent::BlockchainEvent(BlockchainEvent::RaydiumHeartbeatPriceUpdate(price_update)) |
            BotEvent::BlockchainEvent(BlockchainEvent::RaydiumSwapEvent(RaydiumSwapEvent { price_update, .. })) => {
                storage::persistent::save_price_to_db(self.context.db_pool.clone(), price_update.clone()).await
            }
            _ => storage::persistent::save_bot_event_to_db(&self.context.db_pool, event.into()).await
        }
        {
            error!("Failed to save event to db: {:?}", e);
        }
        vec![]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn get_status(&self) -> StrategyStatus {
        StrategyStatus::Running(hashmap! {
            "Running".to_owned() => "ok".to_owned()
        })
    }
}
