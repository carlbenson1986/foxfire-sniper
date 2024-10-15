use solana_sdk::bs58;
use solana_sdk::signature::{Keypair, Signer};

pub fn clone_keypair(keypair: &Keypair) -> Keypair {
    let bytes = keypair.to_bytes();
    Keypair::from_bytes(&bytes).unwrap()
}

pub fn private_key_string_base16(keypair: &Keypair) -> String {
    keypair
        .to_bytes()
        .to_vec()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

pub fn private_key_string_base58(keypair: &Keypair) -> String {
    bs58::encode(keypair.to_bytes()).into_string()
}

pub fn public_key_string(keypair: &Keypair) -> String {
    keypair.pubkey().to_string()
}

pub fn from_base16_str_private_key(private_key: &str) -> Keypair {
    let bytes: Vec<u8> = (0..private_key.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&private_key[i..i + 2], 16).unwrap())
        .collect();
    Keypair::from_bytes(&bytes).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone_keypair() {
        let keypair = Keypair::new();
        let cloned_keypair = clone_keypair(&keypair);
        assert_eq!(keypair.to_bytes(), cloned_keypair.to_bytes());
    }

    #[test]
    fn test_private_key_string() {
        let keypair = Keypair::new();
        let private_key = private_key_string_base58(&keypair);
        assert_eq!(private_key.len(), 64);
    }

    #[test]
    fn test_public_key_string() {
        let keypair = Keypair::new();
        let public_key = public_key_string(&keypair);
        assert_eq!(public_key.len(), 44);
    }

    #[test]
    fn test_from_str_private_key() {
        let keypair = Keypair::new();
        let private_key = private_key_string_base16(&keypair);
        let new_keypair = from_base16_str_private_key(&private_key);
        assert_eq!(keypair.to_bytes(), new_keypair.to_bytes());
    }

    #[test]
    fn test_to_from_str_private_key() {
        let keypair = Keypair::new();
        let private_key = private_key_string_base16(&keypair);
        let new_keypair = from_base16_str_private_key(&private_key);
        assert_eq!(private_key, private_key);
    }
}
