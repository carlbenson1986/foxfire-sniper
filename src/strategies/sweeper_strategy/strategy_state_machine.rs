use crate::config::app_context::AppContext;
use crate::config::constants::{BASE_TX_FEE_SOL, MAX_TRANSFERS_IN_ONE_TX, NEW_ACCOUNT_THRESHOLD_SOL, RAYDIUM_SWAP_FEE, RENT_EXEMPTION_THRESHOLD_SOL};
use crate::schema::traders;
use crate::schema::traders::strategy_instance_id;
use crate::schema::traders::dsl::traders as traders_dsl;
use crate::schema::volumestrategyinstances;
use crate::schema::volumestrategyinstances::dsl::volumestrategyinstances as volumestrategyinstances_dsl;
use crate::schema::volumestrategyinstances::{id as volumestrategyinstances_id};
use crate::schema::users::dsl::users;
use crate::schema::users::{chat_id, id as users_id};
use crate::types::actions::{Amount, Asset, SolanaAction, SolanaActionPayload, SolanaTransferActionPayload};
use crate::types::events::BotEvent::HeartBeat;
use crate::types::events::{BotEvent, TickSizeMs};
use crate::types::keys::KeypairClonable;
use crate::types::pool::RaydiumPool;
use crate::types::bot_user::{BotUser, Trader};
use crate::types::volume_strategy::VolumeStrategyInstance;
use crate::utils::Stopwatch;
use crate::strategies::volume_strategy::agent::{self, AgentState};
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
use tracing::{info, trace};
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use crate::solana;

#[derive(Default, Clone)]
pub enum ExecutionStatus {
    #[default]
    Idle,
    Error,
    Done,
    Pending,
}
#[derive(Clone)]
pub struct SweeperStrategyStateMachine {
    pub context: AppContext,
    pub instance: VolumeStrategyInstance,
    pub pool: Arc<RaydiumPool>,
    pub main_wallet: Arc<Mutex<StateMachine<AgentState>>>,
    pub agents: Vec<Arc<Mutex<StateMachine<AgentState>>>>,
    pub strat_actions_generated_from_event: Arc<Mutex<Vec<Arc<Mutex<SolanaAction>>>>>,
    pub stopwatch: Stopwatch,
    pub amount: u64,
}

impl Debug for SweeperStrategyStateMachine {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SweeperStrategy")
            .field("instance", &self.instance)
            .field("pool", &self.pool)
            .finish()
    }
}

#[state_machine(
    initial = "State::list()",
    state(derive(Debug, Clone)),
    on_transition = "Self::on_transition",
    on_dispatch = "Self::on_dispatch"
)]
impl SweeperStrategyStateMachine {
    pub async fn new(context: &AppContext, volume_strategy_instance: &VolumeStrategyInstance) -> Result<Self> {
        let pool = Arc::new(
            context
                .rpc_pool
                .get_pool_details(&volume_strategy_instance.target_pool)
                .await?,
        );
        let stopwatch = Stopwatch::new(volume_strategy_instance.tranche_frequency_hbs as u64);

        let mut conn = context.db_pool.get().await?;
        let user: BotUser = users
            .filter(users_id.eq(&volume_strategy_instance.user_id))
            .first::<BotUser>(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("User not found {:?}", e))?;

        let strat_actions_generated_from_event = Arc::new(Mutex::new(Vec::new()));
        let main_wallet = Arc::new(Mutex::new(
            AgentState::new(
                &context,
                pool.clone(),
                KeypairClonable::new_from_privkey(&user.wallet_private_key).unwrap(),
                None,
                strat_actions_generated_from_event.clone(),
                None,
            )
                .await?
                .state_machine(),
        ));
        let main_wallet_clone = main_wallet.clone();
        let mut strategy = SweeperStrategyStateMachine {
            context: context.clone(),
            instance: volume_strategy_instance.clone(),
            main_wallet,
            strat_actions_generated_from_event,
            pool,
            stopwatch,
            agents: Vec::new(),
            amount: 0,
        };

        Ok(strategy)
    }

    #[action]
    async fn get_agents_with_balance(&mut self) {
        let mut conn = self.context.db_pool.get().await.unwrap();
        let main_wallet = self.main_wallet.lock().await.pubkey().to_string();
        if let Ok(strat_traders) = traders::table
            .filter(traders::id.eq(self.instance.id))
            .filter(traders::wallet.ne(&main_wallet))
            .select(traders::all_columns)
            .load::<Trader>(&mut conn)
            .await {
            self.agents = stream::iter(strat_traders.into_iter())
                .map(|trader| {
                    let parent_strat = self.clone();
                    async move {
                        let trader_sol_balance_res = solana::get_balance(&parent_strat.context, &trader.wallet).await;
                        let trader_token_balance = solana::get_token_balance(&parent_strat.context, &trader.wallet, &parent_strat.pool.base_mint).await.unwrap_or(0);
                        if let Ok(trader_sol_balance) = trader_sol_balance_res {
                            if trader_sol_balance < NEW_ACCOUNT_THRESHOLD_SOL && trader_token_balance < (parent_strat.instance.agents_keep_tokens_lamports as u64) {
                                None
                            } else {
                                AgentState::new_from_trader(
                                    &parent_strat.context,
                                    parent_strat.pool.clone(),
                                    trader.clone(),
                                    parent_strat.strat_actions_generated_from_event.clone(),
                                    parent_strat.main_wallet.lock().await.main_wallet.clone(),
                                )
                                    .await
                                    .ok()
                                    .map(|agent_state| Arc::new(Mutex::new(agent_state.state_machine())))
                            }
                        } else {
                            None
                        }
                    }
                })
                .buffer_unordered(10)
                .filter_map(|result| async move { result })
                .collect()
                .await;
        }
    }

    #[state(entry_action = "get_agents_with_balance")]
    async fn list(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        Transition(State::sweeping())
    }

    #[action]
    async fn sweep(&mut self) {
        let receiver = self.main_wallet.lock().await.pubkey();
        for agent in self.agents.iter_mut() {
            let mut agent = agent.lock().await;
            let token_balance = agent.get_token_balance().await;
            let transfers = if token_balance > 0 {
                vec![
                    SolanaTransferActionPayload {
                        asset: Asset::Token(self.pool.base_mint),
                        receiver,
                        amount: Amount::MaxAndClose,
                    },
                    SolanaTransferActionPayload {
                        asset: Asset::Sol,
                        receiver,
                        amount: Amount::Max,
                    },
                ]
            } else {
                vec![
                    SolanaTransferActionPayload {
                        asset: Asset::Sol,
                        receiver,
                        amount: Amount::Max,
                    }
                ]
            };

            agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Transfer(transfers))).await;
        }
    }

    #[state(entry_action = "sweep")]
    async fn sweeping(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        match self.get_execution_status().await {
            ExecutionStatus::Done => Transition(State::selling_token_main_w()),
            _ => Super,
        }
    }

    #[action]
    async fn sell_token(&mut self) {
        self.main_wallet.lock().await.handle(
            &SolanaStrategyEvent::ForAgent(AgentEvent::Sell(
                Amount::Max
            )),
        ).await;
    }

    #[state(entry_action = "sell_token")]
    async fn selling_token_main_w(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        Transition(State::done())
    }

    #[action]
    async fn drop_all(&mut self) {}

    #[state(entry_action = "drop_all")]
    async fn done(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        Handled
    }

    fn on_transition(&mut self, source: &State, target: &State) {
        info!("Sweep strategy {} transitioned from `{source:?}` to `{target:?}`", self.instance.id);
    }

    fn on_dispatch(&mut self, state: StateOrSuperstate<Self>, event: &SolanaStrategyEvent) {
        match state {
            StateOrSuperstate::State(_) => {
                let mut main_wallet = self.main_wallet.clone();
                let mut agents_in_current_tranche = self.agents.clone();
                let instance = self.instance.clone();
                let event = event.clone();
                tokio::spawn(async move {
                    main_wallet.lock().await.handle(&event).await;
                    //todo make this loop concurrent
                    for agent in agents_in_current_tranche.iter_mut() {
                        let mut agent = agent.lock().await;
                        agent.handle(&event).await;
                    }
                });
            }
            StateOrSuperstate::Superstate(_) => {}
        }
    }

    pub async fn get_execution_status(&self) -> ExecutionStatus {
        if self.agents.is_empty() {
            return ExecutionStatus::Idle;
        }
        let mut agents_with_error = 0;
        let mut agents_with_success = 0;
        for (i, agent) in self.agents.iter().enumerate() {
            let agent = agent.lock().await;
            match agent.state() {
                agent::State::Success { .. } => { agents_with_success += 1; }
                agent::State::Error { msg } => { agents_with_error += 1; }
                agent::State::Deactivated {} => { agents_with_error += 1; }
                _ => return ExecutionStatus::Pending
            }
        }
        if agents_with_success > 0 {
            ExecutionStatus::Done
        } else {
            ExecutionStatus::Error
        }
    }


    pub async fn drop(&mut self) {
        let deactivate_futures = self.agents.iter_mut().map(|agent| async {
            let mut agent = agent.lock().await;
            agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Deactivate)).await
        });
        join_all(deactivate_futures).await;

        self.agents.clear();
    }
}
