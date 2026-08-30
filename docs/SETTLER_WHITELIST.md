# Settler Whitelist — Operator Guide

## Overview

A **settler** is an address authorised to perform settlement operations — value transfers
that finalise obligations across the Remitwise contracts. The settler whitelist is the
set of addresses stored in each contract's instance storage that control:

- **Global emergency pause** (`emergency_killswitch`) — one admin
- **Per-contract pause** (`bill_payments`, `remittance_split`, `savings_goals`, `insurance`, `family_wallet`) — one pause admin each
- **Per-contract upgrade** — one upgrade admin each
- **Orchestrator settlement execution** — one owner
- **Remittance Split USDC distribution** — one owner

Every settler is a single address at any point in time. There is no multi-settler list —
rotation replaces the previous settler atomically.

This guide covers how settlers are added during initialisation, rotated to a new address,
and revoked in an emergency. It is written for **operators** who manage these addresses
day to day.

> **Cross-reference:** see [ACCESS_CONTROL_MATRIX.md](../ACCESS_CONTROL_MATRIX.md)
> for the full function-level access matrix, and [docs/adr-admin-role.md](adr-admin-role.md)
> for the architectural decision record that defines the admin role boundaries.

---

## Settler inventory

| Contract | Role | Storage key | Entrypoint(s) | Blast radius |
|---|---|---|---|---|
| `emergency_killswitch` | Admin | `DataKey::Admin` | `initialize`, `transfer_admin` | Global pause — stops all contracts |
| `bill_payments` | Pause admin | `symbol_short!("PAUSE_ADM")` | `set_pause_admin` | Bill create/pay/cancel |
| `bill_payments` | Upgrade admin | `symbol_short!("UPG_ADM")` | `set_upgrade_admin` | Contract version |
| `remittance_split` | Pause admin | `symbol_short!("PAUSE_ADM")` | `set_pause_admin` | Split initialisation/distribution |
| `remittance_split` | Upgrade admin | `symbol_short!("UPG_ADM")` | `set_upgrade_admin` | Contract version |
| `remittance_split` | Owner | `config.owner` | `initialize_split` | USDC distribution, schedule management |
| `savings_goals` | Pause admin | `symbol_short!("PAUSE_ADM")` | `set_pause_admin` | Goal creation/withdrawal |
| `savings_goals` | Upgrade admin | `symbol_short!("UPG_ADM")` | `set_upgrade_admin` | Contract version |
| `insurance` | Pause admin | `symbol_short!("PAUSE_ADM")` | `set_pause_admin` | Policy creation/premium payment |
| `insurance` | Upgrade admin | `symbol_short!("UPG_ADM")` | `set_upgrade_admin` | Contract version |
| `family_wallet` | Pause admin | `symbol_short!("PAUSE_ADM")` | `set_pause_admin` | Withdrawal, multisig config, emergency mode |
| `family_wallet` | Upgrade admin | `symbol_short!("UPG_ADM")` | `set_upgrade_admin` | Contract version |
| `orchestrator` | Owner | `symbol_short!("OWNER")` | `init` | Settlement flow execution, epoch bumps |

---

## 1. Adding a settler

### 1.1 Emergency Killswitch admin

The killswitch admin is set once during initialisation and cannot be added after the fact
without rotating the existing admin.

```rust
// Deploy time: set the initial killswitch admin.
// The contract's own address is rejected to prevent unrecoverable bricking.
let killswitch = EmergencyKillswitchClient::new(&env, &killswitch_contract_id);
killswitch.initialize(&admin_address);
```

**Constraints:**
- `admin_address` must not be the contract's own address.
- `initialize` is one-shot — calling it again returns `AlreadyInitialized`.

### 1.2 Per-contract pause admin

Each contract's pause admin is set post-initialisation by the contract owner.

```rust
// remittance_split — owner sets the initial pause admin
let split = RemittanceSplitClient::new(&env, &split_contract_id);
split.set_pause_admin(&owner, &pause_admin_address);

// bill_payments — same pattern
let bills = BillPaymentsClient::new(&env, &bills_contract_id);
bills.set_pause_admin(&owner, &pause_admin_address);
```

**Constraints — remittance_split:**
- Caller must be `config.owner`.
- Contract must be initialised (`initialize_split` must have completed).
- Contract must not be paused.

**Constraints — bill_payments:**
- First call: any caller can self-nominate (bootstrap pattern).
- Subsequent calls: only the current pause admin can transfer.

**Constraints — savings_goals / insurance:**
- First call: any caller can self-nominate.
- Subsequent calls: only the current pause admin can transfer.

### 1.3 Per-contract upgrade admin

Same pattern as pause admin, but with different transfer rules (see Rotation below).

```rust
let split = RemittanceSplitClient::new(&env, &split_contract_id);
split.set_upgrade_admin(&owner, &upgrade_admin_address);
```

### 1.4 Orchestrator owner

Set once during initialisation. Controls dependency addresses, settlement flow
execution, and actor epoch bumps.

> **Immutable.** The orchestrator owner cannot be rotated after `init`.
> To change the owner, deploy a new orchestrator instance.

```rust
let orch = OrchestratorClient::new(&env, &orchestrator_contract_id);
orch.init(
    &owner,
    &family_wallet_addr,
    &remittance_split_addr,
    &savings_goals_addr,
    &bill_payments_addr,
    &insurance_addr,
);
```

**Constraints:**
- All five dependency addresses must be unique.
- No dependency address may equal the owner address.
- `init` is one-shot.

### 1.5 Remittance Split owner

Set during `initialize_split`. The owner address is pinned in `config.owner` and
is the only address that can call `distribute_usdc`.

> **Immutable.** The split owner cannot be rotated after `initialize_split`.
> To change the owner, deploy a new contract instance and re-initialise.

```rust
let split = RemittanceSplitClient::new(&env, &split_contract_id);
split.initialize_split(&owner, &nonce, &usdc_contract, &50, &30, &15, &5);
```

---

## 2. Rotating a settler

### 2.1 Emergency Killswitch — direct transfer

The killswitch uses a single-step transfer. The current admin calls `transfer_admin`
with the new address. The previous admin loses all authority immediately.

```rust
// Current admin rotates to new_admin
killswitch.transfer_admin(&new_admin);

// new_admin can now pause
killswitch.pause();
assert!(killswitch.is_paused());

// old_admin cannot pause — auth failure at the Soroban host layer
```

**Rejections:**
| Attempt | Result |
|---|---|
| `new_admin == current_admin` | `InvalidAdmin` |
| `new_admin == contract_address` | `InvalidAdmin` |
| Transfer before `initialize` | `NotInitialized` |

**Event emitted:** `(symbol_short!("emergency"), symbol_short!("admn_xfer"))` with
`AdminTransferred { old_admin, new_admin, timestamp }`.

### 2.2 Remittance Split — pause admin

The owner retains transfer authority for the pause admin (unlike upgrade admin,
which can only be transferred by the current upgrade admin).

```rust
// Owner rotates pause admin to new address
split.set_pause_admin(&owner, &new_pause_admin);

// Verify
assert_eq!(split.get_pause_admin_public(), Some(new_pause_admin));
```

### 2.3 Remittance Split — upgrade admin

Once set, only the **current upgrade admin** can transfer. The owner cannot override
after the initial assignment (privilege escalation prevention).

```rust
// Current upgrade admin rotates to new address
split.set_upgrade_admin(&current_upgrade_admin, &new_upgrade_admin);

// Owner attempt is rejected
let result = split.try_set_upgrade_admin(&owner, &attacker_address);
assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));
```

### 2.4 Bill Payments — upgrade admin

Uses a self-nomination bootstrap: the first caller sets themselves as upgrade
admin. Subsequent transfers require the current upgrade admin.

```rust
// First call: self-nominate (bootstrap)
bills.set_upgrade_admin(&admin, &admin);

// Subsequent calls: current upgrade admin transfers
bills.set_upgrade_admin(&current_upgrade_admin, &new_upgrade_admin);
```

### 2.5 Reporting — two-step rotation

The reporting contract uses a **propose → accept** handshake. The current admin
proposes a successor; the proposed address must actively accept.

```rust
// Step 1: admin proposes new_admin
reporting.propose_new_admin(&admin, &new_admin);

// Step 2: new_admin accepts
reporting.accept_admin_rotation(&new_admin);
```

**Security properties:**
- A fat-fingered address cannot be installed in one call.
- The proposed address must prove key control by signing the accept call.
- Re-proposing overwrites any prior pending proposal (latest wins).
- Post-rotation, the old admin loses all privileges atomically.

For the full state machine and negative-path tests, see
[docs/reporting-admin-rotation.md](reporting-admin-rotation.md).

### 2.6 Savings Goals / Insurance — self-service rotation

These contracts use a self-service pattern: the current admin sets the next admin.

```rust
savings.set_upgrade_admin(&current_admin, &new_admin);
```

---

## 3. Revoking a settler

### 3.1 Emergency: centralised pause via killswitch

The killswitch is a standalone contract that provides a centralised emergency
pause. It does **not** programmatically cascade to other contracts — each contract
checks its own local `PAUSED` flag independently. Use the killswitch alongside
per-contract pauses for layered defence.

```rust
// Admin pauses the killswitch
killswitch.pause();

// Any pending unpause schedule is cleared
// To unpause: first schedule_unpause(future_timestamp), then unpause()
let now = env.ledger().timestamp();
killswitch.schedule_unpause(&(now + 86_400)); // 24-hour cooldown
// ... wait for ledger to reach timestamp ...
killswitch.unpause();
```

**Recovery from stuck-paused state:**
If a re-`pause()` clears the unpause schedule, `unpause()` fails with
`InvalidSchedule`. Use `clear_emergency_state()` to immediately clear the
global pause (admin-only, no timelock):

```rust
killswitch.clear_emergency_state();
```

### 3.2 Per-contract pause

Each contract's pause admin can pause that specific contract independently.

```rust
// Pause only the bill_payments contract
bills.pause(&pause_admin);

// Unpause
bills.unpause(&pause_admin);
```

### 3.3 Module-level pause (killswitch)

The killswitch admin can pause individual modules without global impact.

```rust
killswitch.pause_module(&symbol_short!("remittance"));
killswitch.unpause_module(&symbol_short!("remittance"));
```

### 3.4 Function-level pause (killswitch)

Granular: pause specific functions within a module. Capped at 10 paused functions
per module.

```rust
killswitch.pause_function(&symbol_short!("bills"), &symbol_short!("pay_bill"));
killswitch.unpause_function(&symbol_short!("bills"), &symbol_short!("pay_bill"));
```

### 3.5 Permanent revocation — transfer to burn address

To permanently revoke a settler without a replacement, transfer the role to a
provably unusable address (e.g., a contract address with no auth capability, or
the zero address if validated by the contract).

```rust
// Transfer killswitch admin to a dead address (permanent revocation)
// NOTE: this is irreversible. Ensure the dead address can never sign.
killswitch.transfer_admin(&dead_address);
```

**Warning:** All contracts reject transferring to the contract's own address
to prevent unrecoverable bricking. Choose a dead address carefully.

### 3.6 Actor epoch bump (orchestrator)

Invalidates all stale actor tokens without changing the owner address. Useful
when a signing service is compromised but the owner key is not.

```rust
let new_epoch = orch.bump_actor_epoch(&owner);
// All actor tokens created before this call are now invalid.
```

---

## 4. Pause layers

The Remitwise contracts use multiple **independent** pause layers. There is no
single precedence chain across contracts — each layer is a separate control.

**Within the killswitch**, the check order for `is_function_paused` is:

```
global → module → function
```

**Each downstream contract** (bill_payments, remittance_split, etc.) checks
its own local `PAUSED` flag independently. The killswitch does **not**
cascade to other contracts automatically.

| Layer | Set by | Scope |
|---|---|---|
| Killswitch global | Killswitch admin | Killswitch only — does not cascade |
| Killswitch module | Killswitch admin | One module within killswitch |
| Killswitch function | Killswitch admin | One function within a killswitch module |
| Per-contract pause | Contract pause admin | All state-changing ops in one contract |

For complete coverage during an incident, pause **both** the killswitch and
the relevant per-contract pause admin(s).

---

## 5. Operational checklist

### Before adding a settler
- [ ] Address is a multi-sig account, not a single EOA.
- [ ] All signers on the multi-sig have been verified.
- [ ] The address has been tested on a staging/testnet deployment first.

### Before rotating a settler
- [ ] The new address can sign transactions (prove key control).
- [ ] The old address is known and still controlled by the operator.
- [ ] For killswitch: verify `transfer_admin` emits `AdminTransferred` event.
- [ ] For reporting: verify the proposed address calls `accept_admin_rotation`.
- [ ] Post-rotation: verify the old address can no longer perform privileged operations.

### Before revoking a settler
- [ ] Global pause via killswitch is the fastest response to an active incident.
- [ ] Per-contract pause has smaller blast radius — prefer when the issue is isolated.
- [ ] Permanent revocation to a dead address is irreversible — double-check.

### After any settler change
- [ ] Verify the event was emitted on-chain.
- [ ] Update the runbook with the new address.
- [ ] Rotate any monitoring alert configurations that reference the old address.

---

## 6. Monitoring

Every settler change emits an on-chain event. Operators should monitor for:

| Event | Meaning | Urgency |
|---|---|---|
| `admn_xfer` (any contract) | Admin transferred | Low — routine rotation |
| `AdminTransferred` (killswitch) | Killswitch admin changed | High — verify immediately |
| `paused_v2` / `unpaused_v2` | Global pause toggled | Critical — incident response |
| `m_paused_v2` / `m_unpause_v2` | Module pause toggled | Medium |
| `f_paused_v2` / `f_unpause_v2` | Function pause toggled | Low |
| `epch_bump` (orchestrator) | Actor epoch bumped | Medium — verify if expected |
| `snap_pre` / `snap_rst` | Upgrade snapshot taken/restored | Medium — verify before upgrade |

Refer to [docs/EVENT_TAXONOMY.md](EVENT_TAXONOMY.md) for the full event schema.

---

## 7. References

- [ACCESS_CONTROL_MATRIX.md](../ACCESS_CONTROL_MATRIX.md) — per-function access control
- [docs/adr-admin-role.md](adr-admin-role.md) — admin role design decision
- [docs/killswitch-admin-transfer.md](killswitch-admin-transfer.md) — killswitch rotation details
- [docs/reporting-admin-rotation.md](reporting-admin-rotation.md) — two-step rotation state machine
- [docs/killswitch-timelock.md](killswitch-timelock.md) — unpause timelock design
- [docs/bill-payments-pause-hierarchy.md](bill-payments-pause-hierarchy.md) — bill payments pause details
- [docs/fw-pause-matrix.md](fw-pause-matrix.md) — family wallet pause matrix
- [docs/remittance-split-pause-coverage.md](remittance-split-pause-coverage.md) — remittance split pause coverage
- [docs/remittance-split-admin-roles.md](remittance-split-admin-roles.md) — remittance split admin roles

---

*Document written for Remitwise operators managing settler addresses.*
