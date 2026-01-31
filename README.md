# RemitWise Smart Contracts

Stellar Soroban smart contracts for the RemitWise remittance platform.

## Overview

This workspace contains the core smart contracts that power RemitWise's post-remittance financial planning features:

- **remittance_split**: Automatically splits remittances into spending, savings, bills, and insurance
- **savings_goals**: Goal-based savings with target dates and locked funds
- **bill_payments**: Automated bill payment tracking and scheduling
- **insurance**: Micro-insurance policy management and premium payments
- **family_wallet**: Family member management with spending limits and permissions
- **reporting**: Cross-contract aggregation and comprehensive financial reporting

## Prerequisites

- Rust (latest stable version)
- Stellar CLI (soroban-cli)
- Cargo

## Installation

```bash
# Install Soroban CLI
cargo install --locked --version 21.0.0 soroban-cli

# Build all contracts
cargo build --release --target wasm32-unknown-unknown
```

## Contracts

### Remittance Split

Handles automatic allocation of remittance funds into different categories.

**Key Functions:**

- `initialize_split`: Set percentage allocation (spending, savings, bills, insurance)
- `get_split`: Get current split configuration
- `calculate_split`: Calculate actual amounts from total remittance

### Savings Goals

Manages goal-based savings with target dates.

**Key Functions:**

- `create_goal`: Create a new savings goal (education, medical, etc.)
- `add_to_goal`: Add funds to a goal
- `get_goal`: Get goal details
- `is_goal_completed`: Check if goal target is reached
- `archive_completed_goals`: Archive completed goals to reduce storage
- `get_archived_goals`: Query archived goals
- `restore_goal`: Restore archived goal to active storage
- `cleanup_old_archives`: Permanently delete old archives
- `get_storage_stats`: Get storage usage statistics

### Bill Payments

Tracks and manages bill payments with recurring support.

**Key Functions:**

- `create_bill`: Create a new bill (electricity, school fees, etc.)
- `pay_bill`: Mark a bill as paid and create next recurring bill if applicable
- `get_unpaid_bills`: Get all unpaid bills
- `get_total_unpaid`: Get total amount of unpaid bills
- `archive_paid_bills`: Archive paid bills to reduce storage
- `get_archived_bills`: Query archived bills
- `restore_bill`: Restore archived bill to active storage
- `bulk_cleanup_bills`: Permanently delete old archives
- `get_storage_stats`: Get storage usage statistics

### Insurance

Manages micro-insurance policies and premium payments.

**Key Functions:**

- `create_policy`: Create a new insurance policy
- `pay_premium`: Pay monthly premium
- `get_active_policies`: Get all active policies
- `get_total_monthly_premium`: Calculate total monthly premium cost
- `archive_inactive_policies`: Archive deactivated policies to reduce storage
- `get_archived_policies`: Query archived policies
- `restore_policy`: Restore archived policy to active storage
- `bulk_cleanup_policies`: Permanently delete old archives
- `get_storage_stats`: Get storage usage statistics

### Family Wallet

Manages family members, roles, and spending limits.

**Key Functions:**

- `add_member`: Add a family member with role and spending limit
- `get_member`: Get member details
- `update_spending_limit`: Update spending limit for a member
- `check_spending_limit`: Verify if spending is within limit
- `archive_old_transactions`: Archive executed transactions to reduce storage
- `get_archived_transactions`: Query archived transactions
- `cleanup_expired_pending`: Remove expired pending transactions
- `get_storage_stats`: Get storage usage statistics

### Reporting

Aggregates data from all contracts to generate comprehensive financial reports.

**Key Functions:**

- `get_financial_health_report`: Generate comprehensive financial health report
- `get_remittance_summary`: Get remittance allocation breakdown
- `get_savings_report`: Get savings progress report
- `get_bill_compliance_report`: Get bill payment compliance report
- `get_insurance_report`: Get insurance coverage report
- `calculate_health_score`: Calculate financial health score (0-100)
- `get_trend_analysis`: Compare period-over-period trends
- `store_report`: Store report for future reference
- `get_stored_report`: Retrieve previously stored report
- `archive_old_reports`: Archive old reports to reduce storage
- `get_archived_reports`: Query archived reports
- `cleanup_old_reports`: Permanently delete old archives
- `get_storage_stats`: Get storage usage statistics

## Testing

Run tests for all contracts:

```bash
cargo test
```

Run tests for a specific contract:

```bash
cd remittance_split
cargo test
```

### USDC remittance split checks (local & CI)

- `cargo test -p remittance_split` exercises the USDC distribution logic with a mocked Stellar Asset Contract (`env.register_stellar_asset_contract_v2`) and built-in auth mocking.
- The suite covers minting the payer account, splitting across spending/savings/bills/insurance, and asserting balances along with the new allocation metadata helper.
- The same command is intended for CI so it runs without manual setup; re-run locally whenever split logic changes or new USDC paths are added.

## Gas Benchmarks

See `docs/gas-optimization.md` for methodology, before/after results, and assumptions.

Run the deterministic gas benchmarks:

```bash
RUST_TEST_THREADS=1 cargo test -p bill_payments --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p savings_goals --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p insurance --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p family_wallet --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p remittance_split --test gas_bench -- --nocapture
```

## Deployment

See the [Deployment Guide](DEPLOYMENT.md) for comprehensive deployment instructions.

Quick deploy to testnet:

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/remittance_split.wasm \
  --source <your-key> \
  --network testnet
```

## Development

This is a basic MVP implementation. Future enhancements:

- Integration with Stellar Asset Contract (USDC)
- Cross-contract calls for automated allocation
- Event emissions for transaction tracking
- Multi-signature support for family wallets
- Emergency mode with priority processing

## License

MIT
