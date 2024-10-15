use crate::config::cache::OperationalCache;
use crate::config::constants::{
    GRPC_FEED_COMMITMENT_LEVEL, RPC_COMMITMENT_LEVEL, RT_FEE_PERCENTILE,
    RT_FEE_PERCENTILE_CAPACITY, RT_FEE_ROLLING_AVERAGE_SIZE,
};
use crate::config::settings::{ProviderName, Settings};
use crate::schema::volumestrategyinstances::completed_at;
use crate::schema::volumestrategyinstances::dsl::volumestrategyinstances;
use crate::solana::bloxroute::BloxRoute;
use crate::solana::geyser_pool::GeyserClientPool;
use crate::solana::rpc_pool::RpcClientPool;
use crate::solana::ws_pool::PubsubClientPool;
use crate::storage::cache::RedisPool;
use crate::storage::persistent::DbPool;
use crate::tg_bot::bot_config::BotConfig;
use crate::types::actions::SolanaAction;
use crate::types::engine::StrategyManager;
use crate::types::events::BotEvent;
use crate::types::pool::{RaydiumPool, RaydiumPoolPriceUpdate};
use crate::utils::fee_metrics::FeeMetrics;
use crate::{solana, storage, tg_bot};
use anyhow::{Result};
use config::Map;
use diesel::prelude::*;
use diesel::prelude::*;
use diesel::SelectableHelper;
use diesel::{r2d2, QueryDsl};
use diesel_async::pooled_connection::deadpool::{Object, Pool};
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::{pooled_connection, AsyncConnection, AsyncPgConnection, RunQueryDsl};
use once_cell::sync::Lazy;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use std::collections::{HashMap, HashSet};
use std::default::Default;
use std::fmt::Debug;
use std::sync::Arc;
use teloxide::dispatching::dialogue::serializer::Json;
use teloxide::dispatching::dialogue::RedisStorage;
use teloxide::net::client_from_env;
use teloxide::Bot;
use tokio::sync::{watch, Mutex, RwLock, RwLockReadGuard};
use tracing::{debug, info};
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use uuid::Uuid;
use yellowstone_grpc_client::{GeyserGrpcClient, Interceptor, InterceptorXToken};
use crate::types::volume_strategy::VolumeStrategyInstance;

// app context we're going to pass around, we use nonblocking versions of the clients here
// todo everything except the connections can be changed on the fly
#[derive(Clone)]
pub struct AppContext {
    pub(crate) settings: Arc<RwLock<Settings>>,
    pub(crate) rpc_pool: RpcClientPool,
    pub(crate) ws_pool: Option<PubsubClientPool>,
    pub(crate) geyser_pool: GeyserClientPool,
    pub(crate) bloxroute: BloxRoute,
    pub(crate) db_pool: DbPool,
    pub(crate) redis_pool: RedisPool,
    pub(crate) cache: OperationalCache,
    pub(crate) tg_bot: Option<Bot>,
    pub geyser_resubscribe_account_tx_notify: watch::Sender<()>,
}

impl Debug for AppContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppContext")
            .field("rpc_pool", &self.rpc_pool)
            .field("ws_pool", &self.ws_pool)
            .field("geyser_pool", &self.geyser_pool)
            .field("bloxroute", &self.bloxroute)
            .finish()
    }
}

impl AppContext {
    pub(crate) async fn new(config_filename: &str) -> Self {
        // loading settings
        let settings = Settings::new(config_filename).expect("Failed to load settings");

        // setting up logging
        let logger_level = &settings.logger.level;
        let filter = tracing_subscriber::EnvFilter::new(logger_level)
            .add_directive("h2::codec=info".parse().unwrap())
            .add_directive("hyper::client=info".parse().unwrap())
            .add_directive("tokio_postgres=info".parse().unwrap())
            .add_directive("reqwest=info".parse().unwrap())
            .add_directive("teloxide=info".parse().unwrap())
            .add_directive("tower=info".parse().unwrap())
            .add_directive("hyper::proto::h1=info".parse().unwrap());
        let fmt_layer = fmt::layer()
            .with_target(true)
            .with_span_events(FmtSpan::CLOSE)
            .with_ansi(true)
            .with_thread_ids(true)
            .with_writer(std::io::stderr);
        tracing_subscriber::registry()
            .with(fmt_layer)
            .with(filter)
            .init();

        // Set up storage: redis cache and timescaledb connection pool
        let rpc_pool = RpcClientPool::new(&settings.rpcs, RPC_COMMITMENT_LEVEL);
        let ws_pool = match &settings.websockets {
            Some(ws) => Some(PubsubClientPool::new(&ws).await),
            None => None,
        };
        let geyser_pool =
            GeyserClientPool::new(&settings.geysers, GRPC_FEED_COMMITMENT_LEVEL).await;
        let bloxroute = BloxRoute::new(&settings.executor.bloxroute_auth_header)
            .with_tip(settings.executor.bloxroute_tip)
            .with_bloxroute_optimal_fee(settings.executor.use_bloxroute_optimal_fee)
            .with_bloxroute_trader_api(settings.executor.use_bloxroute_trader_api)
            .with_fee_percentile(settings.executor.bloxroute_fee_percentile);
        let db_pool = storage::persistent::connect(&settings.storage.database_uri);
        let redis_pool = storage::cache::connect(&settings.storage.redis_uri);

        //check current client pools
        let mut conn = db_pool.get().await.unwrap();
        let pool_pubkeys: Vec<Pubkey> = volumestrategyinstances
            .filter(completed_at.is_null())
            .select(VolumeStrategyInstance::as_select())
            .load(&mut conn)
            .await
            .unwrap()
            .iter()
            .map(|instance| instance.target_pool)
            .collect();

        // Query pool details asynchronously
        let pool_futures = pool_pubkeys.into_iter().map(|pool_pubkey| {
            let rpc_pool_clone = rpc_pool.clone();
            async move {
                let pool_details = rpc_pool_clone.get_pool_details(&pool_pubkey).await?;
                let pool_price = rpc_pool_clone.get_pool_price(&pool_pubkey).await?;
                Ok((pool_pubkey, pool_details, pool_price))
            }
        });

        let results: Vec<Result<(Pubkey, RaydiumPool, RaydiumPoolPriceUpdate)>> =
            futures::future::join_all(pool_futures).await;
        let mut target_pools = HashMap::new();
        let mut target_pools_prices = HashMap::new();

        for result in results {
            match result {
                Ok((pubkey, pool, pool_price)) => {
                    target_pools.insert(pubkey, pool);
                    target_pools_prices.insert(pubkey, pool_price);
                }
                Err(e) => {
                    panic!("Failed to query pool details: {:#?}", e);
                }
            }
        }

        let tgbot = settings.tgbot.clone();
        // Set up node connections
        Self {
            settings: Arc::new(RwLock::new(settings)),
            rpc_pool,
            ws_pool,
            geyser_pool,
            bloxroute,
            db_pool,
            redis_pool,
            cache: OperationalCache::new(target_pools, target_pools_prices),
            tg_bot: tgbot.map(|tgbot| Bot::with_client(tgbot.telegram_token, client_from_env())),
            geyser_resubscribe_account_tx_notify: watch::channel(()).0,
        }
    }
    pub async fn start_telegram_bot(
        &self,
        strategy_manager: Arc<dyn StrategyManager<BotEvent, Arc<Mutex<SolanaAction>>> + Send + Sync>,
    ) -> Result<()> {
        let redis_uri = self.settings.read().await.storage.redis_uri.clone();
        let bot_config = BotConfig {
            context: self.clone(),
            storage: RedisStorage::open(redis_uri, Json)
                .await
                .expect("Redis connection error"),
            strategy_manager,
        };
        let bot = self.tg_bot.clone().unwrap();
        tokio::spawn(async move {
            info!("Starting telegram bot... ");
            let mut dispatcher = tg_bot::init::build_dispatcher(bot, &bot_config).await;
            dispatcher.dispatch().await;
        });

        Ok(())
    }

    pub async fn get_settings(&self) -> RwLockReadGuard<'_, Settings> {
        self.settings.read().await
    }

    pub async fn get_keypairs(&self) -> Vec<Keypair> {
        self.settings
            .read()
            .await
            .executor
            .private_keys
            .iter()
            .map(|private_key| Keypair::from_base58_string(private_key))
            .collect::<Vec<Keypair>>()
    }
}
