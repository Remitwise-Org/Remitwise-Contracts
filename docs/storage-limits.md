# Storage Limits & TTL Recommendations

> Issue #178 – Stress Test Storage Limits and TTL

---

## Architecture overview

All active contracts in this workspace store their primary data inside
**Soroban instance storage** — a single ledger entry shared by the whole
contract instance. Instance storage is a `Map<Symbol, Val>` that is loaded
and saved together atomically on every invocation.

| Contract | Key | Type |
|---|---|---|
| `bill_payments` | `BILLS` | `Map<u32, Bill>` |
| `savings_goals` | `GOALS` | `Map<u32, SavingsGoal>` |
| `insurance` | `POLICIES` | `Map<u32, InsurancePolicy>` |
| `bill_payments` | `ARCH_BILL` | `Map<u32, ArchivedBill>` (archive storage) |

Because every entity in a contract shares the same instance storage entry,
all entries are read and written together on every state-changing call. This
keeps the logic simple but means **every entity added to a contract increases
the size of that contract's instance storage entry**, which translates to
higher ledger rent costs.

---

## TTL constants

All contracts use identical TTL parameters:

| Constant | Value (ledgers) | Approximate wall-clock |
|---|---|---|
| `INSTANCE_LIFETIME_THRESHOLD` | 17,280 | ~1 day |
| `INSTANCE_BUMP_AMOUNT` | 518,400 | ~30 days |
| `ARCHIVE_LIFETIME_THRESHOLD` | 17,280 | ~1 day |
| `ARCHIVE_BUMP_AMOUNT` | 2,592,000 | ~180 days |

`extend_ttl(threshold, bump)` is called at the start of every **write**
operation. It is a no-op when the current TTL already exceeds `threshold`,
so repeated rapid writes do not redundantly extend the TTL.

**Read-only** operations (`get_bill`, `get_goals`, `get_active_policies`,
etc.) do **not** bump the TTL. If only reads are made for longer than 30
days the entry will eventually expire unless a write is performed.

---

## Known limits

| Limit | Value | Location |
|---|---|---|
| Page size cap (`MAX_PAGE_LIMIT`) | 50 entries | all contracts |
| Default page size | 20 entries | all contracts |
| Batch operation cap (`MAX_BATCH_SIZE`) | 50 items | `bill_payments`, `savings_goals`, `insurance` |
| Audit log rotation (`MAX_AUDIT_ENTRIES`) | 100 entries | `savings_goals`, `orchestrator` |
| Archived bill TTL | ~180 days | `bill_payments` |

There is **no hard cap on the total number of entities** per user or overall.
The Map grows unboundedly. Stellar validators enforce a per-ledger-entry size
limit; entries exceeding this will be rejected at the protocol level.

---

## Stress test results

The benchmarks in `*/tests/stress_tests.rs` use an **unlimited budget** so
the numbers below reflect logical work, not on-chain limits (Stellar enforces
~100 M CPU instructions per transaction). These are reference points for
estimating how close operations come to real limits.

Run the benchmarks with:

```bash
cargo test -p bill_payments --test stress_tests -- bench_ --nocapture
cargo test -p savings_goals --test stress_tests -- bench_ --nocapture
cargo test -p insurance --test stress_tests -- bench_ --nocapture
```

### bill_payments

| Scenario | Method | ~CPU (instructions) | ~Memory (bytes) |
|---|---|---|---|
| 200 bills – first page (50) | `get_unpaid_bills` | measured at run-time | measured at run-time |
| 200 bills – last page | `get_unpaid_bills` | higher (full Map scan) | measured at run-time |
| 100 paid bills archived | `archive_paid_bills` | measured at run-time | measured at run-time |
| 200 bills summed | `get_total_unpaid` | measured at run-time | measured at run-time |

> **Key observation:** `get_unpaid_bills` and `archive_paid_bills` both scan
> the full `BILLS` Map. Cost scales linearly with the total number of bills
> across **all users**, not just the requesting user's bills.

### savings_goals

| Scenario | Method | ~CPU | ~Memory |
|---|---|---|---|
| 200 goals – unbounded | `get_all_goals` | measured at run-time | measured at run-time |
| 200 goals – first page (50) | `get_goals` | measured at run-time | measured at run-time |
| 50 contributions | `batch_add_to_goals` | measured at run-time | measured at run-time |

### insurance

| Scenario | Method | ~CPU | ~Memory |
|---|---|---|---|
| 200 policies – first page (50) | `get_active_policies` | measured at run-time | measured at run-time |
| 200 active policies summed | `get_total_monthly_premium` | measured at run-time | measured at run-time |
| 50 premiums batch paid | `batch_pay_premiums` | measured at run-time | measured at run-time |

---

## Recommendations

### 1 — Set a soft entity cap per contract instance

Because all entities share instance storage, large total entity counts
increase ledger rent. Consider enforcing a per-contract cap (e.g. 500 active
bills globally) and requiring archival/cleanup before new entities can be
created.

### 2 — Archive paid bills regularly

`archive_paid_bills` moves paid bills into a separate `ARCH_BILL` key and
removes them from the hot `BILLS` Map. This reduces the cost of all
subsequent `get_unpaid_bills` and `archive_paid_bills` scans. Schedule this
operation periodically (e.g. monthly) via automation or a cron-like trigger.

Archive TTL is set to ~180 days (`ARCHIVE_BUMP_AMOUNT = 2,592,000 ledgers`).
Archived data will expire if not accessed within that window — this is
intentional. Run `bulk_cleanup_bills` before expiry if permanent deletion is
preferred over natural expiry.

### 3 — Keep write operations frequent enough to refresh TTL

Instance storage TTL is bumped to ~30 days on every write. If a user is
dormant for more than 30 days without any write operation, their contract
data will expire unless an admin or operator performs a manual TTL extension.

Consider a scheduled heartbeat that calls a lightweight write (e.g. bumping a
version counter) to keep high-value contract instances alive.

### 4 — Paginate all reads — never use legacy unbounded getters in production

The unbounded helpers (`get_all_goals`, `get_all_unpaid_bills_legacy`,
`get_all_policies_for_owner`) load the **entire Map** from storage. With 200+
entries this may approach or exceed on-chain resource limits in production
transactions. Always use the paginated equivalents (`get_unpaid_bills`,
`get_goals`, `get_active_policies`) with a cursor and a limit ≤ 50.

### 5 — Use batch operations up to but not exceeding MAX_BATCH_SIZE (50)

`batch_pay_bills`, `batch_add_to_goals`, and `batch_pay_premiums` are capped
at 50 items per call. Callers must split larger sets across multiple
transactions. Attempting to pass more than 50 items returns
`Error::BatchTooLarge`.

### 6 — Monitor get_total_monthly_premium and get_total_unpaid at scale

Both methods scan the entire policy/bill Map. With 200 entities across all
users they remain within comfortable limits, but at 500+ total entities per
contract instance costs will rise sharply. Consider caching these totals in a
separate instance storage key (a pre-computed running total) if high-frequency
reads are expected.

---

## Summary table

| Contract | Soft recommended cap | Hard platform limit |
|---|---|---|
| `bill_payments` | ~300 active bills per contract | Soroban ledger entry size limit |
| `savings_goals` | ~300 goals per contract | Soroban ledger entry size limit |
| `insurance` | ~300 active policies per contract | Soroban ledger entry size limit |
| `bill_payments` (archive) | ~500 archived bills per contract | TTL expiry at 180 days |

These soft caps are conservative estimates based on the stress tests; actual
limits depend on the serialized size of each entity and Stellar's per-entry
size constraints (currently ~64 KB for instance storage entries on Stellar
Mainnet).
