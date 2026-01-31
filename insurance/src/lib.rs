#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Vec,
};

// Storage TTL constants for active data
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

// Storage TTL constants for archived data (longer retention, less frequent access)
const ARCHIVE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const ARCHIVE_BUMP_AMOUNT: u32 = 2592000; // ~180 days (6 months)

/// Insurance policy data structure with owner tracking for access control
#[derive(Clone)]
#[contracttype]
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub coverage_type: String,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub active: bool,
    pub next_payment_date: u64,
}

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone)]
pub enum InsuranceEvent {
    PolicyCreated,
    PremiumPaid,
    PolicyDeactivated,
}

/// Archived policy - compressed record with essential fields only
#[contracttype]
#[derive(Clone)]
pub struct ArchivedPolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub coverage_type: String,
    pub total_coverage: i128,
    pub deactivated_at: u64,
    pub archived_at: u64,
}

/// Storage statistics for monitoring
#[contracttype]
#[derive(Clone)]
pub struct StorageStats {
    pub active_policies: u32,
    pub archived_policies: u32,
    pub total_active_coverage: i128,
    pub total_archived_coverage: i128,
    pub last_updated: u64,
}

/// Events for archival operations
#[contracttype]
#[derive(Clone)]
pub enum ArchiveEvent {
    PoliciesArchived,
    PolicyRestored,
    ArchivesCleaned,
}

#[contract]
pub struct Insurance;

#[contractimpl]
impl Insurance {
    /// Create a new insurance policy
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner (must authorize)
    /// * `name` - Name of the policy
    /// * `coverage_type` - Type of coverage (e.g., "health", "emergency")
    /// * `monthly_premium` - Monthly premium amount (must be positive)
    /// * `coverage_amount` - Total coverage amount (must be positive)
    ///
    /// # Returns
    /// The ID of the created policy
    ///
    /// # Panics
    /// - If owner doesn't authorize the transaction
    /// - If monthly_premium is not positive
    /// - If coverage_amount is not positive
    pub fn create_policy(
        env: Env,
        owner: Address,
        name: String,
        coverage_type: String,
        monthly_premium: i128,
        coverage_amount: i128,
    ) -> u32 {
        // Access control: require owner authorization
        owner.require_auth();

        // Input validation
        if monthly_premium <= 0 {
            panic!("Monthly premium must be positive");
        }
        if coverage_amount <= 0 {
            panic!("Coverage amount must be positive");
        }

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
            + 1;

        // Set next payment date to 30 days from now
        let next_payment_date = env.ledger().timestamp() + (30 * 86400);

        let policy = InsurancePolicy {
            id: next_id,
            owner: owner.clone(),
            name: name.clone(),
            coverage_type: coverage_type.clone(),
            monthly_premium,
            coverage_amount,
            active: true,
            next_payment_date,
        };

        let policy_owner = policy.owner.clone();
        policies.set(next_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PolicyCreated),
            (next_id, policy_owner),
        );

        next_id
    }

    /// Pay monthly premium for a policy
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the policy owner)
    /// * `policy_id` - ID of the policy
    ///
    /// # Returns
    /// True if payment was successful
    ///
    /// # Panics
    /// - If caller is not the policy owner
    /// - If policy is not found
    /// - If policy is not active
    pub fn pay_premium(env: Env, caller: Address, policy_id: u32) -> bool {
        // Access control: require caller authorization
        caller.require_auth();

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");

        // Access control: verify caller is the owner
        if policy.owner != caller {
            panic!("Only the policy owner can pay premiums");
        }

        if !policy.active {
            panic!("Policy is not active");
        }

        // Update next payment date to 30 days from now
        policy.next_payment_date = env.ledger().timestamp() + (30 * 86400);

        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PremiumPaid),
            (policy_id, caller),
        );

        true
    }

    /// Get a policy by ID
    ///
    /// # Arguments
    /// * `policy_id` - ID of the policy
    ///
    /// # Returns
    /// InsurancePolicy struct or None if not found
    pub fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        policies.get(policy_id)
    }

    /// Get all active policies for a specific owner
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner
    ///
    /// # Returns
    /// Vec of active InsurancePolicy structs belonging to the owner
    pub fn get_active_policies(env: Env, owner: Address) -> Vec<InsurancePolicy> {
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for (_, policy) in policies.iter() {
            if policy.active && policy.owner == owner {
                result.push_back(policy);
            }
        }
        result
    }

    /// Get total monthly premium for all active policies of an owner
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner
    ///
    /// # Returns
    /// Total monthly premium amount for the owner's active policies
    pub fn get_total_monthly_premium(env: Env, owner: Address) -> i128 {
        let mut total = 0i128;
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        for (_, policy) in policies.iter() {
            if policy.active && policy.owner == owner {
                total += policy.monthly_premium;
            }
        }
        total
    }

    /// Deactivate a policy
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the policy owner)
    /// * `policy_id` - ID of the policy
    ///
    /// # Returns
    /// True if deactivation was successful
    ///
    /// # Panics
    /// - If caller is not the policy owner
    /// - If policy is not found
    pub fn deactivate_policy(env: Env, caller: Address, policy_id: u32) -> bool {
        // Access control: require caller authorization
        caller.require_auth();

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");

        // Access control: verify caller is the owner
        if policy.owner != caller {
            panic!("Only the policy owner can deactivate this policy");
        }

        policy.active = false;
        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PolicyDeactivated),
            (policy_id, caller),
        );

        true
    }

    /// Archive inactive policies that were deactivated before the specified timestamp.
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must authorize)
    /// * `before_timestamp` - Archive policies deactivated before this timestamp
    ///
    /// # Returns
    /// Number of policies archived
    pub fn archive_inactive_policies(env: Env, caller: Address, before_timestamp: u64) -> u32 {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_POL"))
            .unwrap_or_else(|| Map::new(&env));

        let current_time = env.ledger().timestamp();
        let mut archived_count = 0u32;
        let mut to_remove: Vec<u32> = Vec::new(&env);

        for (id, policy) in policies.iter() {
            // Archive if policy is inactive and next_payment_date is before the specified timestamp
            if !policy.active && policy.next_payment_date < before_timestamp {
                let archived_policy = ArchivedPolicy {
                    id: policy.id,
                    owner: policy.owner.clone(),
                    name: policy.name.clone(),
                    coverage_type: policy.coverage_type.clone(),
                    total_coverage: policy.coverage_amount,
                    deactivated_at: policy.next_payment_date,
                    archived_at: current_time,
                };
                archived.set(id, archived_policy);
                to_remove.push_back(id);
                archived_count += 1;
            }
        }

        for i in 0..to_remove.len() {
            if let Some(id) = to_remove.get(i) {
                policies.remove(id);
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);
        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_POL"), &archived);

        Self::extend_archive_ttl(&env);
        Self::update_storage_stats(&env);

        env.events().publish(
            (symbol_short!("insure"), ArchiveEvent::PoliciesArchived),
            (archived_count, caller),
        );

        archived_count
    }

    /// Get all archived policies for a specific owner
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner
    ///
    /// # Returns
    /// Vec of all ArchivedPolicy structs belonging to the owner
    pub fn get_archived_policies(env: Env, owner: Address) -> Vec<ArchivedPolicy> {
        let archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_POL"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for (_, policy) in archived.iter() {
            if policy.owner == owner {
                result.push_back(policy);
            }
        }
        result
    }

    /// Get a specific archived policy by ID
    ///
    /// # Arguments
    /// * `policy_id` - ID of the archived policy
    ///
    /// # Returns
    /// ArchivedPolicy struct or None if not found
    pub fn get_archived_policy(env: Env, policy_id: u32) -> Option<ArchivedPolicy> {
        let archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_POL"))
            .unwrap_or_else(|| Map::new(&env));

        archived.get(policy_id)
    }

    /// Restore an archived policy back to active storage
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the policy owner)
    /// * `policy_id` - ID of the policy to restore
    ///
    /// # Returns
    /// True if restoration was successful
    ///
    /// # Panics
    /// - If caller is not the policy owner
    /// - If policy is not found in archive
    pub fn restore_policy(env: Env, caller: Address, policy_id: u32) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_POL"))
            .unwrap_or_else(|| Map::new(&env));

        let archived_policy = match archived.get(policy_id) {
            Some(p) => p,
            None => panic!("Archived policy not found"),
        };

        if archived_policy.owner != caller {
            panic!("Only the policy owner can restore this policy");
        }

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let restored_policy = InsurancePolicy {
            id: archived_policy.id,
            owner: archived_policy.owner.clone(),
            name: archived_policy.name.clone(),
            coverage_type: archived_policy.coverage_type.clone(),
            monthly_premium: archived_policy.total_coverage / 12, // Estimate monthly premium
            coverage_amount: archived_policy.total_coverage,
            active: false, // Restored as inactive, needs reactivation
            next_payment_date: env.ledger().timestamp() + (30 * 86400),
        };

        policies.set(policy_id, restored_policy);
        archived.remove(policy_id);

        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);
        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_POL"), &archived);

        Self::update_storage_stats(&env);

        env.events().publish(
            (symbol_short!("insure"), ArchiveEvent::PolicyRestored),
            (policy_id, caller),
        );

        true
    }

    /// Permanently delete old archives before specified timestamp
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must authorize)
    /// * `before_timestamp` - Delete archives created before this timestamp
    ///
    /// # Returns
    /// Number of archives deleted
    pub fn bulk_cleanup_policies(env: Env, caller: Address, before_timestamp: u64) -> u32 {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_POL"))
            .unwrap_or_else(|| Map::new(&env));

        let mut deleted_count = 0u32;
        let mut to_remove: Vec<u32> = Vec::new(&env);

        for (id, policy) in archived.iter() {
            if policy.archived_at < before_timestamp {
                to_remove.push_back(id);
                deleted_count += 1;
            }
        }

        for i in 0..to_remove.len() {
            if let Some(id) = to_remove.get(i) {
                archived.remove(id);
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_POL"), &archived);

        Self::update_storage_stats(&env);

        env.events().publish(
            (symbol_short!("insure"), ArchiveEvent::ArchivesCleaned),
            (deleted_count, caller),
        );

        deleted_count
    }

    /// Get storage usage statistics
    ///
    /// # Returns
    /// StorageStats struct with current storage metrics
    pub fn get_storage_stats(env: Env) -> StorageStats {
        env.storage()
            .instance()
            .get(&symbol_short!("STOR_STAT"))
            .unwrap_or(StorageStats {
                active_policies: 0,
                archived_policies: 0,
                total_active_coverage: 0,
                total_archived_coverage: 0,
                last_updated: 0,
            })
    }

    /// Extend the TTL of instance storage
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Extend the TTL of archive storage with longer duration
    fn extend_archive_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(ARCHIVE_LIFETIME_THRESHOLD, ARCHIVE_BUMP_AMOUNT);
    }

    /// Update storage statistics
    fn update_storage_stats(env: &Env) {
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(env));

        let archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_POL"))
            .unwrap_or_else(|| Map::new(env));

        let mut active_count = 0u32;
        let mut active_coverage = 0i128;
        for (_, policy) in policies.iter() {
            if policy.active {
                active_count += 1;
                active_coverage = active_coverage.saturating_add(policy.coverage_amount);
            }
        }

        let mut archived_count = 0u32;
        let mut archived_coverage = 0i128;
        for (_, policy) in archived.iter() {
            archived_count += 1;
            archived_coverage = archived_coverage.saturating_add(policy.total_coverage);
        }

        let stats = StorageStats {
            active_policies: active_count,
            archived_policies: archived_count,
            total_active_coverage: active_coverage,
            total_archived_coverage: archived_coverage,
            last_updated: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&symbol_short!("STOR_STAT"), &stats);
    }
}

mod test;
