use crate::config::app_context::AppContext;
use crate::strategies::VolumeStrategy;
use crate::tg_bot::bot_config::BotConfig;
use crate::tg_bot::helpers::buttons::ButtonMenu;
use crate::tg_bot::user_menu::top::handler::{
    BUTTON_BACK_TO_THE_MAIN_MENU, BUTTON_CONFIGURE_STRATEGY, BUTTON_STOP_STRATEGIES,
};
use crate::types::engine::StrategyManager;
use crate::types::bot_user::BotUser;
use futures::stream::{self, StreamExt};
use once_cell::sync::Lazy;

pub async fn get_menu_top(context: &BotConfig, user: &BotUser) -> ButtonMenu {
    let user_strategies =
        stream::iter(context.strategy_manager.get_active_strategies().await.values())
            .filter_map(|strategy_mutex| async move {
                let strategy = strategy_mutex.lock().await;
                if let Some(volume_strategy) = strategy.as_any().downcast_ref::<VolumeStrategy>() {
                    if volume_strategy.get_user_id() == user.id {
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

    let running_strategies = user_strategies.len();

    let mut top_menu = vec![];
    top_menu.push(vec![(
        "📈 Configure Strategy".to_string(),
        BUTTON_CONFIGURE_STRATEGY.to_string(),
    )]);

    if running_strategies > 0 {
        top_menu.push(vec![(
            format!("🔴 Stop All Running Strategies ({running_strategies})"),
            BUTTON_STOP_STRATEGIES.to_string(),
        )]);
    }
    top_menu.push(vec![(
        "🔃".to_string(),
        BUTTON_BACK_TO_THE_MAIN_MENU.to_string(),
    )]);
    top_menu
}

pub fn update_top_menu_with_selected_asset_and_positions(
    menu_items: &ButtonMenu,
    items: &[String],
) -> ButtonMenu {
    let mut updated_menu = menu_items.clone();
    for row in updated_menu.iter_mut() {
        for button in row.iter_mut() {
            if items.contains(&button.1) {
                button.0 = button.0.to_owned().replace('\u{00A0}', "✅");
            }
        }
    }
    updated_menu
}
