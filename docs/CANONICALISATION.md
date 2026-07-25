# String and Bytes Canonicalisation

> **Audience:** contributors adding or reviewing input-handling code.
>
> This document describes every place in the workspace where a raw
> caller-supplied string or byte sequence is transformed into a
> normalised form before storage or comparison. It is the authoritative
> reference for "what does the contract do with my input?" questions.

---

## Table of contents

1. [Tags — casefold and charset validation](#1-tags--casefold-and-charset-validation)
2. [Currency codes — trim, uppercase, and default](#2-currency-codes--trim-uppercase-and-default)
3. [External references — charset validation (case-preserving)](#3-external-references--charset-validation-case-preserving)
4. [Migration snapshot checksum — byte-order and input ordering](#4-migration-snapshot-checksum--byte-order-and-input-ordering)
5. [What is explicitly *not* normalised](#5-what-is-explicitly-not-normalised)

---

## 1. Tags — casefold and charset validation

**Where:** `remitwise-common/src/lib.rs` — `canonicalize_tags()`  
**Called by:** `bill_payments` (`validate_and_normalize_tags`) and `savings_goals` (`validate_and_normalize_tags`).

### Rules

| Step | Behaviour |
|------|-----------|
| Empty batch | `panic!("Tags cannot be empty")` — callers must supply at least one tag. |
| Length per tag | Must be `1..=32` bytes (`TAG_MAX_LEN = 32`). Zero or >32 → `panic!("Tag must be between 1 and 32 characters")`. |
| ASCII uppercase `A-Z` | Silently folded to lowercase (`byte += b'a' - b'A'`). |
| Allowed charset after folding | `[a-z0-9\-_]`. Any other byte invokes the caller-supplied `on_invalid_char` closure (contracts pass `panic_with_error!(env, InvalidTagContent)`). |
| Output order | Preserved; the function does **not** deduplicate. `["Travel", "travel"]` → `["travel", "travel"]`. Deduplication is the caller's responsibility. |
| Idempotency | Applying `canonicalize_tags` to already-canonical tags is a no-op. |

### Example

```rust
// In bill_payments or savings_goals (illustrative — not for direct use)
let raw: Vec<String> = vec!["Travel", "FIRE", "long-term"];
// After canonicalize_tags: ["travel", "fire", "long-term"]
```

The `TagIndex` storage key in `savings_goals` is keyed on `(Address, canonicalized_tag)`, so
lookups must pass the tag through `canonicalize_tags` before querying storage.

### What triggers `on_invalid_char`

| Input | Reason |
|-------|--------|
| `"my goal"` | ASCII space (`0x20`) |
| `"user@domain"` | `@` |
| `"goal.2025"` | `.` |
| `"#savings"` | `#` |
| `"urgent!"` | `!` |

---

## 2. Currency codes — trim, uppercase, and default

**Where:** `bill_payments/src/lib.rs` — `validate_and_normalize_currency()`  
**Used in:** `create_bill` (strict path). `normalize_currency` is a legacy wrapper that silently falls back to `"XLM"` on error; new code should call `validate_and_normalize_currency` directly.

### Rules

| Input | Output | Error? |
|-------|--------|--------|
| `""` (empty) | `"XLM"` | No — empty defaults to XLM |
| `"   "` (spaces only) | `"XLM"` | No — whitespace-only defaults to XLM |
| `"xlm"` | `"XLM"` | No |
| `"UsDc"` | `"USDC"` | No |
| `"  XLM  "` | `"XLM"` | No — leading/trailing ASCII spaces stripped |
| Length > `MAX_CURRENCY_LEN` (10) | — | `InvalidCurrency` |
| Any non-alphabetic character after trim (digit, symbol, space mid-string) | — | `InvalidCurrency` |

### Transformation steps (in order)

1. **Empty check** — zero-length string → return `"XLM"`.
2. **Length check** — reject if `len > 10`.
3. **Trim** — strip leading and trailing ASCII space bytes (`0x20`). If nothing remains → return `"XLM"`.
4. **Charset check** — every byte in the trimmed slice must satisfy `b.is_ascii_alphabetic()` (`A-Z` or `a-z`). Anything else → `Err(InvalidCurrency)`.
5. **Uppercase** — each byte receives `b.to_ascii_uppercase()`.

### Example

```rust
// bill_payments contract — internal helper (Soroban env required)
// Input:  "  ngn  "
// After trim:   "ngn"
// After charset check: ok (all alpha)
// After uppercase: "NGN"
// Stored as: "NGN"
```

---

## 3. External references — charset validation (case-preserving)

External references (`external_ref`) link on-chain entities to off-chain records in
billing or insurance systems. They are **not** normalised for case; the exact
caller-supplied casing is stored and compared verbatim.

### Bill Payments — `validate_external_ref`

**Where:** `bill_payments/src/lib.rs`

| Rule | Value |
|------|-------|
| Minimum length | `1` byte (`MIN_EXTERNAL_REF_LEN`) |
| Maximum length | `64` bytes (`MAX_EXTERNAL_REF_LEN`) |
| Allowed bytes | ASCII alphanumeric, `-`, `_`, `.`, `:` |
| Case treatment | **Preserved as-is** ("for reconciliation fidelity") |
| Uniqueness | Per-owner index enforces no two bills owned by the same address share the same `external_ref`. |

```rust
// Accepted:  "BILL-EXT-123", "inv_2025:001", "A.B.C"
// Rejected:  "bill ref"  (space), "ref@host"  (@), ""  (empty), <65-byte string>  (too long)
```

### Insurance — `validate_ext_ref`

**Where:** `insurance/src/lib.rs`

| Rule | Value |
|------|-------|
| Maximum length | `MAX_EXT_REF_LEN` (constant defined in `insurance/src/lib.rs`) |
| Empty string | Rejected (`InvalidExternalRef`) |
| Case treatment | Preserved as-is |

> Note: insurance `external_ref` accepts `Option<String>`; `None` is passed through without validation.

---

## 4. Migration snapshot checksum — byte-order and input ordering

**Where:** `data_migration/src/lib.rs` — `ExportSnapshot::checksum_for_parts()`  
**Context:** off-chain tooling only; the `data_migration` crate is not `#![no_std]` and uses `std` + `serde_json`.

### SHA-256 variant (current)

The checksum binds three inputs concatenated in this exact order:

```text
SHA-256(
    version_le_bytes       // u32 schema version as 4 little-endian bytes
    || format_utf8_bytes   // format tag string as raw UTF-8  ("json", "binary", "csv", "encrypted")
    || canonical_payload_json  // serde_json serialisation of the payload (see below)
)
```

Encoding choices:

| Field | Encoding | Rationale |
|-------|----------|-----------|
| `version` | `u32::to_le_bytes()` | Little-endian; consistent across the codebase |
| `format` | UTF-8 bytes of the lowercase label | No length prefix; boundary is implicit because `version` is a fixed 4 bytes |
| `payload` | `serde_json::to_vec` (compact, no pretty-print) | Deterministic across runs on the same data |

### Payload canonicalisation

`canonical_payload_bytes` serialises the payload variant into a JSON object with a single top-level key:

| Variant | JSON shape |
|---------|-----------|
| `RemittanceSplit` | `{ "RemittanceSplit": { … } }` |
| `SavingsGoals` | `{ "SavingsGoals": { … } }` |
| `Generic(BTreeMap<…>)` | `{ "Generic": { … } }` |

The `Generic` variant uses a `BTreeMap`, so object keys are always emitted in
sorted (lexicographic) order by `serde_json`. This guarantees byte-identical
serialisation across independent re-exports of the same snapshot.

### Legacy `Simple` checksum

Older snapshots use a rolling 64-bit wrapping sum over the same three fields
(in the same order and with the same little-endian encoding for `version`),
emitted as a decimal string. The `verify_checksum` function accepts either
algorithm; `ChecksumAlgorithm::Sha256` is the default for new exports.

---

## 5. What is explicitly *not* normalised

| Input | Behaviour |
|-------|-----------|
| Goal names (`savings_goals`) | Stored as-is; no case transformation. |
| Bill names (`bill_payments`) | Stored as-is; no case transformation. |
| Policy names (`insurance`) | Stored as-is; no case transformation. |
| `external_ref` | Case-preserved by design (see §3). |
| Stellar addresses (`Address`) | Validated by the Soroban SDK, not by contract code. |
| Orchestrator `operation` symbol | Compared as a `Symbol` (Soroban value type); no string normalisation. |

---

## Cross-references

- Tag storage key convention — [`docs/storage-key-naming-conventions.md`](storage-key-naming-conventions.md)
- Tagging feature overview — [`TAGGING_FEATURE.md`](../TAGGING_FEATURE.md)
- Currency validation issue summary — [`SC-015_CURRENCY_VALIDATION.md`](../SC-015_CURRENCY_VALIDATION.md)
- Migration binary format stability — [`docs/binary-format-stability.md`](binary-format-stability.md)
- Migration data format — [`docs/migration-formats.md`](migration-formats.md)
