// Feature 1: Match Timeout Refund Logic
// Auto-refund staked amounts if dispute window expires without finalization

use soroban_sdk::{Address, Env, Symbol};

pub fn check_and_refund_timeout(
    env: &Env,
    match_id: &str,
    dispute_window_secs: u64,
) -> Result<bool, Symbol> {
    let current_time = env.ledger().timestamp();
    let created_time: u64 = env.storage()
        .persistent()
        .get(&format!("match_created:{}", match_id))
        .unwrap_or(0);

    if created_time == 0 {
        return Err(Symbol::short("match_not_found"));
    }

    let elapsed = current_time.saturating_sub(created_time);

    if elapsed > dispute_window_secs {
        // Timeout reached - refund both players
        return Ok(true);
    }

    Ok(false)
}

pub fn calculate_refund_amount(
    total_staked: u64,
    fee_bps: u32,
) -> (u64, u64) {
    // Calculate per-player refund after deducting protocol fee
    let fee_amount = (total_staked as u128 * fee_bps as u128 / 10000) as u64;
    let refund_per_player = (total_staked - fee_amount) / 2;
    (refund_per_player, fee_amount)
}

pub fn execute_timeout_refund(
    env: &Env,
    match_id: &str,
    player1: &Address,
    player2: &Address,
    refund_amount: u64,
) -> Result<(), Symbol> {
    // Mark match as refunded
    env.storage()
        .persistent()
        .set(&format!("match_refunded:{}", match_id), &true);

    // Emit timeout refund event
    env.events().publish(
        ("timeout", "refund_executed"),
        (match_id, player1, player2, refund_amount),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_calculation() {
        let total_staked = 1000u64;
        let fee_bps = 50u32;
        let (refund, fee) = calculate_refund_amount(total_staked, fee_bps);
        
        assert_eq!(fee, 5);
        assert_eq!(refund, 497);
    }

    #[test]
    fn test_zero_fee_refund() {
        let total_staked = 1000u64;
        let fee_bps = 0u32;
        let (refund, fee) = calculate_refund_amount(total_staked, fee_bps);
        
        assert_eq!(fee, 0);
        assert_eq!(refund, 500);
    }
}
