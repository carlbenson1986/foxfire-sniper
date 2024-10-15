use crate::config::constants::{
    REDIS_LP_MINT_KEYS, REDIS_POOLS_DETAILS, REDIS_POOLS_KEYS, REDIS_SWAP_CACHE_EXPIRES_S,
    REDIS_SWAP_CACHE_PREFIX, REDIS_USERS,
};
use crate::types::events::ExecutionReceipt;
use crate::types::pool::RaydiumPool;
use anyhow::Result;
use r2d2_redis::redis::{Commands, RedisResult};
use serde::Serialize;
use solana_farm_sdk::refdb::Reference::U8;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

pub type RedisPool = r2d2_redis::r2d2::Pool<r2d2_redis::RedisConnectionManager>;

pub fn connect(redis_uri: &str) -> RedisPool {
    let manager = r2d2_redis::RedisConnectionManager::new(redis_uri).unwrap();
    r2d2_redis::r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create redis connection pool.")
}

//pools to monitor prices from
pub fn cache_new_pool(redis: &RedisPool, pair: &RaydiumPool) -> Result<()> {
    let mut conn = redis.get()?;
    let _: RedisResult<()> = conn.lpush(REDIS_POOLS_KEYS, pair.id.to_string());
    let _: RedisResult<()> = conn.hset(
        REDIS_POOLS_DETAILS,
        pair.id.to_string(),
        serde_json::to_string(&pair)?,
    );
    Ok(())
}

//same but without details
pub fn _cache_pool_id(redis: &RedisPool, pair: &Pubkey) -> Result<()> {
    let mut conn = redis.get()?;
    let _: RedisResult<()> = conn.lpush(REDIS_POOLS_KEYS, pair.to_string());
    Ok(())
}

pub fn _get_cached_pool_ids(redis: &RedisPool) -> Result<Vec<Pubkey>> {
    let mut conn = redis.get()?;
    let trading_pairs: Vec<String> = conn.lrange(REDIS_POOLS_KEYS, 0, -1)?;
    Ok(trading_pairs
        .iter()
        .map(|pair| Pubkey::from_str(pair).unwrap())
        .collect())
}

pub fn _remove_pool_from_cache(redis: &RedisPool, pool_id: &Pubkey) -> Result<()> {
    let mut conn = redis.get()?;
    let _: RedisResult<()> = conn.lrem(REDIS_POOLS_KEYS, 0, &pool_id.to_string());
    let _: RedisResult<()> = conn.hdel(REDIS_POOLS_DETAILS, &pool_id.to_string());
    Ok(())
}

// used for burn liquidity event listener
pub fn _cache_new_lp_mint_address(redis: &RedisPool, mint_address: &Pubkey) -> Result<()> {
    let mut conn = redis.get()?;
    let _: RedisResult<()> = conn.lpush(REDIS_LP_MINT_KEYS, mint_address.to_string());
    Ok(())
}

pub fn _get_cached_lp_mint_addresses(redis: &RedisPool) -> Result<Vec<Pubkey>> {
    let mut conn = redis.get()?;
    let trading_pairs: Vec<String> = conn.lrange(REDIS_LP_MINT_KEYS, 0, -1)?;
    Ok(trading_pairs
        .iter()
        .map(|pair| Pubkey::from_str(pair).unwrap())
        .collect())
}

pub fn _remove_lp_mint_address_from_cache(redis: &RedisPool, mint_address: &Pubkey) -> Result<()> {
    let mut conn = redis.get()?;
    let _: RedisResult<()> = conn.lrem(REDIS_LP_MINT_KEYS, 0, &mint_address.to_string());
    Ok(())
}

// should be used for swap confirmation
// this is not used currently, context swap_signatures is used instead for now - just much faster
// leaving it here for future reference if moved to microservices
pub fn _cachenew_swap_execution(
    redis: &RedisPool,
    uuid: &Uuid,
    signature: &Signature,
) -> Result<()> {
    let mut conn = redis.get()?;

    let key_uuid = format!("{}{}", REDIS_SWAP_CACHE_PREFIX, uuid);
    let signature_str = signature.to_string();

    let key_str = format!("{}{}", REDIS_SWAP_CACHE_PREFIX, signature_str);
    let uuid_str = uuid.to_string();
    conn.set_ex(key_uuid, signature_str, REDIS_SWAP_CACHE_EXPIRES_S as usize)?;
    conn.set_ex(key_str, uuid_str, REDIS_SWAP_CACHE_EXPIRES_S as usize)?;

    Ok(())
}

pub fn _get_swap_execution_signature(redis: &RedisPool, uuid: &Uuid) -> Result<Option<Signature>> {
    let mut conn = redis.get()?;

    let key = format!("{}{}", REDIS_SWAP_CACHE_PREFIX, uuid);
    let signature_opt_str: Option<String> = conn.get(key)?;

    Ok(signature_opt_str
        .map(|s| Signature::from_str(&s))
        .transpose()?)
}

pub fn _get_swap_execution_uuid(redis: &RedisPool, signature: &Signature) -> Result<Option<Uuid>> {
    let mut conn = redis.get()?;

    let key = format!("{}{}", REDIS_SWAP_CACHE_PREFIX, signature);
    let uuid_opt_str: Option<String> = conn.get(key)?;
    Ok(uuid_opt_str.map(|s| Uuid::from_str(&s)).transpose()?)
}

pub fn _delete_swap_execution(redis: &RedisPool, uuid: &Uuid, signature: &Signature) -> Result<()> {
    let mut conn = redis.get()?;
    let mut key = format!("{}{}", REDIS_SWAP_CACHE_PREFIX, uuid);
    conn.del(key)?;
    key = format!("{}{}", REDIS_SWAP_CACHE_PREFIX, signature);
    conn.del(key)?;

    Ok(())
}

// Add a new user to the Redis list
pub fn add_user(redis: &RedisPool, user_id: &str) -> Result<()> {
    let mut conn = redis.get()?;
    let _: RedisResult<()> = conn.lpush(REDIS_USERS, user_id);
    Ok(())
}

pub fn sync_users(redis: &RedisPool, user_ids: Vec<String>) -> Result<()> {
    let mut conn = redis.get()?;
    let _: RedisResult<()> = conn.del(REDIS_USERS);
    for user_id in user_ids {
        let _: RedisResult<()> = conn.lpush(REDIS_USERS, user_id);
    }
    Ok(())
}

// Get the list of all users from Redis
pub fn get_users(redis: &RedisPool) -> Result<Vec<String>> {
    let mut conn = redis.get()?;
    let users: Vec<String> = conn.lrange(REDIS_USERS, 0, -1)?;
    Ok(users)
}

// Check if a specific user exists in the Redis list
pub fn user_exists(redis: &RedisPool, user_id: &str) -> Result<bool> {
    let mut conn = redis.get()?;
    let users: Vec<String> = conn.lrange(REDIS_USERS, 0, -1)?;
    Ok(users.contains(&user_id.to_string()))
}

pub fn get_users_count(redis: &RedisPool) -> Result<usize> {
    let mut conn = redis.get()?;
    let count: usize = conn.llen(REDIS_USERS)?;
    Ok(count)
}

// Monitor the Redis list for changes (new user added)
pub fn monitor_users(redis: &RedisPool, previous_count: usize) -> Result<(usize, Option<String>)> {
    let mut conn = redis.get()?;
    let current_count = conn.llen(REDIS_USERS)?;

    if current_count > previous_count {
        let new_user: Option<String> = conn.lindex(REDIS_USERS, 0)?;
        Ok((current_count, new_user))
    } else {
        Ok((current_count, None))
    }
}
