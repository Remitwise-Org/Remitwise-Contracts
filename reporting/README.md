# Reporting Contract

Aggregates financial health data from the remittance_split, savings_goals, bill_payments, and insurance contracts into structured reports. Supports on-chain report storage, archival, and admin-controlled cleanup.

## Features

- Generate financial health reports (health score, remittance summary, savings, bills, insurance)
- Store and retrieve reports per `(user, period_key)`
- Admin-only archival and cleanup of old reports
- Storage TTL management (instance: ~30 days, archive: ~180 days)

## Financial Health Score

> **Authoritative spec:** [`docs/HEALTH_SCORE.md`](../docs/HEALTH_SCORE.md) documents
> the exact component weights, the input each consumes, the clamping to `0..=100`,
> the `DataAvailability` (Partial/Missing) behavior, and worked examples.

The contract calculates a comprehensive financial health score (0-100) based on three components:

### Score Components

- **Savings Score (0-40 points)**: Based on savings goal completion percentage
- **Bills Score (0-40 points)**: Based on bill payment compliance
- **Insurance Score (0-20 points)**: Based on active insurance coverage

### Arithmetic Safety & Normalization

The health score calculation implements hardened arithmetic to ensure security and predictability:

#### Overflow Protection
- Uses saturating arithmetic for amount summations
- Safe division prevents intermediate overflow in percentage calculations
- Individual amounts are clamped to reasonable bounds

#### Bounds Guarantees
- Overall score is always bounded [0, 100]
- Component scores never exceed their maximum values
- Progress percentages are clamped [0, 100]

#### Edge Case Handling
- Zero savings targets result in default score (20 points)
- Negative amounts are clamped to zero
- Extreme input values don't cause calculation failures

#### Security Properties
- Deterministic output for identical inputs
- No external dependencies on ledger state
- Cross-contract calls use configured addresses only

### Example Calculation

For a user with:
- 80% savings goal completion → 32 savings points
- Unpaid bills (none overdue) → 35 bills points  
- Active insurance policy → 20 insurance points

**Total Score**: 32 + 35 + 20 = 87

Reporting stores five downstream contract IDs (`remittance_split`, `savings_goals`,
`bill_payments`, `insurance`, `family_wallet`) set via `configure_addresses`.

**Validation (on every `configure_addresses` call)**:

- **No self-reference** — None of the five addresses may equal the reporting
  contract's own address. Pointing a role at this contract would create ambiguous
  cross-contract calls and break the intended "one deployment per role" model.
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
- Soroban/Stellar contract IDs are not an EVM-style "zero address"; "malformed"
  in this layer means duplicate or self-reference as above.

## Quickstart

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

#### `get_financial_health_report(user, total_remittance, period_start, period_end) -> Result<FinancialHealthReport, ReportingError>`
Generates a full report by querying all sub-contracts.

#### `get_remittance_summary(user, total_amount, period_start, period_end) -> Result<RemittanceSummary, ReportingError>`
#### `get_savings_report(user, period_start, period_end) -> Result<SavingsReport, ReportingError>`
#### `get_bill_compliance_report(user, period_start, period_end) -> Result<BillComplianceReport, ReportingError>`
#### `get_insurance_report(user, period_start, period_end) -> Result<InsuranceReport, ReportingError>`
#### `calculate_health_score(user, total_remittance) -> HealthScore`
#### `get_trend_analysis(user, current_amount, previous_amount) -> TrendData`
#### `get_trend_analysis_multi(user, history) -> Vec<TrendData>`

`get_trend_analysis_multi` walks the supplied `(timestamp, amount)` history in
input order. Callers should sort by timestamp ascending before calling when they
need chronological trends; the contract does not sort or reject unsorted input.

The first history point is compared against a zero baseline, so a single-point
history returns one trend entry with `previous_amount = 0`. Empty history returns
an empty vector. For positive previous amounts, `change_percentage` is
`(current - previous) * 100 / previous`, clamped to `i32`; decreases from a
positive baseline are negative. When the previous amount is zero or negative,
the percentage is `100` if the current amount is positive and `0` otherwise.
Trend deltas use checked arithmetic and saturate at the `i128` bounds on
overflow.

All report generation endpoints validate the period bounds and fail closed with
`InvalidPeriod` when `period_start > period_end`.

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

#### `get_archived_reports_page(user: Address, cursor: u32, limit: u32) -> ArchivedPage`
Returns a paginated slice of archived reports for a specific user. **This is the supported entrypoint for archive reads.**

- `cursor` — Starting index in the user's archived list (`0` for the first page).
- `limit` — Maximum items to return in the page. `0` is normalized to `DEFAULT_PAGE_LIMIT` (20); values above `MAX_PAGE_LIMIT` (50) are clamped.
- Returns [`ArchivedPage`]:
  - `items`       — Up to `limit` `ArchivedReport` entries.
  - `next_cursor` — `0` when there are no more pages (canonical terminator). Otherwise, the index of the next page's first item.
  - `count`       — Total number of archived reports for `user`. Unaffected by `cursor` or `limit`.

The cursor **always terminates**: out-of-range cursors (`cursor >= count`) and empty archives both return an empty page with `next_cursor == 0`. Walk the full archive with:

```rust
let mut cursor = 0u32;
loop {
    let page = client.get_archived_reports_page(&user, &cursor, &DEFAULT_PAGE_LIMIT);
    // ... process `page.items` ...
    if page.next_cursor == 0 { break; }
    cursor = page.next_cursor;
}
```

> **Deprecation note (Issue #832):** `get_archived_reports(user)` is preserved for backwards compatibility but is **bounded** to the first `DEFAULT_PAGE_LIMIT` (20) entries — it no longer walks the entire `ARCH_IDX(user)` list. Callers should migrate to `get_archived_reports_page` to walk the full archive without hitting the host return-size/gas budget.

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
