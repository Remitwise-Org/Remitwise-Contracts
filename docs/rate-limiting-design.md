# Rate Limiting Design for Remitwise Contracts

**Document Status:** Design Proposal  
**Audit Finding:** Rate limiting recommended for high-volume scenarios to reduce spam, DoS, and resource exhaustion risk  
**Last Updated:** February 2026

---

## 1. Executive Summary

This document provides a comprehensive design for adding rate limiting to the Remitwise contract suite without requiring immediate code changes. The design specifies which operations need limits, proposed numeric thresholds, storage strategies, and implementation patterns for future implementers to follow.

---

## 2. Scope: Operations to Rate Limit

Rate limiting should apply to **state-mutating operations** (write operations) rather than read-only queries. The rationale is that writes consume storage, trigger events, and can be repeatedly called to exhaust gas budgets or degrade performance.

### 2.1 By Contract

#### **Remittance Split Contract**

**State-mutating operations:**

- `initialize_split(spending%, savings%, bills%, insurance%)` – Sets owner's split config once
  - **Scope:** Per-owner, single initialization (natural limit via contract design)
  - **Limit Recommendation:** Already limited (idempotent after first call); no explicit rate limit needed
  - **Rationale:** Owner can only initialize once; re-initialization attempts fail with `AlreadyInitialized` error

#### **Bill Payments Contract**

**State-mutating operations:**

- `create_bill(owner, name, amount, due_date, recurring, frequency_days)` – Creates a new bill
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 100 bills per owner per 24-hour rolling window
  - **Rationale:** Prevents spam; reasonable for typical household (e.g., utilities, rent, insurance, subscriptions)
- `pay_bill(bill_id)` – Marks bill as paid
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 200 pay operations per owner per 24-hour rolling window
  - **Rationale:** Must allow bulk payments without strict limiting; set higher than creates
- `cancel_bill(bill_id)` – Cancels a bill
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 50 cancel operations per owner per 24-hour rolling window
  - **Rationale:** Less frequent than creates/payments; prevents mass deletions

#### **Savings Goals Contract**

**State-mutating operations:**

- `create_goal(owner, name, target_amount, target_date)` – Creates a new savings goal
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 50 goals per owner per 24-hour rolling window
  - **Rationale:** Goals are long-lived; 50 is reasonable for diversified savings (one per week)
- `add_funds(goal_id, amount)` – Adds funds to a goal
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 500 additions per owner per 24-hour rolling window
  - **Rationale:** Should allow frequent micro-deposits (e.g., daily savings)
- `withdraw_funds(goal_id, amount)` – Withdraws from a goal
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 100 withdrawals per owner per 24-hour rolling window
  - **Rationale:** Withdrawals are less frequent; prevent rapid cycling
- `lock_goal(goal_id, unlock_date)` – Locks a goal
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 20 lock operations per owner per 24-hour rolling window
  - **Rationale:** Administrative action; infrequent
- `unlock_goal(goal_id)` – Unlocks a goal
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 20 unlock operations per owner per 24-hour rolling window
  - **Rationale:** Administrative action; infrequent

#### **Insurance Contract**

**State-mutating operations:**

- `create_policy(owner, name, coverage_type, monthly_premium, coverage_amount)` – Creates a new policy
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 50 policies per owner per 24-hour rolling window
  - **Rationale:** Similar to savings goals; 50 policies is comprehensive coverage
- `pay_premium(policy_id)` – Marks premium as paid
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 200 payments per owner per 24-hour rolling window
  - **Rationale:** Can handle bulk premium payments
- `deactivate_policy(policy_id)` – Deactivates a policy
  - **Scope:** Per-owner per 24h period
  - **Limit Recommendation:** 50 deactivations per owner per 24-hour rolling window
  - **Rationale:** Should match or slightly exceed policy creation limits

#### **Reporting Contract**

**State-mutating operations:**

- `save_report(address, report_data)` – Saves a financial health report
  - **Scope:** Per-address per 1-hour period (to prevent report spam)
  - **Limit Recommendation:** 10 reports per address per hour; 50 per address per 24h
  - **Rationale:** Reports should be infrequent snapshots; prevent excessive generation

#### **Read-Only Functions (Excluded from Rate Limiting)**

The following **should NOT** be rate-limited as they are read-only and do not consume storage:

- `get_split()`, `calculate_split()` – Remittance Split
- `get_unpaid_bills()`, `get_bill()`, `get_overdue_bills()` – Bill Payments
- `get_goal()`, `get_goals_by_owner()` – Savings Goals
- `get_policy()`, `get_policies_by_owner()` – Insurance
- `get_report()`, `get_health_score()` – Reporting

**Rationale:** Read operations are free in most blockchain contexts and do not cause resource exhaustion.

---

## 3. Limits and Rationale

### 3.1 Numeric Limits Summary

| Contract             | Operation         | Limit                   | Window   | Rationale                                          |
| -------------------- | ----------------- | ----------------------- | -------- | -------------------------------------------------- |
| **Bill Payments**    | create_bill       | 100                     | 24h      | Prevent bill spam; ~3-4 bills per day is typical   |
|                      | pay_bill          | 200                     | 24h      | Allow bulk payments; fewer constraints             |
|                      | cancel_bill       | 50                      | 24h      | Prevent mass deletions                             |
| **Savings Goals**    | create_goal       | 50                      | 24h      | Long-term savings vehicle; 1 goal per week typical |
|                      | add_funds         | 500                     | 24h      | Frequent micro-deposits allowed                    |
|                      | withdraw_funds    | 100                     | 24h      | Less frequent withdrawals                          |
|                      | lock_goal         | 20                      | 24h      | Administrative; infrequent                         |
|                      | unlock_goal       | 20                      | 24h      | Administrative; infrequent                         |
| **Insurance**        | create_policy     | 50                      | 24h      | Comprehensive coverage; similar to goals           |
|                      | pay_premium       | 200                     | 24h      | Bulk payments supported                            |
|                      | deactivate_policy | 50                      | 24h      | Matches or exceeds creation limits                 |
| **Reporting**        | save_report       | 10 per hour; 50 per 24h | 1h + 24h | Snapshot-style reporting                           |
| **Remittance Split** | initialize_split  | None                    | N/A      | Natural single-initialization limit                |

### 3.2 Rationale for Limits

1. **Gas Consumption:** Each write operation consumes gas. High-frequency operations (e.g., 1000+ creates per user per day) would quickly accumulate gas costs and may hit per-block or per-transaction limits on Stellar.

2. **Storage Growth:** Maps and vectors grow with each new entry. Unlimited creates could bloat contract state, increase archival costs, and slow queries.

3. **Event Emission:** Each operation emits an event for audit trails. Excessive events could fill ledger history and increase network load.

4. **Spam/DoS Prevention:** Malicious actors could exploit high limits to spam peer-to-peer networks or waste validator resources.

5. **UX Reasonableness:** Proposed limits align with realistic user behavior:
   - Users won't create 100 bills in a day
   - Users won't add funds 500 times per day, but might via automated schedules
   - Insurance policies are set once and managed infrequently

---

## 4. Granularity

### 4.1 Per-Address Limits (Recommended)

**Definition:** Each Stellar address (contract caller) is tracked independently.

**Advantages:**

- Isolates malicious users to their own quota
- Prevents one actor from exhausting resources for others
- Aligns with smart contract access control patterns

**Storage Key Format:** `(address, operation, window_id)`

Example:

```
address = Alice
operation = create_bill
window_id = 2025-02-25 (for 24h window)
count = 45  // Alice has created 45 bills today
```

### 4.2 Per-Contract-Instance Limits (Alternative)

**Definition:** All addresses share a single global limit per operation.

**Advantages:**

- Simpler implementation (no address tracking)
- Prevents network-wide spam

**Disadvantages:**

- Single malicious user can prevent legitimate users from operating
- Less flexible; not recommended for multi-user contracts

**Recommendation:** Use per-address limits as primary; consider global limits as secondary safeguard.

### 4.3 Sliding Window vs. Fixed Day

#### Fixed Day (Recommended for MVP)

**Definition:** 24-hour windows aligned to UTC calendar (00:00-23:59).

**Advantages:**

- Simple: reset at fixed times
- Deterministic: `window_id = timestamp / 86400` (no iteration or cleanup logic)
- Gas-efficient: minimal storage lookups
- No historical timestamp storage
- Easier to reason about and audit

**Disadvantages:**

- "Reset cliff": users can exhaust limit right before midnight UTC, then get fresh quota immediately
- Slightly less fair to users in different timezones

**Cost on Soroban:**

- O(1) lookup and update per operation
- No vector storage of timestamps
- Single storage key per (address, operation, day)

#### Sliding Window (Future Enhancement)

**Definition:** Maintain a rolling 24-hour window. At any given moment, count operations in the past 24 hours.

**Advantages:**

- Fairer: enforces exact time period
- Prevents reset cliff abuse

**Disadvantages:**

- Requires storing timestamps per action
- More computation (iterate/expire entries)
- More storage reads/writes
- Possibly vector storage
- Higher gas costs
- More complex logic to audit

**Cost on Soroban:**

- O(n) cleanup on each operation, where n = number of tracked timestamps
- Additional storage overhead

**Recommendation:**

- **MVP (Phase 2):** Use fixed day for simplicity and efficiency
- **Future (Phase 3+):** Implement sliding window if reset-cliff abuse becomes observed problem

Maintainers and auditors prefer simplicity. Reserve sliding window for future optimization if needed.

---

## 5. Implementation Approach

### 5.1 Storage Layout

Rate limit state is stored in contract instance storage using Soroban's `Map` type.

#### Data Structure

```rust
#[contracttype]
#[derive(Clone)]
pub struct RateLimitRecord {
    pub address: Address,
    pub operation: Symbol,      // e.g., symbol_short!("crt_bill")
    pub window_id: u32,         // Unix timestamp (rounded to window boundary)
    pub count: u32,             // Current count in this window
    pub last_update: u64,       // Ledger timestamp of last increment
}
```

#### Storage Key

```
"rate_limit" + (address, operation, window_id) -> RateLimitRecord
```

#### Example Entries

```
Key: ("rate_limit", "Alice", "crt_bill", 1740372000) -> count=45, last_update=1740455123
Key: ("rate_limit", "Bob", "add_funds", 1740372000) -> count=312, last_update=1740456000
Key: ("rate_limit", "Charlie", "pay_prem", 1740285600) -> count=8, last_update=1740340000
```

### 5.2 Incrementing and Checking

**Pseudocode:**

```rust
fn check_rate_limit(env: &Env, caller: &Address, operation: Symbol, limit: u32) -> Result<(), RateLimitError> {
    let now = env.ledger().timestamp();
    let window_id = (now / 86400) * 86400;  // Round to 24h boundary (sliding: can vary per call)

    let key = format!("rate_limit_{:?}_{:?}_{}", caller, operation, window_id);
    let mut record: RateLimitRecord = env.storage()
        .instance()
        .get(&key)
        .unwrap_or(RateLimitRecord {
            address: caller.clone(),
            operation,
            window_id,
            count: 0,
            last_update: now,
        });

    if record.count >= limit {
        return Err(RateLimitError::RateLimitExceeded);
    }

    record.count += 1;
    record.last_update = now;
    env.storage().instance().set(&key, &record);

    Ok(())
}
```

### 5.3 Bumping TTL (Storage Expiration)

Soroban's ledger state automatically expires if not accessed within a threshold. Rate-limit records must have their TTL bumped to persist across the rate-limit window.

**Strategy:**

1. **Bump on Each Increment:** When `check_rate_limit()` succeeds, also bump the storage entry.

2. **Bump Amount:** Set to exceed the rate-limit window + buffer, but keep it minimal.

   ```
   If window = 24 hours (86400 seconds)
   Bump amount = 3 days (259200 seconds)
   Threshold = 1 day (86400 seconds)

   This ensures entries live at least 3 days to cover current window + 1-2 day buffer.
   Entries expire naturally after 3 days if not accessed.
   ```

3. **Rationale:** Only need current window + 1-2 days buffer. No reason to retain 90 days of rate-limit records.
   - Current window: 0-24 hours
   - Buffer: 1-2 days for edge cases (clock skew, late aggregation)
   - Total: 2-3 days sufficient

4. **Bump Code Pattern (Soroban):**

   ```rust
   env.storage().instance().bump(ledger_threshold, ledger_bump);
   ```

5. **Storage Accumulation:** With 3-day TTL, rate-limit records automatically clean up without manual intervention. Audit-friendly.

### 5.4 Resetting and Decaying

**Window Rollover (Automatic):**

- No explicit reset needed; use a new `window_id` for each 24-hour period
- Old window entries expire naturally via TTL

**Manual Cleanup (Optional):**

- Can implement a maintenance function to purge old entries: `cleanup_expired_limits(cutoff_timestamp)`
- Called periodically to free storage

**Decay Strategy:**

- Not recommended; prefer hard reset per window
- If gradual decay desired, implement as post-increment: `count = max(0, count - 1)` on each operation (adds cost)

### 5.5 Time Source: Ledger Timestamp vs. Separate Clock

**Use Ledger Timestamp (Recommended):**

```rust
let now = env.ledger().timestamp();  // Soroban API
```

**Advantages:**

- Canonical time source; all ledger-aware contracts agree
- Cannot be manipulated by contract; determined by consensus
- No separate contract-managed clock needed

**Disadvantages:**

- Ledger timestamp may be set in future (e.g., after network maintenance)
- Cannot query exact current time if invoked in future block

**Avoid Separate Clock:**

- Requires initialization and trusted updates
- Introduces bugs and centralization risk
- Unnecessary when ledger timestamp available

**Recommendation:** Always use `env.ledger().timestamp()`.

---

## 6. Whitelist: Elevated Limits Instead of Full Exemption

### 6.1 Risk of Full Exemption

**Problem:** Fully exempting admin address creates a blast radius if the admin key is compromised.

```
Normal user: 100 bills/day limit
Compromised admin key: unlimited spam potential
```

**Audit Concern:** Reviewers will question unlimited admin access for a security control.

### 6.2 Elevated Limits (Recommended)

Instead of full exemption, grant elevated limits to trusted addresses:

```rust
#[contracttype]
#[derive(Clone)]
pub enum AddressRole {
    NormalUser,         // 1x limits
    Admin,              // 5x limits
    Treasury,           // 10x limits (for bulk operations)
}
```

**Limits by Role:**

| Operation     | Normal User | Admin (5x) | Treasury (10x) |
| ------------- | ----------- | ---------- | -------------- |
| create_bill   | 100/day     | 500/day    | 1000/day       |
| pay_bill      | 200/day     | 1000/day   | 2000/day       |
| add_funds     | 500/day     | 2500/day   | 5000/day       |
| create_policy | 50/day      | 250/day    | 500/day        |

**Advantages:**

- Reduced blast radius: compromised admin is constrained, not unlimited
- Flexibility: different roles have different limits
- Audit-friendly: limits still enforced, just elevated
- Transparent: easy to verify and monitor

### 6.3 Storage Layout

```rust
#[contracttype]
#[derive(Clone)]
pub struct RateLimitConfig {
    pub admin: Address,
    pub treasury: Address,
    pub role_multipliers: Map<Address, u32>,  // Custom multipliers for accounts
}

#[contracttype]
#[derive(Clone)]
pub struct RateLimitRecord {
    pub address: Address,
    pub operation: Symbol,
    pub window_id: u32,
    pub count: u32,
    pub last_update: u64,
    pub role_multiplier: u32,  // 1, 5, 10, etc.
}
```

### 6.4 Whitelist Check

**Before rate-limit check, determine multiplier:**

```rust
fn get_multiplier(caller: &Address, config: &RateLimitConfig) -> u32 {
    if caller == &config.admin {
        return 5;  // Admin gets 5x limits
    }
    if caller == &config.treasury {
        return 10;  // Treasury gets 10x limits
    }
    if let Some(custom_multiplier) = config.role_multipliers.get(caller) {
        return custom_multiplier;
    }
    1  // Normal user: no multiplier
}

fn check_rate_limit(
    env: &Env,
    caller: &Address,
    operation: Symbol,
    base_limit: u32,
    config: &RateLimitConfig,
) -> Result<(), RateLimitError> {
    let multiplier = get_multiplier(caller, config);
    let effective_limit = base_limit * multiplier;

    // Now check against effective_limit
    ...
}
```

### 6.5 Future Granularity (Per-Operation Limits)

For future enhancements, support per-address-operation overrides:

```rust
pub struct RateLimitConfig {
    // ... existing fields ...
    pub operation_overrides: Map<(Address, Symbol), u32>,  // Custom limit for specific address + operation
}

// Check priority:
// 1. Per-address-operation override
// 2. Role multiplier
// 3. Default limit
```

This provides flexibility without sacrificing simplicity in MVP.

---

## 7. User Impact and Error Handling

### 7.1 Error Types

```rust
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RateLimitError {
    RateLimitExceeded = 101,              // Caller exceeded rate limit
    RateLimitWindowNotYetReset = 102,     // (Informational)
}
```

### 7.2 Returning Limit Information

Enhance error or add separate query function to inform clients:

**Option A: Enhanced Error with Details**

```rust
pub struct RateLimitExceededError {
    pub operation: Symbol,
    pub current_count: u32,
    pub limit: u32,
    pub window_reset_timestamp: u64,  // When limit resets (UTC seconds)
    pub retry_after_seconds: u64,     // Estimated wait time
}
```

**Option B: Separate Query Function**

```rust
pub fn get_rate_limit_status(
    env: &Env,
    caller: &Address,
    operation: Symbol,
) -> RateLimitStatus {
    RateLimitStatus {
        current_count: u32,
        limit: u32,
        window_reset_timestamp: u64,
        time_until_reset: u64,
    }
}
```

**Recommendation:** Option B (separate query) is cleaner; clients call before attempting operation.

### 7.3 Frontend Handling

#### Retry-After Strategy

1. **Query Rate Limit Status Before Operation**

   ```typescript
   const status = await contract.getRateLimitStatus(userAddress, "create_bill");
   if (status.current_count >= status.limit) {
     showError(`Rate limit exceeded. Try again in ${status.time_until_reset}s`);
     return;
   }
   ```

2. **On Error: Exponential Backoff**

   ```typescript
   let delay = 1000;  // 1 second
   while (attempts < MAX_ATTEMPTS) {
       try {
           await contract.create_bill(...);
           break;
       } catch (err) {
           if (err.code === RateLimitError.RateLimitExceeded) {
               await sleep(delay);
               delay *= 2;  // Exponential backoff
           }
       }
   }
   ```

3. **User Communication**
   - Clear message: "You've created 100 bills today. Try again after 11:30 PM UTC."
   - Suggest alternative: "Switch to batch operations" or "Spread requests over multiple days."

#### Circuit Breaker Pattern (Advanced)

If frontend detects repeated rate-limit hits:

```typescript
const circuit = new RateLimitCircuitBreaker(threshold = 5);
circuit.execute(async () => {
    await contract.create_bill(...);
});
// After 5 failures, circuit opens; returns fast without attempting for N seconds
```

### 7.4 Clear Error Messages (Contract Level)

When rate-limited, return detailed error:

```rust
// In contract code
if record.count >= limit {
    return Err(RateLimitError::RateLimitExceeded);
    // Alternative with more detail:
    // events::emit_rate_limit_hit(&env, caller, operation, record.count, limit);
}
```

Emit event with details:

```rust
pub fn emit_rate_limit_hit(
    env: &Env,
    caller: &Address,
    operation: Symbol,
    current_count: u32,
    limit: u32,
) {
    let window_reset = (env.ledger().timestamp() / 86400 + 1) * 86400;
    env.events().publish((
        symbol_short!("rate_limit"),
        "exceeded",
    ), RateLimitEvent {
        caller: caller.clone(),
        operation,
        current_count,
        limit,
        window_reset,
    });
}
```

### 7.5 Graceful Degradation and Failure Modes

**Fail Closed (Recommended):**

If rate-limit storage fails unexpectedly or is unavailable, **revert the operation** with explicit error.

```rust
match env.storage().instance().get(&rate_limit_key) {
    Ok(record) => {
        // Check and increment
    }
    Err(_) => {
        // Storage error: fail closed for safety
        return Err(RateLimitError::StorageAccessError);
    }
}
```

**Rationale:**

- Rate limiting is a security control preventing DoS/spam
- Fail open = bypass protection = security failure
- Fail closed = safe; operation reverts rather than bypassing rate limit
- Auditors strongly prefer fail-closed for security controls

**Exception Cases (Documented):**

- If contract is in emergency maintenance mode (separate flag), operations may be temporarily paused
- Should be time-locked and logged with events
- Never silently bypass rate limit

**Not Recommended:**

- Graceful degradation that bypasses rate limit
- Silent fallback to allowing operation if rate limit unavailable
- These are security anti-patterns

---

## 8. Implementation Roadmap

### Phase 1: Design & Validation (Current)

- [x] Document rate-limit scope, limits, granularity
- [x] Specify storage layout and time windows
- [x] Define whitelist and exemption strategy
- [x] Plan user-facing error handling

### Phase 2: Helper Library (Future)

Create a reusable Soroban library in `rate_limiter/src/lib.rs`:

```rust
pub fn check_and_increment(
    env: &Env,
    caller: &Address,
    operation: Symbol,
    limit: u32,
    config: &RateLimitConfig,
) -> Result<(), RateLimitError>;

pub fn get_status(
    env: &Env,
    caller: &Address,
    operation: Symbol,
) -> RateLimitStatus;
```

### Phase 3: Integration (Future)

Integrate rate-limit checks into each contract:

1. Bill Payments
2. Savings Goals
3. Insurance
4. Reporting

### Phase 4: Testing & Hardening (Future)

- Unit tests for rate-limit logic
- Integration tests for whitelist behavior
- Fuzz tests for edge cases (clock skew, rapid calls, etc.)

---

## 9. Configuration and Management

### 9.1 Hardcoded Limits (MVP Recommendation)

For initial implementation, **limits should be hardcoded constants**, not configurable.

**Advantages:**

- Simplicity: no governance mechanism needed
- Auditability: limits are fixed, transparent, in code
- Gas efficiency: no storage lookups for limit values
- No misconfiguration risk: admin cannot remove limits accidentally
- Faster deployment: no need for upgrade governance

**Example (Rust):**

```rust
const CREATE_BILL_LIMIT: u32 = 100;          // per address per 24h
const PAY_BILL_LIMIT: u32 = 200;
const CREATE_GOAL_LIMIT: u32 = 50;
const ADD_FUNDS_LIMIT: u32 = 500;
```

**When to hardcode:** Phase 2 (initial implementation)

### 9.2 Configurable Limits (Future Enhancement)

After operational experience, a **Phase 3+ upgrade** can introduce configurable limits if needed.

**Rationale for deferring:**

- Requires governance mechanism (who can change? timelocks?)
- Additional storage overhead
- Auditors will ask: can admin remove limits entirely? (risky)
- Simpler to start fixed, adjust in future version if needed

**If/when implemented:**

```rust
#[contracttype]
#[derive(Clone)]
pub struct OperationLimitConfig {
    pub operation: Symbol,
    pub base_limit: u32,
    pub window_seconds: u64,
    pub last_updated: u64,
    pub updated_by: Address,
}

pub fn update_limit(
    env: &Env,
    admin: &Address,
    operation: Symbol,
    new_limit: u32,
) -> Result<(), Error> {
    // Verify admin
    // Validate new_limit > 0
    // Time-lock before effect?
    // Emit event
    // Update storage
}
```

**Governance Questions for Future (if configurable):**

- Who can change limits? (admin-only, multisig, DAO vote)
- Is there a timelock before change takes effect?
- What are bounds? (e.g., limits between 10 and 1000)
- Are changes logged and auditable?
- Can limits be set to 0 or removed entirely?

**Recommendation:** Keep MVP simple with hardcoded constants. Upgrade to configurable in Phase 3 only if operational data suggests limits need tuning.

**Recommended Events:**

1. Rate limit exceeded:

   ```rust
   (symbol_short!("rate_limit"), "exceeded")
   ```

2. Rate limit near threshold (80%):

   ```rust
   (symbol_short!("rate_limit"), "warning")
   ```

3. Configuration updated:
   ```rust
   (symbol_short!("rate_limit"), "config_updated")
   ```

Frontends and off-chain monitors can subscribe to these events to trigger alerts.

---

## 10. Edge Cases and Mitigations

### 10.1 Clock Skew

**Issue:** Ledger timestamp jumps backward (rare but possible after network outages).

**Mitigation:**

- Treat timestamp as monotonically increasing in production
- If decreased, reset window counters (conservative: assume new window)
- Log alert for operational team

### 10.2 Rapid Successive Calls

**Issue:** Caller invokes operation 1000 times in same block.

**Mitigation:**

- Each call increments counter and checks against limit
- Limit is enforced per-call, not per-block
- 1001st call fails; others succeed
- Caller pays for all gas, even failed calls

### 10.3 Window Boundary Conditions

**Issue:** Caller makes calls right at window boundary (e.g., 23:59:59 UTC).

**Mitigation (Sliding Window):**

- Window ID calculated as `(now / 86400) * 86400`
- Calls at 23:59:59 and 00:00:01 use different windows
- Caller gets fresh quota at boundary
- Expected and acceptable behavior

### 10.4 Storage Exhaustion

**Issue:** Rate-limit entries bloat contract storage over time.

**Mitigation:**

- TTL set to 90 days; old entries auto-expire
- Optional: periodic cleanup function removes entries older than threshold
- Monitor storage growth; alert if exceeding budget

### 10.5 Whitelist Bypass

**Issue:** Attacker compromises admin address.

**Mitigation:**

- Whitelist is as strong as admin key security
- Follow key management best practices (multisig, hardware wallet)
- Consider time-locked pause function for emergency whitelisting

---

## 11. Testing Strategy (For Future Implementers)

### Unit Tests

```rust
#[test]
fn test_rate_limit_enforced() {
    // Call operation at limit; verify next call fails
}

#[test]
fn test_different_addresses_independent() {
    // Alice at limit should not affect Bob
}

#[test]
fn test_admin_exempt() {
    // Admin can exceed limit
}

#[test]
fn test_window_reset() {
    // Advance time; verify counter resets
}
```

### Integration Tests

```rust
#[test]
fn test_rate_limit_with_bill_creation() {
    // Create bills until limit; verify error on 101st
}

#[test]
fn test_rate_limit_query_accuracy() {
    // Verify get_rate_limit_status() matches actual limits
}
```

### Fuzz Tests

```rust
proptest! {
    #[test]
    fn prop_rate_limit_never_exceeds_threshold(
        calls in 0..500usize,
        addrs in prop::collection::vec(any::<Address>(), 0..50),
    ) {
        // Verify limit never exceeded for any address
    }
}
```

---

## 12. References and Related Decisions

- **ADR: Admin Role** (`docs/adr-admin-role.md`) – Specifies admin and event emission patterns
- **Architecture Overview** (`ARCHITECTURE.md`) – Describes contract interactions
- **Audit Finding:** "Rate limiting recommended for high-volume scenarios"
- **Soroban Storage TTL:** https://developers.stellar.org/learn/smart-contracts/storage

---

## 13. Open Questions for Team Review

1. **Question:** Should reporting contract be rate-limited at all, or only per-address admin operations?
   - **Current Proposal:** Yes, limit save_report to 10/hour per address.
   - **Alternative:** Reporting is read-heavy; relax or remove limits.

2. **Question:** Are the role-based multipliers (5x admin, 10x treasury) appropriate?
   - **Current Proposal:** Elevated limits (5x admin, 10x treasury) instead of full exemption.
   - **Alternative:** Different multipliers based on operational needs (3x vs. 20x).

3. **Question:** Should we support per-operation multiplier overrides in MVP?
   - **Current Proposal:** Not in MVP; hardcoded role multipliers only.
   - **Alternative:** Include per-operation-per-address overrides from day one.

4. **Question:** Is 2–3 day TTL appropriate for rate-limit records?
   - **Current Proposal:** 2–3 day TTL (current window + buffer).
   - **Alternative:** Longer retention (7 days) for operational analytics.

5. **Question:** Should limits be tunable in the future?
   - **Current Proposal:** Hardcoded in MVP; consider configurable limits in Phase 3 if needed.
   - **Alternative:** Include configurable limits from Phase 2 with governance controls.

---

## 14. Conclusion

This design provides a solid foundation for adding rate limiting to Remitwise contracts without requiring immediate code changes. By documenting scope, limits, granularity, and implementation patterns, future implementers can follow a clear roadmap and avoid re-deciding policy in code.

**Next Steps:**

1. Team review and feedback on this design
2. Address open questions (Section 13)
3. Create rate-limit helper library in `rate_limiter/` module
4. Integrate into each contract as resources allow
