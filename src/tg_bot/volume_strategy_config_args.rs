use crate::types::volume_strategy::VolumeStrategyInstance;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

pub trait UpdateConfig {
    fn update(&mut self, new_config: Self);
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct VolumeStrategyConfigArgs {
    pub user_id: Option<i32>,
    pub target_pool: Option<String>,
    pub tranche_size_sol: Option<f64>,
    pub tranche_frequency_hbs: Option<i64>,
    pub tranche_length_hbs: Option<i64>,
    pub agents_buying_in_tranche: Option<i32>,
    pub agents_selling_in_tranche: Option<i32>,
    pub agents_keep_tokens_lamports: Option<i64>,
}

impl VolumeStrategyConfigArgs {
    pub fn is_ready(&self) -> bool {
        self.tranche_size_sol.is_some()
            && self.tranche_frequency_hbs.is_some()
            && self.tranche_length_hbs.is_some()
            && self.target_pool.is_some()
            && self.agents_buying_in_tranche.is_some()
            && self.agents_selling_in_tranche.is_some()
    }

    pub fn missing_fields(&self) -> Vec<&str> {
        let mut missing_fields = vec![];
        if self.tranche_size_sol.is_none() {
            missing_fields.push("tranche size sol\n");
        }
        if self.tranche_frequency_hbs.is_none() {
            missing_fields.push("tranche frequency hbs\n");
        }
        if self.tranche_length_hbs.is_none() {
            missing_fields.push("tranche length hbs\n");
        }
        if self.target_pool.is_none() {
            missing_fields.push("target pool\n");
        }
        if self.agents_buying_in_tranche.is_none() {
            missing_fields.push("agents buying in tranche\n");
        }
        if self.agents_selling_in_tranche.is_none() {
            missing_fields.push("agents selling in tranche");
        }
        missing_fields
    }
}

impl TryFrom<&VolumeStrategyConfigArgs> for VolumeStrategyInstance {
    type Error = &'static str;

    fn try_from(value: &VolumeStrategyConfigArgs) -> Result<Self, Self::Error> {
        Ok(VolumeStrategyInstance {
            id: 0,
            user_id: value.user_id.ok_or("user_id is None")?,
            target_pool: Pubkey::from_str(&value.target_pool.clone().ok_or("target_pool is None")?)
                .map_err(|_| "target_pool is not a valid Pubkey")?,
            started_at: Default::default(),
            completed_at: None,
            tranche_size_sol: value.tranche_size_sol.ok_or("tranche_size_sol is None")?,
            tranche_frequency_hbs: value
                .tranche_frequency_hbs
                .ok_or("tranche_frequency_hbs is None")?,
            tranche_length_hbs: value
                .tranche_length_hbs
                .ok_or("tranche_length_hbs is None")?,
            agents_buying_in_tranche: value
                .agents_buying_in_tranche
                .ok_or("agents_buying_in_tranche is None")?,
            agents_selling_in_tranche: value
                .agents_selling_in_tranche
                .ok_or("agents_selling_in_tranche is None")?,
            agents_keep_tokens_lamports: value
                .agents_keep_tokens_lamports
                .ok_or("agents_keep_tokens_lamports is None")?,
        })
    }
}

impl UpdateConfig for VolumeStrategyConfigArgs {
    fn update(&mut self, new_config: Self) {
        if let Some(user_id) = new_config.user_id {
            self.user_id = Some(user_id);
        }
        if let Some(target_pool) = new_config.target_pool {
            self.target_pool = Some(target_pool);
        }
        if let Some(tranche_size_sol) = new_config.tranche_size_sol {
            self.tranche_size_sol = Some(tranche_size_sol);
        }
        if let Some(tranche_frequency_hbs) = new_config.tranche_frequency_hbs {
            self.tranche_frequency_hbs = Some(tranche_frequency_hbs);
        }
        if let Some(tranche_length_hbs) = new_config.tranche_length_hbs {
            self.tranche_length_hbs = Some(tranche_length_hbs);
        }
        if let Some(agents_buying_in_tranche) = new_config.agents_buying_in_tranche {
            self.agents_buying_in_tranche = Some(agents_buying_in_tranche);
        }
        if let Some(agents_selling_in_tranche) = new_config.agents_selling_in_tranche {
            self.agents_selling_in_tranche = Some(agents_selling_in_tranche);
        }
        if let Some(agents_keep_tokens_lamports) = new_config.agents_keep_tokens_lamports {
            self.agents_keep_tokens_lamports = Some(agents_keep_tokens_lamports);
        }
    }
}
