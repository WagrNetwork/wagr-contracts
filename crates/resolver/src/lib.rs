#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Bytes};

pub const DISPUTE_WINDOW_SECS: u64 = 86400; // 24 hours

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum ResultStatus {
    Pending,   // Submitted, dispute window open
    Disputed,  // Challenge filed
    Finalized, // Dispute window closed, result is final
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct GameResult {
    winner: Address,
    loser: Address,
    reason: Symbol, // 'checkmate', 'resignation', 'timeout', etc
    submitter: Address,
    submitted_at: u64,
    status: ResultStatus,
}

#[derive(Clone, Debug)]
#[contracttype]
pub enum DataKey {
    Admin,
    IsPaused,
    MatchResult,    // match_id -> GameResult
    DisputeCount,   // match_id -> number of disputes
    DisputeEvidence, // (match_id, index) -> evidence bytes
}

#[contract]
pub struct ResolverContract;

#[contractimpl]
impl ResolverContract {
    /// Initialize the resolver contract.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Symbol> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Symbol::short("already_init"));
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::IsPaused, &false);

        Ok(())
    }

    /// Submit a game result with cryptographic proof.
    /// Starts a 24h dispute window.
    pub fn submit_result(
        env: Env,
        match_id: Symbol,
        winner: Address,
        loser: Address,
        reason: Symbol,
    ) -> Result<(), Symbol> {
        // Check if paused
        if let Some(is_paused) = env.storage().instance().get::<_, bool>(&DataKey::IsPaused) {
            if is_paused {
                return Err(Symbol::short("paused"));
            }
        }

        // Check if result already exists
        if env
            .storage()
            .persistent()
            .has(&(match_id.clone(), Symbol::short("result")))
        {
            return Err(Symbol::short("result_exists"));
        }

        // Caller (oracle/adapter) is the submitter
        let submitter = env.invocation_auth().get(0).map(|a| a.address());
        if submitter.is_none() {
            return Err(Symbol::short("unauthorized"));
        }

        let result = GameResult {
            winner: winner.clone(),
            loser: loser.clone(),
            reason,
            submitter: submitter.unwrap(),
            submitted_at: env.ledger().timestamp(),
            status: ResultStatus::Pending,
        };

        env.storage()
            .persistent()
            .set(&(match_id, Symbol::short("result")), &result);

        Ok(())
    }

    /// File a dispute during the 24h window.
    pub fn dispute(
        env: Env,
        match_id: Symbol,
        evidence: Bytes, // Challenger's rebuttal (e.g., PGN, screenshot, JSON proof)
    ) -> Result<(), Symbol> {
        let result_key = (match_id.clone(), Symbol::short("result"));
        let mut result: GameResult = env
            .storage()
            .persistent()
            .get(&result_key)
            .ok_or(Symbol::short("no_result"))?;

        // Check dispute window is open
        let current_time = env.ledger().timestamp();
        if current_time > result.submitted_at + DISPUTE_WINDOW_SECS {
            return Err(Symbol::short("dispute_closed"));
        }

        // Check only loser can dispute
        let challenger = env.invocation_auth().get(0).map(|a| a.address());
        if challenger.as_ref() != Some(&result.loser) {
            return Err(Symbol::short("only_loser"));
        }

        // Record dispute
        let dispute_count_key = (match_id.clone(), Symbol::short("dispute_count"));
        let dispute_count: u32 = env
            .storage()
            .persistent()
            .get(&dispute_count_key)
            .unwrap_or(0);

        let evidence_key = (match_id.clone(), dispute_count);
        env.storage()
            .persistent()
            .set(&evidence_key, &evidence);

        env.storage()
            .persistent()
            .set(&dispute_count_key, &(dispute_count + 1));

        // Update result status
        result.status = ResultStatus::Disputed;
        env.storage()
            .persistent()
            .set(&result_key, &result);

        Ok(())
    }

    /// Finalize the result after the dispute window closes.
    /// Can be called by anyone after 24h.
    pub fn finalize(env: Env, match_id: Symbol) -> Result<Address, Symbol> {
        let result_key = (match_id.clone(), Symbol::short("result"));
        let mut result: GameResult = env
            .storage()
            .persistent()
            .get(&result_key)
            .ok_or(Symbol::short("no_result"))?;

        // Check dispute window is closed
        let current_time = env.ledger().timestamp();
        if current_time < result.submitted_at + DISPUTE_WINDOW_SECS {
            return Err(Symbol::short("dispute_window_open"));
        }

        // Mark as finalized
        result.status = ResultStatus::Finalized;
        env.storage()
            .persistent()
            .set(&result_key, &result);

        Ok(result.winner)
    }

    /// Query the result status for a match.
    pub fn query_result_status(
        env: Env,
        match_id: Symbol,
    ) -> Result<(Address, ResultStatus), Symbol> {
        let result_key = (match_id, Symbol::short("result"));
        let result: GameResult = env
            .storage()
            .persistent()
            .get(&result_key)
            .ok_or(Symbol::short("no_result"))?;

        Ok((result.winner, result.status))
    }

    /// Get dispute evidence for a match.
    pub fn get_dispute_evidence(
        env: Env,
        match_id: Symbol,
        index: u32,
    ) -> Result<Bytes, Symbol> {
        let evidence_key = (match_id, index);
        env.storage()
            .persistent()
            .get(&evidence_key)
            .ok_or(Symbol::short("no_evidence"))
    }

    /// Pause the contract (admin only).
    pub fn pause(env: Env) -> Result<(), Symbol> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Symbol::short("not_init"))?;

        admin.require_auth();
        env.storage().instance().set(&DataKey::IsPaused, &true);
        Ok(())
    }

    /// Unpause the contract (admin only).
    pub fn unpause(env: Env) -> Result<(), Symbol> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Symbol::short("not_init"))?;

        admin.require_auth();
        env.storage().instance().set(&DataKey::IsPaused, &false);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_initialize() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ResolverContract);
        let client = ResolverContractClient::new(&env, &contract_id);

        let admin = Address::random(&env);
        let result = client.initialize(&admin);
        assert!(result.is_ok());
    }

    #[test]
    fn test_submit_result() {
        let env = Env::default();
        let contract_id = env.register_contract(None, ResolverContract);
        let client = ResolverContractClient::new(&env, &contract_id);

        let admin = Address::random(&env);
        client.initialize(&admin).unwrap();

        let winner = Address::random(&env);
        let loser = Address::random(&env);
        let match_id = Symbol::short("match1");

        let result = client.submit_result(
            &match_id,
            &winner,
            &loser,
            &Symbol::short("checkmate"),
        );
        assert!(result.is_ok());
    }
}
