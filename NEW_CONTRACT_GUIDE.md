# Adding a New Contract to RemitWise

This guide walks you through every step required to add a new Soroban smart contract to the RemitWise workspace. Follow each section in order and use the checklist at the bottom before submitting your pull request.

---

## Table of Contents

1. [Directory Structure](#1-directory-structure)
2. [Contract Patterns](#2-contract-patterns)
   - [Storage](#storage)
   - [Events](#events)
   - [Errors](#errors)
3. [Writing Tests](#3-writing-tests)
4. [Gas Benchmarks](#4-gas-benchmarks)
5. [Linting & Formatting](#5-linting--formatting)
6. [CI Hooks](#6-ci-hooks)
7. [Documentation](#7-documentation)
8. [Linking Into the Workspace](#8-linking-into-the-workspace)
9. [New Contract Checklist](#9-new-contract-checklist)

---

## 1. Directory Structure

Create a Cargo library crate at the workspace root. Use `snake_case` for the crate name.

```
remitwise-contracts/
└── your_contract/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs          # Contract entry-point; re-exports public types
    │   ├── contract.rs     # #[contract] impl block
    │   ├── storage.rs      # All storage keys and read/write helpers
    │   ├── events.rs       # Event structs and emit helpers
    │   ├── errors.rs       # ContractError enum
    │   └── types.rs        # Shared structs / enums (optional)
    └── tests/
        ├── integration.rs  # Full happy-path + edge-case tests
        └── gas_bench.rs    # Gas / resource benchmarks
```

### `Cargo.toml` template

```toml
[package]
name = "your_contract"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
soroban-sdk = { workspace = true }

[dev-dependencies]
soroban-sdk = { workspace = true, features = ["testutils"] }

[profile.release]
opt-level = "z"
overflow-checks = true
debug = 0
strip = "symbols"
codegen-units = 1
lto = true
```

Add the new crate to the workspace-level `Cargo.toml`:

```toml
[workspace]
members = [
  # ... existing crates ...
  "your_contract",
]
```

---

## 2. Contract Patterns

### Storage

**Rule:** Never use raw `Symbol` strings scattered across the codebase. Centralise every key in `storage.rs`.

```rust
// src/storage.rs
use soroban_sdk::{contracttype, Env, Address};

/// Every storage key for this contract lives here.
#[contracttype]
pub enum DataKey {
    /// Keyed by owner address
    Record(Address),
    /// Singleton admin config
    Config,
}

/// Read helpers return `Option<T>` – callers decide whether to panic.
pub fn get_record(env: &Env, owner: &Address) -> Option<YourType> {
    env.storage().persistent().get(&DataKey::Record(owner.clone()))
}

pub fn set_record(env: &Env, owner: &Address, value: &YourType) {
    env.storage()
        .persistent()
        .set(&DataKey::Record(owner.clone()), value);
}
```

**Storage tier guidance:**

| Tier | Use when |
|---|---|
| `persistent()` | User data that must survive ledger expiry (goals, bills, policies) |
| `temporary()` | Short-lived caches, nonces |
| `instance()` | Contract-level config initialised once (admin, fee rates) |

**Archiving pattern:** When a list grows unboundedly (e.g., paid bills, completed goals) follow the pattern used in `bill_payments` and `savings_goals`:

1. Expose an `archive_*` function that moves records to a separate key.
2. Expose `get_archived_*`, `restore_*`, and `cleanup_old_*` functions.
3. Expose `get_storage_stats` so the frontend can monitor growth.

### Events

**Rule:** Every state-changing function must emit at least one event. Use short `Symbol` topics (≤ 8 chars) for on-chain efficiency.

```rust
// src/events.rs
use soroban_sdk::{contracttype, symbol_short, Env};

/// Published when a new record is created.
#[contracttype]
pub struct RecordCreatedEvent {
    pub record_id: u64,
    pub owner: soroban_sdk::Address,
    pub amount: i128,
    pub timestamp: u64,
}

pub fn emit_record_created(env: &Env, event: RecordCreatedEvent) {
    env.events().publish(
        (symbol_short!("created"),),
        event,
    );
}
```

**Event field requirements:**

- `*_id` field identifying the entity acted upon.
- Monetary `amount` for any financial event.
- `timestamp` — use `env.ledger().timestamp()`.
- Any human-readable context (`name`, `due_date`, etc.) that the frontend needs to render a notification.

**Topic naming convention** — align with existing contracts:

| Action | Topic symbol |
|---|---|
| Create / initialise | `created` / `init` |
| Update / add funds | `added` / `calc` |
| Complete / finish | `completed` |
| Pay | `paid` |
| Recurring creation | `recurring` |
| Deactivate / remove | `deactive` |

### Errors

**Rule:** All contract panics must go through a typed error enum. Never call `panic!()` or `.unwrap()` directly in contract code.

```rust
// src/errors.rs
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ContractError {
    // Initialisation
    AlreadyInitialized    = 1,
    NotInitialized        = 2,

    // Auth
    Unauthorized          = 10,

    // Business logic – start at 100 to leave room above
    RecordNotFound        = 100,
    InsufficientFunds     = 101,
    InvalidAmount         = 102,
    DeadlineExceeded      = 103,
}
```

Reserve ranges per category (as above) so variants never collide when the error list grows.

Use errors in the contract:

```rust
if amount <= 0 {
    return Err(ContractError::InvalidAmount);
}
```

---

## 3. Writing Tests

All tests live in `tests/integration.rs`. Use `soroban_sdk::testutils` — never deploy to a live network for unit or integration tests.

### Minimal test module skeleton

```rust
// tests/integration.rs
#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use your_contract::{YourContract, YourContractClient};

fn setup() -> (Env, Address, YourContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();                         // mock auth for all calls
    let contract_id = env.register_contract(None, YourContract);
    let client = YourContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    (env, admin, client)
}

#[test]
fn test_happy_path() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    // ... assert state
}

#[test]
#[should_panic(expected = "InvalidAmount")]
fn test_rejects_zero_amount() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    client.do_something(&admin, &0_i128); // must panic
}
```

### Coverage requirements

- **Happy path** – every public function called with valid inputs.
- **Edge cases** – boundary values (zero, max), empty lists.
- **Auth checks** – verify unauthorized callers are rejected.
- **Error paths** – at least one `#[should_panic]` test per `ContractError` variant.
- **Event assertions** – verify emitted events with `env.events().all()`.

### USDC / Stellar Asset Contract tests

When your contract transfers USDC, mock the SAC using `env.register_stellar_asset_contract_v2` (see `remittance_split` tests for a reference implementation).

---

## 4. Gas Benchmarks

Every new contract must ship a `tests/gas_bench.rs` file.

```rust
// tests/gas_bench.rs
#![cfg(test)]

use soroban_sdk::Env;
use your_contract::{YourContract, YourContractClient};

#[test]
fn bench_create_record() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, YourContract);
    let client = YourContractClient::new(&env, &id);

    // ... setup ...
    client.create_record(/* args */);

    let resources = env.budget().borrow().resource_per_type();
    println!("CPU (instructions): {}", resources.cpu_insns);
    println!("Memory (bytes):     {}", resources.mem_bytes);

    // Fail loudly if limits are exceeded
    assert!(resources.cpu_insns < 500_000, "CPU budget exceeded");
    assert!(resources.mem_bytes < 50_000,  "Memory budget exceeded");
}
```

Run your benchmarks locally:

```bash
RUST_TEST_THREADS=1 cargo test -p your_contract --test gas_bench -- --nocapture
```

Then add your contract to `scripts/run_gas_benchmarks.sh` so results appear in `gas_results.json`.

---

## 5. Linting & Formatting

The CI pipeline enforces these — fix all warnings locally before pushing.

```bash
# Format
cargo fmt --all

# Lint (must pass with zero warnings)
cargo clippy --all-targets --all-features -- -D warnings

# Check WASM build
cargo build --release --target wasm32-unknown-unknown -p your_contract
```

**Common Clippy fixes for Soroban contracts:**

- Replace `env.storage()...get().unwrap()` with a helper that returns `Option<T>`.
- Derive `Clone` only when needed — prefer `Copy` for small value types.
- Avoid `u32 as i128` casts; use explicit `from`/`try_from`.

---

## 6. CI Hooks

The repository CI (`.github/workflows/`) runs on every push and pull request. Your new contract is automatically included in the workspace-wide jobs. You **must** additionally:

1. **Add a gas benchmark step** — open `.github/workflows/gas-benchmarks.yml` and append:

```yaml
- name: Bench your_contract
  run: RUST_TEST_THREADS=1 cargo test -p your_contract --test gas_bench -- --nocapture
```

2. **Verify the regression script covers your contract** — open `scripts/compare_gas_results.sh` and confirm your contract name is present in the comparison list.

3. **Check CI passes end-to-end** locally with `cargo test --workspace` before opening a PR.

---

## 7. Documentation

### In-code documentation

- Every `pub` function must have a `///` doc comment explaining parameters, return value, panics, and the event emitted.
- Every `ContractError` variant must have a one-line `///` comment.

```rust
/// Creates a new record for `owner`.
///
/// # Panics
/// - [`ContractError::AlreadyInitialized`] if a record already exists.
/// - [`ContractError::InvalidAmount`] if `amount` is zero or negative.
///
/// # Events
/// Emits [`RecordCreatedEvent`] on success.
pub fn create_record(env: Env, owner: Address, amount: i128) -> u64 { ... }
```

### Contract-level README

Create `your_contract/README.md` following the same structure used by the other contracts:

```markdown
# Your Contract

One-sentence description.

## Key Functions
...

## Events
...

## Error Codes
...
```

### Update the workspace README

Add your contract to the **Contracts** section in the root `README.md` following the existing pattern, and link to `NEW_CONTRACT_GUIDE.md` in the **Development** section if it isn't already there.

---

## 8. Linking Into the Workspace

After the files are in place:

```bash
# 1. Verify the whole workspace compiles
cargo build --release --target wasm32-unknown-unknown

# 2. Run all tests
cargo test

# 3. Deploy to testnet (optional during development)
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/your_contract.wasm \
  --source <your-key> \
  --network testnet
```

---

## 9. New Contract Checklist

Use this checklist in your pull request description. Every box must be checked before the PR can be merged.

### Structure
- [ ] Crate created under workspace root with `snake_case` name
- [ ] Crate added to workspace-level `Cargo.toml` `[workspace.members]`
- [ ] Source split into `contract.rs`, `storage.rs`, `events.rs`, `errors.rs`

### Storage
- [ ] All keys defined in a `DataKey` enum in `storage.rs`
- [ ] Read/write helper functions used consistently — no raw key strings in `contract.rs`
- [ ] Archiving pattern implemented if any list can grow unboundedly
- [ ] `get_storage_stats` exposed for unbounded storage

### Events
- [ ] Every state-changing function emits at least one event
- [ ] All events include `id`, `amount` (if financial), and `timestamp`
- [ ] Topic symbols are ≤ 8 characters and follow naming convention

### Errors
- [ ] `ContractError` enum defined with `#[contracterror]`
- [ ] No raw `panic!()` or `.unwrap()` in contract code
- [ ] Each variant has a `///` doc comment

### Tests
- [ ] `tests/integration.rs` covers every public function (happy path)
- [ ] Edge-case and error-path tests with `#[should_panic]`
- [ ] Auth rejection tests present
- [ ] Event emission verified in at least one test
- [ ] All tests pass: `cargo test -p your_contract`

### Gas Benchmarks
- [ ] `tests/gas_bench.rs` created with benchmark for each key operation
- [ ] Contract added to `scripts/run_gas_benchmarks.sh`
- [ ] Benchmarks pass locally: `RUST_TEST_THREADS=1 cargo test -p your_contract --test gas_bench -- --nocapture`

### Linting & Formatting
- [ ] `cargo fmt --all` run with no diff
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [ ] WASM build succeeds: `cargo build --release --target wasm32-unknown-unknown -p your_contract`

### CI
- [ ] Gas benchmark step added to `.github/workflows/gas-benchmarks.yml`
- [ ] `scripts/compare_gas_results.sh` covers new contract
- [ ] `cargo test --workspace` passes locally

### Documentation
- [ ] All `pub` functions have `///` doc comments (params, panics, events)
- [ ] All `ContractError` variants have `///` doc comments
- [ ] `your_contract/README.md` created (functions, events, errors sections)
- [ ] Contract listed in workspace root `README.md` **Contracts** section
- [ ] `NEW_CONTRACT_GUIDE.md` linked from root `README.md` **Development** section (if not already)

---

*For questions, open a discussion on the repository or ping the #contracts channel.*