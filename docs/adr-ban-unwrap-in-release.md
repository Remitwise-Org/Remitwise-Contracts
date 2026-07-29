# ADR: Ban unwrap (and equivalent panicking methods) in release builds

- Status: Accepted
- Date: 2026-07-24
- Related issue: #1093
- Contracts in scope: All Soroban contracts in workspace

## Context

RemitWise Contracts is a Soroban smart contract suite running on the Stellar blockchain. Soroban contracts execute in a sandboxed WASM environment where:

1. Contracts are compiled with `#![no_std]` — no standard library, no OS primitives
2. The release profile uses `panic = "abort"` — when a panic occurs, the transaction aborts immediately with no unwinding
3. A runtime panic in any contract function causes the entire transaction to abort, state is rolled back, and the error is propagated to the caller

Unlike traditional Rust applications where a panic might crash a single thread or process, a panic in a Soroban contract aborts the transaction and consumes the user's gas budget without completing the intended operation.

## Decision

**All production code** (non-test) in RemitWise Contracts **bans** the following panicking methods:

- `unwrap()`
- `expect()`
- Direct `panic!()` macro invocations
- `unreachable!()` in paths that are not proven unreachable by the type system

Test code (guarded with `#[cfg(test)]`) is explicitly exempted from this ban.

### Enforcement

The ban is enforced at two layers:

1. **Compile-time:** Every contract crate includes the following directive at the crate root:
   ```rust
   #![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
   ```

2. **CI validation:** The CI pipeline runs:
   ```bash
   cargo clippy --workspace --lib -- -D clippy::unwrap_used -D clippy::expect_used
   ```
   This step (labeled "SC-054" in `check_ci.sh`) fails the build if any production code uses `unwrap()` or `expect()`.

### Required Pattern

All functions that can fail must return `Result<T, ContractError>` and use the `?` operator for explicit error propagation:

```rust
use soroban_sdk::{contracterror, contractimpl, Env, Address, Map};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BillPaymentsError {
    BillNotFound = 1,
    Unauthorized = 2,
    InvalidAmount = 3,
}

#[contractimpl]
impl BillPayments {
    pub fn get_bill(env: Env, bill_id: u32) -> Result<Bill, BillPaymentsError> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&BILLS)
            .ok_or(BillPaymentsError::BillNotFound)?;
        
        bills
            .get(bill_id)
            .ok_or(BillPaymentsError::BillNotFound)
    }
}
```

For values that cannot fail, use `unwrap_or_else`, `unwrap_or`, or `unwrap_or_default`:

```rust
let bills: Map<u32, Bill> = env
    .storage()
    .instance()
    .get(&BILLS)
    .unwrap_or_else(|| Map::new(&env));
```

## Consequences

### Positive

1. **Error paths are explicit and visible in code review:** Every path that can fail surfaces as a `Result` type in the function signature, making it impossible to miss error cases during review.

2. **No silent transaction aborts:** A developer cannot accidentally introduce a code path that aborts the transaction due to an unexpected `None` or `Err` in a place they assumed was safe. The compiler forces them to handle the error case or explicitly choose a default value.

3. **Upstream callers can react to errors:** When a contract function returns `Result<T, ContractError>` instead of panicking, the caller (whether another contract or an off-chain client) receives a typed error code and can decide how to handle it — retry, show a user-friendly error message, or execute a fallback path.

### Negative

1. **Slightly more verbose code:** Explicit error handling with `?` and `ok_or()` is more verbose than `.unwrap()`. Functions must declare `Result` return types even when the developer "knows" the operation cannot fail.

2. **Learning curve for new contributors:** Contributors unfamiliar with Soroban must learn the `Result`-based error handling patterns and understand why `unwrap()` is forbidden before making changes. This is addressed by the [Contributor Overview](CONTRIBUTOR_OVERVIEW.md) and this ADR.

### Counter-examples — what NOT to do

#### BAD: Using `unwrap()` on storage access

```rust
// ❌ BAD — do not do this
pub fn pay_bill(env: Env, bill_id: u32) {
    let mut bills: Map<u32, Bill> = env
        .storage()
        .instance()
        .get(&BILLS)
        .unwrap(); // PANICS if BILLS key is absent
    
    let mut bill = bills.get(bill_id).unwrap(); // PANICS if bill_id not found
    bill.paid = true;
    bills.set(bill_id, bill);
    // ... rest of function never executes if either unwrap panics;
    // transaction aborts; user pays gas for nothing; no error code returned
}
```

**What happens at runtime:**
- If the `BILLS` storage key is absent (e.g., contract just initialized, no bills created yet), the first `unwrap()` panics.
- If `bill_id` does not exist in the map, the second `unwrap()` panics.
- The transaction aborts immediately — no state changes are committed, the user's gas is consumed, and the caller receives a generic panic error instead of a typed error like `BillNotFound`.

#### GOOD: Explicit error handling

```rust
// ✅ GOOD — explicit error handling
pub fn pay_bill(env: Env, bill_id: u32) -> Result<(), BillPaymentsError> {
    let mut bills: Map<u32, Bill> = env
        .storage()
        .instance()
        .get(&BILLS)
        .ok_or(BillPaymentsError::BillNotFound)?;
    
    let mut bill = bills
        .get(bill_id)
        .ok_or(BillPaymentsError::BillNotFound)?;
    
    bill.paid = true;
    bills.set(bill_id, bill);
    env.storage().instance().set(&BILLS, &bills);
    
    Ok(())
}
```

**What happens at runtime:**
- If the `BILLS` key is absent, the function returns `Err(BillPaymentsError::BillNotFound)`.
- If `bill_id` does not exist, the function returns `Err(BillPaymentsError::BillNotFound)`.
- The caller receives a typed error code and can handle it appropriately (show "Bill not found" to the user, log the error, retry with a different ID, etc.).
- No gas is wasted on a transaction that was doomed to fail.

#### BAD: Using `expect()` with a descriptive message

```rust
// ❌ STILL BAD — expect also panics
pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
    from.require_auth();
    
    let balance: i128 = env
        .storage()
        .instance()
        .get(&from)
        .expect("balance must exist for authenticated user"); // PANICS
    
    // ... transaction aborts; descriptive message is lost in the panic
}
```

**Why this is still bad:**
- `expect()` is just `unwrap()` with a custom panic message.
- The descriptive message is helpful during local development, but in production the transaction still aborts and the caller sees a generic panic error, not the descriptive message.
- The Clippy lint `clippy::expect_used` is enforced for the same reason as `clippy::unwrap_used`.

#### GOOD: Default values for non-critical paths

```rust
// ✅ GOOD — default value when absence is not an error
pub fn get_total_bills(env: Env, owner: Address) -> u32 {
    env
        .storage()
        .instance()
        .get(&owner)
        .unwrap_or(0u32) // No panic; returns 0 if key absent
}
```

**When to use `unwrap_or` / `unwrap_or_else` / `unwrap_or_default`:**
- When the absence of a value is not an error condition, but a normal state (e.g., counters that start at 0, collections that start empty).
- These methods do not panic — they provide a fallback value when the `Option` is `None` or `Result` is `Err`.

## Alternatives Considered

### Allow `unwrap()` with safety comments

**Rejected.** Comments are not enforced by the compiler or CI. Over time, as the codebase evolves:
- Comments become stale and inaccurate
- New contributors may not read or understand the safety comment
- A refactor might invalidate the safety assumption without updating the comment
- Code review cannot mechanically verify that a safety comment is correct

The invariant "this unwrap is safe" is better expressed by restructuring the code to use types that make failure impossible (e.g., pass a `T` instead of `Option<T>` when the caller has already validated the value exists).

### Use `expect()` with descriptive messages

**Rejected.** `expect()` panics just like `unwrap()`. The descriptive message is useful during local testing, but does not help the caller in production:
- The transaction still aborts and state is rolled back
- The Soroban panic error does not surface the `expect()` message to the caller
- Typed errors (`Result<T, ContractError>`) are strictly more useful: they allow the caller to match on specific error variants and react accordingly

### Allow panics in "truly unreachable" code paths

**Rejected** for production code, with one exception:
- If a code path is provably unreachable due to type system invariants, the compiler will catch it — no `unreachable!()` is needed.
- If the type system does not prove the path is unreachable, the path *might* be reachable due to future changes, external input, or developer error.
- **Exception:** `unreachable!()` is allowed in test code and in exhaustive `match` arms where all enum variants are explicitly covered and the compiler confirms exhaustiveness.

## Implementation Guidance

### For New Contributors

1. Read the [Contributor Overview](CONTRIBUTOR_OVERVIEW.md) for general development standards.
2. Every contract function that accesses storage, parses input, or performs computation that can fail **must** return `Result<T, ContractError>`.
3. Use `?` to propagate errors up the call stack.
4. Use `unwrap_or_else`, `unwrap_or`, or `unwrap_or_default` when a missing value is not an error, but a default case.
5. Run `cargo clippy --workspace --lib -- -D clippy::unwrap_used -D clippy::expect_used` before opening a PR.

### For Code Reviewers

1. Reject any PR that introduces `unwrap()` or `expect()` in production code (non-test).
2. Challenge uses of `unwrap_or` / `unwrap_or_else` if the absence of the value *should* be an error (e.g., a bill that must exist for payment).
3. Ensure error types are specific and actionable (e.g., `BillNotFound`, not a generic `Error`).

### For Auditors and Integrators

- All error codes are documented in [ARCHITECTURE.md](../ARCHITECTURE.md#standardized-error-codes-issue-336).
- Every contract error type is a `#[contracterror]` enum with sequential `u32` discriminants starting at 1.
- Callers can match on error codes to implement retry logic, user-facing error messages, or fallback paths.
