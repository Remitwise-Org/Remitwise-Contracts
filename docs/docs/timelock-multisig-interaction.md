# Timelock + Multisig Interaction

> **Audience:** Contributors reviewing or extending the `family_wallet` and
> `emergency_killswitch` contracts.
> This document shows the canonical sequence of `propose → sign → execute` and
> `pause → schedule_unpause → unpause`, the auth required at every step, and
> how the two mechanisms interact.

---

## 1. Overview

RemitWise uses two distinct safety mechanisms that work together:

| Mechanism | Contract | Purpose |
|-----------|----------|---------|
| **Multisig** | `family_wallet` | Requires *k-of-n* signer approval before a high-value action executes |
| **Timelock** | `emergency_killswitch` | Enforces a mandatory cooling-off delay before the system can be unpaused after an incident |

A paused system blocks multisig proposals and execution. The timelock therefore
acts as a gate that prevents the multisig flow from resuming until operators
have explicitly verified the incident is resolved.

---

## 2. Multisig Flow — `propose → sign → execute`

### 2.1 Transaction types

```rust
pub enum TransactionType {
    LargeWithdrawal      = 1,  // amount > multisig spending_limit
    SplitConfigChange    = 2,
    RoleChange           = 3,
    EmergencyTransfer    = 4,
    PolicyCancellation   = 5,
    RegularWithdrawal    = 6,  // immediate path, no multisig
}
```

`RegularWithdrawal` bypasses multisig entirely. All other types go through the
three-step flow below.

### 2.2 Step 1 — Propose

**Entrypoint:** `propose_transaction(env, caller, tx_type, data)`
(or the `withdraw` wrapper which resolves the tier automatically)

**Auth required:** `caller.require_auth()` — caller must be a family member
with at least the `Member` role and a non-expired role expiry.

**What happens:**

1. Contract checks the wallet is not paused.
2. Resolves `WithdrawalTier` for `withdraw` calls; `LargeWithdrawal` config is
   read from storage to compare `amount` against `spending_limit`.
3. A `PendingTransaction` is created and stored under `PEND_TXS`:

```rust
pub struct PendingTransaction {
    pub tx_id:      u64,
    pub tx_type:    TransactionType,
    pub proposer:   Address,
    pub signatures: Vec<Address>,   // proposer counted as first signer
    pub created_at: u64,
    pub expires_at: u64,            // created_at + PROP_EXP (default 86 400 s)
    pub data:       TransactionData,
}
```

4. The proposer's address is recorded in `signatures` — their implicit approval
   is **not** double-counted if they later call `sign_transaction`.

**Concrete example — large withdrawal:**

```bash
soroban contract invoke \
  --id $FAMILY_WALLET_CONTRACT \
  --source $PROPOSER_SECRET \
  -- withdraw \
  --caller "$PROPOSER_ADDR" \
  --recipient "$RECIPIENT_ADDR" \
  --token "$TOKEN_CONTRACT" \
  --amount 5000000000   # 500 XLM (stroops); exceeds spending_limit → LargeWithdrawal
```

Returns `tx_id` (e.g. `42`).

---

### 2.3 Step 2 — Sign

**Entrypoint:** `sign_transaction(env, signer, tx_id)`

**Auth required:** `signer.require_auth()`

**What happens:**

1. Proposal is fetched; panics if not found.
2. Checks `ledger.timestamp() <= expires_at` — expired proposals return
   `TransactionExpired`.
3. Checks `signer` is in the configured `signers` vector for the `tx_type`.
   Non-listed signers return `SignerNotMember`.
4. **Deduplication:** if `signer` is already in `signatures`, call is a no-op
   (`Ok(false)`). A single signer cannot inflate the approval count.
5. `signer` is appended to `signatures`.
6. If `signatures.len() >= threshold`, execution is triggered automatically
   (see §2.4).

**Concrete example:**

```bash
soroban contract invoke \
  --id $FAMILY_WALLET_CONTRACT \
  --source $SIGNER_SECRET \
  -- sign_transaction \
  --signer "$SIGNER_ADDR" \
  --tx_id 42
```

Returns `Ok(true)` on a fresh signature, `Ok(false)` if already signed.

---

### 2.4 Step 3 — Execute (auto-triggered at threshold)

Execution is triggered inside `sign_transaction` when the approval count reaches
`threshold`. There is no separate public `execute` entrypoint — the last
required signer's `sign_transaction` call atomically applies the action.

**Auth required:** Same as Step 2 (the final signer's `require_auth()`).

**What happens on execution:**

| `tx_type` | Action applied |
|-----------|---------------|
| `LargeWithdrawal` | Token transfer from proposer → recipient via Soroban token contract |
| `SplitConfigChange` | Updates remittance split percentages |
| `RoleChange` | Updates a member's `FamilyRole` |
| `EmergencyTransfer` | Transfers under emergency guardrails |
| `PolicyCancellation` | Cancels an insurance policy |

The `PendingTransaction` is removed from `PEND_TXS` after execution.

### 2.5 Auth summary table

| Step | Caller constraint | Role gate | Blocked when paused? |
|------|------------------|-----------|---------------------|
| `propose_transaction` | Any family member | `Member` or higher | ✅ Yes |
| `sign_transaction` | Must be in configured `signers` | `Member` or higher | ✅ Yes |
| Execute (auto) | Triggered by final signer | — | ✅ Yes (paused check is in propose; execution cannot be reached if proposal was blocked) |
| `configure_multisig` | Owner or Admin | `Admin` or higher | ✅ Yes |
| `cancel_transaction` | Proposer, Owner, or Admin | `Member` or higher | — |
| `cleanup_expired_pending` | Owner or Admin | `Admin` or higher | ✅ Yes |

---

## 3. Timelock Flow — `pause → schedule_unpause → unpause`

Managed by the `emergency_killswitch` contract.

### 3.1 Step 1 — Pause (immediate)

**Entrypoint:** `pause(env, caller)`

**Auth required:** `caller` must be the configured pause admin (default: owner).

Sets `DataKey::GlobalPaused = true` immediately. Any pending unpause schedule
stored under `DataKey::UnpauseSchedule` is **cancelled** at this point — a
re-pause during an incident cannot be bypassed by a stale schedule.

```bash
soroban contract invoke \
  --id $KILLSWITCH_CONTRACT \
  --source $PAUSE_ADMIN_SECRET \
  -- pause \
  --caller "$PAUSE_ADMIN_ADDR"
```

### 3.2 Step 2 — Schedule Unpause

**Entrypoint:** `schedule_unpause(env, caller, time)`

**Auth required:** Admin or Owner.

`time` must satisfy `time > env.ledger().timestamp()`. Past-dated schedules are
rejected with `Error::InvalidSchedule`. The value is written to
`DataKey::UnpauseSchedule`.

```bash
# Schedule unpause 6 hours from now (current ledger timestamp + 21600 seconds)
soroban contract invoke \
  --id $KILLSWITCH_CONTRACT \
  --source $ADMIN_SECRET \
  -- schedule_unpause \
  --caller "$ADMIN_ADDR" \
  --time $(($(date +%s) + 21600))
```

### 3.3 Step 3 — Unpause

**Entrypoint:** `unpause(env, caller)`

**Auth required:** Admin or Owner.

Enforces `env.ledger().timestamp() >= scheduled_time`. Calling before the
window expires returns `Error::Unauthorized`. Calling with no active schedule
returns `Error::InvalidSchedule`.

```bash
soroban contract invoke \
  --id $KILLSWITCH_CONTRACT \
  --source $ADMIN_SECRET \
  -- unpause \
  --caller "$ADMIN_ADDR"
```

### 3.4 Timelock state machine
### 3.5 Auth summary table

| Step | Caller constraint | Effect on existing schedule |
|------|------------------|-----------------------------|
| `pause` | Pause admin | Cancels any pending schedule |
| `schedule_unpause` | Admin or Owner | Sets future unpause timestamp |
| `unpause` | Admin or Owner | Only valid after `ledger.timestamp() >= scheduled_time` |

---

## 4. How the Two Mechanisms Interact
**Key invariants:**

- A `pause()` call at any point resets the schedule — even if called while a
  `schedule_unpause` is pending.
- Expired multisig proposals (`expires_at` exceeded during a pause) cannot be
  executed after unpause; they must be reproposed.
- `is_paused()` returns `true` for the entire duration of the cooling-off
  window, so downstream systems observing the event stream see a clean
  Active → Paused → Active transition.

---

## 5. Error Reference

### family_wallet errors (multisig)

| Error | Code | When raised |
|-------|------|-------------|
| `Unauthorized` | 1 | Caller role insufficient or role expired |
| `TransactionExpired` | — | `ledger.timestamp() > expires_at` at sign time |
| `SignerNotMember` | — | Signer not in configured `signers` for `tx_type` |
| `InvalidTransactionType` | 8 | Unknown `tx_type` passed to `configure_multisig` |

### emergency_killswitch errors (timelock)

| Error | Code | When raised |
|-------|------|-------------|
| `Unauthorized` | 1 | Caller not pause admin; or `unpause()` before timelock expires |
| `InvalidSchedule` | 5 | `schedule_unpause` with past timestamp; `unpause()` with no schedule |

---

## 6. Running the Tests

```bash
# Multisig unit tests
cargo test -p family_wallet

# Timelock unit tests
cargo test -p emergency_killswitch

# Full workspace lint (must pass before pushing)
cargo clippy --workspace --all-targets -- -D warnings

# WASM build verification
cargo build --target wasm32-unknown-unknown --release
```

---

## 7. Related Documents

- [`docs/multisig-proposal-expiry.md`](multisig-proposal-expiry.md) — proposal
  expiry duration configuration and cleanup
- [`docs/killswitch-timelock.md`](killswitch-timelock.md) — detailed timelock
  invariants and test coverage
- [`docs/family-wallet-design.md`](family-wallet-design.md) — full permissions
  matrix and role model
- [`docs/fw-signature-dedup.md`](fw-signature-dedup.md) — signature
  deduplication guarantees
- [`docs/fw-pause-matrix.md`](fw-pause-matrix.md) — layered pause scope
  precedence
