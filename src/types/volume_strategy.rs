use crate::schema::*;
use crate::types::bot_user::BotUser;
use crate::types::engine::StrategyId;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::sql_types::*;
use diesel_derives::{Associations, Identifiable, Insertable, Queryable, Selectable};
use serde_derive::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

#[derive(
    Default,
    Debug,
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
#[table_name = "volumestrategyinstances"]
pub struct VolumeStrategyInstance {
    pub id: StrategyId,
    pub user_id: i32,
    #[diesel(
        sql_type = Nullable < Text >,
        serialize_as = crate::utils::serdealizers::PubkeyString,
        deserialize_as = crate::utils::serdealizers::PubkeyString
    )]
    pub target_pool: Pubkey,
    pub started_at: chrono::NaiveDateTime,
    pub completed_at: Option<chrono::NaiveDateTime>,
    pub tranche_size_sol: f64,
    pub tranche_frequency_hbs: i64,
    pub tranche_length_hbs: i64,
    pub agents_buying_in_tranche: i32,
    pub agents_selling_in_tranche: i32,
    pub agents_keep_tokens_lamports: i64,
}
#[derive(Debug, Clone, Insertable, Associations)]
#[diesel(check_for_backend(Pg))]
#[belongs_to(BotUser, foreign_key = "user_id")]
#[table_name = "volumestrategyinstances"]
pub struct NewVolumeStrategyInstance {
    pub user_id: i32,
    #[diesel(
        sql_type = Nullable < Text >,
        serialize_as = crate::utils::serdealizers::PubkeyString,
        deserialize_as = crate::utils::serdealizers::PubkeyString
    )]
    pub target_pool: Pubkey,
    pub started_at: chrono::NaiveDateTime,
    pub completed_at: Option<chrono::NaiveDateTime>,
    pub tranche_size_sol: f64,
    pub tranche_frequency_hbs: i64,
    pub tranche_length_hbs: i64,
    pub agents_buying_in_tranche: i32,
    pub agents_selling_in_tranche: i32,
    pub agents_keep_tokens_lamports: i64,
}


impl From<&VolumeStrategyInstance> for NewVolumeStrategyInstance {
    fn from(new: &VolumeStrategyInstance) -> Self {
        NewVolumeStrategyInstance {
            user_id: new.user_id,
            target_pool: new.target_pool,
            started_at: chrono::Utc::now().naive_utc(),
            completed_at: new.completed_at,
            tranche_size_sol: new.tranche_size_sol,
            tranche_frequency_hbs: new.tranche_frequency_hbs,
            tranche_length_hbs: new.tranche_length_hbs,
            agents_buying_in_tranche: new.agents_buying_in_tranche,
            agents_selling_in_tranche: new.agents_selling_in_tranche,
            agents_keep_tokens_lamports: new.agents_keep_tokens_lamports,
        }
    }
}
