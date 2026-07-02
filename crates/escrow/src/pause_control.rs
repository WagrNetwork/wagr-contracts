// Feature 10: Emergency Pause/Unpause
// Admin pause matching functionality for emergency situations

use soroban_sdk::{Address, Env, Symbol};

pub struct PauseState {
    pub is_paused: bool,
    pub paused_at: u64,
    pub paused_by: Address,
    pub reason: String,
}

pub fn pause_contract(
    env: &Env,
    caller: &Address,
    reason: &str,
) -> Result<(), Symbol> {
    // Check admin access
    let admin: Address = env.storage()
        .instance()
        .get(&"admin")
        .ok_or(Symbol::short("no_admin"))?;

    if admin != *caller {
        return Err(Symbol::short("unauthorized"));
    }

    let pause_state = PauseState {
        is_paused: true,
        paused_at: env.ledger().timestamp(),
        paused_by: caller.clone(),
        reason: reason.to_string(),
    };

    env.storage()
        .instance()
        .set(&"pause_state", &pause_state);

    env.events().publish(
        ("contract", "paused"),
        (caller, reason),
    );

    Ok(())
}

pub fn unpause_contract(
    env: &Env,
    caller: &Address,
) -> Result<(), Symbol> {
    // Check admin access
    let admin: Address = env.storage()
        .instance()
        .get(&"admin")
        .ok_or(Symbol::short("no_admin"))?;

    if admin != *caller {
        return Err(Symbol::short("unauthorized"));
    }

    env.storage()
        .instance()
        .set(&"is_paused", &false);

    env.events().publish(
        ("contract", "unpaused"),
        caller,
    );

    Ok(())
}

pub fn check_paused(env: &Env) -> Result<(), Symbol> {
    let is_paused: bool = env.storage()
        .instance()
        .get(&"is_paused")
        .unwrap_or(false);

    if is_paused {
        return Err(Symbol::short("paused"));
    }

    Ok(())
}

pub fn get_pause_state(env: &Env) -> Result<PauseState, Symbol> {
    env.storage()
        .instance()
        .get(&"pause_state")
        .ok_or(Symbol::short("no_pause_info"))
}

pub fn get_pause_duration(env: &Env) -> Result<u64, Symbol> {
    let pause_state = get_pause_state(env)?;
    let current_time = env.ledger().timestamp();
    
    Ok(current_time.saturating_sub(pause_state.paused_at))
}

pub fn can_execute_staking(env: &Env) -> Result<bool, Symbol> {
    check_paused(env)?;
    Ok(true)
}

pub fn can_execute_result_submission(env: &Env) -> Result<bool, Symbol> {
    check_paused(env)?;
    Ok(true)
}

pub fn can_execute_payout(env: &Env) -> Result<bool, Symbol> {
    check_paused(env)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pause_state_structure() {
        let state = PauseState {
            is_paused: true,
            paused_at: 1000000,
            paused_by: Address::from_contract_id(&[0u8; 32]),
            reason: "Emergency".to_string(),
        };

        assert!(state.is_paused);
        assert_eq!(state.reason, "Emergency");
    }

    #[test]
    fn test_pause_duration_calculation() {
        let paused_at = 1000000u64;
        let current_time = 1000000u64 + 3600u64; // 1 hour later
        
        let duration = current_time.saturating_sub(paused_at);
        assert_eq!(duration, 3600);
    }
}
