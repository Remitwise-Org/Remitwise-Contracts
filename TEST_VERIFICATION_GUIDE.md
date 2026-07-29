# Step-by-Step Verification Process for Policy Cap Implementation

This document provides a clear, step-by-step process to verify that the assignment has been successfully completed. Follow each step in order to validate the implementation.

## Phase 1: Code Compilation and Syntax Validation

### Step 1: Verify No Compilation Errors

```bash
# Navigate to the workspace
cd /workspaces/Remitwise-Contracts

# Run a clean check (no test execution)
cargo check -p insurance

# Expected Result: BUILD SUCCESS with no errors or warnings
```

**What to look for:**
- No compiler errors
- All dependency warnings are pre-existing (not related to your changes)

### Step 2: Build the Insurance Contract

```bash
cargo build -p insurance --lib

# Expected Result: Successful WASM build
```

**What to look for:**
- Compilation succeeds
- No linker errors

## Phase 2: Test Execution

### Step 3: Run All Insurance Tests

```bash
# Run all insurance contract tests
cargo test -p insurance --lib

# Expected Result: All tests pass
```

**What to look for:**
- All tests pass (0 failures)
- Tests show:
  - cap_boundary_at_49_succeeds ✓
  - cap_boundary_at_50_succeeds ✓
  - cap_over_limit_returns_error ✓
  - cap_is_per_owner_not_global ✓
  - cap_deactivate_frees_slot ✓
  - cap_archive_active_policy_frees_slot_for_new ✓
  - stats_increments_on_create ✓
  - stats_decrements_on_deactivate ✓
  - restore_at_cap_returns_false ✓
  - get_active_policies_pagination_consistency ✓
  - And all other tests...

### Step 4: Run Policy Cap Tests Specifically

```bash
cargo test -p insurance caps_and_stats_tests --lib

# Expected Result: All 25+ tests pass
```

### Step 5: Run Tests with Output (Verbose)

```bash
cargo test -p insurance --lib -- --nocapture

# Expected Result: Detailed output showing all tests passing
```

**What to look for:**
- Test names clearly indicate what they're testing:
  - `cap_*` tests for capacity boundary checks
  - `stats_*` tests for storage stat counters
  - `restore_*` tests for archive/restore functionality
  - `deactivate_*` tests for deactivation

### Step 6: Test Individual Boundary Cases

Run these specific tests to confirm critical boundary behavior:

```bash
# Test the exact 49-policy mark
cargo test cap_boundary_at_49 --lib

# Test the exact 50-policy cap
cargo test cap_boundary_at_50 --lib

# Test create at 51 (should fail)
cargo test cap_over_limit_returns_error --lib

# Test deactivate frees a slot
cargo test cap_deactivate_frees_slot --lib

# Test restore at cap (should fail)
cargo test restore_at_cap_returns_false --lib

# Expected: Each test passes individually
```

## Phase 3: Code Review and Documentation

### Step 7: Verify Documentation File Exists and is Complete

```bash
# Check the documentation file
cat docs/insurance-policy-cap.md

# Expected: File exists with complete sections:
# - Overview of MAX_POLICIES_PER_OWNER
# - Storage keys (KEY_OWNER_ACTIVE, KEY_OWNER_INDEX, KEY_EXT_REF_IDX)
# - Policy lifecycle (create, deactivate, archive, restore)
# - Security assumptions
# - Pagination guarantees
# - Examples and scenarios
```

**Verification checklist:**
- [ ] File exists at `docs/insurance-policy-cap.md`
- [ ] Contains explanation of `OWN_ACT` and `OWN_IDX` indexes
- [ ] Explains cap enforcement at 50 policies
- [ ] Describes slot freeing via deactivate/archive
- [ ] Explains restore behavior and cap rechecking
- [ ] Contains scenario examples
- [ ] Is at least 400+ lines of detailed documentation

### Step 8: Review Code Comments in lib.rs

```bash
# Check for inline documentation in lib.rs
grep -A 5 "OWN_ACT\|OWN_IDX\|active_count" insurance/src/lib.rs | head -50

# Expected: Multiple inline doc comments explaining index accounting
```

**Key areas to verify have documentation:**
- [ ] `create_policy()` explains cap check and OWN_ACT increment
- [ ] `deactivate_policy()` explains active count decrement
- [ ] `archive_policy()` explains slot freeing
- [ ] `restore_policy()` explains cap re-checking
- [ ] `owner_active_count()` explains it reads from OWN_ACT
- [ ] `adjust_owner_active()` explains the increment/decrement mechanism
- [ ] Storage key constants have comments explaining their purpose

### Step 9: Verify Test Coverage

```bash
# Count the number of tests in the caps_and_stats_tests.rs file
grep "#\[test\]" insurance/tests/caps_and_stats_tests.rs | wc -l

# Expected: 25+ tests

# List all test names
grep "fn test_\|fn cap_\|fn stats_\|fn restore_\|fn deactivate_" insurance/tests/caps_and_stats_tests.rs | grep -v "//"
```

**Minimum test coverage required:**
- [ ] At least 5 cap boundary tests
- [ ] At least 5 stats update tests
- [ ] At least 3 deactivate tests
- [ ] At least 3 archive tests
- [ ] At least 3 restore tests
- [ ] At least 2 pagination tests
- [ ] Total: 25+ distinct test cases

## Phase 4: Functional Validation

### Step 10: Manual Test Scenario - Create at Boundary

```bash
# Create a simple test runner (or add to test file):
# 1. Create owner with 0 policies
# 2. Create 49 policies (below cap)
# 3. Create 50th policy (at cap, should succeed)
# 4. Attempt create 51st (should return PolicyLimitExceeded)
# 5. Deactivate 1 policy
# 6. Create again (should succeed, back at 50)
```

**Expected behavior:**
- Create 1-50: All succeed
- Create 51: Returns `Err(PolicyLimitExceeded)`
- After deactivate: Create again succeeds
- Final active count: 50

### Step 11: Manual Test Scenario - Deactivate-Archive-Restore Cycle

```bash
# Test scenario:
# 1. Create 50 policies (at cap)
# 2. Deactivate policy #1 (active count = 49)
# 3. Archive policy #1 (active count stays 49, archived = 1)
# 4. Create new policy (active count = 50, back at cap)
# 5. Attempt restore policy #1 (should fail, already at cap)
# 6. Deactivate new policy (active count = 49)
# 7. Restore policy #1 (should succeed, active count = 50)
```

**Expected storage stats at each step:**
```
Step 1: active=50, archived=0
Step 2: active=49, archived=0
Step 3: active=49, archived=1
Step 4: active=50, archived=1
Step 5: active=50, archived=1 (restore failed)
Step 6: active=49, archived=1
Step 7: active=50, archived=0
```

### Step 12: Verify Pagination Consistency

```bash
# Test that pagination doesn't duplicate or lose policies:
# 1. Create 5 policies
# 2. Fetch all with pagination (limit=2)
# 3. Collect results across all pages
# 4. Verify:
#    - Total count across pages = 5
#    - No policy appears twice
#    - Inactive policies excluded
#    - Results are ordered by policy ID
```

## Phase 5: Integration and Finalization

### Step 13: Run Full Insurance Test Suite

```bash
cargo test -p insurance

# Expected: ALL tests pass (including existing tests)
```

### Step 14: Check Git Status

```bash
cd /workspaces/Remitwise-Contracts
git status

# Expected files modified/added:
# - insurance/src/lib.rs (fixed and enhanced)
# - insurance/tests/caps_and_stats_tests.rs (enhanced)
# - docs/insurance-policy-cap.md (new documentation)
```

### Step 15: Verify Branch

```bash
git branch

# Expected: Currently on feature/ins-policy-cap-index-tests
# or main branch
```

## Summary of Success Criteria

Your assignment is **successfully completed** if all of the following are true:

✓ **Code Quality**
  - [ ] No compilation errors in lib.rs
  - [ ] No compilation errors in tests
  - [ ] All syntax is valid Rust

✓ **Test Coverage** (Minimum 95%)
  - [ ] 25+ distinct test cases
  - [ ] All tests passing
  - [ ] Tests cover:
    - Cap boundary (49, 50, 51)
    - Deactivate slot freeing
    - Archive slot freeing
    - Restore cap checking
    - Pagination consistency
    - Stats counter accuracy

✓ **Documentation**
  - [ ] docs/insurance-policy-cap.md exists and is comprehensive (400+ lines)
  - [ ] Contains explanation of OWN_ACT and OWN_IDX indexes
  - [ ] Explains cap enforcement mechanism
  - [ ] Contains scenario examples
  - [ ] Inline comments in lib.rs functions explain index accounting

✓ **Security Assumptions Validated**
  - [ ] No unbounded growth (cap prevents >50 active policies)
  - [ ] No lock-out (deactivate/archive always free slots)
  - [ ] Index consistency (EXT_IDX only has active policies)

✓ **Storage Stats Determinism**
  - [ ] active_policies count is accurate
  - [ ] archived_policies count is accurate
  - [ ] Counters updated atomically
  - [ ] Stats never become inconsistent

## Troubleshooting

### If Tests Fail

1. **Check compilation errors:**
   ```bash
   cargo check -p insurance
   ```

2. **Run specific test with backtrace:**
   ```bash
   RUST_BACKTRACE=1 cargo test -p insurance <test_name> -- --nocapture
   ```

3. **Verify the cap constant:**
   ```bash
   grep "MAX_POLICIES_PER_OWNER" insurance/src/lib.rs
   # Should show: pub const MAX_POLICIES_PER_OWNER: u32 = 50;
   ```

4. **Check for missing helper functions:**
   ```bash
   grep "fn owner_active_count\|fn adjust_owner_active\|fn sorted_unique_ids\|fn advance_next_payment_date" insurance/src/lib.rs
   # All four should exist
   ```

### If Documentation is Missing

```bash
ls -la docs/insurance-policy-cap.md
# File should exist and be > 1KB
```

### If Pagination Tests Fail

Verify that `get_active_policies()` correctly filters by `active = true` and uses `OWN_IDX` for bounded iteration.

## Final Verification Command

Run this single command to validate everything:

```bash
cd /workspaces/Remitwise-Contracts && \
cargo test -p insurance --lib 2>&1 | tee test_results.txt && \
echo "TEST SUMMARY:" && \
tail -20 test_results.txt && \
echo "DOCUMENTATION:" && \
ls -la docs/insurance-policy-cap.md && \
echo "CODE CHECK:" && \
cargo check -p insurance && \
echo "✅ All validations passed!"
```

**Expected output:**
```
✓ test result: ok. 25+ passed; 0 failed
✓ docs/insurance-policy-cap.md exists
✓ Checking insurance v... ok
✅ All validations passed!
```
