# Insurance Contract

> **RemitWise Soroban Smart Contract — Micro-Insurance Policy Management**

---

## Table of Contents

1. [Overview](#overview)
2. [Coverage Types & Constraints](#coverage-types--constraints)
3. [Validation Rules](#validation-rules)
4. [Security Model](#security-model)
5. [Contract Functions](#contract-functions)
6. [Events](#events)
7. [Error Codes](#error-codes)
8. [Storage Layout](#storage-layout)
9. [Running Tests](#running-tests)
10. [Integration Guide](#integration-guide)
11. [Security Notes](#security-notes)
12. [Security Assumptions](#security-assumptions)

---

## Overview

The `insurance` contract manages micro-insurance policies for RemitWise users.  
It enforces **strict validation** on every policy creation call, rejecting:

- Unsupported coverage-type / amount combinations
- Monthly premiums outside the per-type allowed range
- Coverage amounts outside the per-type allowed range
- Economically implausible coverage-to-premium ratios
- Empty or oversized string fields
- Negative or zero numeric values

All state-mutating functions require explicit caller authorization (`require_auth()`).  
Administrative actions (deactivate, set_external_ref) are restricted to the contract owner.

---

## Coverage Types & Constraints

All monetary values are in **stroops** (1 XLM = 10,000,000 stroops).

| Coverage Type | Min Premium | Max Premium   | Min Coverage | Max Coverage      |
|---------------|-------------|---------------|--------------|-------------------|
| `Health`      | 1,000,000   | 500,000,000   | 10,000,000   | 100,000,000,000   |
| `Life`        | 500,000     | 1,000,000,000 | 50,000,000   | 500,000,000,000   |
| `Property`    | 2,000,000   | 2,000,000,000 | 100,000,000  | 1,000,000,000,000 |
| `Auto`        | 1,500,000   | 750,000,000   | 20,000,000   | 200,000,000,000   |
| `Liability`   | 800,000     | 400,000,000   | 5,000,000    | 50,000,000,000    |

### Ratio Guard

In addition to range checks, every policy creation enforces:

```
coverage_amount <= monthly_premium × 12 × 500
```

This limits leverage to **500× annual premium**, blocking economically nonsensical
inputs (e.g. a $0.10/month premium insuring $1 billion in coverage) while remaining
generous enough not to interfere with real-world micro-insurance products.

---

## Validation Rules

Policy creation (`create_policy`) validates inputs in this order:

1. **Contract initialized** — panics if `init` was never called
2. **Caller auth** — `caller.require_auth()` must succeed
3. **Name non-empty** — `name.len() > 0`
4. **Name length** — `name.len() <= 64` bytes
5. **Premium positive** — `monthly_premium > 0`
6. **Coverage positive** — `coverage_amount > 0`
7. **Premium in range** — within per-type `[min_premium, max_premium]`
8. **Coverage in range** — within per-type `[min_coverage, max_coverage]`
9. **Ratio guard** — `coverage_amount <= monthly_premium * 12 * 500`
10. **External ref length** — `external_ref.len() <= 128` (if provided, also must be > 0)
11. **Capacity** — active policy count < 1,000

All overflow arithmetic uses `checked_mul` / `checked_add` / `saturating_add`
to prevent silent numeric wrap-around.

---

## Security Model

### Authorization

| Function                        | Who can call?                 |
|---------------------------------|-------------------------------|
| `init`                          | Owner (once)                  |
| `create_policy`                 | Any authenticated caller      |
| `pay_premium`                   | Any authenticated caller      |
| `set_external_ref`              | Owner only                    |
| `deactivate_policy`             | Owner only                    |
| `set_pause_all`                 | Owner only                    |
| `set_pause_fn`                  | Owner only                    |
| `batch_pay_premiums`            | Any authenticated caller      |
| `create_premium_schedule`       | Any authenticated caller      |
| `modify_premium_schedule`       | Schedule owner only           |
| `cancel_premium_schedule`       | Schedule owner only           |
| `execute_due_premium_schedules` | Anyone (permissionless crank)  |
| `is_paused` / `is_fn_paused`   | Anyone (read-only)            |
| `get_*` (queries)               | Anyone (read-only)            |

### Invariants

- Policy IDs are monotonically increasing `u32` values starting at 1.
  The counter is stored persistently and uses `checked_add` to detect overflow.
- An inactive policy can never receive premium payments.
- An already-inactive policy cannot be deactivated again.
- The owner address is set exactly once and cannot be changed after `init`.

### Known Limitations (pre-mainnet)

- **No reentrancy guard**: Soroban's single-threaded execution model prevents
  classical reentrancy, but cross-contract call chains should be reviewed before
  any orchestrator integration.
- **No rate limiting**: Premium payments are not throttled per ledger.
  Rate limiting should be enforced at the application layer.
- **Owner key management**: Loss of the owner key permanently prevents
  administrative operations. A multisig owner address is strongly recommended
  for production deployments.

---

## Contract Functions

### `init(owner: Address)`

Initializes the contract. Must be called exactly once.

- Sets the contract owner.
- Resets the policy counter to 0.
- Initializes the active-policy list to empty.
- Panics with `"already initialized"` on a second call.

---

### `create_policy(caller, name, coverage_type, monthly_premium, coverage_amount, external_ref) → u32`

Creates a new insurance policy after full validation (see [Validation Rules](#validation-rules)).

Returns the new policy's `u32` ID.

**Parameters**

| Parameter         | Type              | Description                                      |
|-------------------|-------------------|--------------------------------------------------|
| `caller`          | `Address`         | Policyholder address (must sign)                 |
| `name`            | `String`          | Human-readable label (1–64 bytes)                |
| `coverage_type`   | `CoverageType`    | One of: Health, Life, Property, Auto, Liability  |
| `monthly_premium` | `i128`            | Monthly cost in stroops (> 0, in-range)          |
| `coverage_amount` | `i128`            | Insured value in stroops (> 0, in-range)         |
| `external_ref`    | `Option<String>`  | Optional off-chain reference (1–128 bytes)       |

**Emits**: `PolicyCreatedEvent`

---

### `pay_premium(caller, policy_id, amount) → bool`

- `owner`: Address of the policy owner
- `cursor`: Starting policy ID (0 for first page)
- `limit`: Maximum items per page
- `env`: Environment

**Returns:** `PolicyPage` struct with active items, `count`, and `next_cursor`.

Paging contract semantics:
- Items are ordered by policy `id` ascending.
- `next_cursor` is set to the last returned policy ID.
- `next_cursor = 0` indicates paging is complete.
- Concatenating all pages until `next_cursor = 0` yields no duplicate policy IDs.

#### Date Progression Logic

The next payment date is calculated to prevent schedule drift:

- **Early/On-time payment**: The next due date advances by exactly one 30-day interval
  from the *previous* due date (not the current timestamp).
- **Late payment**: The next due date advances from the previous due date by 30 days.
  If that new date is still in the past, it continues advancing by 30-day intervals
  until the next due date is in the future.

This ensures that:
1. Early payments don't shift the schedule forward (no "drift bonus")
2. Late payments don't double-cover periods (no skipped periods)
3. The payment schedule remains deterministic regardless of payment timing

**Example**: If `next_payment_due` is January 15th and payment is made on January 10th,
the new `next_payment_due` will be February 14th (January 15th + 30 days), not
February 9th (January 10th + 30 days).

**Emits**: `PremiumPaidEvent`

---

### `set_external_ref(owner, policy_id, ext_ref) → bool`

Owner-only. Updates or clears the `external_ref` field of a policy.

**Parameters**

| Parameter    | Type              | Description                              |
|--------------|-------------------|------------------------------------------|
| `owner`      | `Address`         | Contract owner (must authorize)          |
| `policy_id`  | `u32`             | Target policy ID                         |
| `ext_ref`    | `Option<String>`  | New external reference (1–128 bytes or None) |

**Validates**

- Caller is the contract owner
- Policy exists
- External ref length is in range (1–128 bytes if Some, or None to clear)

**Emits**: `ExternalRefUpdatedEvent`

---

### `deactivate_policy(owner, policy_id) → bool`

Owner-only. Marks a policy as inactive and removes it from the active-policy list.

**Hardening Features**:
- **Idempotent**: If the policy is already inactive, returns `false` without duplicate events.
- **Schedule Cleanup**: Automatically deactivates any associated `PremiumSchedule` if present.
- **Standardized Events**: Emits `InsuranceEvent::PolicyDeactivated` and (if applicable) `InsuranceEvent::ScheduleCancelled`.

---

### `set_pause_all(owner, paused: bool)`

Owner-only. Sets or clears the **global emergency pause** flag.  
When `paused = true`, ALL state-mutating functions (`create_policy`, `pay_premium`,
`deactivate_policy`, `set_external_ref`, schedule operations, `batch_pay_premiums`)
will panic with `"contract is paused"`.

The owner can always call this function regardless of the current pause state.

---

### `set_pause_fn(owner, fn_name: Symbol, paused: bool)`

Owner-only. Sets or clears a **granular per-function pause** flag.

Supported `fn_name` values:

| `fn_name`      | Functions blocked                                         |
|----------------|-----------------------------------------------------------|
| `"create"`     | `create_policy`                                           |
| `"pay"`        | `pay_premium`, `batch_pay_premiums`                       |
| `"deactivate"` | `deactivate_policy`                                       |
| `"set_ref"`    | `set_external_ref`                                        |
| `"schedule"`   | `create_premium_schedule`, `modify_premium_schedule`, `cancel_premium_schedule` |

The **global pause always takes priority** over per-function flags.  
If the global flag is set, all functions are blocked regardless of per-function settings.

---

### `is_paused() → bool`

Returns whether the global emergency pause flag is set.

---

### `is_fn_paused(fn_name: Symbol) → bool`

Returns `true` if the specified function is blocked — either because the
global pause is set **or** the per-function flag for `fn_name` is `true`.

---

### `get_active_policies() → Vec<u32>`

Returns the list of all active policy IDs.

---

### `get_policy(policy_id) → Policy`

Returns the full `Policy` record. Panics if the policy does not exist.

---

### `get_total_monthly_premium() → i128`

Returns the sum of `monthly_premium` across all active policies.
Uses `saturating_add` to prevent overflow on extremely large portfolios.

---

### `add_tag(caller, policy_id, tag)`

Attaches a string label to a policy. Duplicate tags are silently ignored.

**Parameters**

| Parameter   | Type      | Description                                              |
|-------------|-----------|----------------------------------------------------------|
| `caller`    | `Address` | Must be the policy owner or contract admin (must sign)   |
| `policy_id` | `u32`     | ID of the target policy                                  |
| `tag`       | `String`  | Label to attach (1–32 bytes, case-sensitive)             |

**Emits**: `("insure", "tag_added")` with data `(policy_id, tag)` — only when
the tag is new. No event is emitted for a duplicate call.

---

### `remove_tag(caller, policy_id, tag)`

Removes a string label from a policy. If the tag is not present the function
returns gracefully without panicking.

**Parameters**

| Parameter   | Type      | Description                                              |
|-------------|-----------|----------------------------------------------------------|
| `caller`    | `Address` | Must be the policy owner or contract admin (must sign)   |
| `policy_id` | `u32`     | ID of the target policy                                  |
| `tag`       | `String`  | Label to remove (case-sensitive)                         |

**Emits**:
- `("insure", "tag_rmvd")` with data `(policy_id, tag)` when the tag was found and removed.
- `("insure", "tag_miss")` with data `(policy_id, tag)` when the tag was not present.

---

## Events

All events are published via `env.events().publish(topic, data)` and can be
indexed off-chain using the RemitWise event indexer.

### `PolicyCreatedEvent`

Published on successful `create_policy`.

| Field             | Type           |
|-------------------|----------------|
| `policy_id`       | `u32`          |
| `name`            | `String`       |
| `coverage_type`   | `CoverageType` |
| `monthly_premium` | `i128`         |
| `coverage_amount` | `i128`         |
| `timestamp`       | `u64`          |

Topic: `("created", "policy")`

### `PremiumPaidEvent`

Published on successful `pay_premium`.

| Field               | Type     |
|---------------------|----------|
| `policy_id`         | `u32`    |
| `name`              | `String` |
| `amount`            | `i128`   |
| `next_payment_date` | `u64`    |
| `timestamp`         | `u64`    |

Topic: `("paid", "premium")`

### `PolicyDeactivatedEvent`

Published on successful `deactivate_policy`.

| Field       | Type     |
|-------------|----------|
| `policy_id` | `u32`    |
| `name`      | `String` |
| `timestamp` | `u64`    |

Topic: `("deactive", "policy")`

### `ExternalRefUpdatedEvent`

Published on successful `set_external_ref`.

| Field              | Type               |
|--------------------|--------------------|
| `policy_id`        | `u32`              |
| `name`             | `String`           |
| `new_external_ref` | `Option<String>`   |
| `old_external_ref` | `Option<String>`   |
| `timestamp`        | `u64`              |

**Description**: Tracks external reference mutations for audit trails. The `old_external_ref`
and `new_external_ref` fields capture the complete state transition (None→Some, Some→Some,
Some→None), allowing off-chain systems to reconcile policy metadata across multiple updates.

Topic: `("pol", "ext_upd")`

---

## Error Codes

Errors are surfaced as Rust panics with descriptive string messages.
The `InsuranceError` enum documents the full set of error conditions:

| Code | Variant               | Message (approximate)                                            |
|------|-----------------------|------------------------------------------------------------------|
| 1    | `Unauthorized`        | `"unauthorized"`                                                 |
| 2    | `AlreadyInitialized`  | `"already initialized"`                                          |
| 3    | `NotInitialized`      | `"not initialized"`                                              |
| 4    | `PolicyNotFound`      | `"policy not found"`                                             |
| 5    | `PolicyInactive`      | `"policy inactive"` / `"policy already inactive"`                |
| 6    | `InvalidName`         | `"name cannot be empty"` / `"name too long"`                     |
| 7    | `InvalidPremium`      | `"monthly_premium must be positive"` / `"…out of range…"`        |
| 8    | `InvalidCoverageAmount` | `"coverage_amount must be positive"` / `"…out of range…"`      |
| 9    | `UnsupportedCombination` | `"unsupported combination: coverage_amount too high…"`        |
| 10   | `InvalidExternalRef`  | `"external_ref length out of range"`                             |
| 11   | `MaxPoliciesReached`  | `"max policies reached"`                                         |

---

## Storage Layout

All data is stored in the **instance** storage bucket (persists for the contract
lifetime when TTL is regularly bumped by users).

| Key                   | Type        | Description                          |
|-----------------------|-------------|--------------------------------------|
| `DataKey::Owner`      | `Address`   | Contract owner                       |
| `DataKey::PolicyCount`| `u32`       | Monotonic ID counter                 |
| `DataKey::Policy(id)` | `Policy`    | Full policy record                   |
| `DataKey::ActivePolicies` | `Vec<u32>` | List of active policy IDs        |

---

## Running Tests

```bash
# Run all tests for this contract
cargo test -p insurance

# Run with output (see panic messages)
cargo test -p insurance -- --nocapture

# Run a single test
cargo test -p insurance test_create_health_policy_success -- --nocapture

# Run gas benchmarks (if configured)
RUST_TEST_THREADS=1 cargo test -p insurance --test gas_bench -- --nocapture
```

### Expected output (all tests passing)

```
running 82 tests
test tests::test_init_success ... ok
test tests::test_create_health_policy_success ... ok
...
test tests::test_set_external_ref_on_deactivated_policy_succeeds ... ok
test result: ok. 82 passed; 0 failed; 0 ignored
```

The comprehensive test suite includes:
- 4 basic external_ref tests (set, clear, authorization, length validation)
- 18 exhaustive external_ref mutation tests covering:
  - Event emission validation
  - State transitions (None→Some, Some→Some, Some→None)
  - Idempotent and sequential mutations
  - Persistence across policy operations
  - Boundary conditions (min/max length)
  - Empty string and special character handling
  - Policy isolation and field preservation
  - Deactivated policy behavior
  - Authorization enforcement
---

## Integration Guide

### Typical policyholder flow

```rust
// 1. Initialize (deploy once)
client.init(&owner_address);

// 2. Create a health policy
let policy_id = client.create_policy(
    &user_address,
    &String::from_str(&env, "Family Health Plan"),
    &CoverageType::Health,
    &10_000_000i128,   // 1 XLM / month
    &100_000_000i128,  // 10 XLM coverage
    &Some(String::from_str(&env, "PROVIDER-ABC-123")),
);

// 3. Pay monthly premium
client.pay_premium(&user_address, &policy_id, &10_000_000i128);

// 4. Query total cost
let total = client.get_total_monthly_premium(); // sums all active policies
```

### Checking constraints before calling

To avoid a failed transaction, verify on the client side that:

```
min_premium[type] <= monthly_premium <= max_premium[type]
min_coverage[type] <= coverage_amount <= max_coverage[type]
coverage_amount <= monthly_premium * 12 * 500
name.len() in 1..=64
external_ref.len() in 1..=128  (if supplied)
```

---

## Security Notes

1. **Always use `require_auth`** — every state-changing function in this contract
   calls `require_auth` on the relevant address before performing any writes.

2. **Checked arithmetic** — all multiplication operations used in validation use
   `checked_mul` to surface overflows rather than silently wrapping.

3. **Monotonic IDs** — policy IDs increment by exactly 1 per creation with
   `checked_add`, so an overflow (at `u32::MAX` ≈ 4 billion policies) panics
   rather than resetting to 0 and colliding with existing policies.

4. **No self-referential calls** — this contract does not call back into itself
   or other contracts, eliminating classical reentrancy vectors.

5. **Pause controls** — the contract supports two layers of pause protection:
   - **Global emergency pause** (`set_pause_all`): blocks ALL mutating operations.
     The owner can always toggle this flag, even while the contract is paused.
   - **Granular per-function pauses** (`set_pause_fn`): block only specific
     functions (e.g. `"create"`, `"pay"`) while leaving others operational.
   - **Priority rule**: the global pause always overrides per-function flags.
   - **Read-only queries** (`get_policy`, `get_active_policies`,
     `get_total_monthly_premium`, `is_paused`, `is_fn_paused`) are **never**
     blocked by pause controls.
   - Both pause toggle functions require `owner.require_auth()`, preventing
     non-owner addresses from activating or deactivating pauses.

6. **Pre-mainnet gaps** (inherited from project-level THREAT_MODEL.md):
   - `[SECURITY-003]` Rate limiting for emergency transfers is not yet implemented.
   - `[SECURITY-005]` MAX_POLICIES (1,000) provides a soft cap but no per-user limit.

For security disclosures, email **security@remitwise.com**.
