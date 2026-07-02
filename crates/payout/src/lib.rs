#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum DataKey {
    Admin,
    FeeCollector,
    FeeBps,
    IsPaused,
    FeeBalance,             // asset -> accumulated fees
    PayoutStatus,           // match_id -> bool (settled or not)
    WinnerWithdrawalStatus, // (match_id, winner) -> bool (withdrawn or not)
}

#[contract]
pub struct PayoutContract;

#[contractimpl]
impl PayoutContract {
    /// Initialize the payout contract.
    pub fn initialize(
        env: Env,
        admin: Address,
        fee_collector: Address,
        fee_bps: u32,
    ) -> Result<(), Symbol> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Symbol::short("already_init"));
        }

        admin.require_auth();

        if fee_bps > 1000 {
            return Err(Symbol::short("invalid_fee"));
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &fee_collector);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage()
            .instance()
            .set(&DataKey::IsPaused, &false);

        Ok(())
    }

    /// Settle a match with winner-take-all payout (deduct fees from pot).
    /// Called by resolver after result is finalized.
    pub fn resolve_winner_take_all(
        env: Env,
        match_id: Symbol,
        winner: Address,
        total_pot: u128,
        asset: Address,
    ) -> Result<(), Symbol> {
        // Check if paused
        if let Some(is_paused) = env.storage().instance().get::<_, bool>(&DataKey::IsPaused) {
            if is_paused {
                return Err(Symbol::short("paused"));
            }
        }

        // Check if already settled
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&(match_id.clone(), Symbol::short("settled")))
            .unwrap_or(false)
        {
            return Err(Symbol::short("already_settled"));
        }

        // Get fee rate
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::FeeBps)
            .unwrap_or(0);

        // Calculate fee
        let fee: u128 = (total_pot as u128 * fee_bps as u128) / 10000;
        let winner_amount = total_pot - fee;

        // Accrue fee
        let fee_balance_key = (Symbol::short("fee_balance"), asset.clone());
        let current_fee_balance: u128 = env
            .storage()
            .persistent()
            .get(&fee_balance_key)
            .unwrap_or(0);

        env.storage()
            .persistent()
            .set(&fee_balance_key, &(current_fee_balance + fee));

        // Store payout amount for winner to withdraw
        let winnings_key = (match_id.clone(), winner.clone());
        env.storage()
            .persistent()
            .set(&winnings_key, &winner_amount);

        // Mark as settled
        env.storage()
            .persistent()
            .set(&(match_id, Symbol::short("settled")), &true);

        Ok(())
    }

    /// Settle a match with a custom split (e.g., 70/30 split).
    pub fn resolve_split(
        env: Env,
        match_id: Symbol,
        winner: Address,
        loser: Address,
        total_pot: u128,
        winner_split_bps: u32, // e.g., 7000 for 70%
        asset: Address,
    ) -> Result<(), Symbol> {
        // Check if paused
        if let Some(is_paused) = env.storage().instance().get::<_, bool>(&DataKey::IsPaused) {
            if is_paused {
                return Err(Symbol::short("paused"));
            }
        }

        // Check if already settled
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&(match_id.clone(), Symbol::short("settled")))
            .unwrap_or(false)
        {
            return Err(Symbol::short("already_settled"));
        }

        // Validate split
        if winner_split_bps > 10000 {
            return Err(Symbol::short("invalid_split"));
        }

        // Get fee rate
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::FeeBps)
            .unwrap_or(0);

        // Calculate fee from total pot
        let fee: u128 = (total_pot as u128 * fee_bps as u128) / 10000;
        let net_pot = total_pot - fee;

        // Split the net pot
        let winner_amount = (net_pot * winner_split_bps as u128) / 10000;
        let loser_amount = net_pot - winner_amount;

        // Accrue fee
        let fee_balance_key = (Symbol::short("fee_balance"), asset.clone());
        let current_fee_balance: u128 = env
            .storage()
            .persistent()
            .get(&fee_balance_key)
            .unwrap_or(0);

        env.storage()
            .persistent()
            .set(&fee_balance_key, &(current_fee_balance + fee));

        // Store payout amounts
        let winner_key = (match_id.clone(), winner.clone());
        let loser_key = (match_id.clone(), loser.clone());
        env.storage()
            .persistent()
            .set(&winner_key, &winner_amount);
        env.storage()
            .persistent()
            .set(&loser_key, &loser_amount);

        // Mark as settled
        env.storage()
            .persistent()
            .set(&(match_id, Symbol::short("settled")), &true);

        Ok(())
    }

    /// Withdraw winnings for a player.
    pub fn withdraw_winnings(
        env: Env,
        match_id: Symbol,
        player: Address,
        asset: Address,
    ) -> Result<u128, Symbol> {
        player.require_auth();

        // Check winnings exist
        let winnings_key = (match_id.clone(), player.clone());
        let amount: u128 = env
            .storage()
            .persistent()
            .get(&winnings_key)
            .ok_or(Symbol::short("no_winnings"))?;

        // Check not already withdrawn
        let withdrawn_key = (match_id.clone(), player.clone(), Symbol::short("withdrawn"));
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&withdrawn_key)
            .unwrap_or(false)
        {
            return Err(Symbol::short("already_withdrawn"));
        }

        // Transfer to player
        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(
            &env.current_contract_address(),
            &player,
            &(amount as i128),
        );

        // Mark as withdrawn
        env.storage()
            .persistent()
            .set(&withdrawn_key, &true);

        Ok(amount)
    }

    /// Withdraw accumulated fees (fee collector only).
    pub fn withdraw_fees(
        env: Env,
        asset: Address,
    ) -> Result<u128, Symbol> {
        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(Symbol::short("not_init"))?;

        fee_collector.require_auth();

        // Get fee balance
        let fee_balance_key = (Symbol::short("fee_balance"), asset.clone());
        let fee_balance: u128 = env
            .storage()
            .persistent()
            .get(&fee_balance_key)
            .unwrap_or(0);

        if fee_balance == 0 {
            return Err(Symbol::short("no_fees"));
        }

        // Transfer to fee collector
        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(
            &env.current_contract_address(),
            &fee_collector,
            &(fee_balance as i128),
        );

        // Reset fee balance
        env.storage()
            .persistent()
            .set(&fee_balance_key, &0u128);

        Ok(fee_balance)
    }

    /// Get fee balance for an asset.
    pub fn get_fee_balance(env: Env, asset: Address) -> Result<u128, Symbol> {
        let fee_balance_key = (Symbol::short("fee_balance"), asset);
        Ok(env
            .storage()
            .persistent()
            .get(&fee_balance_key)
            .unwrap_or(0))
    }

    /// Set fee rate (admin only).
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
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_initialize() {
        let env = Env::default();
        let contract_id = env.register_contract(None, PayoutContract);
        let client = PayoutContractClient::new(&env, &contract_id);

        let admin = Address::random(&env);
        let fee_collector = Address::random(&env);

        let result = client.initialize(&admin, &fee_collector, &50);
        assert!(result.is_ok());
    }

    #[test]
    fn test_fee_calculation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, PayoutContract);
        let client = PayoutContractClient::new(&env, &contract_id);

        let admin = Address::random(&env);
        let fee_collector = Address::random(&env);

        client.initialize(&admin, &fee_collector, &50).unwrap();

        // 200 XLM total, 50 bps fee = 1 XLM fee
        let total_pot = 200_000_000u128; // 200 XLM with 7 decimals
        let fee_bps = 50u32;
        let expected_fee = (total_pot * fee_bps as u128) / 10000;

        assert_eq!(expected_fee, 1_000_000u128); // 1 XLM
    }
}
