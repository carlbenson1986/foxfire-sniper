use crate::config::app_context::AppContext;
use crate::types::actions::SolanaAction;
use crate::types::engine::StrategyManager;
use crate::types::events::BotEvent;
use std::sync::Arc;
use teloxide::dispatching::dialogue::serializer::Json;
use teloxide::dispatching::dialogue::RedisStorage;
use teloxide::prelude::Message;
use teloxide::Bot;
use tokio::sync::broadcast::Sender;
use tokio::sync::Mutex;

pub type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
#[derive(Clone)]
pub struct BotConfig {
    pub context: AppContext,
    pub strategy_manager: Arc<dyn StrategyManager<BotEvent, Arc<Mutex<SolanaAction>>>>,
    pub storage: Arc<RedisStorage<Json>>,
}
