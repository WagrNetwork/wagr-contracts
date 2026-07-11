#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Symbol};

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
    AlreadySettled = 5,
    InvalidSplit = 6,
    NoWinnings = 7,
    AlreadyWithdrawn = 8,
    NoFees = 9,
    Unauthorized = 10,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[contracttype]
pub enum DataKey {
    Admin,
    FeeCollector,
    Arbiter, // Only the arbiter (the resolver/settlement authority) can settle matches
    FeeBps,
    IsPaused,
}

#[contract]
pub struct PayoutContract;

fn fee_balance_key(env: &Env, asset: &Address) -> (Symbol, Address) {
    (Symbol::new(env, "feebal"), asset.clone())
}

fn settled_key(env: &Env, match_id: &Symbol) -> (Symbol, Symbol) {
    (Symbol::new(env, "settled"), match_id.clone())
}

fn winnings_key(env: &Env, match_id: &Symbol, player: &Address) -> (Symbol, Symbol, Address) {
    (Symbol::new(env, "winnings"), match_id.clone(), player.clone())
}

fn withdrawn_key(env: &Env, match_id: &Symbol, player: &Address) -> (Symbol, Symbol, Address) {
    (Symbol::new(env, "withdrawn"), match_id.clone(), player.clone())
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

fn require_arbiter(env: &Env) -> Result<Address, Error> {
    let arbiter: Address = env
        .storage()
        .instance()
        .get(&DataKey::Arbiter)
        .ok_or(Error::NotInitialized)?;
    arbiter.require_auth();
    Ok(arbiter)
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
impl PayoutContract {
    /// Initialize the payout contract. The admin is also set as the initial
    /// arbiter (the only party authorized to settle matches); change it with
    /// `set_arbiter` once a dedicated resolver address is known.
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
        env.storage().instance().set(&DataKey::Arbiter, &admin);
        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &fee_collector);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage().instance().set(&DataKey::IsPaused, &false);

        Ok(())
    }

    /// Settle a match with winner-take-all payout (deduct fees from pot).
    /// Only callable by the arbiter (the trusted settlement authority, e.g.
    /// the resolver contract's operator) once a result has been finalized.
    pub fn resolve_winner_take_all(
        env: Env,
        match_id: Symbol,
        winner: Address,
        total_pot: u128,
        asset: Address,
    ) -> Result<(), Error> {
        require_arbiter(&env)?;
        require_not_paused(&env)?;

        let skey = settled_key(&env, &match_id);
        if env.storage().persistent().get::<_, bool>(&skey).unwrap_or(false) {
            return Err(Error::AlreadySettled);
        }

        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
        let fee: u128 = (total_pot * fee_bps as u128) / 10000;
        let winner_amount = total_pot - fee;

        let fbkey = fee_balance_key(&env, &asset);
        let current_fee_balance: u128 = env.storage().persistent().get(&fbkey).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&fbkey, &(current_fee_balance + fee));

        let wkey = winnings_key(&env, &match_id, &winner);
        env.storage().persistent().set(&wkey, &winner_amount);
        env.storage()
            .persistent()
            .extend_ttl(&wkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        env.storage().persistent().set(&skey, &true);

        env.events().publish(
            ("payout", "settled", match_id),
            (winner, winner_amount, fee),
        );

        Ok(())
    }

    /// Settle a match with a custom split (e.g., 70/30 split) between winner
    /// and loser. Only callable by the arbiter.
    pub fn resolve_split(
        env: Env,
        match_id: Symbol,
        winner: Address,
        loser: Address,
        total_pot: u128,
        winner_split_bps: u32, // e.g., 7000 for 70%
        asset: Address,
    ) -> Result<(), Error> {
        require_arbiter(&env)?;
        require_not_paused(&env)?;

        let skey = settled_key(&env, &match_id);
        if env.storage().persistent().get::<_, bool>(&skey).unwrap_or(false) {
            return Err(Error::AlreadySettled);
        }

        if winner_split_bps > 10000 {
            return Err(Error::InvalidSplit);
        }

        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
        let fee: u128 = (total_pot * fee_bps as u128) / 10000;
        let net_pot = total_pot - fee;

        let winner_amount = (net_pot * winner_split_bps as u128) / 10000;
        let loser_amount = net_pot - winner_amount;

        let fbkey = fee_balance_key(&env, &asset);
        let current_fee_balance: u128 = env.storage().persistent().get(&fbkey).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&fbkey, &(current_fee_balance + fee));

        let wkey = winnings_key(&env, &match_id, &winner);
        let lkey = winnings_key(&env, &match_id, &loser);
        env.storage().persistent().set(&wkey, &winner_amount);
        env.storage().persistent().set(&lkey, &loser_amount);
        env.storage()
            .persistent()
            .extend_ttl(&wkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);
        env.storage()
            .persistent()
            .extend_ttl(&lkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        env.storage().persistent().set(&skey, &true);

        env.events().publish(
            ("payout", "split_settled", match_id),
            (winner, winner_amount, loser, loser_amount, fee),
        );

        Ok(())
    }

    /// Withdraw winnings for a player.
    pub fn withdraw_winnings(env: Env, match_id: Symbol, player: Address, asset: Address) -> Result<u128, Error> {
        player.require_auth();

        let wkey = winnings_key(&env, &match_id, &player);
        let amount: u128 = env.storage().persistent().get(&wkey).ok_or(Error::NoWinnings)?;

        let dkey = withdrawn_key(&env, &match_id, &player);
        if env.storage().persistent().get::<_, bool>(&dkey).unwrap_or(false) {
            return Err(Error::AlreadyWithdrawn);
        }

        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(&env.current_contract_address(), &player, &(amount as i128));

        env.storage().persistent().set(&dkey, &true);

        env.events()
            .publish(("payout", "withdrawn", match_id), (player, amount));

        Ok(amount)
    }

    /// Withdraw accumulated fees (fee collector only).
    pub fn withdraw_fees(env: Env, asset: Address) -> Result<u128, Error> {
        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(Error::NotInitialized)?;
        fee_collector.require_auth();

        let fbkey = fee_balance_key(&env, &asset);
        let fee_balance: u128 = env.storage().persistent().get(&fbkey).unwrap_or(0);

        if fee_balance == 0 {
            return Err(Error::NoFees);
        }

        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(
            &env.current_contract_address(),
            &fee_collector,
            &(fee_balance as i128),
        );

        env.storage().persistent().set(&fbkey, &0u128);

        env.events()
            .publish(("fee", "withdrawn"), (asset, fee_balance));

        Ok(fee_balance)
    }

    /// Get fee balance for an asset.
    pub fn get_fee_balance(env: Env, asset: Address) -> u128 {
        let fbkey = fee_balance_key(&env, &asset);
        env.storage().persistent().get(&fbkey).unwrap_or(0)
    }

    /// Set fee rate (admin only).
    pub fn set_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), Error> {
        require_admin(&env)?;
        if new_fee_bps > 1000 {
            return Err(Error::InvalidFee);
        }
        env.storage().instance().set(&DataKey::FeeBps, &new_fee_bps);
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

    /// Set the arbiter, the sole authority permitted to settle matches (admin only).
    pub fn set_arbiter(env: Env, new_arbiter: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Arbiter, &new_arbiter);
        Ok(())
    }

    /// Set admin (admin only).
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// Pause the contract (admin only).
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
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PayoutContract);
        let client = PayoutContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);

        client.initialize(&admin, &fee_collector, &50);
    }

    #[test]
    fn test_winner_take_all_requires_arbiter_auth() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PayoutContract);
        let client = PayoutContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        client.initialize(&admin, &fee_collector, &50);

        let asset = Address::generate(&env);
        let winner = Address::generate(&env);
        let total_pot = 200_000_000u128;

        let match_id = Symbol::new(&env, "match1");
        client.resolve_winner_take_all(&match_id, &winner, &total_pot, &asset);

        let fee_bps = 50u32;
        let expected_fee = (total_pot * fee_bps as u128) / 10000;
        assert_eq!(client.get_fee_balance(&asset), expected_fee);
    }

    #[test]
    fn test_double_settle_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PayoutContract);
        let client = PayoutContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        client.initialize(&admin, &fee_collector, &50);

        let asset = Address::generate(&env);
        let winner = Address::generate(&env);
        let match_id = Symbol::new(&env, "match1");

        client.resolve_winner_take_all(&match_id, &winner, &200_000_000u128, &asset);
        let result = client.try_resolve_winner_take_all(&match_id, &winner, &200_000_000u128, &asset);
        assert!(result.is_err());
    }
}
