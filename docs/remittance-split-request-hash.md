# Remittance Split — Request Hash Preimage Documentation

## Overview

`get_request_hash` computes the canonical SHA-256 binding hash for a `DistributeUsdcRequest`.
The hash is used by `distribute_usdc_hashed` to verify that the on-chain request matches what
the signer authorised off-chain, preventing field-substitution (confused-deputy) attacks.

## Preimage Layout

Fields are concatenated in the following fixed order and hashed with SHA-256:

| # | Field | Encoding | Source |
|---|-------|----------|--------|
| 1 | `DISTRIBUTE_USDC_DOMAIN` (`b"distribute_usdc_v1"`) | raw bytes | constant |
| 2 | `domain_id` = `symbol_short!("distrib")` | `Val::get_payload()` as u64 LE | `SplitAuthPayload`-style functional tag |
| 3 | `request.from` | `Val::get_payload()` as u64 LE | sender address |
| 4 | `request.usdc_contract` | `Val::get_payload()` as u64 LE | token contract address |
| 5 | `request.accounts.spending` | `Val::get_payload()` as u64 LE | spending destination |
| 6 | `request.accounts.savings` | `Val::get_payload()` as u64 LE | savings destination |
| 7 | `request.accounts.bills` | `Val::get_payload()` as u64 LE | bills destination |
| 8 | `request.accounts.insurance` | `Val::get_payload()` as u64 LE | insurance destination |
| 9 | `request.total_amount` | i128 as 16 bytes LE | amount to distribute |
| 10 | `request.nonce` | u64 as 8 bytes LE | replay-protection nonce |
| 11 | `request.deadline` | u64 as 8 bytes LE | expiry timestamp |

**Output:** 32-byte `Bytes` (SHA-256 digest)

## Security Properties

### Field-Substitution Resistance
Every scalar field contributes to the hash. Mutating any single field while keeping
the original hash causes `distribute_usdc_hashed` to return
`RemittanceSplitError::RequestHashMismatch` (error code 15).

Covered fields and their tamper tests:

| Field | Test |
|-------|------|
| `from` | `test_request_hash_mismatch_on_from_tamper` |
| `usdc_contract` | `test_request_hash_mismatch_on_usdc_contract_tamper` |
| `total_amount` | `test_request_hash_mismatch_on_amount_tamper` |
| `nonce` | `test_request_hash_mismatch_on_nonce_tamper` |
| `deadline` | `test_request_hash_mismatch_on_deadline_tamper` |
| All account addresses | `test_request_hash_changes_with_parameters` |

### Cross-Domain Replay Protection
The `DISTRIBUTE_USDC_DOMAIN` constant (`b"distribute_usdc_v1"`) and the `"distrib"` domain
tag (analogous to `SplitAuthPayload.domain_id`) prevent a hash computed for one entrypoint
from being replayed against another entrypoint. See `test_request_hash_mismatch_on_domain_id_swap`.

### Nonce Reuse
The `nonce` field is included in the hash. Reusing the same nonce with a different deadline
produces a hash mismatch. See `test_request_hash_mismatch_nonce_reuse_new_deadline`.

## Usage

```rust
// Off-chain: build request and compute hash
let request = DistributeUsdcRequest { from, usdc_contract, nonce, accounts, total_amount, deadline };
let hash = client.get_request_hash(&request);

// On-chain: submit with hash — contract recomputes and verifies
client.distribute_usdc_hashed(&request, &hash);
```

For the recommended consumer verification sequence and a comparison with the
legacy `u64` request fingerprints, see
[Committed Hashes for Downstream Integrators](COMMITTED_HASHES.md).
