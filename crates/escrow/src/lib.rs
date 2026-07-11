#![no_std]

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[contracttype]
pub enum DataKey {
    Admin,
    FeeCollector,
    FeeBps,
    IsPaused,
    TotalEscrow,
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
    (Symbol::new(env, "staketime"), match_id.clone(), player.clone())
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
        token.transfer(&player, &env.current_contract_address(), &(amount as i128));

        players.push_back(player.clone());
        env.storage().persistent().set(&pkey, &players);
        env.storage().persistent().extend_ttl(&pkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

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
        Ok(env.storage().persistent().get::<_, u128>(&skey).unwrap_or(0))
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
        let contract_address = env.register_stellar_asset_contract(admin.clone());
        let asset_client = soroban_sdk::token::StellarAssetClient::new(env, &contract_address);
        (contract_address, asset_client)
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);

        client.initialize(&admin, &fee_collector, &50);
    }

    #[test]
    fn test_initialize_invalid_fee() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EscrowContract);
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
        let contract_id = env.register_contract(None, EscrowContract);
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
        let contract_id = env.register_contract(None, EscrowContract);
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

        env.ledger().with_mut(|l| l.timestamp += LEDGER_TIMEOUT_SECS + 1);

        client.timeout_refund(&match_id, &player, &player, &asset);
        assert_eq!(client.query_stake_balance(&match_id, &player), 0);
    }

    #[test]
    fn test_pause_blocks_stake() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EscrowContract);
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
