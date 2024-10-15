use crate::config::app_context::AppContext;
use crate::schema::users::dsl::users;
use crate::schema::users::wallet_address;
use crate::tg_bot::notify_user;
use crate::types::actions::{Amount, Asset, SolanaAction, SolanaActionPayload, SolanaTransferActionPayload};
use crate::types::engine::{Strategy, StrategyStatus};
use crate::types::events::{BlockchainEvent, BotEvent};
use crate::types::keys::KeypairClonable;
use crate::types::bot_user::BotUser;
use crate::utils::decimals::lamports_to_sol;
use crate::utils::formatters::format_sol;
use anyhow::Result;
use async_trait::async_trait;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use solana_sdk::pubkey::Pubkey;
use std::any::Any;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::str::FromStr;
use std::sync::Arc;
use maplit::hashmap;
use teloxide::payloads::AnswerPreCheckoutQuerySetters;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone)]
pub struct DepositWithdrawStrategy {
    context: AppContext,
}

impl DepositWithdrawStrategy {
    /// Create a new instance of the strategy.
    pub async fn new(context: AppContext) -> Self {
        Self { context }
    }
}

impl Debug for DepositWithdrawStrategy {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "DepositWithdrawStrategy")
    }
}

#[async_trait]
impl Strategy<BotEvent, Arc<Mutex<SolanaAction>>> for DepositWithdrawStrategy {
    /// Initialize the strategy. This is called once at startup
    async fn sync_state(&mut self) -> Result<()> {
        Ok(())
    }

    // Process incoming signals2
    async fn process_event(&mut self, event: BotEvent) -> Vec<Arc<Mutex<SolanaAction>>> {
        match event {
            BotEvent::BlockchainEvent(BlockchainEvent::Deposit(
                                          _signature,
                                          user_wallet,
                                          amount,
                                      )) => {
                let engine_config = self.context.settings.read().await.engine.clone();
                let executor_config = self.context.settings.read().await.executor.clone();
                let bot_wallet_opt = engine_config.bot_wallet.clone();
                if let Some(bot_wallet_str) = bot_wallet_opt {
                    let bot_fee = engine_config.bot_fee.unwrap_or(0.0);
                    let mut conn = self.context.db_pool.get().await.unwrap();
                    if let Ok(user) = users
                        .filter(wallet_address.eq(&user_wallet.to_string()))
                        .first::<BotUser>(&mut conn)
                        .await
                    {
                        notify_user(
                            &self.context.tg_bot.as_ref().unwrap(),
                            user.chat_id,
                            &format!(
                                "💸 Deposit received `{}` SOL",
                                format_sol(lamports_to_sol(amount))
                            ),
                        )
                            .await;
                        let main_wallet =
                            KeypairClonable::new_from_privkey(&user.wallet_private_key).unwrap();
                        vec![Arc::new(Mutex::new(SolanaAction::new(
                            main_wallet.clone(),
                            vec![
                                SolanaActionPayload::SolanaTransferActionPayload(
                                    SolanaTransferActionPayload {
                                        asset: Asset::Sol,
                                        receiver: Pubkey::from_str(&bot_wallet_str).unwrap(),
                                        amount: Amount::Exact((amount as f64 * bot_fee) as u64),
                                    },
                                )],
                        )))]
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn get_status(&self) -> StrategyStatus {
        StrategyStatus::Running(hashmap! {
            "Deposit loop health".to_owned() => "Running".to_owned(),
        })
    }
}
