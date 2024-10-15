use crate::utils::keys::clone_keypair;
use anyhow::{bail, Result};
use solana_sdk::bs58;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::io::Write;
use diesel::pg::Pg;
use diesel::serialize::{IsNull, Output, ToSql};
use diesel::{serialize, sql_types};
use serde::{Serialize, Serializer};
use tracing::error;

pub struct KeypairClonable(Keypair);

impl From<KeypairClonable> for String {
    fn from(kp: KeypairClonable) -> Self {
        kp.pubkey().to_string()
    }
}

impl Default for KeypairClonable {
    fn default() -> Self {
        KeypairClonable(Keypair::new())
    }
}

impl Clone for KeypairClonable {
    fn clone(&self) -> Self {
        KeypairClonable(clone_keypair(&self.0))
    }
}

impl Debug for KeypairClonable {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.pubkey().to_string())
    }
}

impl From<KeypairClonable> for Keypair {
    fn from(keypair_clonable: KeypairClonable) -> Keypair {
        keypair_clonable.0
    }
}


impl Serialize for KeypairClonable {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.pubkey().to_string())
    }
}

impl ToSql<sql_types::Text, Pg> for KeypairClonable {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        out.write_all(self.0.pubkey().to_string().as_bytes())?;
        Ok(IsNull::No)
    }
}


impl KeypairClonable {
    pub fn new() -> Self {
        KeypairClonable(Keypair::new())
    }

    pub fn new_from_privkey(privkey: &str) -> Result<Self> {
        let keypair = keypair_from_base58_string(privkey)?;
        Ok(KeypairClonable(keypair))
        // KeypairClonable(Keypair::from_base58_string(privkey))
    }

    pub fn get_keypair(&self) -> Keypair {
        clone_keypair(&self.0)
    }

    pub fn pubkey(&self) -> Pubkey {
        self.0.pubkey()
    }
}

fn keypair_from_base58_string(privkey: &str) -> Result<Keypair> {
    // Attempt to decode the base58 string
    let secret_key_bytes = match bs58::decode(privkey).into_vec() {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to decode base58 string: {:?}", e);
            bail!("Failed to decode base58 string");
        }
    };

    // Ensure the length is either 32 or 64 bytes
    if secret_key_bytes.len() != 64 && secret_key_bytes.len() != 32 {
        bail!("Invalid length for secret key");
    }

    let keypair = if secret_key_bytes.len() == 64 {
        Keypair::from_bytes(&secret_key_bytes)?
    } else {
        let secret = ed25519_dalek::SecretKey::from_bytes(&secret_key_bytes)?;
        let public = ed25519_dalek::PublicKey::from(&secret);
        Keypair::from_bytes(&[secret.to_bytes(), public.to_bytes()].concat())?
    };

    Ok(keypair)
}
