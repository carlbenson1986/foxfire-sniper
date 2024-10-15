use crate::tg_bot::bot_config::BotConfig;
use crate::tg_bot::notifications::invalid_state;
use crate::tg_bot::state::State;
use crate::tg_bot::user_menu::command::BCommand;
use crate::tg_bot::user_menu::strategies::handler;
use crate::tg_bot::user_menu::strategies::handler::select_strategy_handler;
use crate::tg_bot::user_menu::top::endpoints;
use crate::tg_bot::user_menu::top::handler::top_menu_callback_handler;
use dptree::case;
use serde::ser::StdError;
use std::sync::Arc;
use teloxide::dispatching::dialogue::serializer::Json;
use teloxide::dispatching::dialogue::RedisStorage;
use teloxide::dispatching::{dialogue, DefaultKey, Dispatcher, UpdateHandler};
use teloxide::prelude::*;
use teloxide::types::{MessageKind, UpdateKind};
use tracing::field::debug;
use tracing::{debug, info, warn};

pub async fn build_dispatcher(
    bot: Bot,
    config: &BotConfig,
) -> Dispatcher<Bot, Box<(dyn StdError + std::marker::Send + Sync + 'static)>, DefaultKey> {
    let whitelisted_chat_ids_opt_ref = &config
        .context
        .settings
        .read()
        .await
        .tgbot
        .as_ref()
        .unwrap()
        .whitelisted_chat_ids
        .clone();
    let admin_chat_ids_opt_ref = &config.context.settings.read().await.tgbot.as_ref().unwrap().admin_chat_ids.clone();
    fn schema(
        whitelisted_chat_ids_opt_ref: &Option<Vec<i64>>,
        admin_chat_ids_opt_ref: &Option<Vec<i64>>,
    ) -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
        let admin_chat_ids_opt_ref = admin_chat_ids_opt_ref.clone();
        let whitelisted_chat_ids_opt_ref = whitelisted_chat_ids_opt_ref.clone();
        let command_handler = teloxide::filter_command::<BCommand, _>()
            .branch(
                dptree::filter(move |msg: Message, config: BotConfig| {
                    let whitelisted_chat_ids_opt_ref_clone = whitelisted_chat_ids_opt_ref.clone();
                    match whitelisted_chat_ids_opt_ref_clone {
                        None => msg.chat.is_private(),
                        Some(whitelisted_chat_ids) => {
                            debug!("message chat id: {}", msg.chat.id.0);
                            whitelisted_chat_ids.contains(&msg.chat.id.0)
                        }
                    }
                })
                .branch(case![BCommand::Start].endpoint(endpoints::start)),
            )
            .branch(
                dptree::filter(move |msg: Message, config: BotConfig| {
                    let admin_chat_ids_opt_ref = admin_chat_ids_opt_ref.clone();
                    admin_chat_ids_opt_ref.map_or(false, |admin_chat_ids| {
                        admin_chat_ids.contains(&msg.chat.id.0)
                    })
                })
                .branch(case![BCommand::Collect].endpoint(endpoints::collect)),
            );

        // Expecting input from the user
        let message_handler = Update::filter_message()
            .branch(command_handler)
            .branch(
                case![State::ReceiveTokenAddress {
                    strategy_in_progress,
                    strategy_menu_message
                }]
                .endpoint(handler::receive_token_address_handler),
            )
            .branch(
                case![State::ReceiveTrancheSizeSol {
                    strategy_in_progress,
                    strategy_menu_message
                }]
                .endpoint(handler::receive_tranche_size_sol_handler),
            )
            .branch(
                case![State::ReceiveTrancheFrequencyHbs {
                    strategy_in_progress,
                    strategy_menu_message
                }]
                .endpoint(handler::receive_tranche_frequency_hbs_handler),
            )
            .branch(
                case![State::ReceiveTrancheLengthHbs {
                    strategy_in_progress,
                    strategy_menu_message
                }]
                .endpoint(handler::receive_tranche_length_hbs_handler),
            )
            .branch(
                case![State::ReceiveAgentsBuyingInTranche {
                    strategy_in_progress,
                    strategy_menu_message
                }]
                .endpoint(handler::receive_agents_buying_in_tranche_handler),
            )
            .branch(
                case![State::ReceiveAgentsSellingInTranche {
                    strategy_in_progress,
                    strategy_menu_message
                }]
                .endpoint(handler::receive_agents_selling_in_tranche_handler),
            )
            .branch(
                case![State::ReceiveButtonAgentsKeepTokensLamports {
                    strategy_in_progress,
                    strategy_menu_message
                }]
                .endpoint(handler::receive_button_agents_keep_tokens_lamports_handler),
            )
            .branch(dptree::endpoint(invalid_state));

        // Handling button presses
        let callback_query_handler = Update::filter_callback_query()
            // strategy menu
            .branch(
                case![State::ReceiveStrategy {
                    strategy_in_progress
                }]
                .endpoint(select_strategy_handler),
            )
            .branch(dptree::endpoint(top_menu_callback_handler));

        dialogue::enter::<Update, RedisStorage<Json>, State, _>()
            .branch(message_handler)
            .branch(callback_query_handler)
    }

    let storage = config.storage.clone();

    Dispatcher::builder(
        bot,
        schema(whitelisted_chat_ids_opt_ref, admin_chat_ids_opt_ref),
    )
    .dependencies(dptree::deps![config.clone(), storage])
    .default_handler(|upd| async move {
        warn!("unhandled update: {:?}", upd);
    })
    .error_handler(LoggingErrorHandler::with_custom_text(
        "an error has occurred in the dispatcher",
    ))
    .enable_ctrlc_handler()
    .build()
}
