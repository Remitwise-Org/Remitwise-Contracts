# View Functions Read-Only Enforcement

## Summary

Adds a grep-based CI check that view functions (`get_*`, `is_*`) never write storage, implementing defense-in-depth against state-mutation-via-query attacks.

Closes #[ISSUE_NUMBER_TO_BE_FILLED]

## Motivation

View functions are query entrypoints expected to have no side effects. If a view function can call `env.storage().*.set()`, `.remove()`, or `.extend_ttl()`, an attacker gains the ability to:

### Threat Model

1. **Hidden State Changes**: Mutate contract state through seemingly innocuous read operations, bypassing audit trails and event logs designed for write operations.

2. **Audit Trail Bypass**: Since view functions are often called without strict authorization checks (or with relaxed auth), they can be exploited to alter state without triggering proper authorization flow or emitting expected events.

3. **Denial of Service**: Exhaust storage resources through repeated "read" calls that actually write data, degrading contract performance or causing unexpected failures.

4. **Query Manipulation**: Off-chain indexers and frontends expect view functions to be side-effect free. Storage writes in view functions cause inconsistencies between on-chain state and indexed data.

5. **Replay Attacks**: If a view function writes based on its inputs, an attacker can craft queries that manipulate state in ways the contract owner did not authorize.

## Changes

### 1. Static Analysis Script

**File**: `scripts/check_view_functions_readonly.sh`

- Scans all contract `lib.rs` files for `pub fn get_*` and `pub fn is_*` functions
- Detects storage mutations via regex patterns:
  - `env.storage().*.set(` — writes a value to storage
  - `env.storage().*.remove(` — deletes a storage entry
  - `env.storage().*.extend_ttl(` — mutates TTL (treated as a write)
- Exits 1 if any violations found, 0 otherwise
- Reports contract name, line number, function name, and the offending line of code

### 2. Negative Tests

**File**: `testutils/tests/view_fn_readonly_test.rs`

Tests verify the check script correctly:
- **Happy path**: Passes when view functions only call `storage().*.get()`
- **Negative paths** (all must be detected):
  - `get_*` function calling `.set()`
  - `is_*` function calling `.set()`
  - `get_*` function calling `.remove()`
  - `get_*` function calling `.extend_ttl()`

Each test uses inline bash scripts mirroring the check's core detection logic, avoiding dependencies on the full script's hard-coded contract list.

## Current Violations (Pre-fix Baseline)

The check currently detects **existing violations** in the codebase across multiple contracts:

- **remittance_split**: `get_version`, `get_upgrade_admin_public`, `get_pause_admin_public`, `get_treasury_public`, `get_pending_treasury_public`, `get_audit_log`, `is_paused`
- **savings_goals**: `get_version`, `get_upgrade_admin_public`, `get_goals_by_tag`, `get_nonce`, `get_audit_log`, `get_savings_schedules`, `get_savings_schedule`, `is_paused`
- **bill_payments**: `get_pause_admin_public`, `get_version`, `get_upgrade_admin_public`, `is_paused`, `is_function_paused_public`
- **insurance**: `get_policy`, `get_total_monthly_premium`, `get_version`
- **family_wallet**: `get_archived_transactions`, `get_storage_stats`
- **orchestrator**: `get_nonce`, `get_execution_stats`, `get_version`, `get_execution_state`
- **reporting**: (no direct violations detected in view functions)
- **emergency_killswitch**: (no direct violations detected)

These violations are logged by the script output. **This PR adds the check; fixing the violations is tracked separately** to avoid scope creep and ensure the check itself is reviewed independently.

## Testing

### Unit Tests

```bash
cargo test -p testutils view_fn_readonly_test
```

Expected:
- ✅ `test_view_fn_read_only_passes_clean_fixture` — passes (no violations for clean code)
- ✅ `test_get_fn_writing_storage_is_detected` — passes (detects `.set()` in `get_*`)
- ✅ `test_is_fn_writing_storage_is_detected` — passes (detects `.set()` in `is_*`)
- ✅ `test_get_fn_removing_storage_is_detected` — passes (detects `.remove()`)
- ✅ `test_get_fn_extending_ttl_is_detected` — passes (detects `.extend_ttl()`)
- ✅ `test_check_script_exists_and_is_executable` — passes (script exists and is executable)

### Integration Tests

```bash
bash scripts/check_view_functions_readonly.sh
```

Expected:
- **Exit 1** (violations found in current codebase)
- Reports all view functions that write storage with file paths, line numbers, and function names

## CI Integration

This check can be added to `.github/workflows/ci.yml` as a new job:

```yaml
view-function-check:
  name: View Function Read-Only Check
  runs-on: ubuntu-latest
  timeout-minutes: 5

  steps:
    - name: Checkout code
      uses: actions/checkout@v4

    - name: Check view functions are read-only
      run: |
        bash scripts/check_view_functions_readonly.sh
```

**Note**: The check will initially **fail** in CI until the existing violations are fixed. A follow-up PR should address the violations contract-by-contract.

## Defense-in-Depth

This check complements existing security measures:

- **Authorization checks**: Prevent unauthorized callers from executing mutations
- **Event logging**: Ensure state changes are auditable
- **Gas metering**: Prevent resource exhaustion
- **Static analysis**: Catch violations before deployment (this PR)

## Follow-up Work

After this PR merges:

1. **Fix existing violations**: Create contract-specific PRs to:
   - Move storage writes out of view functions into separate mutation entrypoints
   - Add proper authorization to mutation entrypoints
   - Emit events for state changes
   - Update documentation

2. **Enable CI enforcement**: Once violations are fixed, add the check to CI as a required check that blocks PRs with violations.

3. **Documentation**: Update the [THREAT_MODEL.md](THREAT_MODEL.md) to reference this defense layer.

## Reviewers

Please verify:

- [ ] The grep patterns correctly identify storage mutations (`.set()`, `.remove()`, `.extend_ttl()`)
- [ ] The grep patterns do NOT incorrectly flag storage reads (`.get()`)
- [ ] The threat model accurately describes the risks being mitigated
- [ ] The negative tests exercise all mutation types (set, remove, extend_ttl)
- [ ] The script is executable and follows existing script conventions

## Category

🔒 Security · Defense-in-Depth

## Campaign

FWC26
