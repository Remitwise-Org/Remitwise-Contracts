# Reversibility Matrix

**Target Audience:** Operators & Support Teams

This matrix summarizes which smart contract operations within the Remitwise ecosystem can be reversed or undone, and which are strictly irreversible. 

For reversible actions, concrete CLI examples are provided to demonstrate how an operator can execute the undo operation.

---

## 1. Savings Goals (`savings_goals`)

| Operation | Reversibility | How to Undo / Notes |
| :--- | :--- | :--- |
| `create_goal` | Overwritable | State can't be deleted, but goal details can be abandoned. |
| `add_to_goal` | **Irreversible** | Funds are locked to the goal on-chain. |
| `archive_completed_goals` | **Reversible** | Call `restore_goal(goal_id)` to move it back to active instance storage. |
| `cleanup_old_archives` | **Irreversible** | Permanently deletes the archive from ledger state. |

**Undo Example:** Restoring an archived goal.
```bash
soroban contract invoke \
  --id C_SAVINGS_ID \
  --source admin_account \
  --network testnet \
  -- \
  restore_goal \
  --goal_id 12345
```

---

## 2. Bill Payments (`bill_payments`)

| Operation | Reversibility | How to Undo / Notes |
| :--- | :--- | :--- |
| `create_bill` | Overwritable | Can be ignored if created by mistake. |
| `pay_bill` | **Irreversible** | Funds are transferred out. Requires out-of-band manual refund. |
| `set_external_ref` | **Reversible** | Call `set_external_ref` again with a new ID, or an empty string to clear. |
| `create_bill_schedule`| **Reversible** | Call `cancel_bill_schedule(schedule_id)`. |
| `archive_paid_bills` | **Reversible** | Call `restore_bill(bill_id)` to retrieve from archive. |
| `bulk_cleanup_bills` | **Irreversible** | Permanently deletes archived bills. |

**Undo Example:** Canceling an active bill schedule.
```bash
soroban contract invoke \
  --id C_BILLS_ID \
  --source admin_account \
  --network testnet \
  -- \
  cancel_bill_schedule \
  --schedule_id 999
```

---

## 3. Insurance (`insurance`)

| Operation | Reversibility | How to Undo / Notes |
| :--- | :--- | :--- |
| `create_policy` | Overwritable | Can be ignored or immediately deactivated. |
| `pay_premium` | **Irreversible** | Premium funds transferred to provider. |
| `deactivate_policy` | **Reversible** | Call `reactivate_policy(policy_id)` to restore active status. |

**Undo Example:** Reactivating a deactivated policy.
```bash
soroban contract invoke \
  --id C_INSURANCE_ID \
  --source admin_account \
  --network testnet \
  -- \
  reactivate_policy \
  --policy_id 444
```

---

## 4. Family Wallet (`family_wallet`)

| Operation | Reversibility | How to Undo / Notes |
| :--- | :--- | :--- |
| `withdraw` | **Irreversible** | Asset transfer is final. |
| `update_spending_limit`| Overwritable | Call `update_spending_limit` again with corrected bounds. |
| `configure_emergency` | Overwritable | Call `configure_emergency` again with new paths/thresholds. |
| `set_emergency_mode` | **Reversible** | Depending on config, can be unset by admins/owner. |

**Undo Example:** Overwriting a mistaken spending limit.
```bash
soroban contract invoke \
  --id C_WALLET_ID \
  --source admin_account \
  --network testnet \
  -- \
  update_spending_limit \
  --caller admin_account \
  --member user_account \
  --daily_limit 100 \
  --monthly_limit 500 \
  --max_single_tx 50 \
  --min_precision 1
```

---

## 5. Emergency Killswitch (`emergency_killswitch`)

| Operation | Reversibility | How to Undo / Notes |
| :--- | :--- | :--- |
| `pause` | **Reversible** | Call `unpause()`. Requires waiting out the `schedule_unpause` timelock. |
| `pause_module` | **Reversible** | Call `unpause_module(module_id)`. |
| `pause_function` | **Reversible** | Call `unpause_function(module_id, func)`. |

**Undo Example:** Lifting a global pause.
```bash
soroban contract invoke \
  --id C_KILLSWITCH_ID \
  --source admin_account \
  --network testnet \
  -- \
  unpause \
  --caller admin_account
```
