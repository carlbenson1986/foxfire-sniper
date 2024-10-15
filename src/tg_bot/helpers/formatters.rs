use solana_sdk::signature::Signature;

pub fn create_solscan_link(tx_hash: &Signature) -> String {
    let withdrawal_link = format!("https://solscan.io/tx/{}", tx_hash.to_string());
    format!("\nTx hash [{}]({})", tx_hash.to_string(), withdrawal_link)
}

pub fn format_curr(amount: f64) -> String {
    format!("{:.3} {}", amount, "SOL")
}
