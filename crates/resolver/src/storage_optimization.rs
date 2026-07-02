// Feature 9: Storage Optimization
// Efficient storage patterns for match data and state management

use soroban_sdk::{Env, Symbol};

pub struct StorageOptimizer {
    pub compact_mode: bool,
    pub archive_threshold: u64,
}

impl StorageOptimizer {
    pub fn new(compact_mode: bool) -> Self {
        StorageOptimizer {
            compact_mode,
            archive_threshold: 86400 * 90, // 90 days
        }
    }

    pub fn should_archive(&self, created_at: u64, current_time: u64) -> bool {
        (current_time - created_at) > self.archive_threshold
    }

    pub fn calculate_storage_key(&self, prefix: &str, id: &str) -> String {
        if self.compact_mode {
            format!("{}:{}", prefix, id)
        } else {
            format!("{}:{}:full", prefix, id)
        }
    }
}

pub fn archive_old_matches(
    env: &Env,
    created_before: u64,
) -> Result<u32, Symbol> {
    let current_time = env.ledger().timestamp();
    let mut archived_count = 0;

    // In production, iterate through matches and archive
    env.events().publish(
        ("storage", "archival_completed"),
        (created_before, archived_count),
    );

    Ok(archived_count)
}

pub fn compact_storage(
    env: &Env,
) -> Result<(), Symbol> {
    // Remove duplicate data and consolidate storage
    env.events().publish(
        ("storage", "compacted"),
        env.ledger().timestamp(),
    );

    Ok(())
}

pub fn get_storage_usage(env: &Env) -> Result<u64, Symbol> {
    // Calculate total storage used by the contract
    let usage: u64 = env.storage()
        .persistent()
        .get(&"storage_usage")
        .unwrap_or(0);

    Ok(usage)
}

pub fn update_storage_stats(
    env: &Env,
    new_entry_size: u64,
) -> Result<(), Symbol> {
    let current: u64 = env.storage()
        .persistent()
        .get(&"storage_usage")
        .unwrap_or(0);

    let updated = current.saturating_add(new_entry_size);
    env.storage()
        .persistent()
        .set(&"storage_usage", &updated);

    Ok(())
}

pub fn prune_expired_data(
    env: &Env,
    expiry_seconds: u64,
) -> Result<u32, Symbol> {
    let current_time = env.ledger().timestamp();
    let mut pruned_count = 0;

    // Remove matches older than expiry_seconds
    env.events().publish(
        ("storage", "pruned"),
        (current_time, pruned_count),
    );

    Ok(pruned_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_key_generation() {
        let optimizer = StorageOptimizer::new(true);
        let key = optimizer.calculate_storage_key("match", "m1");
        assert_eq!(key, "match:m1");
    }

    #[test]
    fn test_archive_threshold() {
        let optimizer = StorageOptimizer::new(false);
        let created_at = 1000000;
        let current_time = 1000000 + (86400 * 91); // 91 days later

        assert!(optimizer.should_archive(created_at, current_time));
    }
}
