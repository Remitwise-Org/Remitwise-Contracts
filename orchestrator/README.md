# Orchestrator Contract

A Soroban smart contract that coordinates cross-contract remittance flows across
family wallet, remittance split, savings goals, bill payments, and insurance.

## Features

- Dependency address configuration with duplicate/self-reference validation (`init`)
- Reentrancy protection via an execution lock (`EXEC_LOCK`, RAII `LockGuard`)
- Caller-scoped nonce replay protection, hardened with deadline windows and
  request-hash binding (`execute_remittance_flow_signed`)
- Actor-epoch invalidation for stale signed tokens (`bump_actor_epoch`)
- Compensation/rollback support for failed multi-step flows
- Unsigned, signed, and best-effort "fan-out" flow execution
- Bounded audit logging (ring buffer) and execution statistics tracking
- Pre-upgrade snapshot / restore / discard for safe contract upgrades
- Reentrancy-guarded reward claiming (checks-effects-interactions pattern)

## Quickstart

```rust
use orchestrator::{OrchestratorClient, RemittanceFlowParams};

// One-time setup: register the five downstream dependency addresses.
// Fails with `DuplicateDependency` if any address repeats or equals `caller`.
client.init(&owner, &family_wallet_addr, &remittance_split_addr, &savings_addr, &bills_addr, &insurance_addr);

let params = RemittanceFlowParams {
    caller: owner.clone(),
    total_amount: 10_000_0000000, // 10 USDC
    family_wallet: family_wallet_addr,
    remittance_split: remittance_split_addr,
    savings: savings_addr,
    bills: bills_addr,
    insurance: insurance_addr,
    goal_id: 1,
    bill_id: 1,
    policy_id: 1,
};

client.execute_remittance_flow(&params);
```

## API Reference

### Setup

#### `init(env, caller, family_wallet, remittance_split, savings_goals, bill_payments, insurance)`

One-time initialization. `caller` becomes the permanent owner. Rejects duplicate
addresses and self-references among the five dependencies. Returns `Unauthorized`
if already initialized.

### Flow Execution

#### `execute_remittance_flow(env, params)`

Executes the full remittance flow across all contracts in a single call, with
explicit dependency addresses passed in `params`. Protected by the reentrancy
lock. On downstream failure, previously-applied steps are compensated
(best-effort) and `RemittanceFlowRolledBack` is returned.

#### `execute_remittance_flow_signed(env, executor, amount, nonce, deadline, request_hash, actor_epoch)`

Executes the flow using dependency addresses and execution parameter IDs
(`goal_id`/`bill_id`/`policy_id`) resolved from instance storage (set at
`init`). Adds replay protection on top of `execute_remittance_flow`:

- **Nonce**: must equal the caller's current sequential counter and must not
  already be in the used-nonce set (bounded to the last `MAX_USED_NONCES_PER_ADDR`
  nonces per address).
- **Deadline**: must be in the future and within `MAX_DEADLINE_WINDOW_SECS`
  (1 hour) of the current ledger time.
- **Request hash**: must match a hash computed over
  `(nonce, amount, deadline, goal_id, bill_id, policy_id)`, preventing a
  relayer from redirecting funds to a different goal/bill/policy after signing.
- **Actor epoch**: must match the contract's current epoch (see
  `bump_actor_epoch`), invalidating stale pre-signed tokens.

#### `execute_flow_fanout(env, executor, amount)`

Splits `amount` three ways and attempts the savings/bill/insurance calls
independently via `try_*`. Unlike the two entrypoints above, **there is no
compensation on partial failure** — callers receive a `FanOutFlowResult` with
per-step outcomes and decide how to handle partial success themselves.

### Rewards

#### `claim_rewards_summary_external(env, caller, reward_token)`

Claims and transfers the caller's pending reward balance from a SEP-41-style
`reward_token` contract. Reentrancy-guarded: the pending balance is zeroed
*before* the external `transfer` call (checks-effects-interactions), so a
malicious token contract re-entering this function cannot double-claim.

#### `get_pending_rewards(env, address) -> i128`

Read-only lookup of an address's pending reward balance.

### Governance / Maintenance

#### `bump_actor_epoch(env, caller) -> u64`

Owner-only. Increments the actor epoch, invalidating all previously-issued
signed-flow tokens. Use if a signing key is suspected compromised.

#### `get_actor_epoch_public(env) -> u64`

Read-only current actor epoch, for constructing new signed-flow tokens.

#### `get_version(env) -> u32` / `set_version(env, caller, new_version)`

Owner-only version bump; emits `(orch, upgraded)`.

#### `pre_upgrade(env, caller)` / `restore_from_snapshot(env, caller)` / `discard_snapshot(env, caller)`

Owner-only. Snapshot critical instance storage (owner, dependency addresses,
execution-lock state, stats, parameter IDs, actor epoch) before an upgrade, and
restore or discard it afterward. `restore_from_snapshot` enforces a freshness
window (`SnapshotTooOld`) and a schema-version check.

### Queries

#### `get_execution_stats(env) -> Option<ExecutionStats>`

Returns aggregate execution counters and the number of audit entries evicted
by the ring-buffer cap.

#### `get_execution_state(env) -> bool`

Returns whether the reentrancy lock (`EXEC_LOCK`) is currently held.

#### `get_audit_log(env, from_index, limit) -> Vec<AuditEntry>`

Returns a page of the bounded audit ring-buffer (capped at
`MAX_AUDIT_ENTRIES` = 100). `limit` is clamped to `[1, MAX_AUDIT_ENTRIES]`
(0 → default 20). `from_index` is a position in the current rotated window,
not a stable global ID — an out-of-range cursor returns an empty page.

#### `get_nonce(env, address) -> u64`

Returns the current sequential nonce for an address.

#### `get_fee_schedule(env) -> Option<(u32, u32, u32, u32)>`

Read-only cross-contract call into Remittance Split to surface the current
split percentages (spending, savings, bills, insurance).

## Security Model

| Property | Mechanism |
|---|---|
| Reentrancy | `EXEC_LOCK` acquired via RAII `LockGuard` before any downstream call; released on all return paths including early errors |
| Replay protection | Per-address sequential nonce + bounded used-nonce set, checked before the sequential-counter check |
| Parameter tampering | `request_hash` binds nonce/amount/deadline/goal_id/bill_id/policy_id together |
| Stale signed tokens | `actor_epoch` must match the contract's current epoch; owner can bump the epoch to invalidate all outstanding tokens |
| Dependency misconfiguration | `init` rejects duplicate addresses and self-references among the five dependencies |
| Reward double-claim | Pending balance zeroed before the external token transfer (checks-effects-interactions) |

## Running Tests

```bash
cargo test -p orchestrator -- --nocapture
```

Test coverage spans unit tests in `src/test.rs` and `src/lib.rs`
(`tests_nonce_eviction`), event-schema assertions in `src/events_schema_test.rs`,
and integration-style guards in `tests/` (cross-contract epoch handling,
investigation/dispute epoch guards, gas benchmarks).
