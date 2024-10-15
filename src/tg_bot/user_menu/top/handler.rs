use crate::strategies::VolumeStrategy;
use crate::tg_bot::bot_config::{BotConfig, HandlerResult};
use crate::tg_bot::helpers::get_user_from_button_press;
use crate::tg_bot::notifications::{notify_user, notify_with_fading_message, TimeToShow};
use crate::tg_bot::state::MyDialogue;
use crate::tg_bot::user_menu::strategies;
use crate::tg_bot::user_menu::strategies::screen::render_strategies_menu;
use crate::tg_bot::user_menu::top::screen::render_main_menu;
use chrono::{NaiveDate, TimeZone, Utc};
use futures::stream::{self, StreamExt};
use log::warn;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::InputFile;

// Common Buttons
pub const BUTTON_BACK_TO_THE_MAIN_MENU: &str = "▲ Main menu";
pub const BUTTON_CANCEL: &str = "❌Cancel";
pub const SYSTEM: &str = "System Stats";
// Main menu button names (NOT THE TEXT DISPLAYED
pub const BUTTON_STOP_STRATEGIES: &str = "Strategies";
pub const BUTTON_CONFIGURE_STRATEGY: &str = "SelectStrategy";
pub const BUTTON_ACCOUNT: &str = "Account";

pub async fn top_menu_callback_handler(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    config: BotConfig,
) -> HandlerResult {
    // Access the user ID
    if let Some(message) = q.message.clone() {
        match get_user_from_button_press(&config.context, &message).await {
            Ok(user) => {
                let state = dialogue.get_or_default().await?;
                if let Some(button) = q.data {
                    let current_state = dialogue.get_or_default().await?;
                    match button.as_str() {
                        BUTTON_CANCEL => {
                            if let Some(trade_menu_message) = current_state.get_message_to_delete()
                            {
                                bot.delete_message(message.chat.id, trade_menu_message.id)
                                    .await?;
                            }
                            let updated_state = current_state.cancel_text_input();
                            dialogue.update(updated_state.clone()).await?;
                            strategies::screen::render_strategies_menu(
                                &config,
                                &message,
                                &updated_state,
                            )
                            .await?;
                        }
                        BUTTON_BACK_TO_THE_MAIN_MENU => {
                            if let Some(message_to_delete) = current_state.get_message_to_delete() {
                                bot.delete_message(message.chat.id, message_to_delete.id)
                                    .await?;
                            }
                            let updated_state = current_state.to_main_menu();
                            dialogue.update(updated_state.clone()).await?;
                            render_main_menu(&config, &user, Some(&message), &updated_state)
                                .await?;
                        }
                        BUTTON_STOP_STRATEGIES => {
                            let user_strategies = stream::iter(
                                config.strategy_manager.get_active_strategies().await.values(),
                            )
                            .filter_map(|strategy_mutex| async move {
                                let strategy = strategy_mutex.lock().await;
                                if let Some(volume_strategy) =
                                    strategy.as_any().downcast_ref::<VolumeStrategy>()
                                {
                                    if volume_strategy.state_machine.instance.user_id == user.id {
                                        Some(volume_strategy.clone())
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .await;

                            let mut dropped_strats = "".to_string();
                            for strategy in user_strategies {
                                config
                                    .strategy_manager
                                    .drop_strategy(strategy.state_machine.instance.id)
                                    .await?;
                                dropped_strats += &format!(
                                    "Strategy id {} stopped \n",
                                    strategy.state_machine.instance.id
                                );
                            }
                            notify_user(&bot, message.chat.id.0, &dropped_strats).await;
                            render_main_menu(&config, &user, Some(&message), &current_state)
                                .await?;
                            // let state = current_state.to_strategies_list();
                            // match render_strategies_menu(&config, &user, &message, &state).await {
                            //     Ok(_) => {
                            //         dialogue.update(state.clone()).await?;
                            //     }
                            //     Err(e) => {
                            //         let error_message = format!("{e}");
                            //         notify_with_fading_message(&bot, message.chat.id.0.clone(), &error_message, TimeToShow::Quick);
                            //     }
                            // }
                        }
                        BUTTON_CONFIGURE_STRATEGY => {
                            let mut state = current_state.to_receive_strategy();
                            if state.get_strategy_in_progress_in_any().is_none() {
                                let mut strategy = config
                                    .context
                                    .settings
                                    .read()
                                    .await
                                    .get_volume_strategy_config()
                                    .unwrap()
                                    .clone();
                                strategy.user_id = Some(user.id);
                                state = state.update_configured_strategy(strategy.clone());
                            }
                            render_strategies_menu(&config, &message, &state).await?;
                            dialogue.update(state).await?;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                let error_message = format!("{e}");
                notify_with_fading_message(
                    &bot,
                    message.chat.id.0.clone(),
                    &error_message,
                    TimeToShow::Quick,
                );
            }
            _ => {}
        }
    }
    Ok(())
}
