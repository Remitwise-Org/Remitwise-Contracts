# Access Control Matrix - Remitwise Contracts

## Overview

This document provides a comprehensive access-control matrix mapping each public method across all contracts to its required caller (owner/admin/anyone/other contract). It also identifies risky functions requiring tighter controls and documents cross-contract call constraints.

---

## 1. Bill Payments Contract

| Public Method | Required Caller | Access Control Details |
|--------------|-----------------|------------------------|
| `create_bill` | Owner | Owner must authorize (`owner.require_auth()`). Validates amount > 0. |
| `pay_bill` | Owner | Owner must authorize. Must own the bill. Bill must not be paid. |
| `get_bill` | Anyone | No auth required. Returns Option<Bill>. |
| `get_unpaid_bills` | Owner | `owner.require_auth()`. Paginated (`cursor`, `limit`); `limit` clamped via `clamp_limit` (0 → `DEFAULT_PAGE_LIMIT`=20, max `MAX_PAGE_LIMIT`=50). Returns `BillPage`. |
| `get_owner_bill_count` | Anyone | No auth. O(1) read of the owner's active-bill index; bounded by `MAX_BILLS_PER_OWNER` (1000). |
| `get_all_bills_for_owner` | Owner | `owner.require_auth()`. Now paginated (`cursor`, `limit`, clamped via `clamp_limit`) — signature changed from the legacy unbounded form. Returns `BillPage` (paid + unpaid). |
| `get_overdue_bills` | Anyone | No auth. Paginated (`cursor`, `limit`) across all owners; unpaid + `due_date < now`. |
| `get_overdue_bills_for_owner` | Owner | `owner.require_auth()`. Paginated (`cursor`, `limit`) version scoped to one owner; O(owner_bills) via `OWN_IDX`. |
| `get_all_bills_page` | Admin | `caller.require_auth()`; caller must equal the pause admin (`Self::get_pause_admin`) or the call returns `Unauthorized`. Paginated (`cursor`, `limit`) replacement for the old unbounded `get_all_bills`, which no longer exists in code. |
| `cancel_bill` | Owner | Owner must authorize. Must own the bill. |
| `archive_paid_bills` | Owner | Owner must authorize. Requires not paused. |
| `restore_bill` | Owner | Owner must authorize. Must own archived bill. |
| `bulk_cleanup_bills` | Owner | Owner must authorize. Admin-level cleanup. |
| `batch_pay_bills` | Owner | Owner must authorize. Batch processing of bill payments. |
| `get_total_unpaid` | Anyone | No auth. Returns unpaid total for owner. |
| `get_storage_stats` | Anyone | No auth. Returns StorageStats. |
| `get_bills_by_currency` | Anyone | No auth. Paginated (`cursor`, `limit`, clamped). Filtered by owner and currency (case/whitespace-insensitive; empty currency defaults to "XLM"). Uses per-owner currency index. |
| `get_unpaid_bills_by_currency` | Anyone | No auth. Paginated (`cursor`, `limit`, clamped). Filtered by owner, currency, unpaid status. |
| `get_total_unpaid_by_currency` | Anyone | No auth. Sum of unpaid bills in specific currency (saturating addition). |
| `get_archived_bills` | Anyone | No auth (no `require_auth()` call) — filtered by `owner` param only, so any caller can page through any owner's archive by supplying their address. Paginated (`cursor`, `limit`, clamped). |
| `get_archived_bills_page` | Anyone | No auth. Same shape/semantics as `get_archived_bills`, reading via the `ARCH_IDX` per-owner index; O(clamp_limit(limit)) regardless of total archive size. |
| `get_archived_bill` | Anyone | No auth. Returns specific archived bill. |
| `get_all_unpaid_bills_legacy` | Anyone | No auth. **Legacy/unbounded**: returns *all* unpaid bills for `owner` in a single `Vec` (no pagination), scanning `1..=NEXT_ID`. Doc comment explicitly says "only safe for owners with a small number of bills" — see Risky Functions note below. |
| **Pause Functions** |||
| `set_pause_admin` | Initial: Owner Subsequent: Admin | Auth required. Validates caller is current admin. |
| `pause` | Admin | Pause admin only. |
| `unpause` | Admin | Pause admin only. Can have time-lock. |
| `schedule_unpause` | Admin | Admin only. Validates future timestamp. |
| `pause_function` | Admin | Pause admin only. Function-level pause. |
| `unpause_function` | Admin | Pause admin only. |
| `emergency_pause_all` | Admin | Pause admin only. Pauses entire contract. |
| `is_paused` | Anyone | No auth. |
| `is_function_paused_public` | Anyone | No auth. |
| `get_pause_admin_public` | Anyone | No auth. |
| **Upgrade Functions** |||
| `set_upgrade_admin` | Initial: Owner Subsequent: Upgrade Admin | Validates caller is current admin. |
| `set_version` | Upgrade Admin | Upgrade admin only. |
| `get_version` | Anyone | No auth. |

### Risky Functions - Bill Payments
- **`get_all_bills_page`** (formerly `get_all_bills`, which no longer exists): Admin-only access to all bills across all owners, now paginated. Could expose sensitive data.
- **`archive_paid_bills` / `bulk_cleanup_bills`**: Bulk operations that modify storage. Should require additional confirmations for large batches.
- **`emergency_pause_all`**: Can disable entire contract. Should have time-lock.
- **`get_all_unpaid_bills_legacy`**: Unbounded `Vec<Bill>` return with no `cursor`/`limit` and no auth check — scans `1..=NEXT_ID` (up to `MAX_BILLS_PER_OWNER` per owner across the whole contract). Flagged as a genuine gas/DoS and stale-callers risk; the doc comment itself says to prefer the paginated `get_unpaid_bills`. Should be removed or hard-deprecated.
- **`get_archived_bills` / `get_archived_bills_page`**: No `require_auth()` — any address can page through any other owner's archived-bill history by passing their address as the `owner` parameter. Same anyone-can-read-by-owner-param pattern as several other "public by design" getters, but worth a second look given archived bills persist longer.

---

## 2. Family Wallet Contract

| Public Method | Required Caller | Access Control Details |
|--------------|-----------------|------------------------|
| `init` | Owner | Owner must authorize. One-time initialization. |
| `add_member` | Admin | Admin must authorize. Validates role != Owner. |
| `get_member` | Anyone | No auth. Returns member if exists. |
| `update_spending_limit` | Admin | Admin must authorize. Can update any member's limit. |
| `check_spending_limit` | Anyone | No auth. Returns bool for spending permission. |
| `configure_multisig` | Owner/Admin | Auth required. Configures transaction thresholds. |
| `propose_transaction` | Member | Family member must authorize. Creates pending tx. |
| `sign_transaction` | Member | Family member must authorize. Signs pending tx. |
| `withdraw` | Member | Auth required. Proposes withdrawal tx. |
| `propose_split_config_change` | Member | Auth required. Proposes config change. |
| `propose_role_change` | Member | Auth required. Proposes role change. |
| `propose_emergency_transfer` | Member | Auth required. Can bypass multisig in emergency mode. |
| `propose_policy_cancellation` | Member | Auth required. Proposes policy cancellation. |
| `configure_emergency` | Owner/Admin | Auth required. Sets emergency config. |
| `set_emergency_mode` | Owner/Admin | Auth required. Toggles emergency mode. |
| `add_family_member` | Owner/Admin | Auth required. Adds member to wallet. |
| `remove_family_member` | Owner | Owner only. Cannot remove self. |
| `get_pending_transaction` | Anyone | No auth. Returns pending tx if exists. |
| `get_pending_transactions_page` | Member | `caller.require_auth()`. Paginated (cursor = last-seen `tx_id`, `limit` clamped to `MAX_PENDING_PAGE_LIMIT`=100, default 20). Owner/Admin see all pending proposals; regular members only see proposals they personally created (`tx.proposer == caller`). |
| `get_multisig_config` | Anyone | No auth. Returns config for tx type. |
| `get_family_member` | Anyone | No auth. Returns member details. |
| `get_member_addresses_page` | Anyone | No auth. Paginated (`cursor`, `limit` clamped to `MAX_MEMBER_PAGE_LIMIT`=100, default 20) list of member addresses. |
| `get_owner` | Anyone | No auth. Returns wallet owner. |
| `get_emergency_config` | Anyone | No auth. Returns emergency settings. |
| `is_emergency_mode` | Anyone | No auth. Returns bool. |
| `get_last_emergency_at` | Anyone | No auth. Returns last emergency timestamp. |
| `archive_old_transactions` | Owner/Admin | Auth required. Archives executed txs. |
| `get_archived_transactions` | Owner/Admin | `caller.require_auth()`; panics unless `is_owner_or_admin(caller)`. **Updated from prior matrix**: this row previously listed no auth — the current signature is `(env, caller, limit)` and does enforce Owner/Admin. `limit` clamped to `MAX_ARCHIVE_PAGE_LIMIT` (100), default `DEFAULT_ARCHIVE_PAGE_LIMIT` (20). Returns a flat `Vec`, not a cursor page. |
| `cleanup_expired_pending` | Owner/Admin | Auth required. Removes expired pending txs. |
| `get_storage_stats` | Anyone | No auth. Returns StorageStats. |
| `set_role_expiry` | Admin | Admin must authorize. Sets role expiration. |
| `get_role_expiry_public` | Anyone | No auth. Returns role expiry. |
| **Pause Functions** |||
| `pause` | Admin | Admin must be Auth. Requires Admin role. |
| `unpause` | Admin | Auth required. Validates pause admin. |
| `set_pause_admin` | Owner | Owner only. Sets pause admin. |
| `is_paused` | Anyone | No auth. |
| **Upgrade Functions** |||
| `set_upgrade_admin` | Owner | Owner only. Sets upgrade admin. |
| `set_version` | Upgrade Admin | Validates upgrade admin. |
| `get_version` | Anyone | No auth. |
| **Batch Operations** |||
| `batch_add_family_members` | Admin | Admin must authorize. Max 30 members. |
| `batch_remove_family_members` | Owner | Owner only. Max 30 members. |
| **Audit** |||
| `get_access_audit` | Anyone | No auth (no `require_auth()`, no role check). Returns the last `limit` entries from the full access-audit trail. Older/simpler variant — see `get_access_audit_page` below for the hardened, paginated replacement. |
| `get_access_audit_page` | Admin | `caller.require_auth()`; requires `require_role_at_least(caller, FamilyRole::Admin)`. Cursor is `from_index` (inclusive zero-based index); `limit` clamped to `MAX_AUDIT_PAGE_LIMIT`=50, default `DEFAULT_AUDIT_PAGE_LIMIT`=20. `next_cursor == total` (log length) is the end-of-log sentinel. |

### Risky Functions - Family Wallet
- **`remove_family_member`**: Owner can remove any member. Risk: owner could lock themselves out accidentally.
- **`propose_emergency_transfer`**: Can bypass multisig when emergency mode is enabled. High risk for fund diversion.
- **`configure_multisig`**: Can change threshold to 1, effectively disabling multisig.
- **`set_emergency_mode`**: Can enable emergency mode, allowing direct transfers.
- **`get_access_audit` vs `get_access_audit_page`**: `get_access_audit` has no auth/role check at all, while the newer `get_access_audit_page` requires Admin. Since the audit trail is described elsewhere in this doc as "privacy-sensitive — reveals who accessed what and when," the unauthenticated `get_access_audit` is an access-control inconsistency worth resolving (either gate it the same way or deprecate it in favor of the paginated version).
- **`batch_remove_family_members`**: Can remove multiple members at once. Should have additional safeguards.
- **`configure_emergency`**: Can set max_amount, cooldown, min_balance. Changes emergency transfer limits.

---

## 3. Savings Goals Contract

| Public Method | Required Caller | Access Control Details |
|--------------|-----------------|------------------------|
| `init` | Anyone (internal) | No external auth. Initializes storage. |
| `create_goal` | Owner | Owner must authorize. Creates new savings goal. Enforces per-owner cap `MAX_GOALS_PER_OWNER` (2000, counts active + archived goals) — returns `GoalCapReached` if exceeded. |
| `add_to_goal` | Owner | Owner must authorize. Adds funds to goal. |
| `batch_add_to_goals` | Owner | Owner must authorize. Batch add to multiple goals. |
| `withdraw_from_goal` | Owner | Owner must authorize. Must not be locked. |
| `lock_goal` | Owner | Owner only. Locks goal for withdrawal. |
| `unlock_goal` | Owner | Owner only. Unlocks goal. |
| `add_tags_to_goal` | Owner | Owner must authorize. Adds tags to goal. |
| `remove_tags_from_goal` | Owner | Owner must authorize. Removes tags from goal. |
| `get_goal` | Anyone | No auth. Returns goal if exists. |
| `get_goals` | Anyone | No auth. Paginated query by owner. |
| `get_goals_by_tag` | Anyone | No auth (no `require_auth()`) — filtered by `owner` param, canonicalized `tag` lookup via per-owner `TagIndex`. Paginated (`cursor`, `limit` via `clamp_limit`). Same anyone-can-read-by-owner-param pattern as `get_goals`. |
| `get_all_goals` | Anyone | No auth. Legacy function. |
| `is_goal_completed` | Anyone | No auth. |
| `export_snapshot` | Owner | Owner must authorize. Exports all goals. |
| `import_snapshot` | Owner | Owner must authorize. Validates nonce. |
| `get_audit_log` | Anyone | No auth. Log is capped at `MAX_AUDIT_ENTRIES` (5 for this contract — evicts oldest entries beyond that). |
| `set_time_lock` | Owner | Owner must authorize. Sets future unlock date. |
| `create_savings_schedule` | Owner | Owner must authorize. Creates recurring deposit. |
| `modify_savings_schedule` | Owner | Owner must authorize. Modifies schedule. |
| `cancel_savings_schedule` | Owner | Owner must authorize. Cancels schedule. |
| `execute_due_savings_schedules` | Anyone (internal) | No auth. Auto-executes due schedules. |
| `get_savings_schedules` | Owner | No explicit auth. Filtered by owner. |
| `get_savings_schedule` | Anyone | No auth. |
| **Archival & Tag-Index (added since last matrix update)** |||
| `archive_goal` | Owner | `caller.require_auth()`. Caller must be the goal's `owner`; goal must be completed (`current_amount >= target_amount`) and not already archived. Moves the goal out of active storage and out of all its tag indexes into `ArchivedGoal`/archived-owner-index storage. |
| `restore_goal` | Owner | `caller.require_auth()`. Caller must be the archived goal's `owner`; fails if an active goal with the same ID already exists. Re-inserts the goal into all its prior tag indexes. |
| `get_archived_goals_page` | Anyone | No auth. Paginated (`cursor`, `limit` via `clamp_limit`) read of one owner's archived goals via the `ArchivedGoalsIndex`; invalid non-zero cursors panic ("Invalid cursor"). |
| `get_archived_goals` | Anyone | No auth. Convenience alias that calls `get_archived_goals_page` directly with the same signature/semantics. |
| `get_archived_goal` | Anyone | No auth. Returns a single archived goal by ID (`Option<ArchivedSavingsGoal>`). |
| **Pause Functions** |||
| `set_pause_admin` | Initial: Anyone Subsequent: Admin | First caller becomes admin. |
| `pause` | Admin | Admin only. |
| `unpause` | Admin | Admin only. Can have time-lock. |
| `pause_function` | Admin | Admin only. |
| `unpause_function` | Admin | Admin only. |
| `is_paused` | Anyone | No auth. |
| **Upgrade Functions** |||
| `set_upgrade_admin` | Initial: Anyone Subsequent: Upgrade Admin | First caller becomes admin. |
| `set_version` | Upgrade Admin | Upgrade admin only. |
| `get_version` | Anyone | No auth. |

### Risky Functions - Savings Goals
- **`import_snapshot`**: Can overwrite all goals. Should require additional confirmations.
- **`execute_due_savings_schedules`**: Anyone can trigger automatic deposits. While this is by design, it could lead to unexpected deductions.
- **`lock_goal` / `unlock_goal`**: Can lock funds. Owner should be aware of implications.

---

## 4. Remittance Split Contract

| Public Method | Required Caller | Access Control Details |
|--------------|-----------------|------------------------|
| `initialize_split` | Owner | Owner must authorize. Validates nonce. One-time. |
| `update_split` | Owner | Owner must authorize. Validates nonce. |
| `get_split` | Anyone | No auth. Returns default [50,30,15,5] if not initialized. |
| `get_config` | Anyone | No auth. Returns SplitConfig if exists. |
| `calculate_split` | Anyone | No auth. Returns Vec<i128> of allocations. |
| `distribute_usdc` | Owner | Owner must authorize. Transfers tokens to accounts. |
| `get_usdc_balance` | Anyone | No auth. Queries token balance. |
| `get_split_allocations` | Anyone | No auth. Returns detailed allocations. |
| `get_nonce` | Anyone | No auth. Returns transaction nonce. |
| `export_snapshot` | Owner | Owner must authorize. Exports config. |
| `import_snapshot` | Owner | Owner must authorize. Imports config. |
| `get_audit_log` | Anyone | No auth. **Updated**: now returns a paginated `AuditPage` (`items`, `next_cursor`, `count`) instead of a bare `Vec` — call as `get_audit_log(from_index, limit)`; `limit` clamped via `clamp_limit`. Log itself capped at `MAX_AUDIT_ENTRIES` (100). |
| `create_remittance_schedule` | Owner | Owner must authorize. Creates auto-split schedule. Enforces per-owner cap `MAX_SCHEDULES_PER_OWNER` (50). |
| `modify_remittance_schedule` | Owner | Owner must authorize. |
| `cancel_remittance_schedule` | Owner | Owner must authorize. |
| `get_remittance_schedules` | Owner | No explicit auth. Filtered by owner. |
| `get_remittance_schedule` | Anyone | No auth. |
| `get_schedules_paginated` | Anyone | No auth. Paginated (`from_index`, `limit` via `clamp_limit`) read of one owner's schedules, ordered by schedule ID ascending; out-of-range `from_index` returns an empty page. Includes cancelled schedules (kept for audit). |
| `get_remittance_schedules_page` | Anyone | No auth. Same semantics as `get_schedules_paginated` but with `cursor` naming (zero-based index) instead of `from_index`; functionally equivalent, cursor-page variant. |
| **Pause Functions** |||
| `set_pause_admin` | Owner | Owner only after initialization. |
| `pause` | Admin | Admin or owner. |
| `unpause` | Admin | Admin or owner. |
| `is_paused` | Anyone | No auth. |
| **Upgrade Functions** |||
| `set_upgrade_admin` | Owner | Owner only. |
| `set_version` | Upgrade Admin | Upgrade admin only. |
| `get_version` | Anyone | No auth. |

### Risky Functions - Remittance Split
- **`distribute_usdc`**: Transfers tokens. Should require multisig for large amounts.
- **`import_snapshot`**: Can replace entire configuration. High impact.
- **`initialize_split`**: One-time action. After this, only owner can modify.

---

## 5. Insurance Contract

| Public Method | Required Caller | Access Control Details |
|--------------|-----------------|------------------------|
| `create_policy` | Owner | Owner must authorize. Creates insurance policy. Enforces per-owner active cap `MAX_POLICIES_PER_OWNER` (200). |
| `pay_premium` | Owner | Owner must authorize. Must own policy, policy must be active. |
| `batch_pay_premiums` | Owner | Owner must authorize. Batch premium payments. |
| `get_policy` | Anyone | No auth. Returns policy if exists. |
| `get_active_policies` | Anyone | No auth. Paginated (`cursor`, `limit`) by owner; `limit` clamped 0→`DEFAULT_PAGE_LIMIT` (20), max `MAX_PAGE_LIMIT` (50). Results additionally bounded by the per-owner active cap `MAX_POLICIES_PER_OWNER` (200), since that's the most policies any one owner index can hold. Returns `Err(NotInitialized)` if contract uninitialized. |
| `get_deactivated_policies` | Anyone | No auth. Paginated (`cursor`, `limit` via same clamp as `get_active_policies`) mirror of `get_active_policies` filtered to `active == false`. Returns an empty page (not an error) if uninitialized. |
| `get_all_policies_for_owner` | Owner | Owner must authorize. |
| `get_total_monthly_premium` | Anyone | No auth. Returns sum of active premiums. |
| `deactivate_policy` | Owner | Owner must authorize. Deactivates policy. |
| `archive_policy` | Owner | `caller.require_auth()`. Caller must be the policy's `owner` **or** the contract owner. Deactivates first if still active, then moves the policy ID into the `ArchivedPolicies` index. Returns `bool` (false on not-found/unauthorized rather than panicking). |
| `restore_policy` | Owner | `caller.require_auth()`. Caller must be the policy's `owner` **or** the contract owner. Re-checks the per-owner active cap (`MAX_POLICIES_PER_OWNER`=200) before restoring — returns `false` if the cap would be exceeded. |
| `reactivate_policy` | Owner | `caller.require_auth()`. Caller must be the policy's `owner` **or** the contract owner. Only for previously-deactivated (not archived) policies; enforces a cooldown — fails with `PolicyDeactivationTooSoon` if `now < deactivated_at + MAX_TENURE_SECS`. |
| `create_premium_schedule` | Owner | Owner must authorize. Creates auto-pay schedule. Enforces per-owner cap `MAX_SCHEDULES_PER_OWNER` (50). |
| `modify_premium_schedule` | Owner | Owner must authorize. |
| `cancel_premium_schedule` | Owner | Owner must authorize. |
| `execute_due_premium_schedules` | Anyone (internal) | No auth. Auto-executes due schedules. |
| `get_premium_schedules` | Owner | No explicit auth. Filtered by owner. |
| `get_premium_schedule` | Anyone | No auth. |
| **Pause Functions** |||
| `set_pause_admin` | Initial: Anyone Subsequent: Admin | First caller becomes admin. |
| `pause` | Admin | Admin only. |
| `unpause` | Admin | Admin only. Can have time-lock. |
| `pause_function` | Admin | Admin only. |
| `unpause_function` | Admin | Admin only. |
| `emergency_pause_all` | Admin | Admin only. Pauses all functions. |
| `is_paused` | Anyone | No auth. |
| **Upgrade Functions** |||
| `set_upgrade_admin` | Initial: Anyone Subsequent: Upgrade Admin | First caller becomes admin. |
| `set_version` | Upgrade Admin | Upgrade admin only. |
| `get_version` | Anyone | No auth. |

### Risky Functions - Insurance
- **`deactivate_policy`**: Can deactivate coverage. Owner should confirm.
- **`execute_due_premium_schedules`**: Auto-pays premiums. Could lead to unexpected deductions.
- **`batch_pay_premiums`**: Batch operation. Could pay multiple policies at once.

---

## 6. Orchestrator Contract (Cross-Contract Coordinator)

> **Note**: The previous version of this matrix listed `execute_savings_deposit`,
> `execute_bill_payment`, and `execute_insurance_payment` as public orchestrator methods.
> **These do not exist in `orchestrator/src/lib.rs`** — they have been removed from this
> matrix. The table below reflects the actual current public API (verified via
> `grep '^    pub fn' orchestrator/src/lib.rs`).

| Public Method | Required Caller | Access Control Details |
|--------------|-----------------|------------------------|
| `init` | Anyone (once) | `caller.require_auth()`. Succeeds only if no owner is currently set (`OWNER` unset); returns `Unauthorized` if already initialized. The first successful caller becomes the permanent contract owner and registers the five dependency addresses (family wallet, remittance split, savings goals, bill payments, insurance), rejecting duplicates/self-references. |
| `execute_remittance_flow` | Caller (any address) | `params.caller.require_auth()`. No owner/role restriction — any authenticated address can trigger a flow for itself. Guarded by amount > 0, a reentrancy lock (`EXEC_LOCK`), and per-step `FamilyWallet::check_spending_limit`. Emits `flow`/`flow_ok`/`flow_fail` lifecycle events and a `flow_exec` audit entry. |
| `execute_remittance_flow_signed` | Caller (any address) | `executor.require_auth()` first, then: contract must be initialized; amount > 0; reentrancy lock check; **actor-epoch match** (`verify_matching_epoch`, guards against stale signed tokens after `bump_actor_epoch`); **hardened nonce validation** — nonce not previously used, `deadline` not expired, and `request_hash` must match a hash computed over `(nonce, amount, deadline, goal_id, bill_id, policy_id)` so a relayer cannot redirect funds to a different goal/bill/policy after signing. Nonce is advanced only on success. |
| `execute_flow_fanout` | Caller (any address) | `executor.require_auth()`. Splits `amount` three ways and attempts savings/bill/insurance calls independently via `try_*` — no compensation/rollback on partial failure (contrast with `execute_remittance_flow`, which rolls back). Each downstream contract still separately enforces caller-must-be-owner on its side (e.g. `add_to_goal`, `pay_bill`, `pay_premium`). |
| `get_nonce` | Anyone | No auth. Returns the current replay-protection nonce for an address. |
| `get_execution_stats` | Anyone | No auth. Returns aggregate `ExecutionStats` (counts, last execution time, evicted audit entries). |
| `get_fee_schedule` | Anyone | No auth. Read-only cross-contract call into Remittance Split (`get_split`) to surface the current allocation percentages. |
| `claim_rewards_summary_external` | Caller (any address) | `caller.require_auth()`. Reentrancy-guarded (`EXEC_LOCK` + `ReentrancyDetected` typed error instead of panic); zeroes the pending-reward balance **before** the external token `transfer` call (checks-effects-interactions) to defeat reentrant double-claims. Fails with `NoPendingRewards` if balance is zero. |
| `get_pending_rewards` | Anyone | No auth. Read-only balance lookup, does not claim. |
| `get_audit_log` | Anyone | No auth. Paginated (`from_index`, `limit`); `limit` clamped to `[1, MAX_AUDIT_ENTRIES]` (100), 0 → default 20. Log is a ring-buffer capped at `MAX_AUDIT_ENTRIES` — `from_index` is a position in the current rotated window, not a stable global ID. Out-of-range `from_index` returns an empty `Vec` (not paginated as a struct — plain `Vec<AuditEntry>`). |
| `get_version` | Anyone | No auth. |
| `set_version` | Owner | `caller.require_auth()`; must equal stored `OWNER` or `Unauthorized`. |
| `bump_actor_epoch` | Owner | `caller.require_auth()`; must equal stored `OWNER`. Defense-in-depth: invalidates all previously-issued `execute_remittance_flow_signed` actor tokens by incrementing `ACTOR_EPOCH`, forcing re-signing. |
| `get_actor_epoch_public` | Anyone | No auth. Lets actors fetch the current epoch before constructing a signed-flow token. |
| `pre_upgrade` | Owner | `caller.require_auth()`; must equal stored `OWNER`. Snapshots owner, dependency addresses, execution-lock state, stats, parameter IDs, and actor epoch to persistent storage ahead of a contract upgrade. |
| `restore_from_snapshot` | Owner | `caller.require_auth()`; must equal stored `OWNER`, and the snapshot's own recorded owner must still match. Also enforces `require_recent_snapshot` (fails `SnapshotTooOld` if the snapshot is stale) and a schema-version check. Consumes (deletes) the snapshot on success. |
| `discard_snapshot` | Owner | `caller.require_auth()`; must equal stored `OWNER`. Deletes a pending snapshot without restoring it. |
| `get_execution_state` | Anyone | No auth. Returns whether the reentrancy lock (`EXEC_LOCK`) is currently held. |

### Cross-Contract Call Constraints

The orchestrator makes the following cross-contract calls (via `execute_remittance_flow`,
`execute_remittance_flow_signed`, and `execute_flow_fanout`):

1. **Family Wallet** (`check_spending_limit`)
   - Validates caller has permission and is within their spending limit
   - Called only by the two `execute_remittance_flow*` entrypoints, not `execute_flow_fanout`

2. **Remittance Split** (`calculate_split`, `get_split`)
   - Gets allocation percentages/amounts
   - No auth required on the called contract

3. **Savings Goals** (`add_to_goal` / `try_add_to_goal`)
   - Deposits to goal
   - Downstream contract requires caller to be goal owner

4. **Bill Payments** (`pay_bill` / `try_pay_bill`)
   - Pays bill
   - Downstream contract requires caller to be bill owner

5. **Insurance** (`pay_premium` / `try_pay_premium`)
   - Pays premium
   - Downstream contract requires caller to be policy owner

6. **Reward Token** (SEP-41 `transfer`, called from `claim_rewards_summary_external`)
   - Orchestrator's own contract address acts as escrow/holder
   - Balance is zeroed before the call (checks-effects-interactions) to prevent reentrant double-claims

### Risky Functions - Orchestrator
- **`execute_remittance_flow_signed`**: The highest-value entrypoint in the contract — combines nonce/deadline/request-hash replay protection with actor-epoch validation specifically to defend against relayer-submitted, pre-signed transactions being replayed or redirected to different goal/bill/policy IDs. Any weakening of `require_nonce_hardened`, `compute_request_hash`, or `verify_matching_epoch` reintroduces a fund-redirection risk. Bump `bump_actor_epoch` if a signing key is suspected compromised.
- **`execute_remittance_flow`**: Executes multiple cross-contract operations atomically under a reentrancy lock. If any step fails, previously-applied steps are compensated (best-effort) and the flow reports `RemittanceFlowRolledBack`.
- **`execute_flow_fanout`**: Deliberately has **no rollback** — a partial failure (e.g. bill payment succeeds but insurance premium fails) leaves state partially applied by design. Callers must inspect `FanOutFlowResult` and handle partial success themselves.
- **`claim_rewards_summary_external`**: Calls an externally-supplied `reward_token` address with no allowlist — any SEP-41-shaped contract can be passed. Combined with the reentrancy guard this is safe from double-spend, but a malicious `reward_token` could still revert/misbehave in ways worth fuzzing.
- **`restore_from_snapshot`**: Can roll back dependency addresses, execution-lock state, and actor epoch to a prior snapshot. Protected by `require_recent_snapshot`, but an owner error here has contract-wide blast radius.

---

## 7. Reporting Contract

| Public Method | Required Caller | Access Control Details |
|--------------|-----------------|------------------------|
| `init` | Admin | Admin must authorize. One-time initialization. |
| `configure_addresses` | Admin | Admin only. Configures contract addresses. |
| `check_dependencies` | Admin | Admin only. Returns dependency health statuses. |
| `get_remittance_summary` | Anyone | No auth. Queries split calculator. |
| `get_savings_report` | Anyone | No auth. Queries savings goals. |
| `get_bill_compliance_report` | Anyone | No auth. Queries bill payments. |
| `get_insurance_report` | Anyone | No auth. Queries insurance. |
| `calculate_health_score` | Anyone | No auth. Calculates health metrics. |
| `get_financial_health_report` | Anyone | No auth. Generates comprehensive report. |
| `get_trend_analysis` | Anyone | No auth (`_caller`/`_user` params are unused/underscore-prefixed in the signature). Compares two amounts. |
| `get_trend_analysis_multi` | User | `user.require_auth()`. Computes one `TrendData` per point in a caller-supplied `(period_key, amount)` history Vec (no server-side length cap observed — bounded only by transaction size/CPU limits). |
| `get_top_bills_report` | User | `user.require_auth()`. **Cap endpoint**: results bounded to `MAX_ITEMS_PER_REPORT` (= shared `remitwise_common::MAX_TOP_N` = 10) via bounded sorted-insertion (`insert_top_n`); a defence-in-depth `require_bounded_top_n` guard fails closed if a future change ever raised the cap above `MAX_TOP_N`. Paginates the underlying Bill Payments dependency call internally (`DEP_PAGE_LIMIT`=50) so the report itself computes over all of the user's bills even though only the top 10 are returned. |
| `get_top_savings_report` | User | `user.require_auth()`. Same `MAX_ITEMS_PER_REPORT`/`MAX_TOP_N` (10) cap and `require_bounded_top_n` guard as `get_top_bills_report`, applied to savings goals sorted by target amount descending. |
| `store_report` | User | User must authorize. Stores report for user. |
| `get_stored_report` | User | No explicit auth. Filtered by user. |
| `get_addresses` | Anyone | No auth. Returns configured addresses. |
| `get_admin` | Anyone | No auth. Returns admin address. |
| `archive_old_reports` | Admin | Admin only. Archives old reports. |
| `get_archived_reports` | User | No explicit auth. Filtered by user. Deprecated in favor of the paginated `get_archived_reports_page` (see doc comment on that method). |
| `get_archived_reports_page` | User | `user.require_auth()`. Paginated (`cursor`, `limit` via `clamp_limit`, 0→`DEFAULT_PAGE_LIMIT`=20, max `MAX_PAGE_LIMIT`=50). Out-of-range `cursor` returns an empty page with `next_cursor = 0` rather than echoing back `cursor` (a fixed footgun from an earlier version). `count` is always the user's total archive size, independent of `cursor`/`limit`. |
| `cleanup_old_reports` | Admin | Admin only. Deletes old archives. |
| `get_storage_stats` | Anyone | No auth. |

### Risky Functions - Reporting
- **`store_report`**: Stores data for user. Could be used to fill storage.
- **`archive_old_reports` / `cleanup_old_reports`**: Admin can delete data.
- **`get_trend_analysis_multi`**: Accepts a caller-supplied `history: Vec<(u64, i128)>` with no explicit length cap in the function itself — unlike the Top-N report endpoints, which are hard-capped at `MAX_ITEMS_PER_REPORT`/`MAX_TOP_N` (10), this endpoint's cost scales linearly with whatever the caller passes. Worth capping explicitly to match the Top-N pattern rather than relying solely on the network's transaction resource limits.

---

## 8. Emergency Killswitch Contract

The killswitch is the contract operators reach for during an incident. Pause checks follow a
strict precedence order: **global → module → function**. A function blocked at a higher level
remains blocked even if its own function-level flag is clear.

| Public Method | Required Caller | Access Control Details |
|---|---|---|
| `initialize` | Anyone (once) | Sets admin. Rejects contract's own address. Fails if already initialized. |
| `transfer_admin` | Admin | Admin must authorize. Rejects self-address and current admin as new admin. Emits `AdminTransferred`. |
| `pause` | Admin | Admin only. Sets global pause flag, clears any pending unpause schedule. |
| `unpause` | Admin | Admin only. Requires a valid `schedule_unpause` timestamp ≥ current ledger time. Clears schedule on success. |
| `schedule_unpause` | Admin | Admin only. Stores a future timestamp; must be ≥ current ledger time. |
| `pause_module` | Admin | Admin only. Sets `ModulePaused(module_id)` flag. |
| `unpause_module` | Admin | Admin only. Clears `ModulePaused(module_id)` flag. |
| `pause_function` | Admin | Admin only. Appends func to `PausedFunctions(module_id)`. Capped at `MAX_PAUSED_FUNCTIONS` (10). |
| `unpause_function` | Admin | Admin only. Removes func from `PausedFunctions(module_id)`. |
| **Read / Observability** |||
| `is_paused` | Anyone | No auth. Returns global pause flag. |
| `is_function_paused` | Anyone | No auth. Precedence: global → module → function. |
| `get_unpause_schedule` | Anyone | No auth. Returns pending unpause timestamp or `None` if none scheduled. |
| `list_paused_functions` | Anyone | No auth. Returns `PausedFunctions(module_id)` vec (empty if none). Bounded by `MAX_PAUSED_FUNCTIONS` (10); no pagination required. Does **not** reflect module- or global-level pauses — use `is_function_paused` for the full check. |
| `is_module_paused` | Anyone | No auth. Returns `ModulePaused(module_id)` flag directly. Does **not** include global-pause state. |

### Risky Functions - Emergency Killswitch
- **`transfer_admin`**: Irreversibly changes the sole authority over the killswitch. A botched transfer could leave the killswitch unrecoverable.
- **`pause`**: Immediately halts the system globally and silently drops any pending unpause schedule.
- **`unpause`**: Requires a pre-set schedule; calling without one returns `InvalidSchedule`, preventing accidental unpauses.

---

## Cross-Contract Call Summary

| Caller Contract | Called Contract | Function Called | Constraint |
|----------------|-----------------|-----------------|------------|
| Orchestrator | Family Wallet | `check_spending_limit` | Caller must be family member (not called by `execute_flow_fanout`) |
| Orchestrator | Remittance Split | `calculate_split`, `get_split` | Must be initialized |
| Orchestrator | Savings Goals | `add_to_goal` / `try_add_to_goal` | Caller must be goal owner |
| Orchestrator | Bill Payments | `pay_bill` / `try_pay_bill` | Caller must be bill owner |
| Orchestrator | Insurance | `pay_premium` / `try_pay_premium` | Caller must be policy owner |
| Orchestrator | Reward Token (SEP-41) | `transfer` | Orchestrator contract address is the token holder/escrow; balance zeroed before call |
| Reporting | Remittance Split | `get_split`, `try_get_split`, `calculate_split` | Must be initialized |
| Reporting | Savings Goals | `get_all_goals`, `get_goals` (paginated), `try_get_all_goals`, `is_goal_completed` | None |
| Reporting | Bill Payments | `get_unpaid_bills`, `get_all_bills_for_owner` (paginated), `try_get_total_unpaid` | None (Note: `get_all_bills`, referenced by name in the prior version of this matrix, no longer exists in Bill Payments) |
| Reporting | Insurance | `get_active_policies` (paginated), `get_total_monthly_premium`, `get_policy`, `try_get_total_monthly_premium` | None |

---

## Summary of Improvements Needed

Based on the access control analysis, the following improvements are recommended:

### High Priority

1. **Family Wallet - Emergency Transfer Bypass**
   - **Issue**: `propose_emergency_transfer` can bypass multisig when emergency mode is enabled
   - **Recommendation**: Add a configurable limit on emergency transfers even in emergency mode

2. **Remittance Split - Missing Nonce Validation**
   - **Issue**: `calculate_split` has no access control - anyone can calculate splits
   - **Recommendation**: Consider adding optional owner-only calculation for sensitive amounts

3. **Bill Payments - Admin Access to All Bills**
   - **Issue**: `get_all_bills_page` (the paginated successor to the removed `get_all_bills`) exposes all bills across all owners to the pause admin
   - **Recommendation**: Consider limiting to audit purposes only with event logging

### Medium Priority

4. **Batch Operations Lack Confirmation**
   - **Issue**: `batch_pay_bills`, `batch_add_to_goals` execute multiple operations
   - **Recommendation**: Add optional confirmation for large batch sizes

5. **Snapshot Import Overwrites All Data**
   - **Issue**: `import_snapshot` in Savings Goals and Remittance Split can replace all data
   - **Recommendation**: Require multi-sig or time-lock for snapshot imports

6. **No Rate Limiting on Critical Functions**
   - **Issue**: No rate limiting on functions like `pay_bill`, `withdraw`
   - **Recommendation**: Implement rate limiting for high-frequency operations

### Low Priority

7. **Role Expiry Not Enforced Consistently**
   - **Issue**: `set_role_expiry` exists but not all functions check expiry
   - **Recommendation**: Audit all functions to ensure role expiry is checked

8. **Pause Functions Could Be More Granular**
   - **Issue**: Function-level pause exists but not consistently applied
   - **Recommendation**: Review all contracts for consistent function-level pause

9. **Bill Payments - Unbounded Legacy Query**
   - **Issue**: `get_all_unpaid_bills_legacy` returns every unpaid bill for an owner in a single unbounded `Vec` with no `cursor`/`limit`, unlike every other list endpoint added since the pagination pass
   - **Recommendation**: Deprecate in favor of `get_unpaid_bills` (paginated), or add an explicit hard cap if the legacy signature must be kept for backward compatibility

10. **Family Wallet - Inconsistent Audit-Log Access Control**
    - **Issue**: `get_access_audit` has no auth or role check, while the newer `get_access_audit_page` requires Admin — for data this doc itself calls "privacy-sensitive"
    - **Recommendation**: Gate `get_access_audit` the same way as `get_access_audit_page`, or remove it in favor of the paginated, Admin-gated version

---

## Appendix: Pagination & Cap Constants

All values below were confirmed by grepping each contract crate directly (`grep -n "^const \|^pub const "`),
not inferred. "Own copy" means the contract defines the constant locally (sometimes duplicating
`remitwise-common`'s value) rather than importing it.

| Constant | Value | Where Defined | Used By |
|---|---|---|---|
| `MAX_PAGE_LIMIT` | 50 | `remitwise-common` (re-exported); `savings_goals` also keeps its own local copy of the same value | `bill_payments`, `insurance`, `remittance_split`, `reporting`, `savings_goals` (own copy) via `clamp_limit()` — caps every `cursor`/`limit` paginated read across these contracts |
| `DEFAULT_PAGE_LIMIT` | 20 | `remitwise-common` (re-exported); `savings_goals` own copy | Same paginated reads as `MAX_PAGE_LIMIT`; used when caller passes `limit == 0` |
| `MAX_AUDIT_ENTRIES` | Varies: `savings_goals` = 5, `orchestrator` = 100, `remittance_split` = 100 | Each contract defines its own local constant — **not shared** | Caps the audit-log ring buffer / paginated `get_audit_log` in each of the three contracts. Note the large disparity between `savings_goals` (5) and the other two (100) — worth confirming `savings_goals`' 5-entry retention is intentional, since it evicts far more aggressively. |
| `MAX_ACCESS_AUDIT_ENTRIES` | 200 | `family_wallet` | Caps the underlying `ACC_AUDIT` log that both `get_access_audit` and `get_access_audit_page` read from |
| `MAX_AUDIT_PAGE_LIMIT` / `DEFAULT_AUDIT_PAGE_LIMIT` | 50 / 20 | `family_wallet` | `get_access_audit_page`'s `limit` clamp |
| `MAX_PENDING_PAGE_LIMIT` / `DEFAULT_PENDING_PAGE_LIMIT` | 100 / 20 | `family_wallet` | `get_pending_transactions_page`'s `limit` clamp |
| `MAX_MEMBER_PAGE_LIMIT` / `DEFAULT_MEMBER_PAGE_LIMIT` | 100 / 20 | `family_wallet` | `get_member_addresses_page`'s `limit` clamp |
| `MAX_ARCHIVE_PAGE_LIMIT` / `DEFAULT_ARCHIVE_PAGE_LIMIT` | 100 / 20 | `family_wallet` | `get_archived_transactions`'s `limit` clamp |
| `MAX_ARCHIVE_ENTRIES` | 500 | `family_wallet` | Cap on the `ARCH_TX` archived-transaction map |
| `MAX_BILLS_PER_OWNER` | 1,000 | `bill_payments` | Per-owner active-bill cap, enforced in `create_bill`/etc.; bounds the cost of owner-scoped paginated reads (`get_unpaid_bills`, `get_all_bills_for_owner`, `get_overdue_bills_for_owner`, `get_owner_bill_count`) |
| `MAX_BILL_SCHEDULES_PER_OWNER` | 50 | `bill_payments` | Per-owner bill-payment schedule cap |
| `MAX_POLICIES_PER_OWNER` | 200 | `insurance` | Per-owner **active** policy cap, re-checked by `restore_policy`; bounds `get_active_policies` / `get_deactivated_policies` |
| `MAX_SCHEDULES_PER_OWNER` | 50 | `insurance` (premium schedules); `remittance_split` (remittance schedules) — two independent constants of the same name/value in different contracts | Per-owner schedule cap in each contract |
| `MAX_GOALS_PER_OWNER` | 2,000 | `savings_goals` | Per-owner goal cap (counts active **and** archived goals together), enforced in `create_goal` |
| `MAX_USED_NONCES_PER_ADDR` | 256 | `orchestrator`, `remittance_split` (independent constants) | Bounds the per-address used-nonce list for replay protection |
| `MAX_ITEMS_PER_REPORT` / `MAX_TOP_N` | 10 | `remitwise-common` (`MAX_TOP_N`); `reporting` aliases it as `MAX_ITEMS_PER_REPORT` | Hard cap on `get_top_bills_report` / `get_top_savings_report` results, enforced by bounded sorted-insertion plus a defence-in-depth `require_bounded_top_n` guard |
| `DEP_PAGE_LIMIT` | 50 | `reporting` | Page size `reporting` uses internally when it paginates through a *dependency* contract's data (e.g. walking all of a user's bills/goals to build a Top-N report) |
| `MAX_PAUSED_FUNCTIONS` | 10 | `emergency_killswitch` | Cap on `PausedFunctions(module_id)`, enforced by `pause_function` |

---

## Appendix: Role Hierarchy

| Role | Numeric Value | Privileges |
|------|---------------|------------|
| Owner | 1 | Full control, can add/remove admins, can pause |
| Admin | 2 | Can manage members, configure settings |
| Member | 3 | Can propose/sign transactions |
| Viewer | 4 | Read-only access |

---

*Document generated for Remitwise Contracts Access Control Analysis*

