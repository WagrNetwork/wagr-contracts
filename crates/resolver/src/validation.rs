use soroban_sdk::Address;

pub fn validate_winner_loser(winner: &Address, loser: &Address) -> bool {
    winner.to_string() != loser.to_string()
}

pub fn validate_reason(reason: &str) -> bool {
    !reason.is_empty() && reason.len() <= 100
}
