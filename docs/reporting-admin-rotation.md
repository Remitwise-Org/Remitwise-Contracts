# Reporting — Two-Step Admin Rotation

## Overview

The reporting contract's admin controls every dependency address via
`configure_addresses` (the `ContractAddresses` bundle: remittance-split,
savings-goals, bill-payments, insurance, family-wallet). Whoever holds the admin
role can repoint reporting at arbitrary contracts, so the admin handover is a
**security-critical** operation.

Handover uses a **two-step rotation** (propose → accept) so that:

- a fat-fingered or malicious address cannot be installed in a single call, and
- the incoming admin must actively claim the role, proving it controls the key.

This document pins the rotation **state machine** and the negative paths that
make it safe.

---

## State machine

```
              propose_new_admin(admin, new)
   ┌────────────┐   caller == ADMIN    ┌──────────────────────────┐
   │  ADMIN = A │ ───────────────────► │ ADMIN = A, PEND_ADM = new │
   │ (no pend.) │                      │  (rotation in progress)   │
   └────────────┘                      └──────────────────────────┘
        ▲                                   │            │
        │ accept_admin_rotation(new)        │            │ propose_new_admin(admin, new2)
        │ caller == PEND_ADM                │            ▼  (supersede: PEND_ADM = new2)
        │ ADMIN = PEND_ADM; PEND_ADM cleared│      PEND_ADM overwritten
        └───────────────────────────────────┘
```

| Storage key | Meaning |
|-------------|---------|
| `ADMIN` (`Address`) | Current administrator |
| `PEND_ADM` (`Address`, optional) | Address proposed to become admin; absent when no rotation is in progress |

### Transitions

| Function | Guard | Effect |
|----------|-------|--------|
| `propose_new_admin(caller, new_admin)` | `caller.require_auth()` **and** `caller == ADMIN` | Sets `PEND_ADM = new_admin` (overwriting any prior pending proposal) |
| `accept_admin_rotation(caller)` | `caller.require_auth()` **and** `PEND_ADM` exists **and** `caller == PEND_ADM` | Sets `ADMIN = PEND_ADM`, then removes `PEND_ADM` |

---

## Negative paths (all must reject)

| Attempt | Result | Why it matters |
|---------|--------|----------------|
| Non-admin calls `propose_new_admin` | `Unauthorized`; state unchanged | Prevents an attacker from nominating their own successor |
| Anyone other than `PEND_ADM` calls `accept_admin_rotation` (incl. the current admin) | `Unauthorized`; state unchanged | Prevents acceptance hijack |
| A **superseded** proposed address calls `accept_admin_rotation` | `Unauthorized`; state unchanged | A re-proposal invalidates the first; only the latest proposal is valid |
| `accept_admin_rotation` with no pending proposal | `NotAdminProposed`; state unchanged | No silent/replayed acceptance |
| Missing signature on `propose`/`accept` (caller arg matches, but no auth) | Aborts at `require_auth` (`Error(Auth, InvalidAction)`) | The equality check is not the only gate — the key must actually sign |

**Invariant:** none of the rejected paths mutate `ADMIN`. Rotation only ever
completes through a `propose` by the real admin followed by an `accept` from the
exact address that proposal named.

### Supersede semantics

`PEND_ADM` holds a single address. A second `propose_new_admin` overwrites it,
so the previously proposed address can no longer accept. There is no queue and
no multi-candidate state — the latest proposal wins.

### Post-rotation privilege revocation

Because every privileged entry point (`configure_addresses`,
`propose_new_admin`, archive/cleanup, etc.) checks `caller == ADMIN` against the
single `ADMIN` slot, a completed rotation **atomically** revokes the old admin:
it can no longer reconfigure dependencies or start another rotation, while the
new admin gains those powers.

### Self-rotation (proposing the current admin)

Proposing the current admin as the new admin is a harmless no-op: the admin can
accept, remains admin, and `PEND_ADM` is cleared. No privilege is lost.

---

## Test coverage

Tests live in `reporting/src/tests_auth_acl.rs` under the
`// ADMIN ROTATION HARDENING TESTS` section:

| Test | What it verifies |
|------|-----------------|
| `test_admin_rotation_non_admin_cannot_propose` | Non-admin propose → `Unauthorized`; admin + pending unchanged |
| `test_admin_rotation_only_proposed_can_accept` | Interloper and current admin rejected; only `PEND_ADM` completes the rotation |
| `test_admin_rotation_supersede_invalidates_first_proposal` | Re-propose invalidates the first proposed address; only the latest accepts |
| `test_admin_rotation_old_admin_loses_privileges` | Post-rotation old admin cannot `configure_addresses` or `propose`; new admin can |
| `test_admin_rotation_accept_with_no_pending_proposal` | Accept with nothing pending → `NotAdminProposed`; admin unchanged |
| `test_admin_rotation_propose_current_admin_is_safe` | Self-rotation is a no-op; pending cleared; privileges retained |
| `test_admin_rotation_rejected_paths_do_not_change_admin` | Every negative path leaves `ADMIN` intact; a legitimate rotation still succeeds afterward |
| `test_admin_rotation_propose_requires_admin_signature` | `propose_new_admin` aborts at `require_auth` without the admin's signature |
| `test_admin_rotation_accept_requires_proposed_signature` | `accept_admin_rotation` aborts at `require_auth` without the proposed address's signature |

---

## Errors

| Error | Raised when |
|-------|-------------|
| `Unauthorized` | Proposer is not the current admin, or acceptor is not the proposed address |
| `NotAdminProposed` | `accept_admin_rotation` called with no pending proposal |
| `NotInitialized` | `propose_new_admin` called before `init` |

---

## Security finding

None. The two-step rotation enforces both `require_auth` and the
proposer/acceptor identity checks on every path; the negative-path tests above
pass against the current implementation, so no auth gap was found. This work is
test + documentation only.
