use solana_farm_sdk::math;

pub fn tokens_to_ui_amount_with_decimals_f64(amount: u64, decimals: u8) -> f64 {
    if amount == 0 {
        return 0.0;
    }
    let divisor = math::checked_pow(10u64, decimals as usize).unwrap();
    amount as f64 / divisor as f64
}

pub fn ui_amount_with_decimals_to_tokens(amount: f64, decimals: u8) -> u64 {
    if amount == 0.0 {
        return 0;
    }
    let multiplier = math::checked_pow(10u64, decimals as usize).unwrap();
    (amount * multiplier as f64) as u64
}

pub fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / 1000000000.0
}

pub fn sol_to_lamports(sol: f64) -> u64 {
    (sol * 1000000000.0) as u64
}
