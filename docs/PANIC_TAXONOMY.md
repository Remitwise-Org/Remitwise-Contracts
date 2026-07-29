# Panic Taxonomy

Every panic path in RemitWise Contracts, documented with recovery guidance.

**Audience:** Contributors (new and experienced).

**Goal:** Understand what every `panic!()` means, why it fires, and how to fix it — without reading every commit or asking a senior engineer.

---

## Table of Contents

1. [Why This Matters](#why-this-matters)
2. [Runtime Behaviour: `panic = "abort"`](#runtime-behaviour-panic--abort)
3. [Panic Mechanism Quick Reference](#panic-mechanism-quick-reference)
4. [Category 1 — Storage Integrity Panics](#category-1--storage-integrity-panics)
5. [Category 2 — Authorisation Panics](#category-2--authorisation-panics)
6. [Category 3 — Input Validation Panics](#category-3--input-validation-panics)
7. [Category 4 — State Invariant Panics](#category-4--state-invariant-panics)
8. [Category 5 — `panic_with_error!()`  (Typed Contract Errors)](#category-5--panic_with_error---typed-contract-errors)
9. [Category 6 — Cryptographic Panics](#category-6--cryptographic-panics)
10. [Category 7 — Precondition Assertions (`assert!`)](#category-7--precondition-assertions-assert)
11. [Category 8 — Schedule Execution Panics](#category-8--schedule-execution-panics)
12. [Category 9 — Batch Operation Panics](#category-9--batch-operation-panics)
13. [View Function No-Panic Rule](#view-function-no-panic-rule)
14. [Test-Expected Panics (`#[should_panic]`)](#test-expected-panics-should_panic)
15. [Recovery Cheat Sheet](#recovery-cheat-sheet)
16. [Related Documentation](#related-documentation)

---

## Why This Matters

In Soroban, a panic aborts the entire transaction **immediately**. There is no try/catch, no unwinding, and no way to recover inside the same transaction. The caller pays gas for a failed invocation and receives a generic error — not a typed `#[contracterror]` code they can match on.

Writing this down:

- **Lets reviewers verify behaviour** against the documented intent.
- **Lets new contributors get productive** without reading every commit.
- **Lets the support team answer common questions** without paging an engineer.

---

## Runtime Behaviour: `panic = "abort"`

All RemitWise contracts compile with `panic = "abort"` in their `Cargo.toml` release profile:

```toml
[profile.release]
panic = "abort"
```

This means:

| What happens | Detail |
|---|---|
| Transaction aborts | No state changes are committed. |
| Gas is consumed | The caller pays for all work up to the panic point. |
| Error propagation | The caller receives a generic "VM execution aborted" error, **not** the panic message. |
| No unwinding | `Drop` implementations do not run. The lock-guard RAII pattern in the Orchestrator's `LockGuard` relies on the host automatically clearing the lock when the contract instance terminates. |

> **Key insight:** A panic after a storage write but before the transaction commits is **safe** — Soroban rolls back all storage changes atomically. A panic before a storage write is also safe but wastes the caller's gas. The only truly dangerous pattern is a panic that leaves the caller unable to diagnose what went wrong.

---

## Panic Mechanism Quick Reference

| Mechanism | Where | Ban in production? | Typed error? | Recovery |
|---|---|---|---|---|
| `panic!("msg")` | Contract entrypoints (mutating) | Allowed with caution | ❌ No | Review the message; adjust input or fix state |
| `panic_with_error!(env, err)` | Contract entrypoints (mutating) | Allowed | ✅ Yes | Match on the `#[contracterror]` variant |
| `unwrap()` / `expect()` | **Banned** in prod (ADR) | ✅ Banned | ❌ No | Replace with `?` / `ok_or()` / `unwrap_or_else` |
| `assert!(cond, msg)` | Internal helpers | Allowed for preconditions | ❌ No | Fix the precondition violation |
| `ed25519_verify` host panic | Signature verification | Allowed (host) | ❌ No | Handle via pre-checks before calling verify |
| `#[should_panic]` | Test code only | N/A | N/A | Assert correct panic message |

---

## Category 1 — Storage Integrity Panics

These panics fire when the contract's internal data structures are in an inconsistent state. They **should never fire under correct operation**. If one fires in production, it indicates either:

- A bug in the contract logic (most likely).
- Storage corruption (extremely unlikely on Stellar).
- A missing initialisation step (e.g., wallet not initialised before use).

### Panics and Their Meanings

#### `"Wallet not initialized"` — `family_wallet/src/lib.rs`

```rust
let config = env.storage().instance().get(&CONFIG_KEY)
    .unwrap_or_else(|| panic!("Wallet not initialized"));
```

**When it fires:** A family wallet operation (`add_member`, `withdraw`, `configure_multisig`, etc.) was called before `init()`.

**Recovery:** Call `init()` first. If you see this on an already-deployed contract, verify that `init()` completed successfully and the storage entry has not expired (TTL).

#### `"Wallet already initialized"` — `family_wallet/src/lib.rs`

```rust
if env.storage().instance().has(&CONFIG_KEY) {
    panic!("Wallet already initialized");
}
```

**When it fires:** `init()` was called more than once. `init()` is idempotent by design — a second call with different parameters is rejected.

**Recovery:** Do not call `init()` twice. If you need to reconfigure, use the dedicated admin entrypoints.

#### `"Pagination index out of sync"` — `savings_goals/src/lib.rs`

```rust
let goal = env.storage().persistent()
    .get::<_, SavingsGoal>(&DataKey::Goal(goal_id))
    .unwrap_or_else(|| panic!("Pagination index out of sync"));
```

**When it fires:** The owner's goal-ID list references a `goal_id` that does not exist in persistent storage. This is a data-integrity violation — it should never happen because goal creation always writes the goal *before* appending its ID to the owner's index.

**Recovery:** If this fires, the contract state is corrupted. Open an incident; the root cause is a logic bug in goal creation, archiving, or tag-index updates.

#### `"Pagination index owner mismatch"` — `savings_goals/src/lib.rs`

```rust
if goal.owner != owner {
    panic!("Pagination index owner mismatch");
}
```

**When it fires:** A goal in the owner's pagination index is owned by a different address. This is an invariant violation.

**Recovery:** Same as pagination index out of sync — indicates a state corruption bug.

#### `"Tag index out of sync"` / `"Tag index owner mismatch"` — `savings_goals/src/lib.rs`

**When it fires:** The tag index (`TagIndex(owner, tag)`) references a goal that either doesn't exist or belongs to a different owner.

**Recovery:** Likely a bug in `add_to_tag_index` / `remove_from_tag_index` or `archive_goal` / `restore_goal`. Investigate recent tag operations on the affected owner.

#### `"Tag normalization failed"` — `savings_goals/src/lib.rs`

```rust
let canonical_tag = normalized.get(0)
    .unwrap_or_else(|| panic!("Tag normalization failed"));
```

**When it fires:** `validate_and_normalize_tags` returned an empty `Vec` despite receiving a non-empty input. This is a logic error in the shared `canonicalize_tags_checked` helper.

**Recovery:** Re-examine the tag input. If the input is valid (1–32 bytes, `[a-z0-9\-_]`), this is a bug in `canonicalize_tags_checked`.

#### `"Bill not found"` — `bill_payments/src/lib.rs`

```rust
let bill = match env.storage().persistent().get::<_, Bill>(&DataKey::Bill(bill_id)) {
    Some(b) => b,
    None => panic!("Bill not found"),
};
```

**When it fires:** A tag operation (`add_tags_to_bill`, `remove_tags_from_bill`) targeted a non-existent `bill_id`.

**Recovery:** Verify `bill_id` exists before calling tag operations. Use `get_bill(bill_id)` to check existence first.

#### `"Contract addresses not configured"` — `reporting/src/lib.rs`

```rust
let addresses = env.storage().instance().get(&ADDRESSES_KEY)
    .unwrap_or_else(|| panic!("Contract addresses not configured"));
```

**When it fires:** `get_remittance_summary` or `get_health_score` was called before `init()` configured the upstream contract addresses (remittance_split, savings_goals, bill_payments, insurance, family_wallet).

**Recovery:** Call `init()` with the full set of contract addresses before querying summaries. If you see this on a deployed contract, verify `init()` completed and the storage entry TTL has not expired.

#### `"Multi-sig config not found"` — `family_wallet/src/lib.rs`

```rust
let config: MultiSigConfig = env.storage().instance()
    .get(&Self::get_config_key(resolved_tx_type))
    .unwrap_or_else(|| panic!("Multi-sig config not found"));
```

**When it fires:** A multisig operation (`propose_transaction`, `sign_transaction`, `withdraw`) was called but the multisig configuration for the relevant `TransactionType` has not been initialised. This should not happen under normal operation since `init()` seeds default configs for all transaction types.

**Recovery:** Call `init()` first, or re-initialise the config via `configure_multisig`. If the storage entry's TTL has expired, bump it or re-populate the config.

#### `"Regular multi-sig config not found"` — `family_wallet/src/lib.rs`

```rust
let reg_config: MultiSigConfig = env.storage().instance()
    .get(&reg_config_key)
    .unwrap_or_else(|| panic!("Regular multi-sig config not found"));
```

**When it fires:** The regular withdrawal multisig config is missing — same root cause as above but specifically for the `RegularWithdrawal` transaction type used to determine withdrawal tier.

**Recovery:** Same as above — ensure `init()` completed or re-initialise via `configure_multisig`.

#### `"Pending transactions map not initialized"` — `family_wallet/src/lib.rs`

```rust
let mut pending_txs: Map<u64, PendingTransaction> = env.storage().instance()
    .get(&symbol_short!("PEND_TXS"))
    .unwrap_or_else(|| panic!("Pending transactions map not initialized"));
```

**When it fires:** A multisig operation (`propose_transaction`, `sign_transaction`, `cancel_transaction`, `get_pending_transaction`) was called but the `PEND_TXS` map has not been created. This is seeded by `init()`.

**Recovery:** Call `init()` first. If `init()` was called but the entry expired, re-initialise.

#### `"Executed transactions map not initialized"` — `family_wallet/src/lib.rs`

```rust
let mut executed_txs: Map<u64, ExecutedTxMeta> = env.storage().instance()
    .get(&symbol_short!("EXEC_TXS"))
    .unwrap_or_else(|| panic!("Executed transactions map not initialized"));
```

**When it fires:** `sign_transaction` attempted to record execution metadata but the `EXEC_TXS` map was never seeded.

**Recovery:** Call `init()` first.

#### `"Inconsistent executed transaction metadata"` — `family_wallet/src/lib.rs`

```rust
if meta.tx_id != tx_id {
    panic!("Inconsistent executed transaction metadata");
}
```

**When it fires:** During `archive_old_transactions`, the `ExecutedTxMeta.tx_id` field does not match the map key. This is a data-integrity violation.

**Recovery:** Indicates storage corruption — open an incident.

#### `"Inconsistent pending transaction data"` — `family_wallet/src/lib.rs`

```rust
if tx.tx_id != tx_id {
    panic!("Inconsistent pending transaction data");
}
```

**When it fires:** During `cleanup_expired_pending`, the `PendingTransaction.tx_id` field does not match the map key. Same data-integrity guard as the executed-transaction variant.

**Recovery:** Indicates storage corruption — open an incident.

#### `"snapshot mismatch: one is None"` — `data_migration/src/lib.rs`

```rust
(Some(_), None) | (None, Some(_)) => panic!("snapshot mismatch: one is None"),
```

**When it fires:** During snapshot round-trip validation (`export → import → export` comparison), one side of a pair is `None` while the other is `Some`. This indicates a serialisation or deserialisation bug in the migration format.

**Recovery:** This should only fire during development or testing of the migration crate. Check that the export/import pair uses compatible formats and that both sides of the comparison are populated.

#### `"Archive retention cutoff must not exceed ledger time"` — `family_wallet/src/lib.rs`

```rust
if cutoff > env.ledger().timestamp() {
    panic!("Archive retention cutoff must not exceed ledger time");
}
```

**When it fires:** A future timestamp was supplied as the archive retention cutoff when cleaning up expired pending transactions.

**Recovery:** Use `env.ledger().timestamp()` or a past timestamp for bulk cleanup operations. Cleanup only applies to records older than the cutoff.

#### `"Policy not found"` — `insurance/src/lib.rs`

```rust
let mut policy = policies.get(id).unwrap_or_else(|| panic!("Policy not found"));
```

**When it fires:** A tagging, reference-update, or status-change operation targeted a non-existent `policy_id`.

**Recovery:** Verify `policy_id` exists by calling `get_policy(policy_id)` first. Use `get_policies_page` to browse active policies.

#### `"Schedule not found"` (insurance) — `insurance/src/lib.rs`

```rust
let mut schedule = schedules.get(schedule_id).unwrap_or_else(|| panic!("Schedule not found"));
```

**When it fires:** A premium schedule operation targeted a non-existent `schedule_id`.

**Recovery:** Verify the schedule exists via `get_schedule`. Ensure the schedule was created via `create_premium_schedule` and hasn't been cancelled.

#### `"Index out of sync"` / `"Duplicate goal id in index"` / `"Goal index out of sync"` — `savings_goals/src/lib.rs`

```rust
let id = ids.get(i).unwrap_or_else(|| panic!("Index out of sync"));
if seen.contains(&id) {
    panic!("Duplicate goal id in index");
}
// ...
panic!("Goal index out of sync");
```

**When it fires:** The internal validation functions `validate_goal_index` and `validate_archived_goal_index` detect inconsistencies during bulk cleanup or archive operations. These guards verify that every ID in the owner's goal-ID list references a valid `SavingsGoal` / `ArchivedSavingsGoal`, that no duplicate IDs exist, and that the count matches.

**Recovery:** Indicates storage corruption — open an incident. These are defence-in-depth guards in cleanup/validation code paths.

#### `"Duplicate archived goal id in index"` / `"Archived goal index out of sync"` — `savings_goals/src/lib.rs`

**When it fires:** Same guards as the active goal equivalents above, but for the archived goal index.

**Recovery:** Same as above — indicates storage corruption in archived data.

#### `"Item not found"` — `family_wallet/src/lib.rs`

```rust
v.push_back(entries.get(i).unwrap_or_else(|| panic!("Item not found")));
```

**When it fires:** A pagination helper iterating over indexed entries encountered a map key without a corresponding value. This is a data-integrity violation in paginated access audit or member listing.

**Recovery:** Indicates storage corruption — open an incident.

---

## Category 2 — Authorisation Panics

These panics fire when an unauthenticated or unauthorised caller attempts a privileged operation, or when administrative state is missing.

### Panics and Their Meanings

#### `"Unauthorized"` — multiple contracts

```rust
// savings_goals/src/lib.rs
match current {
    None => {
        if caller != new_admin {
            panic!("Unauthorized");
        }
    }
    Some(ref admin) if admin != &caller => panic!("Unauthorized"),
    _ => {}
}
```

**When it fires:** The caller is not the current admin and is attempting a privileged operation (set pause admin, upgrade, pause/unpause).

**Recovery:** The caller must authenticate as the current admin. If no admin is set (bootstrap), the caller must be setting themselves as admin. Use `require_auth()` before calling.

#### `"No pause admin set"` — `savings_goals/src/lib.rs`

```rust
let admin = Self::get_pause_admin(&env)
    .unwrap_or_else(|| panic!("No pause admin set"));
```

**When it fires:** `pause()` or `unpause()` was called before a pause admin was configured via `set_pause_admin()`.

**Recovery:** Call `set_pause_admin(env, caller, admin_address)` first, where `caller == admin_address`.

#### `"No upgrade admin set"` — `savings_goals/src/lib.rs`

```rust
let admin = match Self::get_upgrade_admin(&env) {
    Some(a) => a,
    None => panic!("No upgrade admin set"),
};
```

**When it fires:** `set_version()`, `pre_upgrade()`, `restore_from_snapshot()`, or `discard_snapshot()` was called before an upgrade admin was configured.

**Recovery:** Call `set_upgrade_admin(env, caller, admin_address)` first, where `caller == admin_address`.

#### `"Only the goal owner can ..."` — `savings_goals/src/lib.rs`

```rust
if goal.owner != caller {
    panic!("Only the goal owner can lock this goal");
}
```

**When it fires:** A non-owner attempted to lock, unlock, archive, restore, add tags to, or remove tags from a goal.

**Recovery:** The caller must be the goal's registered owner. Verify you're using the correct signer address.

#### `"Only the bill owner can add tags"` / `"Only the bill owner can remove tags"` — `bill_payments/src/lib.rs`

**When it fires:** A non-owner attempted to add or remove tags on a bill.

**Recovery:** Same as goal owner checks — caller must be the bill's owner.

#### `"Only Owner or Admin can …"` — `family_wallet/src/lib.rs`

A pattern of panics guarding privileged family wallet operations:
- `"Only Owner or Admin can configure emergency settings"`
- `"Only Owner or Admin can change emergency mode"`
- `"Only Owner or Admin can add family members"`
- `"Only Owner or Admin can archive transactions"`
- `"Only Owner or Admin can view archived transactions"`
- `"Only Owner or Admin can cleanup expired transactions"`

```rust
if !Self::is_owner_or_admin(&env, &caller) {
    panic!("Only Owner or Admin can configure emergency settings");
}
```

**When it fires:** A caller who is neither Owner nor Admin attempted a privileged governance operation.

**Recovery:** Verify the caller has `FamilyRole::Owner` or `FamilyRole::Admin`. Use `is_owner_or_admin` to check before calling.

#### `"Cannot add Owner via add_family_member"` — `family_wallet/src/lib.rs`

```rust
if role == FamilyRole::Owner {
    panic!("Cannot add Owner via add_family_member");
}
```

**When it fires:** `add_family_member` was called with `FamilyRole::Owner` — the Owner role can only be set via `init()`.

**Recovery:** Use `init()` to establish the initial owner. For subsequent member additions, use `Member` or `Admin` roles.

#### `"Only Owner can remove family members"` / `"Cannot remove owner"` — `family_wallet/src/lib.rs`

```rust
if caller != owner {
    panic!("Only Owner can remove family members");
}
if member == owner {
    panic!("Cannot remove owner");
}
```

**When it fires:** A non-Owner attempted to remove a member, or someone attempted to remove the Owner.

**Recovery:** Only the contract Owner can call `remove_family_member`, and the Owner address itself can never be removed.

#### `"Only pause admin can pause"` / `"Only pause admin can unpause"` — `family_wallet/src/lib.rs`

```rust
if admin != caller {
    panic!("Only pause admin can pause");
}
```

**When it fires:** The family wallet's `pause()` or `unpause()` was called by someone who is not the configured pause admin (distinct from the contract Owner — the family wallet has its own pause admin role).

**Recovery:** Verify the caller matches the address set via `set_pause_admin`. The Owner can reassign the pause admin if needed.

#### `"Member not found"` — `family_wallet/src/lib.rs`

```rust
if members.get(member.clone()).is_none() {
    panic!("Member not found");
}
```

**When it fires:** `set_role_expiry` was called for a member address that does not exist in the `MEMBERS` map.

**Recovery:** Verify the member exists by calling `get_member` first. The member may need to be added via `add_family_member`.

#### `"Only family members can propose transactions"` — `family_wallet/src/lib.rs`

#### `"Only family members can propose transactions"` — `family_wallet/src/lib.rs`

```rust
if !Self::is_family_member(&env, &proposer) {
    panic!("Only family members can propose transactions");
}
```

**When it fires:** `propose_transaction` was called by an address not registered as a family member.

**Recovery:** Add the caller as a family member via `add_family_member` first.

#### `"Role has expired"` — `family_wallet/src/lib.rs`

```rust
if Self::role_has_expired(&env, &caller) {
    panic!("Role has expired");
}
```

**When it fires:** A member with an expiry timestamp set via `set_role_expiry` attempted an operation after `ledger.timestamp() >= expires_at`.

**Recovery:** The Owner or Admin must either extend the expiry via `set_role_expiry` or re-add the member.

#### `"Only upgrade admin can set version"` — `family_wallet/src/lib.rs`

```rust
if admin != caller {
    panic!("Only upgrade admin can set version");
}
```

**When it fires:** `set_version()` was called by someone who is not the configured upgrade admin.

**Recovery:** Verify the caller matches the upgrade admin address. The Owner can reassign via `set_upgrade_admin`.

#### `"Not a family member"` — `family_wallet/src/lib.rs`

```rust
if !Self::is_family_member(&env, &caller) {
    panic!("Not a family member");
}
```

**When it fires:** A governance operation (`check_governance`) was called by an address not registered as a family member.

**Recovery:** Add the caller via `add_family_member` first.

#### `"Insufficient role"` — `family_wallet/src/lib.rs`

```rust
if Self::role_ordinal(member.role) > Self::role_ordinal(min_role) {
    panic!("Insufficient role");
}
```

**When it fires:** A member's role ordinal is higher (less privileged) than the minimum required role for the operation.

**Recovery:** Promote the member to the required role via `add_family_member`.

#### `"Only Owner or Admin can perform this operation"` — `family_wallet/src/lib.rs`

```rust
if !Self::is_owner_or_admin(&env, &caller) {
    panic!("Only Owner or Admin can perform this operation");
}
```

**When it fires:** Generalised governance check — the caller is neither Owner nor Admin.

**Recovery:** Same as other Owner/Admin checks — verify the caller's `FamilyRole`.

#### `"Contract is paused"` (family wallet) — `family_wallet/src/lib.rs`

```rust
if Self::is_paused(&env) {
    panic!("Contract is paused");
}
```

**When it fires:** A state-modifying family wallet entrypoint was called while the contract is paused. The family wallet has its own pause mechanism independent of the global killswitch.

**Recovery:** Call `unpause()` — requires the family wallet's pause admin.

#### `"Only the policy owner can update this policy reference"` — `insurance/src/lib.rs`

```rust
if policy.owner != caller {
    panic!("Only the policy owner can update this policy reference");
}
```

**When it fires:** A non-owner attempted to update a policy's external reference.

**Recovery:** The caller must be the policy's registered owner.

---

## Category 3 — Input Validation Panics

These panics fire when invalid input is provided. They are intentional for functions that return `bool` or `()` and cannot propagate errors via `Result`.

> ⚠️ **Design note:** Where possible, prefer returning `Result<T, ContractError>` over panicking on bad input. Some of the panics below are legacy and should be migrated to typed errors in future refactors.

### Panics and Their Meanings

#### `"symbol input must contain between 1 and 32 characters after trimming"` — `remitwise-common/src/lib.rs`

```rust
pub fn canonicalise_symbol(env: &Env, input: &soroban_sdk::String) -> Symbol {
    let len = input.len();
    if len == 0 {
        panic!("symbol input must contain between 1 and 32 characters after trimming");
    }
    // ...
    let trimmed = s.trim();
    if trimmed_len == 0 || trimmed_len > 32 {
        panic!("symbol input must contain between 1 and 32 characters after trimming");
    }
```

**When it fires:** `canonicalise_symbol` received an empty string, a whitespace-only string, or a string exceeding 32 bytes after trimming.

**Recovery:** Ensure the input symbol is 1–32 non-whitespace bytes. Use `canonicalize_tags_checked` (the non-panicking variant) for untrusted input.

#### `"symbol input is too long"` — `remitwise-common/src/lib.rs`

**When it fires:** Input string exceeds the 256-byte internal buffer. This is a defence against extremely large inputs.

**Recovery:** Truncate or reject input exceeding 256 bytes before calling `canonicalise_symbol`.

#### `"symbol input is not valid UTF-8"` / `"canonicalised symbol is not valid UTF-8"`

**When it fires:** The input contains bytes that are not valid UTF-8. This should rarely fire since `soroban_sdk::String` is UTF-8 by definition.

**Recovery:** Verify the `String` was constructed correctly. This typically indicates a serialisation bug.

#### `"non-whitespace character"` — `remitwise-common/src/lib.rs`

**When it fires:** The canonicalisation function found only whitespace after trimming.

**Recovery:** Same as "symbol input must contain between 1 and 32 characters" — provide non-whitespace content.

#### `"Tags cannot be empty"` / `"Tag must be between 1 and 32 characters"` — `savings_goals/src/lib.rs`

```rust
fn validate_and_normalize_tags(env: &Env, tags: &Vec<String>) -> Vec<String> {
    match remitwise_common::canonicalize_tags_checked(env, tags) {
        Err(remitwise_common::TagError::Empty) => {
            if tags.is_empty() {
                panic!("Tags cannot be empty");
            }
            panic!("Tag must be between 1 and 32 characters");
        }
        Err(remitwise_common::TagError::TooLong) => {
            panic!("Tag must be between 1 and 32 characters");
        }
```

**When it fires:** A tag batch is empty or contains a zero-length or over-32-byte tag.

**Recovery:** Provide at least one tag, each 1–32 bytes in the allowed charset (`[a-z0-9\-_]`).

#### `"Invalid range: from ({from}) must be strictly less than to ({to})"` — `remitwise-common/src/lib.rs`

```rust
pub fn verify_ordered_pair(from: u64, to: u64) {
    if from >= to {
        panic!("Invalid range: from ({from}) must be strictly less than to ({to})");
    }
}
```

**When it fires:** A range where `from >= to` was passed to a function that requires strictly ordered bounds.

**Recovery:** Ensure `from < to`. This is typically a caller-side validation issue.

#### `"Invalid cursor"` — `savings_goals/src/lib.rs`

```rust
if cursor != 0 {
    if let Some(pos) = ids.iter().position(|id| id == cursor) {
        start_index = (pos as u32) + 1;
    } else {
        panic!("Invalid cursor");
    }
}
```

**When it fires:** A paginated query received a cursor that does not correspond to any goal in the owner's index. The cursor is not `0` (first page) and not a valid goal ID.

**Recovery:** Start with `cursor = 0` and only use `next_cursor` values returned by previous pages. Never construct cursors manually.

#### `"Contract is paused"` / `"Function is paused"` — `savings_goals/src/lib.rs`

```rust
fn require_not_paused(env: &Env, func: Symbol) {
    if Self::get_global_paused(env) {
        panic!("Contract is paused");
    }
    if Self::is_function_paused(env, func) {
        panic!("Function is paused");
    }
}
```

**When it fires:** A state-modifying entrypoint was called while the contract or that specific function is paused.

**Recovery:** The pause admin must call `unpause()` or `unpause_function()`. This is an operational control, not a bug.

---

## Category 4 — State Invariant Panics

These panics enforce business-rule invariants. They fire when an operation is attempted in a state that should prevent it.

#### `"Goal not found"` — `savings_goals/src/lib.rs`

**When it fires:** `lock_goal`, `unlock_goal`, `archive_goal`, or `restore_goal` targeted a non-existent goal ID.

**Recovery:** Verify `goal_id` exists by calling `get_goal(goal_id)` first.

#### `"Goal not completed"` — `savings_goals/src/lib.rs`

```rust
if goal.current_amount < goal.target_amount {
    panic!("Goal not completed");
}
```

**When it fires:** `archive_goal` was called on a goal that hasn't reached its target amount.

**Recovery:** Only archive completed goals (`current_amount >= target_amount`).

#### `"Goal already archived"` — `savings_goals/src/lib.rs`

**When it fires:** `archive_goal` was called on a goal that is already in the archived-goal store.

**Recovery:** Check archiving status before calling `archive_goal`.

#### `"Archived goal not found"` — `savings_goals/src/lib.rs`

```rust
let archived_goal = match env.storage().persistent()
    .get::<_, ArchivedSavingsGoal>(&DataKey::ArchivedGoal(goal_id)) {
    Some(g) => g,
    None => panic!("Archived goal not found"),
};
```

**When it fires:** `restore_goal` was called for a `goal_id` that does not exist in the archived-goal store.

**Recovery:** Verify the goal has actually been archived before restoring. Check with `get_archived_goals` first.

#### `"Archived pagination index out of sync"` / `"Archived pagination index owner mismatch"` — `savings_goals/src/lib.rs`

**When it fires:** Same data-integrity guards as the active-goal equivalents, but for archived goal pagination. The archived owner index references a goal that does not exist or belongs to a different owner.

**Recovery:** Indicates storage corruption — open an incident.

#### `"Active goal already exists"` — `savings_goals/src/lib.rs`

```rust
if env.storage().persistent().has(&DataKey::Goal(goal_id)) {
    panic!("Active goal already exists");
}
```

**When it fires:** `restore_goal` was called on an archived goal whose ID is already occupied in active storage.

**Recovery:** This indicates an ID collision — likely a bug in ID management.

#### `"Amount must be positive"` / `"Spending limit exceeded"` — `family_wallet/src/lib.rs`

```rust
if amount <= 0 {
    panic!("Amount must be positive");
}
if !Self::check_spending_limit(env.clone(), proposer.clone(), amount) {
    panic!("Spending limit exceeded");
}
```

**When it fires:** `withdraw` or `propose_emergency_transfer` was called with an amount ≤ 0, or the amount exceeded the caller's per-transaction spending limit.

**Recovery:** Pass a strictly positive amount and ensure it does not exceed the member's configured `spending_limit`. Check limits via `check_spending_limit` before calling.

#### `"Emergency max amount must be positive"` / `"Emergency min balance must be non-negative"` / `"Emergency daily limit must be non-negative"` — `family_wallet/src/lib.rs`

```rust
if max_amount <= 0 {
    panic!("Emergency max amount must be positive");
}
if min_balance < 0 {
    panic!("Emergency min balance must be non-negative");
}
if daily_limit < 0 {
    panic!("Emergency daily limit must be non-negative");
}
```

**When it fires:** `configure_emergency` received invalid emergency guardrail values.

**Recovery:** `max_amount` must be ≥ 1 stroop; `min_balance` and `daily_limit` must be ≥ 0.

#### `"Identical emergency transfer proposal already pending"` — `family_wallet/src/lib.rs`

```rust
if t == &token && r == &recipient && *a == amount {
    panic!("Identical emergency transfer proposal already pending");
}
```

**When it fires:** `propose_emergency_transfer` was called with the same `(token, recipient, amount)` tuple as an already-pending emergency proposal from the same proposer.

**Recovery:** Wait for the existing proposal to be executed, cancelled, or expired before submitting a duplicate. Use `get_pending_transactions_page` to inspect pending proposals.

#### `"Maximum pending emergency proposals reached"` — `family_wallet/src/lib.rs`

```rust
if active_proposals >= 1 {
    panic!("Maximum pending emergency proposals reached");
}
```

**When it fires:** A proposer already has one pending emergency transfer and attempts to create another.

**Recovery:** Only one emergency proposal is allowed per proposer at a time. Cancel or execute the existing one first.

#### `"Time-locked unpause not yet reached"` — `savings_goals/src/lib.rs`

```rust
if let Some(at) = unpause_at {
    if env.ledger().timestamp() < at {
        panic!("Time-locked unpause not yet reached");
    }
}
```

**When it fires:** `unpause()` was called before the time-lock window expired. The pause admin can set a future `unpause_at` timestamp to enforce a mandatory cooling-off period.

**Recovery:** Wait until `env.ledger().timestamp() >= unpause_at`, then call `unpause()` again.

#### `"Unauthorized: bootstrap requires caller == new_admin"` / `"Unauthorized: only current upgrade admin can transfer"` — `savings_goals/src/lib.rs`

```rust
match &current_upgrade_admin {
    None => {
        if caller != new_admin {
            panic!("Unauthorized: bootstrap requires caller == new_admin");
        }
    }
    Some(ref current_admin) => {
        if *current_admin != caller {
            panic!("Unauthorized: only current upgrade admin can transfer");
        }
    }
}
```

**When it fires:** Bootstrap or transfer of the upgrade admin role was attempted by an unauthorised caller. Bootstrap requires `caller == new_admin`; transfer requires the current admin.

**Recovery:** For bootstrap, the caller must set themselves as admin. For transfer, only the current upgrade admin can reassign the role.

#### `"Unlock date must be in the future"` — `savings_goals/src/lib.rs`

```rust
if unlock_date <= env.ledger().timestamp() {
    panic!("Unlock date must be in the future");
}
```

**When it fires:** `set_time_lock` was called with an `unlock_date` that is in the past or equal to the current ledger timestamp.

**Recovery:** Provide a future `unlock_date` (strictly greater than `env.ledger().timestamp()`).

#### `"Only the goal owner can set time-lock"` — `savings_goals/src/lib.rs`

**When it fires:** `set_time_lock` was called by someone other than the goal's owner.

**Recovery:** The caller must be the goal's registered owner.

### Emergency Transfer Guardrails (Family Wallet)

#### `"Emergency config not set"` — `family_wallet/src/lib.rs`

```rust
let config = env.storage().instance().get(&EMERGENCY_CONFIG_KEY)
    .unwrap_or_else(|| panic!("Emergency config not set"));
```

**When it fires:** `propose_emergency_transfer` was called before `configure_emergency`.

**Recovery:** Call `configure_emergency` first to set up emergency guardrails.

#### `"Emergency amount exceeds maximum allowed"` — `family_wallet/src/lib.rs`

**When it fires:** An emergency transfer proposal exceeds `EmergencyConfig.max_amount`.

**Recovery:** Reduce the transfer amount below the configured maximum, or increase `max_amount` via `configure_emergency`.

#### `"Emergency daily limit exceeded"` — `family_wallet/src/lib.rs`

**When it fires:** The cumulative emergency transfer volume for the current day exceeds `EmergencyConfig.daily_limit`.

**Recovery:** Wait until the next day (daily volume resets at midnight UTC), or increase `daily_limit` via `configure_emergency`.

#### `"Emergency volume arithmetic overflow"` — `family_wallet/src/lib.rs`

**When it fires:** Arithmetic overflow while accumulating daily emergency transfer volume. This should be extremely rare.

**Recovery:** Indicates an edge case in volume tracking — open an incident.

#### `"Emergency transfer cooldown period not elapsed"` — `family_wallet/src/lib.rs`

**When it fires:** A second emergency transfer was proposed before the cooldown period from the previous emergency transfer elapsed.

**Recovery:** Wait until `ledger.timestamp() >= last_emergency_at + cooldown_seconds` before proposing another emergency transfer.

#### `"Transaction tier mismatch: invalid multisig enforcement"` — `family_wallet/src/lib.rs`

```rust
if resolved_tx_type != expected_tx_type {
    panic!("Transaction tier mismatch: invalid multisig enforcement");
}
```

**When it fires:** The transaction type resolved from the withdrawal amount does not match the expected type — the multisig enforcement tier is inconsistent.

**Recovery:** Indicates a logic bug in the withdrawal tier calculation. Investigate the amount thresholds and configured multisig tiers.

#### `"Snapshot owner mismatch"` — `family_wallet/src/lib.rs`

```rust
if snapshot.owner != owner {
    panic!("Snapshot owner mismatch");
}
```

**When it fires:** `restore_from_snapshot` was called but the stored snapshot's owner does not match the current caller.

**Recovery:** Only the original snapshot creator can restore. Verify the caller address matches.

---

## Category 5 — `panic_with_error!()`  (Typed Contract Errors)

`soroban_sdk::panic_with_error!` is the Soroban host's mechanism for aborting a transaction with a typed `#[contracterror]`. Unlike `panic!()`, the host surfaces the error variant to the caller, who can match on it.

### Where and Why

#### `SavingsGoalError::InvalidTagContent` — `savings_goals/src/lib.rs`

```rust
Err(remitwise_common::TagError::InvalidChar { .. }) => {
    soroban_sdk::panic_with_error!(env, SavingsGoalError::InvalidTagContent)
}
```

**When it fires:** A tag contains a character outside `[a-z0-9\-_]` (after case-folding).

**Recovery:** Use only lowercase ASCII letters, digits, hyphens, and underscores in tags.

#### `SavingsGoalError::SnapshotNotFound` — `savings_goals/src/lib.rs`

```rust
let snapshot: PreUpgradeSnapshot = env.storage().persistent()
    .get(&SNAPSHOT_KEY)
    .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::SnapshotNotFound));
```

**When it fires:** `restore_from_snapshot` was called but no pre-upgrade snapshot exists (no prior `pre_upgrade` call, or snapshot was already consumed).

**Recovery:** Call `pre_upgrade` before the upgrade, then `restore_from_snapshot` if rollback is needed.

#### `SavingsGoalError::UnsupportedVersion` — `savings_goals/src/lib.rs`

**When it fires:** The stored snapshot's `schema_version` does not match `SNAPSHOT_VERSION`.

**Recovery:** Snapshots from older contract versions may be incompatible. Re-deploy the matching contract version or write a migration.

#### `SavingsGoalError::SnapshotTooOld` — `savings_goals/src/lib.rs`

**When it fires:** The snapshot's `snapshot_taken_at` timestamp exceeds `SNAPSHOT_MAX_AGE_SECS` (30 days).

**Recovery:** Take a fresh snapshot before each upgrade. Stale snapshots are rejected to prevent restoring outdated state.

#### `SavingsGoalError::TimeLockShortening` — `savings_goals/src/lib.rs`

```rust
if new_unlock_date < current_unlock_date {
    soroban_sdk::panic_with_error!(env, SavingsGoalError::TimeLockShortening);
}
```

**When it fires:** An attempt to shorten an active time-lock's `unlock_date`. Time-locks are monotonic — they may be extended forward but never shortened backward.

**Recovery:** Only set `unlock_date` to a value **greater than or equal to** the current unlock date.

#### `BillPaymentsError::InvalidTagContent` — `bill_payments/src/lib.rs`

```rust
Err(remitwise_common::TagError::InvalidChar { .. }) => {
    soroban_sdk::panic_with_error!(env, BillPaymentsError::InvalidTagContent)
}
```

**When it fires:** Same as `SavingsGoalError::InvalidTagContent` — a tag contains an invalid character.

**Recovery:** Same as savings goals — use only `[a-z0-9\-_]`.

#### Family Wallet Errors — `family_wallet/src/lib.rs`

**`panic!("Transaction not found")`** (bare panic) — in `sign_transaction()`:
```rust
let mut pending_tx = pending_txs.get(tx_id)
    .unwrap_or_else(|| panic!("Transaction not found"));
```

**`panic_with_error!(&env, Error::TransactionNotFound)`** (typed) — in `cancel_transaction()`:
```rust
let pending_tx = pending_txs.get(tx_id).unwrap_or_else(|| {
    panic_with_error!(&env, Error::TransactionNotFound);
});
```

**When they fire:** `sign_transaction` or `cancel_transaction` targeted a `tx_id` that does not exist in the pending transactions map. The two functions use different panic mechanisms — `sign_transaction` uses a bare `panic!()` while `cancel_transaction` uses the typed `panic_with_error!`.

**Recovery:** Verify the transaction ID exists in `PEND_TXS` via `get_pending_transaction`. Bare panic in `sign_transaction` should be migrated to a typed error:
- `TransactionNotFound`: Verify the transaction ID exists in pending transactions.
- `Unauthorized`: Caller is not authorised for the multisig operation.
- `InvalidProposalExpiry`: The proposal's expiry timestamp is in the past or invalid.

#### `Error::MinBalanceViolation` — `family_wallet/src/lib.rs`

```rust
.unwrap_or_else(|| panic_with_error!(&env, Error::MinBalanceViolation));
panic_with_error!(&env, Error::MinBalanceViolation);
```

**When it fires:** An emergency transfer would cause the wallet balance to fall below `EmergencyConfig.min_balance`.

**Recovery:** Reduce the transfer amount or lower the `min_balance` guardrail via `configure_emergency`.

#### `Error::SnapshotTooOld` — `family_wallet/src/lib.rs`

```rust
panic_with_error!(&env, Error::SnapshotTooOld);
```

**When it fires:** `restore_from_snapshot` was called but the stored pre-upgrade snapshot's timestamp exceeds the freshness window.

**Recovery:** Take a fresh snapshot via `pre_upgrade()` before restoring.

### Other contracts

#### `"unexpected error: {:?}"` / `"cycle {}: import failed"` / `"cycle {}: export failed"` — `data_migration/src/lib.rs`

```rust
Err(e) => panic!("unexpected error: {:?}", e),
.unwrap_or_else(|_| panic!("cycle {}: import failed", cycle));
.unwrap_or_else(|_| panic!("cycle {}: export failed", cycle));
```

**When they fire:** During multi-cycle migration round-trip tests (import→export→import→export), a cycle failed. These panics are in test/tooling code paths within the `data_migration` crate.

**Recovery:** These should only fire during development. Check the migration format compatibility and that the import/export implementation is correct for the given payload version.

#### `"invalid Symbol character"` — `remitwise-common/src/lib.rs`

```rust
if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
    panic!("invalid Symbol character");
}
```

**When it fires:** One of the duplicated `canonicalise_symbol` function variants encountered a character outside `[a-z0-9_]` after trimming and case-folding.

**Recovery:** Provide input using only ASCII lowercase letters, digits, and underscores. Use `canonicalize_tags_checked` for non-panicking validation.

#### `"Pause channel is inactive"` — `remitwise-common/src/lib.rs`

**When it fires:** A per-channel pause check was performed on a channel that is not active. Used in shared pause management helpers.

**Recovery:** Verify the pause channel name matches a registered channel. Use `get_pause_state()` to inspect active pause channels.

#### `"unexpected Val for encoding: {val:?}"` — `remitwise-common/src/lib.rs`

**When it fires:** The `RemitwiseEvents::emit` helper encountered a `Val` it could not encode into the event data buffer during event emission.

**Recovery:** Check that the event payload type derives `IntoVal` correctly. This typically indicates a type mismatch in event data construction.

---

## Category 6 — Cryptographic Panics

#### `env.crypto().ed25519_verify()` host panic

```rust
// remitwise-common/src/lib.rs
env.crypto().ed25519_verify(&pk_bytes, &msg_bytes, &sig_bytes);
```

**When it fires:** The Soroban host's Ed25519 verifier panics when a signature is invalid. This is host-level behaviour — the contract cannot catch it.

**Recovery:** Two approaches:

1. **Pre-check (recommended):** Call `require_registered_verifier` and validate signature/public-key lengths before calling `ed25519_verify`. Invalid-length keys/signatures return typed `SignatureError` variants instead of panicking.

2. **Accept the panic:** For signature verification in mutating entrypoints where an invalid signature is indeed a fatal error, the panic is acceptable. The transaction aborts, no state is changed, and the caller can retry with a valid signature.

**Important nuance:** In `verify_signature`, the function first calls `ed25519_verify` (which may panic on invalid sig), then also does a length-delimited domain-separated verification using the same host call. If the first call panics, the second is never reached. The length pre-checks (`try_into` for `[u8; 32]` and `[u8; 64]`) return `SignatureError::InvalidPublicKeyLength` / `InvalidSignatureLength` *before* the host call.

---

## Category 7 — Precondition Assertions (`assert!`)

These are `assert!()` / `assert_eq!()` calls in production code that validate internal preconditions. They are **always active** in release builds — if violated, they abort the transaction immediately.

#### `distribute_pro_rata` preconditions — `remitwise-common/src/lib.rs`

```rust
pub fn distribute_pro_rata(total: i128, weights: &[u32], total_weight: u32, out: &mut [i128]) {
    assert!(total >= 0, "total must be non-negative");
    assert!(total_weight > 0, "total_weight must be positive");
    assert!(!out.is_empty(), "out must not be empty");
    assert!(!weights.is_empty(), "weights must not be empty");
    assert_eq!(weights.len(), out.len(), "weights and out must have the same length");
```

**When they fire:** The caller passed invalid arguments to `distribute_pro_rata`.

**Recovery:**
- Ensure `total >= 0`
- Ensure `total_weight > 0`
- Ensure `weights` and `out` are non-empty and have the same length
- Each weight must be `<= total_weight`

These preconditions are documented in the function's doc comment. Callers should validate input before calling.

---

## Category 8 — Schedule Execution Panics

These panics are specific to the **savings goals scheduled execution** feature. They fire during `create_schedule`, `modify_schedule`, `cancel_schedule`, and `execute_due_schedules`.

### Panics and Their Meanings

#### `"Schedule not found"` — `savings_goals/src/lib.rs`

```rust
let schedule = env.storage().persistent()
    .get::<_, SavingsSchedule>(&DataKey::Schedule(schedule_id))
    .unwrap_or_else(|| panic!("Schedule not found"));
```

**When it fires:** `modify_schedule` or `cancel_schedule` targeted a non-existent `schedule_id`.

**Recovery:** Verify `schedule_id` exists via `get_schedule`. Ensure the schedule was created and hasn't been cancelled.

#### `"Only the goal owner can create schedules"` / `"Only the schedule owner can modify it"` / `"Only the schedule owner can cancel it"` — `savings_goals/src/lib.rs`

```rust
if goal.owner != caller {
    panic!("Only the goal owner can create schedules");
}
if schedule.owner != caller {
    panic!("Only the schedule owner can modify it");
}
if schedule.owner != caller {
    panic!("Only the schedule owner can cancel it");
}
```

**When they fire:** A non-owner attempted to create, modify, or cancel a scheduled contribution.

**Recovery:** The caller must be the goal/schedule's registered owner. Verify the signer address.

#### `"Amount must be positive"` (schedule) — `savings_goals/src/lib.rs`

**When it fires:** `create_schedule` or `modify_schedule` was called with `amount <= 0`.

**Recovery:** Pass a strictly positive contribution amount.

#### `"Next due date must be in the future"` — `savings_goals/src/lib.rs`

```rust
if next_due_date <= env.ledger().timestamp() {
    panic!("Next due date must be in the future");
}
```

**When it fires:** `create_schedule` or `modify_schedule` was called with a `next_due_date` that is in the past.

**Recovery:** Provide a future `next_due_date` (strictly greater than `env.ledger().timestamp()`).

#### `"Invalid nonce: expected {}, got {}"` — `savings_goals/src/lib.rs`

```rust
if current != expected {
    panic!("Invalid nonce: expected {}, got {}", expected, current);
}
```

**When it fires:** `execute_due_schedules` detected a nonce mismatch between the stored execution counter and the expected value. The nonce is a monotonic counter that prevents replay attacks.

**Recovery:** This is a defence-in-depth guard — if it fires, there may be a concurrent execution race or a replay attempt. Check for parallel transaction submissions.

#### `"nonce overflow"` — `savings_goals/src/lib.rs`

```rust
let next = current.checked_add(1).unwrap_or_else(|| panic!("nonce overflow"));
```

**When it fires:** The execution nonce counter overflowed `u64::MAX`. This is practically impossible under normal operation.

**Recovery:** Indicates either an astronomical number of schedule executions or a logic bug — open an incident.

---

## Category 9 — Batch Operation Panics

These panics fire during **batch member operations** in the family wallet (`add_family_members_batch`, `remove_family_members_batch`).

### Panics and Their Meanings

#### `"Batch too large"` — `family_wallet/src/lib.rs`

```rust
if members.len() > MAX_BATCH_SIZE {
    panic!("Batch too large");
}
```

**When it fires:** A batch add or remove operation exceeded `MAX_BATCH_SIZE` (defined in `remitwise-common`).

**Recovery:** Split the operation into smaller batches, each ≤ `MAX_BATCH_SIZE`.

#### `"Cannot add Owner via batch"` — `family_wallet/src/lib.rs`

```rust
if role == FamilyRole::Owner {
    panic!("Cannot add Owner via batch");
}
```

**When it fires:** `add_family_members_batch` included a member with `FamilyRole::Owner`. The Owner role can only be set via `init()`.

**Recovery:** Remove any `Owner`-role entries from the batch. Use `init()` for owner setup.

#### `"Duplicate member in batch"` — `family_wallet/src/lib.rs`

```rust
if seen.contains(&address) {
    panic!("Duplicate member in batch");
}
```

**When it fires:** The same address appears more than once in a single batch.

**Recovery:** De-duplicate the member list before submitting.

#### `"Member already exists"` — `family_wallet/src/lib.rs`

```rust
if members.contains_key(&address) {
    panic!("Member already exists");
}
```

**When it fires:** A batch add operation included an address that is already a family member.

**Recovery:** Remove existing members from the batch. Use `get_member` to check membership first.

#### `"Member cap exceeded"` — `family_wallet/src/lib.rs`

```rust
if members.len() + batch.len() > MAX_MEMBERS {
    panic!("Member cap exceeded");
}
```

**When it fires:** Adding the batch would exceed the maximum number of family members.

**Recovery:** Reduce the batch size or remove existing members first.

#### `"Only Owner can remove members"` — `family_wallet/src/lib.rs`

```rust
if caller != owner {
    panic!("Only Owner can remove members");
}
```

**When it fires:** `remove_family_members_batch` was called by a non-Owner.

**Recovery:** Only the contract Owner can batch-remove members.

#### `"Cannot remove owner"` (batch) — `family_wallet/src/lib.rs`

```rust
if member == owner {
    panic!("Cannot remove owner");
}
```

**When it fires:** A batch remove included the Owner address.

**Recovery:** The Owner cannot be removed. Filter the Owner out of the batch.

---

## View Function No-Panic Rule

**Rule:** `get_*` and `is_*` functions must **never** panic unconditionally.

**Why:** View functions are called by off-chain indexers, RPC queries, and frontends. A panic in a view function:

1. Makes the view unreachable for **all** callers until state is repaired.
2. Consumes the caller's gas budget despite being read-only.
3. Breaks indexers that rely on view functions for state snapshots.

**Enforcement:** CI (`testutils/tests/no_panic_in_view_fn_test.rs`) scans every workspace contract's `lib.rs` and flags:

- `.unwrap()` (bare)
- `.expect("...")`
- `panic!(...)`
- `unreachable!(...)`
- `panic_with_error!(...)`

**Safe alternatives:**
```rust
// ❌ BANNED in view functions
let val = env.storage().instance().get(&KEY).unwrap();

// ✅ ALLOWED — safe fallback
let val = env.storage().instance().get(&KEY).unwrap_or(default);
let val = env.storage().instance().get(&KEY).unwrap_or_else(|| compute_default());
```

**What's NOT flagged:** `.unwrap_or()`, `.unwrap_or_default()`, `.unwrap_or_else()` — these handle `None`/`Err` without panicking.

For full details, see:
- [ADR: Ban unwrap in Release Builds](adr-ban-unwrap-in-release.md)
- CI test: `testutils/tests/no_panic_in_view_fn_test.rs`
- CI test: `testutils/tests/view_fn_readonly_test.rs`

---

## Test-Expected Panics (`#[should_panic]`)

Tests annotated with `#[should_panic]` verify that specific inputs cause a panic. These are **not bugs** — they are assertions that the contract correctly rejects invalid input.

### Common `#[should_panic]` tests

| Test | Expected panic | What it validates |
|---|---|---|
| `test_init_twice_panics` | `"Wallet already initialized"` | Idempotent `init()` |
| `test_unauthorized_pause_panics` | `"Unauthorized"` | Pause admin auth check |
| `test_auth_failure` | `"HostError: Error(Auth, InvalidAction)"` | `require_auth()` enforcement |
| `test_invalid_symbol_chars` | `"invalid char: ..."` | Tag/symbol charset validation |
| `test_event_size_limit` | `"exceeds 256-byte budget"` | Event payload size cap |
| `test_invalid_cursor` | panic (no message) | Pagination cursor validation |
| `test_bulk_cleanup_invariants` | `"HostError"` | Bulk cleanup guard |
| `test_negative` | `"Error(Contract, #1)"` | Contract error propagation |

These tests are in `#[cfg(test)]` blocks or `tests/` directories and do not affect production behaviour.

---

## Recovery Cheat Sheet

| Symptom | Likely Cause | First Check |
|---|---|---|
| "Wallet not initialized" | Missing `init()` call | Was `init()` called? Is TTL expired? |
| "Wallet already initialized" | Double `init()` | Call-site logic |
| "Multi-sig config not found" / "Pending transactions map not initialized" | Missing `init()` | Call `init()` first |
| "Contract is paused" / "Function is paused" | Pause admin action | Check `is_paused()` / `get_pause_state()` |
| "Unauthorized" | Wrong signer | Verify signer address; check `require_auth()` |
| "Only Owner or Admin can …" / "Only Owner can …" | Insufficient role | Check caller's `FamilyRole` |
| "Role has expired" | Expiry timestamp passed | Extend expiry via `set_role_expiry` |
| "Amount must be positive" / "Emergency max amount must be positive" | Invalid input | Pass strictly positive amount |
| "Spending limit exceeded" | Per-tx cap hit | Check `check_spending_limit` before calling |
| "Identical emergency transfer proposal already pending" | Duplicate proposal | Cancel or execute existing proposal first |
| "No pause admin set" / "No upgrade admin set" | Missing bootstrap | Call `set_pause_admin` / `set_upgrade_admin` |
| "Goal not found" / "Bill not found" | Invalid ID | Check existence with `get_goal` / `get_bill` |
| "Invalid cursor" | Bad pagination cursor | Start fresh with `cursor = 0` |
| "Pagination index out of sync" / "Tag index out of sync" | State corruption bug | Incident — investigate recent write operations |
| "Tags cannot be empty" / "Tag must be 1–32" | Invalid tag input | Validate tags at the call site |
| "Inconsistent executed/pending transaction metadata" | Storage corruption | Incident — investigate recent write operations |
| "Member not found" (family wallet) | Non-existent member | Check with `get_member` first |
| `SavingsGoalError::InvalidTagContent` (typed) | Invalid tag characters | Use only `[a-z0-9\-_]` |
| `SavingsGoalError::SnapshotNotFound` (typed) | Missing pre-upgrade snapshot | Call `pre_upgrade()` before upgrade |
| `SavingsGoalError::TimeLockShortening` (typed) | Shortened time-lock | Only extend `unlock_date` forward |
| `Error::MinBalanceViolation` (typed) | Emergency transfer below `min_balance` | Reduce amount or lower `min_balance` |
| `Error::SnapshotTooOld` (typed) | Stale pre-upgrade snapshot | Call `pre_upgrade()` first |
| "Contract addresses not configured" | Missing `init()` in reporting | Call `init()` with all contract addresses |
| "Time-locked unpause not yet reached" | Cooling-off period active | Wait until `timestamp >= unpause_at` |
| `ed25519_verify` panic | Bad signature | Check key/sig lengths first; use `require_registered_verifier` |
| "Archived goal not found" | Non-existent archived goal ID | Verify the goal was actually archived |
| "Only pause admin can pause/unpause" (family wallet) | Wrong pause admin | Check `get_pause_admin` / use `set_pause_admin` |
| "Only upgrade admin can set version" | Wrong upgrade admin | Check upgrade admin address |
| "Not a family member" / "Insufficient role" | Wrong role level | Check caller's `FamilyRole` |
| "Policy not found" (insurance) | Invalid policy ID | Check with `get_policy` first |
| "Schedule not found" (insurance) | Invalid schedule ID | Check with `get_schedule` first |
| "Schedule not found" (savings) | Invalid schedule ID | Check with `get_schedule` first |
| "Only the goal owner can create schedules" / "Only the schedule owner can modify/cancel it" | Wrong signer | Verify schedule owner |
| "Next due date must be in the future" | Past `next_due_date` | Pass a future timestamp |
| "Unlock date must be in the future" | Past `unlock_date` | Pass a future timestamp |
| "Only the goal owner can set time-lock" | Wrong signer | Verify goal owner |
| "Emergency config not set" | Missing `configure_emergency` | Call `configure_emergency` first |
| "Emergency amount exceeds maximum allowed" | Amount > `max_amount` | Reduce amount or increase `max_amount` |
| "Emergency daily limit exceeded" | Daily volume cap hit | Wait or increase `daily_limit` |
| "Emergency transfer cooldown period not elapsed" | Cooldown active | Wait until cooldown elapses |
| "Transaction tier mismatch: invalid multisig enforcement" | Withdrawal tier bug | Investigate amount thresholds |
| "Snapshot owner mismatch" | Wrong caller for restore | Verify caller matches snapshot owner |
| "Batch too large" | Batch > `MAX_BATCH_SIZE` | Split into smaller batches |
| "Cannot add Owner via batch" | Owner role in batch | Remove Owner entries; use `init()` |
| "Duplicate member in batch" | Same address twice | De-duplicate batch |
| "Member already exists" | Existing member in batch | Remove existing members from batch |
| "Member cap exceeded" | Too many members | Reduce batch size or remove members |
| "Only Owner can remove members" (batch) | Non-Owner caller | Only Owner can batch-remove |
| "Invalid nonce" / "nonce overflow" | Schedule execution nonce | Check for parallel submissions |
| "Only the policy owner can update this policy reference" | Wrong signer | Verify policy owner |
| `#[should_panic]` in tests | Expected — not a bug | Verify the panic message matches |

---

## Related Documentation

- [ADR: Ban unwrap in Release Builds](adr-ban-unwrap-in-release.md) — Why `unwrap()` and `expect()` are forbidden in production code.
- [Contributor Overview](CONTRIBUTOR_OVERVIEW.md) — Onboarding guide for new contributors.
- [Authorization Matrix](AUTHORIZATION_MATRIX.md) — Per-entrypoint caller authorization requirements.
- [Pause Playbook](PAUSE_PLAYBOOK.md) — Emergency pause mechanisms and recovery procedures.
- [Pagination Handbook](PAGINATION_HANDBOOK.md) — Cursor semantics and pagination invariants.
- [Threat Model](THREAT_MODEL.md) — STRIDE-style threat analysis per contract entrypoint.
- [Security Review Summary](../SECURITY_REVIEW_SUMMARY.md) — Known security gaps and mitigations.
