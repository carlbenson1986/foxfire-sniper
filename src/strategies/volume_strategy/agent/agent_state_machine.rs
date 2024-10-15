use crate::config::app_context::AppContext;
use crate::config::constants::{DELAY_BETWEEN_SIMULATION_RETRIES_MS, RETRIES_IF_ERROR_OR_TIMEOUT, TIMEOUT_FOR_ACTION_EXECUTION_HBS};
use crate::schema::traders::dsl::traders;
use crate::schema::traders::{all_columns, id, is_active, wallet};
use crate::schema::users::last_login;
use crate::types::actions::{Amount, Asset, SolanaAction, SolanaActionPayload, SolanaSwapActionPayload, SolanaTransferActionPayload, SwapMethod};
use crate::types::engine::StrategyId;
use crate::types::events::{BlockchainEvent, BotEvent, ExecutionReceipt, ExecutionResult};
use crate::types::keys::KeypairClonable;
use crate::types::pool::RaydiumPool;
use crate::types::bot_user::{NewTrader, Trader};
use crate::{solana, utils};
use crate::utils::decimals::sol_to_lamports;
use anyhow::{anyhow, bail, Error, Result};
use chrono::Utc;
use diesel::prelude::*;
use diesel::{r2d2, QueryDsl};
use diesel_async::pooled_connection::deadpool::{Object, Pool};
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::{pooled_connection, AsyncConnection, AsyncPgConnection, RunQueryDsl};
use futures::executor;
use log::{debug, warn};
use solana_sdk::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use spl_token::solana_program::program_pack::Pack;
use spl_token::state::Account as TokenAccount;
use statig::awaitable::{prelude::*, StateMachine};
use std::cell::RefCell;
use std::ops::{Add, AddAssign};
use std::rc::Rc;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{Mutex as TokioMutex, Mutex};
use tokio::time::sleep;
use tracing::{error, info, trace};
use uuid::Uuid;
use solana_sdk::transaction::TransactionError;
use crate::storage::persistent::DbPool;
use crate::strategies::events::{AgentEvent, SolanaStrategyEvent};
use crate::utils::Stopwatch;

#[derive(Debug, Clone)]
pub struct AgentState {
    // Context info
    pub context: AppContext,
    pub pool: Arc<RaydiumPool>,
    pub trader: Trader,
    // Agent's cached keypair, to not reconstruct from Trader every time
    pub agent_key: KeypairClonable,
    pub main_wallet: Option<KeypairClonable>,
    actions_in_progress: Vec<Uuid>,

    // Parent strategy into
    pub strategy_id_opt: Option<StrategyId>,
    pub strat_actions_generated_from_event: Arc<Mutex<Vec<Arc<Mutex<SolanaAction>>>>>,
    // each agent works with one token only
    pub pending_actions_in_this_tranche: Arc<Mutex<Vec<Arc<Mutex<SolanaAction>>>>>,

    pub retry_timer: Stopwatch,
    pub timeout_timer: Stopwatch,
}

impl AgentState {
    // Create a new agent - can be either a fresh trader or a main wallet with some balance on it.
    pub async fn new(
        context: &AppContext,
        pool: Arc<RaydiumPool>,
        agent_key: KeypairClonable,
        strategy_id_opt: Option<StrategyId>,
        strat_actions_generated_from_event: Arc<Mutex<Vec<Arc<Mutex<SolanaAction>>>>>,
        main_wallet: Option<KeypairClonable>,
    ) -> Result<Self> {
        // start monitoring account and token account, important - no ? here to query initial balance
        let _ = solana::get_balance(context, &agent_key.pubkey()).await;
        let _ = solana::get_token_balance(context, &agent_key.pubkey(), &pool.base_mint).await;

        let mut agent = Self {
            context: context.clone(),
            pool: pool.clone(),
            trader: Trader::default(),
            strategy_id_opt,
            agent_key,
            strat_actions_generated_from_event,
            actions_in_progress: vec![],
            main_wallet,
            retry_timer: Stopwatch::new(TIMEOUT_FOR_ACTION_EXECUTION_HBS),
            timeout_timer: Stopwatch::new(TIMEOUT_FOR_ACTION_EXECUTION_HBS),
            pending_actions_in_this_tranche: Arc::new(Default::default()),
        };

        if let Some(strategy_id) = strategy_id_opt {
            agent.trader = agent.db_read_or_create(&context.db_pool, &strategy_id).await?;
            if agent.trader.is_active & &agent.trader.strategy_instance_id.is_some() {
                if agent.trader.strategy_instance_id.unwrap() != strategy_id {
                    warn!("Agent is already active in another strategy instance, probably main wallet");
                    agent.trader.strategy_instance_id = Some(strategy_id);
                }
                debug!("Strat {:?} Agent {} created: {:?}", strategy_id_opt, agent.trader.id, agent.agent_key.pubkey());
            } else {
                agent.drop().await;
                bail!("Agent is not active")
            }
        } else {
            debug!("Main wallet agent created: {:?}", agent.agent_key.pubkey());
        }
        Ok(agent)
    }

    pub async fn new_from_trader(
        context: &AppContext,
        pool: Arc<RaydiumPool>,
        trader: Trader,
        strat_actions_generated_from_event: Arc<Mutex<Vec<Arc<Mutex<SolanaAction>>>>>,
        main_wallet: Option<KeypairClonable>,
    ) -> Result<Self> {
        let strategy_id_opt = trader.strategy_instance_id;
        let agent_key = KeypairClonable::new_from_privkey(&trader.private_key)?;
        let mut agent = Self {
            context: context.clone(),
            pool: pool.clone(),
            trader,
            strategy_id_opt,
            agent_key,
            strat_actions_generated_from_event,
            actions_in_progress: vec![],
            main_wallet,
            retry_timer: Stopwatch::new(TIMEOUT_FOR_ACTION_EXECUTION_HBS),
            timeout_timer: Stopwatch::new(TIMEOUT_FOR_ACTION_EXECUTION_HBS),
            pending_actions_in_this_tranche: Arc::new(Default::default()),
        };
        let _ = solana::get_balance(context, &agent.agent_key.pubkey()).await;
        let _ = solana::get_token_balance(context, &agent.agent_key.pubkey(), &pool.base_mint).await;
        debug!("Agent {} created from loaded trader record: {:?}", agent.trader.id, agent.agent_key.pubkey());
        Ok(agent)
    }

    pub fn pubkey(&self) -> Pubkey {
        self.agent_key.pubkey()
    }

    async fn db_read_or_create(&self, db_pool: &DbPool, strategy_id: &StrategyId) -> Result<Trader> {
        let mut conn = db_pool.get().await?;
        let trader = traders
            .filter(wallet.eq(self.agent_key.pubkey().to_string()))
            .get_result::<Trader>(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("Error getting user: {:?}", e));
        match trader {
            Ok(trader) => Ok(trader),
            Err(_) => {
                let new_trader = NewTrader {
                    strategy_instance_id: Some(*strategy_id),
                    wallet: self.agent_key.pubkey(),
                    private_key: utils::keys::private_key_string_base58(&self.agent_key.get_keypair()),
                    created: Utc::now().naive_utc(),
                    is_active: true,
                };
                let trader = diesel::insert_into(traders)
                    .values(new_trader)
                    .returning(all_columns)
                    .get_result::<Trader>(&mut conn)
                    .await
                    .map_err(|e| anyhow::anyhow!("Error updating trader: {:?}", e))?;
                Ok(trader)
            }
        }
    }

    pub(crate) async fn drop(&self) {
        if self.is_main_wallet() {
            return;
        }
        if let Ok(mut conn) = self.context.db_pool.get().await {
            debug!("Deactivating agent: {:?}", self.agent_key.pubkey());
            let _ = diesel::update(traders.filter(wallet.eq(self.agent_key.pubkey().to_string())))
                .set(is_active.eq(false))
                .execute(&mut conn)
                .await
                .map_err(|e| anyhow::anyhow!("Error updating user:context {:?}", e));
        }
        solana::stop_monitoring_account(&self.context, &self.agent_key.pubkey()).await;
        solana::stop_monitoring_token_account(&self.context, &self.agent_key.pubkey(), &self.pool.base_mint).await;
    }

    pub async fn queue_action(&mut self, action: SolanaAction) {
        self.actions_in_progress.push(action.uuid);
        self.timeout_timer.start(TIMEOUT_FOR_ACTION_EXECUTION_HBS);
        // consumed and returned as Arc Mutex
        let action = self.context.cache.register_action(action).await;
        self.strat_actions_generated_from_event.lock().await.push(action);
    }

    pub(crate) async fn process_execution_receipt(
        &mut self,
        receipt: &ExecutionReceipt,
    ) -> Result<Option<()>> {
        // waiting for a receipt when there's no pending action should never happen - if it does, it's a bug
        if self.actions_in_progress.contains(&receipt.action_uuid) {
            self.actions_in_progress.retain(|&x| x != receipt.action_uuid);
            debug!("Agent `{:?}` {} received execution receipt: {:?}", 
                self.pubkey(), 
                match self.strategy_id_opt {
                    Some(i) => format!("strategy {}", i),
                    None => "(main wallet)".to_string()
                }, 
                receipt);
            match &receipt.err {
                Some(err) => {
                    self.timeout_timer.turn_off();
                    bail!("Wallet `{:?}` error: {:?}", self.pubkey(), err)
                }
                None => {
                    Ok(Some(()))
                }
            }
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn is_cooled_down_before_retry(&mut self, tick_ms: u64) -> bool {
        if self.timeout_timer.is_time_elapsed(tick_ms) {
            self.timeout_timer.turn_off();
            true
        } else {
            false
        }
    }

    pub fn get_token_mint(&self) -> Pubkey {
        self.pool.base_mint.clone()
    }

    pub async fn get_sol_balance(&self) -> u64 {
        solana::get_balance(&self.context, &self.agent_key.pubkey()).await.unwrap_or(0)
    }

    pub async fn get_token_balance(&self) -> u64 {
        solana::get_token_balance(&self.context, &self.agent_key.pubkey(), &self.get_token_mint()).await.unwrap_or(0)
    }

    pub fn is_main_wallet(&self) -> bool {
        self.strategy_id_opt.is_none()
    }

    pub async fn get_main_wallet(&self) -> KeypairClonable {
        self.main_wallet.clone().unwrap_or(self.agent_key.clone())
    }
    pub async fn get_main_wallet_pubkey(&self) -> Pubkey {
        self.main_wallet.clone().unwrap_or(self.agent_key.clone()).pubkey()
    }
}

#[state_machine(
    initial = "State::idle()",
    state(derive(Debug, Clone, PartialEq, Eq)),
    superstate(derive(Debug)),
    on_transition = "Self::on_transition",
    on_dispatch = "Self::on_dispatch"
)]
impl AgentState {
    async fn transition_to_success_or_error(
        &mut self,
        event: &SolanaStrategyEvent,
        retry_action: State,
        retry: &i64,
    ) -> Response<State> {
        match event {
            SolanaStrategyEvent::ForAgent(AgentEvent::Deactivate) => {
                let main_wallet_pubkey = self.get_main_wallet_pubkey().await;
                if main_wallet_pubkey == self.pubkey() {
                    Transition(State::Error {
                        msg: format!("Attempt to deactivate main wallet"),
                    })
                } else {
                    Transition(State::deactivating(0))
                }
            }
            SolanaStrategyEvent::Original(BotEvent::ExecutionResult(action_uuid, action, res)) => {
                if self.actions_in_progress.contains(action_uuid) {
                    self.actions_in_progress.retain(|&x| x != *action_uuid);
                    match res {
                        ExecutionResult::ExecutionError(e) => {
                            if *retry <= RETRIES_IF_ERROR_OR_TIMEOUT {
                                Transition(retry_action)
                            } else {
                                Transition(State::Error {
                                    msg: format!("Error executing action: {:#?}", e),
                                })
                            }
                        }
                        _ => Super,
                    }
                } else {
                    Super
                }
            }
            SolanaStrategyEvent::Original(BotEvent::BlockchainEvent(
                                                      BlockchainEvent::ExecutionReceipt(receipt),
                                                  )) => match self.process_execution_receipt(receipt).await {
                Ok(Some(..)) => Transition(State::success()),
                Ok(None) => Super,
                Err(e) => {
                    if *retry <= RETRIES_IF_ERROR_OR_TIMEOUT {
                        Transition(retry_action)
                    } else {
                        Transition(State::Error {
                            msg: format!("Error processing execution receipt: {:?}", e),
                        })
                    }
                }
            },
            SolanaStrategyEvent::Original(BotEvent::HeartBeat(tick_ms, _)) => {
                debug!(
                    "Agent {} {}, trying to {:?}, retry {}, ticks to retry: {:?}",
                    self.trader.id,
                    self.pubkey(),
                    retry_action,
                    retry,
                    self.timeout_timer.ticks_left(*tick_ms)
                );
                if self.is_cooled_down_before_retry(*tick_ms).await {
                    if *retry <= RETRIES_IF_ERROR_OR_TIMEOUT {
                        Transition(retry_action)
                    } else {
                        Transition(State::Error {
                            msg: "Transfer action timeout".to_string(),
                        })
                    }
                } else {
                    Super
                }
            }
            _ => Super,
        }
    }


    #[state]
    async fn idle(&self, event: &SolanaStrategyEvent) -> Response<State> {
        match event {
            SolanaStrategyEvent::ForAgent(AgentEvent::Deactivate) => {
                if !self.is_main_wallet() {
                    Transition(State::deactivated())
                } else {
                    Handled
                }
            }
            SolanaStrategyEvent::ForAgent(AgentEvent::Transfer(batch)) => {
                Transition(State::transferring(batch.clone(), 0))
            }
            SolanaStrategyEvent::ForAgent(AgentEvent::Collect) => {
                Transition(State::collecting(0))
            }
            SolanaStrategyEvent::ForAgent(AgentEvent::Buy(amt)) => Transition(State::buying(amt.clone(), 0)),
            SolanaStrategyEvent::ForAgent(AgentEvent::Sell(amt)) => Transition(State::selling(amt.clone(), 0)),
            _ => Super,
        }
    }


    #[action]
    async fn transfer(&mut self, batch: &Vec<SolanaTransferActionPayload>, retry: &i64) {
        debug!(
            "Agent {} {:?} transferring {:?}",
            self.trader.id,
            self.pubkey(),
            batch
        );
        let fee_payer = if batch.iter().find(|x| x.amount == Amount::MaxAndClose && x.asset == Asset::Sol).is_some() {
            self.main_wallet.clone().unwrap_or(self.agent_key.clone())
        } else {
            self.agent_key.clone()
        };
        self.queue_action(
            SolanaAction::new_with_feepayer(
                self.agent_key.clone(),
                fee_payer,
                batch.iter().map(|x| SolanaActionPayload::SolanaTransferActionPayload(x.clone())).collect(),
            )).await;
    }

    #[state(entry_action = "transfer")]
    async fn transferring(
        &mut self,
        batch: &Vec<SolanaTransferActionPayload>,
        retry: &i64,
        event: &SolanaStrategyEvent,
    ) -> Response<State> {
        self.transition_to_success_or_error(event, State::transferring(batch.clone(), retry + 1), retry)
            .await
    }

    #[state(entry_action = "collect")]
    async fn collecting(
        &mut self,
        retry: &i64,
        event: &SolanaStrategyEvent,
    ) -> Response<State> {
        self.transition_to_success_or_error(event, State::collecting(retry + 1), retry)
            .await
    }

    // buying for all balance
    #[action]
    async fn buy_for_all(&mut self, retry: &i64) {
        if retry > &0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(DELAY_BETWEEN_SIMULATION_RETRIES_MS)).await;
        }
        debug!("Agent `{:?}` buying tokens for all balance, retry: {retry}",self.pubkey(),);
        self.queue_action(
            SolanaAction::new(
                self.agent_key.clone(),
                vec![
                    SolanaActionPayload::SolanaSwapActionPayload(
                        SolanaSwapActionPayload {
                            keys: self.pool.to_liquidity_keys(),
                            swap_method: SwapMethod::BuyTokensForExactSol,
                            amount_in: Amount::MaxButLeaveForTransfer,
                            min_amount_out: 0,
                        }
                    )
                ],
            )).await;
    }

    #[state(entry_action = "buy_for_all")]
    async fn buying(&mut self, amt: &Amount, retry: &i64, event: &SolanaStrategyEvent) -> Response<State> {
        self.transition_to_success_or_error(event, State::buying(amt.clone(), retry + 1), retry)
            .await
    }

    #[action]
    async fn sell(&mut self, amt: &Amount, retry: &i64) {
        if retry > &0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(DELAY_BETWEEN_SIMULATION_RETRIES_MS)).await;
        }
        debug!("Agent `{:?}` selling {:?} tokens", self.pubkey(), amt);
        self.queue_action(
            SolanaAction::new(
                self.agent_key.clone(),
                vec![
                    SolanaActionPayload::SolanaSwapActionPayload(
                        SolanaSwapActionPayload {
                            keys: self.pool.to_liquidity_keys(),
                            swap_method: SwapMethod::SellExactTokensForSol,
                            amount_in: amt.clone(),
                            min_amount_out: 0,
                        }
                    )
                ],
            )).await;
        debug!("Agent `{:?}` selling {:?} tokens action queued", self.pubkey(), amt);
    }

    #[state(entry_action = "sell")]
    async fn selling(&mut self, amt: &Amount, retry: &i64, event: &SolanaStrategyEvent) -> Response<State> {
        self.transition_to_success_or_error(event, State::selling(amt.clone(), retry + 1), retry)
            .await
    }

    #[state]
    async fn success(&self, event: &SolanaStrategyEvent) -> Response<State> {
        match event {
            SolanaStrategyEvent::ForAgent(AgentEvent::Deactivate) => {
                if !self.is_main_wallet() {
                    Transition(State::deactivated())
                } else {
                    Handled
                }
            }
            SolanaStrategyEvent::ForAgent(AgentEvent::Transfer(batch)) => {
                Transition(State::transferring(batch.clone(), 0))
            }
            SolanaStrategyEvent::ForAgent(AgentEvent::Collect) => {
                Transition(State::collecting(0))
            }
            SolanaStrategyEvent::ForAgent(AgentEvent::Buy(amt)) => Transition(State::buying(amt.clone(), 0)),
            SolanaStrategyEvent::ForAgent(AgentEvent::Sell(amt)) => Transition(State::selling(amt.clone(), 0)),
            _ => Super,
        }
    }

    #[state]
    pub async fn error(&mut self, msg: &String, event: &SolanaStrategyEvent) -> Response<State> {
        error!("Agent `{:?}` error: {:?}", self.pubkey(), msg);
        if self.is_main_wallet() {
            warn!("Check main wallet error! Moving main_wallet to idle to accept events");
            Transition(State::idle())
        } else {
            Transition(State::deactivating(0))
        }
    }


    #[action]
    async fn collect(&mut self, retry: &i64) {
        if retry > &0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(DELAY_BETWEEN_SIMULATION_RETRIES_MS)).await;
        }
        debug!(
            "Agent {} {:?} collecting all SOL and tokens",
            self.trader.id,
            self.pubkey(),
        );
        let mut transfers = vec![];
        if self.get_token_balance().await > 0 {
            transfers.push(SolanaActionPayload::SolanaTransferActionPayload(
                SolanaTransferActionPayload {
                    asset: Asset::Token(self.pool.base_mint),
                    receiver: self.main_wallet.clone().unwrap().pubkey(),
                    amount: Amount::MaxAndClose,
                }
            ));
        };
        transfers.push(
            SolanaActionPayload::SolanaTransferActionPayload(
                SolanaTransferActionPayload {
                    asset: Asset::Sol,
                    receiver: self.main_wallet.clone().unwrap().pubkey(),
                    amount: Amount::Max,
                }
            ));
        self.queue_action(
            SolanaAction::new_with_feepayer(
                self.agent_key.clone(),
                self.main_wallet.clone().unwrap(),
                transfers,
            )
        ).await;
    }


    #[state(entry_action = "collect")]
    async fn deactivating(&mut self, retry: &i64, event: &SolanaStrategyEvent) -> Response<State> {
        match event {
            SolanaStrategyEvent::Original(BotEvent::BlockchainEvent(
                                                      BlockchainEvent::ExecutionReceipt(receipt),
                                                  )) => match self.process_execution_receipt(receipt).await {
                Ok(Some(..)) => Transition(State::deactivated()),
                Ok(None) => Super,
                Err(e) => {
                    if retry <= &RETRIES_IF_ERROR_OR_TIMEOUT {
                        Transition(State::deactivating(retry + 1))
                    } else {
                        Transition(State::Error {
                            msg: format!("Error processing execution receipt: {:?}", e),
                        })
                    }
                }
            },
            SolanaStrategyEvent::Original(BotEvent::HeartBeat(tick_ms, _)) => {
                debug!(
                    "Agent {} {}, trying to deactivate, retry {}, ticks to retry: {:?}",
                    self.trader.id,
                    self.pubkey(),
                    retry,
                    self.timeout_timer.ticks_left(*tick_ms)
                );
                if self.is_cooled_down_before_retry(*tick_ms).await {
                    if retry <= &RETRIES_IF_ERROR_OR_TIMEOUT {
                        Transition(State::deactivating(retry + 1))
                    } else {
                        Transition(State::deactivated())
                    }
                } else {
                    Super
                }
            }
            _ => Super,
        }
    }

    #[action]
    async fn drop_action(&mut self) {
        self.drop().await;
    }


    #[state(entry_action = "drop_action")]
    async fn deactivated(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        Handled
    }


    fn on_transition(&mut self, source: &State, target: &State) {
        let agent_id = self.trader.id;
        let agent_key = self.agent_key.pubkey();
        info!("Agent transitioned id {agent_id} `{agent_key:?}` from `{source:?}` to `{target:?}`");
    }

    fn on_dispatch(&mut self, state: StateOrSuperstate<Self>, event: &SolanaStrategyEvent) {
        let pubkey = self.agent_key.pubkey();
        match event {
            SolanaStrategyEvent::Original(BotEvent::HeartBeat(tick_ms, _)) => {
                trace!("Agent {pubkey:?} dispatching {event:?} to `{state:?}`");
            }
            _ => {
                trace!(
                    "Strategy {:?}, agent {:?} {pubkey:?} dispatching `{event:?}` to `{state:?}`",
                    self.trader.strategy_instance_id, self.trader.id
                );
            }
        }
    }
}
