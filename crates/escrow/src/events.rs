use soroban_sdk::{contracttype, Address, Symbol};

#[derive(Clone, Debug)]
#[contracttype]
pub enum EscrowEvent {
    MatchCreated {
        match_id: Symbol,
        player1: Address,
        player2: Address,
        amount: u128,
    },
    StakeDeposited { player: Address, amount: u128 },
    StakeRefunded { player: Address, amount: u128, reason: Symbol },
}
