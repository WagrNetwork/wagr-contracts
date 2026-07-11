#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Bytes, BytesN, Env, Symbol};

pub const DISPUTE_WINDOW_SECS: u64 = 86400; // 24 hours

const BUMP_THRESHOLD: u32 = 518_400; // ~30 days at 5s/ledger
const BUMP_EXTEND_TO: u32 = 535_680; // ~31 days at 5s/ledger

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[contracterror]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Paused = 3,
    ResultExists = 4,
    NoResult = 5,
    DisputeClosed = 6,
    OnlyLoser = 7,
    DisputeWindowOpen = 8,
    NoEvidence = 9,
    Unauthorized = 10,
}

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
    pub winner: Address,
    pub loser: Address,
    pub reason: Symbol, // 'checkmate', 'resignation', 'timeout', etc
    pub submitter: Address,
    pub proof: Bytes, // off-chain proof reference (e.g. signed PGN hash)
    pub submitted_at: u64,
    pub status: ResultStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[contracttype]
pub enum DataKey {
    Admin,
    IsPaused,
}

#[contract]
pub struct ResolverContract;

fn result_key(env: &Env, match_id: &Symbol) -> (Symbol, Symbol) {
    (Symbol::new(env, "result"), match_id.clone())
}

fn dispute_count_key(env: &Env, match_id: &Symbol) -> (Symbol, Symbol) {
    (Symbol::new(env, "dcount"), match_id.clone())
}

fn evidence_key(env: &Env, match_id: &Symbol, index: u32) -> (Symbol, Symbol, u32) {
    (Symbol::new(env, "evidence"), match_id.clone(), index)
}

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)?;
    admin.require_auth();
    Ok(admin)
}

fn require_not_paused(env: &Env) -> Result<(), Error> {
    let is_paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::IsPaused)
        .unwrap_or(false);
    if is_paused {
        return Err(Error::Paused);
    }
    Ok(())
}

#[contractimpl]
impl ResolverContract {
    /// Initialize the resolver contract.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::IsPaused, &false);

        Ok(())
    }

    /// Submit a game result with a proof reference (e.g. signed transcript hash).
    /// The submitter must authenticate the call (the on-chain equivalent of a
    /// signature check) and starts a 24h dispute window.
    pub fn submit_result(
        env: Env,
        match_id: Symbol,
        winner: Address,
        loser: Address,
        reason: Symbol,
        proof: Bytes,
        submitter: Address,
    ) -> Result<(), Error> {
        submitter.require_auth();
        require_not_paused(&env)?;

        let rkey = result_key(&env, &match_id);
        if env.storage().persistent().has(&rkey) {
            return Err(Error::ResultExists);
        }

        let result = GameResult {
            winner: winner.clone(),
            loser,
            reason: reason.clone(),
            submitter: submitter.clone(),
            proof,
            submitted_at: env.ledger().timestamp(),
            status: ResultStatus::Pending,
        };

        env.storage().persistent().set(&rkey, &result);
        env.storage()
            .persistent()
            .extend_ttl(&rkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        env.events()
            .publish(("result", "submitted", match_id), (winner, submitter, reason));

        Ok(())
    }

    /// File a dispute during the 24h window. Only the recorded loser may dispute,
    /// and must authenticate as themselves.
    pub fn dispute(env: Env, match_id: Symbol, evidence: Bytes) -> Result<(), Error> {
        let rkey = result_key(&env, &match_id);
        let mut result: GameResult = env
            .storage()
            .persistent()
            .get(&rkey)
            .ok_or(Error::NoResult)?;

        result.loser.require_auth();

        let current_time = env.ledger().timestamp();
        if current_time > result.submitted_at + DISPUTE_WINDOW_SECS {
            return Err(Error::DisputeClosed);
        }

        let dckey = dispute_count_key(&env, &match_id);
        let dispute_count: u32 = env.storage().persistent().get(&dckey).unwrap_or(0);

        let ekey = evidence_key(&env, &match_id, dispute_count);
        env.storage().persistent().set(&ekey, &evidence);
        env.storage()
            .persistent()
            .extend_ttl(&ekey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        env.storage().persistent().set(&dckey, &(dispute_count + 1));

        result.status = ResultStatus::Disputed;
        env.storage().persistent().set(&rkey, &result);

        env.events()
            .publish(("dispute", "filed", match_id), result.loser);

        Ok(())
    }

    /// Finalize the result after the dispute window closes. Callable by anyone
    /// once the 24h window has passed.
    pub fn finalize(env: Env, match_id: Symbol) -> Result<Address, Error> {
        let rkey = result_key(&env, &match_id);
        let mut result: GameResult = env
            .storage()
            .persistent()
            .get(&rkey)
            .ok_or(Error::NoResult)?;

        let current_time = env.ledger().timestamp();
        if current_time < result.submitted_at + DISPUTE_WINDOW_SECS {
            return Err(Error::DisputeWindowOpen);
        }

        result.status = ResultStatus::Finalized;
        env.storage().persistent().set(&rkey, &result);

        env.events().publish(
            ("result", "finalized", match_id),
            result.winner.clone(),
        );

        Ok(result.winner)
    }

    /// Query the result status for a match.
    pub fn query_result_status(env: Env, match_id: Symbol) -> Result<(Address, ResultStatus), Error> {
        let rkey = result_key(&env, &match_id);
        let result: GameResult = env
            .storage()
            .persistent()
            .get(&rkey)
            .ok_or(Error::NoResult)?;

        Ok((result.winner, result.status))
    }

    /// Get the full recorded result for a match, including proof.
    pub fn get_result(env: Env, match_id: Symbol) -> Result<GameResult, Error> {
        let rkey = result_key(&env, &match_id);
        env.storage().persistent().get(&rkey).ok_or(Error::NoResult)
    }

    /// Get dispute evidence for a match at a given dispute index.
    pub fn get_dispute_evidence(env: Env, match_id: Symbol, index: u32) -> Result<Bytes, Error> {
        let ekey = evidence_key(&env, &match_id, index);
        env.storage()
            .persistent()
            .get(&ekey)
            .ok_or(Error::NoEvidence)
    }

    /// Number of disputes filed for a match.
    pub fn get_dispute_count(env: Env, match_id: Symbol) -> u32 {
        let dckey = dispute_count_key(&env, &match_id);
        env.storage().persistent().get(&dckey).unwrap_or(0)
    }

    /// Set admin (admin only).
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// Pause the contract (admin only).
    pub fn pause(env: Env) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::IsPaused, &true);
        env.events().publish(("contract", "paused"), admin);
        Ok(())
    }

    /// Unpause the contract (admin only).
    pub fn unpause(env: Env) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::IsPaused, &false);
        env.events().publish(("contract", "unpaused"), admin);
        Ok(())
    }

    /// Check whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::IsPaused)
            .unwrap_or(false)
    }

    /// Upgrade the contract's WASM code (admin only).
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        require_admin(&env)?;
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::Env;

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, ResolverContract);
        let client = ResolverContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);
    }

    #[test]
    fn test_submit_and_finalize() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, ResolverContract);
        let client = ResolverContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let winner = Address::generate(&env);
        let loser = Address::generate(&env);
        let submitter = Address::generate(&env);
        let match_id = Symbol::new(&env, "match1");
        let proof = Bytes::from_array(&env, &[1, 2, 3]);

        client.submit_result(
            &match_id,
            &winner,
            &loser,
            &Symbol::new(&env, "checkmate"),
            &proof,
            &submitter,
        );

        let (result_winner, status) = client.query_result_status(&match_id);
        assert_eq!(result_winner, winner);
        assert_eq!(status, ResultStatus::Pending);

        env.ledger()
            .with_mut(|l| l.timestamp += DISPUTE_WINDOW_SECS + 1);

        let finalized_winner = client.finalize(&match_id);
        assert_eq!(finalized_winner, winner);
    }

    #[test]
    fn test_dispute_by_loser() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, ResolverContract);
        let client = ResolverContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let winner = Address::generate(&env);
        let loser = Address::generate(&env);
        let submitter = Address::generate(&env);
        let match_id = Symbol::new(&env, "match1");
        let proof = Bytes::from_array(&env, &[1, 2, 3]);

        client.submit_result(
            &match_id,
            &winner,
            &loser,
            &Symbol::new(&env, "checkmate"),
            &proof,
            &submitter,
        );

        let evidence = Bytes::from_array(&env, &[9, 9, 9]);
        client.dispute(&match_id, &evidence);

        let (_, status) = client.query_result_status(&match_id);
        assert_eq!(status, ResultStatus::Disputed);
        assert_eq!(client.get_dispute_evidence(&match_id, &0), evidence);
    }
}
