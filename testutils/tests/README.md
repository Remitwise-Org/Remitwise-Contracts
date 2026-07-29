# Storage Key Naming Convention Tests

## Overview

This directory contains automated tests that validate storage key naming conventions across all Remitwise smart contracts. These tests ensure that all storage keys comply with Soroban's `symbol_short!` constraints and maintain consistency across the codebase.

## Test Files

- **`storage_key_naming_test.rs`** - Comprehensive validation suite for a hand-maintained catalogue of documented storage keys
- **`storage_key_source_scan_test.rs`** - Parses each contract crate's `src/lib.rs` directly, extracts every storage-key literal actually passed to a Soroban storage accessor (`.get()`, `.set()`, `.has()`, `.remove()`), and re-validates those against the same conventions. This catches drift that the hand-maintained catalogue can miss: a key can be added or renamed in source without anyone remembering to update `storage_key_naming_test.rs`, and this scan will still catch a convention violation because it reads the real code, not a snapshot of it.
- **`reserved_storage_keys_test.rs`** - Parses the reserved-key table out of [`docs/RESERVED_STORAGE_KEYS.md`](../../docs/RESERVED_STORAGE_KEYS.md) and cross-checks it against the same live source scan used above, failing if any contract stores data under a key set aside for a future roadmap feature (yield generation, staking, the V2 migration path, etc.).

## What Gets Validated

### 1. Length Constraints (≤ 9 characters)
Ensures all storage keys fit within the `symbol_short!` macro's 9-character limit. Keys exceeding this limit must use `Symbol::new()` at runtime, which is less efficient.

### 2. Format Validation (UPPERCASE_WITH_UNDERSCORES)
Validates that all keys use uppercase letters with underscores as separators, following Rust constant naming conventions.

### 3. Character Set Validation
Ensures keys only contain valid characters: `A-Z`, `0-9`, and `_`.

### 4. No Duplicate Keys
Checks that each contract doesn't have duplicate storage key definitions.

### 5. No Empty Keys
Validates that all keys are non-empty strings.

### 6. No Leading/Trailing Underscores
Ensures keys don't start or end with underscores.

### 7. No Consecutive Underscores
Validates that keys don't contain multiple consecutive underscores.

### 8. Documentation Coverage
Ensures every storage key has a description explaining its purpose.

### 9. Consistency Checks
Identifies common keys used across multiple contracts and validates they're used consistently.

## Source Scan (Drift Detection)

`storage_key_source_scan_test.rs` runs the same length/format/underscore
checks (items 1, 2, 6, 7 above) directly against every storage-key literal
it finds in each contract's `src/lib.rs`, plus a scanner self-check that a
fixed set of well-known keys are still found (guards against the parsing
heuristic silently regressing). It recognizes two shapes:

1. An inline literal: `env.storage().instance().get(&symbol_short!("KEY"))`
2. A local named constant used at a storage call site:
   `const KEY: Symbol = symbol_short!("KEY");` ... `.set(&KEY, &v)`

`savings_goals` and `insurance` mostly use a composite `DataKey` enum
instead of literal keys, so the scan naturally finds fewer entries for
those two crates - see [STORAGE_LAYOUT.md](../../STORAGE_LAYOUT.md).

## Reserved Key Enforcement

`reserved_storage_keys_test.rs` reuses the same key-extraction approach as
the source scan, but checks against a different list: the "reserved for a
future feature" keys documented in
[`docs/RESERVED_STORAGE_KEYS.md`](../../docs/RESERVED_STORAGE_KEYS.md)
(e.g. `YIELD_CFG`, `STAKE_POL`, `REWARD_CF`). It parses that document's
table directly, so the doc itself stays the single source of truth - remove
a row there once the feature ships, and the test stops treating that key as
reserved without any test-code change.

Three checks:

1. **Parser sanity** - the expected reserved keys are actually found in the
   doc's table (guards against a markdown format change silently emptying
   the reserved set).
2. **Enforcement (happy path)** - no crate in `SCANNED_CRATES` currently
   uses a reserved key.
3. **Detector proof (failure mode)** - runs the same extraction + check
   against a synthetic snippet that intentionally reuses a reserved key,
   proving the detector actually flags it.

## Running the Tests

### Run All Storage Key Tests

```bash
cargo test --package testutils storage_key_naming_test -- --nocapture
```

### Run Specific Test

```bash
# Test length constraints
cargo test --package testutils test_all_keys_within_max_length -- --nocapture

# Test format validation
cargo test --package testutils test_all_keys_uppercase_with_underscores -- --nocapture

# Test for duplicates
cargo test --package testutils test_no_duplicate_keys_within_contract -- --nocapture

# View storage key summary
cargo test --package testutils test_print_storage_key_summary -- --nocapture
```

### Run the Source Scan Only

```bash
cargo test --package testutils --test storage_key_source_scan_test -- --nocapture
```

### Run the Reserved Key Enforcement Check Only

```bash
cargo test --package testutils --test reserved_storage_keys_test -- --nocapture
```

### Run from Workspace Root

```bash
# From the repository root
cargo test --package testutils -- --nocapture
```

## Test Output

### Successful Run

```
running 10 tests
✅ All 74 storage keys are within 9 character limit
✅ All 74 storage keys use UPPERCASE_WITH_UNDERSCORES format
✅ No duplicate keys within any contract
✅ All storage keys are non-empty
✅ No storage keys start with underscore
✅ No storage keys end with underscore
✅ No storage keys have consecutive underscores
✅ All 74 storage keys have descriptions
✅ Common key 'PAUSE_ADM' used consistently across 5 contracts
✅ Common key 'VERSION' used consistently across 5 contracts

📊 Storage Key Summary:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Total storage keys: 74

Keys per contract:
  • bill_payments: 11 keys
  • family_wallet: 23 keys
  • insurance: 11 keys
  • orchestrator: 2 keys
  • remittance_split: 10 keys
  • reporting: 5 keys
  • savings_goals: 12 keys
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Failed Run (Example)

```
running 10 tests

Storage key length violations found:
❌ remittance_split.NEXT_RSCH_ID: 'NEXT_RSCH_ID' exceeds max length 9 (actual: 12)
❌ bill_payments.ARCHIVED_BILLS: 'ARCHIVED_BILLS' exceeds max length 9 (actual: 14)

test test_all_keys_within_max_length ... FAILED
```

## Adding New Storage Keys

When adding a new storage key to any contract:

1. **Add the key definition to the contract**
   ```rust
   const KEY_NEW_KEY: Symbol = symbol_short!("NEW_KEY");
   ```

2. **Update the test file** (`storage_key_naming_test.rs`)
   
   Add your key to the `get_all_storage_keys()` function:
   ```rust
   StorageKey {
       key: "NEW_KEY",
       contract: "your_contract",
       description: "Description of what this key stores",
   },
   ```

3. **Run the tests**
   ```bash
   cargo test --package testutils storage_key_naming_test -- --nocapture
   ```

4. **Fix any violations** reported by the tests

5. **Update documentation**
   - Add the key to `STORAGE_LAYOUT.md`
   - Add the key to `docs/storage-key-naming-conventions.md`

Note: `storage_key_source_scan_test.rs` validates length/format automatically
against the real source, so a non-compliant key literal fails CI even if step
2 (updating the catalogue) is forgotten. The catalogue update in step 2 is
still required for documentation coverage and consistency checks, which only
run against the curated list.

## CI Integration

These tests run automatically in CI through the GitHub Actions workflow (`.github/workflows/ci.yml`). The `storage-key-validation` job will fail if any naming convention violations are detected, either in the hand-maintained catalogue or in the live source scan.

### CI Job Configuration

```yaml
storage-key-validation:
  name: Storage Key Naming Convention Validation
  runs-on: macos-latest
  timeout-minutes: 5

  steps:
    - name: Run storage key naming convention tests
      run: |
        cargo test --package testutils storage_key_naming_test -- --nocapture
      continue-on-error: false

    - name: Run storage key source scan (drift detection)
      run: |
        cargo test --package testutils --test storage_key_source_scan_test -- --nocapture
      continue-on-error: false

    - name: Run reserved storage key enforcement check
      run: |
        cargo test --package testutils --test reserved_storage_keys_test -- --nocapture
      continue-on-error: false
```

## Maintenance

### Updating the Test Suite

The test suite should be updated when:

1. **New contracts are added** - Add all storage keys from the new contract
2. **New keys are added to existing contracts** - Add the new keys to the test
3. **Keys are renamed** - Update the key name in the test
4. **Keys are removed** - Remove the key from the test
5. **Conventions change** - Update validation logic and documentation

### Test Data Structure

Storage keys are defined using the `StorageKey` struct:

```rust
struct StorageKey {
    key: &'static str,          // The actual key string (e.g., "PAUSE_ADM")
    contract: &'static str,     // Contract name (e.g., "bill_payments")
    description: &'static str,  // Human-readable description
}
```

All keys are centralized in the `get_all_storage_keys()` function for easy maintenance.

## Troubleshooting

### Test Fails: "exceeds max length"

**Problem:** A storage key is longer than 9 characters.

**Solution:** Shorten the key using abbreviations. See `docs/storage-key-naming-conventions.md` for common abbreviations.

### Test Fails: "Invalid character"

**Problem:** A storage key contains lowercase letters or invalid characters.

**Solution:** Convert to UPPERCASE and replace invalid characters with underscores.

### Test Fails: "Duplicate key"

**Problem:** The same key is defined multiple times in a contract.

**Solution:** Remove the duplicate definition or rename one of the keys.

### Test Fails: "Missing description"

**Problem:** A storage key in the test doesn't have a description.

**Solution:** Add a meaningful description explaining what the key stores.

## References

- [Storage Key Naming Conventions](../../docs/storage-key-naming-conventions.md)
- [Reserved Storage Keys](../../docs/RESERVED_STORAGE_KEYS.md)
- [Storage Layout Documentation](../../STORAGE_LAYOUT.md)
- [Soroban Symbol Documentation](https://docs.rs/soroban-sdk/latest/soroban_sdk/struct.Symbol.html)
- [Soroban Storage Example](https://developers.stellar.org/docs/build/smart-contracts/example-contracts/storage)

## Contact

For questions or issues with the storage key validation tests, please:
1. Review the documentation in `docs/storage-key-naming-conventions.md`
2. Check existing test output for guidance
3. Open an issue in the repository with the `testing` label
