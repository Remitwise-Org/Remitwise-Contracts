# Operator Signature Scopes

**Target Audience:** Operators, auditors, and contributors reviewing Ed25519 signature verification in the Remitwise contracts.

This document describes how operator keys are scoped per operation in the `remitwise_common` signature verification utilities. It covers the domain-separation mechanism, the verifier registry, and the concrete scope boundaries that prevent cross-operation replay attacks.

---

## 1. Overview

All Ed25519 signature verification in the Remitwise contracts flows through two functions in `remitwise-common/src/lib.rs`:

| Function | Purpose | Scope Binding |
|---|---|---|
| `verify_signature` | General-purpose Ed25519 verification with domain separation | Caller-supplied domain separator |
| `verify_slash_signature` | Defence-in-depth gate for destructive slash operations | Hardcoded `slash-auth` domain |

Both functions require the signer's public key to be registered via `register_verifier` before any verification can succeed.

---

## 2. Domain Separator Mechanism

### How it works

`verify_signature(env, domain_separator, message, signature, public_key)` constructs a length-delimited byte stream from the domain separator and message:

```
payload = len(domain_separator) || domain_separator || len(message) || message
```

The length prefixes are encoded as little-endian `u64` values. This encoding prevents collision attacks where overlapping domain/message pairs would produce identical payloads under plain concatenation.

### Why it matters

Without domain separation, a valid signature over one operation could be replayed on a different operation. For example, a slash authorization could be replayed as a flow execution authorization if both used the same (or empty) domain.

Each operation must use a unique domain separator so signatures are bound to exactly one operation type.

---

## 3. Concrete Signature Scopes

### 3.1 Slash Signatures (`slash-auth`)

**Function:** `verify_slash_signature` (`remitwise-common/src/lib.rs:1507`)

```
domain_separator = b"slash-auth"
```

Slash signatures provide a defence-in-depth gate before executing destructive slash operations. The slash signature is **optional**: if no signature is provided, the gate passes (allowing the slash to proceed under normal auth). If a signature *is* provided, it must be valid under the `slash-auth` domain.

**Scope boundary:** A `slash-auth` signature cannot be replayed on any other operation because no other function uses this domain separator.

### 3.2 Orchestrator Signed Flow

**Function:** `execute_remittance_flow_signed` (`orchestrator/src/lib.rs:529`)

The orchestrator's signed flow uses a **request hash** rather than a raw Ed25519 signature over the domain-separated payload. The request hash is a 64-bit value that binds:

- The operation symbol (`"flow"`)
- The caller's nonce
- The amount (both halves of the `i128`)
- The deadline timestamp
- Routing IDs (goal, bill, policy)

> **Note:** The orchestrator signed flow is a collision-resistant binding, not a cryptographic MAC. For production signing, callers should use an off-chain Ed25519 signature over the same fields and the orchestrator should verify it with `remitwise_common::verify_signature`.

**Scope boundary:** The request hash binds all operation parameters. Changing any field (amount, deadline, routing IDs) invalidates the hash. The nonce prevents replay, and the deadline prevents stale execution.

### 3.3 Future Domain Separators

When new signed operations are added to the contracts, each must:

1. Choose a unique domain separator string (e.g., `b"insurance-premium"`, `b"bill-payment"`)
2. Call `verify_signature(env, domain, message, sig, public_key)`
3. Register the signing key via `register_verifier` beforehand

---

## 4. Verifier Registry

### Registration

Before any signature can be verified, the signer's Ed25519 public key must be registered:

```rust
register_verifier(env, public_key)  // 32-byte Ed25519 public key
```

Registration binds the key to `env.ledger().network_id()` (the SHA-256 hash of the Stellar network passphrase). This prevents a key provisioned for Testnet from being trusted on Mainnet if storage entries are copied or replayed across deployments.

### Verification

`require_registered_verifier(env, public_key)` checks two conditions:

1. The key exists in the registered verifiers map.
2. The key's registered `network_id` matches the current network.

If either check fails, `verify_signature` returns an error before attempting cryptographic verification.

### Revocation

To revoke a verifier, remove it from the registered verifiers map. Once revoked, any in-flight transactions signed by that key will fail the `require_registered_verifier` preflight.

See [VERIFIER_REGISTRY.md](VERIFIER_REGISTRY.md) for the full operational workflow (adding, rotating, and revoking verifiers).

---

## 5. Scope Summary

| Operation | Domain Separator | Key Required | Scope Binding |
|---|---|---|---|
| Slash authorization | `b"slash-auth"` | Registered Ed25519 public key | Message payload (slash amount/params) |
| Orchestrator signed flow | N/A (request hash) | Caller's Stellar address | Operation symbol, nonce, amount, deadline, routing IDs |
| Any future signed entry | Caller-chosen unique domain | Registered Ed25519 public key | Domain separator + message |

---

## 6. Security Properties

| Property | Mechanism |
|---|---|
| Cross-operation replay prevention | Unique domain separators per operation |
| Cross-network replay prevention | Verifier keys bound to `network_id` at registration time |
| Registration enforcement | `require_registered_verifier` called before every `verify_signature` |
| Length-delimited encoding | Prevents collision between overlapping domain/message pairs |
| Optional slash gate | Slash signature is optional (allows unsigned slashes), but if provided must be valid |
| Nonce replay prevention (orchestrator) | Bounded ring-buffer of used nonces per address |

---

## 7. Related Documents

| Document | Covers |
|---|---|
| [VERIFIER_REGISTRY.md](VERIFIER_REGISTRY.md) | Verifier lifecycle: adding, rotating, revoking Ed25519 keys |
| [ORCHESTRATOR_SIGNING.md](ORCHESTRATOR_SIGNING.md) | Request hash construction, deadline model, and replay protection for the signed flow |
| [OPERATORS.md](OPERATORS.md) | Operator management: adding, rotating, and revoking pause/upgrade admins |
| [SECURITY_REVIEW.md](SECURITY_REVIEW.md) | Full security review of the contract suite |
