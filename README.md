# Stellar Groups Contract

A Soroban (Rust) smart contract for the Stellar network implementing
**rotating-savings groups** (ROSCA / ajo / chama style) with a built-in
**referral/affiliate reward** layer.

Built from scratch, following standard Soroban SDK conventions. It was
*not* copied from `Afreum/Afreum_Loyalty_Tiers_Contract` — that repo's
source files weren't accessible for automated fetching (GitHub blocks
crawling `/tree` and un-indexed raw file paths), only its public
description ("Stellar smart contract for Afreum Loyalty Tier
calculations") and general repo shape (Cargo.toml + `src/`, GitHub
Actions CI) were visible. If you can paste the actual `lib.rs` in, I can
align this contract's naming/patterns/error style to match it exactly.

## How it works

- **Groups**: a creator opens a group with a token, a fixed contribution
  amount, a round period (seconds), a max member count, and an optional
  referral cut (basis points, capped at 10%).
- **Joining**: members join up to `max_members`, optionally recording who
  referred them.
- **Contributing**: each round, members call `contribute`. Their payment
  splits automatically — most goes into the round's payout pool, a slice
  goes to their referrer's claimable balance (if they have one).
- **Payout rotation**: once `round_period` has elapsed, anyone can call
  `advance_round`. The pool for that round pays out to the next member in
  join order (index `current_round % member_count`), then the round
  resets. After every member has been paid once, the group goes inactive.
- **Referral rewards**: referrers accumulate a balance per token and can
  `claim_referral` at any time, independent of the group's payout cycle.

## Project layout

```
Cargo.toml                          # workspace root
contracts/stellar_groups/
  Cargo.toml
  src/
    lib.rs        # contract entry points
    group.rs       # Group struct, storage keys, TTL management
    referral.rs    # referral balance accounting
    error.rs       # contract error codes
    test.rs        # unit tests (cfg(test))
```

## Build & test

Requires the Stellar CLI and Rust with the `wasm32v1-none` target
(this sandbox has neither installed, so the code below has **not**
been compiled or run yet — review it before deploying):

```bash
rustup target add wasm32v1-none
cargo install --locked stellar-cli

cd contracts/stellar_groups
cargo test                 # runs src/test.rs
stellar contract build     # produces target/wasm32v1-none/release/*.wasm
```

## Deploy (testnet example)

```bash
stellar keys generate admin --network testnet
stellar contract deploy \
  --wasm target/wasm32v1-none/release/stellar_groups_contract.wasm \
  --source admin --network testnet

stellar contract invoke --id <CONTRACT_ID> --source admin --network testnet \
  -- initialize --admin <ADMIN_ADDRESS>
```

## Notes / things to review before mainnet

- `advance_round` is callable by anyone (a keeper pattern) — add access
  control if you want to restrict who can trigger payouts.
- Uncontributed members in a round simply reduce that round's payout
  (no penalty/slashing logic is included).
- Persistent storage TTLs are extended on every touch using placeholder
  thresholds (`group.rs` / `referral.rs`) — tune these for your target
  network's ledger close time before deploying.
- No re-entrancy concerns from cross-contract calls beyond the token
  `transfer` calls, but get an audit before handling real funds.
