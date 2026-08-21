use crate::error::Error;
use soroban_sdk::{contracttype, Address, Env, Vec};

/// How long (in ledgers) persistent group/referrer entries are kept alive
/// before they'd expire, and the threshold at which we top the TTL back up.
/// ~30 days assuming ~5s ledgers; tune for your network before mainnet use.
const TTL_THRESHOLD: u32 = 120_960;
const TTL_EXTEND_TO: u32 = 518_400;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    GroupCount,
    Group(u64),
    /// (group_id, member) -> referrer address
    Referrer(u64, Address),
    /// (referrer, token) -> claimable balance, see referral.rs
    ReferralBalance(Address, Address),
}

#[contracttype]
#[derive(Clone)]
pub struct Group {
    pub id: u64,
    pub creator: Address,
    pub token: Address,
    /// Amount each member contributes per round, in the token's smallest unit.
    pub contribution_amount: i128,
    /// Minimum seconds that must elapse between rounds.
    pub round_period: u64,
    pub max_members: u32,
    /// Join order also defines payout rotation order.
    pub members: Vec<Address>,
    pub current_round: u32,
    pub round_start_ts: u64,
    /// Members who have already paid into the current round.
    pub contributions_this_round: Vec<Address>,
    /// Basis points (of contribution_amount) routed to a referrer, if any.
    pub referral_bps: u32,
    /// False once every member has received one payout (full cycle complete).
    pub active: bool,
}

pub fn load(env: &Env, group_id: u64) -> Result<Group, Error> {
    let group = env
        .storage()
        .persistent()
        .get(&DataKey::Group(group_id))
        .ok_or(Error::GroupNotFound)?;
    bump_group_ttl(env, group_id);
    Ok(group)
}

pub fn save(env: &Env, group: &Group) {
    env.storage()
        .persistent()
        .set(&DataKey::Group(group.id), group);
    bump_group_ttl(env, group.id);
}

pub fn bump_group_ttl(env: &Env, group_id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::Group(group_id),
        TTL_THRESHOLD,
        TTL_EXTEND_TO,
    );
}

pub fn bump_referrer_ttl(env: &Env, group_id: u64, member: &Address) {
    let key = DataKey::Referrer(group_id, member.clone());
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}
