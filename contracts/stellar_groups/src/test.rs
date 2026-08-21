#![cfg(test)]

use crate::{GroupsContract, GroupsContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

fn create_token<'a>(
    env: &Env,
    admin: &Address,
) -> (
    Address,
    soroban_sdk::token::StellarAssetClient<'a>,
    soroban_sdk::token::Client<'a>,
) {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let address = sac.address();
    let asset_client = soroban_sdk::token::StellarAssetClient::new(env, &address);
    let token_client = soroban_sdk::token::Client::new(env, &address);
    (address, asset_client, token_client)
}

fn setup() -> (
    Env,
    GroupsContractClient<'static>,
    Address, // token
    soroban_sdk::token::StellarAssetClient<'static>,
    soroban_sdk::token::Client<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(GroupsContract, ());
    let client = GroupsContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let token_admin = Address::generate(&env);
    let (token_address, asset_client, token_client) = create_token(&env, &token_admin);

    (env, client, token_address, asset_client, token_client)
}

#[test]
fn full_rosca_cycle_with_referral() {
    let (env, client, token, asset_client, token_client) = setup();

    let alice = Address::generate(&env); // creator
    let bob = Address::generate(&env); // referred by alice
    let carol = Address::generate(&env); // no referrer

    let contribution: i128 = 1000;
    for a in [&alice, &bob, &carol] {
        asset_client.mint(a, &10_000);
    }

    let round_period: u64 = 86_400; // 1 day
    let group_id = client.create_group(
        &alice,
        &token,
        &contribution,
        &round_period,
        &3u32,
        &500u32, // 5% referral cut
    );

    client.join_group(&group_id, &bob, &Some(alice.clone()));
    client.join_group(&group_id, &carol, &None);

    // Round 0: everyone contributes.
    client.contribute(&group_id, &alice);
    client.contribute(&group_id, &bob);
    client.contribute(&group_id, &carol);

    // Referral cut: 5% of 1000 = 50, credited to alice for bob's contribution.
    assert_eq!(client.referral_balance(&alice, &token), 50);

    // Round not ready yet.
    let too_early = client.try_advance_round(&group_id);
    assert!(too_early.is_err());

    env.ledger().with_mut(|l| l.timestamp += round_period);

    let pool_per_member: i128 = contribution - 50; // 950
    let expected_payout = pool_per_member * 3;

    let alice_balance_before = token_client.balance(&alice);
    let recipient = client.advance_round(&group_id);
    assert_eq!(recipient, alice); // round 0 -> first joiner

    let alice_balance_after = token_client.balance(&alice);
    assert_eq!(alice_balance_after - alice_balance_before, expected_payout);

    // Claim the referral reward separately from the payout pool.
    let claimed = client.claim_referral(&alice, &token);
    assert_eq!(claimed, 50);
    assert_eq!(client.referral_balance(&alice, &token), 0);

    let group = client.get_group(&group_id);
    assert_eq!(group.current_round, 1);
    assert!(group.active);
}

#[test]
fn cannot_double_contribute_in_same_round() {
    let (env, client, token, asset_client, _token_client) = setup();
    let alice = Address::generate(&env);
    asset_client.mint(&alice, &10_000);

    let group_id = client.create_group(&alice, &token, &1000i128, &86_400u64, &2u32, &0u32);
    let bob = Address::generate(&env);
    asset_client.mint(&bob, &10_000);
    client.join_group(&group_id, &bob, &None);

    client.contribute(&group_id, &alice);
    let result = client.try_contribute(&group_id, &alice);
    assert!(result.is_err());
}

#[test]
fn group_deactivates_after_full_cycle() {
    let (env, client, token, asset_client, _token_client) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    asset_client.mint(&alice, &10_000);
    asset_client.mint(&bob, &10_000);

    let round_period: u64 = 1000;
    let group_id = client.create_group(&alice, &token, &1000i128, &round_period, &2u32, &0u32);
    client.join_group(&group_id, &bob, &None);

    for _ in 0..2 {
        client.contribute(&group_id, &alice);
        client.contribute(&group_id, &bob);
        env.ledger().with_mut(|l| l.timestamp += round_period);
        client.advance_round(&group_id);
    }

    let group = client.get_group(&group_id);
    assert!(!group.active);
    assert_eq!(group.current_round, 2);
}
