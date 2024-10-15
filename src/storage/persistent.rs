use crate::config::app_context::AppContext;
use crate::schema::users::dsl::users;
use crate::schema::bot_events::dsl::bot_events;
use crate::schema::users::{all_columns, chat_id, id, last_login};
use crate::storage::cache::RedisPool;
use crate::types::pool::RaydiumPoolPriceUpdate;
use crate::types::bot_user::{BotUser, NewBotUser};
use crate::types::sniping_strategy::SnipingStrategyInstance;
use crate::utils::keys::{private_key_string_base58, public_key_string};
use anyhow::Result;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::deadpool::{Object, Pool};
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::{pooled_connection, AsyncConnection, AsyncPgConnection};
use solana_sdk::signature::{Keypair, Signer};
use std::default::Default;
use std::fmt::Debug;
use std::sync::Arc;
use teloxide::types::User as TelegramUser;
use crate::schema::snipingstrategyinstances;
use crate::solana;
use crate::types::events::{BotEvent, BotEventModel};

pub type DbPool = Arc<Pool<AsyncPgConnection>>;

pub fn connect(database_url: &str) -> DbPool {
    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
    Arc::new(
        Pool::builder(manager)
            .build()
            .expect("Failed to create pool."),
    )
}

pub async fn save_price_to_db(diesel_pool: DbPool, price: RaydiumPoolPriceUpdate) -> Result<()> {
    let mut conn = diesel_pool.get().await?;
    let _ = diesel::insert_into(crate::schema::prices::table)
        .values(price)
        .execute(&mut conn)
        .await?;
    Ok(())
}

pub async fn save_new_sniping_strategy_to_db(
    diesel_pool: DbPool,
    strategy: crate::types::sniping_strategy::NewSnipingStrategyInstance,
) -> Result<SnipingStrategyInstance> {
    use crate::schema::snipingstrategyinstances::dsl::*;
    let mut conn = diesel_pool.get().await?;
    let strat = diesel::insert_into(snipingstrategyinstances)
        .values(strategy.clone())
        .returning(id)
        .get_result(&mut conn)
        .await?;
    Ok(crate::types::sniping_strategy::SnipingStrategyInstance {
        id: strat,
        user_id: strategy.user_id,
        started_at: strategy.started_at,
        completed_at: strategy.completed_at,
        sniper_private_key: strategy.sniper_private_key,
        size_sol: strategy.size_sol,
        stop_loss_percent_move_down: strategy.stop_loss_percent_move_down,
        take_profit_percent_move_up: strategy.take_profit_percent_move_up,
        force_exit_horizon_s: strategy.force_exit_horizon_s,
        max_simultaneous_snipes:  strategy.max_simultaneous_snipes,
        min_pool_liquidity_sol: strategy.min_pool_liquidity_sol,
        skip_pump_fun: strategy.skip_pump_fun,
        skip_mintable: strategy.skip_mintable,
        buy_delay_ms: strategy.buy_delay_ms,
        skip_if_price_drops_percent: strategy.skip_if_price_drops_percent,
    })
}

pub async fn load_or_create_user(
    context: &AppContext,
    user_data: &TelegramUser,
) -> Result<BotUser> {
    let mut conn = context.db_pool.get().await?;
    match users
        .filter(chat_id.eq(&(user_data.id.0 as i64)))
        .first::<BotUser>(&mut conn)
        .await
    {
        Ok(existing_user) => diesel::update(users.find(existing_user.id))
            .set(last_login.eq(chrono::Utc::now()))
            .returning(all_columns)
            .get_result::<BotUser>(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("Error updating user: {:?}", e)),
        _ => {
            //create a new user and a wallet in the system
            let keypair = Keypair::new();
            // create user
            let user = diesel::insert_into(users)
                .values(NewBotUser::new_from_tg_user(user_data))
                .returning(all_columns)
                .get_result::<BotUser>(&mut conn)
                .await
                .map_err(|e| anyhow::anyhow!("Error updating user: {:?}", e))?;
            let _ = solana::get_balance(&context, &keypair.pubkey()).await;
            Ok(user)
        }
    }
}


pub async fn save_bot_event_to_db(
    diesel_pool: &DbPool,
    event: BotEventModel,
) -> Result<()> {
    let mut conn = diesel_pool.get().await?;
    let _ = diesel::insert_into(bot_events)
        .values(event)
        .execute(&mut conn)
        .await?;
    Ok(())
}