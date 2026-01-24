#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Vec,
};

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

/// Bill data structure with owner tracking for access control
#[derive(Clone)]
#[contracttype]
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub amount: i128,
    pub due_date: u64,
    pub recurring: bool,
    pub frequency_days: u32,
    pub paid: bool,
}

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone)]
pub enum BillEvent {
    Created,
    Paid,
}

#[contract]
pub struct BillPayments;

#[contractimpl]
impl BillPayments {
    /// Create a new bill
    ///
    /// # Arguments
    /// * `owner` - Address of the bill owner (must authorize)
    /// * `name` - Name of the bill (e.g., "Electricity", "School Fees")
    /// * `amount` - Amount to pay (must be positive)
    /// * `due_date` - Due date as Unix timestamp
    /// * `recurring` - Whether this is a recurring bill
    /// * `frequency_days` - Frequency in days for recurring bills (must be > 0 if recurring)
    ///
    /// # Returns
    /// The ID of the created bill
    ///
    /// # Panics
    /// - If owner doesn't authorize the transaction
    /// - If amount is not positive
    /// - If recurring is true and frequency_days is 0
    pub fn create_bill(
        env: Env,
        owner: Address,
        name: String,
        amount: i128,
        due_date: u64,
        recurring: bool,
        frequency_days: u32,
    ) -> u32 {
        // Access control: require owner authorization
        owner.require_auth();

        // Input validation
        if amount <= 0 {
            panic!("Amount must be positive");
        }
        if recurring && frequency_days == 0 {
            panic!("Frequency days must be greater than 0 for recurring bills");
        }

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
            + 1;

        let bill = Bill {
            id: next_id,
            owner: owner.clone(),
            name: name.clone(),
            amount,
            due_date,
            recurring,
            frequency_days,
            paid: false,
        };

        bills.set(next_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("bill"), BillEvent::Created),
            (next_id, owner),
        );

        next_id
    }

    /// Mark a bill as paid
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the bill owner)
    /// * `bill_id` - ID of the bill
    ///
    /// # Returns
    /// True if payment was successful
    ///
    /// # Panics
    /// - If caller is not the bill owner
    /// - If bill is not found
    /// - If bill is already paid
    pub fn pay_bill(env: Env, caller: Address, bill_id: u32) -> bool {
        // Access control: require caller authorization
        caller.require_auth();

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut bill = bills.get(bill_id).expect("Bill not found");

        // Access control: verify caller is the owner
        if bill.owner != caller {
            panic!("Only the bill owner can pay this bill");
        }

        if bill.paid {
            panic!("Bill is already paid");
        }

        bill.paid = true;

        // If recurring, create next bill
        if bill.recurring {
            let next_due_date = bill.due_date + (bill.frequency_days as u64 * 86400);
            let next_id = env
                .storage()
                .instance()
                .get(&symbol_short!("NEXT_ID"))
                .unwrap_or(0u32)
                + 1;

            let next_bill = Bill {
                id: next_id,
                owner: bill.owner.clone(),
                name: bill.name.clone(),
                amount: bill.amount,
                due_date: next_due_date,
                recurring: true,
                frequency_days: bill.frequency_days,
                paid: false,
            };

            bills.set(next_id, next_bill);
            env.storage()
                .instance()
                .set(&symbol_short!("NEXT_ID"), &next_id);
        }

        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);

        // Emit event for audit trail
        env.events()
            .publish((symbol_short!("bill"), BillEvent::Paid), (bill_id, caller));

        true
    }

    /// Get a bill by ID
    ///
    /// # Arguments
    /// * `bill_id` - ID of the bill
    ///
    /// # Returns
    /// Bill struct or None if not found
    pub fn get_bill(env: Env, bill_id: u32) -> Option<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        bills.get(bill_id)
    }

    /// Get all unpaid bills for a specific owner
    ///
    /// # Arguments
    /// * `owner` - Address of the bill owner
    ///
    /// # Returns
    /// Vec of unpaid Bill structs belonging to the owner
    pub fn get_unpaid_bills(env: Env, owner: Address) -> Vec<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        let max_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);

        for i in 1..=max_id {
            if let Some(bill) = bills.get(i) {
                if !bill.paid && bill.owner == owner {
                    result.push_back(bill);
                }
            }
        }
        result
    }

    /// Get total amount of unpaid bills for a specific owner
    ///
    /// # Arguments
    /// * `owner` - Address of the bill owner
    ///
    /// # Returns
    /// Total amount of all unpaid bills belonging to the owner
    pub fn get_total_unpaid(env: Env, owner: Address) -> i128 {
        let unpaid = Self::get_unpaid_bills(env, owner);
        let mut total = 0i128;
        for bill in unpaid.iter() {
            total += bill.amount;
        }
        total
    }

    /// Extend the TTL of instance storage
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}
