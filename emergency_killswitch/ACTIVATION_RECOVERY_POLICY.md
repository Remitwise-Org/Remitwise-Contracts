# Emergency activation and recovery policy

The killswitch exposes a high-impact control surface. The threshold protocol
in this change is an explicit state machine for activating and recovering one
pause scope. It does not silently change the older admin-only pause methods;
integrators adopting quorum authorization must call `configure_signers`,
`activate`, and `recover` and must pass the returned signer epoch.

Integer overflow, recovery-delay arithmetic, and fail-before-write cap
checks are specified in [docs/killswitch-amount-precision-overflow.md](../docs/killswitch-amount-precision-overflow.md).

## State and epochs

The configured signer list, threshold, and signer epoch are stored together.
Every successful signer-set update increments the epoch. An activation records
the epoch that approved it. A recovery must present both the activation epoch
and the current signer epoch. Rotating the signer set therefore invalidates
recovery approvals collected under the previous policy.

An approval is an address in the configured signer list. The contract rejects
unknown addresses and repeated addresses. Passing the same address twice does
not create two votes. The threshold counts distinct configured signers only.

The epoch is a concurrency guard, not a signature itself. Each signer address
still participates in the Soroban authorization tree when the transaction is
submitted. Off-chain tooling must retain the signer approvals and the epoch in
the incident record.

## Explicit scope

Every activation names one `PauseScope`:

- `Global` blocks all guarded operations;
- `Module(module)` blocks the named module;
- `Function(module, function)` blocks one function in one module.

The scope is stored and emitted with the activation. Recovery clears only the
scope recorded by that activation. A module activation does not accidentally
clear a global pause, and a function activation does not clear other paused
functions. Dependent contracts must consult the corresponding global,
module, or function state before accepting writes.

The activation also records whether that exact scope was already paused. This
prevents recovery from lifting an independent operator pause that existed
before the threshold activation. Recovery removes only the pause introduced by
the activation; pre-existing state is restored unchanged.

The killswitch contract cannot enforce a pause in an unrelated deployment by
itself. Each dependent contract must integrate the state check and should use
the same module and function symbols. Integration tests should verify the
actual write path, not only the killswitch getter.

## Activation invariants

Activation succeeds only when:

1. the supplied epoch equals the configured signer epoch;
2. a signer set exists and the approval list reaches the threshold;
3. every approval is configured and distinct;
4. no prior activation remains active;
5. `now + RECOVERY_DELAY` fits in `u64` (otherwise `Overflow`);
6. the requested function scope is within the bounded paused-function limit;
7. the scope and recovery deadline are written before the event is emitted.

Function-cap and recovery-delay overflow are checked **before** any activation
storage key is written. A function-scope request that would exceed
`MAX_PAUSED_FUNCTIONS` returns `LimitExceeded` with no `ActivationEpoch`,
`RecoveryReadyAt`, or pause-list mutation. Recovery delay uses checked
addition of exactly 3600 seconds; saturating addition is never used, so a
ledger timestamp near `u64::MAX` cannot collapse the delay into an immediate
recovery window.

A second activation is rejected until the first scope has been recovered.

## Recovery invariants

Recovery requires an active marker, the matching current epoch, the same
threshold validation, and a ledger timestamp at or after the stored
`RecoveryReadyAt`. The delay is currently one hour. A caller cannot shorten it
by submitting a different timestamp or by replaying the activation request.

After recovery, the active epoch, scope, and deadline markers are removed in
the same transaction that clears the pause. A repeated recovery returns
`NotActive` and cannot emit another successful recovery event. A new incident
must collect a new quorum and create a new activation marker.

## Failure and retry matrix

| Attempt | Result | Mutable state |
| --- | --- | --- |
| missing signer set | rejected | unchanged |
| wrong signer epoch | rejected | unchanged |
| signer rotation with stale epoch | rejected (`EpochMismatch`) | unchanged |
| duplicate approval | rejected | unchanged |
| unknown approval | rejected | unchanged |
| below threshold | rejected | unchanged |
| second activation | rejected | existing scope retained |
| recovery delay overflow (`now + 3600`) | `Overflow` | unchanged |
| function scope over cap | `LimitExceeded` | unchanged (no activation marker) |
| recovery before delay | rejected | pause retained |
| recovery with stale epoch | rejected | pause retained |
| admin pause during activation | accepted | admin pause preserved on threshold recovery |
| valid recovery | accepted once | scope cleared (unless admin pause active) |

This matrix is intentionally fail-closed. Operators should not retry a stale
epoch automatically; they should reload the signer policy and collect fresh
approvals. A retry after a partial dependent failure is safe only if the
epoch, scope, and active marker still match the incident record.

## Signer-set changes during an incident

Changing the signer set increments the epoch but does not automatically clear
an active pause. This preserves the emergency protection while requiring the
new signer set to approve recovery. If the new policy cannot approve recovery,
the operator must restore a valid signer configuration through the admin
governance process; an old approval bundle must never be reused.

The admin cannot lower the threshold to manufacture a quorum for an already
collected incident without that policy change being visible as a new signer
epoch. Production governance should require independent review for threshold
changes and should record the old and new signer lists.

## Event contract

The protocol emits `signers_set`, `activated`, and `recovered` events under the
`emergency` namespace. Indexers must persist epoch, threshold, scope, and
ledger sequence. They should treat events as an ordered audit stream and
deduplicate by transaction hash.

An `activated` event without the corresponding active getter state after
finality is an operational alert. A `recovered` event must reference an epoch
that was previously active. A version of the indexer that does not understand
a future scope variant must preserve the raw XDR and mark it unknown rather
than treating it as global.

## Threat scenarios

### Replayed activation

The activation marker prevents the same epoch from activating another scope
while the first scope remains active. Recovery clears the marker, and a later
activation must use the current epoch and a fresh transaction authorization.

### Replayed recovery

Recovery deletes the marker and deadline on success. The same request then
fails with `NotActive`. If the signer list changed, the epoch comparison fails
even before approval validation.

### Duplicate signer counting

The approval list is checked against the configured signer list and for
duplicate entries. A caller cannot satisfy a 2-of-3 threshold with one signer
appearing twice.

### Partial downstream pause

The scope is explicit and the activation itself is atomic. A dependent
contract that fails to observe its pause is an integration defect, not a reason
to mark the activation partially successful. The integration test suite should
exercise each dependent write path and report the first non-compliant module.

### Compromised signer

A compromised signer alone cannot activate a threshold requiring additional
distinct signers. If the threshold is met under an incident, governance should
rotate the signer set, which invalidates stale recovery approvals while keeping
the pause active.

## Backward compatibility and rollback

Existing admin-only APIs and storage keys remain available. The new signer
policy is opt-in through explicit configuration, and no default signer set is
invented during initialization. This avoids silently changing the behavior of
deployments that have not completed a governance migration.

To roll back the code, first document whether any threshold activation is
active. A previous version that does not understand the new markers must not
be used to clear a live pause without an incident review. Preserve the signer
epoch and activation events in the deployment record, and verify every
dependent contract after rollback.

## Validation checklist

- [ ] Configure a valid set and confirm the epoch increments.
- [ ] Reject missing, duplicate, unknown, and stale approvals.
- [ ] Activate global, module, and function scopes independently.
- [ ] Reject a second activation before recovery.
- [ ] Reject recovery before the delay.
- [ ] Recover with the current threshold and confirm marker deletion.
- [ ] Rotate signers and reject old-epoch recovery.
- [ ] Retry after a dependent failure without weakening the quorum.
- [ ] Verify events, getters, gas bounds, and no-panic behavior.
- [ ] Exercise every dependent contract's pause check.

This policy keeps signer authorization, epoch freshness, scope identity,
recovery delay, and one-time state transitions explicit and independently
auditable.
