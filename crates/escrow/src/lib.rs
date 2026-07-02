#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec, Map, U256};

pub const LEDGER_TIMEOUT_SECS: u64 = 86400; // 24 hours

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[contracttype]
pub enum DataKey {
    Admin,
    FeeCollector,
    FeeBps,
    IsPaused,
    StakeBalance,      // (match_id, player_address) -> amount
    StakeTimestamp,    // (match_id, player_address) -> timestamp
    CounterpartyMatch, // (match_id, player_address) -> counterparty_address
    TotalEscrow,       // Total escrowed across all matches
}

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Initialize the contract with admin, fee collector, and fee rate (in basis points).
    /// Can only be called once.
    pub fn initialize(
        env: Env,
        admin: Address,
        fee_collector: Address,
        fee_bps: u32,
    ) -> Result<(), Symbol> {
        // Check if already initialized
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Symbol::short("already_init"));
        }

        admin.require_auth();

        // Validate fee_bps <= 1000 (10%)
        if fee_bps > 1000 {
            return Err(Symbol::short("invalid_fee"));
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::FeeCollector, &fee_collector);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage()
            .instance()
            .set(&DataKey::IsPaused, &false);

        Ok(())
    }

    /// Stake funds for a player in a match against a counterparty.
    /// Both players must call this with the same match_id and counterparty address.
    pub fn stake(
        env: Env,
        match_id: Symbol,
        player: Address,
        counterparty: Address,
        amount: u128,
        asset: Address, // XLM or other token
    ) -> Result<(), Symbol> {
        player.require_auth();

        // Check if paused
        if let Some(is_paused) = env.storage().instance().get::<_, bool>(&DataKey::IsPaused) {
            if is_paused {
                return Err(Symbol::short("paused"));
            }
        }

        // Validate amount > 0
        if amount == 0 {
            return Err(Symbol::short("zero_amount"));
        }

        // Prevent self-staking
        if player == counterparty {
            return Err(Symbol::short("self_stake"));
        }

        // Transfer asset from player to this contract
        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(
            &player,
            &env.current_contract_address(),
            &(amount as i128),
        );

        // Store stake
        let balance_key = (match_id.clone(), player.clone());
        env.storage()
            .persistent()
            .set(&balance_key, &amount);

        let timestamp_key = (match_id.clone(), player.clone());
        env.storage()
            .persistent()
            .set(&timestamp_key, &env.ledger().timestamp());

        let counterparty_key = (match_id.clone(), player.clone());
        env.storage()
            .persistent()
            .set(&counterparty_key, &counterparty);

        Ok(())
    }

    /// Query the staked balance for a player in a match.
    pub fn query_stake_balance(
        env: Env,
        match_id: Symbol,
        player: Address,
    ) -> Result<u128, Symbol> {
        let balance_key = (match_id, player);
        Ok(env
            .storage()
            .persistent()
            .get::<_, u128>(&balance_key)
            .unwrap_or(0))
    }

    /// Refund a player's stake if the match times out (24h+ after stake).
    /// Called by the player or admin.
    pub fn timeout_refund(
        env: Env,
        match_id: Symbol,
        player: Address,
        asset: Address,
    ) -> Result<(), Symbol> {
        // Either player or admin can call
        let caller = env.invocation_auth().get(0).map(|a| a.address());
        let admin: Option<Address> = env.storage().instance().get(&DataKey::Admin);

        let is_player = Some(&player) == caller.as_ref();
        let is_admin = admin.as_ref() == caller.as_ref();

        if !is_player && !is_admin {
            return Err(Symbol::short("unauthorized"));
        }

        // Check stake exists and has timed out
        let stake_key = (match_id.clone(), player.clone());
        let amount: u128 = env
            .storage()
            .persistent()
            .get(&stake_key)
            .ok_or(Symbol::short("no_stake"))?;

        let timestamp_key = (match_id.clone(), player.clone());
        let staked_at: u64 = env
            .storage()
            .persistent()
            .get(&timestamp_key)
            .ok_or(Symbol::short("no_timestamp"))?;

        let current_time = env.ledger().timestamp();
        if current_time < staked_at + LEDGER_TIMEOUT_SECS {
            return Err(Symbol::short("not_timeout"));
        }

        // Refund the stake
        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(
            &env.current_contract_address(),
            &player,
            &(amount as i128),
        );

        // Clear stake records
        env.storage().persistent().remove(&stake_key);
        env.storage().persistent().remove(&timestamp_key);
        env.storage()
            .persistent()
            .remove(&(match_id, player));

        Ok(())
    }

    /// Set admin (admin only).
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Symbol> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Symbol::short("not_init"))?;

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// Set fee collector (admin only).
    pub fn set_fee_collector(env: Env, new_collector: Address) -> Result<(), Symbol> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Symbol::short("not_init"))?;

        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &new_collector);
        Ok(())
    }

    /// Set fee rate in basis points (admin only, max 1000).
    pub fn set_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), Symbol> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Symbol::short("not_init"))?;

        admin.require_auth();

        if new_fee_bps > 1000 {
            return Err(Symbol::short("invalid_fee"));
        }

        env.storage().instance().set(&DataKey::FeeBps, &new_fee_bps);
        Ok(())
    }

    /// Pause the contract (admin only).
    pub fn pause(env: Env) -> Result<(), Symbol> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Symbol::short("not_init"))?;

        admin.require_auth();
        env.storage().instance().set(&DataKey::IsPaused, &true);
        Ok(())
    }

    /// Unpause the contract (admin only).
    pub fn unpause(env: Env) -> Result<(), Symbol> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Symbol::short("not_init"))?;

        admin.require_auth();
        env.storage().instance().set(&DataKey::IsPaused, &false);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{Address, Env};

    #[test]
    fn test_initialize() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::random(&env);
        let fee_collector = Address::random(&env);

        let result = client.initialize(&admin, &fee_collector, &50);
        assert!(result.is_ok());
    }

    #[test]
    fn test_initialize_invalid_fee() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::random(&env);
        let fee_collector = Address::random(&env);

        let result = client.initialize(&admin, &fee_collector, &1001); // > 1000 bps
        assert!(result.is_err());
    }
}
