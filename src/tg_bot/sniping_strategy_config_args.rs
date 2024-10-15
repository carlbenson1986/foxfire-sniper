use std::fmt::Debug;
use crate::types::sniping_strategy::{NewSnipingStrategyInstance, SnipingStrategyInstance};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

pub trait UpdateConfig {
    fn update(&mut self, new_config: Self);
}
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SnipingStrategyConfigArgs {
    pub user_id: Option<i32>,
    pub sniper_privkey: Option<String>,
    pub size_sol: Option<f64>,
    pub stop_loss_percent_move_down: Option<f64>,
    pub take_profit_percent_move_up: Option<f64>,
    pub force_exit_horizon_s: Option<i64>,
    pub max_simultaneous_snipes: Option<i64>,
    pub min_pool_liquidity_sol: Option<f64>,
    pub skip_pump_fun: Option<bool>,
    pub skip_mintable: Option<bool>,
    pub buy_delay_ms: Option<i64>,
    pub skip_if_price_drops_percent: Option<f64>,

}

impl Debug for SnipingStrategyConfigArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnipingStrategyConfigArgs")
            .field("user_id", &self.user_id)
            .field("sniper_privkey", match &self.sniper_privkey {
                Some(_) => &"Some(<hidden>)",
                None => &"None",
            })
            .field("size_sol", &self.size_sol)
            .field("stop_loss_percent_move_down", &self.stop_loss_percent_move_down)
            .field("take_profit_percent_move_up", &self.take_profit_percent_move_up)
            .field("force_exit_horizon_s", &self.force_exit_horizon_s)
            .field("max_simultaneous_snipes", &self.max_simultaneous_snipes)
            .field("min_pool_liquidity_sol", &self.min_pool_liquidity_sol)
            .field("skip_pump_fun", &self.skip_pump_fun)
            .field("skip_mintable", &self.skip_mintable)
            .field("buy_delay_ms", &self.buy_delay_ms)
            .field("skip_if_price_drops_percent", &self.skip_if_price_drops_percent)
            .finish()
    }
}

impl SnipingStrategyConfigArgs {
    pub fn is_ready(&self) -> bool {
        self.sniper_privkey.is_some()
            && self.size_sol.is_some()
            && self.stop_loss_percent_move_down.is_some()
            && self.take_profit_percent_move_up.is_some()
            && self.force_exit_horizon_s.is_some()
            && self.max_simultaneous_snipes.is_some()
            && self.min_pool_liquidity_sol.is_some()
    }

    pub fn missing_fields(&self) -> Vec<&str> {
        let mut missing_fields = vec![];
        if self.size_sol.is_none() {
            missing_fields.push("size sol\n");
        }
        if self.stop_loss_percent_move_down.is_none() {
            missing_fields.push("stop loss percent move down\n");
        }
        if self.take_profit_percent_move_up.is_none() {
            missing_fields.push("take profit percent move up\n");
        }
        if self.force_exit_horizon_s.is_none() {
            missing_fields.push("force exit horizon s\n");
        }
        missing_fields
    }
}

impl TryFrom<&SnipingStrategyConfigArgs> for NewSnipingStrategyInstance {
    type Error = &'static str;

    fn try_from(value: &SnipingStrategyConfigArgs) -> Result<Self, Self::Error> {
        Ok(NewSnipingStrategyInstance {
            user_id: value.user_id.ok_or("user_id is None")?,
            started_at: chrono::Utc::now().naive_utc(),
            completed_at: None,
            sniper_private_key: value.sniper_privkey.clone().ok_or("sniper_privkey is None")?,
            size_sol: value.size_sol.ok_or("size_sol is None")?,
            stop_loss_percent_move_down: value
                .stop_loss_percent_move_down
                .ok_or("stop_loss_percent_move_down is None")?,
            take_profit_percent_move_up: value
                .take_profit_percent_move_up
                .ok_or("take_profit_percent_move_up is None")?,
            force_exit_horizon_s: value
                .force_exit_horizon_s
                .ok_or("force_exit_horizon_s is None")?,
            max_simultaneous_snipes: value.max_simultaneous_snipes.unwrap_or(1),
            min_pool_liquidity_sol: value.min_pool_liquidity_sol.ok_or("min_pool_liquidity_sol is None")?,
            skip_pump_fun: value.skip_pump_fun.unwrap_or(false),
            skip_mintable: value.skip_mintable.unwrap_or(false),
            buy_delay_ms: value.buy_delay_ms.unwrap_or(0),
            skip_if_price_drops_percent: value.skip_if_price_drops_percent.unwrap_or(0.0),
        })
    }
}

impl UpdateConfig for SnipingStrategyConfigArgs {
    fn update(&mut self, new_config: Self) {
        if let Some(user_id) = new_config.user_id {
            self.user_id = Some(user_id);
        }
        if let Some(sniper_privkey) = new_config.sniper_privkey.clone() {
            self.sniper_privkey = Some(sniper_privkey);
        }
        if let Some(size_sol) = new_config.size_sol {
            self.size_sol = Some(size_sol);
        }
        if let Some(stop_loss_percent_move_down) = new_config.stop_loss_percent_move_down {
            self.stop_loss_percent_move_down = Some(stop_loss_percent_move_down);
        }
        if let Some(take_profit_percent_move_up) = new_config.take_profit_percent_move_up {
            self.take_profit_percent_move_up = Some(take_profit_percent_move_up);
        }
        if let Some(force_exit_horizon_s) = new_config.force_exit_horizon_s {
            self.force_exit_horizon_s = Some(force_exit_horizon_s);
        }
    }
}
