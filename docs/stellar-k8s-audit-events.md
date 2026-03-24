# Stellar-specific contract events for Kubernetes auditing

This document describes how Soroban contract events emitted by Remitwise contracts can be normalized and forwarded as **Kubernetes `events.v1.Event`** objects for cluster-level auditing, compliance dashboards, and SIEM pipelines.

## Goals

- Give operators a **stable, versioned mapping** from on-chain Stellar topics and payloads to Kubernetes event fields.
- Support **idempotent ingestion** (same ledger/event sequence must not duplicate conflicting audit rows).
- Keep contract code unchanged here; emission stays in Soroban. This guide is for **indexers, sidecars, and admission/reporting services** running in K8s.

## Event identity

| Source | Use as K8s field |
|--------|------------------|
| Stellar network passphrase / ledger chain id | Annotation `remitwise.stellar/network` |
| Ledger sequence | `series` or annotation `remitwise.stellar/ledger` |
| Transaction hash | Annotation `remitwise.stellar/tx-hash` |
| Contract id (C… strkey) | `regarding.kind=Node`, `regarding.name=<contract-id>` or dedicated `ReportingController` |
| Event index in tx meta | Annotation `remitwise.stellar/event-index` |

Together, `(network, ledger, tx_hash, event_index)` should form a **unique idempotency key** for `kubectl get events` deduplication and for your audit store.

## Recommended Kubernetes `Event` shape

Use `events.k8s.io/v1` `Event` (or core `v1` Event, depending on your controller):

- **`type`**: `Normal` for successful state transitions; `Warning` for failures you surface from simulated/reverted invocations (usually indexer-only).
- **`reason`**: UpperCamel, short — e.g. `RemittanceSplitPaused`, `GoalLocked`, `BillPaid`.
- **`action`**: `RemitwiseContractEmit` (constant for all forwards).
- **`note`**: Human-readable one line; include contract id and primary topic symbols.
- **`reportingController`**: `remitwise-contracts-indexer` (or your deployment name).
- **`reportingInstance`**: Pod name (from downward API).

### Labels (for filtering in Loki / Prometheus / RBAC)

```yaml
labels:
  app.kubernetes.io/name: remitwise-indexer
  remitwise.stellar/contract: remittance_split   # logical name from deployment config
  remitwise.stellar/topic: split                 # first Soroban topic symbol when applicable
```

### Annotations (for traceability)

```yaml
annotations:
  remitwise.stellar/network: "Test SDF Network ; September 2015"
  remitwise.stellar/contract-id: "C…"
  remitwise.stellar/ledger: "12345"
  remitwise.stellar/tx-hash: "…"
```

## Topic → reason mapping (examples)

Contract-level conventions are documented in `EVENTS.md`. Illustrative mappings:

| Soroban topic pattern | Suggested `reason` |
|----------------------|---------------------|
| `(split, paused)` | `RemittanceSplitPaused` |
| `(split, unpaused)` | `RemittanceSplitUnpaused` |
| `(savings, <GoalLocked enum>)` | `SavingsGoalLocked` |
| `(Remitwise, crt_bill)` | `BillCreated` |

The indexer should maintain a **single table** of `(contract_address, topic_vec) → reason` for operations teams to extend without recompiling contracts.

## Security notes

- **Do not** place secrets or PII in Kubernetes event `note` fields; ledger data is already public, but operator policies may restrict labels.
- **Integrity**: prefer emitting K8s events only after RPC responses include successful inclusion in a ledger, not at simulation time, unless clearly marked as `Warning` / `Simulated`.
- **Multi-tenant clusters**: scope `namespace` per environment (e.g. `remitwise-audit-testnet` vs `remitwise-audit-mainnet`).

## Reference implementation path

The TypeScript indexer under `indexer/` can be extended with an optional **Kubernetes sink** that:

1. Parses Soroban events (existing `eventProcessor.ts` pipeline).
2. Builds `events.v1.Event` objects using the rules above.
3. Applies idempotency on `(ledger, tx_hash, index)` before `create`.

See `indexer/README.md` (section *Kubernetes audit forwarding*) for the integration hook point.
