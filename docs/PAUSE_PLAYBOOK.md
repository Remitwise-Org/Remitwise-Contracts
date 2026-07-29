# Pause and Unpause Playbook

**Target Audience:** Operators & System Administrators

This playbook details the emergency pause mechanisms across the Remitwise smart contracts, explaining who has the authority to halt operations, the required steps to resume them, and how contract state is affected.

## 1. Overview and Authority

Across the Remitwise smart contracts (e.g., `bill_payments`, `emergency_killswitch`), the ability to pause and unpause operations is strictly restricted to the **Pause Admin**. 

The Pause Admin is designated during contract initialization and is persisted in the contract's instance storage (typically as `PAUSE_ADM` or `DataKey::Admin`). Only the address matching this admin can successfully invoke the pause-related entrypoints.

## 2. What State Persists?

When a contract is paused, the following state changes occur in the contract's instance storage:
*   **`PAUSED` (or `GlobalPaused`) Flag:** A boolean flag is set to `true`. While this flag is true, all state-changing operations across the contract will revert.
*   **`UNP_AT` (or `UnpauseSchedule`) Timestamp:** Any previously pending unpause schedule is **deleted**. This is a destructive action designed to prevent an active timelock from bypassing a fresh emergency pause.

## 3. The Pause Lifecycle

The lifecycle of an emergency pause follows three distinct phases. 

### Phase A: Pausing the Contract
When an emergency is detected, the admin invokes the `pause` entrypoint. This takes effect immediately.

**Concrete Example:**
```bash
soroban contract invoke \
  --id C_CONTRACT_ID \
  --source admin_account \
  --network testnet \
  -- \
  pause \
  --caller admin_account
```
**Expected Output:** *The transaction succeeds, and subsequent user operations revert with `ContractPaused` or `Unauthorized`.*

### Phase B: Scheduling an Unpause (Timelock)
To protect users from sudden, unannounced resumptions of operations, unpausing is **time-locked**. The admin cannot immediately unpause the contract; they must first schedule it by providing a future ledger timestamp (in seconds).

**Concrete Example:**
*(Assuming the admin wants to schedule the unpause for a UNIX timestamp `1750000000`)*
```bash
soroban contract invoke \
  --id C_CONTRACT_ID \
  --source admin_account \
  --network testnet \
  -- \
  schedule_unpause \
  --caller admin_account \
  --at_timestamp 1750000000
```
**Expected Output:** *The `UNP_AT` storage key is populated with the provided timestamp.*

### Phase C: Executing the Unpause
Once the ledger's timestamp has passed the scheduled `UNP_AT` timestamp, the admin can formally lift the pause. 

**Concrete Example:**
```bash
soroban contract invoke \
  --id C_CONTRACT_ID \
  --source admin_account \
  --network testnet \
  -- \
  unpause \
  --caller admin_account
```
**Expected Output:** *The `PAUSED` flag is set to `false`, the `UNP_AT` timestamp is removed, and regular contract operations resume.*

> **Note:** If `unpause` is called before the ledger reaches the scheduled timestamp, the transaction will revert. If the contract gets stuck due to an invalid schedule, the `emergency_killswitch` contract may provide a global override depending on the module.
