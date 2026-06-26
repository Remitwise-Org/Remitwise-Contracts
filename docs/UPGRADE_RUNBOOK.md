# Contract Upgrade Runbook

This document is for **operators** performing a live contract upgrade using `soroban-cli`. It covers the step-by-step process of preparing the environment, executing the upgrade, verifying the changes, and rolling back if necessary.

## Prerequisites

Ensure you have the following before starting:

- The latest compiled WASM file for the contract (e.g., `target/wasm32-unknown-unknown/release/savings_goals.wasm`).
- A funded administrative account (`UPG_ADM`) capable of submitting upgrade transactions on the target network.
- The `soroban-cli` installed and configured for the target network (`testnet` or `mainnet`).
- The deployed contract ID to be upgraded (e.g., `CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX`).

## 1. Prepare for Upgrade

Before pushing the upgrade, install the new binary to the network to obtain its WASM hash.

```bash
soroban contract install \
  --wasm target/wasm32-unknown-unknown/release/savings_goals.wasm \
  --source admin-account \
  --network testnet
```

This command installs the WASM code on the ledger and outputs the new **WASM hash**. Save this hash, as you will need it for the upgrade step.

Example output:
`bf2117565406086937eec93de3f18e5b41ea87d15df3421160af866d214e25a2`

## 2. Execute the Upgrade

Invoke the `upgrade` function on the target contract, passing the new WASM hash obtained from the install step. Ensure you are signing with the account that holds the `UPG_ADM` role.

```bash
soroban contract invoke \
  --id CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX \
  --source admin-account \
  --network testnet \
  -- \
  upgrade \
  --new_wasm_hash bf2117565406086937eec93de3f18e5b41ea87d15df3421160af866d214e25a2
```

If the upgrade requires any state migration (e.g., new storage keys or schema changes), run the migration function immediately after the upgrade:

```bash
soroban contract invoke \
  --id CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX \
  --source admin-account \
  --network testnet \
  -- \
  migrate
```

*(Note: Adjust the migration function name and parameters according to the specific contract's requirements.)*

## 3. Verify the Upgrade

Invoke a read-only function (e.g., `version`) to confirm the contract is running the new logic.

```bash
soroban contract invoke \
  --id CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX \
  --network testnet \
  -- \
  version
```

Verify that the returned version matches the expected new version.

## 4. Rollback Plan

If the upgrade causes unexpected issues, you can roll back to the previous version using the previously installed WASM hash.

1. **Identify the previous WASM hash**: Retrieve the WASM hash that was active before the upgrade.
2. **Execute the rollback**: Invoke the `upgrade` function again, passing the old WASM hash.

```bash
soroban contract invoke \
  --id CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX \
  --source admin-account \
  --network testnet \
  -- \
  upgrade \
  --new_wasm_hash <OLD_WASM_HASH>
```

3. **Revert state (if necessary)**: If the faulty upgrade corrupted or incorrectly migrated state, you may need to run a dedicated state recovery script or restore from an off-chain snapshot. This is highly specific to the contract and the nature of the corruption.

## 5. Post-Upgrade Operations

Once the upgrade is verified:

1. Update the contract documentation to reflect any changes.
2. Notify downstream integrators or frontend teams if APIs or event formats have changed.
3. Monitor contract logs and events for any unusual activity.
