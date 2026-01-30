#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token::TokenClient, vec, Address, Env, Map,
    Symbol, Vec,
};

#[derive(Clone)]
#[contracttype]
pub struct Allocation {
    pub category: Symbol,
    pub amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct AccountGroup {
    pub spending: Address,
    pub savings: Address,
    pub bills: Address,
    pub insurance: Address,
}

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

/// Split configuration with owner tracking for access control
#[derive(Clone)]
#[contracttype]
pub struct SplitConfig {
    pub owner: Address,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub initialized: bool,
}

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone)]
pub enum SplitEvent {
    Initialized,
    Updated,
    Calculated,
}

/// Snapshot for data export/import (migration). Checksum is a simple numeric digest for on-chain verification.
#[contracttype]
#[derive(Clone)]
pub struct ExportSnapshot {
    pub version: u32,
    pub checksum: u64,
    pub config: SplitConfig,
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

const SNAPSHOT_VERSION: u32 = 1;
const MAX_AUDIT_ENTRIES: u32 = 100;

#[contract]
pub struct RemittanceSplit;

#[contractimpl]
impl RemittanceSplit {
    /// Initialize a remittance split configuration
    ///
    /// # Arguments
    /// * `owner` - Address of the split owner (must authorize)
    /// * `nonce` - Caller's transaction nonce (must equal get_nonce(owner)) for replay protection
    /// * `spending_percent` - Percentage for spending (0-100)
    /// * `savings_percent` - Percentage for savings (0-100)
    /// * `bills_percent` - Percentage for bills (0-100)
    /// * `insurance_percent` - Percentage for insurance (0-100)
    ///
    /// # Returns
    /// True if initialization was successful
    ///
    /// # Panics
    /// - If owner doesn't authorize the transaction
    /// - If nonce is invalid (replay)
    /// - If percentages don't sum to 100
    /// - If split is already initialized (use update_split instead)
    pub fn initialize_split(
        env: Env,
        owner: Address,
        nonce: u64,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> bool {
        owner.require_auth();
        Self::require_nonce(&env, &owner, nonce);

        let existing: Option<SplitConfig> = env.storage().instance().get(&symbol_short!("CONFIG"));
        if existing.is_some() {
            Self::append_audit(&env, symbol_short!("init"), &owner, false);
            panic!("Split already initialized. Use update_split to modify.");
        }

        let total = spending_percent + savings_percent + bills_percent + insurance_percent;
        if total != 100 {
            Self::append_audit(&env, symbol_short!("init"), &owner, false);
            panic!("Percentages must sum to 100");
        }

        Self::extend_instance_ttl(&env);

        let config = SplitConfig {
            owner: owner.clone(),
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
            initialized: true,
        };

        env.storage()
            .instance()
            .set(&symbol_short!("CONFIG"), &config);
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &vec![
                &env,
                spending_percent,
                savings_percent,
                bills_percent,
                insurance_percent,
            ],
        );

        Self::increment_nonce(&env, &owner);
        Self::append_audit(&env, symbol_short!("init"), &owner, true);
        env.events()
            .publish((symbol_short!("split"), SplitEvent::Initialized), owner);

        true
    }

    /// Update an existing split configuration
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the owner)
    /// * `nonce` - Caller's transaction nonce for replay protection
    /// * `spending_percent` - New percentage for spending (0-100)
    /// * `savings_percent` - New percentage for savings (0-100)
    /// * `bills_percent` - New percentage for bills (0-100)
    /// * `insurance_percent` - New percentage for insurance (0-100)
    ///
    /// # Returns
    /// True if update was successful
    ///
    /// # Panics
    /// - If caller is not the owner
    /// - If nonce is invalid (replay)
    /// - If percentages don't sum to 100
    /// - If split is not initialized
    pub fn update_split(
        env: Env,
        caller: Address,
        nonce: u64,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> bool {
        caller.require_auth();
        Self::require_nonce(&env, &caller, nonce);

        let mut config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .expect("Split not initialized");

        if config.owner != caller {
            Self::append_audit(&env, symbol_short!("update"), &caller, false);
            panic!("Only the owner can update the split configuration");
        }

        let total = spending_percent + savings_percent + bills_percent + insurance_percent;
        if total != 100 {
            Self::append_audit(&env, symbol_short!("update"), &caller, false);
            panic!("Percentages must sum to 100");
        }

        Self::extend_instance_ttl(&env);

        config.spending_percent = spending_percent;
        config.savings_percent = savings_percent;
        config.bills_percent = bills_percent;
        config.insurance_percent = insurance_percent;

        env.storage()
            .instance()
            .set(&symbol_short!("CONFIG"), &config);
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &vec![
                &env,
                spending_percent,
                savings_percent,
                bills_percent,
                insurance_percent,
            ],
        );

        Self::increment_nonce(&env, &caller);
        Self::append_audit(&env, symbol_short!("update"), &caller, true);
        env.events()
            .publish((symbol_short!("split"), SplitEvent::Updated), caller);

        true
    }

    /// Get the current split configuration
    ///
    /// # Returns
    /// Vec containing [spending, savings, bills, insurance] percentages
    pub fn get_split(env: &Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&symbol_short!("SPLIT"))
            .unwrap_or_else(|| vec![env, 50, 30, 15, 5])
    }

    /// Get the full split configuration including owner
    ///
    /// # Returns
    /// SplitConfig or None if not initialized
    pub fn get_config(env: Env) -> Option<SplitConfig> {
        env.storage().instance().get(&symbol_short!("CONFIG"))
    }

    /// Calculate split amounts from a total remittance amount (checked arithmetic for overflow protection).
    ///
    /// # Arguments
    /// * `total_amount` - The total amount to split (must be positive)
    ///
    /// # Returns
    /// Vec containing [spending, savings, bills, insurance] amounts
    ///
    /// # Panics
    /// - If total_amount is not positive
    /// - On integer overflow
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        if total_amount <= 0 {
            panic!("Total amount must be positive");
        }

        let split = Self::get_split(&env);
        let s0 = split.get(0).unwrap() as i128;
        let s1 = split.get(1).unwrap() as i128;
        let s2 = split.get(2).unwrap() as i128;

        let spending = total_amount
            .checked_mul(s0)
            .and_then(|n| n.checked_div(100))
            .expect("overflow in split calculation");
        let savings = total_amount
            .checked_mul(s1)
            .and_then(|n| n.checked_div(100))
            .expect("overflow in split calculation");
        let bills = total_amount
            .checked_mul(s2)
            .and_then(|n| n.checked_div(100))
            .expect("overflow in split calculation");
        let insurance = total_amount
            .checked_sub(spending)
            .and_then(|n| n.checked_sub(savings))
            .and_then(|n| n.checked_sub(bills))
            .expect("overflow in split calculation");

        env.events().publish(
            (symbol_short!("split"), SplitEvent::Calculated),
            total_amount,
        );

        vec![&env, spending, savings, bills, insurance]
    }

    /// Distribute USDC according to the configured split
    pub fn distribute_usdc(
        env: Env,
        usdc_contract: Address,
        from: Address,
        nonce: u64,
        accounts: AccountGroup,
        total_amount: i128,
    ) -> bool {
        if total_amount <= 0 {
            Self::append_audit(&env, symbol_short!("distrib"), &from, false);
            return false;
        }

        from.require_auth();
        Self::require_nonce(&env, &from, nonce);

        let amounts = Self::calculate_split(env.clone(), total_amount);
        let recipients = [
            accounts.spending,
            accounts.savings,
            accounts.bills,
            accounts.insurance,
        ];
        let token = TokenClient::new(&env, &usdc_contract);

        for (amount, recipient) in amounts.into_iter().zip(recipients.iter()) {
            if amount > 0 {
                token.transfer(&from, recipient, &amount);
            }
        }

        Self::increment_nonce(&env, &from);
        Self::append_audit(&env, symbol_short!("distrib"), &from, true);
        true
    }

    /// Query USDC balance for an address
    pub fn get_usdc_balance(env: &Env, usdc_contract: Address, account: Address) -> i128 {
        TokenClient::new(env, &usdc_contract).balance(&account)
    }

    /// Returns a breakdown of the split by category and resulting amount
    pub fn get_split_allocations(env: &Env, total_amount: i128) -> Vec<Allocation> {
        let amounts = Self::calculate_split(env.clone(), total_amount);
        let categories = [
            symbol_short!("SPENDING"),
            symbol_short!("SAVINGS"),
            symbol_short!("BILLS"),
            symbol_short!("INSURANCE"),
        ];

        let mut result = Vec::new(env);
        for (category, amount) in categories.into_iter().zip(amounts.into_iter()) {
            result.push_back(Allocation { category, amount });
        }
        result
    }

    /// Get current nonce for an address (next call must use this value for replay protection).
    pub fn get_nonce(env: Env, address: Address) -> u64 {
        let nonces: Option<Map<Address, u64>> =
            env.storage().instance().get(&symbol_short!("NONCES"));
        nonces.as_ref().and_then(|m| m.get(address)).unwrap_or(0)
    }

    /// Export current config as snapshot for backup/migration (owner only).
    pub fn export_snapshot(env: Env, caller: Address) -> Option<ExportSnapshot> {
        caller.require_auth();
        let config: SplitConfig = env.storage().instance().get(&symbol_short!("CONFIG"))?;
        if config.owner != caller {
            panic!("Only the owner can export snapshot");
        }
        let checksum = Self::compute_checksum(SNAPSHOT_VERSION, &config);
        Some(ExportSnapshot {
            version: SNAPSHOT_VERSION,
            checksum,
            config,
        })
    }

    /// Import snapshot (restore config). Validates version and checksum. Owner only; contract must already be initialized.
    pub fn import_snapshot(
        env: Env,
        caller: Address,
        nonce: u64,
        snapshot: ExportSnapshot,
    ) -> bool {
        caller.require_auth();
        Self::require_nonce(&env, &caller, nonce);

        if snapshot.version != SNAPSHOT_VERSION {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            panic!("Unsupported snapshot version");
        }
        let expected = Self::compute_checksum(snapshot.version, &snapshot.config);
        if snapshot.checksum != expected {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            panic!("Snapshot checksum mismatch");
        }

        let existing: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .expect("Split not initialized");
        if existing.owner != caller {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            panic!("Only the owner can import snapshot");
        }

        let total = snapshot.config.spending_percent
            + snapshot.config.savings_percent
            + snapshot.config.bills_percent
            + snapshot.config.insurance_percent;
        if total != 100 {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            panic!("Invalid snapshot: percentages must sum to 100");
        }

        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("CONFIG"), &snapshot.config);
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &vec![
                &env,
                snapshot.config.spending_percent,
                snapshot.config.savings_percent,
                snapshot.config.bills_percent,
                snapshot.config.insurance_percent,
            ],
        );

        Self::increment_nonce(&env, &caller);
        Self::append_audit(&env, symbol_short!("import"), &caller, true);
        true
    }

    /// Return recent audit log entries (from_index, limit capped at MAX_AUDIT_ENTRIES).
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

    fn compute_checksum(version: u32, config: &SplitConfig) -> u64 {
        let v = version as u64;
        let s = config.spending_percent as u64;
        let g = config.savings_percent as u64;
        let b = config.bills_percent as u64;
        let i = config.insurance_percent as u64;
        v.wrapping_add(s)
            .wrapping_add(g)
            .wrapping_add(b)
            .wrapping_add(i)
            .wrapping_mul(31)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::Address as _,
        token::{StellarAssetClient, TokenClient},
        Env,
    };

    #[test]
    fn distribute_usdc_apportions_tokens_to_recipients() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
        let payer = Address::generate(&env);
        let amount = 1_000i128;

        StellarAssetClient::new(&env, &token_contract.address()).mint(&payer, &amount);

        let spending = Address::generate(&env);
        let savings = Address::generate(&env);
        let bills = Address::generate(&env);
        let insurance = Address::generate(&env);

        let accounts = AccountGroup {
            spending: spending.clone(),
            savings: savings.clone(),
            bills: bills.clone(),
            insurance: insurance.clone(),
        };

        let distributed =
            client.distribute_usdc(&token_contract.address(), &payer, &0u64, &accounts, &amount);

        assert!(distributed);

        let token_client = TokenClient::new(&env, &token_contract.address());
        assert_eq!(token_client.balance(&spending), 500);
        assert_eq!(token_client.balance(&savings), 300);
        assert_eq!(token_client.balance(&bills), 150);
        assert_eq!(token_client.balance(&insurance), 50);
        assert_eq!(token_client.balance(&payer), 0);
    }

    #[test]
    fn split_allocations_report_categories_and_amounts() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);

        let total_amount = 2000i128;
        let allocations = client.get_split_allocations(&total_amount);

        assert_eq!(allocations.len(), 4);
        let expected_amounts = [1000, 600, 300, 100];
        let categories = [
            symbol_short!("SPENDING"),
            symbol_short!("SAVINGS"),
            symbol_short!("BILLS"),
            symbol_short!("INSURANCE"),
        ];

        for i in 0..4 {
            let allocation = allocations.get(i).unwrap();
            let idx = i as usize;
            assert_eq!(allocation.amount, expected_amounts[idx]);
            assert_eq!(allocation.category, categories[idx]);
        }
    }

    #[test]
    fn export_import_snapshot_and_audit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.initialize_split(&owner, &0u64, &50, &30, &15, &5);
        assert_eq!(client.get_nonce(&owner), 1);

        let snapshot = client.export_snapshot(&owner).unwrap();
        assert_eq!(snapshot.version, 1);
        assert_eq!(snapshot.config.spending_percent, 50);

        client.update_split(&owner, &1u64, &40, &40, &10, &10);
        let snapshot2 = client.export_snapshot(&owner).unwrap();
        assert_eq!(snapshot2.config.spending_percent, 40);

        client.import_snapshot(&owner, &2u64, &snapshot);
        let config = client.get_config().unwrap();
        assert_eq!(config.spending_percent, 50);

        let log = client.get_audit_log(&0, &10);
        assert!(log.len() >= 3);
    }

    #[test]
    #[should_panic(expected = "Invalid nonce")]
    fn replay_attack_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.initialize_split(&owner, &0u64, &50, &30, &15, &5);
        client.initialize_split(&owner, &0u64, &50, &30, &15, &5);
    }
}
