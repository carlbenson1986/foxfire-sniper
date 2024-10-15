use crate::config::constants::{
    BASE_TX_FEE_SOL, NEW_ACCOUNT_THRESHOLD_SOL, RENT_EXEMPTION_THRESHOLD_SOL,
};
use crate::types::actions::{Amount, Asset, SolanaAction, SwapMethod, SolanaActionPayload, SolanaSwapActionPayload, SolanaTransferActionPayload, Balance};
use crate::schema::traders;
use crate::schema::traders::dsl::traders as traders_dsl;
use crate::schema::users::chat_id;
use crate::schema::users::dsl::users;
use crate::schema::volumestrategyinstances;
use crate::schema::volumestrategyinstances::dsl::volumestrategyinstances as volumestrategyinstances_dsl;
use crate::schema::volumestrategyinstances::id;
use crate::tg_bot::bot_config::{BotConfig, HandlerResult};
use crate::tg_bot::helpers::get_user_from_user_message;
use crate::tg_bot::state::MyDialogue;
use crate::tg_bot::user_menu::top::screen::render_main_menu;
use crate::types::keys::KeypairClonable;
use crate::types::bot_user::{BotUser, Trader};
use crate::types::volume_strategy::VolumeStrategyInstance;
use crate::utils::decimals::lamports_to_sol;
use crate::utils::formatters::format_sol;
use crate::{executors, storage};
use anyhow::Result;
use chrono::Utc;
use diesel::associations::HasTable;
use diesel::prelude::*;
use diesel_async::pooled_connection::deadpool::{Object, Pool};
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::{pooled_connection, AsyncConnection, AsyncPgConnection, RunQueryDsl};
use futures::stream::{self, StreamExt};
use generic_array::typenum::private::IsLessOrEqualPrivate;
use log::warn;
use solana_farm_client::raydium_sdk::LiquidityPoolKeys;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Signature, Signer};
use spl_associated_token_account::get_associated_token_address;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use teloxide::Bot;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, error, info, trace};
use uuid::Uuid;
use crate::strategies::sweeper_strategy::SweeperStrategy;
use crate::strategies::SweeperStrategyStateMachine;
use crate::types::actions::Amount::{Max, MaxAndClose};
use crate::types::engine::Executor;

pub async fn start(
    bot: Bot,
    dialogue: MyDialogue,
    message: Message,
    config: BotConfig,
) -> HandlerResult {
    if let Some(user_tg_dto) = get_user_from_user_message(&message) {
        let user = storage::persistent::load_or_create_user(&config.context, &user_tg_dto).await?;
        let current_state = dialogue.get_or_default().await?.to_main_menu();
        dialogue.update(current_state.clone()).await?;
        render_main_menu(&config, &user, None, &current_state).await?;
    } else {
        bot.send_message(
            message.chat.id,
            "Please set a username in Telegram settings",
        )
            .await?;
    }
    Ok(())
}


pub async fn collect(
    bot: Bot,
    dialogue: MyDialogue,
    message: Message,
    config: BotConfig,
) -> HandlerResult {
    if let Some(user_tg_dto) = get_user_from_user_message(&message) {
        let user = storage::persistent::load_or_create_user(&config.context, &user_tg_dto).await?;
        let mut conn = config.context.db_pool.get().await?;
        let all_users = users
            // .filter(chat_id.eq(7015550448))
            .load::<BotUser>(&mut conn)
            .await?;

        for user in all_users {
            let config = config.clone();

            let strategies = volumestrategyinstances_dsl
                .filter(volumestrategyinstances::user_id.eq(user.id))
                .load::<VolumeStrategyInstance>(&mut conn)
                .await?;

            for volume_strat in &strategies {
                debug!(
                    "User {}, trying to collect SOL and tokens for volume strategy {}",
                    user.id,
                    volume_strat.id,
                );

                let strategy = SweeperStrategy::new(&config.context, &volume_strat).await?;
                let cfg_clone = config.clone();
                cfg_clone.strategy_manager
                    .start_strategy(Box::new(strategy))
                    .await;
            }

            bot.send_message(
                message.chat.id,
                format!(
                    "User {}, {} collections started",
                    user.id,
                    strategies.len(),
                ),
            )
                .await?;
        }
    }

    Ok(())
}
