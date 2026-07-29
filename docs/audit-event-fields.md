# Audit Event Fields

This document describes the fields emitted for every audit event across all
Remitwise contracts. It serves as a single source of truth for contributors
implementing or auditing audit-log behaviour.

## Audience

Contributors. If you are adding a new operation to an existing contract or
porting the audit pattern to a new contract, start here.

---

## Internal Audit Logs

Every contract that maintains an on-chain audit log stores a rotating
`Vec<AuditEntry>` (or equivalent) under a storage key. The core fields are
identical across contracts:

| Field       | Type      | Meaning                                            |
|-------------|-----------|----------------------------------------------------|
| `operation` | `Symbol`  | Short symbol naming the operation (e.g. `"create"`) |
| `caller` / `executor` | `Address` | Identity that invoked the operation     |
| `timestamp` | `u64`     | Ledger timestamp (Unix epoch seconds)               |
| `success`   | `bool`    | Whether the operation completed without error       |

### Contracts and storage keys

| Contract          | Key          | Struct              | Additional fields        | Max entries |
|-------------------|--------------|---------------------|--------------------------|-------------|
| `remittance_split` | `AUDIT`      | `AuditEntry`        | —                        | 100         |
| `savings_goals`    | `AUDIT`      | `AuditEntry`        | —                        | 5           |
| `orchestrator`     | `AUDIT`      | `AuditEntry`        | —                        | 100         |
| `family_wallet`    | `ACC_AUDIT`  | `AccessAuditEntry`  | `target: Option<Address>` | 200         |

### Entrypoint example

Every public function that performs a meaningful state change appends one audit
entry through a private `append_audit` helper. The call site always passes the
operation symbol, the caller address, and the success/failure outcome.

```rust
// remittance_split — success path of initialize_split
Self::append_audit(&env, symbol_short!("init"), &caller, true);

// remittance_split — failure path
Self::append_audit(&env, symbol_short!("init"), &caller, false);
```

### Storage key reference

- `remittance_split` — `src/lib.rs:1677`
- `savings_goals` — `src/lib.rs:1863`
- `orchestrator` — `src/lib.rs:534`
- `family_wallet` — `src/lib.rs:2348`

---

## Rotation behaviour

All audit logs are bounded circular buffers. When the log reaches `MAX_AUDIT_ENTRIES`,
the oldest entry is dropped before the new entry is pushed:

```
Before (at capacity): [0, 1, 2, ..., 98, 99]
After push:            [1, 2, 3, ..., 99, 100]  (entry 0 dropped)
```

### Contract-specific constants

```rust
// remittance_split
const MAX_AUDIT_ENTRIES: u32 = 100;

// savings_goals (intentionally small for test coverage)
const MAX_AUDIT_ENTRIES: u32 = 5;

// orchestrator
const MAX_AUDIT_ENTRIES: u32 = 100;

// family_wallet
const MAX_ACCESS_AUDIT_ENTRIES: u32 = 200;
```

---

## Family Wallet Extension

`family_wallet` stores an additional `target` field:

```rust
pub struct AccessAuditEntry {
    pub operation: Symbol,
    pub caller: Address,
    pub target: Option<Address>,   // <-- extra field
    pub success: bool,
    pub timestamp: u64,
}
```

The `target` records the address *acted upon* (e.g. the member being added or
removed, or the recipient of an emergency transfer). Operations that have no
meaningful target pass `None`.

---

## Operation symbols

Each contract defines its own operation symbols as `symbol_short!` literals.
Below is the full inventory:

### remittance_split

| Symbol       | Call sites                     |
|--------------|--------------------------------|
| `"init"`     | `initialize_split`             |
| `"update"`   | `update_split`                 |
| `"distrib"`  | `distribute_usdc`              |
| `"distH"`    | `distribute_usdc_signed`       |
| `"export"`   | `export_snapshot`              |
| `"import"`   | `import_snapshot`              |

### savings_goals

| Symbol       | Call sites                     |
|--------------|--------------------------------|
| `"add_tags"` | tag add operations             |
| `"rem_tags"` | tag removal operations         |
| `"create"`   | `create_goal`                  |
| `"add"`      | `add_to_goal`                  |
| `"batch_ad"` | `batch_add_to_goal`            |
| `"withdraw"` | `withdraw_from_goal`           |
| `"lock"`     | `lock_goal`                    |
| `"unlock"`   | `unlock_goal`                  |
| `"archive"`  | `archive_completed_goals`      |
| `"restore"`  | `restore_goal`                 |
| `"import"`   | `import_snapshot`              |
| `"timelock"` | timelock operations            |

### orchestrator

| Symbol        | Call sites                    |
|---------------|-------------------------------|
| `"flow_exec"` | `execute_remittance_flow`     |

### family_wallet

| Symbol        | Call sites                     |
|---------------|--------------------------------|
| `"add_mem"`   | `add_family_member`            |
| `"rem_mem"`   | `remove_family_member`         |
| `"em_prop"`   | `propose_emergency_transfer`   |
| `"em_conf"`   | `configure_emergency`          |
| `"em_mode"`   | `set_emergency_mode`           |
| `"em_exec"`   | `execute_emergency_transfer`   |
| `"role_exp"`  | `set_role_expiry`              |
| `"adm_xfr"`   | `set_upgrade_admin`            |

---

## Contracts without audit logs

The following contracts do **not** maintain an internal rotating audit log:

- `bill_payments`
- `insurance`
- `reporting`

These contracts emit state-change events via `env.events().publish()` only.
See [EVENTS.md](../EVENTS.md) for their event schemas.

---

## Testing audit behaviour

Every contract with an audit log has tests that verify:

1. A success path appends an audit entry with `success: true`.
2. A failure path (that does not panic-revert) appends an entry with
   `success: false`.
3. The rotating buffer correctly evicts the oldest entry at capacity.
4. The `get_audit_log` / `get_access_audit` query returns entries in the
   correct order and respects the `from_index` / `limit` parameters.

Run the audit-related tests for a specific contract:

```bash
cargo test -p remittance_split
cargo test -p savings_goals
cargo test -p orchestrator
cargo test -p family_wallet
```

---

## Related documents

- [EVENTS.md](../EVENTS.md) — On-chain event schema (external topics and payloads)
- [STORAGE_LAYOUT.md](../STORAGE_LAYOUT.md) — All on-chain storage keys including `AUDIT` / `ACC_AUDIT`
