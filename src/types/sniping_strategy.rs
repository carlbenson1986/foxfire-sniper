use std::fmt::{Debug, Formatter};
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::sql_types::*;
use serde_derive::{Deserialize, Serialize};
use diesel_derives::{Associations, Identifiable, Insertable, Queryable, Selectable};
use solana_sdk::signature::{Keypair, Signer};
use crate::schema::*;
use crate::types::engine::StrategyId;
use crate::types::bot_user::{BotUser};
use crate::types::volume_strategy::{NewVolumeStrategyInstance, VolumeStrategyInstance};

#[derive(
    Default,
    Clone,
    Serialize,
    Deserialize,
    Queryable,
    Selectable,
    Associations,
)]
#[diesel(check_for_backend(Pg))]
#[serde(rename_all = "lowercase")]
#[belongs_to(BotUser, foreign_key = "user_id")]
#[table_name = "snipingstrategyinstances"]
pub struct SnipingStrategyInstance {
    pub id: StrategyId,
    pub user_id: i32,
    pub started_at: chrono::NaiveDateTime,
    pub completed_at: Option<chrono::NaiveDateTime>,
    pub sniper_private_key: String,
    pub size_sol: f64,
    pub stop_loss_percent_move_down: f64,
    pub take_profit_percent_move_up: f64,
    pub force_exit_horizon_s: i64,
    pub max_simultaneous_snipes: i64,
    pub min_pool_liquidity_sol: f64,
    pub skip_pump_fun: bool,
    pub skip_mintable: bool,
    pub buy_delay_ms: i64,
    pub skip_if_price_drops_percent: f64,
}
impl Debug for SnipingStrategyInstance {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "SnipingStrategyInstance {{\n    id: {:#?},\n    user_id: {:#?},\n    started_at: {:#?},\n    completed_at: {:#?},\n    sniper_public_key: {:?},\n    size_sol: {:#?},\n    stop_loss_percent_move_down: {:#?},\n    take_profit_percent_move_up: {:#?},\n    force_exit_horizon_s: {:#?},\n    max_simultaneous_snipes: {:#?},\n    min_pool_liquidity_sol: {:#?},\n    skip_pump_fun: {:#?},\n    skip_mintable: {:#?}\n}}",
               self.id,
               self.user_id,
               self.started_at,
               self.completed_at,
               Keypair::from_base58_string(&self.sniper_private_key).pubkey(),
               self.size_sol,
               self.stop_loss_percent_move_down,
               self.take_profit_percent_move_up,
               self.force_exit_horizon_s,
               self.max_simultaneous_snipes,
               self.min_pool_liquidity_sol,
               self.skip_pump_fun,
               self.skip_mintable,
        )
    }
}

#[derive(Debug, Clone, Insertable, Associations)]
#[diesel(check_for_backend(Pg))]
#[belongs_to(BotUser, foreign_key = "user_id")]
#[table_name = "snipingstrategyinstances"]
pub struct NewSnipingStrategyInstance {
    pub user_id: i32,
    pub started_at: chrono::NaiveDateTime,
    pub completed_at: Option<chrono::NaiveDateTime>,
    pub sniper_private_key: String,
    pub size_sol: f64,
    pub stop_loss_percent_move_down: f64,
    pub take_profit_percent_move_up: f64,
    pub force_exit_horizon_s: i64,
    pub max_simultaneous_snipes: i64,
    pub min_pool_liquidity_sol: f64,
    pub skip_pump_fun: bool,
    pub skip_mintable: bool,
    pub buy_delay_ms: i64,
    pub skip_if_price_drops_percent: f64,
}


impl From<&NewSnipingStrategyInstance> for SnipingStrategyInstance {
    fn from(new: &NewSnipingStrategyInstance) -> Self {
        SnipingStrategyInstance {
            id: StrategyId::default(),
            user_id: new.user_id,
            started_at: new.started_at,
            completed_at: new.completed_at,
            sniper_private_key: new.sniper_private_key.clone(),
            size_sol: new.size_sol,
            stop_loss_percent_move_down: new.stop_loss_percent_move_down,
            take_profit_percent_move_up: new.take_profit_percent_move_up,
            force_exit_horizon_s: new.force_exit_horizon_s,
            max_simultaneous_snipes: new.max_simultaneous_snipes,
            min_pool_liquidity_sol: new.min_pool_liquidity_sol,
            skip_pump_fun: new.skip_pump_fun,
            skip_mintable: new.skip_mintable,
            buy_delay_ms: new.buy_delay_ms,
            skip_if_price_drops_percent: new.skip_if_price_drops_percent,
        }
    }
}
