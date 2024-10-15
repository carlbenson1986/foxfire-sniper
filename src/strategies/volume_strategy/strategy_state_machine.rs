use std::fmt::{Debug, Formatter};
use crate::config::app_context::AppContext;
use crate::config::constants::{BASE_TX_FEE_SOL, MAX_TRANSFERS_IN_ONE_TX, NEW_ACCOUNT_THRESHOLD_SOL, RAYDIUM_SWAP_FEE, RENT_EXEMPTION_THRESHOLD_SOL, TRANSFER_PRIORITY_FEE_SOL};
use crate::schema::traders::dsl::traders;
use crate::schema::traders::strategy_instance_id;
use crate::schema::users::dsl::users;
use crate::schema::users::{chat_id, id};
use crate::types::actions::{Amount, Asset, SolanaAction, SolanaActionPayload, SolanaTransferActionPayload};
use crate::types::events::BotEvent::HeartBeat;
use crate::types::events::{BotEvent, TickSizeMs};
use crate::types::keys::KeypairClonable;
use crate::types::pool::RaydiumPool;
use crate::types::bot_user::{BotUser, Trader};
use crate::types::volume_strategy::VolumeStrategyInstance;
use crate::{solana, utils};
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
use crate::strategies::volume_strategy::agent::{self, AgentState};
use crate::strategies::events::{AgentEvent, SolanaStrategyEvent};
use crate::utils::Stopwatch;
use crate::utils::math;

/// The state of the volume strategy
/// States:
/// initializing (funding_buyers_with_sol) ->
/// buying ->
/// transferring_token_to_sellers ->
/// selling ->
/// collecting ->
/// idle
///
///  action -> state: (incoming events analyzed) Event generated
/// ---------------------------
/// - fund_sol which creates new agents and sends them SOL from the strategy wallet -> initializing: (all agents funding confirmed) Buy
/// - buy -> buying: (all agents confirm token bought) Bought
/// - transfer_token_to_sellers which creates new agents and sends them token from the current agents bought  -> transferring_token_to_sellers: (all agents sent the token except 1 token) Sell
/// - sell -> Selling : (all agents confirm the token has been sold) Collect
/// - collect -> collecting: (the strategy wallet receives all SOL from the sellers) Done
/// - idle: fund The strategy is not trading, waiting till the next tranche starts
///
///
/// Agent Events:
/// - Buy: The agent is buying the asset
/// - Sell: The agent is selling the asset
/// - TransferToken: The agent is transferring the asset
/// - TransferSol: The agent is transferring SOL
/// State of every agent in a tranche:
/// action -> state: Event generated
/// ---------------------------
/// Buy -> buying : Transfer
/// Sell -> selling : Transfer
/// TransferToken -> transferring_token : Done
/// TransferSol -> transferring_sol : Done
/// idle : Buy or Sell
///
/// AgentSelling - The agent is selling the asset
///
/// If someone is buying (selling) the asset, the overall state of the tranche is buying (selling)
/// If all agents are idle, the overall state of the tranche is idle
/// Can't happen that some agents are buying and some are selling
///

// previous text
// 2. Decides to Buy and Sell with a given total amount buy b tokens and sell a tokens where a = b / p.
// 3. Selects a sample of n agents that will be buying and m that will be selling in the interval, so that n + m <= g. Random numbers with binomial distribution is used here to ensure that the number of agents buying and selling approach g/2 with the number of experiments.
// 4. Creates a vector of agents' buys (sells) in SOL (token) using the Dirichlet distribution: b1 + b 2 + ... + bn = b (a1 + a2 + ... + am = a). The Dirichlet distribution ensures that the values are fairly distributed among the agents, providing a balanced allocation that reflects proportional contributions while maintaining the sum of the values equal to b (a) - that's one of the key points in the strategy to mimicking human behavior.
// 5. Build the vector of n + m timestamps when the trades should happen, t1, t2, ..., t n+m, so that tn+m - t1 <= l. Timestamps are distributed according to the Poisson process to ensure that the trades are distributed uniformly in the interval.
// 6. Starts the agents' lifecycles with the given parameters by initializing sending Buy or Sell events to the appropriate agents.

#[derive(Default, Clone)]
pub enum ExecutionStatus {
    #[default]
    Idle,
    Error,
    Done,
    Pending,
}

#[derive(Clone)]
pub struct VolumeStrategyStateMachine {
    pub context: AppContext,
    pub instance: VolumeStrategyInstance,
    pub pool: Arc<RaydiumPool>,
    pub main_wallet: Arc<Mutex<StateMachine<AgentState>>>,
    pub strat_actions_generated_from_event: Arc<Mutex<Vec<Arc<Mutex<SolanaAction>>>>>,
    pub stopwatch: Stopwatch,
    pub agents: Vec<Arc<Mutex<StateMachine<AgentState>>>>,
}

impl Debug for VolumeStrategyStateMachine {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VolumeStrategyPositionState")
            .field("instance", &self.instance)
            .field("pool", &self.pool)
            .finish()
    }
}

// initial = "State::sweeping()",

#[state_machine(
    initial = "State::sweeping()",
    state(derive(Debug, Clone)),
    superstate(derive(Debug)),
    on_transition = "Self::on_transition",
    on_dispatch = "Self::on_dispatch"
)]
impl VolumeStrategyStateMachine {
    pub async fn new(context: &AppContext, instance: &VolumeStrategyInstance) -> Result<Self> {
        let pool = Arc::new(
            context
                .rpc_pool
                .get_pool_details(&instance.target_pool)
                .await?,
        );
        let stopwatch = Stopwatch::new(instance.tranche_frequency_hbs as u64);

        let mut conn = context.db_pool.get().await?;
        let user: BotUser = users
            .filter(id.eq(&instance.user_id))
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
        let mut strategy = VolumeStrategyStateMachine {
            context: context.clone(),
            instance: instance.clone(),
            main_wallet,
            agents: vec![],
            strat_actions_generated_from_event,
            pool,
            stopwatch,
        };
        info!("Strategy {} created with instance {:?}", instance.id, instance);
        Ok(strategy)
    }


    #[action]
    async fn collect_everything_from_staled_agents(&mut self) {
        debug!(
            "ENTER collect_everything_from_staled_agents, main wallet state: {:?}",
            self.main_wallet.lock().await.state()
        );
        let main_wallet = self.main_wallet.lock().await.agent_key.clone();
        let mut conn = self.context.db_pool.get().await.unwrap();
        if let Ok(strat_traders) = crate::schema::traders::table
            // .filter(crate::schema::traders::strategy_instance_id.eq(self.instance.id))
            .filter(crate::schema::traders::wallet.ne(&main_wallet.pubkey().to_string()))
            .select(crate::schema::traders::all_columns)
            .load::<Trader>(&mut conn)
            .await {
            debug!("{} strategy traders loaded", strat_traders.len());
            let agents_w_balance: Vec<Arc<Mutex<StateMachine<AgentState>>>> = stream::iter(strat_traders.into_iter())
                .map(|trader| {
                    let parent_strat = self.clone();
                    let main_wallet_clone = main_wallet.clone();
                    async move {
                        trace!("Creating agent from trader {:?}", trader);
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
                                    Some(main_wallet_clone),
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

            self.agents = agents_w_balance;

            // taking the first agent for the test
            // let single_agent = agents_w_balance.into_iter().next();
            // self.agents = match single_agent {
            //     Some(agent) => vec![agent],
            //     None => vec![],
            // };
        }

        debug!("{} agents created to sweep from", self.agents.len());

        let receiver = self.main_wallet.lock().await.pubkey();
        for agent in self.agents.iter_mut() {
            let mut agent = agent.lock().await;
            agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Collect)).await;
        }
        debug!(
            "EXIT collect_everything_from_staled_agents: Main wallet state: {:?}, Agents state: {:?}",
            self.main_wallet.lock().await.state(),
            join_all(self.agents.iter().map(|agent| async {
                let locked_agent = agent.lock().await;
                locked_agent.state().clone()
            })).await
        );
    }

    #[action]
    async fn zero_agents(&mut self) {
        debug!(
            "ENTER zero_agents: Main wallet state: {:?}, Agents state: {:?}",
            self.main_wallet.lock().await.state(),
            join_all(self.agents.iter().map(|agent| async {
                let locked_agent = agent.lock().await;
                locked_agent.state().clone()
            })).await
        );
        for agent in &self.agents {
            let agent_pubkey = agent.lock().await.pubkey();
            solana::stop_monitoring_account(&self.context, &agent_pubkey).await;
            solana::stop_monitoring_token_account(&self.context, &agent_pubkey, &self.pool.base_mint).await;
        }
        self.agents.clear();
        debug!(
            "EXIT zero_agents, main wallet state: {:?}",
            self.main_wallet.lock().await.state()
        );
    }

    #[state(
        superstate = "running",
        entry_action = "collect_everything_from_staled_agents",
        exit_action = "zero_agents"
    )]
    async fn sweeping(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        match self.get_execution_status().await {
            ExecutionStatus::Done | ExecutionStatus::Error => Transition(State::offloading_inventory()),
            _ => Super,
        }
    }


    #[action]
    async fn offload_inventory(&mut self) {
        debug!(
            "ENTER offload_inventory Main wallet state: {:?}",
            self.main_wallet.lock().await.state()
        );
        let mut agent = self.main_wallet.lock().await;
        let token_balance = agent.get_token_balance().await;
        let sol_balance = agent.get_sol_balance().await;
        if token_balance > 0 && sol_balance >= BASE_TX_FEE_SOL + TRANSFER_PRIORITY_FEE_SOL {
            agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Sell(Amount::Max))).await;
        } else {
            let msg = format!("Main wallet {:?} has insufficient token {token_balance} or SOL balance {sol_balance} to sell", agent.pubkey());
            warn!("{msg}")
        }
        debug!(
            "EXIT offload_inventory Main wallet state: {:?}",
           agent.state()
        );
    }

    #[state(superstate = "running", entry_action = "offload_inventory")]
    async fn offloading_inventory(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        match self.main_wallet.lock().await.state() {
            agent::State::Success { .. } | agent::State::Error { .. } | agent::State::Deactivated {} | agent::State::Idle {} => Transition(State::fund()),
            _ => Super
        }
    }


    // gets the number of agents needed to buy and sends SOL to them from the master wallet
    // spending: tranche_size_sol + transfer fees
    #[action]
    async fn initialize_agents_with_sol(&mut self) {
        debug!(
            "ENTER initialize_agents_with_sol, main wallet state: {:?}",
            self.main_wallet.lock().await.state()
        );
        let agents_num = self.instance.agents_buying_in_tranche;
        // next step: buying tokens for SOL by these agents
        let instance = &self.instance;
        let mut transfer_vec: Vec<SolanaActionPayload> = vec![];

        let main_wallet_pubkey = self.main_wallet.lock().await.pubkey();
        for (i, amount) in utils::math::get_dirichlet_distributed_with_min_amount(
            utils::decimals::sol_to_lamports(instance.tranche_size_sol),
            instance.agents_buying_in_tranche as usize,
            match instance.agents_keep_tokens_lamports {
                // rent exemption is needed to create a wallet
                0 => NEW_ACCOUNT_THRESHOLD_SOL + 3 * (BASE_TX_FEE_SOL + TRANSFER_PRIORITY_FEE_SOL),
                _ => {
                    //1.Transfer from the main wallet to the agent
                    BASE_TX_FEE_SOL + TRANSFER_PRIORITY_FEE_SOL +
                        //2.Buy some tokens
                        BASE_TX_FEE_SOL + TRANSFER_PRIORITY_FEE_SOL +
                        //3. Leave 1 token with rent exemption and transfer the rest of the tokens back
                        //todo try NEW_ACCOUNT_THRESHOLD_SOL instead of RENT_EXEMPTION_THRESHOLD_SOL
                        RENT_EXEMPTION_THRESHOLD_SOL + TRANSFER_PRIORITY_FEE_SOL + BASE_TX_FEE_SOL
                }
            },
        )
            .iter()
            .enumerate()
        {
            if let Ok(agent) = AgentState::new(
                &self.context,
                self.pool.clone(),
                KeypairClonable::new(),
                Some(self.instance.id),
                self.strat_actions_generated_from_event.clone(),
                Some(self.main_wallet.lock().await.agent_key.clone()),
            ).await {
                let pk = agent.pubkey();
                let agent_sm = Arc::new(Mutex::new(agent.state_machine()));
                self.agents.push(agent_sm.clone());
                transfer_vec.push(
                    SolanaActionPayload::SolanaTransferActionPayload(
                        SolanaTransferActionPayload {
                            asset: Asset::Sol,
                            receiver: pk,
                            amount: Amount::ExactWithFees(*amount),
                        },
                    ),
                );
            }
        }

        let mut transfer_batches: Vec<Vec<_>> = Vec::new();
        for chunk in transfer_vec.chunks(MAX_TRANSFERS_IN_ONE_TX) {
            transfer_batches.push(chunk.to_vec());
        }

        for batch in transfer_batches {
            self.main_wallet
                .lock()
                .await
                .handle(&SolanaStrategyEvent::ForAgent(
                    AgentEvent::Transfer(
                        batch.iter().filter_map(
                            |action| {
                                if let SolanaActionPayload::SolanaTransferActionPayload(action) = action {
                                    Some(SolanaTransferActionPayload {
                                        asset: action.asset.clone(),
                                        receiver: action.receiver.clone(),
                                        amount: action.amount.clone(),
                                    })
                                } else {
                                    None
                                }
                            },
                        ).collect()),
                ))
                .await;
        }
        debug!("Strategy {} : initialize_agents_with_sol: created agents: {:?}, sending {:?}, main wallet state: {:?}", self.instance.id, self.agents.len(), transfer_vec,   self.main_wallet.lock().await.state());
    }

    #[state(superstate = "running", entry_action = "initialize_agents_with_sol")]
    async fn fund(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        match self.main_wallet.lock().await.state() {
            agent::State::Success { .. } => Transition(State::buying()),
            // agent::State::Error { msg } => Transition(State::error(msg.to_string())),
            agent::State::Error { msg } => Transition(State::sweeping()),
            _ => Super,
        }
    }

    #[action]
    async fn buy(&mut self) {
        debug!(
            "ENTER BUY: Main wallet state: {:?}, Agents state: {:?}",
            self.main_wallet.lock().await.state(),
            join_all(self.agents.iter().map(|agent| async {
                let locked_agent = agent.lock().await;
                locked_agent.state().clone()
            })).await
        );
        let instance = &self.instance;
        let swap_tasks: Vec<_> = self.agents
            .iter()
            .map(|agent_mutex| async {
                let mut agent = agent_mutex.lock().await;
                agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Buy(Amount::MaxButLeaveForTransfer))).await;
            })
            .collect();

        futures::future::join_all(swap_tasks).await;
    }

    #[state(superstate = "running", entry_action = "buy")]
    async fn buying(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        match self.get_execution_status().await {
            ExecutionStatus::Done => Transition(State::collecting_tokens()),
            ExecutionStatus::Error | ExecutionStatus::Idle => Transition(State::sweeping()),
            // ExecutionStatus::Error => Transition(State::error("Error buying tokens".to_string())),
            // ExecutionStatus::Idle => Transition(State::error("No buying tokens tasks assigned".to_string())),
            _ => Super,
        }
    }

    #[action]
    async fn collect_token_from_agents(&mut self) {
        debug!(
            "ENTER COLLECT TOKEN Main wallet state: {:?}",
            self.main_wallet.lock().await.state()
        );
        let instance = &self.instance;
        let tokens_to_keep = instance.agents_keep_tokens_lamports as u64;
        let minimum_sol = match tokens_to_keep {
            0 => BASE_TX_FEE_SOL + TRANSFER_PRIORITY_FEE_SOL,
            _ => RENT_EXEMPTION_THRESHOLD_SOL + BASE_TX_FEE_SOL + TRANSFER_PRIORITY_FEE_SOL
        };
        let main_wallet_pubkey = self.main_wallet.lock().await.pubkey();
        let transfer_tasks: Vec<_> = self.agents
            .iter()
            .map(|agent_mutex| async {
                let mut agent = agent_mutex.lock().await;
                let agent_token_balance = agent.get_token_balance().await;
                let sol_balance = agent.get_sol_balance().await;
                if agent_token_balance > tokens_to_keep &&
                    sol_balance >= minimum_sol {
                    let transfer_amount = agent_token_balance - tokens_to_keep;
                    let token = agent.get_token_mint();
                    if tokens_to_keep == 0 {
                        agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Collect)).await;
                    } else {
                        agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Transfer(vec![
                            SolanaTransferActionPayload {
                                asset: Asset::Token(token),
                                receiver: main_wallet_pubkey,
                                amount: Amount::Exact(transfer_amount),
                            }
                        ]))).await;
                    }
                } else {
                    error!("Agent {:?} has insufficient token balance {agent_token_balance} or SOL balance {sol_balance} to transfer", agent.pubkey());
                }
            })
            .collect();

        futures::future::join_all(transfer_tasks).await;
        debug!(
            "EXIT ENTERING COLLECT TOKEN Main wallet state: {:?}",
            self.main_wallet.lock().await.state()
        );
    }

    #[state(
        superstate = "running",
        entry_action = "collect_token_from_agents",
        exit_action = "zero_agents"
    )]
    async fn collecting_tokens(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        match self.get_execution_status().await {
            ExecutionStatus::Done => Transition(State::transferring_token_to_sellers()),
            ExecutionStatus::Error => Transition(State::sweeping()),
            // ExecutionStatus::Error => Transition(State::error("Error collecting tokens".to_string())),
            _ => Super,
        }
    }

    #[action]
    async fn transfer_token_to_sellers(&mut self) {
        let token_balance = self.main_wallet.lock().await.get_token_balance().await;
        debug!(
            "ENTER TRANSFER TOKEN Main wallet state: {:?}",
            self.main_wallet.lock().await.state()
        );
        //todo get rid of that, try to get balance with processed status or less delay to sync with geyser
        info!(
            "Create {} sellers and transfer {} token to them",
            self.instance.agents_selling_in_tranche, token_balance
        );
        let self_clone = &self.clone();
        self.create_agents(self.instance.agents_selling_in_tranche).await;

        // Calculate the amount of tokens to be transferred to each seller agent
        let amounts_vector = math::get_dirichlet_distributed_amount(
            token_balance,
            self.instance.agents_selling_in_tranche as usize,
        );

        debug!("Amounts vector: {:?}", amounts_vector);

        let mut transfer_vec: Vec<SolanaTransferActionPayload> = vec![];
        for (i, amount) in amounts_vector.iter().enumerate() {
            let agent = self.agents[i].lock().await;
            if !matches!(agent.state(), agent::State::Error { .. }  | agent::State::Deactivated {}) {
                transfer_vec.push(SolanaTransferActionPayload {
                    asset: Asset::Sol,
                    receiver: agent.pubkey(),
                    amount: Amount::Exact(BASE_TX_FEE_SOL + TRANSFER_PRIORITY_FEE_SOL + RENT_EXEMPTION_THRESHOLD_SOL),
                });
                transfer_vec.push(SolanaTransferActionPayload {
                    asset: Asset::Token(self.pool.base_mint),
                    receiver: agent.pubkey(),
                    amount: Amount::Exact(*amount),
                });
            }
        }

        debug!("Transfer vector: {:?}", transfer_vec);
        // Group transfers into batches if needed
        let mut transfer_batches: Vec<Vec<_>> = Vec::new();
        for chunk in transfer_vec.chunks(MAX_TRANSFERS_IN_ONE_TX) {
            transfer_batches.push(chunk.to_vec());
        }
        debug!("Transfer batches: {:?}", transfer_batches);

        // Execute each batch of token transfers
        for batch in transfer_batches {
            trace!("sending the event to handle to the main wallet {:?}", batch);
            self.main_wallet
                .lock()
                .await
                .handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Transfer(batch)))
                .await;
        }
        debug!(
            "EXIT TRANSFER TOKEN Main wallet state: {:?}",
            self.main_wallet.lock().await.state()
        );
    }

    // once transferred drop buying agents and update db status
    #[state(
        superstate = "running",
        entry_action = "transfer_token_to_sellers"
    )]
    async fn transferring_token_to_sellers(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        match self.main_wallet.lock().await.state() {
            agent::State::Success { .. } | agent::State::Idle {} => Transition(State::selling()),
            agent::State::Error { .. } | agent::State::Deactivated {} => Transition(State::offloading_inventory()),
            _ => Super
        }
    }

    #[action]
    async fn sell(&mut self) {
        debug!(
            "ENTER SELL Main wallet state: {:?}",
            self.main_wallet.lock().await.state()
        );
        let sell_tasks: Vec<_> = self.agents
            .iter()
            .map(|agent_mutex| async {
                let mut agent = agent_mutex.lock().await;
                if self.instance.agents_keep_tokens_lamports == 0 {
                    let token_balance = agent.get_token_balance().await;
                    if token_balance > 0 {
                        agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Collect)).await;
                    }
                } else {
                    let token_balance = agent.get_token_balance().await;
                    let sol_balance = agent.get_sol_balance().await;
                    if sol_balance >= BASE_TX_FEE_SOL + TRANSFER_PRIORITY_FEE_SOL {
                        let balance_to_sell: u64 = token_balance - self.instance.agents_keep_tokens_lamports as u64;
                        agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Sell(Amount::Exact(balance_to_sell)))).await;
                    } else {
                        let msg = format!("Agent {:?} has insufficient SOL balance {sol_balance} to sell", agent.pubkey());
                        error!("{msg}")
                    }
                }
            }).collect();
        futures::future::join_all(sell_tasks).await;
    }

    #[state(superstate = "running", entry_action = "sell")]
    async fn selling(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        match self.get_execution_status().await {
            ExecutionStatus::Done => Transition(State::collecting_sol()),
            ExecutionStatus::Error => Transition(State::sweeping()),
            // ExecutionStatus::Error => Transition(State::error("Error selling".to_string())),
            _ => Super,
        }
    }

    #[action]
    async fn collect_sol_from_agents(&mut self) {
        let instance = &self.instance;
        let main_wallet_pubkey = self.main_wallet.lock().await.pubkey();
        let transfer_tasks: Vec<_> = self.agents
            .iter()
            .map(|agent_mutex| async {
                let mut agent = agent_mutex.lock().await;
                // let sol_balance = agent.get_sol_balance().await;

                // agent.handle(&SolanaTranchedStrategyEvent::ForAgent(AgentEvent::Transfer(vec![
                //     SolanaTransferActionPayload {
                //         asset: Asset::Sol,
                //         receiver: main_wallet_pubkey,
                //         amount: Amount::Max,
                //     }
                // ]))).await;

                agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Collect)).await;
            })
            .collect();

        futures::future::join_all(transfer_tasks).await;
    }

    #[state(
        superstate = "running",
        entry_action = "collect_sol_from_agents",
        exit_action = "zero_agents"
    )]
    async fn collecting_sol(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        match self.get_execution_status().await {
            ExecutionStatus::Done => Transition(State::sleeping()),
            ExecutionStatus::Error => Transition(State::sweeping()),
            // ExecutionStatus::Error => Transition(State::error("Error collecting sol".to_string())),
            _ => Super,
        }
    }

    #[action]
    async fn set_sleep_timer_before_selling(&mut self) {
        debug!(
            "Setting sleep timer before selling for {} ticks",
            self.instance.tranche_frequency_hbs
        );
        self.stopwatch
            .start(self.instance.tranche_frequency_hbs as u64);
    }

    #[state(
        superstate = "running",
        entry_action = "set_sleep_timer_before_selling"
    )]
    async fn sleeping(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        match &event {
            SolanaStrategyEvent::Original(BotEvent::HeartBeat(tick_ms, _)) => {
                let sw = &self.stopwatch;
                debug!(
                    "Strategy {}: ticks left before next cycle: {:?}",
                    self.instance.id,
                    sw.ticks_left(*tick_ms)
                );
                //timeout
                if sw.is_time_elapsed(*tick_ms) {
                    self.stopwatch.turn_off();
                    Transition(State::fund())
                } else {
                    Super
                }
            }
            _ => Super,
        }
    }

    #[state]
    async fn idle(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        Handled
    }

    #[action]
    async fn cleanup(&mut self, msg: &String) {
        let error_text = format!("Strategy stopped, error: {:?}", msg);
        self.drop();
        self.instance.completed_at = Some(Utc::now().naive_utc());
        error!("{:?}", error_text);
        let mut conn = self
            .context
            .db_pool
            .get()
            .await
            .map_err(anyhow::Error::new)
            .unwrap();
        let user: BotUser = users
            .filter(id.eq(&self.instance.user_id))
            .first::<BotUser>(&mut conn)
            .await
            .map_err(anyhow::Error::new)
            .unwrap();
        self.context.tg_bot
            .as_ref().unwrap()
            .send_message(ChatId(user.chat_id), error_text)
            .await;
    }

    #[state(entry_action = "cleanup")]
    async fn error(&mut self, msg: &String, event: &SolanaStrategyEvent) -> Response<State> {
        Handled
    }

    #[superstate]
    async fn running(&mut self, event: &SolanaStrategyEvent) -> Response<State> {
        Handled
    }

    fn on_transition(&mut self, source: &State, target: &State) {
        info!("Strategy {} transitioned from `{source:?}` to `{target:?}`", self.instance.id);
    }

    fn on_dispatch(&mut self, state: StateOrSuperstate<Self>, event: &SolanaStrategyEvent) {
        trace!("Strategy {} dispatching `{event:?}` to `{state:?}`",self.instance.id);
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
            // if no agents are created, the strategy has run into an error
            return ExecutionStatus::Error;
        }
        let mut agents_with_error = 0;
        let mut agents_with_success = 0;
        for (i, agent) in self.agents.iter().enumerate() {
            let agent = agent.lock().await;
            match agent.state() {
                agent::State::Success { .. } => { agents_with_success += 1; }
                agent::State::Error { .. } | agent::State::Deactivating { .. } | agent::State::Deactivated {} => { agents_with_error += 1; }
                // retruning early if at least one agent is still pending
                _ => return ExecutionStatus::Pending
            }
        }
        if agents_with_success == self.agents.len() {
            ExecutionStatus::Done
        } else {
            ExecutionStatus::Error
        }
    }

    pub async fn create_agents(&mut self, number_of_agents: i32) {
        let agent_futures = futures::stream::iter(0..number_of_agents)
            .map(|_| {
                let parent_strategy = self.clone();
                async move {
                    AgentState::new(
                        &parent_strategy.context,
                        parent_strategy.pool.clone(),
                        KeypairClonable::new(),
                        Some(parent_strategy.instance.id),
                        parent_strategy.strat_actions_generated_from_event.clone(),
                        Some(parent_strategy.main_wallet.lock().await.agent_key.clone()),
                    )
                        .await
                        .ok()
                        .map(|agent_state| Arc::new(Mutex::new(agent_state.state_machine())))
                }
            })
            .buffer_unordered(10)
            .filter_map(|result| async move { result })
            .collect::<Vec<_>>();
        let agents = agent_futures.await;
        self.agents = agents;
    }

    // todo sweep everything
    pub async fn drop(&mut self) {
        let deactivate_futures = self.agents.iter_mut().map(|agent| async {
            let mut agent = agent.lock().await;
            agent.handle(&SolanaStrategyEvent::ForAgent(AgentEvent::Deactivate)).await
        });
        join_all(deactivate_futures).await;

        self.agents.clear();
    }
}
