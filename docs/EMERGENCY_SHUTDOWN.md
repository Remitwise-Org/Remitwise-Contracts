# Emergency Shutdown — Repo-Wide Overview

> **Audience:** Operators responding to a live incident (and reviewers auditing incident-response readiness).
> **Goal:** Say, precisely, what an operator can actually press today to stop a contract from accepting writes — and be explicit about what does *not* work yet, so nobody discovers that mid-incident.

This repo has **three separate mechanisms** that all use "pause" / "kill switch" language. They are not the same thing, and activating one does not activate the others. Confusing them during an incident means acting on the wrong one and getting no effect.

| # | Mechanism | Where it lives | Scope | Usable today? |
|---|-----------|-----------------|-------|----------------|
| 1 | Per-contract `pause` / `unpause` | Each contract's own `lib.rs` | One contract at a time | ✅ Yes — this is the real emergency brake |
| 2 | `remitwise-common` kill switch (`KillSwitchError`, `require_no_active_kill_switch`) | `remitwise-common/src/lib.rs`, wired into write entrypoints of 3 contracts | Per-contract (own instance storage) | ❌ No public entrypoint calls `activate_kill_switch` — see below |
| 3 | Standalone `emergency_killswitch` contract | `emergency_killswitch/` (its own deployable contract) | Itself only | ⚠️ Fully functional, but not cross-called by any other contract |

## Mechanism 1: Per-contract `pause` / `unpause` — use this

This is what actually stops a contract from accepting writes. It's implemented independently per contract (no shared code), gated by a contract-specific "pause admin" (set via `set_pause_admin`, separate from the main admin), and requires `caller.require_auth()`.

Worked example — `bill_payments` (see `bill_payments/src/lib.rs`):

```rust
// Halt every state-mutating entrypoint in bill_payments immediately:
client.emergency_pause_all(&pause_admin);

// ...incident resolved, and no scheduled timelock is blocking it...
client.unpause(&pause_admin);
```

`bill_payments` exposes the fullest version of this pattern:
- `pause` / `unpause` — global pause for the whole contract
- `pause_function` / `unpause_function` — pause one operation (e.g. `PAY_BILL`) without stopping the rest
- `emergency_pause_all` — global pause *plus* every function-level flag, in one call (despite the name, this is scoped to `bill_payments` only — it is not a system-wide switch)
- `schedule_unpause` — time-locks the earliest `unpause` can succeed, so a compromised pause-admin key can't be used to immediately undo a pause
- `get_pause_state` / `is_paused` / `get_paused_since` — read-only status checks

Full precedence rules and error codes for this contract are already documented in [docs/bill-payments-pause-hierarchy.md](./bill-payments-pause-hierarchy.md) — read that before touching `bill_payments`'s pause admin in production.

### Coverage across contracts

This mechanism is **not implemented uniformly**. Verified directly against each contract's `lib.rs`:

| Contract | Has `pause`/`unpause`? | Notes |
|---|---|---|
| `bill_payments` | ✅ | Full hierarchy — see above |
| `savings_goals` | ✅ | `pause`, `unpause`, `pause_function`, `unpause_function` |
| `family_wallet` | ✅ | `pause`, `unpause` |
| `remittance_split` | ✅ | `pause`, `unpause` |
| `insurance` | ❌ | Only a vestigial `set_pause_admin` — there is no `pause`/`unpause` entrypoint to actually gate |
| `orchestrator` | ❌ | No pause mechanism at all |
| `reporting` | ❌ | No pause mechanism at all |

If you need to halt `insurance`, `orchestrator`, or `reporting` during an incident, there is currently **no on-chain lever** — see "What to do if a contract has no pause mechanism" below.

## Mechanism 2: `remitwise-common` kill switch — wired in, but currently unreachable

`remitwise-common/src/lib.rs` defines a second, independent binary flag per contract:

- `is_kill_switch_active(env)` / `require_no_active_kill_switch(env)` — the read/guard side
- `activate_kill_switch(env)` / `deactivate_kill_switch(env)` — the write side, explicitly documented as *not* enforcing auth: "it is the caller's responsibility to gate it with admin auth"

`require_no_active_kill_switch` **is** called at the top of write entrypoints in `bill_payments`, `savings_goals`, and `family_wallet` (verified via direct search of each `lib.rs`) — including, in `bill_payments`, at the top of `pause`/`unpause` themselves, so an active kill switch overrides even the pause admin.

The gap: **no contract in this workspace exposes a public entrypoint that calls `activate_kill_switch` or `deactivate_kill_switch`.** Searching the whole workspace (excluding `remitwise-common`'s own tests) for callers of either function returns nothing outside `remitwise-common/src/lib.rs` itself. The guard is real and does get checked on every write — but as of this writing there is no way to actually flip it on in a deployed contract. Treat mechanism 1 (per-contract `pause`) as the one that works; do not assume the kill switch is a usable incident-response lever until a contract adds an admin-gated entrypoint that calls `activate_kill_switch`.

## Mechanism 3: the standalone `emergency_killswitch` contract

`emergency_killswitch/` is a complete, independently deployable Soroban contract with global, per-module, and per-function pause plus scheduled unpause — see [emergency_killswitch/README.md](../emergency_killswitch/README.md) for its API and [emergency_killswitch/RUNBOOK.md](../emergency_killswitch/RUNBOOK.md) for its admin-transfer procedure.

It is **not called by any other contract in this workspace** (verified: no `bill_payments`, `insurance`, `remittance_split`, `family_wallet`, `savings_goals`, `orchestrator`, or `reporting` source file references it). Pausing it has no effect on any other contract's ability to accept writes. If your incident-response plan assumes calling `emergency_killswitch.pause()` halts the system, it does not — today it only halts whatever off-chain caller chooses to check its state first (e.g. a client or indexer that consults it before submitting a transaction, if one is built to do so).

## What to do if a contract has no pause mechanism

For `insurance`, `orchestrator`, and `reporting`, there is currently no on-chain way to stop writes short of a contract upgrade (see the relevant contract's `pre_upgrade` / upgrade-admin flow, where implemented) or halting things at the RPC/infrastructure layer. Adding a `pause`/`unpause` pair to these contracts, mirroring `savings_goals`'s implementation, is a reasonable follow-up — it is out of scope here since this document only describes current behavior.

## Cross-references

- [docs/bill-payments-pause-hierarchy.md](./bill-payments-pause-hierarchy.md) — full precedence rules and error codes for `bill_payments`'s pause hierarchy specifically.
- [emergency_killswitch/README.md](../emergency_killswitch/README.md) — API reference for the standalone contract.
- [emergency_killswitch/RUNBOOK.md](../emergency_killswitch/RUNBOOK.md) — admin-transfer procedure for the standalone contract.
- [ACCESS_CONTROL_MATRIX.md](../ACCESS_CONTROL_MATRIX.md) — which roles can call which administrative entrypoints.
