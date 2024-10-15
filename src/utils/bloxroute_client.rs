use once_cell::sync::Lazy;
use std::env;

static BLOXROUTE_URI: Lazy<usize> = Lazy::new(|| {
    env::var("BLOXROUTE_URI")
        .unwrap_or("1".to_string())
        .parse()
        .unwrap()
});
//
// // src/lib.rs
// pub mod bloxroute_client {
//     use reqwest::Client;
//     use serde_json::json;
//     use solana_sdk::transaction::TransactionMessage;
//     use std::env;
//     use std::error::Error;
//
//     pub async fn send_tx_with_bloxroute(tx: TransactionMessage) -> Result<String, Box<dyn Error>> {
//         // Set up the authorization header
//         let auth_header = env::var("AUTH_HEADER").expect("AUTH_HEADER must be set");
//
//         // Set up the client
//         let client = Client::new();
//
//         // Serialize the transaction message to base64
//         let tx_base64 = base64::encode(tx.serialize());
//
//         // Create the JSON payload
//         let payload = json!({
//             "transaction": {
//                 "content": tx_base64
//             }
//         });
//
//         // Send the request
//         let response = client.post("https://ny.solana.dex.blxrbdn.com/api/v2/submit")
//             .header("Authorization", auth_header)
//             .json(&payload)
//             .send()
//             .await?;
//
//         // Check if the request was successful
//         if response.status().is_success() {
//             // Return the response text
//             Ok(response.text().await?)
//         } else {
//             // Return an error with the response text
//             Err(response.text().await?.into())
//         }
//     }
// }
