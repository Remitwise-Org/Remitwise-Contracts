# Reporting Contract

Aggregates financial health data from the remittance_split, savings_goals, bill_payments, and insurance contracts into structured reports. Supports on-chain report storage, archival, and admin-controlled cleanup.

## Features

- Generate financial health reports (health score, remittance summary, savings, bills, insurance)
- Store and retrieve reports per `(user, period_key)`
- Admin-only archival and cleanup of old reports
- Storage TTL management (instance: ~30 days, archive: ~180 days)

<<<<<<< feature/reporting-address-config-integrity
## Dependency contract address integrity

Reporting stores five downstream contract IDs (`remittance_split`, `savings_goals`,
`bill_payments`, `insurance`, `family_wallet`) set via `configure_addresses`.

**Validation (on every `configure_addresses` call)**:

- **No self-reference** — None of the five addresses may equal the reporting
  contract’s own address. Pointing a role at this contract would create ambiguous
  cross-contract calls and break the intended “one deployment per role” model.
- **Pairwise uniqueness** — All five values must differ. Two roles must not share
  the same contract ID, or aggregation would silently read the wrong deployment
  twice (audit and correctness risk).

**`verify_dependency_address_set`** exposes the same checks without writing
storage and without requiring authorization. Use it from admin UIs or scripts to
pre-validate a bundle before submitting a configuration transaction.

**Error**: `InvalidDependencyAddressConfiguration` (`6`) when the proposed set
is rejected.

**Security notes**:

- Validation is **O(1)** (fixed five slots, constant comparisons).
- This does **not** prove each address is the *correct* Remitwise deployment for
  its role (that requires off-chain governance / deployment manifests). It only
  enforces **structural** integrity: distinct callees and no reporting
  self-loop.
- Soroban/Stellar contract IDs are not an EVM-style “zero address”; “malformed”
  in this layer means duplicate or self-reference as above.
=======
## Quickstart
>>>>>>> main

```rust
// 1. Initialize
client.init(&admin);

// 2. Configure sub-contract addresses (admin only)
client.configure_addresses(&admin, &remittance_split, &savings_goals, &bill_payments, &insurance, &family_wallet);

// 3. Generate a report
let report = client.get_financial_health_report(&user, &total_remittance, &period_start, &period_end);

// 4. Store it (user must authorize)
client.store_report(&user, &report, &202401u64);

// 5. Retrieve it
let stored = client.get_stored_report(&user, &202401u64);
```

## API Reference

### Initialization

#### `init(admin: Address) -> Result<(), ReportingError>`
Initializes the contract. Can only be called once.

- Errors: `AlreadyInitialized`

#### `configure_addresses(caller, remittance_split, savings_goals, bill_payments, insurance, family_wallet) -> Result<(), ReportingError>`
Sets sub-contract addresses. Admin only.

- Errors: `NotInitialized`, `Unauthorized`

### Report Generation

#### `get_financial_health_report(user, total_remittance, period_start, period_end) -> FinancialHealthReport`
Generates a full report by querying all sub-contracts.

#### `get_remittance_summary(user, total_amount, period_start, period_end) -> RemittanceSummary`
#### `get_savings_report(user, period_start, period_end) -> SavingsReport`
#### `get_bill_compliance_report(user, period_start, period_end) -> BillComplianceReport`
#### `get_insurance_report(user, period_start, period_end) -> InsuranceReport`
#### `calculate_health_score(user, total_remittance) -> HealthScore`
#### `get_trend_analysis(user, current_amount, previous_amount) -> TrendData`

### Storage

#### `store_report(user: Address, report: FinancialHealthReport, period_key: u64) -> bool`
Stores a report under `(user, period_key)`. Requires `user` authorization.

#### `get_stored_report(user: Address, period_key: u64) -> Option<FinancialHealthReport>`
Retrieves a stored report. Returns `None` if not found.

#### `get_addresses() -> Option<ContractAddresses>`
#### `get_admin() -> Option<Address>`
#### `get_storage_stats() -> StorageStats`

### Admin Maintenance

#### `archive_old_reports(caller: Address, before_timestamp: u64) -> u32`
Moves reports generated before `before_timestamp` to archive storage. Admin only.

#### `get_archived_reports(user: Address) -> Vec<ArchivedReport>`
Returns archived reports for a specific user.

#### `cleanup_old_reports(caller: Address, before_timestamp: u64) -> u32`
Permanently deletes archives created before `before_timestamp`. Admin only.

## Authorization Model

| Operation | Who can call |
|---|---|
| `init` | Anyone (once) |
| `configure_addresses` | Admin only |
| `store_report` | The report owner (`user.require_auth()`) |
| `get_stored_report` | Anyone (key-isolated by `(user, period_key)`) |
| `archive_old_reports` | Admin only |
| `cleanup_old_reports` | Admin only |
| `get_archived_reports` | Anyone (filtered by user address) |

## Security Notes

- `store_report` calls `user.require_auth()` — a caller cannot store a report under another user's address without that user's signature.
- `get_stored_report` uses a composite key `(Address, u64)` — user A querying user B's period key returns `None` because the keys are distinct.
- `get_archived_reports` filters by address server-side — user A cannot see user B's archived reports.
- `archive_old_reports` and `cleanup_old_reports` panic with a clear message if called by a non-admin, and both call `caller.require_auth()` first.
- Double-initialization is prevented: `init` returns `AlreadyInitialized` on a second call.

## Running Tests

```bash
cargo test -p reporting
```

Test coverage includes:

- Contract initialization and double-init rejection
- `configure_addresses` admin-only enforcement
- `store_report` owner auth recording and user isolation
- `get_stored_report` key isolation across users and periods
- `archive_old_reports` admin-only enforcement (non-admin panics)
- `cleanup_old_reports` admin-only enforcement (non-admin panics)
- `get_archived_reports` per-user filtering
- Multi-user full lifecycle with no data leakage
- Timestamp boundary conditions for archival
- Storage TTL extension on all state-changing operations
