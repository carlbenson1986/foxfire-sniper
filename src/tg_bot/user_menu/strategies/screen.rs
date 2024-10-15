use crate::config::app_context::AppContext;
use crate::tg_bot::bot_config::BotConfig;
use crate::tg_bot::state::State;
use crate::tg_bot::user_menu::strategies::buttons;
use crate::tg_bot::user_menu::strategies::buttons::BUTTON_INFO;
use crate::tg_bot::user_menu::top::handler::{BUTTON_BACK_TO_THE_MAIN_MENU, BUTTON_CANCEL};
use crate::types::bot_user::BotUser;
use anyhow::Result;
use futures::future;
use std::collections::HashMap;
use std::sync::Arc;
use teloxide::dispatching::dialogue::Storage;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId};

pub async fn render_strategies_menu(
    config: &BotConfig,
    message: &Message,
    current_state: &State,
) -> Result<Message> {
    let strategy_being_configured = current_state.get_strategy_in_progress();
    // get and display user strategies
    let mut strategies_menu: Vec<Vec<InlineKeyboardButton>> = vec![
        vec![InlineKeyboardButton::callback(BUTTON_INFO, BUTTON_INFO)],
        vec![buttons::button_target_pool(&strategy_being_configured)],
        vec![buttons::button_tranche_size_sol(&strategy_being_configured)],
        vec![buttons::button_tranche_frequency_hbs(
            &strategy_being_configured,
        )],
        // vec![buttons::button_tranche_length_hbs(&strategy_being_configured)],
        vec![buttons::button_agents_buying_in_tranche(
            &strategy_being_configured,
        )],
        vec![buttons::button_agents_selling_in_tranche(
            &strategy_being_configured,
        )],
        // vec![buttons::button_agents_keep_tokens_lamports(
        //     &strategy_being_configured,
        // )],
    ];
    if strategy_being_configured.is_ready() {
        strategies_menu.push(vec![InlineKeyboardButton::callback(
            buttons::BUTTON_START_STRATEGY,
            buttons::BUTTON_START_STRATEGY,
        )]);
    }

    strategies_menu.push(vec![InlineKeyboardButton::callback(
        BUTTON_BACK_TO_THE_MAIN_MENU,
        BUTTON_BACK_TO_THE_MAIN_MENU,
    )]);

    if current_state.awaiting_text_input() {
        strategies_menu.push(vec![InlineKeyboardButton::callback(
            BUTTON_CANCEL,
            BUTTON_CANCEL,
        )]);
    }

    config
        .context
        .tg_bot
        .as_ref().unwrap()
        .edit_message_text(
            message.chat.id,
            message.id,
            "Configure a strategy".to_string(),
        )
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .reply_markup(InlineKeyboardMarkup::new(strategies_menu))
        .await
        .map_err(|e| anyhow::anyhow!("Error rendering strategies menu: {:?}", e))
}
