# Contributor Overview

Welcome to **RemitWise Contracts**! This guide is written for new **contributors** to get up to speed and productive on day one without needing to parse past commits or tribal knowledge.

---

## Workspace Structure

The workspace contains high-performance Soroban smart contracts built for the Stellar ecosystem:

* **`remitwise-common`**: Shared enums (`Category`, `FamilyRole`, `CoverageType`), error types, constants, and standard event emission tools (`RemitwiseEvents`).
* **`remittance_split`**: Automated allocation of incoming remittance funds across categories (spending, savings, bills, insurance).
* **`savings_goals`**: Goal-based savings lockups with target dates and state management.
* **`bill_payments`**: Automated bill tracking, recurring payment schedules, and execution.
* **`insurance`**: Micro-insurance policy registry, premium payments, and status tracking.
* **`family_wallet`**: Governance, multi-signature controls, daily spending limits, and emergency transfer fallbacks.
* **`orchestrator`**: Cross-contract routing and atomic execution across ecosystem modules.
* **`reporting`**: Aggregate financial health scores and summary metrics with graceful degradation.
* **`emergency_killswitch`**: Emergency pause controls and administrative overrides.

---

## Environment Setup

### Prerequisites

1. **Rust Toolchain**: Install stable Rust with the WASM target:
   ```bash
   rustup toolchain install stable
   rustup target add wasm32-unknown-unknown
   ```
2. **Soroban CLI**:
   ```bash
   cargo install --locked --version 21.0.0 soroban-cli
   ```

---

## Core Development Standards

### 1. `#![no_std]` Discipline
Contracts compiled for WASM must remain strictly `#![no_std]`.
* Do **not** use `std::vec::Vec`, `std::collections::HashMap`, or standard memory primitives in contract code.
* Use `soroban_sdk` types (`soroban_sdk::Vec`, `soroban_sdk::Map`, `soroban_sdk::Bytes`, `soroban_sdk::Symbol`, `soroban_sdk::Address`).
* Unit tests (annotated with `#[cfg(test)]`) may use `std` or dev-dependencies like `ed25519-dalek`.

### 2. Authorization & Security
* Every state-changing function modifying user assets or settings **must** verify authority using `address.require_auth()`.
* Access control relies on `FamilyRole` and administrative keys stored in contract instance storage.

### 3. State & Storage TTL
* Use `env.storage().instance()` for contract-wide configuration.
* Use `env.storage().persistent()` for persistent user state (e.g. goals, bills, policies).
* Extend storage TTL using standard thresholds (`INSTANCE_LIFETIME_THRESHOLD`, `INSTANCE_BUMP_AMOUNT`) defined in `remitwise-common`.

---

## Concrete Contract Example

Here is a minimal, complete entrypoint pattern following codebase conventions:

```rust
#![no_std]
use soroban_sdk::{contract, contractimpl, Symbol, Address, Env};

#[contract]
pub struct ContributorExampleContract;

#[contractimpl]
impl ContributorExampleContract {
    /// Increments a user interaction counter and verifies caller signature.
    pub fn record_action(env: Env, caller: Address) -> u32 {
        caller.require_auth();

        let count_key = Symbol::new(&env, "counter");
        let mut count: u32 = env.storage().instance().get(&count_key).unwrap_or(0);
        count += 1;
        
        env.storage().instance().set(&count_key, &count);
        count
    }
}
```

---

## Verification & Testing Workflow

Before opening a pull request, run the following verification steps locally:

1. **Verify WASM Build**:
   ```bash
   cargo build --release --target wasm32-unknown-unknown
   ```

2. **Run Workspace Tests**:
   ```bash
   cargo test --workspace
   ```

3. **Run Package-Specific Tests**:
   ```bash
   cargo test -p remitwise-common
   cargo test -p family_wallet
   ```

4. **Lint and Static Analysis**:
   ```bash
   cargo clippy --workspace --all-targets -- -D warnings
   ```

5. **Format Check**:
   ```bash
   cargo fmt --all -- --check
   ```

---

## Related Documentation

* [Architecture Overview](../ARCHITECTURE.md)
* [Storage Layout Reference](../STORAGE_LAYOUT.md)
* [Authorization Matrix](AUTHORIZATION_MATRIX.md)
* [Threat Model](../THREAT_MODEL.md)

