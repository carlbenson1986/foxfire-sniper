use std::collections::{BTreeMap, HashMap};
use crate::config::app_context::AppContext;
use crate::config::constants::{BASE_TX_FEE_SOL, MAX_TRANSFERS_IN_ONE_TX, NEW_ACCOUNT_THRESHOLD_SOL, RAYDIUM_SWAP_FEE, RENT_EXEMPTION_THRESHOLD_SOL};
use crate::schema::traders;
use crate::schema::traders::strategy_instance_id;
use crate::schema::traders::dsl::traders as traders_dsl;
use crate::schema::volumestrategyinstances;
use crate::schema::volumestrategyinstances::dsl::volumestrategyinstances as volumestrategyinstances_dsl;
use crate::schema::volumestrategyinstances::id as volumestrategyinstances_id;
use crate::schema::users::dsl::users;
use crate::schema::users::{chat_id, id as users_id};
use crate::types::actions::{Amount, Asset, SolanaAction, SolanaActionPayload, SolanaTransferActionPayload};
use crate::types::events::{BlockchainEvent, BotEvent, TickSizeMs};
use crate::types::keys::KeypairClonable;
use crate::types::pool::RaydiumPool;
use crate::types::bot_user::{BotUser, Trader};
use crate::types::volume_strategy::VolumeStrategyInstance;
use crate::types::sniping_strategy::SnipingStrategyInstance;
use crate::utils::Stopwatch;
use crate::strategies::sniper_strategy::agent::{self, SniperAgentState};
use crate::strategies::events::{AgentEvent, SolanaStrategyEvent};
use anyhow::Result;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use futures::executor;
use futures_util::future::join_all;
use futures_util::{stream, StreamExt};
use log::{debug, error};
use solana_sdk::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use statig::awaitable::{prelude::*, StateMachine};
use std::sync::{Arc, Mutex as StdMutex};
use teloxide::prelude::{ChatId, Requester};
use tokio::sync::{Mutex as TokioMutex, Mutex};
use tokio::time::sleep;
use tracing::{info, trace, warn};
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use tracing::field::debug;
use crate::solana;
use crate::tg_bot::sniping_strategy_config_args::SnipingStrategyConfigArgs;
use crate::utils::decimals::sol_to_lamports;

#[derive(Default, Clone)]
pub enum ExecutionStatus {
    #[default]
    Idle,
    Error,
    Done,
    Pending,
}
#[derive(Clone)]
pub struct SniperStrategyStateMachine {
    pub context: AppContext,
    pub instance: Arc<SnipingStrategyInstance>,
    pub sniper_wallet: KeypairClonable,
    pub pool_snipes: Arc<Mutex<HashMap<Pubkey, Arc<Mutex<StateMachine<SniperAgentState>>>>>>,
    pub actions: Arc<Mutex<Vec<Arc<Mutex<SolanaAction>>>>>,
}

impl Debug for SniperStrategyStateMachine {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SweeperStrategy")
            .field("instance", &self.instance)
            .finish()
    }
}

#[state_machine(
    initial = "State::running()",
    state(derive(Debug, Clone)),
    on_transition = "Self::on_transition",
    on_dispatch = "Self::on_dispatch"
)]
impl SniperStrategyStateMachine {
    pub async fn new(context: &AppContext, instance: SnipingStrategyInstance) -> Result<Self> {
        let sniper_wallet = KeypairClonable::new_from_privkey(&instance.sniper_private_key)?;
        let mut strategy = Self {
            context: context.clone(),
            instance: Arc::new(instance),
            sniper_wallet,
            pool_snipes: Arc::new(Mutex::new(HashMap::new())),
            actions: Arc::new(Mutex::new(Vec::new())),
        };
        Ok(strategy)
    }

    #[state]
    async fn running(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        // once we got a new pool, we're spamming agent with it, that's it
        match event {
            SolanaStrategyEvent::Original(BotEvent::BlockchainEvent(BlockchainEvent::RaydiumNewPoolEvent(new_pool, price))) => {
                if self.pool_snipes.lock().await.contains_key(&new_pool.id) {
                    warn!("Pool already captured, skipping");
                    return Handled;
                }

                if self.instance.skip_pump_fun && new_pool.id.to_string().contains("pump") {
                    warn!("Pool contains `pump`, skipping");
                    return Handled;
                }

                let account_data = self.context.rpc_pool.get_account_data(&new_pool.base_mint).await.unwrap();
                if self.context.rpc_pool.is_freezable_by_account_data(&account_data).await.unwrap() {
                    warn!("Pool is freezable, skipping");
                    return Handled;
                }

                if self.instance.skip_mintable && self.context.rpc_pool.is_mintable_by_account_data(&account_data).await.unwrap() {
                    warn!("Pool is mintable, skipping");
                    return Handled;
                }

                if price.quote_reserve < self.instance.min_pool_liquidity_sol {
                    warn!("Pool has insufficient liquidity, skipping");
                    return Handled;
                }

                // we assuming that all those Done or Errorneous snipes are already removed
                let concurrent_snipes = self.pool_snipes.lock().await.len();
                if concurrent_snipes >= self.instance.max_simultaneous_snipes as usize {
                    warn!("Max simultaneous snipes {} reached, skipping token {:?}", concurrent_snipes, new_pool.base_mint);
                    return Handled;
                } else {
                    debug!("{} concurrent snipes in progress, the limit is {}", concurrent_snipes, self.instance.max_simultaneous_snipes);
                }

                // starting a snipe
                let pool_arc = Arc::new(new_pool.clone());
                let pool_snipes = Arc::clone(&self.pool_snipes);
                let instance = self.instance.clone();
                let sniper_wallet = self.sniper_wallet.clone();
                let context = self.context.clone();
                let actions = self.actions.clone();
                let initial_price_update = price.clone();

                tokio::spawn(async move {
                    let mut pool_snipes = pool_snipes.lock().await;
                    let pool_pubkey = pool_arc.id;
                    let pool_snipe = Arc::new(Mutex::new(
                        SniperAgentState::new(
                            &context,
                            pool_arc,
                            sniper_wallet.clone(),
                            instance,
                            actions,
                            initial_price_update)
                            .await
                            .unwrap()
                            .state_machine(),
                    ));
                    pool_snipes.insert(pool_pubkey, pool_snipe);
                });
                Super
            }
            _ => { Super }
        }
        // Transition(State::done())
    }

    #[action]
    async fn sell_all_tokens(&mut self) {}

    #[state(entry_action = "sell_all_tokens")]
    async fn done(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        Handled
    }

    fn on_transition(&mut self, source: &State, target: &State) {
        info!("Sniper strategy {} transitioned from `{source:?}` to `{target:?}`", self.instance.id);
    }

    fn on_dispatch(&mut self, state: StateOrSuperstate<Self>, event: &SolanaStrategyEvent) {
        match state {
            StateOrSuperstate::State(_) => {
                let pool_snipes_arc = Arc::clone(&self.pool_snipes);
                let event = event.clone();

                tokio::spawn(async move {
                    let futures: Vec<_> = pool_snipes_arc.lock().await
                        .values()
                        .map(|sniper| {
                            let sniper = Arc::clone(sniper);
                            let event = event.clone();
                            tokio::task::spawn(async move {
                                let mut sniper = sniper.lock().await;
                                sniper.handle(&event).await;
                            })
                        })
                        .collect();
                    join_all(futures).await;


                    // Now, filter out the snipers that are done or in error state and remove them
                    let pool_snipes = Arc::clone(&pool_snipes_arc);
                    let pubkeys_to_remove: Vec<Pubkey> = {
                        let pool_snipes_locked = pool_snipes.lock().await;
                        stream::iter(pool_snipes_locked.iter())
                            .filter_map(|(pubkey, sniper)| {
                                let sniper = Arc::clone(sniper);
                                async move {
                                    match sniper.lock().await.state() {
                                        agent::State::Done { .. } | agent::State::Error { .. } => Some(*pubkey),
                                        _ => None,
                                    }
                                }
                            })
                            .collect()
                            .await
                    };

                    // Now remove the filtered snipers in parallel
                    stream::iter(pubkeys_to_remove)
                        .for_each_concurrent(None, |pubkey| {
                            let pool_snipes = Arc::clone(&pool_snipes_arc);
                            async move {
                                pool_snipes.lock().await.remove(&pubkey);
                            }
                        })
                        .await;
                    
                });
            }
            StateOrSuperstate::Superstate(_) => {}
        }
    }
}
