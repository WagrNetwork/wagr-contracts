use soroban_sdk::{contracttype, Address, Symbol};

#[derive(Clone, Debug)]
#[contracttype]
pub enum ResolverEvent {
    ResultSubmitted { match_id: Symbol, winner: Address, reason: Symbol },
    DisputeFiled { match_id: Symbol, challenger: Address },
    ResultFinalized { match_id: Symbol, status: Symbol },
}
