# Committed Hashes for Downstream Integrators

This guide identifies the request hashes exposed by the Remitwise contracts,
the fields each hash commits to, and the verification path integrators should
use. It is written for relayers, wallets, and backend services that construct
or submit Remitwise requests.

## Quick Reference

| Contract and entrypoint | Value | Commits to | Recommended verification |
|---|---|---|---|
| `remittance_split::distribute_usdc_hashed` | 32-byte SHA-256 `Bytes` | Domain tag and every field in `DistributeUsdcRequest` | Call `get_request_hash` with the exact request, then submit both unchanged |
| `remittance_split::distribute_usdc` | `u64` fingerprint | Operation, nonce, amount, and deadline | Use `compute_request_hash`; do not treat the result as a signature or cryptographic digest |
| `orchestrator::execute_remittance_flow_signed` | `u64` fingerprint | Operation, nonce, amount, deadline, and the three stored routing IDs | Mirror the current contract formula only when this legacy entrypoint is required |

The two `u64` values are deterministic request fingerprints, not
cryptographic hashes. They detect accidental or unsophisticated parameter
changes, but they do not provide collision resistance or authenticate a
request. Soroban authorization is enforced separately by each entrypoint.

## Canonical Remittance Split Commitment

Use `distribute_usdc_hashed` for a request whose destinations and token address
must be bound to a cryptographic digest. Its public `get_request_hash` helper is
the canonical way to produce the hash.

The preimage is concatenated in this order:

| # | Field | Encoding |
|---|---|---|
| 1 | `b"distribute_usdc_v1"` | Raw domain-separator bytes |
| 2 | `symbol_short!("distrib")` | `Val` payload as `u64` little-endian |
| 3 | `request.from` | `Val` payload as `u64` little-endian |
| 4 | `request.usdc_contract` | `Val` payload as `u64` little-endian |
| 5 | `request.accounts.spending` | `Val` payload as `u64` little-endian |
| 6 | `request.accounts.savings` | `Val` payload as `u64` little-endian |
| 7 | `request.accounts.bills` | `Val` payload as `u64` little-endian |
| 8 | `request.accounts.insurance` | `Val` payload as `u64` little-endian |
| 9 | `request.total_amount` | `i128` little-endian |
| 10 | `request.nonce` | `u64` little-endian |
| 11 | `request.deadline` | `u64` little-endian |

The output is `SHA-256(preimage)` as a 32-byte Soroban `Bytes` value.

### Construct and submit a request

Use the generated contract client so address encoding and hashing match the
deployed contract:

```rust
let request = DistributeUsdcRequest {
    usdc_contract,
    from,
    nonce,
    accounts: AccountGroup {
        spending,
        savings,
        bills,
        insurance,
    },
    total_amount,
    deadline,
};

let request_hash = client.get_request_hash(&request);
assert_eq!(request_hash.len(), 32);

client.distribute_usdc_hashed(&request, &request_hash);
```

Do not modify the request after calling `get_request_hash`. The contract
recomputes the digest before authorization or transfers and returns
`RequestHashMismatch` when the supplied bytes differ.

### Verification checklist

1. Read the current nonce for `request.from`.
2. Choose a non-zero deadline no more than 3,600 seconds after the submission
   ledger timestamp.
3. Build one immutable `DistributeUsdcRequest`.
4. Call `get_request_hash` with that request.
5. Present the request and digest to the authorizing wallet together.
6. Submit the same request and digest to `distribute_usdc_hashed`.
7. Treat `RequestHashMismatch` as a stale or mutated request and rebuild it;
   do not retry with a different request under the old digest.

The digest does not commit to the split percentages or other contract
configuration. The contract reads and validates that state at execution time.
If an application needs to display the resulting amounts before approval, it
must also refresh the current split configuration.

## Legacy Remittance Split Fingerprint

`distribute_usdc` accepts a `u64` produced by the public
`compute_request_hash` helper:

```text
(
    operation_payload
    + nonce
    + amount_low_64
    + amount_high_64
    + deadline
) * 1_000_000_007 mod 2^64
```

For this entrypoint, `operation` is `symbol_short!("distrib")`.

This fingerprint does **not** commit to:

- `from` (the helper retains a caller argument for API compatibility but does
  not mix it into the value);
- `usdc_contract`;
- any destination in `AccountGroup`; or
- the current split configuration.

Those values still pass authorization, trusted-token, self-transfer, and
configuration checks on-chain. The fingerprint must not be used as evidence
that an off-chain signer approved those uncommitted fields. Prefer
`distribute_usdc_hashed` when field-level commitment is required.

## Orchestrator Signed-Flow Fingerprint

`execute_remittance_flow_signed` recomputes a `u64` fingerprint internally:

```text
(
    operation_payload
    + nonce
    + amount_low_64
    + amount_high_64
    + deadline
    + goal_id
    + bill_id
    + policy_id
) * 1_000_000_007 mod 2^64
```

The operation is the fixed short symbol `"flow"`. The routing IDs are read
from orchestrator instance storage at validation time. Changing any of those
IDs invalidates a previously prepared fingerprint.

The fingerprint does not commit to the executor, `actor_epoch`, or downstream
contract addresses. Those values are authorized or validated independently.
There is currently no public orchestrator hash helper, so integrations using
this path must keep their implementation synchronized with
`Orchestrator::compute_request_hash`. Prefer obtaining the value from a single
trusted backend implementation rather than duplicating the formula across
clients.

On `InvalidNonce`, refresh all of the following before rebuilding:

- the executor's nonce;
- the deadline;
- the remittance amount; and
- `goal_id`, `bill_id`, and `policy_id`.

See [Orchestrator Signed-Flow Request-Hash and Deadline Model](ORCHESTRATOR_SIGNING.md)
for the deadline and replay-protection sequence.

## Snapshot Checksums Are Different

Exported snapshots also contain fields named `checksum`, but these values are
integrity guards rather than authorization commitments:

- `data_migration::ExportSnapshot` uses SHA-256 over
  `version_le || format_utf8 || canonical_payload_json`. Consumers should call
  `ExportSnapshot::verify_checksum()` before import.
- `remittance_split::ExportSnapshot` uses a non-cryptographic `u64` checksum
  over schema version, split percentages, and schedule count.
- `savings_goals::GoalsExportSnapshot` uses a non-cryptographic `u64`
  checksum over schema version, `next_id`, and selected numeric goal fields.

The import paths recompute these checksums before mutating state. They provide
corruption detection, not sender authentication. Do not use them as request
signatures, and do not compare checksum values across different snapshot
types. See [Data Migration Formats](migration-formats.md) for the portable
off-chain snapshot format.

## WASM Hashes

The hash returned by `soroban contract install` identifies installed contract
WASM and is passed to an upgrade entrypoint. It is not one of the request
commitments above. Operators should record and verify it using the
[Contract Upgrade Runbook](UPGRADE_RUNBOOK.md).
