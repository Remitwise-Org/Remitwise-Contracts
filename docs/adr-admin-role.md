# ADR: Admin Role for Contract-Wide Settings

- Status: Proposed
- Date: 2026-02-23
- Related issue: #98
- Related audit finding: consider an admin role for contract management
- Contracts in scope: `remittance_split`, `savings_goals`, `bill_payments`, `insurance`

## Context

Current contracts are owner-centric: each resource is controlled by its owner address, and there is no contract-wide superuser. This limits incident response and post-deploy governance (pause, safety toggles, global limits, key rotation).

A future admin design must improve operational safety without creating custodial risk.

## Decision

Adopt a **restricted admin role** with explicit powers and explicit prohibitions.

### 1) Who is the admin

- Each contract has a stored `admin` address set at deployment/initialization.
- Recommended default for production: a multi-sig account (not a single EOA).
- For local/dev deployments, deployer may be initial admin.
- Admin may be set to `None` only via explicit revocation flow.

### 2) Admin capabilities (allowed)

Admin actions are contract-level only. Initial scope:

- `pause` / `unpause` state-changing operations.
- Set global safety/config limits (for example `max_goals_per_user`, analogous per-contract caps).
- Toggle emergency features (for example `emergency_withdraw_enabled`) where implemented.
- Trigger migration/upgrade coordination flags/version metadata (no direct fund movement).
- Propose and execute admin rotation.
- Revoke admin role permanently (optional governance end-state).

### 3) Admin boundaries (not allowed)

Admin is **not** a superuser over user assets.

- Cannot spend, transfer, or withdraw user funds.
- Cannot call owner-only fund functions on behalf of users.
- Cannot mutate a user resource’s owner field directly.
- Cannot bypass owner authorization except in explicitly defined emergency flows.
- Cannot perform silent privileged actions: all admin writes must emit auditable events.

If a capability is not explicitly listed as an admin function, it is out of scope.

### 4) Interaction with owner-only functions

- Owner-only functions remain owner-authorized by default.
- If paused:
  - Writes are blocked.
  - Read/query functions remain available.
- Emergency override must be narrow and explicit:
  - Example: force-unlock a goal may be allowed only when `paused == true` and/or `emergency_mode == true`.
  - Override should change only lock state; it must not transfer funds.
  - Owner still executes withdrawal using owner auth.
- Any owner-impacting admin action requires dedicated event emission and rationale in docs.

### 5) Rotation and revocation

Admin changes use two-step governance:

- `propose_admin(new_admin)`
- `accept_admin()` (or `execute_admin_transfer`) after timelock

Policy:

- Minimum timelock: 24-72 hours (recommended: 48h).
- Production recommendation: multi-sig as both current and next admin.
- Emit events for propose/cancel/execute/revoke.
- Revocation is irreversible unless a separate bootstrap mechanism is explicitly designed (not assumed).

### 6) Per-contract vs global admin

Choose **per-contract admins** (recommended).

Rationale:

- Limits blast radius if one admin key is compromised.
- Supports staggered governance maturity across contracts.
- Avoids tight coupling across unrelated contract state.

Operational note:

- The same multi-sig may still be configured as admin on all contracts for convenience, while preserving per-contract isolation.

## Security constraints for implementation

Future implementation must preserve these invariants:

- No admin path can move user funds.
- Owner auth checks remain required for user fund movements.
- Pause/emergency logic is fail-closed for writes and transparent via events.
- Admin actions are enumerable, minimal, and test-covered.

## Consequences

Pros:

- Better incident response and safer governance operations.
- Clear scope boundaries reduce implementation ambiguity and scope creep.

Tradeoffs:

- Introduces privileged role risk, mitigated by multi-sig + timelock + strict capability limits.
- Adds governance and operational complexity.

## Implementation guidance (non-binding)

When this ADR is implemented:

- Add admin feature flags incrementally per contract.
- Keep emergency overrides opt-in and narrowly scoped.
- Ship with explicit tests for every “cannot” rule in this ADR.
- Document event schema so monitoring can alert on admin actions.
