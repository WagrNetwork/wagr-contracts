use soroban_sdk::{Address, Symbol};

pub fn validate_address(addr: &Address) -> bool {
    !addr.to_string().is_empty()
}

pub fn validate_match_id(match_id: &Symbol) -> bool {
    !match_id.to_string().is_empty()
}

pub fn validate_amount(amount: u128) -> bool {
    amount > 0
}
