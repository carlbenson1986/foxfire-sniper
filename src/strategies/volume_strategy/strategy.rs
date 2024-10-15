use crate::config::app_context::AppContext;
use crate::config::settings::StrategyConfig;
use crate::schema::volumestrategyinstances::completed_at;
use crate::schema::volumestrategyinstances::dsl::volumestrategyinstances;
use crate::schema::*;
use crate::types::actions::{SolanaAction, SwapMethod};
use crate::types::engine::{Strategy, StrategyStatus};
use crate::types::events::{BlockchainEvent, BotEvent};
use crate::types::keys::KeypairClonable;
use crate::types::pool::{RaydiumPool, RaydiumPoolPriceUpdate};
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
pub struct VolumeStrategy {
    pub state_machine: StateMachine<VolumeStrategyStateMachine>,
}

impl Debug for VolumeStrategy {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "VolumeStrategy, config: {:?}, state: {:?}",
               self.state_machine.instance,
               self.state_machine.state()
        )
    }
}

impl VolumeStrategy {
    /// Create a new instance of the strategy.
    pub async fn new(
        context: &AppContext,
        strategy_config: &VolumeStrategyInstance,
    ) -> Result<Self> {
        let state_machine = VolumeStrategyStateMachine::new(context, strategy_config)
            .await?
            .state_machine();
        Ok(Self {
            state_machine,
        })
    }

    pub fn get_user_id(&self) -> i32 {
        self.state_machine.instance.user_id
    }
}

#[async_trait]
impl Strategy<BotEvent, Arc<Mutex<SolanaAction>>> for VolumeStrategy {
    /// Initialize the strategy. This is called once at startup
    async fn sync_state(&mut self) -> Result<()> {
        // collect amounts from all traders to their main wallets
        Ok(())
    }

    // Process incoming signals2
    // #[instrument(skip(self))]
    async fn process_event(&mut self, event: BotEvent) -> Vec<Arc<Mutex<SolanaAction>>> {
        match &event {
            BotEvent::ExecutionResult(..) => {
                self.state_machine.handle(&event.clone().into()).await;
            }
            BotEvent::HeartBeat(..) => {
                self.state_machine.handle(&event.clone().into()).await;
            }
            BotEvent::BlockchainEvent(_) => {
                self.state_machine.handle(&event.clone().into()).await;
            }
            _ => {}
        }
        // Lock the mutex to get mutable access
        let mut strat_actions_generated_from_event = self.state_machine.strat_actions_generated_from_event.lock().await;
        let actions = std::mem::replace(&mut *strat_actions_generated_from_event, Vec::new());
        actions
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn get_status(&self) -> StrategyStatus {
        StrategyStatus::Running(hashmap! {
            "Running".to_owned() => format!("{:?}", self.state_machine.state()),
        })
    }
}
