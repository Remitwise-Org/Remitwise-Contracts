# Remittance Split — `verify_snapshot` Integrity Preflight

## Overview

`verify_snapshot(env, snapshot)` is the standalone, read-only preflight an
integrator runs before committing to `import_snapshot`. It runs the same
validation pipeline as `import_snapshot` steps 2–7 — schema-version boundary,
checksum integrity, `initialized` flag, per-field percentage range, percentage
sum, and timestamp sanity — **without** modifying any contract state.

- **No caller / no auth.** `verify_snapshot` takes no `Address` and calls no
  `require_auth`; any address may call it.
- **Side-effect free.** It reads only `env.ledger().timestamp()` and the passed
  `snapshot`; it writes no storage and appends no audit entry.
- **Returns** `Ok(true)` when all checks pass, otherwise an error.

It does **not** check ownership mapping or nonce validity; those require a
caller context and are enforced only by `import_snapshot`.

---

## verify ↔ import relationship

`verify_snapshot` and `import_snapshot` share the same integrity gate, so a
snapshot rejected by the preflight is rejected by the import with the same error
code:

| Mutation | `verify_snapshot` | `import_snapshot` |
|----------|-------------------|-------------------|
| Tampered `checksum` | `ChecksumMismatch` | `ChecksumMismatch` |
| `schema_version` outside `[MIN_SUPPORTED_SCHEMA_VERSION, SCHEMA_VERSION]` | `UnsupportedVersion` | `UnsupportedVersion` |
| `schema_version` in-range but mismatched vs. checksum | `ChecksumMismatch` | `ChecksumMismatch` |
| Percentage change that alters the digest input | `ChecksumMismatch` | `ChecksumMismatch` |
| Schedule **count** change | `ChecksumMismatch` | `ChecksumMismatch` |

`import_snapshot` additionally enforces nonce, ownership, and pause checks that
`verify_snapshot` does not. `require_nonce` is a pure compare (it does not
consume the nonce), so a rejected import leaves replay state untouched.

---

## Checksum coverage — ⚠️ FINDING

`compute_checksum` is a **linear digest** of a small field set:

```text
checksum = (schema_version
            + spending_percent + savings_percent + bills_percent + insurance_percent
            + schedule_count) .wrapping_mul(31)
```

Because the four percentages of any **valid** snapshot always sum to `100`, the
checksum is in practice a function of `(schema_version, schedule_count)` only.
It therefore does **not** bind:

| Field class | Bound by checksum? | Consequence |
|-------------|--------------------|-------------|
| Split distribution (which bucket gets what) | ❌ (only the sum, always 100) | A sum-preserving rebalance (e.g. swap spending/savings) passes verification — funds can be rerouted undetected |
| `config.owner` | ❌ | Owner field is not bound (verify also performs no owner check) |
| `config.timestamp` | ❌ | Any value ≤ current ledger time is accepted |
| `config.usdc_contract` | ❌ | The token address can be substituted — directly undermines the field's stated anti-substitution purpose |
| `exported_at` | ❌ (and never validated) | Any value accepted |
| Schedule **contents** (amount, due, interval, …) | ❌ (only the count) | Schedule bodies can be altered as long as the count is preserved |

This **contradicts the `ExportSnapshot` doc comment**, which claims the checksum
is "an FNV-1a digest covering every scalar field … Any single-bit mutation to
any covered field will produce a different checksum." It is neither FNV-1a nor
full-field-covering.

### Disposition

Per the task's guidance ("document the finding in the PR rather than patching
silently"), this work is **test + documentation only** — the checksum is not
changed. The `finding_*` tests in `remittance_split/src/test.rs` pin the current
(insecure) behaviour as regression anchors.

### Suggested remediation (not applied here)

Replace `compute_checksum` with a real digest (e.g. FNV-1a / a hash) computed
over the serialized bytes of **every** covered field — the full distribution,
`owner`, `timestamp`, `usdc_contract`, `exported_at`, and each schedule's
contents — and bump `SCHEMA_VERSION` so old snapshots are re-checksummed on
re-export. This is a breaking change to the snapshot format and is out of scope
for a test-only task.

---

## Secondary observation — `export_snapshot` panics with no schedules

While building the suite, a separate latent bug surfaced: `export_snapshot`
panics with `Error(Storage, MissingValue)` for an owner who has **never created
a schedule**. `get_remittance_schedules` unconditionally bumps the TTL of the
`OwnerSchedules` persistent key, which does not exist until the first schedule
is created. Net effect: an owner with zero schedules cannot export a snapshot.
This is an availability bug, not a `verify_snapshot` bypass; it is documented
here and worked around in the test helper (which assembles the empty-schedule
snapshot directly, exactly as `export_snapshot` would). Not patched.

---

## Test coverage

Tests live in `remittance_split/src/test.rs` under
`mod verify_snapshot_tamper`. proptest seeds are bounded (`PROPTEST_CASES = 48`),
consistent with the existing fuzz suites.

### Detection (mutations the verifier rejects)

| Test | What it proves |
|------|----------------|
| `prop_checksum_single_bit_flip_rejected` | Positive control passes; flipping any one of the 64 checksum bits → `ChecksumMismatch` |
| `prop_schema_version_mutation_rejected` | Out-of-range version → `UnsupportedVersion`; in-range-but-wrong version → `ChecksumMismatch` |
| `prop_percentage_field_mutation_rejected` | A sum-altering percentage change → `ChecksumMismatch` |
| `prop_schedule_count_mutation_rejected` | Dropping a schedule (count change) → `ChecksumMismatch` |
| `prop_verify_and_import_agree_on_rejection` | A snapshot rejected by `verify_snapshot` is rejected by `import_snapshot` with the same code |
| `test_verify_empty_schedule_list` | Edge: zero schedules verifies; checksum flip still rejected |
| `test_verify_min_supported_schema_version_accepted` | Edge: a consistent snapshot at `MIN_SUPPORTED_SCHEMA_VERSION` verifies |
| `test_verify_max_length_schedule_config` | Edge: `MAX_SCHEDULES_PER_OWNER` schedules verifies; checksum flip still rejected |
| `test_verify_snapshot_is_side_effect_free_and_callerless` | No storage writes (config/nonce/audit unchanged) and works with an empty auth set |

### Finding characterization (mutations the verifier accepts)

| Test | Gap pinned |
|------|------------|
| `finding_sum_preserving_percentage_rebalance_accepted` | Distribution not bound |
| `finding_config_timestamp_tamper_accepted` | `config.timestamp` not bound |
| `finding_usdc_contract_substitution_accepted` | `config.usdc_contract` not bound |
| `finding_config_owner_tamper_accepted_by_verify` | `config.owner` not bound/validated |
| `finding_exported_at_tamper_accepted` | `exported_at` not bound |
| `finding_schedule_content_tamper_accepted` | Schedule contents not bound (only count) |
