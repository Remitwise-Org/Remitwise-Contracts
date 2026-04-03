# Remittance Split Contract

A Soroban smart contract for configuring and executing percentage-based USDC distributions
across spending, savings, bills, and insurance categories.

## Security Model

`distribute_usdc` is the only function that moves funds. It enforces the following invariants
in strict order before any token interaction occurs:

1. **Domain-Separated Auth** — `initialize_split` uses a structured `InitializationPayload`
   containing the network ID, contract address, owner, and nonce. This payload must be
   explicitly signed, preventing the authorization from being replayed on different contract
   instances or Stellar networks.
2. **Auth first** — For other operations, `caller.require_auth()` is the very first operation;
   no state is read before the caller proves authority.
3. **Pause guard** — the contract must not be globally paused.
4. **Owner-only** — `from` must equal the address stored as `config.owner` at initialization.
   Any other address is rejected with `Unauthorized`, even if it can self-authorize.
4. **Trusted token** — `usdc_contract` must match the address pinned in `config.usdc_contract`
   at initialization time. Passing a different address returns `UntrustedTokenContract`,
   preventing token-substitution attacks.
5. **Amount validation** — `total_amount` must be > 0.
6. **Self-transfer guard** — none of the four destination accounts may equal `from`.
   Returns `SelfTransferNotAllowed` if any match.
7. **Replay protection** — nonce must equal `get_nonce(from)` and is incremented after success.
8. **Audit + event** — a `DistributionCompleted` event is emitted on success for off-chain indexing.
9. **Schedule ID Sequencing** — `create_remittance_schedule` generates strictly monotonic IDs using a synchronized counter (`NEXT_RSCH`), ensuring no collisions across high-volume operations.
10. **Schedule Execution Guardrails** — Remittance schedules enforce minimum intervals (1 hour), maximum lead times (1 year), and strict owner-only modifications to prevent unsafe state transitions or accidental fund depletion.

## Features

- Percentage-based allocation (spending / savings / bills / insurance, must sum to 100)
- Hardened `distribute_usdc` with 7-layer auth checks
- Nonce-based replay protection on split initialization, split updates, distributions, and snapshot imports
- Global pause that freezes every mutating entrypoint except `unpause`
- Pause / unpause with transferable admin controls
- Remittance schedules (create / modify / cancel)
- Snapshot export/import with checksum verification
- Audit log (last 100 entries, ring-buffer)
- TTL extension on initialization, split updates, snapshot imports, and schedule mutations

## Pause Model

The contract uses a single global `PAUSED` flag as an emergency stop.

While paused, these mutating entrypoints return `Unauthorized` before changing state:

- `set_pause_admin`
- `pause`
- `set_upgrade_admin`
- `set_version`
- `initialize_split`
- `update_split`
- `distribute_usdc`
- `import_snapshot`
- `create_remittance_schedule`
- `modify_remittance_schedule`
- `cancel_remittance_schedule`

`unpause` is intentionally the only mutating entrypoint that remains callable while paused so the
contract can always be recovered by the active pause admin.

Read-only helpers such as `get_config`, `get_split`, `get_split_allocations`,
`calculate_split`, `get_nonce`, `get_remittance_schedule`, and `export_snapshot` remain available
while paused.

Because the contract currently reuses `Unauthorized` for pause rejections, off-chain callers
should check `is_paused()` when distinguishing an auth failure from an emergency stop.

## Quickstart

```rust
// 1. Initialize — pin the trusted USDC contract address at setup time
client.initialize_split(
    &owner,
    &0,           // nonce
    &usdc_addr,   // trusted token contract — immutable after init
    &50,          // spending %
    &30,          // savings %
    &15,          // bills %
    &5,           // insurance %
);

// 2. Distribute
client.distribute_usdc(
    &usdc_addr,   // must match the address stored at init
    &owner,       // must be config.owner and must authorize
    &1,           // nonce (increments after each call)
    &AccountGroup { spending, savings, bills, insurance },
    &1_000_0000000, // stroops
);
```

## API Reference

### Data Structures

#### `SplitConfig`

```rust
pub struct SplitConfig {
    pub owner: Address,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub timestamp: u64,
    pub initialized: bool,
    /// Trusted USDC contract address — pinned at initialization, validated on every distribute_usdc call.
    pub usdc_contract: Address,
}
```

#### `AccountGroup`

```rust
pub struct AccountGroup {
    pub spending: Address,
    pub savings: Address,
    pub bills: Address,
    pub insurance: Address,
}
```

### Functions

#### `initialize_split(env, owner, nonce, usdc_contract, spending_percent, savings_percent, bills_percent, insurance_percent) -> bool`

Initializes the split configuration and pins the trusted USDC token contract address.

- `owner` must authorize.
- `usdc_contract` is stored immutably and validated on every `distribute_usdc` call.
- Percentages must sum to exactly 100.
- Can only be called once (`AlreadyInitialized` on repeat).

#### `distribute_usdc(env, usdc_contract, from, nonce, deadline, request_hash, accounts, total_amount) -> bool`

Distributes USDC from `from` to the four split destination accounts.

**Security checks (in order):**
1. `from.require_auth()`
2. Contract not paused
3. `from == config.owner`
4. `usdc_contract == config.usdc_contract`
5. `total_amount > 0`
6. No destination account equals `from`
7. Hardened replay protection (matches `nonce`, ensures `deadline` is valid, checks `request_hash`, prevents duplicate uses)

**Errors:**
| Error | Condition |
|---|---|
| `Unauthorized` | Caller is not the config owner, or contract is paused |
| `UntrustedTokenContract` | `usdc_contract` ≠ stored trusted address |
| `SelfTransferNotAllowed` | Any destination account equals `from` |
| `InvalidAmount` | `total_amount` ≤ 0 |
| `NotInitialized` | Contract not yet initialized |
| `InvalidNonce` | Sequential nonce incorrect |
| `DeadlineExpired` | Request timestamp exceeded the `deadline` |
| `RequestHashMismatch` | Sent `request_hash` does not bind the correct parameters |
| `NonceAlreadyUsed` | Replay attempt within duplicate window |

#### `update_split(env, caller, nonce, spending_percent, savings_percent, bills_percent, insurance_percent) -> bool`

Updates split percentages. Owner-only, nonce-protected, and blocked while paused.

---

### Snapshot Export / Import

#### `export_snapshot(env, caller) -> Option<ExportSnapshot>`

Exports the current split configuration as a portable, integrity-verified snapshot.

The snapshot includes a **FNV-1a checksum** computed over:
- snapshot `version`
- all four percentage fields
- `config.timestamp`
- `config.initialized` flag
- `exported_at` (ledger timestamp at export time)

**Parameters:**
- `caller`: Address of the owner (must authorize)

**Returns:** `Some(ExportSnapshot)` on success, `None` if not initialized

**Events:** emits `SplitEvent::SnapshotExported`

**ExportSnapshot structure:**
```rust
pub struct ExportSnapshot {
    pub version: u32,      // snapshot format version (currently 2)
    pub checksum: u64,     // FNV-1a integrity hash
    pub config: SplitConfig,
    pub exported_at: u64,  // ledger timestamp at export
}
```

---

#### `import_snapshot(env, caller, nonce, snapshot) -> bool`

Restores a split configuration from a previously exported snapshot.

**Integrity checks performed (in order):**

| # | Check | Error |
|---|-------|-------|
| 1 | `snapshot.version` within `[MIN_SNAPSHOT_VERSION, SNAPSHOT_VERSION]` | `UnsupportedVersion` |
| 2 | FNV-1a checksum matches recomputed value | `ChecksumMismatch` |
| 3 | `snapshot.config.initialized == true` | `SnapshotNotInitialized` |
| 4 | Each percentage field `<= 100` | `InvalidPercentageRange` |
| 5 | Sum of percentages `== 100` | `InvalidPercentages` |
| 6 | `config.timestamp` and `exported_at` not in the future | `FutureTimestamp` |
| 7 | Caller is the current contract owner | `Unauthorized` |
| 8 | `snapshot.config.owner == caller` | `OwnerMismatch` |

**Parameters:**
- `caller`: Address of the caller (must be current owner and snapshot owner)
- `nonce`: Replay-protection nonce (must equal current stored nonce)
- `snapshot`: `ExportSnapshot` returned by `export_snapshot`

**Returns:** `true` on success

**Events:** emits `SplitEvent::SnapshotImported`

**Note:** `nonce` is only incremented by `initialize_split` and `import_snapshot`. `update_split` checks the nonce but does **not** increment it.

---

#### `verify_snapshot(env, snapshot) -> bool`

Read-only integrity check for a snapshot payload — performs all structural checks (version, checksum, initialized flag, percentage ranges and sum, timestamp bounds) without requiring authorization or modifying state.

**Parameters:**
- `snapshot`: `ExportSnapshot` to verify

**Returns:** `true` if all integrity checks pass, `false` otherwise

**Use case:** pre-flight validation before calling `import_snapshot`, or off-chain verification of exported payloads.

---

## Snapshot Import Validation

### Ordered Validation Pipeline

`import_snapshot` runs the following checks in strict order. The first failing check aborts the
call, appends a failed audit entry, and returns the corresponding error. No state is written on
failure.

| Step | Guard | Error returned |
|------|-------|----------------|
| 1 | `caller.require_auth()` + contract not paused + nonce matches | `Unauthorized` / `InvalidNonce` |
| 2 | `snapshot.version` within `[MIN_SUPPORTED_SCHEMA_VERSION, SCHEMA_VERSION]` | `UnsupportedVersion` |
| 3 | FNV-1a checksum matches recomputed value | `ChecksumMismatch` |
| 4 | `snapshot.config.initialized == true` | `SnapshotNotInitialized` |
| 5 | Each percentage field `<= 100` | `InvalidPercentageRange` |
| 6 | Sum of all four percentage fields `== 100` | `InvalidPercentages` |
| 7 | `snapshot.config.timestamp` and `exported_at` are not in the future | `InvalidAmount` |
| 8 | Caller is the current on-chain owner (`existing.owner == caller`) | `Unauthorized` |
| 9 | Snapshot owner matches caller (`snapshot.config.owner == caller`) | `OwnerMismatch` |

### New Error Variants (discriminants 17–20)

These variants were added as part of the snapshot import hardening and extend the
`RemittanceSplitError` enum:

| Discriminant | Variant | Trigger condition |
|---|---|---|
| 17 | `SnapshotNotInitialized` | The snapshot's `config.initialized` flag is `false`; importing an uninitialized config is rejected. |
| 18 | `FutureTimestamp` | Reserved for future use; the pipeline currently maps future-timestamp failures to `InvalidAmount` (discriminant 4). |
| 19 | `OwnerMismatch` | `snapshot.config.owner` does not equal the calling address, meaning the snapshot was exported by a different owner. |
| 20 | `InvalidPercentageRange` | At least one of the four percentage fields exceeds 100; delegated to `validate_percentages`. |

### `verify_snapshot` Pre-flight Helper

`verify_snapshot` is a **stateless, read-only** function that mirrors steps 2–7 of the
`import_snapshot` pipeline. It is intended as a pre-flight check before committing a nonce and
writing state.

**Checks performed (in order):**

| Step | Guard | Error returned |
|------|-------|----------------|
| 1 | Schema version within supported range | `UnsupportedVersion` |
| 2 | FNV-1a checksum integrity | `ChecksumMismatch` |
| 3 | `config.initialized == true` | `SnapshotNotInitialized` |
| 4 | Per-field percentage range (`<= 100`) | `InvalidPercentageRange` |
| 5 | Percentage sum `== 100` | `InvalidPercentages` |
| 6 | Timestamp not in the future | `InvalidAmount` |

**Not checked by `verify_snapshot`:**
- Caller authorization / nonce (steps 1, 8 of the full pipeline)
- Ownership match (step 9 of the full pipeline)

This means `verify_snapshot` can be called by anyone without consuming a nonce or requiring the
caller to be the contract owner. It returns `true` when all structural checks pass and `false`
(or propagates an error) when any check fails.

#### `calculate_split(env, total_amount) -> Vec<i128>`

Storage-read-only calculation — returns `[spending, savings, bills, insurance]` amounts.
Insurance receives the integer-division remainder to guarantee `sum == total_amount`.
This helper remains callable while paused.

#### `set_pause_admin(env, caller, new_admin) -> ()`

Transfers pause authority to `new_admin`. Owner-only and blocked while paused.

#### `pause(env, caller) -> ()`

Enables the global emergency stop. Only the active pause admin may call it.

#### `unpause(env, caller) -> ()`

Disables the global emergency stop. This is the only mutating entrypoint callable while paused.

#### `set_upgrade_admin(env, caller, new_admin) -> ()`

Assigns or transfers upgrade authority to `new_admin`. The owner sets the initial admin; after
that, only the current upgrade admin may transfer the role. Blocked while paused.

#### `set_version(env, caller, new_version) -> ()`

Persists a new version marker for migrations. Upgrade-admin-only and blocked while paused.

#### `import_snapshot(env, caller, nonce, snapshot) -> bool`

Imports a validated snapshot back into contract storage. Owner-only, nonce-protected, and blocked
while paused. Snapshots must carry a supported `schema_version` and a valid checksum.

#### `create_remittance_schedule(env, owner, amount, next_due, interval) -> u32`

Creates a remittance schedule. Blocked while paused.

#### `modify_remittance_schedule(env, caller, schedule_id, amount, next_due, interval) -> bool`

Updates a remittance schedule. Owner-only and blocked while paused.

#### `cancel_remittance_schedule(env, caller, schedule_id) -> bool`

Cancels a remittance schedule. Owner-only and blocked while paused.

#### `get_config(env) -> Option<SplitConfig>`

Returns the current configuration, or `None` if not initialized.

#### `get_nonce(env, address) -> u64`

Returns the current nonce for `address`. Pass this value as the `nonce` argument on the next call.

#### `create_remittance_schedule(env, owner, amount, next_due, interval) -> u32`

Creates a recurring or one-time remittance schedule.
- **Constraints:**
  - `owner` must be the same address as the contract `config.owner`.
  - `amount` must be > 0.
  - `next_due` must be in the future and ≤ 1 year from now.
  - `interval` must be ≥ 1 hour if recurring (> 0).

#### `modify_remittance_schedule(env, caller, schedule_id, amount, next_due, interval) -> bool`

Updates an existing schedule.
- **Constraints:**
  - `caller` must be the `config.owner`.
  - Schedule must be `active`.
  - All creation constraints apply to the new parameters.

#### `cancel_remittance_schedule(env, caller, schedule_id) -> bool`

Deactivates a schedule.
- **Constraints:**
  - `caller` must be the `config.owner`.
  - Schedule must be `active`.

## Error Reference

```rust
pub enum RemittanceSplitError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    PercentagesDoNotSumTo100 = 3,
    InvalidAmount = 4,
    Overflow = 5,
    Unauthorized = 6,
    InvalidNonce = 7,
    UnsupportedVersion = 8,
    ChecksumMismatch = 9,
    InvalidDueDate = 10,
    ScheduleNotFound = 11,
    UntrustedTokenContract = 12,   // token substitution attack prevention
    SelfTransferNotAllowed = 13,   // self-transfer guard
    DeadlineExpired = 14,          // request expired
    RequestHashMismatch = 15,      // request hash binding failed
    NonceAlreadyUsed = 16,         // replay duplicate protection
    SnapshotNotInitialized = 17,   // snapshot config.initialized is false
    FutureTimestamp = 18,          // reserved; pipeline uses InvalidAmount for future timestamps
    OwnerMismatch = 19,            // snapshot.config.owner != caller
    InvalidPercentageRange = 20,   // a percentage field exceeds 100
}
```

## Events

| Topic | Data | When |
|---|---|---|
| `("split", Initialized)` | `owner: Address` | `initialize_split` succeeds |
| `("split", Updated)` | `caller: Address` | `update_split` succeeds |
| `("split", Calculated)` | `total_amount: i128` | `calculate_split` called |
| `("split", DistributionCompleted)` | `(from: Address, total_amount: i128)` | `distribute_usdc` succeeds |
| `("split", SnapshotExported)` | `caller: Address` | `export_snapshot` succeeds |
| `("split", SnapshotImported)` | `caller: Address` | `import_snapshot` succeeds |

## Security Assumptions

- The `usdc_contract` address passed to `initialize_split` must be a legitimate SEP-41 token.
  The contract does not verify the token's bytecode — it trusts the address provided at init.
- The owner is responsible for keeping their signing key secure. There is no key rotation
  mechanism; deploy a new contract instance if ownership must change.
- Nonces are per-address and stored in instance storage. They are not shared across contract
  instances.
- The pause mechanism is a defense-in-depth control. It freezes all mutating entrypoints except
  `unpause`, but it does not protect against a compromised owner key or compromised pause admin.
- Pause-admin transfer, upgrade-admin transfer, version changes, snapshot imports, and schedule
  changes all require the contract to be unpaused first.

## Running Tests

```bash
cargo test -p remittance_split
```

Test coverage includes:
- Happy-path distribution with real SAC token balances verified
- All 7 auth checks individually (owner, token, self-transfer, pause, nonce, amount, init)
- Replay attack prevention
- `update_split` nonce advancement and replay rejection
- Pause admin transfer and unauthorized pause attempts
- Paused-path coverage for upgrade admin changes, version changes, snapshot imports, and schedule mutators
- Unpause recovery checks for split updates, distributions, and schedule operations
- Rounding correctness (sum always equals total)
- Overflow detection for large i128 values
- Boundary percentages (100/0/0/0, 0/0/0/100, 25/25/25/25)
- Multiple sequential distributions with nonce advancement
- Event emission verification
- TTL extension on initialization
