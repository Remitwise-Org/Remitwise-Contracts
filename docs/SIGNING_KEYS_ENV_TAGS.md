# Signing Keys Carry Environment Tags

**Audience:** Downstream integrators — mobile-app developers, backend relayers, and smart-contract engineers who build on top of the RemitWise contracts.

## Problem

A signed authorization produced for one Stellar network (e.g. Testnet) or one contract instance must not be valid on another (e.g. Mainnet). Without environment binding, an attacker who captures a signed message from a test environment could replay it against a production contract.

The RemitWise contracts solve this with three layered defences, each of which embeds an environment tag into the signing key or signed payload:

| Layer | Tag | What it binds | Where enforced |
|-------|-----|---------------|----------------|
| 1. Verifier network binding | `network_id` (SHA-256 of network passphrase) | A verifier public key to the Stellar network where it was registered | `remitwise_common::require_registered_verifier` |
| 2. Domain separator | Opaque byte string (e.g. `b"distribute_usdc_v1"`) | A signed message to a specific entrypoint or operation domain | `remitwise_common::verify_signature` |
| 3. Actor epoch | `u64` counter | An actor token to a specific generation of the orchestrator's epoch | `Orchestrator::verify_matching_epoch` |

---

## 1. Verifier Network Binding

### How it works

When an off-chain verifier (e.g. an attestation service) is registered, the contract stores the mapping:

```
public_key → network_id
```

where `network_id` is `env.ledger().network_id()` — the SHA-256 hash of the Stellar network passphrase (e.g. `"Test SDF Network ; September 2015"` for Testnet, `"Public Global Stellar Network ; September 2015"` for Mainnet).

At verification time, `require_registered_verifier` checks:

```
if stored_network_id == current_network_id → accept
if stored_network_id != current_network_id → VerifierNetworkMismatch
if key not found → UnregisteredVerifier
```

### Concrete example

```rust
// Testnet registration (e.g. in a CI integration test)
register_verifier(&env, &testnet_verifier_pk);
// network_id stored = hash("Test SDF Network ; September 2015")

// If the same storage is replayed onto Mainnet:
require_registered_verifier(&mainnet_env, &testnet_verifier_pk);
// → Err(SignatureError::VerifierNetworkMismatch)
// because current network_id = hash("Public Global Stellar Network ; September 2015")
```

### What it prevents

- A verifier key that was only ever intended to be trusted on Testnet cannot silently become trusted on Mainnet if the underlying storage entry is copied or replayed across deployments.
- A relayer cannot take a signed attestation from Testnet and submit it to a Mainnet contract instance.

### Code location

- `remitwise_common::register_verifier` — stores `(public_key, network_id)` in instance storage.
- `remitwise_common::require_registered_verifier` — enforces the network match.
- `remitwise_common::verify_signature` — calls `require_registered_verifier` before any Ed25519 verification.

---

## 2. Domain Separators

### How it works

Every call to `verify_signature` accepts a `domain_separator: &[u8]` parameter. The signed payload is constructed as:

```
preimage = domain_separator || message
```

The domain separator is a fixed byte string chosen per entrypoint or operation type. For example, the remittance-split contract uses:

```rust
const DISTRIBUTE_USDC_DOMAIN: &[u8] = b"distribute_usdc_v1";
```

The payload is encoded as a **length-delimited byte stream** to prevent adjacent or overlapping separators and messages from colliding:

```
encoded = LE_u64(len(domain_separator)) || domain_separator || LE_u64(len(message)) || message
```

This means the pair `(domain="ab", message="cdef")` and `(domain="abc", message="def")` produce different encoded payloads, even though their plain concatenation would be identical.

### Concrete example

```rust
// A signature produced for the "distribute_usdc_v1" domain:
let domain_a = b"distribute_usdc_v1";
let message  = b"send 100 USDC to savings";
// signed = Ed25519(domain_a || message)

// Replayed against a different domain:
let domain_b = b"distribute_eth_v1";
verify_signature(env, domain_b, message, signature, public_key);
// → Err(SignatureError::VerificationFailed)
// because the signed payload was domain_a || message, not domain_b || message
```

### What it prevents

- A signature produced for one operation type (e.g. USDC distribution) cannot be replayed against a different operation (e.g. ETH distribution).
- A signature produced for one contract entrypoint cannot be replayed against another entrypoint that uses a different domain separator.

### Code location

- `remitwise_common::verify_signature` — prepends the domain separator and calls `env.crypto().ed25519_verify`.
- `remittance_split/src/lib.rs` — defines `DISTRIBUTE_USDC_DOMAIN` and passes it to the hash construction.

---

## 3. Actor Epoch

### How it works

The orchestrator contract maintains a `u64` epoch counter in instance storage. Every signed remittance-flow request must include the current epoch value:

```rust
pub fn execute_remittance_flow_signed(
    env: Env,
    executor: Address,
    amount: i128,
    nonce: u64,
    deadline: u64,
    request_hash: u64,
    actor_epoch: u64,  // ← environment tag
) -> Result<bool, OrchestratorError>;
```

The contract verifies:

```
if actor_epoch == current_epoch → accept
if actor_epoch != current_epoch → EpochMismatch
```

The contract owner can call `bump_actor_epoch` to increment the counter, which instantly invalidates **all** previously signed actor tokens.

### Concrete example

```
1. Actor fetches current epoch:  get_actor_epoch_public() → 0
2. Actor signs a flow request with actor_epoch = 0
3. Contract owner detects a compromise and calls bump_actor_epoch() → epoch becomes 1
4. Actor submits the previously signed request with actor_epoch = 0
5. verify_matching_epoch(env, 0) → Err(EpochMismatch) because current epoch is 1
```

### What it prevents

- An attacker who obtains a stale actor token (e.g. through a compromised signing service) cannot replay it after the epoch has been bumped.
- A signed request from a previous deployment or test run is rejected if the epoch has changed.

### Code location

- `Orchestrator::bump_actor_epoch` — increments the epoch counter (owner-only).
- `Orchestrator::get_actor_epoch` — reads the current epoch from instance storage.
- `Orchestrator::verify_matching_epoch` — enforces the epoch match.
- `Orchestrator::execute_remittance_flow_signed` — calls `verify_matching_epoch` at step 5 of the execution pipeline.

---

## Defence-in-Depth Summary

These three layers compose to provide defence-in-depth against cross-environment replay:

| Attack scenario | Layer that blocks it |
|----------------|----------------------|
| Verifier key copied from Testnet storage to Mainnet storage | Verifier network binding (layer 1) |
| Signed attestation from Testnet submitted to Mainnet | Verifier network binding (layer 1) |
| Signature from one operation type replayed against another | Domain separator (layer 2) |
| Signature from one entrypoint replayed against a different entrypoint | Domain separator (layer 2) |
| Stale actor token replayed after epoch bump | Actor epoch (layer 3) |
| Signed request from a previous deployment replayed against a new deployment | Actor epoch (layer 3) |

---

## Testing the Guards

The workspace includes tests that verify each layer:

```bash
# Verifier network binding
cargo test -p remitwise-common -- test_verifier_network_mismatch

# Domain separator cross-domain replay
cargo test -p remitwise-common -- test_sign_for_domain_a_replay_against_domain_b_fails
cargo test -p remitwise-common -- proptest_signature_signed_for_domain_a_replayed_against_domain_b_fails

# Actor epoch
cargo test -p orchestrator -- epoch_mismatch
cargo test -p orchestrator -- cross_contract_epoch_guard
```

---

## Related Documents

- [Orchestrator Signed-Flow Request-Hash and Deadline Model](ORCHESTRATOR_SIGNING.md) — detailed request-hash construction and deadline validation.
- [Cross-Contract Invariants](CROSS_CONTRACT_INVARIANTS.md) — replay-protection invariants that span multiple contracts.
- [Threat Model](THREAT_MODEL.md) — comprehensive threat model covering replay attacks.
- [Committed Hashes](COMMITTED_HASHES.md) — exact hash computation boundaries for downstream integrators.