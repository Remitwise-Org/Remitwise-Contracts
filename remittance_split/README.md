# Remittance Split Contract

A Soroban smart contract for configuring and executing percentage-based USDC distributions
across spending, savings, bills, and insurance categories.

## Security Model

`distribute_usdc` is the only function that moves funds. It enforces the following invariants
in strict order before any token interaction occurs:

1. **Auth first** — `from.require_auth()` is the very first operation; no state is read before
   the caller proves authority.
2. **Pause guard** — the contract must not be globally paused.
3. **Owner-only** — `from` must equal the address stored as `config.owner` at initialization.
   Any other address is rejected with `Unauthorized`, even if it can self-authorize.
4. **Trusted token** — `usdc_contract` must match the address pinned in `config.usdc_contract`
   at initialization time. Passing a different address returns `UntrustedTokenContract`,
   preventing token-substitution attacks.
5. **Amount validation** — `total_amount` must be > 0.
6. **Self-transfer guard** — none of the four destination accounts may equal `from`.
   Returns `SelfTransferNotAllowed` if any match.
7. **Replay protection** — nonce must equal `get_nonce(from)` and is incremented after success.
8. **Audit + event** — a `DistributionCompleted` event is emitted on success for off-chain indexing.

## Features

- Percentage-based allocation (spending / savings / bills / insurance, must sum to 100)
- Hardened `distribute_usdc` with 7-layer auth checks
- Nonce-based replay protection on all state-changing operations
- Pause / unpause with admin controls
- Remittance schedules (create / modify / cancel)
- Snapshot export/import with checksum verification
- Audit log (last 100 entries, ring-buffer)
- TTL extension on every state-changing call

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

#### `distribute_usdc(env, usdc_contract, from, nonce, accounts, total_amount) -> bool`

Distributes USDC from `from` to the four split destination accounts.

**Security checks (in order):**
1. `from.require_auth()`
2. Contract not paused
3. `from == config.owner`
4. `usdc_contract == config.usdc_contract`
5. `total_amount > 0`
6. No destination account equals `from`
7. Nonce matches

**Errors:**
| Error | Condition |
|---|---|
| `Unauthorized` | Caller is not the config owner, or contract is paused |
| `UntrustedTokenContract` | `usdc_contract` ≠ stored trusted address |
| `SelfTransferNotAllowed` | Any destination account equals `from` |
| `InvalidAmount` | `total_amount` ≤ 0 |
| `NotInitialized` | Contract not yet initialized |
| `InvalidNonce` | Replay attempt |

#### `update_split(env, caller, nonce, spending_percent, savings_percent, bills_percent, insurance_percent) -> bool`

Updates split percentages. Owner-only, nonce-protected.

#### `calculate_split(env, total_amount) -> Vec<i128>`

Pure calculation — returns `[spending, savings, bills, insurance]` amounts.
Insurance receives the integer-division remainder to guarantee `sum == total_amount`.

#### `get_config(env) -> Option<SplitConfig>`

Returns the current configuration, or `None` if not initialized.

#### `get_nonce(env, address) -> u64`

Returns the current nonce for `address`. Pass this value as the `nonce` argument on the next call.

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
    UntrustedTokenContract = 12,   // NEW: token substitution attack prevention
    SelfTransferNotAllowed = 13,   // NEW: self-transfer guard
}
```

## Events

| Topic | Data | When |
|---|---|---|
| `("split", Initialized)` | `owner: Address` | `initialize_split` succeeds |
| `("split", Updated)` | `caller: Address` | `update_split` succeeds |
| `("split", Calculated)` | `total_amount: i128` | `calculate_split` called |
| `("split", DistributionCompleted)` | `(from: Address, total_amount: i128)` | `distribute_usdc` succeeds |

## Security Assumptions

- The `usdc_contract` address passed to `initialize_split` must be a legitimate SEP-41 token.
  The contract does not verify the token's bytecode — it trusts the address provided at init.
- The owner is responsible for keeping their signing key secure. There is no key rotation
  mechanism; deploy a new contract instance if ownership must change.
- Nonces are per-address and stored in instance storage. They are not shared across contract
  instances.
- The pause mechanism is a defense-in-depth control. It does not protect against a compromised
  owner key.

## Running Tests

```bash
cargo test -p remittance_split
```

Test coverage includes:
- Happy-path distribution with real SAC token balances verified
- All 7 auth checks individually (owner, token, self-transfer, pause, nonce, amount, init)
- Replay attack prevention
- Rounding correctness (sum always equals total)
- Overflow detection for large i128 values
- Boundary percentages (100/0/0/0, 0/0/0/100, 25/25/25/25)
- Multiple sequential distributions with nonce advancement
- Event emission verification
- TTL extension
