use crate::config::settings::StrategyConfig;
use crate::tg_bot::bot_config::{BotConfig, HandlerResult};
use crate::tg_bot::state::MyDialogue;
use crate::tg_bot::user_menu::top::handler::BUTTON_BACK_TO_THE_MAIN_MENU;
use crate::tg_bot::user_menu::top::screen::render_main_menu;
use std::str::FromStr;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::Bot;

pub const BUTTON_POSITION_HISTORY: &str = "PositionHistory";
pub const BUTTON_DEPOSIT: &str = "Deposit";
pub const BUTTON_WITHDRAW: &str = "Withdraw";
pub const BUTTON_BALANCE_HISTORY: &str = "BalanceHistory";
pub const LINK_XPERP_WALLET: &str = "LinkXperp";
pub const CLAIM_REWARDS: &str = "ClaimRewards";
pub const BUTTON_REFRESH_ACCOUNT_MENU: &str = "RefreshAccountMenu";

pub async fn strategy_menu_handler(
    bot: Bot,
    dialogue: MyDialogue,
    _strategy_in_progress: Option<StrategyConfig>,
    q: CallbackQuery,
    config: &BotConfig,
) -> HandlerResult {
    Ok(())
}

pub async fn get_token_address_handler(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    _config: Arc<BotConfig>,
) -> HandlerResult {
    Ok(())
}
