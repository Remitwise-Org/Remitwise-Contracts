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
| `get_unpaid_bills` | Anyone | No auth required. Paginated query filtered by owner. |
| `get_all_bills_for_owner` | Owner | Owner must authorize. Returns all bills (paid + unpaid). |
| `get_overdue_bills` | Anyone | No auth. Returns unpaid bills past due date. |
| `get_all_bills` | Admin | Pause admin only. Admin auth required. |
| `cancel_bill` | Owner | Owner must authorize. Must own the bill. |
| `archive_paid_bills` | Owner | Owner must authorize. Requires not paused. |
| `restore_bill` | Owner | Owner must authorize. Must own archived bill. |
| `bulk_cleanup_bills` | Owner | Owner must authorize. Admin-level cleanup. |
| `batch_pay_bills` | Owner | Owner must authorize. Batch processing of bill payments. |
| `get_total_unpaid` | Anyone | No auth. Returns unpaid total for owner. |
| `get_storage_stats` | Anyone | No auth. Returns StorageStats. |
| `get_bills_by_currency` | Anyone | No auth. Filtered by owner and currency. |
| `get_unpaid_bills_by_currency` | Anyone | No auth. Filtered by owner, currency, unpaid status. |
| `get_total_unpaid_by_currency` | Anyone | No auth. Sum of unpaid bills in specific currency. |
| `get_archived_bills` | Owner | No explicit auth in signature, but filtered by owner. |
| `get_archived_bill` | Anyone | No auth. Returns specific archived bill. |
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
- **`get_all_bills`**: Admin-only access to all bills across all owners. Could expose sensitive data.
- **`archive_paid_bills` / `bulk_cleanup_bills`**: Bulk operations that modify storage. Should require additional confirmations for large batches.
- **`emergency_pause_all`**: Can disable entire contract. Should have time-lock.

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
| `get_multisig_config` | Anyone | No auth. Returns config for tx type. |
| `get_family_member` | Anyone | No auth. Returns member details. |
| `get_owner` | Anyone | No auth. Returns wallet owner. |
| `get_emergency_config` | Anyone | No auth. Returns emergency settings. |
| `is_emergency_mode` | Anyone | No auth. Returns bool. |
| `get_last_emergency_at` | Anyone | No auth. Returns last emergency timestamp. |
| `archive_old_transactions` | Owner/Admin | Auth required. Archives executed txs. |
| `get_archived_transactions` | Anyone | No auth. Returns archived txs. |
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
| `get_access_audit` | Anyone | No auth. Returns audit entries. |

### Risky Functions - Family Wallet
- **`remove_family_member`**: Owner can remove any member. Risk: owner could lock themselves out accidentally.
- **`propose_emergency_transfer`**: Can bypass multisig when emergency mode is enabled. High risk for fund diversion.
- **`configure_multisig`**: Can change threshold to 1, effectively disabling multisig.
- **`set_emergency_mode`**: Can enable emergency mode, allowing direct transfers.
- **`batch_remove_family_members`**: Can remove multiple members at once. Should have additional safeguards.
- **`configure_emergency`**: Can set max_amount, cooldown, min_balance. Changes emergency transfer limits.

---

## 3. Savings Goals Contract

| Public Method | Required Caller | Access Control Details |
|--------------|-----------------|------------------------|
| `init` | Anyone (internal) | No external auth. Initializes storage. |
| `create_goal` | Owner | Owner must authorize. Creates new savings goal. |
| `add_to_goal` | Owner | Owner must authorize. Adds funds to goal. |
| `batch_add_to_goals` | Owner | Owner must authorize. Batch add to multiple goals. |
| `withdraw_from_goal` | Owner | Owner must authorize. Must not be locked. |
| `lock_goal` | Owner | Owner only. Locks goal for withdrawal. |
| `unlock_goal` | Owner | Owner only. Unlocks goal. |
| `get_goal` | Anyone | No auth. Returns goal if exists. |
| `get_goals` | Anyone | No auth. Paginated query by owner. |
| `get_all_goals` | Anyone | No auth. Legacy function. |
| `is_goal_completed` | Anyone | No auth. |
| `export_snapshot` | Owner | Owner must authorize. Exports all goals. |
| `import_snapshot` | Owner | Owner must authorize. Validates nonce. |
| `get_audit_log` | Anyone | No auth. |
| `set_time_lock` | Owner | Owner must authorize. Sets future unlock date. |
| `create_savings_schedule` | Owner | Owner must authorize. Creates recurring deposit. |
| `modify_savings_schedule` | Owner | Owner must authorize. Modifies schedule. |
| `cancel_savings_schedule` | Owner | Owner must authorize. Cancels schedule. |
| `execute_due_savings_schedules` | Anyone (internal) | No auth. Auto-executes due schedules. |
| `get_savings_schedules` | Owner | No explicit auth. Filtered by owner. |
| `get_savings_schedule` | Anyone | No auth. |
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
| `get_audit_log` | Anyone | No auth. |
| `create_remittance_schedule` | Owner | Owner must authorize. Creates auto-split schedule. |
| `modify_remittance_schedule` | Owner | Owner must authorize. |
| `cancel_remittance_schedule` | Owner | Owner must authorize. |
| `get_remittance_schedules` | Owner | No explicit auth. Filtered by owner. |
| `get_remittance_schedule` | Anyone | No auth. |
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
| `create_policy` | Owner | Owner must authorize. Creates insurance policy. |
| `pay_premium` | Owner | Owner must authorize. Must own policy, policy must be active. |
| `batch_pay_premiums` | Owner | Owner must authorize. Batch premium payments. |
| `get_policy` | Anyone | No auth. Returns policy if exists. |
| `get_active_policies` | Anyone | No auth. Paginated by owner. |
| `get_all_policies_for_owner` | Owner | Owner must authorize. |
| `get_total_monthly_premium` | Anyone | No auth. Returns sum of active premiums. |
| `deactivate_policy` | Owner | Owner must authorize. Deactivates policy. |
| `create_premium_schedule` | Owner | Owner must authorize. Creates auto-pay schedule. |
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

| Public Method | Required Caller | Access Control Details |
|--------------|-----------------|------------------------|
| `execute_savings_deposit` | Caller | Caller must authorize. Checks family wallet permission first. |
| `execute_bill_payment` | Caller | Caller must authorize. Validates spending limit. |
| `execute_insurance_payment` | Caller | Caller must authorize. Validates spending limit. |
| `execute_remittance_flow` | Caller | Caller must authorize. Full remittance flow with all validations. |
| `get_execution_stats` | Anyone | No auth. Returns execution statistics. |
| `get_audit_log` | Anyone | No auth. Returns audit entries. |

### Cross-Contract Call Constraints

The orchestrator makes the following cross-contract calls:

1. **Family Wallet** (`check_spending_limit`)
   - Validates caller has permission
   - Checks spending limit

2. **Remittance Split** (`calculate_split`)
   - Gets allocation amounts
   - No auth required on called contract

3. **Savings Goals** (`add_to_goal`)
   - Deposits to goal
   - Requires caller to be goal owner

4. **Bill Payments** (`pay_bill`)
   - Pays bill
   - Requires caller to be bill owner

5. **Insurance** (`pay_premium`)
   - Pays premium
   - Requires caller to be policy owner

### Risky Functions - Orchestrator
- **`execute_remittance_flow`**: Executes multiple operations atomically. If any step fails, all revert.
- **`execute_savings_deposit` / `execute_bill_payment` / `execute_insurance_payment`**: Individual operations that check permissions but execute immediately.

---

## 7. Reporting Contract

| Public Method | Required Caller | Access Control Details |
|--------------|-----------------|------------------------|
| `init` | Admin | Admin must authorize. One-time initialization. |
| `configure_addresses` | Admin | Admin only. Configures contract addresses. |
| `get_remittance_summary` | Anyone | No auth. Queries split calculator. |
| `get_savings_report` | Anyone | No auth. Queries savings goals. |
| `get_bill_compliance_report` | Anyone | No auth. Queries bill payments. |
| `get_insurance_report` | Anyone | No auth. Queries insurance. |
| `calculate_health_score` | Anyone | No auth. Calculates health metrics. |
| `get_financial_health_report` | Anyone | No auth. Generates comprehensive report. |
| `get_trend_analysis` | Anyone | No auth. Compares periods. |
| `store_report` | User | User must authorize. Stores report for user. |
| `get_stored_report` | User | No explicit auth. Filtered by user. |
| `get_addresses` | Anyone | No auth. Returns configured addresses. |
| `get_admin` | Anyone | No auth. Returns admin address. |
| `archive_old_reports` | Admin | Admin only. Archives old reports. |
| `get_archived_reports` | User | No explicit auth. Filtered by user. |
| `cleanup_old_reports` | Admin | Admin only. Deletes old archives. |
| `get_storage_stats` | Anyone | No auth. |

### Risky Functions - Reporting
- **`store_report`**: Stores data for user. Could be used to fill storage.
- **`archive_old_reports` / `cleanup_old_reports`**: Admin can delete data.

---

## Cross-Contract Call Summary

| Caller Contract | Called Contract | Function Called | Constraint |
|----------------|-----------------|-----------------|------------|
| Orchestrator | Family Wallet | `check_spending_limit` | Caller must be family member |
| Orchestrator | Remittance Split | `calculate_split` | Must be initialized |
| Orchestrator | Savings Goals | `add_to_goal` | Caller must be goal owner |
| Orchestrator | Bill Payments | `pay_bill` | Caller must be bill owner |
| Orchestrator | Insurance | `pay_premium` | Caller must be policy owner |
| Reporting | Remittance Split | `get_split`, `calculate_split` | Must be initialized |
| Reporting | Savings Goals | `get_all_goals`, `is_goal_completed` | None |
| Reporting | Bill Payments | `get_unpaid_bills`, `get_all_bills` | None |
| Reporting | Insurance | `get_active_policies`, `get_total_monthly_premium` | None |

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
   - **Issue**: `get_all_bills` exposes all bills to admin
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

