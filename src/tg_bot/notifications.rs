use crate::tg_bot::bot_config::HandlerResult;
use crate::tg_bot::helpers::formatters::{create_solscan_link, format_curr};
use crate::tg_bot::state::MyDialogue;
use crate::types::bot_user::BotUser;
use anyhow::Result;
use solana_sdk::signature::Signature;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::Bot;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};

pub enum TimeToShow {
    Quick,
    Moderate,
    Long,
}

pub async fn invalid_state(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    // let current_state = dialogue.get_or_default().await?;
    // bot.send_message(msg.chat.id, "Command not recognized").await?;
    // bot.send_message(msg.chat.id, format!("State: {:#?}", current_state)).await?;
    Ok(())
}

pub async fn notify_user(bot: &Bot, user_chat_id: i64, text: &str) {
    let _msg = bot
        .send_message(ChatId(user_chat_id), text)
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await;
}

pub async fn inform_about_successful_deposit(
    bot: &Bot,
    user: &BotUser,
    deposit_amount: f64,
    deposit_threshold: f64,
    tx_hash: Signature,
) -> Result<()> {
    let user = user.clone();
    let threshold_note = if deposit_amount < deposit_threshold {
        format!(
            "\nPlease note the minimum deposit is {}, top up to open a position!",
            format_curr(deposit_threshold)
        )
    } else {
        String::new()
    };
    let text = format!(
        "💸 Successfully deposited {} 💸 {threshold_note}\n",
        format_curr(deposit_amount),
    ) + &*create_solscan_link(&tx_hash);
    notify_user(bot, user.chat_id, &text).await;
    Ok(())
}

pub fn notify_with_fading_message(
    bot: &Bot,
    user_chat_id: i64,
    text: &str,
    time_to_show: TimeToShow,
) {
    let message_text = text.to_string();
    let bot = bot.clone();
    tokio::spawn(async move {
        let time = match time_to_show {
            TimeToShow::Quick => 15,
            TimeToShow::Moderate => 30,
            TimeToShow::Long => 60,
        };

        let msg = bot
            .send_message(ChatId(user_chat_id), message_text)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(time)).await;
        let _ = bot.delete_message(ChatId(user_chat_id), msg.id).await;
    });
}
