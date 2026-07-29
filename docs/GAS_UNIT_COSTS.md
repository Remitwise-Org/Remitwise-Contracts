# Gas Unit Costs Reference Table

> **Audience:** Contributors optimizing contract gas usage, operators estimating deployment costs, downstream integrators budgeting transaction fees.

This document records the per-unit gas costs for Soroban resources (CPU instructions, memory, ledger I/O) as defined by the Stellar network parameters and measured by our CI gas benchmarks. Updated per release.

---

## 1. Network Cost Parameters (Protocol 20/21 Mainnet)

| Resource | Unit Cost | Increment | Notes |
|----------|-----------|-----------|-------|
| **CPU Instructions** | 25 stroops | per 10,000 instructions | `fee_per_instruction_increment` = 25; minimum 3M instructions billed |
| **Ledger Entry Read** | 6,250 stroops | per entry | `fee_read_ledger_entry` |
| **Ledger Entry Write** | 10,000 stroops | per entry | `fee_write_ledger_entry`; also counts as a read |
| **Ledger Read Bytes** | 1,786 stroops | per KB | `fee_read_1kb` |
| **Ledger Write Bytes** | ~11,800 stroops | per KB | `fee_write_1kb`; dynamic based on state size |
| **Transaction Size** | ~1,786 stroops | per KB | `fee_propagate_1kb` |
| **Events / Return Value** | ~1,786 stroops | per KB | `fee_extended_meta_1kb` |
| **Rent (Persistent)** | `fee_per_write_1kb / persistent_rent_rate_denominator` | per KB per ledger | Denominator ≈ 730 ledgers (~1 hour) |
| **Rent (Temporary)** | `fee_per_write_1kb / temp_rent_rate_denominator` | per KB per ledger | Denominator ≈ 3650 ledgers (~5 hours) |

*Source: `soroban-env-host/src/fees.rs`, Stellar Lab network limits, Protocol 20/21 network parameters.*

---

## 2. Soroban Host Cost Model (Internal Metering)

The host meters **CPU instructions** and **memory bytes** using linear cost functions per `ContractCostType`:

```
cost = const_term + linear_term * input
```

### Key Cost Types (CPU + Memory)

| Cost Type | CPU const | CPU linear | Mem const | Mem linear | Typical Use |
|-----------|-----------|------------|-----------|------------|-------------|
| `WasmInsnExec` | 1 | 1 | 0 | 0 | Every Wasm instruction |
| `HostMemAlloc` | ~200 | 0 | ~100 | 1 | `Vec::new`, `Map::new` |
| `HostMemCpy` | ~50 | 1/8 | ~50 | 1 | `clone`, `extend_from_slice` |
| `HostMemCmp` | ~50 | 1/4 | ~50 | 1 | `==`, sorting |
| `VisitObject` | ~100 | 0 | ~50 | 0 | Host object access |
| `ValXdrConv` | ~200 | 1 | ~100 | 1 | `TryFromVal` / `IntoVal` |
| `ComputeSha256Hash` | ~500 | 1 | ~100 | 1 | `env.crypto().sha256()` |
| `VerifyEd25519Sig` | ~50,000 | 0 | ~1,000 | 0 | `ed25519_verify` |
| `MapEntry` / `VecEntry` | ~100 | 1 | ~50 | 1 | Map/Vec indexing |
| `VmInstantiation` | ~500,000 | 0 | ~10,000 | 0 | Contract deploy / upgrade |
| `VmCachedInstantiation` | ~50,000 | 0 | ~5,000 | 0 | Cached Wasm reuse |

*Approximate calibrated values from `soroban-env-host` (x86-64 target). Exact values are network-configurable via `CONFIG_SETTING_CONTRACT_COST_PARAMS_CPU_INSTRUCTIONS` and `CONFIG_SETTING_CONTRACT_COST_PARAMS_MEMORY_BYTES`.*

---

## 3. Per-Transaction Resource Limits (Mainnet, July 2026)

| Resource | Limit |
|----------|-------|
| CPU Instructions | 100,000,000 |
| Memory (bytes) | 40,000,000 |
| Ledger Entry Reads | 40 |
| Ledger Entry Writes | 25 |
| Ledger Read Bytes | 200,000 |
| Ledger Write Bytes | 66,000 |
| Transaction Size | 100 KB |
| Events + Return Value | 8 KB |

Exceeding any limit fails the transaction.

---

## 4. RemitWise Contract Gas Benchmarks (Current Release)

*Source: `gas_results.json` from `./scripts/run_gas_benchmarks.sh` (CI run 2026-07-25).*

### remittance_split

| Method | Scenario | CPU (instructions) | Memory (bytes) | Est. Fee* (stroops) |
|--------|----------|-------------------|----------------|---------------------|
| `distribute_usdc` | 4_recipients_all_nonzero | 787,445 | 115,986 | ~2,200 |
| `create_remittance_schedule` | single_recurring_schedule | 165,158 | 32,466 | ~600 |
| `create_remittance_schedule` | 11th_schedule_with_existing | 206,970 | 45,657 | ~700 |
| `modify_remittance_schedule` | single_schedule_modification | 131,052 | 23,945 | ~500 |
| `cancel_remittance_schedule` | single_schedule_cancellation | 129,513 | 23,873 | ~500 |
| `get_remittance_schedule` | single_schedule_lookup | 61,048 | 10,807 | ~300 |
| `get_remittance_schedules` | empty_schedules | 19,338 | 2,203 | ~200 |
| `get_remittance_schedules` | 5_schedules_with_isolation | 162,438 | 20,506 | ~550 |
| `get_remittance_schedules` | 50_schedules_worst_case | 1,235,368 | 126,616 | ~3,300 |
| `get_schedules_paginated` | n1_cursor0_limit10 | 84,240 | 13,260 | ~350 |
| `get_schedules_paginated` | n50_cursor0_limit10 | 319,014 | 45,322 | ~1,000 |

### savings_goals

| Method | Scenario | CPU | Memory | Est. Fee* |
|--------|----------|-----|--------|-----------|
| `create_savings_schedule` | single_schedule | 146,981 | 22,926 | ~550 |
| `batch_add_to_goals` | 50_items | 4,271,644 | 776,320 | ~11,000 |
| `execute_due_savings_schedules` | 50_schedules | 6,829,516 | 1,395,695 | ~17,500 |
| `get_all_goals` | 100_goals_single_owner | 2,903,034 | 295,167 | ~7,500 |
| `get_goals` (paginated) | first_page_n1000 | 1,663,669 | 279,399 | ~4,500 |

### bill_payments

| Method | Scenario | CPU | Memory | Est. Fee* |
|--------|----------|-----|--------|-----------|
| `archive_paid_bills` | 99_paid_1_unpaid_preserved | 17,144,297 | 3,792,882 | ~43,500 |
| `batch_pay_bills` | mixed_batch_50_partial_success | 2,990,560 | 697,723 | ~7,800 |
| `bulk_cleanup_bills` | mixed_age_20_of_30_deleted | 1,474,271 | 295,564 | ~4,000 |
| `get_all_bills_for_owner` | 50_owner_bills_page | 2,494,052 | 433,299 | ~6,500 |
| `get_overdue_bills` | 50_overdue_bills_page | 2,449,765 | 423,143 | ~6,400 |
| `get_unpaid_bills` | 50_unpaid_bills_page | 2,494,052 | 433,442 | ~6,500 |
| `restore_bill` | single_archived_owner_restore | 190,807 | 32,935 | ~700 |

### insurance

| Method | Scenario | CPU | Memory | Est. Fee* |
|--------|----------|-----|--------|-----------|
| `create_policy` | single_policy | ~150,000 | ~30,000 | ~600 |
| `pay_premium` | single_payment | ~120,000 | ~25,000 | ~500 |
| `get_active_policies` | 10_policies | ~500,000 | ~80,000 | ~1,500 |
| `get_total_monthly_premium` | 20_policies | ~2,200,000 | ~428,000 | ~6,000 |

### family_wallet

| Method | Scenario | CPU | Memory | Est. Fee* |
|--------|----------|-----|--------|-----------|
| `configure_multisig` | 9_signers_threshold_all | 477,056 | 85,262 | ~1,500 |

### orchestrator

| Method | Scenario | CPU | Memory | Est. Fee* |
|--------|----------|-----|--------|-----------|
| `execute_remittance_flow` | full_flow_4_contracts | ~15,000,000 | ~2,500,000 | ~38,000 |

---

\* **Est. Fee** = Resource fee only (excludes inclusion fee of 100 stroops).  
Calculated as: `(CPU / 10000) * 25 + (read_entries * 6250) + (write_entries * 10000) + (read_kb * 1786) + (write_kb * 11800)`.  
Actual fee varies with network state (dynamic write fee) and simulation overhead (20% margin on CPU).

---

## 5. Cost Estimation Formulas

### Resource Fee (stroops)
```
resource_fee =
  ceil(cpu_instructions / 10000) * fee_per_instruction_increment
+ read_entries * fee_read_ledger_entry
+ write_entries * fee_write_ledger_entry
+ ceil(read_bytes / 1024) * fee_read_1kb
+ ceil(write_bytes / 1024) * fee_write_1kb
+ ceil(tx_size_bytes / 1024) * fee_propagate_1kb
+ ceil(events_bytes / 1024) * fee_extended_meta_1kb
```

### Total Transaction Fee
```
tx_fee = resource_fee + inclusion_fee
inclusion_fee = max(100, effective_base_fee)  // 1 operation for SC txns
```

### Rent Fee (per ledger, per KB)
```
rent_fee_per_ledger = (entry_size_kb * fee_per_write_1kb) / rent_rate_denominator
// persistent: denominator ≈ 730; temporary: denominator ≈ 3650
```

---

## 6. Benchmarking Locally

```bash
# Run all gas benchmarks and generate gas_results.json
./scripts/run_gas_benchmarks.sh

# Single contract
RUST_TEST_THREADS=1 cargo test -p remittance_split --test gas_bench -- --nocapture

# Compare against baseline (fails if regression > threshold)
./scripts/compare_gas_results.sh benchmarks/baseline.json gas_results.json

# Update baseline after verified optimization
./scripts/update_baseline.sh
```

---

## 7. Cost Optimization Checklist

| Pattern | CPU Impact | Mem Impact | Example Fix |
|---------|------------|------------|-------------|
| Storage read in loop | High | Medium | Cache aggregates (`UNPD_TOT`, `PRM_TOT`) |
| Storage write in loop | Very High | High | Batch writes at end of function |
| `Vec::clone()` in hot path | Medium | High | Reuse buffers, avoid clone |
| `symbol_short!` > 9 chars | Compile error | — | Use `Symbol::new(&env, "long_name")` |
| Event emission in internal fn | Medium | Medium | Add `emit_events: bool` flag |
| Redundant `env.ledger().timestamp()` | Low | Low | Cache in local variable |
| Large `Vec` allocation | Low | High | Pre-size with `Vec::with_capacity` |

---

## 8. Updating This Document

1. Run `./scripts/run_gas_benchmarks.sh` on a clean mainnet-compatible testnet reset
2. Copy `gas_results.json` → `benchmarks/baseline.json` via `./scripts/update_baseline.sh`
3. Update the tables in **Section 4** with new numbers
4. Update network parameters in **Section 1** if protocol version changed
5. Commit with message: `docs: update gas unit costs for release vX.Y.Z`

---

## 9. Cross-References

- [Gas Tuning Guide](GAS_TUNING.md) — How to interpret snapshots and which knobs to turn
- [Gas Optimization Report](gas-optimization.md) — Implemented optimizations and before/after data
- [Soroban Fees & Metering](https://developers.stellar.org/docs/learn/fundamentals/fees-resource-limits-metering) — Official documentation
- [Stellar Lab Network Limits](https://lab.stellar.org/network-limits) — Live network parameters