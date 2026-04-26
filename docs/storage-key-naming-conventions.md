# Storage Key Naming Conventions

## Overview

This document defines the naming conventions for storage keys used across all Remitwise smart contracts. These conventions ensure consistency, readability, and compatibility with Soroban's `symbol_short!` macro constraints.

## Conventions

### 1. Maximum Length: 9 Characters

Storage keys must not exceed 9 characters in length. This is a hard constraint imposed by Soroban's `symbol_short!` macro, which pre-computes short symbols at compile time for optimal performance.

**Rationale:**
- `symbol_short!` generates compile-time constants for symbols up to 9 characters
- Symbols exceeding 9 characters require `Symbol::new()` at runtime, which is less efficient
- Shorter keys reduce storage costs and improve contract performance

**Examples:**
```rust
✅ const KEY_PAUSE_ADM: Symbol = symbol_short!("PAUSE_ADM");  // 9 chars - OK
✅ const KEY_NEXT_ID: Symbol = symbol_short!("NEXT_ID");      // 7 chars - OK
❌ const KEY_PAUSE_ADMIN: Symbol = symbol_short!("PAUSE_ADMIN"); // 11 chars - TOO LONG
```

### 2. Format: UPPERCASE_WITH_UNDERSCORES

All storage keys must use UPPERCASE letters with underscores as word separators.

**Rationale:**
- Distinguishes storage keys from other identifiers
- Follows Rust constant naming conventions
- Improves code readability and maintainability

**Valid characters:**
- Uppercase letters: `A-Z`
- Underscores: `_`
- Numbers: `0-9` (not at the start)

**Examples:**
```rust
✅ "PAUSE_ADM"   // Correct
✅ "NEXT_ID"     // Correct
✅ "MS_WDRAW"    // Correct
❌ "pause_adm"   // Lowercase not allowed
❌ "PauseAdm"    // Mixed case not allowed
❌ "PAUSE-ADM"   // Hyphens not allowed
```

### 3. No Leading or Trailing Underscores

Storage keys should not start or end with underscores.

**Examples:**
```rust
✅ "PAUSE_ADM"   // Correct
❌ "_PAUSE_ADM"  // Leading underscore
❌ "PAUSE_ADM_"  // Trailing underscore
```

### 4. No Consecutive Underscores

Avoid using multiple consecutive underscores.

**Examples:**
```rust
✅ "PAUSE_ADM"   // Correct
❌ "PAUSE__ADM"  // Consecutive underscores
```

### 5. Descriptive but Concise

Keys should be descriptive enough to understand their purpose while staying within the 9-character limit. Use common abbreviations when necessary.

**Common Abbreviations:**
- `ADM` - Admin
- `SCH` - Schedule
- `PREM` - Premium
- `REM` - Remittance
- `SAV` - Savings
- `ARCH` - Archive
- `STOR` - Storage
- `STAT` - Statistics/Status
- `CONF` - Configuration
- `EM` - Emergency
- `MS` - MultiSig
- `UNPD` - Unpaid
- `PEND` - Pending
- `EXEC` - Executed
- `UPG` - Upgrade
- `OWN` - Owner
- `IDX` - Index

## Common Storage Keys

The following keys are used consistently across multiple contracts:

| Key | Purpose | Used In |
|-----|---------|---------|
| `PAUSE_ADM` | Pause admin address | remittance_split, savings_goals, bill_payments, insurance, family_wallet |
| `PAUSED` | Global pause flag | remittance_split, savings_goals, bill_payments, insurance, family_wallet |
| `UPG_ADM` | Upgrade admin address | remittance_split, savings_goals, bill_payments, insurance, family_wallet |
| `VERSION` | Contract version | remittance_split, savings_goals, bill_payments, insurance, family_wallet |
| `NEXT_ID` | Next entity ID counter | savings_goals, bill_payments, insurance |
| `AUDIT` | Audit log entries | remittance_split, savings_goals, orchestrator |
| `NONCES` | Replay protection nonces | remittance_split, savings_goals |

## Contract-Specific Keys

### remittance_split
- `CONFIG` - Owner + percentages + initialized flag
- `SPLIT` - Ordered percentages
- `REM_SCH` - Remittance schedules
- `NEXT_RSCH` - Next remittance schedule ID

### savings_goals
- `GOALS` - Primary goal records
- `SAV_SCH` - Recurring savings schedules
- `NEXT_SSCH` - Next savings schedule ID
- `PAUSED_FN` - Per-function pause switches
- `UNP_AT` - Optional time-locked unpause timestamp

### bill_payments
- `BILLS` - Active bill records
- `ARCH_BILL` - Archived paid bills
- `STOR_STAT` - Aggregated storage metrics
- `UNPD_TOT` - Unpaid totals by owner

### insurance
- `POLICIES` - Insurance policy records
- `PREM_SCH` - Premium schedules
- `NEXT_PSCH` - Next premium schedule ID
- `OWN_IDX` - Owner index for policies

### family_wallet
- `OWNER` - Wallet owner
- `MEMBERS` - Family members and roles
- `MS_WDRAW` - Multisig config for large withdrawals
- `MS_SPLIT` - Multisig config for split changes
- `MS_ROLE` - Multisig config for role changes
- `MS_EMERG` - Multisig config for emergency transfer
- `MS_POL` - Multisig config for policy cancellation
- `MS_REG` - Config key for regular withdrawals
- `PEND_TXS` - Pending multisig transactions
- `EXEC_TXS` - Executed transaction markers
- `NEXT_TX` - Next pending tx ID
- `EM_CONF` - Emergency transfer constraints
- `EM_MODE` - Emergency mode toggle
- `EM_LAST` - Last emergency transfer timestamp
- `ARCH_TX` - Archived executed transaction metadata
- `ROLE_EXP` - Role expiry timestamps
- `ACC_AUDIT` - Rolling access audit trail
- `PROP_EXP` - Proposal expiry duration

### reporting
- `ADMIN` - Reporting admin
- `ADDRS` - Cross-contract address registry
- `REPORTS` - Active reports
- `ARCH_RPT` - Archived report summaries

### orchestrator
- `STATS` - Aggregate execution counters

## Automated Validation

Storage key naming conventions are automatically validated in CI through the `storage_key_naming_test` test suite located in `testutils/tests/storage_key_naming_test.rs`.

### Running Tests Locally

```bash
# Run all storage key validation tests
cargo test --package testutils storage_key_naming_test -- --nocapture

# Run specific test
cargo test --package testutils test_all_keys_within_max_length -- --nocapture
```

### Test Coverage

The automated tests validate:

1. **Length Constraint** - All keys are ≤ 9 characters
2. **Format Validation** - All keys use UPPERCASE_WITH_UNDERSCORES
3. **No Duplicates** - No duplicate keys within a contract
4. **Non-Empty** - All keys are non-empty strings
5. **No Leading Underscores** - Keys don't start with `_`
6. **No Trailing Underscores** - Keys don't end with `_`
7. **No Consecutive Underscores** - Keys don't contain `__`
8. **Documentation Coverage** - All keys have descriptions
9. **Consistency Check** - Common keys are used consistently

### CI Integration

The storage key validation runs automatically on every pull request and push to main through the GitHub Actions workflow defined in `.github/workflows/ci.yml`.

The `storage-key-validation` job will fail if any naming convention violations are detected, preventing non-compliant keys from being merged.

## Adding New Storage Keys

When adding a new storage key:

1. **Choose a descriptive name** that clearly indicates the key's purpose
2. **Keep it under 9 characters** using abbreviations if necessary
3. **Use UPPERCASE_WITH_UNDERSCORES** format
4. **Check for consistency** - if a similar key exists in other contracts, use the same name
5. **Update the test** - add the new key to `testutils/tests/storage_key_naming_test.rs`
6. **Document it** - add the key to `STORAGE_LAYOUT.md` and this document
7. **Run tests** - verify all validation tests pass before committing

### Example: Adding a New Key

```rust
// ✅ Good example
const KEY_RATE_LIM: Symbol = symbol_short!("RATE_LIM");  // 8 chars, clear purpose

// ❌ Bad examples
const KEY_RATE_LIMITER: Symbol = symbol_short!("RATE_LIMITER");  // 12 chars - too long
const KEY_RL: Symbol = symbol_short!("RL");  // 2 chars - too cryptic
const KEY_rate_lim: Symbol = symbol_short!("rate_lim");  // lowercase - wrong format
```

## References

- [STORAGE_LAYOUT.md](../STORAGE_LAYOUT.md) - Complete storage layout documentation
- [Soroban Storage Documentation](https://developers.stellar.org/docs/build/smart-contracts/example-contracts/storage)
- [Soroban Symbol Documentation](https://docs.rs/soroban-sdk/latest/soroban_sdk/struct.Symbol.html)

## Changelog

- **2025-01-XX** - Initial documentation created with automated validation tests
