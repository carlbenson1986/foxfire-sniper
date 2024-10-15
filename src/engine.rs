use crate::config::constants::{ENGINE_MESSAGE_CHANNEL_CAPACITY, NEW_STRATEGY_POLLING_FREQUENCY_MS};
use crate::types::engine::{
    Aggregator, Collector, Executor, Strategy, StrategyId, StrategyManager,
};
use futures::FutureExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast::{self, Sender};
use tokio::sync::Mutex;
use tokio::task::{JoinHandle, JoinSet};
use tokio_stream::StreamExt;
use tracing::{error, info};
use crate::types::events::{BotEvent, ExecutionError, ExecutionResult};

/// The main engine. This struct is responsible for orchestrating the
/// data flow between collectors, strategies, and executors.
pub struct Engine<E, A> {
    /// The set of collectors that the engine will use to collect raw events.
    collectors: Vec<Box<dyn Collector<E>>>,

    /// The set of aggregators that produce higher-level derived events from events like indicators
    aggregators: Vec<Box<dyn Aggregator<E>>>,

    /// The set of strategies that the engine will use to process events.
    strategy_manager: Arc<dyn StrategyManager<E, A>>,

    /// The set of executors that the engine will use to execute actions.
    executors: Vec<Arc<dyn Executor<A, E>>>,

    /// The capacity of the event channel.
    event_channel_capacity: usize,

    /// The capacity of the action channel.
    action_channel_capacity: usize,
}

impl<E, A> Engine<E, A> {
    pub fn new(strategy_manager: Arc<dyn StrategyManager<E, A> + Send + Sync>) -> Self {
        Self {
            collectors: vec![],
            aggregators: vec![],
            strategy_manager,
            executors: vec![],
            event_channel_capacity: ENGINE_MESSAGE_CHANNEL_CAPACITY,
            action_channel_capacity: ENGINE_MESSAGE_CHANNEL_CAPACITY,
        }
    }
    pub fn with_event_channel_capacity(mut self, capacity: usize) -> Self {
        self.event_channel_capacity = capacity;
        self
    }

    pub fn with_action_channel_capacity(mut self, capacity: usize) -> Self {
        self.action_channel_capacity = capacity;
        self
    }
}

impl<E, A> Engine<E, A>
where
    E: Send + Clone + 'static + std::fmt::Debug,
    A: Send + Clone + 'static + std::fmt::Debug,
{
    /// Adds a collector to be used by the engine.
    pub fn add_collector(&mut self, collector: Box<dyn Collector<E>>) {
        self.collectors.push(collector);
    }

    /// Adds an aggegator to be used by the engine.
    pub fn add_aggregator(&mut self, aggregator: Box<dyn Aggregator<E>>) {
        self.aggregators.push(aggregator);
    }

    /// Adds an executor to be used by the engine.
    pub fn add_executor(&mut self, executor: Arc<dyn Executor<A, E>>) {
        self.executors.push(executor);
    }

    /// The core run loop of the engine. This function will spawn a thread for
    /// each collector, strategy, and executor. It will then orchestrate the
    /// data flow between them.
    pub async fn run(self) -> Result<JoinSet<()>, Box<dyn std::error::Error>> {
        let (event_sender, _): (Sender<E>, _) = broadcast::channel(self.event_channel_capacity);
        let (action_sender, _): (Sender<A>, _) = broadcast::channel(self.action_channel_capacity);

        let mut set = JoinSet::new();

        // Spawn executors in separate threads.
        for executor in self.executors {
            let mut receiver = action_sender.subscribe();
            let event_sender = event_sender.clone();
            set.spawn(async move {
                info!("starting executor... ");
                loop {
                    if let Ok(action) = receiver.recv().await {
                        let event_sender = event_sender.clone();
                        let executor = executor.clone();
                        tokio::spawn(async move {
                            match executor.execute(action).await {
                                Ok(event) => {
                                    match event_sender.send(event) {
                                        Ok(_) => {}
                                        Err(e) => error!("error sending event: {}", e),
                                    }
                                }
                                Err(e) => error!("error executing action: {}", e),
                            }
                        }
                        );
                    }
                }
            });
        }

        // Spawn strategies in separate threads.

        // Spawn strategy manager handler
        let strategy_manager = Arc::clone(&self.strategy_manager);
        let event_sender_clone = event_sender.clone();
        let action_sender = action_sender.clone();

        tokio::spawn(async move {
            if let Err(e) = strategy_manager.run_strategy_manager(event_sender_clone, action_sender).await {
                error!("Strategy manager error: {:?}", e);
            }
        });
        self.strategy_manager.sync_state().await?;

        for mut aggregator in self.aggregators {
            let mut event_receiver = event_sender.subscribe();
            let event_sender = event_sender.clone();
            set.spawn(async move {
                info!("starting aggregator... ");
                loop {
                    match event_receiver.recv().await {
                        Ok(event) => {
                            for derived_event in aggregator.aggregate_event(event) {
                                match event_sender.send(derived_event) {
                                    Ok(_) => {}
                                    Err(e) => error!("error sending derived event: {}", e),
                                }
                            }
                        }
                        Err(e) => error!("error receiving event: {}", e),
                    }
                }
            });
        }

        // Spawn collectors in separate threads.
        for collector in self.collectors {
            let event_sender = event_sender.clone();
            set.spawn(async move {
                info!("starting collector... ");
                let mut event_stream = collector.get_event_stream().await.unwrap();
                while let Some(event) = event_stream.next().await {
                    match event_sender.send(event) {
                        Ok(_) => {}
                        Err(e) => error!("error sending event: {}", e),
                    }
                }
            });
        }

        Ok(set)
    }
}
