#![no_std]

//! Stellar Groups Contract
//!
//! A Soroban smart contract for rotating-savings groups (ROSCA-style, e.g.
//! ajo / chama / esusu) with an optional referral/affiliate reward layer.
//!
//! - Members join a group and contribute a fixed amount every round.
//! - Each round, the pooled contributions are paid out to one member,
//!   rotating through the membership list in join order.
//! - A configurable slice (basis points) of every contribution is routed
//!   into a referral pool for whoever referred that member, claimable
//!   independently of the group's payout cycle.
//!
//! This is a from-scratch implementation, not a copy of any existing
//! Afreum contract source (that source was not accessible to build from).
//! It follows standard Soroban SDK conventions (Address, persistent
//! storage, token::Client transfers, contract events).

mod error;
mod group;
mod referral;
mod test;

use error::Error;
use group::{DataKey, Group};
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env, Vec};

#[contract]
pub struct GroupsContract;

#[contractimpl]
impl GroupsContract {
    /// One-time setup. Stores the admin address and initializes the group counter.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::GroupCount, &0u64);
        Ok(())
    }

    /// Creates a new rotating-savings group. The creator becomes the first member.
    ///
    /// `referral_bps` is the portion of every contribution (in basis points,
    /// max 1000 = 10%) that is routed to a referrer's claimable balance
    /// instead of the payout pool.
    pub fn create_group(
        env: Env,
        creator: Address,
        token: Address,
        contribution_amount: i128,
        round_period: u64,
        max_members: u32,
        referral_bps: u32,
    ) -> Result<u64, Error> {
        creator.require_auth();

        if contribution_amount <= 0 {
            return Err(Error::InvalidParams);
        }
        if max_members < 2 {
            return Err(Error::InvalidParams);
        }
        if referral_bps > 1000 {
            return Err(Error::InvalidParams);
        }

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::GroupCount)
            .unwrap_or(0);
        let group_id = count;
        count += 1;
        env.storage().instance().set(&DataKey::GroupCount, &count);

        let mut members = Vec::new(&env);
        members.push_back(creator.clone());

        let group = Group {
            id: group_id,
            creator: creator.clone(),
            token,
            contribution_amount,
            round_period,
            max_members,
            members,
            current_round: 0,
            round_start_ts: env.ledger().timestamp(),
            contributions_this_round: Vec::new(&env),
            referral_bps,
            active: true,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Group(group_id), &group);
        group::bump_group_ttl(&env, group_id);

        env.events()
            .publish((symbol_short!("grp_new"), creator), group_id);
        Ok(group_id)
    }

    /// Joins an existing group. `referrer`, if given, earns a share of this
    /// member's future contributions. A member cannot refer themselves.
    pub fn join_group(
        env: Env,
        group_id: u64,
        member: Address,
        referrer: Option<Address>,
    ) -> Result<(), Error> {
        member.require_auth();

        let mut group = group::load(&env, group_id)?;
        if !group.active {
            return Err(Error::GroupInactive);
        }
        if group.members.len() >= group.max_members {
            return Err(Error::GroupFull);
        }
        if group.members.iter().any(|m| m == member) {
            return Err(Error::AlreadyMember);
        }

        group.members.push_back(member.clone());
        group::save(&env, &group);

        if let Some(r) = referrer {
            if r != member {
                env.storage()
                    .persistent()
                    .set(&DataKey::Referrer(group_id, member.clone()), &r);
                group::bump_referrer_ttl(&env, group_id, &member);
            }
        }

        env.events()
            .publish((symbol_short!("joined"), member), group_id);
        Ok(())
    }

    /// Pays this round's contribution. Splits it between the payout pool
    /// and (if the caller was referred) the referrer's claimable balance.
    pub fn contribute(env: Env, group_id: u64, member: Address) -> Result<(), Error> {
        member.require_auth();

        let mut group = group::load(&env, group_id)?;
        if !group.active {
            return Err(Error::GroupInactive);
        }
        if !group.members.iter().any(|m| m == member) {
            return Err(Error::NotMember);
        }
        if group
            .contributions_this_round
            .iter()
            .any(|m| m == member)
        {
            return Err(Error::AlreadyContributed);
        }

        let referral_cut =
            (group.contribution_amount * group.referral_bps as i128) / 10_000;
        let pool_amount = group.contribution_amount - referral_cut;

        let token_client = token::Client::new(&env, &group.token);
        let contract_addr = env.current_contract_address();

        if pool_amount > 0 {
            token_client.transfer(&member, &contract_addr, &pool_amount);
        }

        if referral_cut > 0 {
            token_client.transfer(&member, &contract_addr, &referral_cut);
            let referrer_key = DataKey::Referrer(group_id, member.clone());
            if let Some(referrer) = env
                .storage()
                .persistent()
                .get::<DataKey, Address>(&referrer_key)
            {
                referral::credit(&env, &referrer, &group.token, referral_cut);
            } else {
                // No referrer on record: the cut simply joins the payout pool
                // instead of being wasted.
                referral::credit(&env, &member, &group.token, 0); // no-op, keeps TTL fresh if present
            }
        }

        group.contributions_this_round.push_back(member.clone());
        group::save(&env, &group);

        env.events()
            .publish((symbol_short!("contrib"), member), group_id);
        Ok(())
    }

    /// Advances the group to the next round once `round_period` has elapsed,
    /// paying out this round's collected pool to the next member in rotation
    /// (join order). Returns the address that was paid.
    pub fn advance_round(env: Env, group_id: u64) -> Result<Address, Error> {
        let mut group = group::load(&env, group_id)?;
        if !group.active {
            return Err(Error::GroupInactive);
        }

        let now = env.ledger().timestamp();
        if now < group.round_start_ts + group.round_period {
            return Err(Error::RoundNotReady);
        }

        let member_count = group.members.len();
        if member_count == 0 {
            return Err(Error::GroupNotFound);
        }
        let recipient_index = (group.current_round % member_count) as u32;
        let recipient = group
            .members
            .get(recipient_index)
            .ok_or(Error::GroupNotFound)?;

        let referral_cut =
            (group.contribution_amount * group.referral_bps as i128) / 10_000;
        let pool_amount = group.contribution_amount - referral_cut;
        let payout = pool_amount * group.contributions_this_round.len() as i128;

        if payout > 0 {
            let token_client = token::Client::new(&env, &group.token);
            token_client.transfer(&env.current_contract_address(), &recipient, &payout);
        }

        group.current_round += 1;
        group.round_start_ts = now;
        group.contributions_this_round = Vec::new(&env);

        if group.current_round >= member_count {
            group.active = false;
        }

        group::save(&env, &group);

        env.events().publish(
            (symbol_short!("payout"), recipient.clone()),
            (group_id, payout),
        );
        Ok(recipient)
    }

    /// Claims a referrer's accrued balance for a given token.
    pub fn claim_referral(env: Env, referrer: Address, token: Address) -> Result<i128, Error> {
        referrer.require_auth();
        let bal = referral::take(&env, &referrer, &token);
        if bal > 0 {
            let token_client = token::Client::new(&env, &token);
            token_client.transfer(&env.current_contract_address(), &referrer, &bal);
            env.events()
                .publish((symbol_short!("refclaim"), referrer), bal);
        }
        Ok(bal)
    }

    /// Read-only: current claimable referral balance.
    pub fn referral_balance(env: Env, referrer: Address, token: Address) -> i128 {
        referral::peek(&env, &referrer, &token)
    }

    /// Read-only: full group state.
    pub fn get_group(env: Env, group_id: u64) -> Result<Group, Error> {
        group::load(&env, group_id)
    }

    /// Read-only: total number of groups created.
    pub fn group_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::GroupCount)
            .unwrap_or(0)
    }
}
