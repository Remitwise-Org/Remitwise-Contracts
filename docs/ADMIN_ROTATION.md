# Admin Rotation

> **Audience:** Operators rotating a contract's admin key (and reviewers checking whether a delay protects that rotation).
> **Goal:** Say exactly what happens, in what order, when you rotate an admin — and be upfront that despite the name this issue was filed under, there is currently **no time-delay ("timelock") anywhere in this codebase's admin-rotation flow.**

## There is no rotation timelock today

Searching the whole workspace for a propose→wait→accept pattern on a *main contract admin* turns up exactly one implementation: `reporting`'s `propose_new_admin` / `accept_admin_rotation` (`reporting/src/lib.rs`). Reading it end to end: **`accept_admin_rotation` has no timestamp check at all.** The proposed admin can call it in the very next ledger after `propose_new_admin` — there is no minimum wait, no `PROPOSED_AT` storage key, nothing that would block acceptance until some future time.

If you came here expecting a mandatory cooldown between proposing a new admin and that admin taking effect (the usual reason to call something a "timelock" — e.g. so a compromised current-admin key can't be used to instantly hand control to an attacker's address with no window for anyone to notice and react), **it does not exist**. Do not build downstream tooling, monitoring, or an incident-response plan around an admin-rotation delay that isn't there.

Do not confuse this with the *other* time-based admin mechanism in the codebase — the pause-admin grant TTL — described below, which is unrelated.

## `reporting`'s two-step admin rotation (what actually exists)

```rust
// Step 1 — current admin proposes a successor. Immediate; no delay.
client.propose_new_admin(&current_admin, &new_admin_address);

// Step 2 — the proposed address accepts. Can be called in the same
// transaction batch as step 1, or any time after — there is no expiry
// on a pending proposal either.
client.accept_admin_rotation(&new_admin_address);
```

- **Storage:** the pending successor is held under the `PEND_ADM` instance-storage key until accepted (then cleared) or overwritten by a later `propose_new_admin` call. The active admin lives under `ADMIN`.
- **Step 1 checks:** `caller.require_auth()`; caller must equal the stored `ADMIN`; `new_admin` must not equal the current admin (`SameAdmin` error otherwise).
- **Step 2 checks:** `caller.require_auth()`; caller must equal the address stored in `PEND_ADM` (`NotAdminProposed` if nothing is pending, `Unauthorized` if the caller isn't the proposed address).
- **Why two steps at all, without a delay:** this still protects against one real mistake — proposing a typo'd or otherwise-uncontrolled address as the new admin. Since that address must itself sign `accept_admin_rotation`, an admin rotation to an address nobody controls silently fails closed instead of bricking admin access. It does **not** protect against a compromised current-admin key, since the attacker controls both steps and can complete the rotation in one atomic sequence.
- This flow was not previously documented in `reporting/README.md`'s own API reference — that's now cross-linked from here.

## The pause-admin grant TTL — a different mechanism, don't conflate it

`bill_payments` and `family_wallet` have a *time-based expiry* on the **pause admin's grant** (not a rotation delay): `ADMIN_GRANT_TTL` (30 days, `bill_payments/src/lib.rs`). Every pause-related entrypoint calls `require_admin_grant_valid`, which checks that the pause admin was granted (or last refreshed) within the last `ADMIN_GRANT_TTL` seconds — if the grant has gone stale, pause operations fail with `AdminGrantExpired` until the grant is refreshed.

This is the opposite shape of a rotation timelock: it doesn't delay a new admin from taking effect, it *expires* an existing admin's standing authority if nobody refreshes it. It's about limiting how long a dormant grant stays trusted, not about giving reviewers a window to catch a bad rotation.

## Cross-references

- [reporting/README.md](../reporting/README.md) — full API reference for `reporting`; add `propose_new_admin`/`accept_admin_rotation` here if you extend this flow.
- [ACCESS_CONTROL_MATRIX.md](../ACCESS_CONTROL_MATRIX.md) — which roles can call which administrative entrypoints across all contracts.
- [docs/EMERGENCY_SHUTDOWN.md](./EMERGENCY_SHUTDOWN.md) — the pause-admin role that `ADMIN_GRANT_TTL` gates, and how it differs from the main contract admin rotated above.
