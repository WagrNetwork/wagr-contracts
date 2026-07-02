pub fn calculate_winner_take_all(total_pot: u128, fee_bps: u32) -> (u128, u128) {
    let fee = (total_pot * fee_bps as u128) / 10000;
    let winner_amount = total_pot - fee;
    (winner_amount, fee)
}

pub fn calculate_split_payout(
    total_pot: u128,
    fee_bps: u32,
    winner_split_bps: u32,
) -> (u128, u128, u128) {
    let fee = (total_pot * fee_bps as u128) / 10000;
    let net_pot = total_pot - fee;
    let winner_amount = (net_pot * winner_split_bps as u128) / 10000;
    let loser_amount = net_pot - winner_amount;
    (winner_amount, loser_amount, fee)
}
