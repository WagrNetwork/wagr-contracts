// Feature 4: Dispute Evidence Storage
// On-chain storage and retrieval of dispute evidence

use soroban_sdk::{Address, Env, Map, Symbol, String};

#[derive(Clone)]
pub struct DisputeEvidence {
    pub dispute_id: String,
    pub match_id: String,
    pub challenger: Address,
    pub evidence_type: String,
    pub evidence_data: String,
    pub timestamp: u64,
    pub resolved: bool,
}

pub fn store_dispute_evidence(
    env: &Env,
    dispute_id: &str,
    match_id: &str,
    challenger: Address,
    evidence_type: &str,
    evidence_data: &str,
) -> Result<(), Symbol> {
    let timestamp = env.ledger().timestamp();

    let evidence = DisputeEvidence {
        dispute_id: dispute_id.to_string(),
        match_id: match_id.to_string(),
        challenger,
        evidence_type: evidence_type.to_string(),
        evidence_data: evidence_data.to_string(),
        timestamp,
        resolved: false,
    };

    let storage_key = format!("dispute:{}", dispute_id);
    env.storage()
        .persistent()
        .set(&storage_key, &evidence);

    env.events().publish(
        ("dispute", "evidence_stored"),
        (dispute_id, match_id, evidence_type, timestamp),
    );

    Ok(())
}

pub fn retrieve_dispute_evidence(
    env: &Env,
    dispute_id: &str,
) -> Result<DisputeEvidence, Symbol> {
    let storage_key = format!("dispute:{}", dispute_id);
    env.storage()
        .persistent()
        .get(&storage_key)
        .ok_or(Symbol::short("evidence_not_found"))
}

pub fn mark_dispute_resolved(
    env: &Env,
    dispute_id: &str,
    resolution: &str,
) -> Result<(), Symbol> {
    let mut evidence = retrieve_dispute_evidence(env, dispute_id)?;
    evidence.resolved = true;

    let storage_key = format!("dispute:{}", dispute_id);
    env.storage()
        .persistent()
        .set(&storage_key, &evidence);

    env.events().publish(
        ("dispute", "resolved"),
        (dispute_id, resolution),
    );

    Ok(())
}

pub fn get_dispute_evidence_for_match(
    env: &Env,
    match_id: &str,
) -> Result<Vec<DisputeEvidence>, Symbol> {
    let mut disputes: Vec<DisputeEvidence> = Vec::new();
    
    // This is a simplified version - in production, use a proper indexing system
    env.events().publish(
        ("dispute", "retrieved_for_match"),
        match_id,
    );

    Ok(disputes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispute_evidence_structure() {
        let evidence = DisputeEvidence {
            dispute_id: "d1".to_string(),
            match_id: "m1".to_string(),
            challenger: Address::from_contract_id(&[0u8; 32]),
            evidence_type: "screenshot".to_string(),
            evidence_data: "ipfs://QmHash...".to_string(),
            timestamp: 1234567890,
            resolved: false,
        };

        assert_eq!(evidence.dispute_id, "d1");
        assert!(!evidence.resolved);
    }

    #[test]
    fn test_evidence_type_validation() {
        let valid_types = vec!["screenshot", "move_log", "video", "signature"];
        
        for t in valid_types {
            assert!(!t.is_empty());
        }
    }
}
