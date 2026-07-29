# Kill-Switch Recovery Runbook

**Audience:** Contract operators and on-call engineers.

This document covers the complete lifecycle of a kill-switch event: how to engage
the kill switch, what happens while it is active, and how to safely resume
operations after the incident is resolved.

---

## Overview

RemitWise contracts have two complementary write-freeze mechanisms:

| Mechanism | Type | How it clears |
|---|---|---|
| **Kill switch** (`KILL_SW`) | Binary toggle — stays active until cleared | Explicit `deactivate_kill_switch` call |
| **Investigation epoch** (`INV_EPOCH`) | Time-bounded — has an end timestamp | Expires automatically; can be cleared early |

Both mechanisms are implemented in `remitwise-common/src/lib.rs` and are called
at the top of every write entry point across all contracts.  When either one is
active, write entry points return a typed error (`KillSwitchError::WriteBlocked`
or `InvestigationEpochError::WriteBlocked`) without executing any mutations.

Read-only entry points are **not** affected — queries, balance checks, and audit
log reads continue to work during a freeze.

---

## 1. Engaging the Kill Switch

### When to use it

Activate the binary kill switch when you need an **immediate, indefinite freeze**
— for example:

* A confirmed exploit or vulnerability that requires triage before any further
  mutations are allowed.
* A regulatory or legal hold instruction.
* An emergency maintenance window where all write paths must be closed.

If you need to freeze writes for a **bounded investigation window** (hours or
days) and want automatic expiry, use the investigation epoch instead (§ 4).

### How to activate

Call `activate_kill_switch` on the relevant contract.  This sets a `bool` flag
in instance storage (`KILL_SW = true`).  No arguments are required.  Every
write entry point that calls `require_no_active_kill_switch` will immediately
start returning `KillSwitchError::WriteBlocked`.

```bash
# Example: activate via soroban CLI
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network testnet \
  -- activate_kill_switch
```

> **Auth note:** `activate_kill_switch` and `deactivate_kill_switch` in
> `remitwise-common` do **not** enforce their own authentication — the calling
> contract's entry point is responsible for gating with `admin.require_auth()`.
> Never expose these helpers via an unauthenticated public entry point.

### What is frozen

Once the kill switch is active, every entry point that calls
`require_no_active_kill_switch` will reject mutations.  As of the current
implementation this covers **all write entry points** across:

* `bill_payments`
* `insurance`
* `remittance_split`
* `family_wallet`
* `savings_goals`
* `orchestrator`
* `reporting`

Read-only entry points (`get_*`, `is_*`, `list_*`) continue to work.

---

## 2. Verifying the Kill Switch is Active

```bash
# Check via soroban CLI
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <ANY_KEY> \
  --network testnet \
  -- is_kill_switch_active
# Returns: true
```

Attempt any write to confirm it is blocked:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <USER_KEY> \
  --network testnet \
  -- pay_bill --caller <ADDR> --bill_id 1 --amount 1000
# Error: KillSwitchError::WriteBlocked (contract error code 1)
```

---

## 3. Clearing the Kill Switch (Recovery)

When the incident is resolved:

1. **Confirm the fix is deployed** (patched WASM uploaded and instance storage
   migrated if needed).
2. **Obtain admin authorization** for the deactivation call.
3. **Call `deactivate_kill_switch`.**

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network testnet \
  -- deactivate_kill_switch
```

This removes the `KILL_SW` flag from instance storage.  From this point forward,
`require_no_active_kill_switch` returns `Ok(())` and write operations resume.

4. **Verify recovery:**

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <ANY_KEY> \
  --network testnet \
  -- is_kill_switch_active
# Returns: false
```

5. **Perform a smoke-test write** (e.g., a test bill payment on testnet) to
   confirm the entry point is unblocked end-to-end.

### Idempotency guarantee

`deactivate_kill_switch` is a safe no-op when the kill switch is already
inactive.  Calling it twice or calling it on a never-activated contract will not
cause any errors or unexpected side effects.

---

## 4. Investigation Epoch (Time-Bounded Alternative)

The investigation epoch (`INV_EPOCH`) is a time-bounded write freeze.  It is
appropriate when you know in advance how long the freeze should last and want it
to expire automatically without a manual deactivation step.

### Starting an investigation epoch

```bash
# Freeze writes for 4 hours (14400 seconds)
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network testnet \
  -- start_investigation_epoch --duration_secs 14400
```

The contract stores `epoch_end = ledger_timestamp + duration_secs`.  Every write
entry point that calls `require_no_investigation_epoch` will return
`InvestigationEpochError::WriteBlocked` until the ledger timestamp reaches
`epoch_end`.

### Automatic expiry

No action is needed to clear the epoch after `epoch_end` — the next write attempt
will succeed automatically.

### Early clearance

If the investigation resolves before the epoch expires:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network testnet \
  -- clear_investigation_epoch
```

This removes the `INV_EPOCH` key from instance storage.  `clear_investigation_epoch`
is also a safe no-op when called on a contract with no active epoch.

---

## 5. Kill-Switch Epoch Guard (emergency_killswitch)

The `emergency_killswitch` contract uses a separate **kill-switch epoch** counter
(`DataKey::KillSwitchEpoch`) to prevent replay of stale `transfer_admin`
authorizations.  Every call to `transfer_admin` must supply the current epoch;
if the epoch does not match exactly, the call is rejected with
`Error::EpochMismatch`.

### When to bump the epoch

Bump the epoch whenever you need to **invalidate all prior authorizations** —
for example after a signing key rotation, after a suspected leak of an admin
authorization payload, or as a precautionary measure after an incident.

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network testnet \
  -- bump_kill_switch_epoch --caller <ADMIN_ADDR>
# Returns: new_epoch (u64)
```

After the bump, any `transfer_admin` call that was authorized at the previous
epoch will be rejected.  Authorized callers must re-acquire a fresh authorization
that references the new epoch value.

### Reading the current epoch

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <ANY_KEY> \
  --network testnet \
  -- get_kill_switch_epoch
# Returns: current epoch (u64, starts at 0 after initialize)
```

---

## 6. Checklist

Use this checklist for every kill-switch event:

```
Engagement
[ ] Confirmed the incident type warrants a kill switch
[ ] Admin key secured and available
[ ] Kill switch activated (or investigation epoch started) on all affected contracts
[ ] Verified is_kill_switch_active == true (or epoch active) on each contract

During freeze
[ ] Read-only paths confirmed still operational
[ ] Incident investigation in progress
[ ] Patch reviewed and approved
[ ] Patched WASM built and verified (cargo build --target wasm32-unknown-unknown --release)
[ ] Patched WASM uploaded to chain

Recovery
[ ] Deactivation call executed on all contracts (or epoch cleared early)
[ ] is_kill_switch_active == false confirmed on each contract
[ ] Smoke-test write executed and succeeded on each contract
[ ] Kill-switch epoch bumped (if signing key rotation was involved)
[ ] Incident post-mortem drafted
```

---

## 7. Related Documentation

* [Killswitch Trust Model](killswitch-trust-model.md) — who can trigger, who
  can clear, what state is preserved
* [Pause Playbook](PAUSE_PLAYBOOK.md) — granular pause controls (module/function
  level) managed by the `emergency_killswitch` contract
* [Epoch Model](EPOCH_MODEL.md) — how epoch counters bump, what they invalidate,
  and the stale-authorization replay threat
* [Cross-Contract Invariants](CROSS_CONTRACT_INVARIANTS.md) — kill switch as a
  cross-contract invariant in the reviewer checklist
* [Migration Flags](MIGRATION_FLAGS.md) — investigation-epoch write freezes
  during data migration

---

## 8. Emergency Contacts

This document intentionally does not list specific engineer contacts (they
change frequently).  Consult your team's on-call rotation and escalation policy
for the current contact list.
