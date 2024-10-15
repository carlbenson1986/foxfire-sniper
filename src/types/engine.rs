use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

/// A stream of events emitted by a [Collector](Collector).
pub type EventStream<'a, E> = Pin<Box<dyn Stream<Item=E> + Send + 'a>>;
pub type StrategyId = i32;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum StrategyStatus {
    Running(HashMap<String, String>),
    Stopped,
}
/// Collector trait, which defines a source of raw events, like a swap on a dex or a tick on a cex.
#[async_trait]
pub trait Collector<E>: Send + Sync {
    /// Returns the core event stream for the collector.
    async fn get_event_stream(&self) -> Result<EventStream<'_, E>>;
}

/// Aggregator trait, which defines a higher level event stream by aggregating raw events like indicators, candles, time series transformations, statistical computations, analytics, ML preprocessors
/// These will generate features used by strategies.
#[async_trait]
pub trait Aggregator<E>: Send + Sync {
    /// Returns the core event stream for the collector.
    fn aggregate_event(&mut self, event: E) -> Vec<E>;
}

/// Strategy trait, which defines the core logic for each opportunity.
#[async_trait]
pub trait Strategy<E, A>: Any + Send + Sync + Debug {
    /// Sync the initial state of the strategy if needed, usually by fetching
    /// onchain data.
    async fn sync_state(&mut self) -> Result<()>;

    /// Process an event, and return an action if needed.
    async fn process_event(&mut self, event: E) -> Vec<A>;

    async fn get_status(&self) -> StrategyStatus;

    fn as_any(&self) -> &dyn Any;
}

#[async_trait]
pub trait StrategyManager<E, A>: Send + Sync {
    async fn sync_state(&self) -> Result<()>;
    async fn start_strategy(
        &self,
        strategy: Box<dyn Strategy<E, A> + Send + Sync>,
    ) -> Result<StrategyId>;

    async fn drop_strategy(&self, id: StrategyId) -> Result<()>;

    async fn get_active_strategies(
        &self,
    ) -> HashMap<StrategyId, Arc<Mutex<Box<dyn Strategy<E, A> + Send + Sync>>>>;

    async fn run_strategy_manager(
        &self,
        event_sender: tokio::sync::broadcast::Sender<E>,
        action_sender: tokio::sync::broadcast::Sender<A>,
    ) -> Result<()>;
}

/// Executor trait, responsible for executing actions returned by strategies.
#[async_trait]
pub trait Executor<A, E>: Send + Sync {
    /// Execute an action.
    async fn execute(&self, action: A) -> Result<E>;
}
