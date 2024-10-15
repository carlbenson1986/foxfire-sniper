use crate::tg_bot::volume_strategy_config_args::{UpdateConfig, VolumeStrategyConfigArgs};
use crate::types::engine::Strategy;
use std::fmt::{Display, Formatter};
use strum_macros::Display;
use teloxide::dispatching::dialogue::serializer::Json;
use teloxide::dispatching::dialogue::RedisStorage;
use teloxide::prelude::Dialogue;
use teloxide::types::Message;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DialogueMessages {
    pub(crate) message_to_edit: Message,
    pub(crate) message_to_delete: Message,
}

pub type MyDialogue = Dialogue<State, RedisStorage<Json>>;

#[derive(Display, Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum State {
    #[default]
    // Start screen: Initial state
    Init,
    // Strategy screen: Strategy selection state - strategy menu opened (Volume bot/ MM bot/ etc)
    ReceiveStrategy {
        strategy_in_progress: Option<VolumeStrategyConfigArgs>,
    },
    // Awaiting token input for the strategy
    ReceiveTokenAddress {
        strategy_in_progress: Option<VolumeStrategyConfigArgs>,
        strategy_menu_message: DialogueMessages,
    },
    ReceiveTrancheSizeSol {
        strategy_in_progress: Option<VolumeStrategyConfigArgs>,
        strategy_menu_message: DialogueMessages,
    },
    ReceiveTrancheFrequencyHbs {
        strategy_in_progress: Option<VolumeStrategyConfigArgs>,
        strategy_menu_message: DialogueMessages,
    },
    ReceiveTrancheLengthHbs {
        strategy_in_progress: Option<VolumeStrategyConfigArgs>,
        strategy_menu_message: DialogueMessages,
    },
    ReceiveAgentsBuyingInTranche {
        strategy_in_progress: Option<VolumeStrategyConfigArgs>,
        strategy_menu_message: DialogueMessages,
    },
    ReceiveAgentsSellingInTranche {
        strategy_in_progress: Option<VolumeStrategyConfigArgs>,
        strategy_menu_message: DialogueMessages,
    },
    ReceiveButtonAgentsKeepTokensLamports {
        strategy_in_progress: Option<VolumeStrategyConfigArgs>,
        strategy_menu_message: DialogueMessages,
    },
    // List of strategies or the start screen with the strategy selected
    StrategySelected {
        strategy_in_progress: Option<VolumeStrategyConfigArgs>,
        selected_strategy_id: Option<i32>,
    },
}

// Transition functions
impl State {
    pub fn to_main_menu(&self) -> Self {
        self.get_strategy_in_progress_in_any()
            .map_or(State::Init, |_| State::StrategySelected {
                strategy_in_progress: self.get_strategy_in_progress_in_any(),
                selected_strategy_id: None,
            })
    }

    pub fn back(self) -> Self {
        self.to_main_menu()
    }

    pub fn to_receive_token_address(&self, message: DialogueMessages) -> Self {
        State::ReceiveTokenAddress {
            strategy_in_progress: self.get_strategy_in_progress_in_any(),
            strategy_menu_message: message.clone(),
        }
    }

    pub fn to_receive_tranche_size(&self, message: DialogueMessages) -> Self {
        State::ReceiveTrancheSizeSol {
            strategy_in_progress: self.get_strategy_in_progress_in_any(),
            strategy_menu_message: message.clone(),
        }
    }

    pub fn to_receive_tranche_frequency_hbs(&self, message: DialogueMessages) -> Self {
        State::ReceiveTrancheFrequencyHbs {
            strategy_in_progress: self.get_strategy_in_progress_in_any(),
            strategy_menu_message: message.clone(),
        }
    }

    pub fn to_receive_tranche_length_hbs(&self, message: DialogueMessages) -> Self {
        State::ReceiveTrancheLengthHbs {
            strategy_in_progress: self.get_strategy_in_progress_in_any(),
            strategy_menu_message: message.clone(),
        }
    }

    pub fn to_receive_agents_buying_in_tranche(&self, message: DialogueMessages) -> Self {
        State::ReceiveAgentsBuyingInTranche {
            strategy_in_progress: self.get_strategy_in_progress_in_any(),
            strategy_menu_message: message.clone(),
        }
    }

    pub fn to_receive_agents_selling_in_tranche(&self, message: DialogueMessages) -> Self {
        State::ReceiveAgentsSellingInTranche {
            strategy_in_progress: self.get_strategy_in_progress_in_any(),
            strategy_menu_message: message.clone(),
        }
    }

    pub fn to_receive_button_agents_keep_tokens_lamports(&self, message: DialogueMessages) -> Self {
        State::ReceiveButtonAgentsKeepTokensLamports {
            strategy_in_progress: self.get_strategy_in_progress_in_any(),
            strategy_menu_message: message.clone(),
        }
    }

    pub fn to_receive_strategy(&self) -> Self {
        State::ReceiveStrategy {
            strategy_in_progress: self.get_strategy_in_progress_in_any(),
        }
    }

    pub fn to_selected_strategy(&self, strategy_id: i32) -> Self {
        State::StrategySelected {
            strategy_in_progress: self.get_strategy_in_progress_in_any(),
            selected_strategy_id: Some(strategy_id),
        }
    }

    pub fn reset(self) -> Self {
        self.get_strategy_in_progress_in_any()
            .map(|strat| State::StrategySelected {
                strategy_in_progress: Some(strat),
                selected_strategy_id: None,
            })
            .unwrap_or(State::Init)
    }

    pub fn get_strategy_in_progress_in_any(&self) -> Option<VolumeStrategyConfigArgs> {
        match self {
            State::ReceiveStrategy {
                strategy_in_progress,
            } => strategy_in_progress.clone(),
            State::ReceiveTokenAddress {
                strategy_in_progress,
                ..
            } => strategy_in_progress.clone(),
            State::ReceiveTrancheSizeSol {
                strategy_in_progress,
                ..
            } => strategy_in_progress.clone(),
            State::ReceiveTrancheFrequencyHbs {
                strategy_in_progress,
                ..
            } => strategy_in_progress.clone(),
            State::ReceiveTrancheLengthHbs {
                strategy_in_progress,
                ..
            } => strategy_in_progress.clone(),
            State::ReceiveAgentsBuyingInTranche {
                strategy_in_progress,
                ..
            } => strategy_in_progress.clone(),
            State::ReceiveAgentsSellingInTranche {
                strategy_in_progress,
                ..
            } => strategy_in_progress.clone(),
            State::ReceiveButtonAgentsKeepTokensLamports {
                strategy_in_progress,
                ..
            } => strategy_in_progress.clone(),
            State::StrategySelected {
                strategy_in_progress,
                ..
            } => strategy_in_progress.clone(),
            _ => None,
        }
    }

    pub fn update_configured_strategy(&self, strategy_update: VolumeStrategyConfigArgs) -> Self {
        let mut strategy = self.get_strategy_in_progress();
        strategy.update(strategy_update);
        match self {
            State::ReceiveStrategy { .. } => State::ReceiveStrategy {
                strategy_in_progress: Some(strategy),
            },
            State::ReceiveTokenAddress {
                strategy_menu_message,
                ..
            } => State::ReceiveStrategy {
                strategy_in_progress: Some(strategy),
            },
            State::ReceiveTrancheSizeSol {
                strategy_menu_message,
                ..
            } => State::ReceiveStrategy {
                strategy_in_progress: Some(strategy),
            },
            State::ReceiveTrancheFrequencyHbs {
                strategy_menu_message,
                ..
            } => State::ReceiveStrategy {
                strategy_in_progress: Some(strategy),
            },
            State::ReceiveTrancheLengthHbs {
                strategy_menu_message,
                ..
            } => State::ReceiveStrategy {
                strategy_in_progress: Some(strategy),
            },
            State::ReceiveAgentsBuyingInTranche {
                strategy_menu_message,
                ..
            } => State::ReceiveStrategy {
                strategy_in_progress: Some(strategy),
            },
            State::ReceiveAgentsSellingInTranche {
                strategy_menu_message,
                ..
            } => State::ReceiveStrategy {
                strategy_in_progress: Some(strategy),
            },
            State::ReceiveButtonAgentsKeepTokensLamports {
                strategy_menu_message,
                ..
            } => State::ReceiveStrategy {
                strategy_in_progress: Some(strategy),
            },
            State::StrategySelected {
                selected_strategy_id,
                ..
            } => State::StrategySelected {
                strategy_in_progress: Some(strategy),
                selected_strategy_id: selected_strategy_id.clone(),
            },
            _ => State::ReceiveStrategy {
                strategy_in_progress: Some(strategy),
            },
        }
    }

    pub fn get_strategy_in_progress(&self) -> VolumeStrategyConfigArgs {
        self.get_strategy_in_progress_in_any()
            .unwrap_or_else(|| VolumeStrategyConfigArgs::default())
    }

    pub fn awaiting_text_input(&self) -> bool {
        match self {
            State::ReceiveTokenAddress { .. } => true,
            State::ReceiveTrancheSizeSol { .. } => true,
            State::ReceiveTrancheFrequencyHbs { .. } => true,
            State::ReceiveTrancheLengthHbs { .. } => true,
            State::ReceiveAgentsBuyingInTranche { .. } => true,
            State::ReceiveAgentsSellingInTranche { .. } => true,
            State::ReceiveButtonAgentsKeepTokensLamports { .. } => true,
            _ => false,
        }
    }

    pub fn cancel_text_input(&self) -> Self {
        if self.awaiting_text_input() {
            self.get_strategy_in_progress_in_any()
                .map(|strategy| State::ReceiveStrategy {
                    strategy_in_progress: Some(strategy),
                })
                .unwrap_or(State::Init)
        } else {
            self.clone()
        }
    }

    pub fn to_strategies_list(self) -> Self {
        State::ReceiveStrategy {
            strategy_in_progress: self.get_strategy_in_progress_in_any(),
        }
    }

    pub fn get_message_to_delete(&self) -> Option<Message> {
        match self {
            State::ReceiveTokenAddress {
                strategy_menu_message,
                ..
            } => Some(strategy_menu_message.message_to_delete.clone()),
            State::ReceiveTrancheSizeSol {
                strategy_menu_message,
                ..
            } => Some(strategy_menu_message.message_to_delete.clone()),
            State::ReceiveTrancheFrequencyHbs {
                strategy_menu_message,
                ..
            } => Some(strategy_menu_message.message_to_delete.clone()),
            State::ReceiveTrancheLengthHbs {
                strategy_menu_message,
                ..
            } => Some(strategy_menu_message.message_to_delete.clone()),
            State::ReceiveAgentsBuyingInTranche {
                strategy_menu_message,
                ..
            } => Some(strategy_menu_message.message_to_delete.clone()),
            State::ReceiveAgentsSellingInTranche {
                strategy_menu_message,
                ..
            } => Some(strategy_menu_message.message_to_delete.clone()),
            State::ReceiveButtonAgentsKeepTokensLamports {
                strategy_menu_message,
                ..
            } => Some(strategy_menu_message.message_to_delete.clone()),
            _ => None,
        }
    }
}
