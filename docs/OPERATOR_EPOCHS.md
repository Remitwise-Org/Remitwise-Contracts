# Operator Guide: Cross-Contract Epoch Consistency

> Companion to [`CROSS_CONTRACT_EPOCHS.md`](./CROSS_CONTRACT_EPOCHS.md). This document
> is for **operators / deployers** and explains how to configure, advance, and
> reconcile the cross-contract epoch that Issue #1720 introduced.

## Why epochs exist across contracts

The orchestrator drives a single remittance "fan-out" by calling several
downstream contracts in one transaction:

- `family_wallet` — `check_spending_limit`
- `remittance_split` — `calculate_split`, `get_split`
- `savings_goals` — `add_to_goal`, `remove_from_goal` (compensation)
- `bill_payments` — `pay_bill`, `reverse_payment` (compensation)
- `insurance` — `pay_premium`, `reverse_premium` (compensation)

Each of these **privileged** entry points now requires two extra arguments on
every call:

1. `orchestrator: Address` — the expected caller contract identity.
2. `epoch: u64` — the current cross-contract epoch.

The downstream contract rejects the call (with `CrossContractEpochError::EpochMismatch`)
unless **both** hold:

- `orchestrator` equals the address stored as its **trusted orchestrator**, and
- `epoch` equals the downstream's own stored `XC_EPOCH`.

This gives two guarantees:

- **Identity binding:** only the configured orchestrator may drive privileged
  cross-contract operations.
- **Replay / drift protection:** a captured call from an old epoch cannot be
  replayed after the epoch advances, and a mis-configured downstream that is out
  of sync cannot silently accept stale calls.

## Deployment: configuring the trusted orchestrator

Before any fan-out will succeed, each downstream contract must be told the
orchestrator's address. This is a **one-time, privileged** operation.

| Contract         | Who may call `set_trusted_orchestrator`                |
| ---------------- | ----------------------------------------------------- |
| `family_wallet`  | The wallet **owner**                                  |
| `bill_payments`  | The bill-payments **admin**                           |
| `insurance`      | The insurance **owner**                               |
| `remittance_split` | The split contract **owner** (existing `init` owner) |
| `savings_goals`  | **Bootstrap:** first call must have `caller == orchestrator`; afterwards only the stored orchestrator may change it |

Example (pseudo):

```rust
// owner / admin of the downstream contract:
downstream_client.set_trusted_orchestrator(&owner, &orchestrator_address);
```

`savings_goals` has no owner concept, so it bootstraps: the very first
`set_trusted_orchestrator` call is accepted only when `caller == orchestrator`.
That first call is normally made by the orchestrator itself during deployment
wiring. After that, only the stored orchestrator may update it.

## Advancing epochs: the coordinated bump

Epochs are advanced by the orchestrator's owner via
`bump_actor_epoch`. This is **coordinated and atomic**:

1. The orchestrator increments its own `ACTOR_EPOCH`.
2. It then calls `bump_cross_contract_epoch` on **every** configured downstream
   contract (read from the stored `FW_ADDR` / `RS_ADDR` / `SG_ADDR` / `BP_ADDR`
   / `INS_ADDR` routing addresses).
3. Each downstream advances its own `XC_EPOCH` by 1.

Because every downstream validates that *this* orchestrator is its trusted
orchestrator before advancing, a single mis-configured or missing downstream
aborts the whole `bump_actor_epoch` transaction. There is no partial drift:
either all epochs move together, or none do.

> Operational note: ensure **all** downstream `set_trusted_orchestrator` calls
> succeed *before* the first coordinated `bump_actor_epoch`. Until the first
> successful coordinated bump, the orchestrator's `ACTOR_EPOCH` and each
> downstream `XC_EPOCH` all start at `0`, which is already consistent — so
> fan-outs work immediately after configuring trust, even before any bump.

## Reading the current epoch

- Orchestrator: `get_actor_epoch()` → `u64`.
- Any downstream: `get_cross_contract_epoch()` → `u64`.

These are view methods useful for health checks and off-chain reconciliation.

## Events for reconciliation

Every fan-out publishes an event that carries the epoch used:

```
topic: (symbol_short!("orch"), symbol_short!("flow_ep"))   data: <epoch: u64>
```

The coordinated bump publishes:

```
topic: (symbol_short!("orch"), symbol_short!("epch_bump")) data: (old_epoch, new_epoch)
```

Downstream contracts additionally emit `epch_bump` (with their own new epoch)
and `orch_set` (when the trusted orchestrator is configured). Off-chain
indexers correlate the orchestrator's `flow_ep` with each downstream event
emitted during the same transaction to confirm the whole flow ran at a single,
consistent epoch.

## Troubleshooting

| Symptom                                                  | Cause / fix                                                                 |
| -------------------------------------------------------- | --------------------------------------------------------------------------- |
| Cross-contract call fails with `EpochMismatch`            | Downstream `XC_EPOCH` disagrees with orchestrator `ACTOR_EPOCH`, or the trusted orchestrator is not set. Re-run a coordinated `bump_actor_epoch` after fixing `set_trusted_orchestrator`. |
| `bump_actor_epoch` reverts                               | A downstream is missing or has no trusted orchestrator configured. Verify routing addresses and trust on every downstream. |
| Fan-out works once, then fails after an epoch bump       | A downstream did not receive the bump (e.g. address not registered in routing). Check `get_cross_contract_epoch()` per contract. |
