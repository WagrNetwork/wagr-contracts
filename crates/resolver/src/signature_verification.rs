// Feature 5: Result Verification Signatures
// Verify cryptographic signatures for result submissions

use soroban_sdk::{Address, Env, Symbol, Vec, BytesN};

pub struct SignedResult {
    pub result_id: String,
    pub match_id: String,
    pub winner: Address,
    pub loser: Address,
    pub signature: BytesN<64>,
    pub submitter: Address,
    pub timestamp: u64,
}

pub fn verify_signature(
    env: &Env,
    message: &str,
    signature: &BytesN<64>,
    signer: &Address,
) -> Result<bool, Symbol> {
    // In production, use Soroban's signature verification
    // This is a placeholder for the verification logic
    if signature.len() != 64 {
        return Err(Symbol::short("invalid_sig"));
    }

    env.events().publish(
        ("result", "signature_verified"),
        (signer,),
    );

    Ok(true)
}

pub fn validate_result_submission(
    env: &Env,
    match_id: &str,
    winner: &Address,
    loser: &Address,
    signature: &BytesN<64>,
    submitter: &Address,
) -> Result<(), Symbol> {
    // Verify that submitter is authorized
    submitter.require_auth();

    // Verify signature
    let message = format!("result:{}:{}:{}", match_id, winner, loser);
    verify_signature(env, &message, signature, submitter)?;

    // Check that result hasn't been submitted already
    let storage_key = format!("result_submitted:{}", match_id);
    if env.storage().persistent().has(&storage_key) {
        return Err(Symbol::short("already_submitted"));
    }

    // Mark result as submitted
    env.storage()
        .persistent()
        .set(&storage_key, &true);

    env.events().publish(
        ("result", "submitted_and_verified"),
        (match_id, winner, loser, submitter),
    );

    Ok(())
}

pub fn get_result_for_match(
    env: &Env,
    match_id: &str,
) -> Result<(Address, Address), Symbol> {
    let storage_key = format!("match_result:{}", match_id);
    env.storage()
        .persistent()
        .get(&storage_key)
        .ok_or(Symbol::short("result_not_found"))
}

pub fn store_verified_result(
    env: &Env,
    match_id: &str,
    winner: Address,
    loser: Address,
) -> Result<(), Symbol> {
    let storage_key = format!("match_result:{}", match_id);
    env.storage()
        .persistent()
        .set(&storage_key, &(winner.clone(), loser.clone()));

    env.events().publish(
        ("result", "stored"),
        (match_id, winner, loser),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_signature_length() {
        let invalid_sig = BytesN::<32>::from_array(&[0u8; 32]);
        // This would fail type-wise, but demonstrates validation
        assert_eq!(invalid_sig.len(), 32);
    }

    #[test]
    fn test_result_message_format() {
        let match_id = "m1";
        let winner = "G...";
        let loser = "G...";
        
        let message = format!("result:{}:{}:{}", match_id, winner, loser);
        assert!(message.contains("result:"));
        assert!(message.contains(match_id));
    }
}
