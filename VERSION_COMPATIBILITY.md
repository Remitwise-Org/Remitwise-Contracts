# Remitwise-Contracts Version Compatibility Matrix

This document defines the versioning strategy and compatibility guarantees for the Remitwise-Contracts workspace. It ensures that contract upgrades (WASM updates) preserve data integrity and maintain cross-contract interoperability.

## 1. Versioning Strategy

Remitwise-Contracts use a single-integer versioning system (`CONTRACT_VERSION`) defined in `remitwise-common`. 

- **Incremental Upgrades**: Each contract upgrade MUST increment the `CONTRACT_VERSION` if storage structures or logic Change.
- **State Preservation**: Soroban contracts preserve their `instance`, `temporary`, and `persistent` storage across WASM upgrades if the contract ID remains the same.
- **Backward Compatibility**: Newer versions of contracts MUST be able to deserialize and process data written by previous versions.

## 2. Compatibility Matrix

| Component | V1 (Current) | V2 (Planned) | Compatibility Requirement |
|-----------|--------------|--------------|---------------------------|
| **RemittanceSplit** | `1` | `2` | Must preserve split percentages and owner. |
| **SavingsGoals** | `1` | `2` | Must preserve all existing goal balances and deadlines. |
| **BillPayments** | `1` | `2` | Must preserve unpaid bills and recurring settings. |
| **Insurance** | `1` | `2` | Must preserve all active policies and premium dates. |
| **Orchestrator** | `1` | `2` | Must maintain cross-contract call signatures. |

## 3. Migration Scenarios

### Scenario A: Smooth In-Place Upgrade
- **Action**: Replace WASM for a contract while keeping the same ID.
- **Validation**: Contract functions correctly with pre-existing data without re-initialization.

### Scenario B: Data Transformation
- **Action**: V2 introduces a new field in a `contracttype` struct.
- **Validation**: V2 logic handles cases where the field is missing (defaulting) or migrates the record on first access.

### Scenario C: Cross-Contract Version Mismatch
- **Action**: Orchestrator (V1) calls a V2 downstream contract.
- **Validation**: Call signatures remain compatible or Orchestrator is updated to handle multiple versions.

## 4. Security Assumptions
- **Authorization**: Upgrades MUST NOT reset or bypass `require_auth` checks.
- **Ownership**: Contract ownership/admin roles MUST persist across upgrades.
- **TTL**: Upgrades should trigger a mandatory `extend_ttl` on critical instance storage.

## 5. Verification
Verification is performed via the `integration_tests` suite:
- `test_contract_upgrade_compatibility`
- `test_version_matrix_interoperability`
- `test_v2_data_migration_simulation` (New)
