# Remitwise Common Library

**New in 2026.7:** Stable period bucketing for timestamps; use `PeriodKind` and `Timestamp::to_period_key` for reproducible calendar grouping (see below).


Shared types, constants, and utilities used across all Remitwise Soroban smart contracts.

## Features

- Shared types: Category, FamilyRole, CoverageType, SupportedToken, Percent, Rate, PeriodKind
- Period bucketing: Timestamp::to_period_key (day/week/month)
- Token registry: SupportedToken, stroop/decimal constants, currency helpers
- Rate arithmetic & percent conversion: BPS_PER_PERCENT, Percent type, Rate::from_percent
- Event taxonomy: EventCategory, EventPriority, RemitwiseEvents emitter
- Pagination utilities: clamp_limit
- Storage TTL constants
- Tag canonicalisation and validation
- **Symbol canonicalisation:** `canonicalise_symbol`, `canonicalise_symbol_checked`, `canonicalise_symbols` — trim, casefold, charset validation
- Encoding stability tests
- **Required config:** `require_env_var` — read required instance-storage config with a clear `EnvVarError::Missing`

## Quickstart

```rust
use remitwise_common::{
    Category, FamilyRole, EventCategory, EventPriority, RemitwiseEvents,
    canonicalize_tags_checked, TagError, Timestamp, clamp_limit,
    SupportedToken, STROOPS_PER_XLM, DEFAULT_CURRENCY,
};

// Look up token metadata
let xlm = SupportedToken::XLM;
assert_eq!(xlm.decimals(), 7);
assert_eq!(xlm.base_units_per_unit(), STROOPS_PER_XLM);

// Parse a currency code
let token = SupportedToken::from_currency_code("USDC"); // Some(USDC)

// Use the default currency constant
assert_eq!(DEFAULT_CURRENCY, "XLM");

// Normalize a pagination limit
let limit = clamp_limit(100); // becomes 50

// Measure future distance without underflowing
let seconds = Timestamp::seconds_until(1_700_000_000, 1_700_000_300);
assert_eq!(seconds, 300);

// Emit an event
RemitwiseEvents::emit(
    &env,
    EventCategory::Transaction,
    EventPriority::High,
    symbol_short!("paid"),
    (bill_id, amount),
);

// Validate and canonicalize tags
let tags = vec![&env, String::from_str(&env, "Rent"), String::from_str(&env, "Utilities")];
match canonicalize_tags_checked(&env, &tags) {
    Ok(normalized) => { /* use normalized */ },
    Err(TagError::Empty) => { /* handle */ },
    Err(TagError::TooLong) => { /* handle */ },
    Err(TagError::InvalidChar { position }) => { /* handle */ },
}
```

## Types

### PeriodKind
- Enum for selecting period bucket (`Day`, `Week`, `Month`).
- Use with `Timestamp::to_period_key(ts, period)`.

### Period-active guard (`verify_period_active`)

Defence-in-depth helper for absorbing writes into periods that are either future
(`period_start > now`) or already archived (`is_archived == true`). Pure, stateless
— the caller supplies `is_archived` from its own archive tracking. The headline
use site is any `(user, period_key)`-partitioned write entry point (e.g.
`reporting::store_report`, any future bill/insurance/family-wallet write that
keys by period). Closes the `Closes #1234` threat model: pre-loading future
buckets and resurrecting sealed archive periods. Returns
`Err(PeriodKeyError::PeriodNotActive)` on either failure mode; surface this
through a contract-specific `#[contracterror]`.

```rust,ignore
use remitwise_common::verify_period_active;

let now = env.ledger().timestamp();
let is_archived = my_archive_map_contains(&env, pk);
verify_period_active(period_start, now, is_archived)
    .unwrap_or_else(|_| panic_with_error!(&env, MyError::PeriodNotActive));
```

### Ledger-sequence monotonicity (`require_ledger_seq_monotonic`)

Defence-in-depth helper for rejecting ledger-sequence regressions (`env.ledger().sequence() < prev`)
at the entry point it matters most. Reads the authoritative current sequence
from the host and compares it to a previously observed baseline. Returns
`Err(LedgerError::LedgerSequenceRegression)` on regression; tolerates equal
and monotonic-progression cases. Closes the `Closes #1240` threat model:
off-by-N replay of signed operations, stale-storage baseline after upgrade,
and `u32` cast underflow.

```rust,ignore
use remitwise_common::require_ledger_seq_monotonic;

require_ledger_seq_monotonic(&env, prev_seq_baseline)
    .unwrap_or_else(|_| panic_with_error!(&env, MyError::LedgerRegression));
```

See [`docs/PERIOD_INVARIANTS.md`](../docs/PERIOD_INVARIANTS.md) and
[`docs/LEDGER_MONOTONICITY.md`](../docs/LEDGER_MONOTONICITY.md) for the
full specifications and recommended call-site patterns.

### Required config (`require_env_var`)

Generic helper for reading a **required** per-contract configuration value from
instance storage. Returns `Err(EnvVarError::Missing)` when the key is absent
instead of silently defaulting. Accepts any Soroban-storable type
(`bool`, `u32`, `i128`, `Address`, …). Closes `#1143`.

```rust,ignore
use remitwise_common::{require_env_var, EnvVarError};
use soroban_sdk::{symbol_short, panic_with_error};

let max: i128 = require_env_var(&env, &symbol_short!("MAX_AMNT"))
    .unwrap_or_else(|e| panic_with_error!(&env, e));
```

See [`docs/require-env-var.md`](../docs/require-env-var.md) for the full
API, usage patterns, and test coverage.

### Category

Financial categories for remittance allocation:
- Spending
- Savings
- Bills
- Insurance

### SupportedToken

Every token the Remitwise platform recognises. Adding a variant forces all
consumers to handle it via exhaustive match.

- XLM (7 decimals, stroops)
- USDC (6 decimals)
- EURC (7 decimals)

See `docs/token-registry.md` for the full registry documentation.

### FamilyRole

Access control roles:
- Owner
- Admin
- Member
- Viewer

### CoverageType

Insurance coverage types:
- Health
- Life
- Property
- Auto
- Liability

### Percent & Rate

Type-safe percentage and basis-points arithmetic:
- `Percent`: Whole percentage newtype (`Percent::from_percentage(5)` for 5%)
- `Rate`: Basis-points newtype (`10_000` bps = 100%) with `Rate::from_percent` and `Rate::apply_to`

See `docs/type-safe-percent-conversion.md` for complete documentation.

## Constants

- `DEFAULT_PAGE_LIMIT`: 20
- `MAX_PAGE_LIMIT`: 50
- `MAX_BATCH_SIZE`: 50
- `TAG_MAX_LEN`: 32
- `CONTRACT_VERSION`: 1
- `STROOPS_PER_XLM`: 10_000_000
- `DEFAULT_CURRENCY`: "XLM"
- `MAX_CURRENCY_LEN`: 10
- `BASIS_POINTS`: 10_000
- `BPS_PER_PERCENT`: 100
- `BASIS_POINTS_PER_PERCENT`: 100
- `SECONDS_PER_DAY`: 86400
- `SECONDS_PER_WEEK`: 604800

## Utilities

### `same_address(a, b)`

Compares two Soroban `Address` values by reference without requiring the
caller to clone either address:

```rust
if same_address(&stored_owner, &caller) {
    // addresses match — no clone needed
}
```

`Address` does not implement `Copy`, so a direct `==` comparison would
normally require two owned values (consuming both, or cloning one). This
helper accepts both addresses by shared reference and delegates to the
host-native equality check, keeping call-site code clean.

The helper does not normalise, modify, or consume either address.

### `clamp_limit(limit)`

Normalizes pagination limits:
- 0 → DEFAULT_PAGE_LIMIT
- 1..=MAX_PAGE_LIMIT → unchanged
- > MAX_PAGE_LIMIT → MAX_PAGE_LIMIT

### `require_matching_settlement_currency_with_config(inv, sym, config)`

Validates settlement currencies against the invoice whitelist while enforcing a configurable cap:
- The default config allows up to 10 currencies.
- Oversized whitelists return `SettlementCurrencyError::WhitelistTooLarge`.
- A matching currency still succeeds when the whitelist stays within the configured cap.

### `Timestamp::seconds_until(now, target)`

Computes the future distance to `target` with saturating semantics:
- `target > now` → returns `target - now`
- `target == now` → returns `0`
- `target < now` → returns `0`

### `canonicalize_tags_checked(env, tags)`

Validates and canonicalizes tags with error handling.

### Symbol canonicalisation

Three functions in `remitwise-common` normalise caller-supplied strings into Soroban `Symbol` values. All three apply the same rules: **trim → casefold to lowercase → charset validation (`[a-z0-9_]`).**

| Function | Behaviour on invalid input | Use when |
|---|---|---|
| `canonicalise_symbol(env, s)` | panics with a descriptive message | Internal/trusted call sites |
| `canonicalise_symbol_checked(env, s)` | `Err(SymbolValidationError)` | Untrusted input, need typed error |
| `canonicalise_symbols(env, vec)` | `Err(SymbolValidationError)` on first bad element | Batch / list entry points |

```rust
use remitwise_common::{canonicalise_symbol_checked, SymbolValidationError};

match canonicalise_symbol_checked(&env, &raw_key) {
    Ok(sym)  => { /* store/compare sym */ },
    Err(SymbolValidationError::Empty)              => { /* reject empty */ },
    Err(SymbolValidationError::TooLong)            => { /* reject too long */ },
    Err(SymbolValidationError::InvalidChar { position }) => { /* reject bad char */ },
}
```

See [`docs/CANONICALISATION.md §5`](../docs/CANONICALISATION.md) for the full specification.

### `RemitwiseEvents::emit(env, category, priority, action, data)`

Emits a standardized event.

### `emit_audit(env, op, actor, meta)` — shared audit-event helper (#1268)

Emits a compliance-level audit event with a canonical schema, ensuring all
contracts produce audit events that indexers and compliance tools can subscribe
to with a single filter.

**Topic schema** (4-element tuple, always identical):

```text
("Remitwise", 5 /*Compliance*/, 2 /*High*/, "audit")
```

**Arguments:**

| Param | Type | Description |
|-------|------|-------------|
| `env`   | `&Env`    | Soroban environment |
| `op`    | `Symbol`  | Short symbol identifying the operation (e.g. `symbol_short!("flow_exec")`) |
| `actor` | `&Address`| Principal that triggered the operation |
| `meta`  | `T: IntoVal` | Compact operation-specific payload (amount, result, IDs…) |

**Constraints:**
- `meta` must serialise to ≤ 256 XDR bytes (enforced by a test-build panic).
- `op` must be a short symbol (≤ 9 bytes).

**Example:**

```rust
use remitwise_common::emit_audit;
use soroban_sdk::symbol_short;

// Emit an audit event for a completed settlement:
emit_audit(&env, symbol_short!("settle"), &caller, (amount, true));

// Emit an audit event for an access check:
emit_audit(&env, symbol_short!("access"), &member, role_discriminant);
```

**Migration note:** Contracts that previously rolled inline audit publish calls
can be migrated to `emit_audit` without changing the storage layout — the helper
only emits an event, it does not write to storage.

## Running Tests

```bash
cargo test -p remitwise-common
```
