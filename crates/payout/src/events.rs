use soroban_sdk::{contracttype, Address, Symbol};

#[derive(Clone, Debug)]
#[contracttype]
pub enum PayoutEvent {
    PayoutSettled { match_id: Symbol, winner: Address, amount: u128 },
    FeesAccrued { asset: Address, amount: u128 },
    WinningsWithdrawn { player: Address, amount: u128 },
}
