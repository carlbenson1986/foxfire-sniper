use crate::config::app_context::AppContext;
use crate::tg_bot::bot_config::BotConfig;
use crate::tg_bot::helpers::buttons::make_keyboard;
use crate::tg_bot::state::State;
use crate::tg_bot::user_menu;
use crate::types::bot_user::BotUser;
use crate::utils;
use crate::utils::decimals::lamports_to_sol;
use anyhow::Result;
use chrono::{Datelike, Duration, TimeZone, Timelike, Utc};
use std::sync::Arc;
use teloxide::dispatching::dialogue::Storage;
use teloxide::prelude::*;
use tracing::error;
// main menu render can be called in three ways:
// 1. Start menu (initialize)
// 2. Back - re-render, edit existing message
// 3. From the engine - no storage data

pub async fn render_main_menu(
    config: &BotConfig,
    user: &BotUser,
    message: Option<&Message>,
    current_state: &State,
) -> Result<()> {
    let mut top_menu_buttons = user_menu::top::buttons::get_menu_top(config, user).await;
    let keyboard = make_keyboard(&top_menu_buttons);
    let current_balance = lamports_to_sol(
        config
            .context
            .rpc_pool
            .get_balance(&user.wallet_address)
            .await?,
    );
    let min_deposit = config
        .context
        .settings
        .read()
        .await
        .tgbot
        .as_ref().unwrap()
        .minimum_deposit_sol;
    let deposit_message = if current_balance < min_deposit {
        format!(
            "Minimum deposit is `{}` SOL\\. Please deposit more funds to start\\.",
            utils::formatters::format_sol(min_deposit)
        )
    } else {
        match current_state {
            State::StrategySelected { .. } => {
                "You are all set\\! Configure the strategy and start the bot\\.".to_string()
            }
            _ => "Select a target token, a strategy and start the bot".to_string(),
        }
    };
    let header = format!("Welcome to the industry leader in volume bot services, designed to elevate your project intuitively with just a few clicks\\.\n\
    \nCurrent balance: `{}` SOL\\.\n\n\
    Your personal SOL deposit address on Solana mainnet, click to copy:\n`{}`\n\n\
    {deposit_message}", utils::formatters::format_sol(current_balance), user.wallet_address);

    if let Some(message) = message {
        config
            .context
            .tg_bot
            .as_ref().unwrap()
            .edit_message_text(ChatId(user.chat_id), message.id, header)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .disable_web_page_preview(true)
            .reply_markup(keyboard)
            .await
            .map_err(|e| anyhow::anyhow!("Error rendering top menu: {:?}", e))
    } else {
        config
            .context
            .tg_bot
            .as_ref().unwrap()
            .send_message(ChatId(user.chat_id), header)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .disable_web_page_preview(true)
            .reply_markup(keyboard)
            .await
            .map_err(|e| anyhow::anyhow!("Error rendering top menu: {:?}", e))
    };
    Ok(())
}
