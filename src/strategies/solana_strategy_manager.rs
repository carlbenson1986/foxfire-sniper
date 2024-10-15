use anyhow::{bail, Result};
use async_trait::async_trait;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use futures_util::future::join_all;
use std::collections::HashMap;
use std::sync::Arc;
use rand::random;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info};
use tokio::sync::watch;
use crate::config::app_context::AppContext;
use crate::config::constants::NEW_STRATEGY_POLLING_FREQUENCY_MS;
use crate::schema::users::dsl::users;
use crate::schema::users::is_active;
use crate::schema::volumestrategyinstances;
use crate::{solana, utils};
use crate::strategies::{DepositWithdrawStrategy, VolumeStrategy};
use crate::strategies::sniper_strategy::SniperStrategy;
use crate::strategies::sweeper_strategy::SweeperStrategy;
use crate::types::actions::SolanaAction;
use crate::types::engine::{Strategy, StrategyId, StrategyManager, StrategyStatus};
use crate::types::events::{BotEvent, SystemEvent};
use crate::types::bot_user::{BotUser};
use crate::types::volume_strategy::{NewVolumeStrategyInstance, VolumeStrategyInstance};
use crate::utils::crypto::hash_i32_to_i32;

pub struct SolanaStrategyManager {
    strategies: Arc<
        RwLock<
            HashMap<
                StrategyId,
                Arc<Mutex<Box<dyn Strategy<BotEvent, Arc<Mutex<SolanaAction>>> + Send + Sync>>>,
            >,
        >,
    >,
    context: AppContext,
    strategy_notify: watch::Sender<()>,
}

#[async_trait]
impl StrategyManager<BotEvent, Arc<Mutex<SolanaAction>>> for SolanaStrategyManager {
    async fn sync_state(&self) -> Result<()> {
        use crate::schema::volumestrategyinstances::dsl::*;

        let mut conn = self.context.db_pool.get().await?;

        let active_users: Vec<BotUser> = users
            .filter(is_active.eq(true))
            .load::<BotUser>(&mut conn)
            .await?;

        let active_strategies: Vec<VolumeStrategyInstance> = volumestrategyinstances
            .filter(completed_at.is_null())
            .load::<VolumeStrategyInstance>(&mut conn)
            .await?;

        let update_balances = |user: &BotUser| {
            let rpc_pool = self.context.rpc_pool.clone();
            let cache = self.context.cache.clone();
            let wallet_address = user.wallet_address.clone();
            let active_strategies = active_strategies.clone();
            let uid = user.id;
            async move {
                let _ = solana::get_balance(&self.context, &wallet_address).await;
                if let Some(strategy_pool) = active_strategies.iter().find(|s| s.user_id == uid) {
                    let pool_details = self.context.rpc_pool.get_pool_details(&strategy_pool.target_pool).await?;
                    let _ = solana::get_token_balance(&self.context, &wallet_address, &pool_details.base_mint).await;
                }
                Ok::<(), anyhow::Error>(())
            }
        };

        join_all(active_users.iter().map(update_balances)).await;

        let mut strategies = self.strategies.write().await;
        for strategy_instance in active_strategies {
            let strat_id = strategy_instance.id.clone();
            let mut strategy =
                Box::new(VolumeStrategy::new(&self.context, &strategy_instance).await?);
            strategy.sync_state().await;
            strategies.insert(strat_id, Arc::new(Mutex::new(strategy)));
            self.strategy_notify.send(()).ok();
        }

        Ok(())
    }

    async fn start_strategy(
        &self,
        strategy: Box<dyn Strategy<BotEvent, Arc<Mutex<SolanaAction>>> + Send + Sync>,
    ) -> Result<StrategyId> {
        let mut strategies = self.strategies.write().await;
        let id = if let Some(volume_strategy) = strategy.as_any().downcast_ref::<VolumeStrategy>() {
            let mut strategy = volume_strategy.state_machine.instance.clone();
            let mut strategy_instance = NewVolumeStrategyInstance::from(&strategy);
            use crate::schema::volumestrategyinstances::dsl::*;

            let mut conn = self.context.db_pool.get().await.unwrap();
            let strat_id = diesel::insert_into(volumestrategyinstances)
                .values(strategy_instance)
                .returning(id)
                .get_result(&mut conn)
                .await?;
            strategy.id = strat_id;
            let volume_strategy =
                Box::new(VolumeStrategy::new(&self.context, &strategy).await?);
            strategies.insert(strat_id, Arc::new(Mutex::new(volume_strategy)));
            strat_id
        } else if let Some(sweeper_strategy) = strategy.as_any().downcast_ref::<SweeperStrategy>() {
            let mut strategy = sweeper_strategy.state_machine.instance.clone();
            let mut strategy_instance = Box::new(SweeperStrategy::new(&self.context, &strategy).await?);
            let strat_id = utils::crypto::hash_i32_to_i32(strategy_instance.state_machine.instance.id);
            strategies.insert(strat_id, Arc::new(Mutex::new(strategy_instance)));
            strat_id
        } else if let Some(sniper_strategy) = strategy.as_any().downcast_ref::<SniperStrategy>() {
            let mut strategy = sniper_strategy.state_machine.instance.clone();
            let mut strategy_instance = Box::new(SniperStrategy::new(&self.context, &strategy).await?);
            let strat_id = strategy_instance.state_machine.instance.id;
            strategies.insert(strat_id, Arc::new(Mutex::new(strategy_instance)));
            strat_id
        } else {
            let id = hash_i32_to_i32(random());
            strategies.insert(id, Arc::new(Mutex::new(strategy)));
            id
        };
        self.strategy_notify.send(()).ok();
        Ok(id)
    }

    async fn drop_strategy(&self, strat_id: StrategyId) -> Result<()> {
        let mut strategies = self.strategies.write().await;
        match strategies.get(&strat_id) {
            None => { return Err(anyhow::anyhow!("Strategy with id {} not found", strat_id)); }
            Some(strategy) => {
                strategy.lock().await.process_event(
                    BotEvent::SystemEvent(SystemEvent::DestroyStrategy(strat_id))
                ).await;
            }
        }

        strategies.remove(&strat_id);

        use crate::schema::volumestrategyinstances::dsl::*;
        let mut conn = self.context.db_pool.get().await?;
        diesel::update(volumestrategyinstances.filter(id.eq(strat_id)))
            .set(completed_at.eq(Some(Utc::now().naive_utc())))
            .execute(&mut conn)
            .await?;
        Ok(())
    }

    async fn get_active_strategies(
        &self,
    ) -> HashMap<StrategyId, Arc<Mutex<Box<dyn Strategy<BotEvent, Arc<Mutex<SolanaAction>>> + Send + Sync>>>> {
        let strategies = self.strategies.read().await;
        let mut active_strategies = HashMap::new();
        for (k, v) in strategies.iter() {
            let strategy = v.lock().await;

            let is_strategy_active = if let Some(volume_strategy) = strategy.as_any().downcast_ref::<VolumeStrategy>() {
                volume_strategy.state_machine.instance.completed_at.is_none()
            } else {
                true
            };
            if is_strategy_active {
                active_strategies.insert(*k, v.clone());
            }
        }

        active_strategies
    }

    async fn run_strategy_manager(
        &self,
        event_sender: tokio::sync::broadcast::Sender<BotEvent>,
        action_sender: tokio::sync::broadcast::Sender<Arc<Mutex<SolanaAction>>>,
    ) -> Result<()> {
        let mut running_strategies: HashMap<StrategyId, JoinHandle<()>> = HashMap::new();
        let mut rx = self.strategy_notify.subscribe();

        if self.context.tg_bot.is_some() {
            let deposit = DepositWithdrawStrategy::new(self.context.clone()).await;
            self.spawn_strategy(
                i32::MAX,
                Arc::new(Mutex::new(Box::new(deposit))),
                event_sender.subscribe(),
                action_sender.clone(),
            )
                .await?;
        }
        let mut cached_strategies_ids = vec![];
        //todo add self destruct if a strategy is in done or error state

        loop {
            tokio::select! {
                _ = rx.changed() => {
                    let current_strategies = self.get_active_strategies().await;
                    let mut ids = current_strategies.keys().cloned().collect::<Vec<_>>();
                    ids.sort();
                    if ids == cached_strategies_ids {
                        continue;
                    } else {
                        cached_strategies_ids = ids;
                    }
                    debug!("Strategy manager notified about strategies list change: {:?}", cached_strategies_ids);
                    // Start new strategies
                    let mut strategies_ids_to_stop = vec![];
                    for (id, strategy) in current_strategies.iter() {
                        if !running_strategies.contains_key(id) {
                            let handle = self
                                .spawn_strategy(
                                    *id,
                                    strategy.clone(),
                                    event_sender.subscribe(),
                                    action_sender.clone(),
                                )
                                .await?;
                            running_strategies.insert(*id, handle);
                        }
                        if let StrategyStatus::Stopped = strategy.lock().await.get_status().await {
                            strategies_ids_to_stop.push(*id);
                        }
                    }
                    // Remove stopped strategies
                    running_strategies.retain(|id, handle| {
                        // terminated by user
                        if !current_strategies.contains_key(id) || strategies_ids_to_stop.contains(id) {
                            handle.abort();
                            false
                        } else {
                            true
                        }
                    });
                }
                    // Periodic check, just in case
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(3)) => {
                    self.strategy_notify.send(()).ok();
                }
            }
        }
    }
}

impl SolanaStrategyManager {
    pub async fn new(context: &AppContext) -> Result<Self> {
        let (tx, _) = watch::channel(());
        let manager = SolanaStrategyManager {
            strategies: Arc::new(Default::default()),
            context: context.clone(),
            strategy_notify: tx,
        };
        Ok(manager)
    }
    async fn spawn_strategy(
        &self,
        id: StrategyId,
        strategy: Arc<Mutex<Box<dyn Strategy<BotEvent, Arc<Mutex<SolanaAction>>> + Send + Sync>>>,
        mut event_receiver: Receiver<BotEvent>,
        action_sender: Sender<Arc<Mutex<SolanaAction>>>,
    ) -> Result<JoinHandle<()>> {
        Ok(tokio::spawn(async move {
            {
                let mut strategy = strategy.lock().await;
                if let Err(e) = strategy.sync_state().await {
                    error!("Error syncing strategy state for {}: {:?}", id, e);
                    return;
                }
            }
            info!("Starting strategy {}, {:?} ...", id, strategy.lock().await);

            let process_event = |event: BotEvent| async {
                let actions = strategy.lock().await.process_event(event).await;
                for action in actions {
                    if let Err(e) = action_sender.send(action) {
                        error!("Error sending action for strategy {}: {:?}", id, e);
                    }
                }
            };

            while let Ok(event) = event_receiver.recv().await {
                process_event(event).await;
            }

            info!("Strategy {} stopped", id);
        }))
    }
}
