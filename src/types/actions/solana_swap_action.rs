use serde_derive::{Deserialize, Serialize};
use solana_farm_client::raydium_sdk::{LiquidityPoolKeys, MarketStateLayoutV3};
use solana_sdk::pubkey::Pubkey;
use crate::types::actions::Amount;
use crate::types::keys::KeypairClonable;
use crate::types::pool::RaydiumPool;


#[derive(Debug, Clone, Copy,Default,  Serialize, Deserialize, PartialEq, Eq)]
pub enum SwapMethod {
    #[default]
    BuyTokensForExactSol,
    SellExactTokensForSol,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolanaSwapActionPayload {
    pub keys: LiquidityPoolKeys,
    pub swap_method: SwapMethod,
    pub amount_in: Amount,
    pub min_amount_out: u64,
}

impl SolanaSwapActionPayload {
    pub fn new(
        pool: &RaydiumPool,
        sniper: KeypairClonable,
        swap_method: SwapMethod,
        amount_in: Amount,
    ) -> Self {
        SolanaSwapActionPayload {
            keys: pool.to_liquidity_keys(),
            swap_method,
            amount_in,
            min_amount_out: 0,
        }
    }
    pub(crate) fn add_market_state_layout(&mut self, market_info: MarketStateLayoutV3) {
        self.keys.market_base_vault = market_info.base_vault;
        self.keys.market_quote_vault = market_info.quote_vault;
        self.keys.market_bids = market_info.bids;
        self.keys.market_asks = market_info.asks;
        self.keys.market_event_queue = market_info.event_queue;
    }
}