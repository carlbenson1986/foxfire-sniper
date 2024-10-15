//! Solana Farm Client Pool Instructions

use {
    crate::error::FarmClientError,
    solana_farm_sdk::{
        instruction::amm::AmmInstruction, pool::PoolRoute, token::TokenSelector, Protocol,
    },
    solana_sdk::{instruction::Instruction, pubkey::Pubkey},
};

use super::FarmClient;

impl FarmClient {
    /// Creates a new Instruction for adding liquidity to the Pool.
    /// If one of the token amounts is 0 and pool requires both tokens,
    /// amount will be autocalculated based on the current pool price.
    pub fn new_instruction_add_liquidity_pool(
        &self,
        wallet_address: &Pubkey,
        pool_name: &str,
        max_token_a_ui_amount: f64,
        max_token_b_ui_amount: f64,
    ) -> Result<Instruction, FarmClientError> {
        // get pool info
        let pool = self.get_pool(pool_name)?;

        // get tokens info
        let token_a = self.get_token_by_ref_from_cache(&pool.token_a_ref)?;
        let token_b = self.get_token_by_ref_from_cache(&pool.token_b_ref)?;

        // convert amounts if wrapped tokens are used
        let mut max_token_a_amount =
            self.to_token_amount_option(max_token_a_ui_amount, &token_a)?;
        let mut max_token_b_amount =
            self.to_token_amount_option(max_token_b_ui_amount, &token_b)?;

        // fill in instruction data
        let data = AmmInstruction::AddLiquidity {
            max_token_a_amount,
            max_token_b_amount,
        }
        .to_vec()?;

        let accounts = match pool.route {
            PoolRoute::Raydium { .. } => {
                self.get_add_liquidity_accounts_raydium(wallet_address, pool_name)?
            }
            PoolRoute::Orca { .. } => {
                self.get_add_liquidity_accounts_orca(wallet_address, pool_name)?
            }
        };

        Ok(Instruction {
            program_id: pool.router_program_id,
            data,
            accounts,
        })
    }

    /// Creates a new Instruction for removing liquidity from the Pool
    pub fn new_instruction_remove_liquidity_pool(
        &self,
        wallet_address: &Pubkey,
        pool_name: &str,
        ui_amount: f64,
    ) -> Result<Instruction, FarmClientError> {
        // get pool info
        let pool = self.get_pool(pool_name)?;

        // get tokens info
        let lp_token = self.get_token_by_ref_from_cache(&pool.lp_token_ref)?;

        // fill in instruction data
        let data = AmmInstruction::RemoveLiquidity {
            amount: self.to_token_amount_option(ui_amount, &lp_token)?,
        }
        .to_vec()?;

        let accounts = match pool.route {
            PoolRoute::Raydium { .. } => {
                self.get_remove_liquidity_accounts_raydium(wallet_address, pool_name)?
            }
            PoolRoute::Orca { .. } => {
                self.get_remove_liquidity_accounts_orca(wallet_address, pool_name)?
            }
        };

        Ok(Instruction {
            program_id: pool.router_program_id,
            data,
            accounts,
        })
    }

    /// Creates a new Instruction for tokens swap
    pub fn new_instruction_swap(
        &self,
        wallet_address: &Pubkey,
        protocol: Protocol,
        from_token: &str,
        to_token: &str,
        ui_amount_in: f64,
        min_ui_amount_out: f64,
    ) -> Result<Instruction, FarmClientError> {
        // get pool to swap in
        let pool = self.find_pools(protocol, from_token, to_token)?[0];
        let reverse = FarmClient::pool_has_reverse_tokens(&pool.name, from_token)?;

        // get tokens info
        let token_a = self.get_token_by_ref_from_cache(&pool.token_a_ref)?;
        let token_b = self.get_token_by_ref_from_cache(&pool.token_b_ref)?;

        // convert amounts if wrapped tokens are used
        let mut max_amount_in = if reverse {
            self.to_token_amount_option(ui_amount_in, &token_b)?
        } else {
            self.to_token_amount_option(ui_amount_in, &token_a)?
        };
        let mut min_amount_out = if reverse {
            self.to_token_amount_option(min_ui_amount_out, &token_a)?
        } else {
            self.to_token_amount_option(min_ui_amount_out, &token_b)?
        };
        // fill in accounts and instruction data
        let data = if reverse {
            AmmInstruction::Swap {
                token_a_amount_in: 0,
                token_b_amount_in: max_amount_in,
                min_token_amount_out: min_amount_out,
            }
        } else {
            AmmInstruction::Swap {
                token_a_amount_in: max_amount_in,
                token_b_amount_in: 0,
                min_token_amount_out: min_amount_out,
            }
        }
        .to_vec()?;

        let accounts = match pool.route {
            PoolRoute::Raydium { .. } => {
                self.get_swap_accounts_raydium(wallet_address, &pool.name)?
            }
            PoolRoute::Orca { .. } => self.get_swap_accounts_orca(wallet_address, &pool.name)?,
        };

        Ok(Instruction {
            program_id: pool.router_program_id,
            data,
            accounts,
        })
    }

    /// Creates a new Instruction for wrapping the token into protocol specific token
    pub fn new_instruction_wrap_token(
        &self,
        wallet_address: &Pubkey,
        pool_name: &str,
        token_to_wrap: TokenSelector,
        ui_amount: f64,
    ) -> Result<Instruction, FarmClientError> {
        // get pool info
        let pool = self.get_pool(pool_name)?;

        // get underlying token info
        let token = if token_to_wrap == TokenSelector::TokenA {
            self.get_token_by_ref_from_cache(&pool.token_a_ref)?
        } else {
            self.get_token_by_ref_from_cache(&pool.token_b_ref)?
        };

        // fill in instruction data
        let data = AmmInstruction::WrapToken {
            amount: self.to_token_amount_option(ui_amount, &token)?,
        }
        .to_vec()?;

        panic!("WrapToken instruction is not supported for this route type");
    }

    /// Creates a new Instruction for unwrapping original token from protocol specific token
    pub fn new_instruction_unwrap_token(
        &self,
        wallet_address: &Pubkey,
        pool_name: &str,
        token_to_unwrap: TokenSelector,
        ui_amount: f64,
    ) -> Result<Instruction, FarmClientError> {
        // get pool info
        let pool = self.get_pool(pool_name)?;
        panic!("UnwrapToken instruction is not supported for this route type");
    }

    /// Creates a new complete set of Instructions for adding liquidity to the Pool
    pub fn all_instructions_add_liquidity_pool(
        &self,
        wallet_address: &Pubkey,
        pool_name: &str,
        max_token_a_ui_amount: f64,
        max_token_b_ui_amount: f64,
    ) -> Result<Vec<Instruction>, FarmClientError> {
        if max_token_a_ui_amount < 0.0
            || max_token_b_ui_amount < 0.0
            || (max_token_a_ui_amount == 0.0 && max_token_b_ui_amount == 0.0)
        {
            return Err(FarmClientError::ValueError(format!(
                "Invalid add liquidity amounts {} and {} specified for Pool {}: Must be greater or equal to zero and at least one non-zero.",
                max_token_a_ui_amount, max_token_b_ui_amount, pool_name
            )));
        }
        // if one of the tokens is SOL and amount is zero, we need to estimate that
        // amount to get it transfered to WSOL
        let is_saber_pool = pool_name.starts_with("SBR.");
        let (is_token_a_sol, is_token_b_sol) = self.pool_has_sol_tokens(pool_name)?;
        let token_a_ui_amount = if max_token_a_ui_amount == 0.0 && is_token_a_sol && !is_saber_pool
        {
            let pool_price = self.get_pool_price(pool_name)?;
            if pool_price > 0.0 {
                max_token_b_ui_amount * 1.03 / pool_price
            } else {
                0.0
            }
        } else {
            max_token_a_ui_amount
        };
        let token_b_ui_amount = if max_token_b_ui_amount == 0.0 && is_token_b_sol && !is_saber_pool
        {
            max_token_a_ui_amount * self.get_pool_price(pool_name)? * 1.03
        } else {
            max_token_b_ui_amount
        };

        let mut inst = Vec::<Instruction>::new();
        let _ = self.check_pool_accounts(
            wallet_address,
            pool_name,
            token_a_ui_amount,
            token_b_ui_amount,
            0.0,
            true,
            &mut inst,
        )?;

        // check if tokens need to be wrapped to a Saber decimal token

        // create and send instruction
        inst.push(self.new_instruction_add_liquidity_pool(
            wallet_address,
            pool_name,
            max_token_a_ui_amount,
            max_token_b_ui_amount,
        )?);
        if is_token_a_sol || is_token_b_sol {
            inst.push(self.new_instruction_close_token_account(wallet_address, "SOL")?);
        }

        Ok(inst)
    }

    /// Creates a new complete set of Instructions for removing liquidity from the Pool
    pub fn all_instructions_remove_liquidity_pool(
        &self,
        wallet_address: &Pubkey,
        pool_name: &str,
        ui_amount: f64,
    ) -> Result<Vec<Instruction>, FarmClientError> {
        let mut inst = Vec::<Instruction>::new();
        let _ = self.check_pool_accounts(
            wallet_address,
            pool_name,
            0.0,
            0.0,
            ui_amount,
            true,
            &mut inst,
        )?;

        inst.push(self.new_instruction_remove_liquidity_pool(
            wallet_address,
            pool_name,
            ui_amount,
        )?);

        // check if tokens need to be unwrapped
        let (is_token_a_sol, is_token_b_sol) = self.pool_has_sol_tokens(pool_name)?;
        let (is_token_a_wrapped, is_token_b_wrapped) =
            self.pool_has_saber_wrapped_tokens(pool_name)?;

        if is_token_a_wrapped {
            inst.push(self.new_instruction_unwrap_token(
                wallet_address,
                pool_name,
                TokenSelector::TokenA,
                0.0,
            )?);
        }
        if is_token_b_wrapped {
            inst.push(self.new_instruction_unwrap_token(
                wallet_address,
                pool_name,
                TokenSelector::TokenB,
                0.0,
            )?);
        }
        if is_token_a_sol || is_token_b_sol {
            inst.push(self.new_instruction_close_token_account(wallet_address, "SOL")?);
        }

        Ok(inst)
    }

    /// Creates a new complete set of Instructions for swapping tokens
    pub fn all_instructions_swap(
        &self,
        wallet_address: &Pubkey,
        protocol: Protocol,
        from_token: &str,
        to_token: &str,
        ui_amount_in: f64,
        min_ui_amount_out: f64,
    ) -> Result<Vec<Instruction>, FarmClientError> {
        // find pool to swap in
        let pool = self.find_pools(protocol, from_token, to_token)?[0];

        // check amount
        if ui_amount_in < 0.0 {
            return Err(FarmClientError::ValueError(format!(
                "Invalid token amount {} specified for pool {}: Must be zero or greater.",
                ui_amount_in,
                pool.name.as_str()
            )));
        }

        // if amount is zero use entire balance
        let ui_amount_in = if ui_amount_in == 0.0 {
            if from_token == "SOL" {
                return Err(FarmClientError::ValueError(format!(
                    "Invalid SOL amount {} specified for pool {}: Must be greater than zero.",
                    ui_amount_in,
                    pool.name.as_str()
                )));
            }
            let balance = self.get_token_account_balance(wallet_address, from_token)?;
            if balance == 0.0 {
                return Err(FarmClientError::InsufficientBalance(from_token.to_string()));
            }
            balance
        } else {
            ui_amount_in
        };

        // check token accounts
        let mut inst = Vec::<Instruction>::new();
        let reverse = FarmClient::pool_has_reverse_tokens(&pool.name, from_token)?;
        if reverse {
            let _ = self.check_pool_accounts(
                wallet_address,
                &pool.name,
                0.0,
                ui_amount_in,
                0.0,
                false,
                &mut inst,
            )?;
        } else {
            let _ = self.check_pool_accounts(
                wallet_address,
                &pool.name,
                ui_amount_in,
                0.0,
                0.0,
                false,
                &mut inst,
            )?;
        }

        // check if tokens must be wrapped to Saber decimal token
        let (is_token_a_wrapped, is_token_b_wrapped) =
            self.pool_has_saber_wrapped_tokens(&pool.name)?;
        if is_token_a_wrapped && !reverse {
            inst.push(self.new_instruction_wrap_token(
                wallet_address,
                &pool.name,
                TokenSelector::TokenA,
                ui_amount_in,
            )?);
        }
        if is_token_b_wrapped && reverse {
            inst.push(self.new_instruction_wrap_token(
                wallet_address,
                &pool.name,
                TokenSelector::TokenB,
                ui_amount_in,
            )?);
        }

        // create and send instruction
        inst.push(self.new_instruction_swap(
            wallet_address,
            protocol,
            from_token,
            to_token,
            ui_amount_in,
            min_ui_amount_out,
        )?);
        if is_token_b_wrapped && !reverse {
            inst.push(self.new_instruction_unwrap_token(
                wallet_address,
                &pool.name,
                TokenSelector::TokenB,
                0.0,
            )?);
        }
        if is_token_a_wrapped && reverse {
            inst.push(self.new_instruction_unwrap_token(
                wallet_address,
                &pool.name,
                TokenSelector::TokenA,
                0.0,
            )?);
        }
        if to_token == "SOL" {
            inst.push(self.new_instruction_close_token_account(wallet_address, "SOL")?);
        }

        Ok(inst)
    }
}
