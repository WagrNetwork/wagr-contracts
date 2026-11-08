#![no_std]
// Events still use `env.events().publish`; migrating to `#[contractevent]` types is tracked as an open issue.
#![allow(deprecated)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Symbol, Vec,
};

pub const LEDGER_TIMEOUT_SECS: u64 = 86400; // 24 hours
pub const MIN_PLAYERS: u32 = 2;
pub const MAX_PLAYERS: u32 = 8;

const BUMP_THRESHOLD: u32 = 518_400; // ~30 days at 5s/ledger
const BUMP_EXTEND_TO: u32 = 535_680; // ~31 days at 5s/ledger

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[contracterror]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidFee = 3,
    Paused = 4,
    ZeroAmount = 5,
    AlreadyStaked = 6,
    TooManyPlayers = 7,
    NotEnoughPlayers = 8,
    NotTimedOut = 9,
    NoStake = 10,
    Unauthorized = 11,
    AssetMismatch = 12,
    AlreadyReleased = 13,
    SettlerNotSet = 14,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[contracttype]
pub enum DataKey {
    Admin,
    FeeCollector,
    FeeBps,
    IsPaused,
    TotalEscrow,
    /// Contract (normally the payout contract) allowed to release a match pot.
    Settler,
}

#[contract]
pub struct EscrowContract;

fn players_key(env: &Env, match_id: &Symbol) -> (Symbol, Symbol) {
    (Symbol::new(env, "players"), match_id.clone())
}

fn stake_key(env: &Env, match_id: &Symbol, player: &Address) -> (Symbol, Symbol, Address) {
    (Symbol::new(env, "stake"), match_id.clone(), player.clone())
}

fn stake_time_key(env: &Env, match_id: &Symbol, player: &Address) -> (Symbol, Symbol, Address) {
    (
        Symbol::new(env, "staketime"),
        match_id.clone(),
        player.clone(),
    )
}

fn asset_key(env: &Env, match_id: &Symbol) -> (Symbol, Symbol) {
    (Symbol::new(env, "asset"), match_id.clone())
}

fn released_key(env: &Env, match_id: &Symbol) -> (Symbol, Symbol) {
    (Symbol::new(env, "released"), match_id.clone())
}

fn is_released(env: &Env, match_id: &Symbol) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&released_key(env, match_id))
        .unwrap_or(false)
}

/// Every match is denominated in exactly one asset, fixed by the first stake.
/// Rejects any call that names a different asset, so a match can never be
/// paid out or refunded in a token other than the one that was deposited.
fn check_asset(env: &Env, match_id: &Symbol, asset: &Address) -> Result<(), Error> {
    let stored: Option<Address> = env.storage().persistent().get(&asset_key(env, match_id));
    match stored {
        Some(a) if a != *asset => Err(Error::AssetMismatch),
        _ => Ok(()),
    }
}

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)?;
    admin.require_auth();
    Ok(admin)
}

fn require_not_paused(env: &Env) -> Result<(), Error> {
    let is_paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::IsPaused)
        .unwrap_or(false);
    if is_paused {
        return Err(Error::Paused);
    }
    Ok(())
}

#[contractimpl]
impl EscrowContract {
    /// Initialize the contract with admin, fee collector, and fee rate (in basis points).
    /// Can only be called once.
    pub fn initialize(
        env: Env,
        admin: Address,
        fee_collector: Address,
        fee_bps: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        admin.require_auth();

        if fee_bps > 1000 {
            return Err(Error::InvalidFee);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &fee_collector);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage().instance().set(&DataKey::IsPaused, &false);
        env.storage().instance().set(&DataKey::TotalEscrow, &0u128);

        Ok(())
    }

    /// Stake funds for a player in a match. Supports 2-8 players per match
    /// (multi-way matches, not just 1v1). Each player calls this once with
    /// their own stake; the match's participant list grows dynamically.
    pub fn stake(
        env: Env,
        match_id: Symbol,
        player: Address,
        amount: u128,
        asset: Address,
    ) -> Result<(), Error> {
        player.require_auth();
        require_not_paused(&env)?;

        if amount == 0 {
            return Err(Error::ZeroAmount);
        }
        if is_released(&env, &match_id) {
            return Err(Error::AlreadyReleased);
        }
        check_asset(&env, &match_id, &asset)?;

        let pkey = players_key(&env, &match_id);
        let mut players: Vec<Address> = env
            .storage()
            .persistent()
            .get(&pkey)
            .unwrap_or(Vec::new(&env));

        if players.iter().any(|p| p == player) {
            return Err(Error::AlreadyStaked);
        }
        if players.len() >= MAX_PLAYERS {
            return Err(Error::TooManyPlayers);
        }

        // Transfer asset from player to this contract.
        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(&player, env.current_contract_address(), &(amount as i128));

        let akey = asset_key(&env, &match_id);
        if !env.storage().persistent().has(&akey) {
            env.storage().persistent().set(&akey, &asset);
        }
        env.storage()
            .persistent()
            .extend_ttl(&akey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        players.push_back(player.clone());
        env.storage().persistent().set(&pkey, &players);
        env.storage()
            .persistent()
            .extend_ttl(&pkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        let skey = stake_key(&env, &match_id, &player);
        env.storage().persistent().set(&skey, &amount);
        env.storage()
            .persistent()
            .extend_ttl(&skey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        let tkey = stake_time_key(&env, &match_id, &player);
        env.storage()
            .persistent()
            .set(&tkey, &env.ledger().timestamp());
        env.storage()
            .persistent()
            .extend_ttl(&tkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        let total: u128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalEscrow)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalEscrow, &(total + amount));

        env.events()
            .publish(("stake", "deposited", match_id), (player, amount));

        Ok(())
    }

    /// Query the staked balance for a player in a match.
    pub fn query_stake_balance(env: Env, match_id: Symbol, player: Address) -> Result<u128, Error> {
        let skey = stake_key(&env, &match_id, &player);
        Ok(env
            .storage()
            .persistent()
            .get::<_, u128>(&skey)
            .unwrap_or(0))
    }

    /// List the players who have staked into a match so far.
    pub fn get_match_players(env: Env, match_id: Symbol) -> Result<Vec<Address>, Error> {
        let pkey = players_key(&env, &match_id);
        Ok(env
            .storage()
            .persistent()
            .get(&pkey)
            .unwrap_or(Vec::new(&env)))
    }

    /// Refund a player's stake if the match times out (24h+ after stake).
    /// Caller must be the player themselves or the admin.
    pub fn timeout_refund(
        env: Env,
        match_id: Symbol,
        player: Address,
        caller: Address,
        asset: Address,
    ) -> Result<(), Error> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;

        if caller != player && caller != admin {
            return Err(Error::Unauthorized);
        }
        if is_released(&env, &match_id) {
            return Err(Error::AlreadyReleased);
        }
        // Refund only in the asset that was actually staked.
        check_asset(&env, &match_id, &asset)?;

        let skey = stake_key(&env, &match_id, &player);
        let amount: u128 = env
            .storage()
            .persistent()
            .get(&skey)
            .ok_or(Error::NoStake)?;

        let tkey = stake_time_key(&env, &match_id, &player);
        let staked_at: u64 = env
            .storage()
            .persistent()
            .get(&tkey)
            .ok_or(Error::NoStake)?;

        let current_time = env.ledger().timestamp();
        if current_time < staked_at + LEDGER_TIMEOUT_SECS {
            return Err(Error::NotTimedOut);
        }

        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(&env.current_contract_address(), &player, &(amount as i128));

        env.storage().persistent().remove(&skey);
        env.storage().persistent().remove(&tkey);

        let pkey = players_key(&env, &match_id);
        let mut players: Vec<Address> = env
            .storage()
            .persistent()
            .get(&pkey)
            .unwrap_or(Vec::new(&env));
        if let Some(idx) = players.iter().position(|p| p == player) {
            players.remove(idx as u32);
            env.storage().persistent().set(&pkey, &players);
        }

        let total: u128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalEscrow)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalEscrow, &total.saturating_sub(amount));

        env.events()
            .publish(("stake", "refunded", match_id), (player, amount));

        Ok(())
    }

    /// Release the whole pot of a match to `recipient` (normally the payout
    /// contract itself). Only the configured settler may call this, the match
    /// needs at least `MIN_PLAYERS` stakes, and it can happen once. Returns
    /// the asset and the pot size so the caller settles exactly what was held.
    pub fn release(
        env: Env,
        match_id: Symbol,
        recipient: Address,
    ) -> Result<(Address, u128), Error> {
        let settler: Address = env
            .storage()
            .instance()
            .get(&DataKey::Settler)
            .ok_or(Error::SettlerNotSet)?;
        settler.require_auth();
        require_not_paused(&env)?;

        if is_released(&env, &match_id) {
            return Err(Error::AlreadyReleased);
        }

        let players: Vec<Address> = env
            .storage()
            .persistent()
            .get(&players_key(&env, &match_id))
            .unwrap_or(Vec::new(&env));
        if players.len() < MIN_PLAYERS {
            return Err(Error::NotEnoughPlayers);
        }

        let asset: Address = env
            .storage()
            .persistent()
            .get(&asset_key(&env, &match_id))
            .ok_or(Error::NoStake)?;

        let mut pot: u128 = 0;
        for p in players.iter() {
            let skey = stake_key(&env, &match_id, &p);
            pot += env
                .storage()
                .persistent()
                .get::<_, u128>(&skey)
                .unwrap_or(0);
            env.storage().persistent().remove(&skey);
            env.storage()
                .persistent()
                .remove(&stake_time_key(&env, &match_id, &p));
        }

        let rkey = released_key(&env, &match_id);
        env.storage().persistent().set(&rkey, &true);
        env.storage()
            .persistent()
            .extend_ttl(&rkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(&env.current_contract_address(), &recipient, &(pot as i128));

        let total: u128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalEscrow)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalEscrow, &total.saturating_sub(pot));

        env.events()
            .publish(("stake", "released", match_id), (recipient, pot));

        Ok((asset, pot))
    }

    /// The asset a match is denominated in (set by its first stake).
    pub fn get_match_asset(env: Env, match_id: Symbol) -> Option<Address> {
        env.storage().persistent().get(&asset_key(&env, &match_id))
    }

    /// Set the settler allowed to call `release` (admin only).
    pub fn set_settler(env: Env, settler: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Settler, &settler);
        Ok(())
    }

    /// Set admin (admin only).
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// Set fee collector (admin only).
    pub fn set_fee_collector(env: Env, new_collector: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &new_collector);
        Ok(())
    }

    /// Set fee rate in basis points (admin only, max 1000).
    pub fn set_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), Error> {
        require_admin(&env)?;
        if new_fee_bps > 1000 {
            return Err(Error::InvalidFee);
        }
        env.storage().instance().set(&DataKey::FeeBps, &new_fee_bps);
        Ok(())
    }

    /// Total amount currently held in escrow across all matches.
    pub fn get_total_escrow(env: Env) -> u128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalEscrow)
            .unwrap_or(0)
    }

    /// Pause the contract (admin only). Blocks new stakes.
    pub fn pause(env: Env) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::IsPaused, &true);
        env.events().publish(("contract", "paused"), admin);
        Ok(())
    }

    /// Unpause the contract (admin only).
    pub fn unpause(env: Env) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::IsPaused, &false);
        env.events().publish(("contract", "unpaused"), admin);
        Ok(())
    }

    /// Check whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::IsPaused)
            .unwrap_or(false)
    }

    /// Upgrade the contract's WASM code (admin only).
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        require_admin(&env)?;
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::Env;

    fn create_token<'a>(
        env: &Env,
        admin: &Address,
    ) -> (Address, soroban_sdk::token::StellarAssetClient<'a>) {
        let contract_address = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let asset_client = soroban_sdk::token::StellarAssetClient::new(env, &contract_address);
        (contract_address, asset_client)
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);

        client.initialize(&admin, &fee_collector, &50);
    }

    #[test]
    fn test_initialize_invalid_fee() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);

        let result = client.try_initialize(&admin, &fee_collector, &1001);
        assert!(result.is_err());
    }

    #[test]
    fn test_stake_and_query() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        client.initialize(&admin, &fee_collector, &50);

        let token_admin = Address::generate(&env);
        let (asset, token_admin_client) = create_token(&env, &token_admin);
        let player = Address::generate(&env);
        token_admin_client.mint(&player, &1000);

        let match_id = Symbol::new(&env, "match1");
        client.stake(&match_id, &player, &100, &asset);

        assert_eq!(client.query_stake_balance(&match_id, &player), 100);
        assert_eq!(client.get_total_escrow(), 100);
    }

    #[test]
    fn test_timeout_refund() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        client.initialize(&admin, &fee_collector, &50);

        let token_admin = Address::generate(&env);
        let (asset, token_admin_client) = create_token(&env, &token_admin);
        let player = Address::generate(&env);
        token_admin_client.mint(&player, &1000);

        let match_id = Symbol::new(&env, "match1");
        client.stake(&match_id, &player, &100, &asset);

        env.ledger()
            .with_mut(|l| l.timestamp += LEDGER_TIMEOUT_SECS + 1);

        client.timeout_refund(&match_id, &player, &player, &asset);
        assert_eq!(client.query_stake_balance(&match_id, &player), 0);
    }

    #[test]
    fn test_refund_rejects_other_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &Address::generate(&env), &50);

        // A real asset someone else staked, and a worthless one the attacker controls.
        let (real, real_admin) = create_token(&env, &Address::generate(&env));
        let (junk, junk_admin) = create_token(&env, &Address::generate(&env));
        let victim = Address::generate(&env);
        let attacker = Address::generate(&env);
        real_admin.mint(&victim, &1000);
        junk_admin.mint(&attacker, &1000);

        client.stake(&Symbol::new(&env, "m1"), &victim, &500, &real);
        client.stake(&Symbol::new(&env, "m2"), &attacker, &500, &junk);
        env.ledger()
            .with_mut(|l| l.timestamp += LEDGER_TIMEOUT_SECS + 1);

        let res = client.try_timeout_refund(&Symbol::new(&env, "m2"), &attacker, &attacker, &real);
        assert_eq!(res, Err(Ok(Error::AssetMismatch)));
    }

    #[test]
    fn test_stake_rejects_mixed_assets() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);
        client.initialize(&Address::generate(&env), &Address::generate(&env), &50);

        let (a, a_admin) = create_token(&env, &Address::generate(&env));
        let (b, b_admin) = create_token(&env, &Address::generate(&env));
        let p1 = Address::generate(&env);
        let p2 = Address::generate(&env);
        a_admin.mint(&p1, &1000);
        b_admin.mint(&p2, &1000);

        let m = Symbol::new(&env, "m1");
        client.stake(&m, &p1, &100, &a);
        assert_eq!(
            client.try_stake(&m, &p2, &100, &b),
            Err(Ok(Error::AssetMismatch))
        );
    }

    #[test]
    fn test_release_moves_full_pot_once() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);
        client.initialize(&Address::generate(&env), &Address::generate(&env), &50);
        let settler = Address::generate(&env);
        client.set_settler(&settler);

        let (asset, asset_admin) = create_token(&env, &Address::generate(&env));
        let token = soroban_sdk::token::Client::new(&env, &asset);
        let p1 = Address::generate(&env);
        let p2 = Address::generate(&env);
        asset_admin.mint(&p1, &1000);
        asset_admin.mint(&p2, &1000);

        let m = Symbol::new(&env, "m1");
        client.stake(&m, &p1, &300, &asset);
        assert_eq!(
            client.try_release(&m, &settler),
            Err(Ok(Error::NotEnoughPlayers))
        );
        client.stake(&m, &p2, &300, &asset);

        let (a, pot) = client.release(&m, &settler);
        assert_eq!((a, pot), (asset.clone(), 600));
        assert_eq!(token.balance(&settler), 600);
        assert_eq!(client.get_total_escrow(), 0);
        assert_eq!(
            client.try_release(&m, &settler),
            Err(Ok(Error::AlreadyReleased))
        );

        // Stakes are gone, so no one can also claim a timeout refund.
        env.ledger()
            .with_mut(|l| l.timestamp += LEDGER_TIMEOUT_SECS + 1);
        assert!(client.try_timeout_refund(&m, &p1, &p1, &asset).is_err());
    }

    #[test]
    fn test_pause_blocks_stake() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        client.initialize(&admin, &fee_collector, &50);
        client.pause();

        let token_admin = Address::generate(&env);
        let (asset, token_admin_client) = create_token(&env, &token_admin);
        let player = Address::generate(&env);
        token_admin_client.mint(&player, &1000);

        let match_id = Symbol::new(&env, "match1");
        let result = client.try_stake(&match_id, &player, &100, &asset);
        assert!(result.is_err());
    }
}
