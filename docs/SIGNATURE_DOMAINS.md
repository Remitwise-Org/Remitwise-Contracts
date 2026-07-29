# Signature Domains

A central registry of every domain string used for signature separation across the workspace.

**Audience:** Contributors reviewing or adding signature-verified entrypoints.

Domain separation prevents a signature captured for one purpose from being replayed against a different operation. Every call to `verify_signature` (or an equivalent mechanism like `require_auth_for_args` with a `domain_id` field) binds the domain string into the signed payload so that a valid signature for domain A is invalid for domain B.

## Production domains

| Domain | Value | Kind | Crate | Defined at | Used in |
|---|---|---|---|---|---|
| `slash-auth` | `b"slash-auth"` | Byte literal domain separator | `remitwise-common` | `src/lib.rs:1514` | `verify_slash_signature()` |
| `distribute_usdc_v1` | `b"distribute_usdc_v1"` | `const DISTRIBUTE_USDC_DOMAIN` | `remittance_split` | `src/lib.rs:28` | `get_request_hash()` |
| `init` | `symbol_short!("init")` | Soroban `Symbol` (`SplitAuthPayload.domain_id`) | `remittance_split` | `src/lib.rs:1129` | `initialize_split()` |
| `distrib` | `symbol_short!("distrib")` | Soroban `Symbol` (hash preimage field #2) | `remittance_split` | `src/lib.rs:2240` | `get_request_hash()` |

### `slash-auth`

**File:** `remitwise-common/src/lib.rs:1514`

Used in `verify_slash_signature()` as the domain separator passed to `verify_signature()`. It prevents a slash-authorization signature from being replayed against a different operation or contract.

```rust
if verify_signature(env, b"slash-auth", message, sig, public_key).is_err() {
    return Err(SlashError::InvalidSignature);
}
```

The `verify_signature` function (`remitwise-common/src/lib.rs:1437`) prepends the domain to the message and signs the concatenation:

```
domain ‖ message
```

It then performs a second verification with a length-delimited encoding to prevent adjacent-domain collision attacks:

```
len(domain) ‖ domain ‖ len(message) ‖ message
```

### `distribute_usdc_v1`

**File:** `remittance_split/src/lib.rs:28` (constant definition), `remittance_split/src/lib.rs:2237` (usage)

Raw byte domain separator prepended to the SHA-256 hash preimage in `get_request_hash()`. The version suffix (`_v1`) allows bumping the domain in future contract versions to invalidate old signatures.

```rust
const DISTRIBUTE_USDC_DOMAIN: &[u8] = b"distribute_usdc_v1";

// In get_request_hash():
preimage.extend_from_slice(DISTRIBUTE_USDC_DOMAIN);
```

### `init`

**File:** `remittance_split/src/lib.rs:1129`

Soroban `Symbol` used as `SplitAuthPayload.domain_id` in `initialize_split()`. Bound into `require_auth_for_args()` to scope the owner's authorisation to the `initialize_split` entrypoint alone.

```rust
let payload = SplitAuthPayload {
    domain_id: symbol_short!("init"),
    network_id: env.ledger().network_id(),
    contract_addr: env.current_contract_address(),
    owner_addr: owner.clone(),
    nonce_val: nonce,
    // ...
};
owner.require_auth_for_args(vec![&env, payload.into_val(&env)]);
```

### `distrib`

**File:** `remittance_split/src/lib.rs:2240`

Soroban `Symbol` used as a functional domain tag in the hash preimage of `get_request_hash()`. It is the second field (after the `distribute_usdc_v1` byte separator) and prevents the hash computed for one entrypoint from being replayed against another.

```rust
let did_bits: u64 = symbol_short!("distrib").to_val().get_payload();
preimage.extend_from_slice(&did_bits.to_le_bytes());
```

## Test-only domains

These appear only in `remitwise-common/src/tests.rs` and are not used in production.

| Domain | Value | Purpose |
|---|---|---|
| `test-domain` | `b"test-domain"` | Generic test domain for `test_verify_signature_valid` and related tests |
| `domain1` / `domain2` | `b"domain1"` / `b"domain2"` | Cross-domain replay test |
| `domain-A-auth-v1` / `domain-B-auth-v1` | `b"domain-A-auth-v1"` / `b"domain-B-auth-v1"` | Named domain replay test |
| `abc` / `ab` | `b"abc"` / `b"ab"` | Adjacent domain/message collision test |
| random 8-byte vectors | `proptest::collection::vec(any::<u8>(), 8)` | Property test for cross-domain replay rejection |

## Adding a new domain

1. Choose a descriptive name that is unique across the workspace.
2. If the domain is a byte literal, add it to the table above.
3. If the domain is a Soroban `Symbol`, add it to the table with its `symbol_short!` invocation.
4. Use it consistently as the first binding element in every signature or hash that needs scoping.
5. Reference this document in your PR description.

## Related docs

- [`docs/AUTHORIZATION_MATRIX.md`](AUTHORIZATION_MATRIX.md) — Per-entrypoint caller authorization requirements
- [`docs/COMMITTED_HASHES.md`](COMMITTED_HASHES.md) — Request-hash coverage and verification
- [`docs/remittance-split-request-hash.md`](remittance-split-request-hash.md) — Remittance split request hash preimage structure
