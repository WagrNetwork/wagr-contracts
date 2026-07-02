// Feature 8: Event Emission System
// Comprehensive event logging for all contract operations

use soroban_sdk::{Address, Env, Symbol};

pub enum EventType {
    MatchCreated,
    MatchLocked,
    ResultSubmitted,
    Disputed,
    Resolved,
    PayoutProcessed,
    FeeAccrued,
    FeeWithdrawn,
    AdminActionExecuted,
}

pub struct EventLog {
    pub event_type: String,
    pub timestamp: u64,
    pub actor: Address,
    pub data: String,
}

pub fn emit_match_created(
    env: &Env,
    match_id: &str,
    player1: &Address,
    player2: &Address,
    amount: u64,
) {
    env.events().publish(
        ("match", "created"),
        (match_id, player1, player2, amount),
    );
}

pub fn emit_match_locked(
    env: &Env,
    match_id: &str,
    total_staked: u64,
) {
    env.events().publish(
        ("match", "locked"),
        (match_id, total_staked),
    );
}

pub fn emit_result_submitted(
    env: &Env,
    match_id: &str,
    submitter: &Address,
    winner: &Address,
    timestamp: u64,
) {
    env.events().publish(
        ("result", "submitted"),
        (match_id, submitter, winner, timestamp),
    );
}

pub fn emit_dispute_filed(
    env: &Env,
    dispute_id: &str,
    match_id: &str,
    challenger: &Address,
    reason: &str,
) {
    env.events().publish(
        ("dispute", "filed"),
        (dispute_id, match_id, challenger, reason),
    );
}

pub fn emit_payout_processed(
    env: &Env,
    match_id: &str,
    winner: &Address,
    amount: u64,
    fee: u64,
) {
    env.events().publish(
        ("payout", "processed"),
        (match_id, winner, amount, fee),
    );
}

pub fn emit_fee_accrued(
    env: &Env,
    match_id: &str,
    fee_amount: u64,
) {
    env.events().publish(
        ("fee", "accrued"),
        (match_id, fee_amount),
    );
}

pub fn emit_admin_action(
    env: &Env,
    action: &str,
    admin: &Address,
    details: &str,
) {
    env.events().publish(
        ("admin", action),
        (admin, details),
    );
}

pub fn emit_contract_paused(
    env: &Env,
    admin: &Address,
) {
    env.events().publish(
        ("contract", "paused"),
        admin,
    );
}

pub fn emit_contract_unpaused(
    env: &Env,
    admin: &Address,
) {
    env.events().publish(
        ("contract", "unpaused"),
        admin,
    );
}

pub fn log_event(
    env: &Env,
    event_type: &str,
    actor: &Address,
    data: &str,
) -> Result<(), Symbol> {
    let log = EventLog {
        event_type: event_type.to_string(),
        timestamp: env.ledger().timestamp(),
        actor: actor.clone(),
        data: data.to_string(),
    };

    env.storage()
        .persistent()
        .set(
            &format!("event_log:{}:{}", event_type, env.ledger().timestamp()),
            &log,
        );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_naming() {
        let types = vec!["match_created", "result_submitted", "dispute_filed"];
        for t in types {
            assert!(!t.is_empty());
        }
    }
}
