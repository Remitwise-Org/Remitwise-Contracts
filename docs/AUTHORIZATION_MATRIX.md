# Authorization Matrix

Audience: **Contributor** — use this table to verify that every public contract entrypoint
requires the expected caller authorization and that the intent matches the code.

## Legend

| Column | Meaning |
|--------|---------|
| **Entrypoint** | Function name as invoked via Soroban |
| **Required auth** | The address whose `require_auth()` (or `require_auth_for_args()`) is called — the caller **must** control this key |
| **Optional / secondary check** | Additional gate checked *after* auth (ownership, role, admin match); failure returns `Unauthorized` |
| **Paused?** | Whether the entrypoint is blocked while the contract is paused |

### Auth primitives used

| Notation | Meaning |
|----------|---------|
| `X.require_auth()` | Digital signature of `X` verified by the Soroban host |
| `X.require_auth_for_args(payload)` | Same as above, but binds the signature to the argument payload (prevents replay across contexts) |
| `is_owner_or_admin` | Caller must be the `Owner` or an `Admin` member of the wallet |
| `require_role_at_least(Role)` | Caller's role (or higher) must match; `Owner` -> `Admin` -> `Member` -> `Viewer` |
| owner-of-resource check | The calling address must match the stored owner of the specific resource (goal, bill, policy, schedule) |
| admin / pause-admin check | `caller` must match the stored admin address |
| read-only | No auth, no side effects (pure query) |

---

## remittance_split

| Entrypoint | Required auth | Optional / secondary check | Paused? |
|---|---|---|---|
| `initialize_split` | `owner.require_auth_for_args(payload)` | nonce replay guard | yes |
| `update_split` | `caller.require_auth()` | `config.owner == caller` | yes |
| `get_split` | read-only | — | no |
| `get_config` | read-only | — | no |
| `calculate_split` | read-only | — | no |
| `distribute_usdc` | `from.require_auth()` | `config.owner == from` + token contract pin | yes |
| `distribute_usdc_hashed` | `request.from.require_auth()` | hash-verified request integrity | no |
| `get_usdc_balance` | read-only | — | no |
| `get_split_allocations` | read-only | — | no |
| `get_nonce` | read-only | — | no |
| `pause` | `caller.require_auth()` | pause-admin match (defaults to owner) | guards |
| `unpause` | `caller.require_auth()` | pause-admin match | — |
| `set_pause_admin` | `caller.require_auth()` | `config.owner == caller` | yes |
| `upgrade` | `caller.require_auth()` | upgrade-admin match | — |
| `set_version` | `caller.require_auth()` | upgrade-admin match | yes |
| `export_snapshot` | `caller.require_auth()` | `config.owner == caller` | no |
| `import_snapshot` | `caller.require_auth()` | checksum + nonce validation | yes |
| `verify_snapshot` | read-only | — | no |
| `get_audit_log` | read-only | — | no |
| `create_remittance_schedule` | `owner.require_auth()` | `config.owner == owner` | yes |
| `modify_remittance_schedule` | `caller.require_auth()` | schedule owner match | yes |
| `cancel_remittance_schedule` | `caller.require_auth()` | schedule owner match | yes |
| `execute_due_remittance_schedules` | permissionless | — | skips if paused |

---

## savings_goals

| Entrypoint | Required auth | Optional / secondary check | Paused? |
|---|---|---|---|
| `init` | none (one-shot) | — | no |
| `create_goal` | `owner.require_auth()` | — | yes (per-function) |
| `add_to_goal` | `caller.require_auth()` | `goal.owner == caller` | yes (per-function) |
| `batch_add_to_goals` | `caller.require_auth()` | `goal.owner == caller` per item | yes (per-function) |
| `withdraw_from_goal` | `caller.require_auth()` | `goal.owner == caller` | yes (per-function) |
| `lock_goal` | `caller.require_auth()` | `goal.owner == caller` | yes (per-function) |
| `unlock_goal` | `caller.require_auth()` | `goal.owner == caller` | yes (per-function) |
| `archive_goal` | `caller.require_auth()` | `goal.owner == caller` | yes (per-function) |
| `restore_goal` | `caller.require_auth()` | ownership check | yes (per-function) |
| `get_goal` | read-only | — | no |
| `get_goals` | read-only | — | no |
| `get_all_goals` | read-only | — | no |
| `is_goal_completed` | read-only | — | no |
| `pause` | `caller.require_auth()` | pause-admin match | — |
| `unpause` | `caller.require_auth()` | pause-admin match + timelock | — |
| `upgrade` | `caller.require_auth()` | upgrade-admin match | — |
| `set_version` | `caller.require_auth()` | upgrade-admin match | no |
| `export_snapshot` | `caller.require_auth()` | — (any authed caller) | no |
| `import_snapshot` | `caller.require_auth()` | checksum + nonce | no |
| `get_audit_log` | read-only | — | no |
| `add_tags_to_goal` | `caller.require_auth()` | `goal.owner == caller` | no |
| `remove_tags_from_goal` | `caller.require_auth()` | `goal.owner == caller` | no |
| `set_time_lock` | `caller.require_auth()` | `goal.owner == caller` | no |
| `create_savings_schedule` | `owner.require_auth()` | `goal.owner == owner` | no |
| `modify_savings_schedule` | `caller.require_auth()` | schedule owner match | no |
| `cancel_savings_schedule` | `caller.require_auth()` | schedule owner match | no |
| `execute_due_savings_schedules` | permissionless | — | no |

---

## bill_payments

| Entrypoint | Required auth | Optional / secondary check | Paused? |
|---|---|---|---|
| `create_bill` | `owner.require_auth()` | — | yes (per-function) |
| `pay_bill` | `caller.require_auth()` | `bill.owner == caller` | yes (per-function) |
| `batch_pay_bills` | `caller.require_auth()` | `bill.owner == caller` per bill | yes (per-function) |
| `cancel_bill` | `caller.require_auth()` | `bill.owner == caller` | yes (per-function) |
| `set_external_ref` | `caller.require_auth()` | `bill.owner == caller` | no |
| `add_tags_to_bill` | `caller.require_auth()` | `bill.owner == caller` | no |
| `remove_tags_from_bill` | `caller.require_auth()` | `bill.owner == caller` | no |
| `get_bill` | read-only | — | no |
| `get_unpaid_bills` | `owner.require_auth()` | — | no |
| `get_all_bills_for_owner` | `owner.require_auth()` | — | no |
| `get_overdue_bills_for_owner` | `owner.require_auth()` | — | no |
| `get_all_bills_page` | `caller.require_auth()` | pause-admin check | no |
| `get_total_unpaid` | read-only | — | no |
| `get_archived_bills` | read-only | — | no |
| `get_archived_bill` | read-only | — | no |
| `archive_paid_bills` | `caller.require_auth()` | — (any authed caller) | yes (per-function) |
| `restore_bill` | `caller.require_auth()` | — | yes (per-function) |
| `bulk_cleanup_bills` | `caller.require_auth()` | — | yes (per-function) |
| `pause` | `caller.require_auth()` | pause-admin match | — |
| `unpause` | `caller.require_auth()` | pause-admin match + timelock | — |
| `upgrade` | `caller.require_auth()` | upgrade-admin match | — |
| `set_version` | `caller.require_auth()` | upgrade-admin match | no |
| `get_storage_stats` | read-only | — | no |

---

## insurance

| Entrypoint | Required auth | Optional / secondary check | Paused? |
|---|---|---|---|
| `init` | none (one-shot) | — | no |
| `create_policy` | `caller.require_auth()` | — | no |
| `pay_premium` | `caller.require_auth()` | `policy.owner == caller` | no |
| `batch_pay_premiums` | `caller.require_auth()` | `policy.owner == caller` per policy | no |
| `deactivate_policy` | `caller.require_auth()` | `policy.owner == caller` **or** contract owner | no |
| `set_external_ref` | `caller.require_auth()` | contract owner match | no |
| `get_active_policies` | read-only | — | no |
| `get_policy` | read-only | — | no |
| `get_total_monthly_premium` | read-only | — | no |

---

## family_wallet

| Entrypoint | Required auth | Optional / secondary check | Paused? |
|---|---|---|---|
| `init` | `owner.require_auth()` | — | no |
| `add_family_member` | `caller.require_auth()` | `is_owner_or_admin` | yes |
| `batch_add_family_members` | `caller.require_auth()` | `require_role_at_least(Admin)` | yes |
| `remove_family_member` | `caller.require_auth()` | caller is Owner (not the target) | yes |
| `batch_remove_family_members` | `caller.require_auth()` | `require_role_at_least(Owner)` | yes |
| `update_spending_limit` | `caller.require_auth()` | `is_owner_or_admin` | yes |
| `set_precision_spending_limit` | `caller.require_auth()` | `is_owner_or_admin` | yes |
| `configure_multisig` | `caller.require_auth()` | `is_owner_or_admin` | yes |
| `propose_transaction` | `proposer.require_auth()` | `require_role_at_least(Member)` + family member check | yes |
| `sign_transaction` | `signer.require_auth()` | `is_family_member` + `require_role_at_least(Member)` | yes |
| `withdraw` | via `propose_transaction` | — | yes |
| `configure_emergency` | `caller.require_auth()` | `is_owner_or_admin` | yes |
| `set_emergency_mode` | `caller.require_auth()` | `is_owner_or_admin` | yes |
| `archive_old_transactions` | `caller.require_auth()` | `is_owner_or_admin` | yes |
| `get_archived_transactions` | `caller.require_auth()` | `is_owner_or_admin` | no |
| `cleanup_expired_pending` | `caller.require_auth()` | `is_owner_or_admin` | yes |
| `revalidate_proposals` | `caller.require_auth()` | `is_owner_or_admin` | yes |
| `cancel_transaction` | `caller.require_auth()` | proposer **or** `is_owner_or_admin` | yes |
| `set_role_expiry` | `caller.require_auth()` | `require_role_at_least(Admin)` | yes |
| `set_pause_admin` | `caller.require_auth()` | `require_role_at_least(Owner)` | no |
| `set_proposal_expiry` | `caller.require_auth()` | `require_role_at_least(Owner)` | yes |
| `upgrade` | `caller.require_auth()` | `require_role_at_least(Owner)` | yes |
| `set_version` | `caller.require_auth()` | upgrade-admin match + role expiry | yes |
| `pause` | `caller.require_auth()` | `require_role_at_least(Admin)`; pause-admin match | — |
| `unpause` | `caller.require_auth()` | pause-admin match + role expiry | — |
| `propose_emergency_transfer` | via `propose_transaction` | — | yes |
| `propose_split_config_change` | via `propose_transaction` | — | yes |
| `propose_role_change` | via `propose_transaction` | — | yes |
| `propose_policy_cancellation` | via `propose_transaction` | — | yes |
| `get_member` | read-only | — | no |
| `check_spending_limit` | read-only | — | no |
| `get_pending_transaction` | read-only | — | no |
| `get_pending_transactions_page` | read-only | — | no |
| `get_multisig_config` | read-only | — | no |
| `get_family_member` | read-only | — | no |
| `get_owner` | read-only | — | no |
| `get_emergency_config` | read-only | — | no |
| `is_emergency_mode` | read-only | — | no |
| `get_spending_tracker` | read-only | — | no |
| `get_member_addresses_page` | read-only | — | no |
| `get_storage_stats` | read-only | — | no |
| `get_access_audit` | read-only | — | no |
| `get_role_expiry_public` | read-only | — | no |
| `get_upgrade_admin_public` | read-only | — | no |
| `get_proposal_expiry_public` | read-only | — | no |
| `get_version` | read-only | — | no |
| `is_paused` | read-only | — | no |
| `get_last_emergency_at` | read-only | — | no |
| `validate_precision_spending` | read-only | — | no |

---

## reporting

| Entrypoint | Required auth | Optional / secondary check | Paused? |
|---|---|---|---|
| `init` | `admin.require_auth()` | — | no |
| `propose_new_admin` | `caller.require_auth()` | `caller == admin` | no |
| `accept_admin_rotation` | `caller.require_auth()` | `caller == pending_admin` | no |
| `configure_addresses` | `caller.require_auth()` | `caller == admin` | no |
| `check_dependencies` | `caller.require_auth()` | `caller == admin` | no |
| `get_remittance_summary` | `user.require_auth()` | — | no |
| `get_savings_report` | `user.require_auth()` | — | no |
| `get_bill_compliance_report` | `user.require_auth()` | — | no |
| `get_insurance_report` | `user.require_auth()` | — | no |
| `get_family_spending_report` | `user.require_auth()` | — | no |
| `get_financial_health_report` | `user.require_auth()` | — | no |
| `get_top_bills_report` | `user.require_auth()` | — | no |
| `get_top_savings_report` | `user.require_auth()` | — | no |
| `store_report` | `user.require_auth()` | — | no |
| `get_stored_report` | `user.require_auth()` | — | no |
| `get_archived_reports` | `user.require_auth()` | — | no |
| `get_archived_reports_page` | `user.require_auth()` | — | no |
| `archive_old_reports` | `caller.require_auth()` | `caller == admin` | no |
| `cleanup_old_reports` | `caller.require_auth()` | `caller == admin` | no |
| `calculate_health_score` | read-only | — | no |
| `get_trend_analysis` | read-only | — | no |
| `get_trend_analysis_multi` | read-only | — | no |
| `get_addresses` | read-only | — | no |
| `get_admin` | read-only | — | no |
| `get_storage_stats` | read-only | — | no |

---

## orchestrator

| Entrypoint | Required auth | Optional / secondary check | Paused? |
|---|---|---|---|
| `init` | `caller.require_auth()` | — | no |
| `execute_remittance_flow` | `params.caller.require_auth()` | reentrancy lock | no |
| `execute_remittance_flow_signed` | `executor.require_auth()` | nonce + deadline + hash validation | no |
| `register_sibling_contract` | read / admin-gated | — | no |
| `update_sibling_contract` | read / admin-gated | — | no |
| `remove_sibling_contract` | read / admin-gated | — | no |
| `set_version` | `caller.require_auth()` | `caller == owner` | no |
| `get_nonce` | read-only | — | no |
| `get_execution_stats` | read-only | — | no |
| `get_audit_log` | read-only | — | no |
| `get_version` | read-only | — | no |

---

## Common patterns at a glance

| Pattern | Entrypoints | Contracts |
|---------|-------------|-----------|
| `caller.require_auth()` + owner check | `distribute_usdc`, `update_split`, `modify_remittance_schedule` | remittance_split |
| `caller.require_auth()` + resource-owner check | `add_to_goal`, `pay_bill`, `pay_premium`, `withdraw_from_goal` | savings_goals, bill_payments, insurance |
| `caller.require_auth()` + role check | `add_family_member`, `configure_multisig`, `set_pause_admin` | family_wallet |
| `caller.require_auth()` + admin check | `pause`, `unpause`, `set_version`, `archive_old_reports` | remittance_split, savings_goals, bill_payments, reporting |
| `user.require_auth()` (user-gated read) | `get_remittance_summary`, `get_unpaid_bills`, `get_stored_report` | reporting, bill_payments |
| `caller.require_auth()` (permissioned maintenance) | `archive_paid_bills`, `restore_bill`, `bulk_cleanup_bills` | bill_payments |
| no auth (read-only / permissionless) | `get_*`, `calculate_*`, `is_*`, `execute_due_*` | all contracts |

## How auth verification works in Soroban

Soroban authorization uses **envelope signatures**. When a client calls `X.require_auth()`,
the Soroban host verifies that the transaction envelope includes a valid signature from `X`
for the current call context.

Example — a user calling `distribute_usdc`:

```rust
// Inside remittance_split/src/lib.rs:
pub fn distribute_usdc(env: Env, usdc_contract: Address, from: Address, /* ... */) {
    from.require_auth();                          // (1) Must own `from`'s key
    Self::require_not_paused(&env)?;              // (2) Contract must not be paused
    let config: SplitConfig = env.storage()
        .instance().get(&symbol_short!("CONFIG"))
        .ok_or(RemittanceSplitError::NotInitialized)?;
    if config.owner != from {                     // (3) Only the configured owner
        return Err(RemittanceSplitError::Unauthorized);
    }
    // ... proceed with distribution
}
```

For role-based auth (family_wallet):

```rust
// Inside family_wallet/src/lib.rs:
pub fn add_family_member(env: Env, caller: Address, member: Address, role: FamilyRole) -> bool {
    caller.require_auth();                        // (1) Must control the caller key
    Self::require_not_paused(&env);               // (2) Not paused
    if !Self::is_owner_or_admin(&env, &caller) {  // (3) Must be Owner or Admin
        panic!("Only Owner or Admin can add family members");
    }
    // ... add member
}
```

For bound-argument auth (`initialize_split` uses this to prevent cross-function replay):

```rust
let payload = SplitAuthPayload {
    domain_id: symbol_short!("init"),
    network_id: env.ledger().network_id(),
    contract_addr: env.current_contract_address(),
    owner_addr: owner.clone(),
    nonce_val: nonce,
    // ...
};
owner.require_auth_for_args(vec![&env, payload.into_val(&env)]);
```
