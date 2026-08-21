use crate::group::DataKey;
use soroban_sdk::{Address, Env};

const TTL_THRESHOLD: u32 = 120_960;
const TTL_EXTEND_TO: u32 = 518_400;

fn key(referrer: &Address, token: &Address) -> DataKey {
    DataKey::ReferralBalance(referrer.clone(), token.clone())
}

/// Adds `amount` to a referrer's claimable balance for a given token.
/// A zero amount is a no-op that still refreshes the entry's TTL if it exists.
pub fn credit(env: &Env, referrer: &Address, token: &Address, amount: i128) {
    let k = key(referrer, token);
    if amount == 0 {
        if env.storage().persistent().has(&k) {
            env.storage()
                .persistent()
                .extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
        return;
    }
    let bal: i128 = env.storage().persistent().get(&k).unwrap_or(0);
    env.storage().persistent().set(&k, &(bal + amount));
    env.storage()
        .persistent()
        .extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Returns the current balance without modifying it.
pub fn peek(env: &Env, referrer: &Address, token: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&key(referrer, token))
        .unwrap_or(0)
}

/// Zeroes out and returns the balance (used when claiming).
pub fn take(env: &Env, referrer: &Address, token: &Address) -> i128 {
    let k = key(referrer, token);
    let bal: i128 = env.storage().persistent().get(&k).unwrap_or(0);
    if bal > 0 {
        env.storage().persistent().set(&k, &0i128);
    }
    bal
}
