use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use strum_macros::Display;
use teloxide::utils::command::{BotCommands, ParseError};

#[derive(BotCommands, Clone, Display, Debug)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum BCommand {
    #[command(description = "Main menu")]
    Start,
    #[command(
        description = "Collect all SOL and SPL tokens from the strategies wallets and send them to the main wallet"
    )]
    Collect,
    // #[command(description = "Usage information")]
    // Help,
    // #[command(description = "Pause bot")]
    // Pause,
    // #[command(description = "Operational Stats")]
    // Statistics,
}
