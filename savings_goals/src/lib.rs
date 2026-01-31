#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Symbol, Vec,
};

// Storage TTL constants for active data
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

// Storage TTL constants for archived data (longer retention, less frequent access)
const ARCHIVE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const ARCHIVE_BUMP_AMOUNT: u32 = 2592000; // ~180 days (6 months)

/// Savings goal data structure with owner tracking for access control
#[contract]
pub struct SavingsGoalContract;

#[contracttype]
#[derive(Clone)]
pub struct SavingsGoal {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub target_amount: i128,
    pub current_amount: i128,
    pub target_date: u64,
    pub locked: bool,
}

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone)]
pub enum SavingsEvent {
    GoalCreated,
    FundsAdded,
    FundsWithdrawn,
    GoalCompleted,
    GoalLocked,
    GoalUnlocked,
}

/// Snapshot for goals export/import (migration). Checksum is numeric for on-chain verification.
#[contracttype]
#[derive(Clone)]
pub struct GoalsExportSnapshot {
    pub version: u32,
    pub checksum: u64,
    pub next_id: u32,
    pub goals: Vec<SavingsGoal>,
}

/// Audit log entry for security and compliance.
#[contracttype]
#[derive(Clone)]
pub struct AuditEntry {
    pub operation: Symbol,
    pub caller: Address,
    pub timestamp: u64,
    pub success: bool,
}

/// Archived goal - compressed record with essential fields only
#[contracttype]
#[derive(Clone)]
pub struct ArchivedGoal {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub target_amount: i128,
    pub final_amount: i128,
    pub archived_at: u64,
}

/// Storage statistics for monitoring
#[contracttype]
#[derive(Clone)]
pub struct StorageStats {
    pub active_goals: u32,
    pub archived_goals: u32,
    pub total_active_amount: i128,
    pub total_archived_amount: i128,
    pub last_updated: u64,
}

/// Events for archival operations
#[contracttype]
#[derive(Clone)]
pub enum ArchiveEvent {
    GoalsArchived,
    GoalRestored,
    ArchivesCleaned,
}

const SNAPSHOT_VERSION: u32 = 1;
const MAX_AUDIT_ENTRIES: u32 = 100;

#[contractimpl]
impl SavingsGoalContract {
    // Storage keys
    const STORAGE_NEXT_ID: Symbol = symbol_short!("NEXT_ID");
    const STORAGE_GOALS: Symbol = symbol_short!("GOALS");

    /// Initialize contract storage
    pub fn init(env: Env) {
        let storage = env.storage().persistent();

        if storage.get::<_, u32>(&Self::STORAGE_NEXT_ID).is_none() {
            storage.set(&Self::STORAGE_NEXT_ID, &1u32);
        }

        if storage
            .get::<_, Map<u32, SavingsGoal>>(&Self::STORAGE_GOALS)
            .is_none()
        {
            storage.set(&Self::STORAGE_GOALS, &Map::<u32, SavingsGoal>::new(&env));
        }
    }

    /// Create a new savings goal
    ///
    /// # Arguments
    /// * `owner` - Address of the goal owner (must authorize)
    /// * `name` - Name of the goal (e.g., "Education", "Medical")
    /// * `target_amount` - Target amount to save (must be positive)
    /// * `target_date` - Target date as Unix timestamp
    ///
    /// # Returns
    /// The ID of the created goal
    ///
    /// # Panics
    /// - If owner doesn't authorize the transaction
    /// - If target_amount is not positive
    pub fn create_goal(
        env: Env,
        owner: Address,
        name: String,
        target_amount: i128,
        target_date: u64,
    ) -> u32 {
        // Access control: require owner authorization
        owner.require_auth();

        // Input validation
        if target_amount <= 0 {
            Self::append_audit(&env, symbol_short!("create"), &owner, false);
            panic!("Target amount must be positive");
        }

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
            + 1;

        let goal = SavingsGoal {
            id: next_id,
            owner: owner.clone(),
            name,
            target_amount,
            current_amount: 0,
            target_date,
            locked: true,
        };

        goals.set(next_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);

        Self::append_audit(&env, symbol_short!("create"), &owner, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::GoalCreated),
            (next_id, owner),
        );

        next_id
    }

    /// Add funds to a savings goal
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the goal owner)
    /// * `goal_id` - ID of the goal
    /// * `amount` - Amount to add (must be positive)
    ///
    /// # Returns
    /// Updated current amount
    ///
    /// # Panics
    /// - If caller is not the goal owner
    /// - If goal is not found
    /// - If amount is not positive
    pub fn add_to_goal(env: Env, caller: Address, goal_id: u32, amount: i128) -> i128 {
        // Access control: require caller authorization
        caller.require_auth();

        // Input validation
        if amount <= 0 {
            Self::append_audit(&env, symbol_short!("add"), &caller, false);
            panic!("Amount must be positive");
        }

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("add"), &caller, false);
                panic!("Goal not found");
            }
        };

        // Access control: verify caller is the owner
        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("add"), &caller, false);
            panic!("Only the goal owner can add funds");
        }

        goal.current_amount = goal.current_amount.checked_add(amount).expect("overflow");
        let new_amount = goal.current_amount;
        let is_completed = goal.current_amount >= goal.target_amount;
        let goal_owner = goal.owner.clone();

        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        Self::append_audit(&env, symbol_short!("add"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::FundsAdded),
            (goal_id, goal_owner.clone(), amount),
        );

        // Emit completion event if goal is now complete
        if is_completed {
            env.events().publish(
                (symbol_short!("savings"), SavingsEvent::GoalCompleted),
                (goal_id, goal_owner),
            );
        }

        new_amount
    }

    /// Withdraw funds from a savings goal
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the goal owner)
    /// * `goal_id` - ID of the goal
    /// * `amount` - Amount to withdraw (must be positive and <= current_amount)
    ///
    /// # Returns
    /// Updated current amount
    ///
    /// # Panics
    /// - If caller is not the goal owner
    /// - If goal is not found
    /// - If goal is locked
    /// - If amount is not positive
    /// - If amount exceeds current balance
    pub fn withdraw_from_goal(env: Env, caller: Address, goal_id: u32, amount: i128) -> i128 {
        // Access control: require caller authorization
        caller.require_auth();

        // Input validation
        if amount <= 0 {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            panic!("Amount must be positive");
        }

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
                panic!("Goal not found");
            }
        };

        // Access control: verify caller is the owner
        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            panic!("Only the goal owner can withdraw funds");
        }

        // Check if goal is locked
        if goal.locked {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            panic!("Cannot withdraw from a locked goal");
        }

        // Check sufficient balance
        if amount > goal.current_amount {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            panic!("Insufficient balance");
        }

        goal.current_amount = goal.current_amount.checked_sub(amount).expect("underflow");
        let new_amount = goal.current_amount;

        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        Self::append_audit(&env, symbol_short!("withdraw"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::FundsWithdrawn),
            (goal_id, caller, amount),
        );

        new_amount
    }

    /// Lock a savings goal (prevent withdrawals)
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the goal owner)
    /// * `goal_id` - ID of the goal
    ///
    /// # Panics
    /// - If caller is not the goal owner
    /// - If goal is not found
    pub fn lock_goal(env: Env, caller: Address, goal_id: u32) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("lock"), &caller, false);
                panic!("Goal not found");
            }
        };

        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("lock"), &caller, false);
            panic!("Only the goal owner can lock this goal");
        }

        goal.locked = true;
        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        Self::append_audit(&env, symbol_short!("lock"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::GoalLocked),
            (goal_id, caller),
        );

        true
    }

    /// Unlock a savings goal (allow withdrawals)
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the goal owner)
    /// * `goal_id` - ID of the goal
    ///
    /// # Panics
    /// - If caller is not the goal owner
    /// - If goal is not found
    pub fn unlock_goal(env: Env, caller: Address, goal_id: u32) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("unlock"), &caller, false);
                panic!("Goal not found");
            }
        };

        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("unlock"), &caller, false);
            panic!("Only the goal owner can unlock this goal");
        }

        goal.locked = false;
        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        Self::append_audit(&env, symbol_short!("unlock"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::GoalUnlocked),
            (goal_id, caller),
        );

        true
    }

    /// Get a savings goal by ID
    ///
    /// # Arguments
    /// * `goal_id` - ID of the goal
    ///
    /// # Returns
    /// SavingsGoal struct or None if not found
    pub fn get_goal(env: Env, goal_id: u32) -> Option<SavingsGoal> {
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        goals.get(goal_id)
    }

    /// Get all savings goals for a specific owner
    ///
    /// # Arguments
    /// * `owner` - Address of the goal owner
    ///
    /// # Returns
    /// Vec of all SavingsGoal structs belonging to the owner
    pub fn get_all_goals(env: Env, owner: Address) -> Vec<SavingsGoal> {
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for (_, goal) in goals.iter() {
            if goal.owner == owner {
                result.push_back(goal);
            }
        }
        result
    }

    /// Check if a goal is completed
    pub fn is_goal_completed(env: Env, goal_id: u32) -> bool {
        let storage = env.storage().instance();
        let goals: Map<u32, SavingsGoal> = storage
            .get(&symbol_short!("GOALS"))
            .unwrap_or(Map::new(&env));
        if let Some(goal) = goals.get(goal_id) {
            goal.current_amount >= goal.target_amount
        } else {
            false
        }
    }

    /// Get current nonce for an address (for import_snapshot replay protection).
    pub fn get_nonce(env: Env, address: Address) -> u64 {
        let nonces: Option<Map<Address, u64>> =
            env.storage().instance().get(&symbol_short!("NONCES"));
        nonces.as_ref().and_then(|m| m.get(address)).unwrap_or(0)
    }

    /// Export all goals as snapshot for backup/migration.
    pub fn export_snapshot(env: Env, caller: Address) -> GoalsExportSnapshot {
        caller.require_auth();
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));
        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);
        let mut list = Vec::new(&env);
        for i in 1..=next_id {
            if let Some(g) = goals.get(i) {
                list.push_back(g);
            }
        }
        let checksum = Self::compute_goals_checksum(SNAPSHOT_VERSION, next_id, &list);
        GoalsExportSnapshot {
            version: SNAPSHOT_VERSION,
            checksum,
            next_id,
            goals: list,
        }
    }

    /// Import snapshot (full restore). Validates version and checksum. Requires nonce for replay protection.
    pub fn import_snapshot(
        env: Env,
        caller: Address,
        nonce: u64,
        snapshot: GoalsExportSnapshot,
    ) -> bool {
        caller.require_auth();
        Self::require_nonce(&env, &caller, nonce);

        if snapshot.version != SNAPSHOT_VERSION {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            panic!("Unsupported snapshot version");
        }
        let expected =
            Self::compute_goals_checksum(snapshot.version, snapshot.next_id, &snapshot.goals);
        if snapshot.checksum != expected {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            panic!("Snapshot checksum mismatch");
        }

        Self::extend_instance_ttl(&env);
        let mut goals: Map<u32, SavingsGoal> = Map::new(&env);
        for g in snapshot.goals.iter() {
            goals.set(g.id, g);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &snapshot.next_id);

        Self::increment_nonce(&env, &caller);
        Self::append_audit(&env, symbol_short!("import"), &caller, true);
        true
    }

    /// Return recent audit log entries.
    pub fn get_audit_log(env: Env, from_index: u32, limit: u32) -> Vec<AuditEntry> {
        let log: Option<Vec<AuditEntry>> = env.storage().instance().get(&symbol_short!("AUDIT"));
        let log = log.unwrap_or_else(|| Vec::new(&env));
        let len = log.len();
        let cap = MAX_AUDIT_ENTRIES.min(limit);
        let mut out = Vec::new(&env);
        if from_index >= len {
            return out;
        }
        let end = (from_index + cap).min(len);
        for i in from_index..end {
            if let Some(entry) = log.get(i) {
                out.push_back(entry);
            }
        }
        out
    }

    /// Archive completed goals that were completed before the specified timestamp.
    /// Moves completed goals from active storage to archive storage.
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must authorize)
    /// * `before_timestamp` - Archive goals completed before this timestamp
    ///
    /// # Returns
    /// Number of goals archived
    pub fn archive_completed_goals(env: Env, caller: Address, before_timestamp: u64) -> u32 {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut archived: Map<u32, ArchivedGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_GOAL"))
            .unwrap_or_else(|| Map::new(&env));

        let current_time = env.ledger().timestamp();
        let mut archived_count = 0u32;
        let mut to_remove: Vec<u32> = Vec::new(&env);

        for (id, goal) in goals.iter() {
            // Archive if goal is completed and target_date is before the specified timestamp
            if goal.current_amount >= goal.target_amount && goal.target_date < before_timestamp {
                let archived_goal = ArchivedGoal {
                    id: goal.id,
                    owner: goal.owner.clone(),
                    name: goal.name.clone(),
                    target_amount: goal.target_amount,
                    final_amount: goal.current_amount,
                    archived_at: current_time,
                };
                archived.set(id, archived_goal);
                to_remove.push_back(id);
                archived_count += 1;
            }
        }

        // Remove archived goals from active storage
        for i in 0..to_remove.len() {
            if let Some(id) = to_remove.get(i) {
                goals.remove(id);
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);
        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_GOAL"), &archived);

        // Extend archive TTL with longer duration
        Self::extend_archive_ttl(&env);

        // Update storage stats
        Self::update_storage_stats(&env);

        Self::append_audit(&env, symbol_short!("archive"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), ArchiveEvent::GoalsArchived),
            (archived_count, caller),
        );

        archived_count
    }

    /// Get all archived goals for a specific owner
    ///
    /// # Arguments
    /// * `owner` - Address of the goal owner
    ///
    /// # Returns
    /// Vec of all ArchivedGoal structs belonging to the owner
    pub fn get_archived_goals(env: Env, owner: Address) -> Vec<ArchivedGoal> {
        let archived: Map<u32, ArchivedGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_GOAL"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for (_, goal) in archived.iter() {
            if goal.owner == owner {
                result.push_back(goal);
            }
        }
        result
    }

    /// Get a specific archived goal by ID
    ///
    /// # Arguments
    /// * `goal_id` - ID of the archived goal
    ///
    /// # Returns
    /// ArchivedGoal struct or None if not found
    pub fn get_archived_goal(env: Env, goal_id: u32) -> Option<ArchivedGoal> {
        let archived: Map<u32, ArchivedGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_GOAL"))
            .unwrap_or_else(|| Map::new(&env));

        archived.get(goal_id)
    }

    /// Restore an archived goal back to active storage
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the goal owner)
    /// * `goal_id` - ID of the goal to restore
    ///
    /// # Returns
    /// True if restoration was successful
    ///
    /// # Panics
    /// - If caller is not the goal owner
    /// - If goal is not found in archive
    pub fn restore_goal(env: Env, caller: Address, goal_id: u32) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut archived: Map<u32, ArchivedGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_GOAL"))
            .unwrap_or_else(|| Map::new(&env));

        let archived_goal = match archived.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("restore"), &caller, false);
                panic!("Archived goal not found");
            }
        };

        if archived_goal.owner != caller {
            Self::append_audit(&env, symbol_short!("restore"), &caller, false);
            panic!("Only the goal owner can restore this goal");
        }

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        // Restore as a new active goal
        let restored_goal = SavingsGoal {
            id: archived_goal.id,
            owner: archived_goal.owner.clone(),
            name: archived_goal.name.clone(),
            target_amount: archived_goal.target_amount,
            current_amount: archived_goal.final_amount,
            target_date: env.ledger().timestamp() + 31536000, // Set new target 1 year from now
            locked: true,
        };

        goals.set(goal_id, restored_goal);
        archived.remove(goal_id);

        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);
        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_GOAL"), &archived);

        // Update storage stats
        Self::update_storage_stats(&env);

        Self::append_audit(&env, symbol_short!("restore"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), ArchiveEvent::GoalRestored),
            (goal_id, caller),
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
    pub fn cleanup_old_archives(env: Env, caller: Address, before_timestamp: u64) -> u32 {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut archived: Map<u32, ArchivedGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_GOAL"))
            .unwrap_or_else(|| Map::new(&env));

        let mut deleted_count = 0u32;
        let mut to_remove: Vec<u32> = Vec::new(&env);

        for (id, goal) in archived.iter() {
            if goal.archived_at < before_timestamp {
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
            .set(&symbol_short!("ARCH_GOAL"), &archived);

        // Update storage stats
        Self::update_storage_stats(&env);

        Self::append_audit(&env, symbol_short!("cleanup"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), ArchiveEvent::ArchivesCleaned),
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
                active_goals: 0,
                archived_goals: 0,
                total_active_amount: 0,
                total_archived_amount: 0,
                last_updated: 0,
            })
    }

    fn require_nonce(env: &Env, address: &Address, expected: u64) {
        let current = Self::get_nonce(env.clone(), address.clone());
        if expected != current {
            panic!("Invalid nonce: expected {}, got {}", current, expected);
        }
    }

    fn increment_nonce(env: &Env, address: &Address) {
        let current = Self::get_nonce(env.clone(), address.clone());
        let next = current.checked_add(1).expect("nonce overflow");
        let mut nonces: Map<Address, u64> = env
            .storage()
            .instance()
            .get(&symbol_short!("NONCES"))
            .unwrap_or_else(|| Map::new(env));
        nonces.set(address.clone(), next);
        env.storage()
            .instance()
            .set(&symbol_short!("NONCES"), &nonces);
    }

    fn compute_goals_checksum(version: u32, next_id: u32, goals: &Vec<SavingsGoal>) -> u64 {
        let mut c = version as u64 + next_id as u64;
        for i in 0..goals.len() {
            if let Some(g) = goals.get(i) {
                c = c
                    .wrapping_add(g.id as u64)
                    .wrapping_add(g.target_amount as u64)
                    .wrapping_add(g.current_amount as u64);
            }
        }
        c.wrapping_mul(31)
    }

    fn append_audit(env: &Env, operation: Symbol, caller: &Address, success: bool) {
        let timestamp = env.ledger().timestamp();
        let mut log: Vec<AuditEntry> = env
            .storage()
            .instance()
            .get(&symbol_short!("AUDIT"))
            .unwrap_or_else(|| Vec::new(env));
        if log.len() >= MAX_AUDIT_ENTRIES {
            let mut new_log = Vec::new(env);
            for i in 1..log.len() {
                if let Some(entry) = log.get(i) {
                    new_log.push_back(entry);
                }
            }
            log = new_log;
        }
        log.push_back(AuditEntry {
            operation,
            caller: caller.clone(),
            timestamp,
            success,
        });
        env.storage().instance().set(&symbol_short!("AUDIT"), &log);
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
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(env));

        let archived: Map<u32, ArchivedGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_GOAL"))
            .unwrap_or_else(|| Map::new(env));

        let mut active_count = 0u32;
        let mut active_amount = 0i128;
        for (_, goal) in goals.iter() {
            active_count += 1;
            active_amount = active_amount.saturating_add(goal.current_amount);
        }

        let mut archived_count = 0u32;
        let mut archived_amount = 0i128;
        for (_, goal) in archived.iter() {
            archived_count += 1;
            archived_amount = archived_amount.saturating_add(goal.final_amount);
        }

        let stats = StorageStats {
            active_goals: active_count,
            archived_goals: archived_count,
            total_active_amount: active_amount,
            total_archived_amount: archived_amount,
            last_updated: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&symbol_short!("STOR_STAT"), &stats);
    }
}

#[cfg(test)]
mod test;
