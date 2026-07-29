# Stable Currencies

**Audience:** Downstream integrators (anyone calling `remittance_split` from a client, backend service, or deployment script)

This document explains why the `remittance_split` contract only accepts a single trusted stable token for distributions, how that token is set, and how — or whether — it can be updated later.

## Why only one trusted stable token

`remittance_split` doesn't validate "is this token a stablecoin" at the protocol level — Soroban tokens don't self-report that. Instead, it pins **one specific token contract address** (referred to as `usdc_contract` throughout the code) at initialization time and rejects any distribution call that supplies a different address:

```rust
// remittance_split/src/lib.rs — distribute_usdc
// 5. Token contract must match the trusted address pinned at initialization.
if config.usdc_contract != usdc_contract {
    Self::append_audit(&env, symbol_short!("distrib"), &from, false);
    return Err(RemittanceSplitError::UntrustedTokenContract);
}
```

The same check runs in `distribute_usdc_hashed` against `request.usdc_contract`.

This exists to prevent **token substitution attacks**: if any token contract address were accepted, a caller could pass a worthless or malicious token contract instead of the real stablecoin, and the contract would report a successful "distribution" while recipients received nothing of value. Pinning a single trusted address at init and re-checking it on every call closes that vector — the contract only ever moves the specific token the owner chose to trust, never whatever address happens to be passed at call time.

In practice, "stable" in this context means: the specific stablecoin contract address (typically a USDC Stellar Asset Contract) that the split owner trusts and configured at initialization — not a category the contract detects automatically.

## How it's set: `initialize_split`

The trusted address is supplied once, as a required argument to `initialize_split`:

```rust
pub fn initialize_split(
    env: Env,
    owner: Address,
    nonce: u64,
    usdc_contract: Address,
    spending_percent: u32,
    savings_percent: u32,
    bills_percent: u32,
    insurance_percent: u32,
) -> Result<bool, RemittanceSplitError>
```

* `usdc_contract` — the trusted stablecoin token contract address; only this address will be permitted in future `distribute_usdc` / `distribute_usdc_hashed` calls.

The repo does not ship a canonical per-network stablecoin address (there's no such reference in `DEPLOYMENT.md` or the deployed-contracts README block). As the integrator, you're responsible for sourcing the correct token contract address for your target network — e.g. the official USDC Stellar Asset Contract on that network — and passing it explicitly when you call `initialize_split`.

## How to update it: you can't — get it right at init

This is the part worth calling out clearly, since it's a common integration mistake: **there is no function that changes `usdc_contract` after initialization.**

`update_split` exists, but it only touches the four allocation percentages:

```rust
pub fn update_split(
    env: Env,
    caller: Address,
    nonce: u64,
    spending_percent: u32,
    savings_percent: u32,
    bills_percent: u32,
    insurance_percent: u32,
) -> Result<bool, RemittanceSplitError>
```

Notice `usdc_contract` isn't even a parameter here. And `initialize_split` can only run once — a second call against an already-initialized split returns `AlreadyInitialized`. Scanning every public function in the contract confirms there's no rotate/set-token admin function anywhere in `remittance_split`.

**Practical consequence:** if you initialize a split with the wrong token address, the only fix is to deploy and initialize a new `remittance_split` contract instance with the correct address — there's no in-place correction. Double-check the `usdc_contract` value before your `initialize_split` call reaches mainnet.

## Every distribution call re-verifies the address

Even though `distribute_usdc` and `distribute_usdc_hashed` both take a `usdc_contract` parameter (needed to construct the `TokenClient` for the actual transfer), that parameter is never trusted at face value — it's checked against the immutable value stored in `SplitConfig` on every call:

```rust
// distribute_usdc_hashed
if config.usdc_contract.ne(&request.usdc_contract) {
    return Err(RemittanceSplitError::UntrustedTokenContract);
}
```

Passing any address other than the one set at `initialize_split` returns `RemittanceSplitError::UntrustedTokenContract` and the call fails before any transfer is attempted.

## See also

- [README.md — "USDC remittance split checks (local & CI)"](../README.md) — how `cargo test -p remittance_split` exercises this with a mocked Stellar Asset Contract.
- [DEPLOYMENT.md](../DEPLOYMENT.md) — deployment steps; does not currently list per-network stablecoin addresses, so confirm the correct address independently before deploying.
