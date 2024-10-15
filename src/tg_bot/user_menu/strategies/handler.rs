use crate::strategies::VolumeStrategy;
use crate::tg_bot::bot_config::{BotConfig, HandlerResult};
use crate::tg_bot::helpers::get_user_from_button_press;
use crate::tg_bot::notifications::{notify_user, notify_with_fading_message, TimeToShow};
use crate::tg_bot::state::{DialogueMessages, MyDialogue};
use crate::tg_bot::volume_strategy_config_args::VolumeStrategyConfigArgs;
use crate::tg_bot::user_menu::strategies;
use crate::tg_bot::user_menu::strategies::{buttons, screen};
use crate::tg_bot::user_menu::top;
use crate::tg_bot::user_menu::top::handler::BUTTON_BACK_TO_THE_MAIN_MENU;
use crate::types::volume_strategy::VolumeStrategyInstance;
use crate::utils::decimals::lamports_to_sol;
use crate::utils::formatters::format_sol;
use anyhow::{bail, Result};
use log::debug;
use num_traits::Zero;
use spl_memo::solana_program::pubkey::Pubkey;
use std::future::Future;
use std::str::FromStr;
use std::sync::Arc;
use teloxide::prelude::{CallbackQuery, ChatId, Message, Requester};
use teloxide::Bot;
use crate::config::constants::{BASE_TX_FEE_SOL, TRANSFER_PRIORITY_FEE_SOL};

pub async fn select_strategy_handler(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    config: BotConfig,
) -> HandlerResult {
    if let Some(message) = q.message.clone() {
        let chat_id = message.chat.id;
        let message_id = message.id;
        if let Ok(user) = get_user_from_button_press(&config.context, &message).await {
            let current_state = dialogue.get_or_default().await?;
            if let Some(button) = q.data {
                match button.as_str() {
                    buttons::BUTTON_INFO => {
                        notify_with_fading_message(&bot, chat_id.0, 
                                                   "This strategy is designed to trade a specific token pair on Raydium\\. \
                                                   It will create buyers and sellers that buy and sell the token in bundles, with a specified frequency\\.",
                                                    TimeToShow::Moderate);
                    }
                    buttons::BUTTON_TARGET_POOL => {
                        let message_to_delete = bot.send_message(chat_id, "📝 Enter the Raydium pool address you wish to trade (NOT the token):").await?;
                        let updated_state =
                            current_state.to_receive_token_address(DialogueMessages {
                                message_to_edit: message.clone(),
                                message_to_delete: message_to_delete.clone(),
                            });
                        dialogue.update(updated_state.clone()).await?;
                        screen::render_strategies_menu(&config, &message, &updated_state).await?;
                    }
                    buttons::BUTTON_TRANCHE_SIZE_SOL => {
                        let message_to_delete = bot.send_message(chat_id, "📝 Determine the total SOL to be traded per swap, divided randomly among the specified buys and sells:").await?;
                        let updated_state =
                            current_state.to_receive_tranche_size(DialogueMessages {
                                message_to_edit: message.clone(),
                                message_to_delete: message_to_delete.clone(),
                            });
                        dialogue.update(updated_state.clone()).await?;
                        screen::render_strategies_menu(&config, &message, &updated_state).await?;
                    }
                    buttons::TRANCHE_FREQUENCY_HBS => {
                        let message_to_delete = bot.send_message(chat_id, "📝 Set the interval at which swap bundles are executed. Shorter intervals are optimal for rapid ranking improvements, while longer intervals sustain engagement and gradual rank enhancement: ").await?;
                        let updated_state =
                            current_state.to_receive_tranche_frequency_hbs(DialogueMessages {
                                message_to_edit: message.clone(),
                                message_to_delete: message_to_delete.clone(),
                            });
                        dialogue.update(updated_state.clone()).await?;
                        screen::render_strategies_menu(&config, &message, &updated_state).await?;
                    }
                    buttons::BUTTON_TRANCHE_DURATION => {
                        let message_to_delete = bot.send_message(chat_id, "📝 Enter the tranche duration in seconds, which is an interval for all traders to complete their buys and sells:").await?;
                        let updated_state =
                            current_state.to_receive_tranche_length_hbs(DialogueMessages {
                                message_to_edit: message.clone(),
                                message_to_delete: message_to_delete.clone(),
                            });
                        dialogue.update(updated_state.clone()).await?;
                        screen::render_strategies_menu(&config, &message, &updated_state).await?;
                    }
                    buttons::AGENTS_BUYING_IN_TRANCHE => {
                        let message_to_delete = bot.send_message(chat_id, "📝  Specify the number of buy transactions per swap bundle (Minimum: 1, Maximum: TBD):").await?;
                        let updated_state =
                            current_state.to_receive_agents_buying_in_tranche(DialogueMessages {
                                message_to_edit: message.clone(),
                                message_to_delete: message_to_delete.clone(),
                            });
                        dialogue.update(updated_state.clone()).await?;
                        screen::render_strategies_menu(&config, &message, &updated_state).await?;
                    }
                    buttons::AGENTS_SELLING_IN_TRANCHE => {
                        let message_to_delete = bot.send_message(chat_id, "📝 Specify the number of sell transactions per swap bundle (Minimum: XX, Maximum: TBD):").await?;
                        let updated_state =
                            current_state.to_receive_agents_selling_in_tranche(DialogueMessages {
                                message_to_edit: message.clone(),
                                message_to_delete: message_to_delete.clone(),
                            });
                        dialogue.update(updated_state.clone()).await?;
                        screen::render_strategies_menu(&config, &message, &updated_state).await?;
                    }
                    buttons::AGENTS_KEEP_TOKENS_LAMPORTS => {
                        let message_to_delete = bot.send_message(chat_id, "📝 Enter the amount of tokens (in lamports, the minimal denomination of the token) to keep on the agent's wallet after each tranche:").await?;
                        let updated_state = current_state
                            .to_receive_button_agents_keep_tokens_lamports(DialogueMessages {
                                message_to_edit: message.clone(),
                                message_to_delete: message_to_delete.clone(),
                            });
                        dialogue.update(updated_state.clone()).await?;
                        screen::render_strategies_menu(&config, &message, &updated_state).await?;
                    }
                    buttons::BUTTON_START_STRATEGY => {
                        let strategy_tranche_size = current_state
                            .get_strategy_in_progress()
                            .tranche_size_sol
                            .unwrap_or(0.0);
                        let min_deposit = config
                            .context
                            .settings
                            .read()
                            .await
                            .tgbot
                            .as_ref().unwrap()
                            .minimum_deposit_sol
                            .max(strategy_tranche_size);
                        let user_sol_balance = config
                            .context
                            .rpc_pool
                            .get_balance_ui(&user.wallet_address)
                            .await?;
                        if user_sol_balance < min_deposit {
                            notify_user(
                                &bot,
                                chat_id.0,
                                &format!(
                                    "You have `{}` and need at least `{}` SOL to start a strategy",
                                    format_sol(user_sol_balance),
                                    format_sol(min_deposit)
                                ),
                            )
                            .await;
                            return Ok(());
                        }
                        let agents = current_state
                            .get_strategy_in_progress()
                            .agents_buying_in_tranche
                            .unwrap_or(1)
                            .max(
                                current_state
                                    .get_strategy_in_progress()
                                    .agents_selling_in_tranche
                                    .unwrap_or(1),
                            );
                        let minimum_sol_needed =
                            lamports_to_sol(BASE_TX_FEE_SOL + TRANSFER_PRIORITY_FEE_SOL) * agents as f64;
                        if strategy_tranche_size < minimum_sol_needed {
                            notify_user(&bot, chat_id.0, &format!("The tranche size must be greater than or equal to `{}` to cover swap fees per each trader", format_sol(minimum_sol_needed))).await;
                            return Ok(());
                        }
                        let volume_strategy = match VolumeStrategyInstance::try_from(
                            &current_state.get_strategy_in_progress(),
                        ) {
                            Ok(strategy) => strategy,
                            Err(e) => {
                                bot.send_message(
                                    ChatId(chat_id.0),
                                    format!("Failed to start a strategy: {:?}", e),
                                );
                                return Ok(());
                            }
                        };
                        let strategy =
                            match VolumeStrategy::new(&config.context.clone(), &volume_strategy).await {
                                Ok(strategy) => strategy,
                                Err(e) => {
                                    bot.send_message(chat_id, &format!("Failed to create a strategy: {:?}", e)).await;
                                    return Ok(());
                                }
                            };
                            
                        match config
                            .strategy_manager
                            .start_strategy(Box::new(strategy))
                            .await
                        {
                            Ok(strategy_id) => {
                                notify_user(
                                    &bot,
                                    chat_id.0,
                                    &format!("Strategy id {strategy_id} started 🔥"),
                                )
                                .await;
                                let updated_state = current_state.to_main_menu();
                                dialogue.update(updated_state.clone()).await?;
                                top::screen::render_main_menu(
                                    &config,
                                    &user,
                                    Some(&message),
                                    &updated_state,
                                )
                                .await?;
                            }
                            Err(e) => {
                                bot.send_message(chat_id, &format!("Failed to start a strategy: {:?}", e)).await;
                            }
                        }
                    }
                    BUTTON_BACK_TO_THE_MAIN_MENU => {
                        if let Some(message_to_delete) = current_state.get_message_to_delete() {
                            bot.delete_message(chat_id, message_to_delete.id).await?;
                        }
                        let updated_state = current_state.back();
                        dialogue.update(updated_state.clone()).await?;
                        top::screen::render_main_menu(
                            &config,
                            &user,
                            Some(&message),
                            &updated_state,
                        )
                        .await?;
                    }

                    _ => {}
                }
                // if button.contains("STRATEGY_") {
                //     let asset_id = button.replace("ASSET_", "").parse::<i32>().unwrap();
                //     let selected_asset = TradingPairRepository::read(asset_id).await.unwrap();
                //     let updated_state = current_state.select_asset(AssetArgs {
                //         trading_pair_id: selected_asset.id,
                //         trading_pair_ticker: selected_asset.ticker.clone(),
                //     });
                //     dialogue.update(updated_state.clone()).await?;
                //     render_main_menu(config, &user, Some(&message), &updated_state);
                // }
            }
        }
    }
    Ok(())
}

async fn parse_number<T>(input: String) -> Result<T>
where
    T: FromStr + PartialOrd + Zero,
    <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    let parsed = input.parse::<T>();
    match parsed {
        Ok(parsed) => {
            if parsed <= T::zero() {
                bail!("The value must be greater than 0")
            }
            Ok(parsed)
        }
        Err(e) => {
            bail!("The input is not a valid number")
        }
    }
}

async fn parse_pubkey(config: BotConfig, input: String) -> Result<String> {
    let pool = Pubkey::from_str(&input);
    match pool {
        Ok(pool) => {
            if !config.context.rpc_pool.is_valid_raydium_pool(&pool).await {
                bail!("The address is not a valid Raydium SOL pool")
            }
            let pool = pool.to_string();
            Ok(pool)
        }
        Err(e) => {
            bail!("The input is not a valid Solana address")
        }
    }
}

type UpdateFunction<T> = fn(T) -> VolumeStrategyConfigArgs;

async fn generic_handler<'a, T, F, Fut>(
    bot: Bot,
    dialogue: MyDialogue,
    params: (Option<VolumeStrategyConfigArgs>, DialogueMessages),
    msg: Message,
    config: &'a BotConfig,
    parse_function: F,
    update_function: UpdateFunction<T>,
) -> HandlerResult
where
    F: Fn(BotConfig, String) -> Fut,
    Fut: Future<Output = Result<T>> + 'a,
    T: Clone + 'a,
{
    let current_state = dialogue.get_or_default().await?;
    let trade_menu_message = params.1;
    let mut info_msg: Option<Message> = None;
    match msg.text().map(ToOwned::to_owned) {
        Some(text) => {
            let parse_result = parse_function(config.clone(), text).await;
            match parse_result {
                Ok(value) => {
                    let updated_state =
                        current_state.update_configured_strategy(update_function(value));
                    dialogue.update(updated_state.clone()).await?;
                    bot.delete_message(msg.chat.id, trade_menu_message.message_to_delete.id)
                        .await?;
                    bot.delete_message(msg.chat.id, msg.id).await?;
                    strategies::screen::render_strategies_menu(
                        config,
                        &trade_menu_message.message_to_edit,
                        &updated_state,
                    )
                    .await?;
                }
                Err(e) => {
                    info_msg = Some(
                        bot.send_message(msg.chat.id, format!("Error: {}", e))
                            .await?,
                    );
                }
            }
        }
        None => {
            bot.delete_message(msg.chat.id, trade_menu_message.message_to_delete.id)
                .await?;
            bot.delete_message(msg.chat.id, msg.id).await?;
            info_msg = Some(
                bot.send_message(msg.chat.id, "Please enter a valid value")
                    .await?,
            );
            strategies::screen::render_strategies_menu(
                config,
                &trade_menu_message.message_to_edit,
                &current_state,
            )
            .await?;
        }
    }

    if let Some(info_message) = info_msg {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let _ = bot
                .delete_message(info_message.chat.id, info_message.id)
                .await;
        });
    }

    Ok(())
}

pub async fn receive_token_address_handler(
    bot: Bot,
    dialogue: MyDialogue,
    params: (Option<VolumeStrategyConfigArgs>, DialogueMessages),
    msg: Message,
    config: BotConfig,
) -> HandlerResult {
    generic_handler(
        bot,
        dialogue,
        params,
        msg,
        &config,
        |config, text| parse_pubkey(config, text),
        |value| VolumeStrategyConfigArgs {
            target_pool: Some(value.to_string()),
            ..VolumeStrategyConfigArgs::default()
        },
    )
    .await
}

pub async fn receive_tranche_size_sol_handler(
    bot: Bot,
    dialogue: MyDialogue,
    params: (Option<VolumeStrategyConfigArgs>, DialogueMessages),
    msg: Message,
    config: BotConfig,
) -> HandlerResult {
    generic_handler(
        bot, dialogue, params, msg, &config,
        |_config, text| async {
            let num = parse_number::<f64>(text).await?;
            let minimum_sol_needed = lamports_to_sol(BASE_TX_FEE_SOL + TRANSFER_PRIORITY_FEE_SOL);
            if num < minimum_sol_needed {
                bail!("The value must be greater than or equal to {:.4} to cover swap fees per each trader",  minimum_sol_needed)
            }
            Ok(num)
        },
        |value| VolumeStrategyConfigArgs {
            tranche_size_sol: Some(value),
            ..VolumeStrategyConfigArgs::default()
        },
    ).await
}

pub async fn receive_tranche_frequency_hbs_handler(
    bot: Bot,
    dialogue: MyDialogue,
    params: (Option<VolumeStrategyConfigArgs>, DialogueMessages),
    msg: Message,
    config: BotConfig,
) -> HandlerResult {
    generic_handler(
        bot,
        dialogue,
        params,
        msg,
        &config,
        |_, text| parse_number::<i64>(text),
        |value| VolumeStrategyConfigArgs {
            tranche_frequency_hbs: Some(value),
            ..VolumeStrategyConfigArgs::default()
        },
    )
    .await
}

pub async fn receive_tranche_length_hbs_handler(
    bot: Bot,
    dialogue: MyDialogue,
    params: (Option<VolumeStrategyConfigArgs>, DialogueMessages),
    msg: Message,
    config: BotConfig,
) -> HandlerResult {
    generic_handler(
        bot,
        dialogue,
        params,
        msg,
        &config,
        |_config, text| parse_number::<i64>(text),
        |value| VolumeStrategyConfigArgs {
            tranche_length_hbs: Some(value),
            ..VolumeStrategyConfigArgs::default()
        },
    )
    .await
}

pub async fn receive_agents_buying_in_tranche_handler(
    bot: Bot,
    dialogue: MyDialogue,
    params: (Option<VolumeStrategyConfigArgs>, DialogueMessages),
    msg: Message,
    config: BotConfig,
) -> HandlerResult {
    generic_handler(
        bot,
        dialogue,
        params,
        msg,
        &config,
        |config, text| parse_number::<i32>(text),
        |value| VolumeStrategyConfigArgs {
            agents_buying_in_tranche: Some(value),
            ..VolumeStrategyConfigArgs::default()
        },
    )
    .await
}

pub async fn receive_agents_selling_in_tranche_handler(
    bot: Bot,
    dialogue: MyDialogue,
    params: (Option<VolumeStrategyConfigArgs>, DialogueMessages),
    msg: Message,
    config: BotConfig,
) -> HandlerResult {
    generic_handler(
        bot,
        dialogue,
        params,
        msg,
        &config,
        |config, text| parse_number::<i32>(text),
        |value| VolumeStrategyConfigArgs {
            agents_selling_in_tranche: Some(value),
            ..VolumeStrategyConfigArgs::default()
        },
    )
    .await
}

pub async fn receive_button_agents_keep_tokens_lamports_handler(
    bot: Bot,
    dialogue: MyDialogue,
    params: (Option<VolumeStrategyConfigArgs>, DialogueMessages),
    msg: Message,
    config: BotConfig,
) -> HandlerResult {
    generic_handler(
        bot,
        dialogue,
        params,
        msg,
        &config,
        |config, text| async move {
            let parsed = text.parse::<i64>();
            match parsed {
                Ok(parsed) => {
                    if parsed < i64::zero() {
                        bail!("The value must be greater or equal than 0")
                    }
                    Ok(parsed)
                }
                Err(e) => {
                    bail!("The input is not a valid number")
                }
            }
        },
        |value| VolumeStrategyConfigArgs {
            agents_keep_tokens_lamports: Some(value),
            ..VolumeStrategyConfigArgs::default()
        },
    )
    .await
}
