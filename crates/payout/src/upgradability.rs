// Feature 7: Contract Upgradability
// Support for contract code upgrades with admin approval

use soroban_sdk::{Address, Env, Symbol, BytesN};

pub struct UpgradeProposal {
    pub proposal_id: String,
    pub proposed_by: Address,
    pub new_wasm_hash: BytesN<32>,
    pub status: UpgradeStatus,
    pub created_at: u64,
    pub executed_at: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpgradeStatus {
    Pending,
    Approved,
    Rejected,
    Executed,
}

pub fn propose_upgrade(
    env: &Env,
    proposal_id: &str,
    new_wasm_hash: BytesN<32>,
) -> Result<(), Symbol> {
    let admin: Address = env.storage()
        .instance()
        .get(&"admin")
        .ok_or(Symbol::short("no_admin"))?;

    admin.require_auth();

    let proposal = UpgradeProposal {
        proposal_id: proposal_id.to_string(),
        proposed_by: admin,
        new_wasm_hash,
        status: UpgradeStatus::Pending,
        created_at: env.ledger().timestamp(),
        executed_at: None,
    };

    let storage_key = format!("upgrade_proposal:{}", proposal_id);
    env.storage()
        .instance()
        .set(&storage_key, &proposal);

    env.events().publish(
        ("upgrade", "proposed"),
        (proposal_id, new_wasm_hash),
    );

    Ok(())
}

pub fn approve_upgrade(
    env: &Env,
    proposal_id: &str,
) -> Result<(), Symbol> {
    let admin: Address = env.storage()
        .instance()
        .get(&"admin")
        .ok_or(Symbol::short("no_admin"))?;

    admin.require_auth();

    let storage_key = format!("upgrade_proposal:{}", proposal_id);
    let mut proposal: UpgradeProposal = env.storage()
        .instance()
        .get(&storage_key)
        .ok_or(Symbol::short("proposal_not_found"))?;

    if proposal.status != UpgradeStatus::Pending {
        return Err(Symbol::short("invalid_status"));
    }

    proposal.status = UpgradeStatus::Approved;
    env.storage()
        .instance()
        .set(&storage_key, &proposal);

    env.events().publish(
        ("upgrade", "approved"),
        proposal_id,
    );

    Ok(())
}

pub fn execute_upgrade(
    env: &Env,
    proposal_id: &str,
) -> Result<(), Symbol> {
    let admin: Address = env.storage()
        .instance()
        .get(&"admin")
        .ok_or(Symbol::short("no_admin"))?;

    admin.require_auth();

    let storage_key = format!("upgrade_proposal:{}", proposal_id);
    let mut proposal: UpgradeProposal = env.storage()
        .instance()
        .get(&storage_key)
        .ok_or(Symbol::short("proposal_not_found"))?;

    if proposal.status != UpgradeStatus::Approved {
        return Err(Symbol::short("not_approved"));
    }

    proposal.status = UpgradeStatus::Executed;
    proposal.executed_at = Some(env.ledger().timestamp());
    env.storage()
        .instance()
        .set(&storage_key, &proposal);

    env.events().publish(
        ("upgrade", "executed"),
        (proposal_id, proposal.new_wasm_hash),
    );

    Ok(())
}

pub fn get_current_wasm_version(env: &Env) -> Result<BytesN<32>, Symbol> {
    env.storage()
        .instance()
        .get(&"wasm_version")
        .ok_or(Symbol::short("version_not_found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upgrade_status_progression() {
        let initial = UpgradeStatus::Pending;
        assert_eq!(initial, UpgradeStatus::Pending);

        let approved = UpgradeStatus::Approved;
        assert_eq!(approved, UpgradeStatus::Approved);

        let executed = UpgradeStatus::Executed;
        assert_eq!(executed, UpgradeStatus::Executed);
    }
}
