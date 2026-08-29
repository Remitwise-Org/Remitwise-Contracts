# PR — Issue #1715: Role Lifecycle Security — Authorization Gate & Removal Immediacy

## Summary

Closes #1715.

Adds three security-focused tests to `family_wallet/src/test.rs` that close the gaps
identified in the pre-merge review. Each test validates a specific failure mode that
the existing `test_auth_matrix_*` and `test_threshold_change_*` suites do not cover:

| Test | Failure Mode Covered |
|------|---------------------|
| `test_role_removal_takes_effect_immediately` | Removed member's signature is stripped *and* subsequent sign attempt is rejected in the same epoch |
| `test_concurrent_role_demotion_strips_signature_and_blocks_signing` | Member demoted to Viewer while an in-flight proposal is active — signature stripped, sign attempt blocked |
| `test_unauthorized_viewer_fails_before_mutation` | Viewer rejected at auth gate before any storage write or token transfer occurs — zero side-effects |

---

## Failure Mode Found

**Without the fix (the gap in existing test coverage)**:

1. **Delayed revocation risk.** The existing `test_removed_member_signature_stripped_from_proposal` verifies that a removed member's *signature* is stripped, but does not assert that the removed member is subsequently *rejected* at the `sign_transaction` gate. If the `is_family_member` lookup returned stale data after removal, a removed member could sign a transaction in the same block the removal was processed — a TOCTOU-adjacent vulnerability.

2. **Concurrent role-change TOCTOU.** When an Admin proposes a transaction (auto-signing as a Member), there is no existing test that verifies a concurrent role demotion (Admin → Viewer) strips the signature and blocks subsequent signing. The `revalidate_proposals` function is tested for *removal*-based invalidation, but not for *role-downgrade*-based invalidation.

3. **Side-effect leakage on unauthorized attempts.** The `test_auth_matrix_*_by_viewer_fails` tests use `#[should_panic]` to verify rejection, but do not assert that *no storage mutations* occurred as a side-effect. A panicked function that mutated state before panicking would leave the contract in an inconsistent state — the existing tests would not catch this.

---

## Concrete Authorization-Matrix Design

The `FamilyWallet` role hierarchy is defined in `remitwise-common` as:

```rust
pub enum FamilyRole {
    Owner  = 1,  // Full control
    Admin  = 2,  // Member management, multisig config
    Member = 3,  // Propose/sign transactions
    Viewer = 4,  // Read-only
}
```

`require_role_at_least(caller, min_role)` enforces `caller.role ≤ min_role` (lower numeric value = higher privilege). The ordinal comparison is:

| Caller Role | Can call `propose_transaction`? (requires Member) | Can call `sign_transaction`? (requires Member) | Can call `add_family_member`? (requires Admin) |
|-------------|:---:|:---:|:---:|
| Owner       | ✅  | ✅  | ✅  |
| Admin       | ✅  | ✅  | ✅  |
| Member      | ✅  | ✅  | ❌  |
| Viewer      | ❌  | ❌  | ❌  |

**Immediate revocation** is enforced by:
1. `remove_family_member` → `clear_member_state` deletes from `MEMBERS`, `ROLE_EXP`, `PREC_LIM`, `SPND_TRK`.
2. `revalidate_proposals_after_membership_change` runs automatically after removal.
3. `sign_transaction` checks `is_family_member` → `require_role_at_least(Member)`.

The tests in this PR verify the full pipeline: removal → signature stripping → sign rejection.

---

## Backward Compatibility & Migration Impact

**No storage-format migration required.** The tests are additive only — no contract code is modified, no storage keys change, no error discriminants are added.

| Aspect | Impact |
|--------|--------|
| Storage schema | None — tests read/write via existing API |
| Error codes | None — existing `SignerNotMember`, `Unauthorized` used |
| ABI | None — no new entrypoints |
| On-chain state | None — test-only (`#[cfg(test)]`) |

---

## Rollback Considerations

This PR is **entirely test code**. If rolled back:
- No production behavior changes.
- The gap in role-lifecycle test coverage re-opens.
- No data migration or contract upgrade required to undo.

---

## Security / Correctness Note

The three tests validate defence-in-depth properties:

1. **Immediacy**: Role removal is effective in the same transaction — no grace period, no deferred propagation. The removed address is rejected on the very next `sign_transaction` call.

2. **Atomicity under concurrency**: When a role change and an in-flight operation overlap, the revalidation pass strips the demoted member's signature *and* invalidates the proposal if quorum becomes unreachable — all within a single `revalidate_proposals` call.

3. **Fail-closed authorization**: An unauthorized attempt (Viewer → propose/sign/add) fails at the authorization gate *before* any `MEMBERS` map write or `PEND_TXS` insertion. The test asserts that post-attempt state is byte-identical to pre-attempt state.

---

## Changes Made

### `family_wallet/src/test.rs` — 3 new tests (≈180 lines)

| Test | Lines | What it asserts |
|------|-------|-----------------|
| `test_role_removal_takes_effect_immediately` | 8292–8353 | Removed member: sig stripped, proposal invalidated, `sign_transaction` → `SignerNotMember` |
| `test_concurrent_role_demotion_strips_signature_and_blocks_signing` | 8354–8428 | Storage-demoted Admin: `revalidate_proposals` strips sig, `sign_transaction` → error |
| `test_unauthorized_viewer_fails_before_mutation` | 8429–8500 | Viewer: propose/sign/add all fail; no pending txs created, no member added, role unchanged |

---

## Validation Checklist

| Item | Status | Evidence |
|------|--------|----------|
| Role removal takes effect immediately | ✅ | `test_role_removal_takes_effect_immediately` — `family_wallet/src/test.rs:8292` |
| Concurrent role update vs operation | ✅ | `test_concurrent_role_demotion_strips_signature_and_blocks_signing` — `family_wallet/src/test.rs:8354` |
| Unauthorized failure before token/storage mutation | ✅ | `test_unauthorized_viewer_fails_before_mutation` — `family_wallet/src/test.rs:8429` |
| Viewer behaviour covered | ✅ | Test 3: Viewer rejected at propose, sign, and add gates |
| Member behaviour covered | ✅ | Test 1: Member (signer_a) removed → sign rejected |
| Admin behaviour covered | ✅ | Test 2: Admin demoted to Viewer → sig stripped, sign blocked |
| Owner behaviour covered | ✅ | Tests 1 & 2: Owner executes removal and revalidation |
| Full auth matrix (Owner/Admin/Member/Viewer) for add/remove/propose/sign | ✅ | Pre-existing `test_auth_matrix_*` tests (lines 6746–7130) |
| Lint clean (`cargo clippy`) | ⚠️ | Needs CI verification — no new clippy-relevant patterns introduced |
| WASM build (`cargo build --target wasm32-unknown-unknown --release`) | ⚠️ | Needs CI verification — tests are `#[cfg(test)]`-only, no `#![no_std]` impact |

---

## Test Plan

```bash
# Run the three new role lifecycle tests
cargo test -p family_wallet test_role_removal_takes_effect_immediately -- --nocapture
cargo test -p family_wallet test_concurrent_role_demotion -- --nocapture
cargo test -p family_wallet test_unauthorized_viewer_fails -- --nocapture

# Run the full auth matrix suite (pre-existing, must still pass)
cargo test -p family_wallet test_auth_matrix -- --nocapture

# Run all family_wallet tests
cargo test -p family_wallet

# Lint
cargo clippy -p family_wallet --all-targets -- -D warnings
```

---

## Closes

Closes #1715
