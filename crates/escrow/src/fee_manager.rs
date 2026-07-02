// Feature 2: Fee Distribution System
// Track and distribute accumulated fees to fee collector

use soroban_sdk::{Address, Env, Symbol, U256};

pub struct FeeManager {
    pub fee_collector: Address,
    pub fee_bps: u32,
    pub total_accumulated: u64,
}

impl FeeManager {
    pub fn new(env: &Env, fee_collector: Address, fee_bps: u32) -> Result<Self, Symbol> {
        if fee_bps > 1000 {
            return Err(Symbol::short("invalid_fee"));
        }

        Ok(FeeManager {
            fee_collector,
            fee_bps,
            total_accumulated: 0,
        })
    }

    pub fn calculate_fee(&self, amount: u64) -> u64 {
        ((amount as u128) * (self.fee_bps as u128) / 10000) as u64
    }

    pub fn accrue_fee(&mut self, amount: u64) -> u64 {
        let fee = self.calculate_fee(amount);
        self.total_accumulated = self.total_accumulated.saturating_add(fee);
        fee
    }

    pub fn get_accumulated_fees(&self) -> u64 {
        self.total_accumulated
    }

    pub fn reset_accumulated(&mut self) {
        self.total_accumulated = 0;
    }

    pub fn update_fee_bps(&mut self, new_fee_bps: u32) -> Result<(), Symbol> {
        if new_fee_bps > 1000 {
            return Err(Symbol::short("invalid_fee"));
        }
        self.fee_bps = new_fee_bps;
        Ok(())
    }

    pub fn update_fee_collector(&mut self, new_collector: Address) {
        self.fee_collector = new_collector;
    }
}

pub fn withdraw_fees(
    env: &Env,
    amount: u64,
) -> Result<(), Symbol> {
    let fee_collector: Address = env.storage()
        .instance()
        .get(&"fee_collector")
        .ok_or(Symbol::short("no_collector"))?;

    fee_collector.require_auth();

    let current_fees: u64 = env.storage()
        .instance()
        .get(&"accumulated_fees")
        .unwrap_or(0);

    if amount > current_fees {
        return Err(Symbol::short("insufficient_fees"));
    }

    env.storage()
        .instance()
        .set(&"accumulated_fees", &(current_fees - amount));

    env.events().publish(("fee", "withdrawn"), (amount, fee_collector));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_calculation() {
        let manager = FeeManager {
            fee_collector: Address::from_contract_id(&[0u8; 32]),
            fee_bps: 50,
            total_accumulated: 0,
        };

        let fee = manager.calculate_fee(1000);
        assert_eq!(fee, 5);
    }

    #[test]
    fn test_fee_accrual() {
        let mut manager = FeeManager {
            fee_collector: Address::from_contract_id(&[0u8; 32]),
            fee_bps: 100,
            total_accumulated: 0,
        };

        manager.accrue_fee(1000);
        assert_eq!(manager.get_accumulated_fees(), 10);

        manager.accrue_fee(1000);
        assert_eq!(manager.get_accumulated_fees(), 20);
    }
}
