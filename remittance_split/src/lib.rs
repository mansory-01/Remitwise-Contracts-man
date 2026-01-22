#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, vec, Env, Vec};

#[contract]
pub struct RemittanceSplit;

#[contractimpl]
impl RemittanceSplit {
    /// Set or update the split percentages used to allocate remittances.
    ///
    /// # Arguments
    /// * `spending_percent` - Percent allocated to spending
    /// * `savings_percent` - Percent allocated to savings
    /// * `bills_percent` - Percent allocated to bills
    /// * `insurance_percent` - Percent allocated to insurance
    ///
    /// # Returns
    /// `true` when the inputs are valid and stored, `false` otherwise.
    pub fn initialize_split(
        env: Env,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> bool {
        let total = spending_percent + savings_percent + bills_percent + insurance_percent;

        if total != 100 {
            return false;
        }

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

        true
    }

    /// Get the current split configuration
    pub fn get_split(env: &Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&symbol_short!("SPLIT"))
            .unwrap_or_else(|| vec![&env, 50, 30, 15, 5])
    }

    /// Calculate split amounts from a total remittance amount
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        let split = Self::get_split(&env);

        let spending = (total_amount * split.get(0).unwrap() as i128) / 100;
        let savings = (total_amount * split.get(1).unwrap() as i128) / 100;
        let bills = (total_amount * split.get(2).unwrap() as i128) / 100;
        let insurance = total_amount - spending - savings - bills;

        vec![&env, spending, savings, bills, insurance]
    }

    /// Validate a percentage split for bounds and sum.
    fn is_valid_split(
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> bool {
        if spending_percent > 100
            || savings_percent > 100
            || bills_percent > 100
            || insurance_percent > 100
        {
            return false;
        }

        let total = spending_percent as u64
            + savings_percent as u64
            + bills_percent as u64
            + insurance_percent as u64;
        total == 100
    }

    /// Compute a percentage share without risking multiplication overflow.
    fn split_amount(total_amount: i128, percent: u32) -> i128 {
        let percent = percent as i128;
        let quotient = total_amount / 100;
        let remainder = total_amount % 100;

        quotient * percent + (remainder * percent) / 100
    }
}
