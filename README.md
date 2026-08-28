#RemitWise Smart Contracts

Stellar Soroban smart contracts for the RemitWise remittance platform.

Overview

This workspace contains the core smart contracts that power RemitWise's post-remittance financial planning features:

- **[remittance_split](remittance_split/README.md)**: Automatically splits remittances into spending, savings, bills, and insurance
- **[savings_goals](savings_goals/README.md)**: Goal-based savings with target dates and locked funds
- **[bill_payments](bill_payments/README.md)**: Automated bill payment tracking and scheduling with recurring bill schedule lifecycle
- **[insurance](insurance/README.md)**: Micro-insurance policy management and premium payments
- **[family_wallet](family_wallet/README.md)**: Family governance, multisig approvals, and emergency transfer controls
- **[orchestrator](orchestrator/README.md)**: Cross-contract coordination and execution of end-to-end remittance flows
- **[reporting](reporting/README.md)**: Financial reporting and insights
- **[emergency_killswitch](emergency_killswitch/README.md)**: Standalone emergency pause contract (global/module/function granularity) — not currently cross-called by the other contracts; see [docs/EMERGENCY_SHUTDOWN.md](docs/EMERGENCY_SHUTDOWN.md) for how this relates to each contract's own pause mechanism
- **[remitwise-common](remitwise-common/README.md)**: Shared types and utilities used across contracts
- **[docs/ERGONOMIC_HELPERS.md](docs/ERGONOMIC_HELPERS.md)**: Contributor guide to the small shared helpers used at contract boundaries
- **[docs/PERIOD_INVARIANTS.md](docs/PERIOD_INVARIANTS.md)**: Time-bound period invariants, ledger timestamp rules, and execution windows
- **[docs/TIMESTAMP_CONVENTIONS.md](docs/TIMESTAMP_CONVENTIONS.md)**: Rules of the road for time — how timestamps are represented, accessed, and compared across all contracts
- **[docs/CROSS_CONTRACT_TIME.md](docs/CROSS_CONTRACT_TIME.md)**: How ledger time is shared across contracts and what it means for cross-contract flows
- **[docs/PERIOD_KEYS.md](docs/PERIOD_KEYS.md)**: Model + consumer contract for `period_key` period identifiers and report storage
- **[docs/AMOUNT_INVARIANTS.md](docs/AMOUNT_INVARIANTS.md)**: Amount zero-handling rules across contract entrypoints
- **[docs/CROSS_CONTRACT_INVARIANTS.md](docs/CROSS_CONTRACT_INVARIANTS.md)**: Invariants that span multiple contracts — split conservation, replay protection, epoch guards, and reviewer checklist
- **[docs/SIGNING_KEYS_ENV_TAGS.md](docs/SIGNING_KEYS_ENV_TAGS.md)**: How signing keys carry environment tags (network ID, domain separators, actor epoch) to prevent cross-environment replay
- **[docs/MIGRATION_FLAGS.md](docs/MIGRATION_FLAGS.md)**: Operator runbook for migration-completion flags, replay-protection sets, and investigation-epoch write freezes
- **[docs/OPERATOR_SIGNATURE_SCOPES.md](docs/OPERATOR_SIGNATURE_SCOPES.md)**: Operator key scopes for `verify_signature`, domain separation, and verifier registry
- **[docs/ADMIN_ROTATION.md](docs/ADMIN_ROTATION.md)**: `reporting`'s two-step admin rotation — and why, despite the name, there is no time-delay ("timelock") in it today

Shared Components

remitwise-common

A common crate containing shared types, enums, constants, and utilities used across multiple contracts.

**Shared Types:**
- `Category`: Financial categories (Spending, Savings, Bills, Insurance)
- `FamilyRole`: Access control roles (Owner, Admin, Member, Viewer)
- `CoverageType`: Insurance coverage types (Health, Life, Property, Auto, Liability)
- `EventCategory` & `EventPriority`: Event logging categories and priorities
- `PauseState`: Structured pause state carrying `paused: bool` and `paused_since: Option<u64>` for analytics

**Shared Constants:**
- Pagination limits (`DEFAULT_PAGE_LIMIT`, `MAX_PAGE_LIMIT`)
- Storage TTL values (`INSTANCE_LIFETIME_THRESHOLD`, `ARCHIVE_LIFETIME_THRESHOLD`, etc.)
- Pause storage key (`STORAGE_PAUSED_AT`)
- Contract versioning (`CONTRACT_VERSION`)
- Batch operation limits (`MAX_BATCH_SIZE`)

**Shared Utilities:**
- `clamp_limit()`: Helper for pagination limit validation
- `verify_ordered_pair()`: Helper for range validation
- `RemitwiseEvents`: Standardized event emission with `emit()` and `emit_batch()` methods

- Pagination limits ("DEFAULT_PAGE_LIMIT", "MAX_PAGE_LIMIT").
- Storage TTL values ("INSTANCE_LIFETIME_THRESHOLD", "ARCHIVE_LIFETIME_THRESHOLD", etc.).
- Contract versioning ("CONTRACT_VERSION").
- Batch operation limits ("MAX_BATCH_SIZE").

Shared Utilities

- "clamp_limit()": Validates and clamps pagination limits.
- "validate_period()": Validates the logical ordering of start and end periods.
- "same_address()": Compares two Soroban "Address" values without requiring callers to clone an address solely for equality checks.
- "RemitwiseEvents": Provides standardized event emission through "emit()" and "emit_batch()".

"same_address()" Helper

The "same_address()" helper provides a consistent way to compare two Soroban "Address" values while avoiding unnecessary address cloning at call sites.

Use the helper when comparing addresses:

if same_address(&owner, &caller) {
    // Address values match.
}

The helper is intended to:

- Improve readability of address comparisons.
- Reduce accidental ".clone()" calls used only to perform equality checks.
- Provide a shared comparison pattern across contracts.
- Preserve backwards compatibility with existing public contract APIs.

The helper does not change the semantics of address equality and does not modify or normalize either address.

Shared Enums & Constants Stability Coverage

This project includes dedicated compatibility guards in "remitwise-common" to prevent breaking changes across dependent contracts.

Coverage includes:

- Invariant tests for enum discriminants and event tags.
- Ordering assumptions verified for role and coverage definitions.
- Round-trip serialization via "soroban_sdk::IntoVal" and "TryFromVal".
- Event topic and payload serialization checks for "RemitwiseEvents".
- Pagination and TTL constant stability assertions.

Running the Compatibility Coverage

cargo test -p remitwise-common

Security Notes

- Enums use "#[repr(u32)]" and are asserted in tests, reducing the risk of contract integration drift.
- Shared constants used for pagination, batch size, storage TTL, and signature timeout are protected by stability checks.
- "clamp_limit()" includes explicit tests for overflow, underflow, and boundary conditions.
- "same_address()" is covered by tests for equal and unequal address values.

CLI Tool

A custom Rust CLI is provided for interacting with the contracts without a UI.

See "cli/README.md" (cli/README.md) for usage instructions.

Additional Components

- indexer: TypeScript event indexer for off-chain querying and analytics. See "indexer/README.md" (indexer/README.md).
- analytics: On-chain analytics and reporting.
- orchestrator: Cross-contract coordination.
- reporting: Financial reporting and insights.

Entrypoint Auth Inventory

Use the helper script below to print every public contract entrypoint alongside its detected authorization model:

python3 scripts/entrypoint_auth_inventory.py
python3 scripts/entrypoint_auth_inventory.py remittance_split

This is useful for reviewers who want a quick inventory of which entrypoints are authenticated, read-only, or otherwise permissionless.

Prerequisites

- Rust (latest stable version).
- Stellar CLI ("soroban-cli").
- Cargo.

Compatibility

Tested Versions

These contracts have been developed and tested with the following versions:

- Soroban SDK: "21.0.0"
- Soroban CLI: "21.0.0"
- Rust Toolchain: "stable" with "wasm32-unknown-unknown" and "wasm32v1-none" targets.
- Protocol Version: Compatible with Stellar Protocol 20+ (Soroban Phase 1).
- Network: Testnet and Mainnet ready.

Version Compatibility Matrix

Component| Version| Status| Notes
soroban-sdk| 21.0.0| ✅ Tested| Current stable release
soroban-cli| 21.0.0| ✅ Tested| Matches SDK version
Protocol 20| -| ✅ Compatible| Soroban Phase 1 features
Protocol 21+| -| ⚠️ Untested| Compatibility should be validated before deployment

Upgrading to New Soroban Versions

When a new Soroban SDK or protocol version is released, follow these steps to validate and upgrade.

1. Review Release Notes

Check the Soroban SDK releases for:

- Breaking changes in contract APIs.
- New features or optimizations.
- Deprecated functions.
- Protocol version requirements.

2. Update Dependencies

Update the SDK version in all applicable contract "Cargo.toml" files:

[dependencies]
soroban-sdk = "X.Y.Z"

[dev-dependencies]
soroban-sdk = { version = "X.Y.Z", features = ["testutils"] }

Contracts to update include:

- "remittance_split/Cargo.toml"
- "savings_goals/Cargo.toml"
- "bill_payments/Cargo.toml"
- "insurance/Cargo.toml"
- "family_wallet/Cargo.toml"
- "data_migration/Cargo.toml"
- "reporting/Cargo.toml"
- "orchestrator/Cargo.toml"

3. Update Soroban CLI

cargo install --locked --version X.Y.Z soroban-cli

Verify the installation:

soroban version

4. Run the Full Test Suite

cargo clean
cargo test
./scripts/run_gas_benchmarks.sh

5. Validate on Testnet

Build optimized contracts:

cargo build --release --target wasm32-unknown-unknown

Deploy a contract:

soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/<contract_name>.wasm \
  --source <your-key> \
  --network testnet

Test a contract interaction:

soroban contract invoke \
  --id <contract-id> \
  --source <your-key> \
  --network testnet \
  -- <function-name> <args>

6. Check for Breaking Changes

Common breaking changes to watch for include:

- Storage API changes, including TTL and archival patterns.
- Event emission and topic structure changes.
- Authorization and signature verification changes.
- Numeric type changes involving "i128", "u128", or fixed-point math.
- Contract lifecycle and upgrade pattern changes.

7. Update Documentation

After successful validation:

- Update this compatibility section with new versions.
- Document migration steps in "DEPLOYMENT.md".
- Update code examples if APIs change.
- Regenerate contract bindings if necessary.

Known Breaking Changes

SDK 21.0.0

No known breaking changes from the previous stable version affecting these contracts.

Future Considerations

- Protocol 21+: May introduce new storage pricing or TTL requirements.
- SDK 22.0.0+: Monitor for changes to contract storage patterns, event APIs, or authorization flows.

Installation

Install the Soroban CLI:

cargo install --locked --version 21.0.0 soroban-cli

Build all contracts:

cargo build --release --target wasm32-unknown-unknown

- Ensure CLI version matches SDK version
- Verify network is running compatible protocol version
- Check for new deployment flags or requirements

### Reporting Compatibility Issues

If you encounter issues with a specific Soroban version:

1. Check existing [GitHub Issues](https://github.com/stellar/rs-soroban-sdk/issues)
2. Verify your environment matches tested versions
3. Create a minimal reproduction case
4. Report with version details and error logs

### Additional Resources

- **[UPGRADE_GUIDE.md](UPGRADE_GUIDE.md)** - Comprehensive upgrade procedures and version-specific migration guides
- **[docs/UPGRADE_RUNBOOK.md](docs/UPGRADE_RUNBOOK.md)** - Step-by-step contract upgrade runbook and rollback plan for operators
- **[docs/UPGRADE_TESTING.md](docs/UPGRADE_TESTING.md)** - Contributor procedure for loading a previous snapshot and verifying upgrade invariants
- **[docs/SETTLEMENT_WINDOWS.md](docs/SETTLEMENT_WINDOWS.md)** - Specification of invoice settlement window rules, creation acceptance bounds, overdue semantics, and late catch-up loops
- **[docs/SETTLEMENT_CURRENCY_POLICY.md](docs/SETTLEMENT_CURRENCY_POLICY.md)** - How settlement currencies are chosen, validated, and enforced across contracts
- **[docs/CURRENCY_SUPPORT_POLICY.md](docs/CURRENCY_SUPPORT_POLICY.md)** - Which currencies are whitelisted, how whitelisting works, and how to add a new currency
- **[VERSION_COMPATIBILITY.md](VERSION_COMPATIBILITY.md)** - Detailed compatibility matrix and testing status
- **[COMPATIBILITY_QUICK_REFERENCE.md](COMPATIBILITY_QUICK_REFERENCE.md)** - Quick reference for common compatibility tasks
- **[.github/SOROBAN_VERSION_CHECKLIST.md](.github/SOROBAN_VERSION_CHECKLIST.md)** - Validation checklist for new versions

## Installation

```bash
# Install Soroban CLI
cargo install --locked --version 21.0.0 soroban-cli

# Build all contracts
cargo build --release --target wasm32-unknown-unknown
```

If you have [`just`](https://github.com/casey/just) installed, `just help`
(or plain `just`) lists the available recipes -- thin wrappers around the
same cargo/make commands used above (`just check`, `just test`, `just wasm`,
`just build`, ...). It's optional; nothing here requires it.

## Examples

The workspace includes runnable examples for each contract in the `examples/` directory. These examples demonstrate basic read and write operations using the Soroban SDK test environment.

To run an example, use `cargo run --example <example_name>`:

| Contract | Example Command |
|----------|-----------------|
| Remittance Split | `cargo run --example remittance_split_example` |
| Savings Goals | `cargo run --example savings_goals_example` |
| Bill Payments | `cargo run --example bill_payments_example` |
| Insurance | `cargo run --example insurance_example` |
| Family Wallet | `cargo run --example family_wallet_example` |
| Reporting | `cargo run --example reporting_example` |
| Orchestrator | `cargo run --example orchestrator_example` |

> [!NOTE]
> These examples run in a mocked environment and do not require a connection to a Stellar network.

## Documentation

- [Contributor Overview](docs/CONTRIBUTOR_OVERVIEW.md) - Onboarding guide for new contributors
- [ADR: Ban unwrap in Release Builds](docs/adr-ban-unwrap-in-release.md) - Why unwrap and panic are forbidden in production contract code
- [Changelog](CHANGELOG.md) - Conventional-commits-style log of every release
- [Governance Parameters](docs/GOVERNANCE_PARAMS.md) - Reference table of every configurable parameter across contracts with min/max/default values and governance mechanisms
- [Token Decimal Catalogue](docs/DECIMAL_CATALOGUE.md) - Reference table of decimals expected for each canonical token
- [Authorization Matrix](docs/AUTHORIZATION_MATRIX.md) - Per-entrypoint caller authorization requirements for all contracts
- [Pagination Handbook](docs/PAGINATION_HANDBOOK.md) - How every paginated read is structured, cursor semantics, reviewer checklist, and implementation guide
- [Pause Playbook](docs/PAUSE_PLAYBOOK.md) - Emergency pause mechanisms and recovery procedures for operators
- [Committed Hashes](docs/COMMITTED_HASHES.md) - Request-hash coverage and verification guidance for downstream integrators
- [Zero-Amount Policy](docs/ZERO_AMOUNT_POLICY.md) - Which entrypoints reject, accept, or normalize zero amounts; quick reference for integrators
- [Entrypoint N Caps](docs/ENTRYPOINT_N_CAPS.md) - Per-entrypoint record-count upper bounds (N caps), error codes, slot-release rules, and reviewer checklist
- [Family Wallet Design (as implemented)](docs/family-wallet-design.md)
- [Reporting Admin Rotation](docs/reporting-admin-rotation.md) - Two-step upgrade-admin handoff procedure for reporting dependency configuration
- [Event Indexing Guide](docs/INDEXING.md) - Mapping contract events to off-chain tables
- [Financial Health Score Model](docs/HEALTH_SCORE.md) - HealthScore component weights, inputs, clamping, and worked examples
- [Model Overview](docs/MODEL.md) - Overview of the financial health score model
- [Frontend Integration Notes](docs/frontend-integration.md)
- [String and Bytes Canonicalisation](docs/CANONICALISATION.md) - Tag casefold, currency trim/uppercase, external-ref charset, and migration checksum byte-order
- [Type-Safe Percent Conversion](docs/type-safe-percent-conversion.md) - Converting whole percentages to basis points with checked overflow arithmetic
- [Configuration Schema Versioning](docs/CONFIG_SCHEMA_VERSIONING.md) - Protocols for bumping configuration schema versions with backward compatibility
- [Storage Layout Reference](STORAGE_LAYOUT.md)
- [Storage Key Naming Conventions](docs/storage-key-naming-conventions.md) - Naming rules for storage keys, plus the CI checks that enforce them
- [Reserved Storage Keys](docs/RESERVED_STORAGE_KEYS.md) - Storage-key prefixes set aside for roadmap features; contributors must not reuse them
- [Audit Event Fields](docs/audit-event-fields.md) - On-chain audit log fields and patterns
- [Event Indexer](indexer/README.md) - Off-chain event indexing and querying
- [Audit Trail](docs/AUDIT_TRAIL.md) - How to reconstruct historical state from events alone
- [Settler Whitelist](docs/SETTLER_WHITELIST.md) - Operator guide: how settlers are added, rotated, and revoked
- [Epoch Model](docs/EPOCH_MODEL.md) - How epoch counters bump, what they invalidate, and the stale-authorization replay threat they mitigate in the emergency killswitch and orchestrator contracts
- [Dispute Epoch Model](docs/DISPUTE_EPOCH_MODEL.md) - Semantics + when dispute epochs bump
- [Killswitch Trust Model](docs/killswitch-trust-model.md) - Who can trigger, who can clear, what state is preserved in the emergency killswitch
- [Killswitch Pause/Unpause State Machine](docs/killswitch-pause-state-machine.md) - The global Unpaused/Paused/PendingUnpause states and transitions, plus how module- and function-level pauses layer on top
- [Kill-Switch Recovery Runbook](docs/KILL_SWITCH_RECOVERY.md) - Operator runbook: how to engage the kill switch, verify the freeze, and safely resume operations after an incident
- [Ledger Monotonicity](docs/LEDGER_MONOTONICITY.md) - Where and why contract code relies on ledger sequence and timestamp monotonicity
- [Timestamp Conventions](docs/TIMESTAMP_CONVENTIONS.md) - How timestamps are represented, stored, and compared across all contracts
- [Tagging Feature](TAGGING_FEATURE.md) - Tag-based organization system
- [Threat Model](THREAT_MODEL.md) - Security analysis and mitigations
- [Entrypoint Threat Breakdown](docs/THREAT_MODEL.md) - STRIDE-style threat analysis per contract entrypoint (contributor-focused)
- [Security Review Summary](SECURITY_REVIEW_SUMMARY.md)
- [Signature Domains](docs/SIGNATURE_DOMAINS.md) - Central registry of domain separation strings used for signature verification and hash preimages
- [Event Versioning ADR](docs/events-versioning.md) - Why contract events are versioned via a `_v2` suffix
- [Event Versioning Discipline](docs/EVENT_VERSIONING.md) - Backward-compatibility rules, migration steps, and indexer guidelines for event schema changes
- [Cross-Contract Epochs](docs/CROSS_CONTRACT_EPOCHS.md) - Actor-epoch semantics, the cross-contract coordination protocol, and the `EpochMismatch` guard
- [Observability Model](docs/OBSERVABILITY_MODEL.md) - Per-contract event catalogue: what each contract emits and what off-chain consumers rely on

## Contracts

### Remittance Split

Handles automatic allocation of remittance funds into different categories.

Key Functions

- "initialize_split": Set percentage allocation for spending, savings, bills, and insurance.
- "get_split": Get the current split configuration.
- "calculate_split": Calculate actual amounts from a total remittance.

Events

- "SplitInitializedEvent": Emitted when split configuration is initialized.
  
  - "spending_percent"
  - "savings_percent"
  - "bills_percent"
  - "insurance_percent"
  - "timestamp"

- "SplitCalculatedEvent": Emitted when split amounts are calculated.
  
  - "total_amount"
  - "spending_amount"
  - "savings_amount"
  - "bills_amount"
  - "insurance_amount"
  - "timestamp"

Savings Goals

Manages goal-based savings with target dates.

Key Functions

- "create_goal": Create a new savings goal.
- "add_to_goal": Add funds to a goal.
- "get_goal": Get goal details.
- "is_goal_completed": Check whether a goal target has been reached.
- "archive_completed_goals": Archive completed goals to reduce storage.
- "get_archived_goals": Query archived goals.
- "restore_goal": Restore an archived goal.
- "cleanup_old_archives": Permanently delete old archives.
- "get_storage_stats": Get storage usage statistics.

Events

- "GoalCreatedEvent": Emitted when a new savings goal is created.
  
  - "goal_id"
  - "name"
  - "target_amount"
  - "target_date"
  - "timestamp"

- "FundsAddedEvent": Emitted when funds are added to a goal.
  
  - "goal_id"
  - "amount"
  - "new_total"
  - "timestamp"

- "GoalCompletedEvent": Emitted when a goal reaches its target amount.
  
  - "goal_id"
  - "name"
  - "final_amount"
  - "timestamp"

Bill Payments

Tracks and manages bill payments with recurring support.

Key Functions

- "create_bill": Create a new bill with an optional "external_ref".
- "pay_bill": Mark a bill as paid and create the next recurring bill when applicable.
- "set_external_ref": Owner-only update or clear operation for a bill's "external_ref".
- "get_unpaid_bills": Get all unpaid bills.
- "get_total_unpaid": Get the total amount of unpaid bills.
- "archive_paid_bills": Archive paid bills to reduce storage.
- "get_archived_bills": Query archived bills.
- "restore_bill": Restore an archived bill.
- "bulk_cleanup_bills": Permanently delete old archives.
- "get_storage_stats": Get storage usage statistics.
- "create_bill_schedule": Create a recurring or one-off bill schedule.
- "modify_bill_schedule": Modify an existing bill schedule.
- "cancel_bill_schedule": Cancel an active bill schedule.
- "execute_due_bill_schedules": Execute all due bill schedules.
- "get_bill_schedules": Get all schedules for an owner.
- "get_bill_schedule": Get a specific schedule by ID.

Events

- "BillCreatedEvent": Emitted when a new bill is created.
  
  - "bill_id"
  - "name"
  - "amount"
  - "due_date"
  - "recurring"
  - "external_ref"
  - "timestamp"

- "BillPaidEvent": Emitted when a bill is marked as paid.
  
  - "bill_id"
  - "name"
  - "amount"
  - "timestamp"

- "RecurringBillCreatedEvent": Emitted when a recurring bill generates the next bill.
  
  - "bill_id"
  - "parent_bill_id"
  - "name"
  - "amount"
  - "due_date"
  - "timestamp"

- "ScheduleCreatedEvent": Emitted when a bill schedule is created.
  
  - "schedule_id"
  - "owner"

- "ScheduleExecutedEvent": Emitted when a bill schedule is executed.
  
  - "schedule_id"

- "ScheduleModifiedEvent": Emitted when a bill schedule is modified.
  
  - "schedule_id"

- "ScheduleCancelledEvent": Emitted when a bill schedule is cancelled.
  
  - "schedule_id"

- "ScheduleMissedEvent": Emitted when a recurring schedule skips intervals.
  
  - "schedule_id"
  - "missed"

Insurance

Manages micro-insurance policies and premium payments.

Key Functions

- "create_policy": Create a new insurance policy with an optional "external_ref".
- "pay_premium": Pay a monthly premium.
- "set_external_ref": Owner-only update or clear operation for a policy's "external_ref".
- "get_active_policies": Get all active policies.
- "get_total_monthly_premium": Calculate total monthly premium cost.
- "deactivate_policy": Deactivate an insurance policy.

Bill and insurance events include "external_ref" where applicable for off-chain linking.

Events

- "PolicyCreatedEvent": Emitted when a new insurance policy is created.
  
  - "policy_id"
  - "name"
  - "coverage_type"
  - "monthly_premium"
  - "coverage_amount"
  - "external_ref"
  - "timestamp"

- "PremiumPaidEvent": Emitted when a premium is paid.
  
  - "policy_id"
  - "name"
  - "amount"
  - "next_payment_date"
  - "timestamp"

- "PolicyDeactivatedEvent": Emitted when a policy is deactivated.
  
  - "policy_id"
  - "name"
  - "timestamp"

Family Wallet

Manages family roles, spending controls, multisig approvals, and emergency transfer policies.

Key Functions

- "init": Initialize the owner, members, default multisig configuration, and emergency settings.
- "add_member": Add a family member and assign a role.
- "update_spending_limit": Update a member's spending limit.
- "configure_multisig": Configure multisig approval requirements.
- "propose_transaction": Propose a transaction requiring multisig approval.
- "sign_transaction": Sign a pending multisig transaction.
- "withdraw": Execute a direct or multisig-gated withdrawal.
- "configure_emergency": Configure emergency transfer settings.
- "set_emergency_mode": Enable or disable emergency mode.
- "propose_emergency_transfer": Propose an emergency transfer.
- "pause": Pause wallet operations.
- "unpause": Resume wallet operations.
- "set_pause_admin": Set the pause administrator.
- "archive_old_transactions": Archive old transaction records.
- "cleanup_expired_pending": Remove expired pending transactions.
- "get_storage_stats": Get storage usage statistics.

For full design details, see "docs/family-wallet-design.md" (docs/family-wallet-design.md).

Reporting

Provides summarized views of user financial data across the RemitWise ecosystem.

Key Features

- "get_remittance_summary": Aggregates split, savings, bills, and insurance data into a single summary.
- Graceful Degradation: Uses a "DataAvailability" indicator ("Complete", "Partial", or "Missing") so summary queries can return available data without hard failure when upstream contracts are unresponsive, unconfigured, or unavailable.

Orchestrator

Coordinates cross-contract operations and end-to-end remittance flows.

See "orchestrator/README.md" (orchestrator/README.md) for detailed functionality and integration guidance.

Emergency Killswitch

Provides centralized emergency pause controls across supported contracts.

See "emergency_killswitch/README.md" (emergency_killswitch/README.md) for detailed functionality and operational procedures.

Events

All contracts emit events for important state changes, enabling real-time tracking and frontend integration.

Events follow Soroban best practices and include:

- Relevant IDs: Events include the ID of the entity being acted upon where applicable.
- Amounts: Financial events include transaction amounts where applicable.
- Timestamps: Events include the ledger timestamp where applicable.
- Context Data: Additional contextual information such as names, dates, and references.

Event Topics

Each contract uses short symbol topics for efficient event identification:

- Remittance Split: "init", "calc"
- Savings Goals: "created", "added", "completed"
- Bill Payments: "created", "paid", "recurring"
- Insurance: "created", "paid", "deactive"
- Family Wallet: "added", "member", "updated", "limit", "emerg/*", "wallet/*"

Querying Events

Events can be queried from the Stellar network using the Soroban SDK or supported RPC/event-indexing infrastructure.

Each event structure is exported according to the relevant contract interface and can be decoded using the contract schema.

Testing

Run tests for all contracts:

cargo test

Issue #1141: "same_address()" Helper

The "same_address()" helper is covered by unit tests to ensure:

- Two identical addresses return "true".
- Two different addresses return "false".
- Address comparison does not require unnecessary caller-side cloning.
- Existing contract behaviour remains unchanged.

Run the shared utility tests:

```bash
python3 scripts/verify_cross_contract_invariants.py
```

See [scripts/README_INVARIANT_TESTS.md](scripts/README_INVARIANT_TESTS.md) for details.

### USDC remittance split checks (local & CI)

- `cargo test -p remittance_split` exercises the USDC distribution logic with a mocked Stellar Asset Contract (`env.register_stellar_asset_contract_v2`) and built-in auth mocking.
- The suite covers minting the payer account, splitting across spending/savings/bills/insurance, and asserting balances along with the new allocation metadata helper.
- The same command is intended for CI so it runs without manual setup; re-run locally whenever split logic changes or new USDC paths are added.

### Orchestrator audit log pagination correctness

The orchestrator audit API (`get_audit_log(from_index, limit)`) supports cursor-based pagination for compliance and monitoring clients.

**Pagination guarantees:**
- Results are ordered from oldest to newest in the current bounded audit window.
- `from_index` is a stable zero-based cursor within that bounded window.
- `limit` is clamped to contract maximum capacity (`MAX_AUDIT_ENTRIES`) for predictable gas and memory usage.
- Page-end calculation uses saturating arithmetic to prevent cursor overflow edge cases.
- Out-of-range cursors return an empty page (safe default).

**Security assumptions and notes:**
- Consumers should treat the cursor as a position in the current rotated window, not an immutable global ID.
- Log rotation drops oldest records at capacity, so clients should read promptly and persist externally if long-term retention is required.
- Tests assert no duplicate entries across sequential pages under heavy execution history and rotation pressure.

Run orchestrator tests (including pagination correctness coverage):

```bash
cargo test -p orchestrator
```

## Gas Benchmarks

RemitWise includes a comprehensive gas benchmarking harness for tracking and optimizing contract performance.

### Quick Start

Run all benchmarks and generate a JSON report:

```bash
./scripts/run_gas_benchmarks.sh
```

This creates `gas_results.json` with CPU and memory costs for all contract operations.

### Regression Detection

Compare current results against baseline to detect performance regressions:

```bash
./scripts/compare_gas_results.sh benchmarks/baseline.json gas_results.json
```

The comparison fails if CPU or memory increases exceed configured thresholds (default 10%).

### Update Baseline

After verifying optimizations:

```bash
./scripts/update_baseline.sh
```

### Documentation

- **[Benchmarking Guide](benchmarks/README.md)**: Complete benchmarking documentation
- **[Gas Unit Costs Reference](docs/GAS_UNIT_COSTS.md)**: Per-operation CPU/memory costs, fee formulas, and current network parameters
- **[Gas Tuning Guide](docs/GAS_TUNING.md)**: How to interpret gas snapshots and optimize costs
- **[Gas Optimization Guide](docs/gas-optimization.md)**: Optimization strategies and best practices
- **[Baseline Results](benchmarks/baseline.json)**: Current performance baseline
- **[Threshold Configuration](benchmarks/thresholds.json)**: Regression detection thresholds

### CI Integration

Gas benchmarks run automatically in CI on every push and pull request. Results are:

- Compared against baseline for regression detection
- Uploaded as artifacts (retained for 30 days)
- Posted as PR comments with comparison details

To view CI results:

1. Go to Actions tab in GitHub
2. Select a workflow run
3. Download the `gas-benchmarks` artifact
4. View `gas_results.json` for metrics

### Individual Contract Benchmarks

Run benchmarks for a specific contract:

```bash
RUST_TEST_THREADS=1 cargo test -p bill_payments --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p savings_goals --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p insurance --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p family_wallet --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p remittance_split --test gas_bench -- --nocapture
```

## WASM Size Delta

Every `check_ci.sh` run prints a per-contract WASM size delta against a committed baseline, so a size regression is visible in the build output the moment it happens rather than at deploy time.

### Quick Start

Build the contracts, then print the delta table:

```bash
cargo build --release --target wasm32-unknown-unknown
./scripts/wasm_size_delta.sh
```

This compares each contract's current `.wasm` byte size (via `scripts/collect_wasm_sizes.sh`) against `benchmarks/wasm_size_baseline.json` and prints a table with the baseline, current, absolute delta, and percentage change per contract, plus a `TOTAL` row.

This is a DX surface, not a gate — unlike gas-benchmark regression checks, it always exits `0` and never fails a build on its own.

### Update Baseline

After a deliberate size change, refresh the baseline in the same PR:

```bash
./scripts/wasm_size_delta.sh --update
```

## Deployment

### Automated Bootstrap Deployment

The fastest way to deploy all contracts with sensible defaults:

```bash
# Deploy to testnet with default settings
./scripts/bootstrap_deploy.sh testnet deployer

# Skip building if contracts are already built
SKIP_BUILD=1 ./scripts/bootstrap_deploy.sh testnet deployer

# Deploy to mainnet
./scripts/bootstrap_deploy.sh mainnet deployer

# Custom output file location
OUTPUT_FILE=./my-contracts.json ./scripts/bootstrap_deploy.sh testnet deployer
```

The bootstrap script will:
1. Build all WASM artifacts (unless SKIP_BUILD=1)
2. Deploy each contract via soroban-cli
3. Initialize contracts with sensible defaults:
   - Remittance split: 50% spending, 30% savings, 15% bills, 5% insurance
   - One example savings goal
   - One example bill
   - One example insurance policy
4. Output contract IDs in a JSON file (default: `deployed-contracts.json`)

The generated JSON file can be easily consumed by frontend/backend:

```json
{
  "network": "testnet",
  "deployer": "GXXXXXXX...",
  "deployed_at": "2024-01-15T10:30:00Z",
  "contracts": {
    "remittance_split": "CXXXXXXX...",
    "savings_goals": "CXXXXXXX...",
    "bill_payments": "CXXXXXXX...",
    "insurance": "CXXXXXXX...",
    "family_wallet": "CXXXXXXX...",
    "reporting": "CXXXXXXX...",
    "orchestrator": "CXXXXXXX..."
  }
}
```

### Manual Deployment

See the [Deployment Guide](DEPLOYMENT.md) for comprehensive manual deployment instructions.

Quick deploy to testnet:

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/remittance_split.wasm \
  --source <your-key> \
  --network testnet
```

## Data Migration

The `data_migration` payload library provides format-agnostic snapshot export/import capabilities across all contracts.

### Import Replay Protection

When importing payloads, the contracts use a `MigrationTracker` to prevent duplicate imports and replay attacks.
- Payload Identity is cryptographically bound to a `(checksum, version)` tuple.
- The state tracker persists this tuple alongside the ingestion `timestamp_ms` to outright reject replayed or copied payloads using `MigrationError::DuplicateImport`.
- Restores are safely guarded against double spending or overriding the active state multiple times.

## Operational Limits

ID and record-count operating limits (including `u32` overflow analysis and monitoring alerts) are documented in the **Operational Limits and Monitoring** section of [ARCHITECTURE.md](ARCHITECTURE.md).

## Development

This is a basic MVP implementation. Future enhancements:

- Integration with Stellar Asset Contract (USDC)
- Cross-contract calls for automated allocation
- Multi-signature support for family wallets
- Emergency mode with priority processing

## Security

### Threat Model

A comprehensive security review and threat model is available in [THREAT_MODEL.md](THREAT_MODEL.md). This document identifies:

Feature Flag Consistency

The CI pipeline includes a feature flag consistency check:

python3 scripts/check_features.py

This verifies that every "cfg(feature = "...")" reference in Rust source code has a corresponding entry in the crate's "[features]" section.

Encrypted Migration Payload Decode Safety

The "data_migration" crate supports an opaque encrypted payload transport format for off-chain migration tooling.

Format

enc:v1:<base64>

Where:

- "enc:v1:" is a strict marker prefix.
- "<base64>" is the base64-encoded ciphertext blob.

Security 
