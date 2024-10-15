use crate::config::app_context;
use crate::config::app_context::AppContext;
use crate::schema::users::chat_id;
use crate::schema::users::dsl::users;
use crate::storage::persistent::DbPool;
use crate::types::bot_user::BotUser;
use anyhow::{bail, Result};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use log::warn;
use teloxide::prelude::Message;
use teloxide::types::{ChatId, User as TelegramUser, UserId};

pub fn get_user_from_user_message(message: &Message) -> Option<TelegramUser> {
    if let Some(user) = message.from() {
        // let id = user.id;
        let username = user.username.clone();
        if !user.is_bot && username.is_some() {
            let user = if message.chat.is_private() {
                user.clone()
            } else {
                warn!("User is not a bot and has a username, but is not a private chat. User: {:?}, Chat: {:?}", user, message.chat);
                TelegramUser {
                    id: UserId(message.chat.id.0 as u64),
                    is_bot: false,
                    first_name: "GroupChat".to_string(),
                    last_name: None,
                    username: Some("GroupChat".to_string()),
                    language_code: None,
                    is_premium: false,
                    added_to_attachment_menu: false,
                }
            };
            Some(user)
        } else {
            None
        }
    } else {
        None
    }
}

pub async fn get_user_from_button_press(
    app_context: &AppContext,
    message: &Message,
) -> Result<BotUser> {
    let mut conn = app_context.db_pool.get().await?;
    let user = match users
        .filter(chat_id.eq(&message.chat.id.0))
        .first::<BotUser>(&mut conn)
        .await
    {
        Ok(user) => {
            match app_context
                .settings
                .read()
                .await
                .tgbot
                .as_ref().unwrap()
                .whitelisted_chat_ids
                .clone()
            {
                Some(whitelisted) => {
                    if whitelisted.contains(&user.chat_id) {
                        user.clone()
                    } else {
                        bail!("The system is currently only available to whitelisted users.");
                    }
                }
                None => user.clone(),
            }
        }
        _ => {
            bail!(
                "User not found in the database, user chat id {}",
                &message.chat.id.0
            );
        }
    };
    Ok(user)
}
