pub fn calculate_fee(amount: u128, fee_bps: u32) -> u128 {
    (amount * fee_bps as u128) / 10000
}

pub fn validate_fee_bps(fee_bps: u32) -> bool {
    fee_bps <= 1000
}
