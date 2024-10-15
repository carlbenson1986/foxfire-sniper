use crate::config::app_context::AppContext;
use crate::config::constants::{DELAY_BETWEEN_SIMULATION_RETRIES_MS, RETRIES_IF_ERROR_OR_TIMEOUT, TIMEOUT_FOR_ACTION_EXECUTION_HBS};
use crate::schema::traders::dsl::traders;
use crate::schema::traders::{all_columns, id, is_active, wallet};
use crate::schema::users::last_login;
use crate::types::actions::{Amount, Asset, SolanaAction, SolanaActionPayload, SolanaSwapActionPayload, SolanaTransferActionPayload, SwapMethod};
use crate::types::engine::StrategyId;
use crate::types::events::{BlockchainEvent, BotEvent, ExecutionReceipt, ExecutionResult};
use crate::types::keys::KeypairClonable;
use crate::types::pool::{RaydiumPool, RaydiumPoolPriceUpdate, RaydiumSwapEvent};
use crate::types::bot_user::{NewTrader, Trader};
use crate::{solana, storage, utils};
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
use tokio::time::{sleep, Instant};
use tracing::{error, info, trace};
use uuid::Uuid;
use solana_sdk::transaction::TransactionError;
use crate::storage::persistent::DbPool;
use crate::strategies::events::{AgentEvent, SolanaStrategyEvent};
use crate::types::sniping_strategy::SnipingStrategyInstance;
use crate::utils::Stopwatch;

#[derive(Debug, Clone)]
pub struct SniperAgentState {
    // Context info
    pub context: AppContext,
    pub pool: Arc<RaydiumPool>,
    // Agent's cached keypair, to not reconstruct from Trader every time
    pub agent_key: KeypairClonable,
    pub sniping_strategy_instance: Arc<SnipingStrategyInstance>,
    actions_in_progress: Vec<Uuid>,

    // Parent strategy into
    pub strat_actions_generated_from_event: Arc<Mutex<Vec<Arc<Mutex<SolanaAction>>>>>,
    // each agent works with one token only

    pub retry_timer: Stopwatch,
    pub buy_delay_timer: Instant,
    pub timeout_timer: Stopwatch,
    pub when_bought_timer: Instant,
    pub deploy_price: RaydiumPoolPriceUpdate,
    pub buy_price: RaydiumPoolPriceUpdate,
    last_time: u64,
}

impl SniperAgentState {
    // Create a new agent - can be either a fresh trader or a main wallet with some balance on it.
    pub async fn new(
        context: &AppContext,
        pool: Arc<RaydiumPool>,
        agent_key: KeypairClonable,
        sniping_strategy_instance: Arc<SnipingStrategyInstance>,
        strat_actions_generated_from_event: Arc<Mutex<Vec<Arc<Mutex<SolanaAction>>>>>,
        deploy_price: RaydiumPoolPriceUpdate,
    ) -> Result<Self> {
        // start monitoring account and token account, important - no ? here to query initial balance
        let _ = solana::get_balance(context, &agent_key.pubkey()).await;
        let _ = solana::get_token_balance(context, &agent_key.pubkey(), &pool.base_mint).await;

        let mut agent = Self {
            context: context.clone(),
            pool: pool.clone(),
            agent_key,
            sniping_strategy_instance,
            strat_actions_generated_from_event,
            actions_in_progress: vec![],
            retry_timer: Stopwatch::new(TIMEOUT_FOR_ACTION_EXECUTION_HBS),
            timeout_timer: Stopwatch::new(TIMEOUT_FOR_ACTION_EXECUTION_HBS),
            when_bought_timer: Instant::now(),
            deploy_price,
            buy_price: Default::default(),
            last_time: 0,
            buy_delay_timer: Instant::now(),
        };

        debug!("Agent created for the pool: {:?}, token: {:?}, deployment price: {:.9} SOL", agent.pool, agent.pool.base_mint, agent.deploy_price.price);

        Ok(agent)
    }

    pub fn pubkey(&self) -> Pubkey {
        self.agent_key.pubkey()
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
            debug!("Agent token `{:?}` received execution receipt: {:?}", self.pool.base_mint, receipt);
            match &receipt.err {
                Some(err) => {
                    self.timeout_timer.turn_off();
                    bail!("Token `{:?}` error: {:?}", self.pool.base_mint, err)
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
}

#[state_machine(
    initial = "State::waiting_to_buy()",
    state(derive(Debug, Clone, PartialEq, Eq)),
    superstate(derive(Debug)),
    on_transition = "Self::on_transition",
    on_dispatch = "Self::on_dispatch"
)]
impl SniperAgentState {
    async fn conditional_transition(
        &mut self,
        event: &SolanaStrategyEvent,
        retry_action: State,
        transition_on_success: State,
        retry: &i64,
    ) -> Response<State> {
        match event {
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
                Ok(Some(..)) => Transition(transition_on_success),
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
                trace!(
                    "Token {:?}, {:?}, retry #{}, ticks to retry: {:?}",
                    self.pool.base_mint,
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


    #[action]
    fn set_buy_delay_timer(&mut self) {
        self.buy_delay_timer = Instant::now();
        debug!("Token `{:?}` setting buy delay timer {} ms", self.pool.base_mint, self.sniping_strategy_instance.buy_delay_ms);
    }

    #[state(entry_action = "set_buy_delay_timer")]
    async fn waiting_to_buy(&self, event: &SolanaStrategyEvent) -> Response<State> {
        let elapsed_ms = self.buy_delay_timer.elapsed().as_millis();
        debug!("Token `{:?}` waiting to buy, elapsed: {} ms", self.pool.base_mint, elapsed_ms);
        if self.sniping_strategy_instance.buy_delay_ms == 0 || (elapsed_ms > self.sniping_strategy_instance.buy_delay_ms as u128) {
            debug!("Token `{:?}` ready to buy", self.pool.base_mint);
            return Transition(State::buying(
                Amount::Exact(sol_to_lamports(self.sniping_strategy_instance.size_sol)),
                0,
            ));
        }
        match event {
            SolanaStrategyEvent::Original(BotEvent::BlockchainEvent(BlockchainEvent::RaydiumHeartbeatPriceUpdate(price_update))) |
            SolanaStrategyEvent::Original(BotEvent::BlockchainEvent(BlockchainEvent::RaydiumSwapEvent(RaydiumSwapEvent { price_update, .. }))) => {
                if price_update.pool == self.pool.id {
                    let relative_price_drop_per_cent = (100.0 * (self.deploy_price.price - price_update.price)) / self.deploy_price.price;
                    debug!("Price change: {:.5}% from deployment price", -relative_price_drop_per_cent);
                    if relative_price_drop_per_cent > self.sniping_strategy_instance.skip_if_price_drops_percent {
                        info!("Token `{:?}` price dropped by {:.5}%, skipping", self.pool.base_mint, relative_price_drop_per_cent);
                        return Transition(State::done());
                    }
                }
            }
            _ => {}
        }
        Super
    }
    // buying for all balance
    #[action]
    async fn buy(&mut self, amt: &Amount, retry: &i64) {
        if retry > &0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(DELAY_BETWEEN_SIMULATION_RETRIES_MS)).await;
        }
        debug!("Buying token `{:?}` for {:?} SOL, retry: {retry}", self.pool.base_mint, amt, retry = retry);
        self.queue_action(
            SolanaAction::new(
                self.agent_key.clone(),
                vec![
                    SolanaActionPayload::SolanaSwapActionPayload(
                        SolanaSwapActionPayload {
                            keys: self.pool.to_liquidity_keys(),
                            swap_method: SwapMethod::BuyTokensForExactSol,
                            amount_in: amt.to_owned(),
                            min_amount_out: 0,
                        }
                    )
                ],
            )).await;
    }

    #[state(entry_action = "buy")]
    async fn buying(&mut self, amt: &Amount, retry: &i64, event: &SolanaStrategyEvent) -> Response<State> {
        self.conditional_transition(event, State::buying(amt.clone(), retry + 1), State::waiting_to_sell(), retry)
            .await
    }

    #[action]
    async fn set_when_bought_timer(&mut self) {
        match self.context.rpc_pool.get_pool_details(&self.pool.id).await {
            Ok(pool_info) => {
                self.context
                    .cache
                    .target_pools
                    .write()
                    .await
                    .insert(self.pool.id, pool_info);
            }
            Err(e) => {
                error!("Error getting pool details: {:?}", e);
            }
        }

        match self.context.rpc_pool.get_pool_price(&self.pool.id).await {
            Ok(pool_price) => {
                self.context
                    .cache
                    .target_pools_prices
                    .lock()
                    .await
                    .insert(self.pool.id, pool_price);
            }
            Err(e) => {
                error!("Error getting pool price: {:?}", e);
            }
        }

        self.when_bought_timer = Instant::now();
        self.buy_price = self.context.cache.target_pools_prices.lock().await.get(&self.pool.id).unwrap().clone();
    }

    #[state(entry_action = "set_when_bought_timer")]
    async fn waiting_to_sell(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        let elapsed = self.when_bought_timer.elapsed().as_secs();
        if elapsed > self.sniping_strategy_instance.force_exit_horizon_s as u64 {
            return Transition(State::selling(Amount::MaxAndClose, 0));
        };

        match event {
            SolanaStrategyEvent::Original(BotEvent::BlockchainEvent(BlockchainEvent::RaydiumHeartbeatPriceUpdate(price_update))) |
            SolanaStrategyEvent::Original(BotEvent::BlockchainEvent(BlockchainEvent::RaydiumSwapEvent(RaydiumSwapEvent { price_update, .. }))) => {
                if price_update.pool == self.pool.id {
                    let relative_price_change_per_cent = (100.0 * (price_update.price - self.buy_price.price)) / self.buy_price.price;
                    debug!("Price change: {:.5}% from buy price", relative_price_change_per_cent);
                    if price_update.price < self.buy_price.price * self.sniping_strategy_instance.stop_loss_percent_move_down {
                        info!("{:?}, selling the token at SL, {}", self.pool.id, price_update.price);
                        return Transition(State::selling(Amount::Max, 0));
                    } else if price_update.price > self.buy_price.price * self.sniping_strategy_instance.take_profit_percent_move_up {
                        info!("{:?}, selling the token at TP, {}", self.pool.id, price_update.price);
                        return Transition(State::selling(Amount::Max, 0));
                    }
                }
            }
            _ => {}
        }
        if elapsed - self.last_time > 1 {
            info!("Token `{:?}` waiting to sell, {:?} s left", self.pool.base_mint, (self.sniping_strategy_instance.force_exit_horizon_s - elapsed as i64).max(0));
            self.last_time = elapsed;
        }
        Super
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
    }

    #[state(entry_action = "sell")]
    async fn selling(&mut self, amt: &Amount, retry: &i64, event: &SolanaStrategyEvent) -> Response<State> {
        self.conditional_transition(event, State::selling(Amount::Max, retry + 1), State::done(), &(retry + 1))
            .await
    }

    #[action]
    pub async fn stop_monitoring(&mut self) {
        self
            .context
            .cache
            .target_pools
            .write()
            .await
            .remove(&self.pool.id);
        solana::stop_monitoring_account(&self.context, &self.agent_key.pubkey()).await;
        solana::stop_monitoring_token_account(&self.context, &self.agent_key.pubkey(), &self.pool.base_mint).await;
    }

    #[state(entry_action = "stop_monitoring")]
    async fn done(&self, event: &SolanaStrategyEvent) -> Response<State> {
        Handled
    }

    #[state]
    pub async fn error(&mut self, msg: &String, event: &SolanaStrategyEvent) -> Response<State> {
        error!("Token `{:?}` error: {:?}, sell manually if token left", self.pool.base_mint, msg);
        Handled
    }

    fn on_transition(&mut self, source: &State, target: &State) {
        let pool_id = self.pool.id;
        let token = self.pool.base_mint;
        info!("Token `{token}` state transitioned from `{source:?}` to `{target:?}`");
    }

    fn on_dispatch(&mut self, state: StateOrSuperstate<Self>, event: &SolanaStrategyEvent) {
        let pubkey = self.agent_key.pubkey();
        match event {
            SolanaStrategyEvent::Original(BotEvent::HeartBeat(tick_ms, _)) => {
                trace!("Agent {pubkey:?} dispatching {event:?} to `{state:?}`");
            }
            _ => {}
        }
    }
}
