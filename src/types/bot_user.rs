use crate::schema::*;
use crate::types::engine::StrategyId;
use crate::utils::keys::private_key_string_base58;
use crate::types::volume_strategy::VolumeStrategyInstance;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::sql_types::*;
use diesel::Insertable;
use serde_derive::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature, Signer};
use teloxide::prelude::ChatId;

#[derive(Debug, Clone, Serialize, Deserialize, Insertable, Default, Queryable, Identifiable)]
#[diesel(table_name = depositswithdrawals)]
pub struct DepositsWithdrawals {
    pub id: i32,
    pub user_id: Option<i32>,
    pub time: chrono::NaiveDateTime,
    pub is_deposit: bool,
    pub amount_sol: Option<f64>,
    pub is_success: bool,
    #[diesel(serialize_as = crate::utils::serdealizers::SignatureString)]
    pub signature: Signature,
    pub signature_fee: Option<String>,
    pub fee_taken_sol: Option<f64>,
    pub description: Option<String>,
}

#[derive(Default, Debug, Clone, Selectable, Queryable, Identifiable, Associations)]
#[belongs_to(VolumeStrategyInstance, foreign_key = "strategy_instance_id")]
#[table_name = "traders"]
pub struct Trader {
    pub id: i32,
    pub strategy_instance_id: Option<i32>,
    #[diesel(
        serialize_as = crate::utils::serdealizers::PubkeyString,
        deserialize_as = crate::utils::serdealizers::PubkeyString,
    )]
    pub wallet: Pubkey,
    pub private_key: String,
    pub created: chrono::NaiveDateTime,
    pub is_active: bool,
}

#[derive(Clone, Insertable, Associations, Debug)]
#[belongs_to(VolumeStrategyInstance, foreign_key = "strategy_instance_id")]
#[table_name = "traders"]
pub struct NewTrader {
    pub strategy_instance_id: Option<i32>,
    #[diesel(
        serialize_as = crate::utils::serdealizers::PubkeyString,
        deserialize_as = crate::utils::serdealizers::PubkeyString,
    )]
    pub wallet: Pubkey,
    pub private_key: String,
    pub created: chrono::NaiveDateTime,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Queryable, Identifiable, Selectable)]
#[diesel(check_for_backend(Pg))]
#[table_name = "users"]
pub struct BotUser {
    pub id: i32,
    pub chat_id: i64,
    pub tg_name: String,
    #[diesel(
        serialize_as = crate::utils::serdealizers::PubkeyString,
        deserialize_as = crate::utils::serdealizers::PubkeyString,
    )]
    pub wallet_address: Pubkey,
    pub wallet_private_key: String,
    pub created: chrono::NaiveDateTime,
    pub last_login: chrono::NaiveDateTime,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub is_active: bool,
    pub is_superuser: bool,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(check_for_backend(Pg))]
#[table_name = "users"]
pub struct NewBotUser {
    pub chat_id: i64,
    pub tg_name: String,
    #[diesel(
        serialize_as = crate::utils::serdealizers::PubkeyString,
        deserialize_as = crate::utils::serdealizers::PubkeyString,
    )]
    pub wallet_address: Pubkey,
    pub wallet_private_key: String,
    pub created: chrono::NaiveDateTime,
    pub last_login: chrono::NaiveDateTime,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub is_active: bool,
    pub is_superuser: bool,
}

impl NewBotUser {
    pub(crate) fn new_from_tg_user(user: &teloxide::types::User) -> Self {
        let keypair = Keypair::new();
        NewBotUser {
            chat_id: user.id.0 as i64,
            tg_name: user.username.clone().unwrap_or("".to_string()),
            wallet_address: keypair.pubkey(),
            wallet_private_key: private_key_string_base58(&keypair),
            created: chrono::Utc::now().naive_utc(),
            last_login: chrono::Utc::now().naive_utc(),
            first_name: Some(user.first_name.clone()),
            last_name: user.last_name.clone(),
            is_active: true,
            is_superuser: false,
        }
    }
    pub fn get_chat_id(&self) -> ChatId {
        ChatId(self.chat_id)
    }
}
