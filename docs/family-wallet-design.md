# Family Wallet Design (As Implemented)

This document describes the current `family_wallet` contract behavior as implemented in `family_wallet/src/lib.rs`.

## Purpose

`family_wallet` provides policy controls for shared-family spending:

- Role-based access control (`Owner`, `Admin`, `Member`, `Viewer`)
- Per-transaction spending limits
- Multi-signature approval flows for high-risk actions
- Emergency transfer mode with guardrails
- Pause, upgrade-admin, and audit utilities

The contract is policy and execution oriented; token transfers are executed from a proposer address to a recipient via Soroban token contract calls.

## Role Model

Role order is numeric and used for `require_role_at_least` checks:

- `Owner` (`1`)
- `Admin` (`2`)
- `Member` (`3`)
- `Viewer` (`4`)

Lower numeric value is higher privilege.

### Role Expiry

- Optional role expiry per member is stored in `ROLE_EXP`.
- A role is considered expired when `ledger.timestamp() >= expires_at` (inclusive boundary).
- Expired roles fail `require_role_at_least` checks (`"Role has expired"` panic).
- Expired roles are also treated as **not privileged** for `Owner`/`Admin` helper checks used by permissioned methods (e.g. emergency config, cleanup, archiving).
- Expiry is set/cleared via `set_role_expiry` and only applies to existing family members (non-members are rejected).

## Permissions Matrix

| Operation | Methods | Allowed caller | Key guards |
|---|---|---|---|
| Initialize wallet | `init` | Owner address passed to `init` | One-time only (`"Wallet already initialized"` panic) |
| Add member (strict) | `add_member` | Owner or Admin | Role cannot be `Owner`; rejects duplicates; spending limit must be `>= 0`; returns `Result` |
| Add member (legacy overwrite path) | `add_family_member` | Owner or Admin | Role cannot be `Owner`; overwrites existing member record; limit forced to `0` |
| Remove member | `remove_family_member` | Owner only | Cannot remove owner |
| Update per-member spending limit | `update_spending_limit` | Owner or Admin | Member must exist; new limit must be `>= 0`; returns `Result` |
| Configure multisig | `configure_multisig` | Owner or Admin | `Result<bool, Error>` return; validates: `signers.len() > 0`; `MIN_THRESHOLD <= threshold <= MAX_THRESHOLD`; `threshold <= signers.len()`; all signers must be family members; spending limit must be `>= 0`; blocked when paused |
| Propose transaction | `propose_transaction` and wrappers (`withdraw`, `propose_*`) | `Member` or higher | Caller must be family member; blocked when paused |
| Sign transaction | `sign_transaction` | `Member` or higher | Must be in configured signer list for tx type; no duplicate signature; not expired |
| Emergency config and mode | `configure_emergency`, `set_emergency_mode` | Owner or Admin | Emergency max amount `> 0`; min balance `>= 0` |
| Pause controls | `pause`, `unpause`, `set_pause_admin` | Pause admin (pause/unpause), Owner (`set_pause_admin`) | Default pause admin is owner unless overridden |
| Upgrade controls | `set_upgrade_admin`, `set_version` | Owner (`set_upgrade_admin`), upgrade admin (`set_version`) | Emits upgrade event on version change |
| Batch member operations | `batch_add_family_members`, `batch_remove_family_members` | Admin+ for add, Owner for remove | Max batch size enforced; cannot add/remove owner |
| Storage cleanup | `archive_old_transactions`, `cleanup_expired_pending` | Owner or Admin | Blocked when paused |
| Reads | `get_*`, `is_*` | Any caller | Read-only |

## Limits and Policy Rules

### Constants

| Name | Value | Meaning |
|---|---|---|
| `SIGNATURE_EXPIRATION` | `86400` seconds | Pending multisig transaction expiry (24h) |
| `MAX_BATCH_MEMBERS` | `30` | Maximum add/remove batch size |
| `MAX_ACCESS_AUDIT_ENTRIES` | `100` | Access audit ring size (last 100 retained) |
| `MAX_SIGNERS` | `100` | Maximum number of authorized signers per multisig config |
| `MIN_THRESHOLD` | `1` | Minimum valid threshold value |
| `MAX_THRESHOLD` | `100` | Maximum valid threshold value |
| `INSTANCE_BUMP_AMOUNT` | `518400` ledgers | Active-instance TTL extension target |
| `ARCHIVE_BUMP_AMOUNT` | `2592000` ledgers | Archive TTL extension target |

### Default Configs Set During `init`

- Multisig configs for `LargeWithdrawal`, `SplitConfigChange`, `RoleChange`, `EmergencyTransfer`, `PolicyCancellation`:
  - `threshold = 2`
  - `signers = []`
  - `spending_limit = 1000_0000000`
- Emergency config:
  - `max_amount = 1000_0000000`
  - `cooldown = 3600`
  - `min_balance = 0`
- Emergency mode disabled by default.

### Spending Limit Semantics

- `check_spending_limit`:
  - Unknown caller or negative amount => `false`
  - Owner/Admin => always `true`
  - Member/Viewer with limit `0` => unlimited (`true`)
  - Positive limit => `amount <= spending_limit`
- `withdraw` thresholding uses **multisig config for `LargeWithdrawal`**:
  - `amount <= spending_limit` => `RegularWithdrawal` immediate path
  - `amount > spending_limit` => `LargeWithdrawal` multisig path

## Key Flows

### 1. Withdrawal Flow

```mermaid
sequenceDiagram
    participant U as Proposer
    participant C as FamilyWallet
    participant T as Token
    participant S as Authorized Signer

    U->>C: withdraw(token, recipient, amount)
    C->>C: load LargeWithdrawal config
    alt amount <= config.spending_limit
        C->>T: transfer(proposer -> recipient, amount)
        C-->>U: tx_id = 0 (executed)
    else amount > config.spending_limit
        C->>C: create pending tx (auto-add proposer signature)
        C-->>U: tx_id > 0
        S->>C: sign_transaction(tx_id)
        alt signatures >= threshold
            C->>T: transfer(proposer -> recipient, amount)
            C->>C: remove pending; mark executed
        else waiting for more signatures
            C->>C: store updated pending signatures
        end
    end
```

**Key Security Features:**
- **Dust Attack Prevention**: `min_precision` prevents micro-transactions that could bypass limits
- **Single Transaction Limits**: `max_single_tx` prevents large withdrawals even within period limits
- **Overflow Protection**: All arithmetic uses `saturating_add()` to prevent overflow
- **Configuration Validation**: Strict parameter validation prevents invalid configurations

### 2. Rollover Behavior

The system implements daily spending periods with secure rollover handling:

```rust
/// Spending period configuration for rollover behavior
pub struct SpendingPeriod {
    /// Period type: 0=Daily, 1=Weekly, 2=Monthly
    pub period_type: u32,
    /// Period start timestamp (aligned to period boundary)
    pub period_start: u64,
    /// Period duration in seconds (86400 for daily)
    pub period_duration: u64,
}
```

**Rollover Security:**
- **UTC Alignment**: Periods align to 00:00 UTC to prevent timezone manipulation
- **Boundary Validation**: Inclusive boundary checks prevent edge case timing attacks
- **Legitimate Rollover**: Validates rollover conditions to prevent time manipulation

### 3. Cumulative Spending Tracking

```rust
/// Cumulative spending tracking for precision validation
pub struct SpendingTracker {
    /// Current period spending amount (in stroops)
    pub current_spent: i128,
    /// Last transaction timestamp for audit trail
    pub last_tx_timestamp: u64,
    /// Transaction count in current period
    pub tx_count: u32,
    /// Period configuration
    pub period: SpendingPeriod,
}
```

**Tracking Features:**
- **Period Persistence**: Spending accumulates across transactions within the same period
- **Automatic Reset**: Counters reset to zero on legitimate period rollover
- **Audit Trail**: Transaction count and timestamps for monitoring
- **Overflow Protection**: Uses saturating arithmetic to prevent overflow attacks

## API Reference

### Configuration Functions

#### `set_precision_spending_limit`
```rust
pub fn set_precision_spending_limit(
    env: Env,
    caller: Address,           // Must be Owner or Admin
    member_address: Address,   // Target member
    precision_limit: PrecisionSpendingLimit,
) -> Result<bool, Error>
```

**Purpose**: Configure enhanced precision limits for a family member  
**Authorization**: Owner or Admin only  
**Validation**: Validates all precision parameters for security

### Validation Functions

#### `validate_precision_spending`
```rust
pub fn validate_precision_spending(
    env: Env,
    caller: Address,
    amount: i128,
) -> Result<(), Error>
```

**Purpose**: Comprehensive spending validation with precision and rollover checks  
**Flow**:
1. Basic validation (positive amount, valid member, role not expired)
2. Role-based bypass (Owner/Admin unlimited)
3. Precision validation (min_precision, max_single_tx)
4. Cumulative validation (period limits, rollover handling)

### Monitoring Functions

#### `get_spending_tracker`
```rust
pub fn get_spending_tracker(env: Env, member_address: Address) -> Option<SpendingTracker>
```

**Purpose**: Read-only access to current spending tracker for monitoring

## Security Assumptions

### 1. Precision Attack Prevention

**Dust Attack Mitigation:**
- `min_precision > 0` prevents micro-transactions
- Recommended minimum: 1 XLM (10^7 stroops) for meaningful amounts

**Overflow Protection:**
- All arithmetic uses `saturating_add()` and `saturating_sub()`
- Configuration validation prevents overflow conditions
- Boundary checks handle edge cases gracefully

### 2. Rollover Security

**Time Manipulation Prevention:**
- Period alignment to UTC boundaries prevents timezone exploitation
- Rollover validation ensures legitimate period transitions
- Inclusive boundary checks prevent timing attacks

**Example Rollover Validation:**
```rust
fn rollover_spending_period(
    old_tracker: SpendingTracker,
    current_time: u64,
) -> Result<SpendingTracker, Error> {
    let new_period = Self::get_current_period(current_time);
    
    // Validate rollover is legitimate (prevent manipulation)
    if current_time < old_tracker.period.period_start.saturating_add(old_tracker.period.period_duration) {
        return Err(Error::RolloverValidationFailed);
    }
    
    // Reset counters for new period
    Ok(SpendingTracker {
        current_spent: 0,
        last_tx_timestamp: current_time,
        tx_count: 0,
        period: new_period,
    })
}
```

### 3. Boundary Validation

**Edge Case Handling:**
- Zero and negative amounts explicitly rejected
- Maximum single transaction enforced before cumulative checks
- Period boundary calculations handle timestamp overflow
- Configuration parameters validated for consistency

## Error Handling

### New Error Types

| Error | Description | Prevention |
|-------|-------------|------------|
| `AmountBelowPrecision` | Amount below minimum precision threshold | Set appropriate `min_precision` |
| `ExceedsMaxSingleTx` | Single transaction exceeds maximum | Configure reasonable `max_single_tx` |
| `ExceedsPeriodLimit` | Cumulative spending exceeds period limit | Monitor via `get_spending_tracker` |
| `RolloverValidationFailed` | Period rollover validation failed | System prevents time manipulation |
| `InvalidPrecisionConfig` | Invalid precision configuration | Validate parameters before setting |

### Error Prevention Strategies

**Configuration Validation:**
```rust
// Validate precision configuration
if precision_limit.limit < 0 {
    return Err(Error::InvalidPrecisionConfig);
}
if precision_limit.min_precision <= 0 {
    return Err(Error::InvalidPrecisionConfig);
}
if precision_limit.max_single_tx <= 0 || precision_limit.max_single_tx > precision_limit.limit {
    return Err(Error::InvalidPrecisionConfig);
}
```

## Migration and Compatibility

### Backward Compatibility

**Legacy Support:**
- Existing members without `precision_limit` use legacy validation
- Legacy `spending_limit` field preserved
- New features are opt-in per member

**Migration Path:**
1. Deploy enhanced contract
2. Existing members continue with legacy limits
3. Gradually migrate via `set_precision_spending_limit`
4. Monitor through `get_spending_tracker`

### Configuration Examples

**Production Configuration:**
```rust
PrecisionSpendingLimit {
    limit: 10000_0000000,      // 10,000 XLM per day
    min_precision: 1_0000000,  // 1 XLM minimum (prevents dust)
    max_single_tx: 5000_0000000, // 5,000 XLM max per transaction
    enable_rollover: true,     // Enable cumulative tracking
}
```

**Conservative Configuration:**
```rust
PrecisionSpendingLimit {
    limit: 1000_0000000,       // 1,000 XLM per day
    min_precision: 5_0000000,  // 5 XLM minimum
    max_single_tx: 500_0000000, // 500 XLM max per transaction
    enable_rollover: true,
}
```

## Testing Strategy

### Test Coverage Areas

1. **Precision Validation**
   - Configuration parameter validation
   - Minimum precision enforcement
   - Maximum single transaction limits
   - Authorization checks

2. **Rollover Behavior**
   - Period alignment and boundaries
   - Spending tracker persistence
   - Legitimate rollover validation
   - Counter reset behavior

3. **Security Edge Cases**
   - Dust attack prevention
   - Overflow protection
   - Time manipulation resistance
   - Boundary condition handling

4. **Compatibility**
   - Legacy limit fallback
   - Owner/Admin bypass
   - Mixed configurations
   - Migration scenarios

### Running Tests

```bash
# Run all family wallet tests
cargo test -p family_wallet

# Run precision-specific tests
cargo test -p family_wallet test_precision
cargo test -p family_wallet test_rollover
cargo test -p family_wallet test_cumulative

# Run with detailed output
cargo test -p family_wallet -- --nocapture
```

## Performance Considerations

### Storage Efficiency

- **Minimal Footprint**: One `SpendingTracker` per member with precision limits
- **Automatic Cleanup**: Trackers reset on period rollover
- **Efficient Access**: O(1) lookups for validation

### Gas Optimization

- **Early Exits**: Owner/Admin bypass all precision checks
- **Conditional Logic**: Legacy members skip precision validation
- **Batch Operations**: Minimize storage reads/writes

## Conclusion

The enhanced spending limit system provides robust protection against precision attacks and rollover edge cases while maintaining backward compatibility. The implementation follows security best practices with comprehensive validation, overflow protection, and audit trails.

Key benefits:
- **Prevents over-withdrawal** through precision and cumulative validation
- **Secure rollover behavior** with time manipulation resistance  
- **Comprehensive testing** covering security edge cases
- **Backward compatible** with existing configurations
- **Well-documented** security assumptions and validation logic

- `add_member` is strict (duplicate-safe and limit-aware), while `add_family_member`/batch add overwrite records and force spending limit to `0`.
- `archive_old_transactions` archives all `EXEC_TXS` entries currently present; `before_timestamp` is written into archived metadata but not used as a filter.
- `SplitConfigChange` and `PolicyCancellation` transaction execution paths currently complete without cross-contract side effects.
- Token-transfer execution from `sign_transaction` path calls `proposer.require_auth()` for transfer types, so proposer authorization is required at execution time.

## Error Codes

The contract uses a comprehensive error code system for validation failures:

| Code | Value | Condition |
|---|---|---|
| `Unauthorized` | 1 | Caller lacks required role/permission |
| `InvalidThreshold` | 2 | Threshold exceeds number of signers |
| `InvalidSigner` | 3 | Signer validation failure |
| `TransactionNotFound` | 4 | Pending transaction not found |
| `TransactionExpired` | 5 | Pending transaction has expired |
| `InsufficientSignatures` | 6 | Not enough signatures collected |
| `DuplicateSignature` | 7 | Signer already signed this transaction |
| `InvalidTransactionType` | 8 | Unknown transaction type |
| `InvalidAmount` | 9 | Amount validation failure |
| `InvalidRole` | 10 | Role validation failure |
| `MemberNotFound` | 11 | Family member not found |
| `TransactionAlreadyExecuted` | 12 | Transaction was already executed |
| `InvalidSpendingLimit` | 13 | Spending limit must be >= 0 |
| `ThresholdBelowMinimum` | 14 | Threshold < MIN_THRESHOLD (1) |
| `ThresholdAboveMaximum` | 15 | Threshold > MAX_THRESHOLD (100) |
| `SignersListEmpty` | 16 | Signers list is empty |
| `SignerNotMember` | 17 | Signer is not a family member |

### Multisig Threshold Bounds Security

The `configure_multisig` function enforces strict bounds validation to prevent invalid execution policy states:

- **Minimum threshold**: `MIN_THRESHOLD = 1` - At least one signature required
- **Maximum threshold**: `MAX_THRESHOLD = 100` - Prevents unreasonably high requirements
- **Consistency check**: `threshold <= signers.len()` - Threshold cannot exceed available signers
- **Signer validation**: All signers must be existing family members
- **Empty list rejection**: At least one signer required

These bounds prevent configuration errors that could lock the wallet (threshold > signers) or render it unusable (empty signers or zero threshold).
