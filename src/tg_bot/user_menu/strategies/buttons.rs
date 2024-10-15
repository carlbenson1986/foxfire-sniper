use crate::tg_bot::state::State;
use crate::tg_bot::volume_strategy_config_args::VolumeStrategyConfigArgs;
use crate::utils;
use teloxide::types::InlineKeyboardButton;

pub const BUTTON_TRANCHE_SIZE_SOL: &str = "Bundle Size";
pub const BUTTON_TRANCHE_SIZE_001: &str = "TrancheSizeSol0.01";
pub const BUTTON_TRANCHE_SIZE_01: &str = "TrancheSizeSol0.1";
pub const BUTTON_TRANCHE_SIZE_1: &str = "TrancheSizeSol1";
pub const TRANCHE_FREQUENCY_HBS: &str = "Bundle Frequency";
pub const TRANCHE_FREQUENCY_HBS_10: &str = "TrancheFrequencyHbs10";
pub const TRANCHE_FREQUENCY_HBS_30: &str = "TrancheFrequencyHbs30";
pub const TRANCHE_FREQUENCY_HBS_60: &str = "TrancheFrequencyHbs60";

pub const BUTTON_START_STRATEGY: &str = "🚀Start Strategy";

pub const BUTTON_TARGET_POOL: &str = "Raydium Pool";
pub const BUTTON_INFO: &str = "ℹ️";
pub const BUTTON_TRANCHE_DURATION: &str = "Tranche Duration";
pub const AGENTS_BUYING_IN_TRANCHE: &str = "Buys per bundle";
pub const AGENTS_SELLING_IN_TRANCHE: &str = "Sells per bundle";
pub const AGENTS_KEEP_TOKENS_LAMPORTS: &str = "Number of Tokens to Keep";

pub fn button_target_pool(
    strategy_being_configured: &VolumeStrategyConfigArgs,
) -> InlineKeyboardButton {
    let target_pool = strategy_being_configured
        .target_pool
        .clone()
        .unwrap_or_default();
    let button_text = if target_pool.is_empty() {
        BUTTON_TARGET_POOL.to_string()
    } else {
        format!("{}: {}", BUTTON_TARGET_POOL, target_pool)
    };
    InlineKeyboardButton::callback(button_text, BUTTON_TARGET_POOL)
}

pub fn button_tranche_size_sol(
    strategy_being_configured: &VolumeStrategyConfigArgs,
) -> InlineKeyboardButton {
    let variable = strategy_being_configured
        .tranche_size_sol
        .clone()
        .unwrap_or_default();
    let button_text = if variable == 0.0 {
        BUTTON_TRANCHE_SIZE_SOL.to_string()
    } else {
        format!(
            "{}: {} SOL",
            BUTTON_TRANCHE_SIZE_SOL,
            utils::formatters::format_sol(variable)
        )
    };
    InlineKeyboardButton::callback(button_text, BUTTON_TRANCHE_SIZE_SOL)
}

pub fn button_tranche_frequency_hbs(
    strategy_being_configured: &VolumeStrategyConfigArgs,
) -> InlineKeyboardButton {
    let variable = strategy_being_configured
        .tranche_frequency_hbs
        .clone()
        .unwrap_or_default();
    let button_text = if variable == 0 {
        TRANCHE_FREQUENCY_HBS.to_string()
    } else {
        format!("{}: {} s", TRANCHE_FREQUENCY_HBS, variable)
    };
    InlineKeyboardButton::callback(button_text, TRANCHE_FREQUENCY_HBS)
}

pub fn button_tranche_length_hbs(
    strategy_being_configured: &VolumeStrategyConfigArgs,
) -> InlineKeyboardButton {
    let variable = strategy_being_configured
        .tranche_length_hbs
        .clone()
        .unwrap_or_default();
    let button_text = if variable == 0 {
        BUTTON_TRANCHE_DURATION.to_string()
    } else {
        format!("{}: {} s", BUTTON_TRANCHE_DURATION, variable)
    };
    InlineKeyboardButton::callback(button_text, BUTTON_TRANCHE_DURATION)
}

pub fn button_agents_buying_in_tranche(
    strategy_being_configured: &VolumeStrategyConfigArgs,
) -> InlineKeyboardButton {
    let variable = strategy_being_configured
        .agents_buying_in_tranche
        .clone()
        .unwrap_or_default();
    let button_text = if variable == 0 {
        AGENTS_BUYING_IN_TRANCHE.to_string()
    } else {
        format!("{}: {:.2}", AGENTS_BUYING_IN_TRANCHE, variable)
    };
    InlineKeyboardButton::callback(button_text, AGENTS_BUYING_IN_TRANCHE)
}

pub fn button_agents_selling_in_tranche(
    strategy_being_configured: &VolumeStrategyConfigArgs,
) -> InlineKeyboardButton {
    let variable = strategy_being_configured
        .agents_selling_in_tranche
        .clone()
        .unwrap_or_default();
    let button_text = if variable == 0 {
        AGENTS_SELLING_IN_TRANCHE.to_string()
    } else {
        format!("{}: {:.2}", AGENTS_SELLING_IN_TRANCHE, variable)
    };
    InlineKeyboardButton::callback(button_text, AGENTS_SELLING_IN_TRANCHE)
}

pub fn button_agents_keep_tokens_lamports(
    strategy_being_configured: &VolumeStrategyConfigArgs,
) -> InlineKeyboardButton {
    let variable = strategy_being_configured
        .agents_keep_tokens_lamports
        .clone()
        .unwrap_or_default();
    let button_text = if variable == 0 {
        AGENTS_KEEP_TOKENS_LAMPORTS.to_string()
    } else {
        format!("{}: {:.2}", AGENTS_KEEP_TOKENS_LAMPORTS, variable)
    };
    InlineKeyboardButton::callback(button_text, AGENTS_KEEP_TOKENS_LAMPORTS)
}
