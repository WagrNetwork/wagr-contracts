pub fn validate_split_bps(split_bps: u32) -> bool {
    split_bps <= 10000
}

pub fn validate_fee_bps(fee_bps: u32) -> bool {
    fee_bps <= 1000
}

pub fn validate_payout_amounts(winner_amount: u128, loser_amount: u128) -> bool {
    winner_amount > 0 && loser_amount >= 0
}
