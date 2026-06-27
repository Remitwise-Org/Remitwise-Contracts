# Security Review Checklist

Audience: **Contributor** — use this checklist when reviewing a PR that modifies
authorization, storage, token transfer, cross-contract calls, or any path marked
`// SECURITY:` in the source. Tick each item only after verifying the behaviour
matches the documented intent.

## When to use this

Apply this checklist to any PR that touches:

- `require_auth()` or `require_auth_for_args()` calls
- Storage reads/writes of user funds or configuration
- Token transfer (`swap`, `xfer`, `clawback`) logic
- Cross-contract invocations (`env.invoke_contract`)
- Pause / unpause or emergency-mode paths
- Role assignments or spending limits
- Event emission containing sensitive data
- Migration or import payload decoding

Small refactors (renaming, comment-only, test-only) that do **not** touch the
above can skip this checklist at the reviewer's discretion.

---

## Checklist

### 1. Authorization

- [ ] Every state-mutating entrypoint calls `require_auth()` (or
      `require_auth_for_args()`) on the expected address.
      See [docs/AUTHORIZATION_MATRIX.md](AUTHORIZATION_MATRIX.md) for the
      per-contract mapping.
- [ ] Owner-only operations reject non-owners with `Unauthorized` (not a
      generic panic).
- [ ] Role-based checks use `require_role_at_least` (not manual role
      comparison) so the role hierarchy is enforced consistently.
- [ ] Expired roles are rejected (see `fw-role-expiry.md` for the
      `family_wallet` pattern).
- [ ] `external_ref` setters are owner-only (no caller-supplied address
      can modify another user's ref).

### 2. Storage safety

- [ ] Persistent storage keys use a unique, namespaced prefix per module
      (see [docs/storage-key-naming-conventions.md](storage-key-naming-conventions.md)).
- [ ] TTL bump calls (`extend_ttl`) are present for every write path and
      use the contract-level constants (`INSTANCE_LIFETIME_THRESHOLD`,
      `ARCHIVE_LIFETIME_THRESHOLD`, etc.).
- [ ] User-supplied keys (e.g. bill IDs, goal IDs) are validated for
      length / format before being used as storage keys.
- [ ] Bulk operations respect `MAX_BATCH_SIZE` (see
      `remitwise-common/src/constants.rs`).
- [ ] Archive / cleanup paths do not leave dangling storage entries that
      can never be pruned.

### 3. Token transfers

- [ ] Amounts are clamped or validated before transfer (no silent truncation
      of `i128` / `u128`).
- [ ] The recipient address is validated (not `Address::ZERO` or a contract
      that will trap).
- [ ] Cross-contract token calls use `require_auth_for_args` with the full
      transfer payload to prevent replay across contexts.
- [ ] Rounding policy is documented and matches the implementation (see
      [docs/remittance-split-rounding-policy.md](remittance-split-rounding-policy.md)).

### 4. Cross-contract calls

- [ ] `env.invoke_contract` results are checked (`.is_err()` or `match`); the
      caller handles failure gracefully (no silent `.unwrap()`).
- [ ] Reentrancy is considered: if contract A calls contract B which calls
      back into A, the code tolerates or prevents re-entrant state mutation
      (see [docs/orchestrator-reentrancy.md](orchestrator-reentrancy.md)).
- [ ] Gas budgets are accounted for: cross-contract calls compute remaining
      gas and do not assume infinite budget.

### 5. Pause & emergency

- [ ] Every pausable entrypoint gates on `require_not_paused()` (or
      equivalent) before any state mutation.
- [ ] Emergency transfers respect the configured rate limit and threshold
      (see [docs/rate-limiting-design.md](rate-limiting-design.md)).
- [ ] Pause-admin is distinct from contract owner; a single compromised key
      cannot permanently freeze funds.

### 6. Events & privacy

- [ ] No personally identifiable information (names, addresses, amounts in
      excess of what the frontend needs) is emitted in event topics.
- [ ] Event payload size stays under 256 bytes (see
      `RemitwiseEvents::emit` size guard).
- [ ] Event topics are short symbols (`Symbol::short`) matching the
      conventions in `README.md` (e.g. `init`, `calc`, `created`).

### 7. Input validation

- [ ] Percentages sum to exactly 100 (remittance split).
- [ ] Date fields are in the future for creation, in the past for payment.
- [ ] Amount fields are positive (> 0).
- [ ] String lengths are bounded (no unbounded `Vec` allocation from user
      input).
- [ ] `external_ref` is optional and validated when present (not blindly
      stored).

### 8. Migration & import safety

- [ ] Replaying a migration payload produces `DuplicateImport` error (not
      silent state overwrite).
- [ ] Encrypted-payload markers are validated strictly (format
      `enc:v1:<base64>`) — see `data_migration` crate tests.
- [ ] Import handlers do not panic on malformed input; they return a
      descriptive error.

### 9. XDR return payload size (Issue #894)

- [ ] Every public contract function that returns a dynamic `Bytes` value calls
      `validate_return_bytes(&result)` from `remitwise-common` before returning.
- [ ] The returned error on oversized payloads is the typed `BytesReturnTooLarge`
      error code — not a generic panic or string message.
- [ ] The `MAX_BYTES_RETURN` constant (4096 bytes, defined in `remitwise-common`)
      is used as the sole limit — no magic numbers inline.
- [ ] The denial-of-service threat being mitigated: without this guard, a
      contract could return an arbitrarily large `Bytes` value, causing excessive
      XDR deserialization work for every downstream consumer of the response.

---

## Example review walkthrough

PR #123 adds a `withdraw_to_any` entrypoint to `family_wallet`.

**Reviewer ticks:**

| Item | Check | Notes |
|------|-------|-------|
| 1.1  | ✅ `caller.require_auth()` called before balance check | line 42 |
| 1.3  | ✅ `require_role_at_least(Member)` gated | line 44 |
| 2.1  | ✅ Key uses `FamilyWallet` prefix + `withdraw` namespace | line 50 |
| 2.2  | ✅ `extend_ttl` called after write | line 55 |
| 3.1  | ✅ `amount` compared against `spending_limit` (saturating) | line 60 |
| 5.1  | ✅ `require_not_paused()` at top of function | line 38 |
| 7.4  | ✅ Recipient address validated: `require!(recipient != Address::ZERO)` | line 39 |

**Result:** Approve.

---

## Related documents

- [THREAT_MODEL.md](../THREAT_MODEL.md) — comprehensive threat model and
  security gaps
- [SECURITY_REVIEW_SUMMARY.md](../SECURITY_REVIEW_SUMMARY.md) — completed
  review deliverables
- [docs/AUTHORIZATION_MATRIX.md](AUTHORIZATION_MATRIX.md) — per-entrypoint
  auth requirements
- [docs/rate-limiting-design.md](rate-limiting-design.md) — rate-limit
  thresholds and design
- [docs/storage-key-naming-conventions.md](storage-key-naming-conventions.md) —
  key format rules
- [docs/orchestrator-reentrancy.md](orchestrator-reentrancy.md) — reentrancy
  mitigations
- [docs/remittance-split-rounding-policy.md](remittance-split-rounding-policy.md) —
  rounding invariants
- [ARCHITECTURE.md](../ARCHITECTURE.md) — operational limits and monitoring
