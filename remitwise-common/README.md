# Remitwise Common Library

Shared types, constants, and utilities used across all Remitwise Soroban smart contracts.

## Features

- Shared enums: Category, FamilyRole, CoverageType, SupportedToken
- Token registry: SupportedToken, stroop/decimal constants, currency helpers
- Event taxonomy: EventCategory, EventPriority, RemitwiseEvents emitter
- Pagination utilities: clamp_limit
- Storage TTL constants
- Tag canonicalization and validation
- Encoding stability tests

## Quickstart

```rust
use remitwise_common::{
    Category, FamilyRole, EventCategory, EventPriority, RemitwiseEvents,
    canonicalize_tags_checked, TagError, clamp_limit,
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

## Constants

- `DEFAULT_PAGE_LIMIT`: 20
- `MAX_PAGE_LIMIT`: 50
- `MAX_BATCH_SIZE`: 50
- `TAG_MAX_LEN`: 32
- `CONTRACT_VERSION`: 1
- `STROOPS_PER_XLM`: 10_000_000
- `DEFAULT_CURRENCY`: "XLM"
- `MAX_CURRENCY_LEN`: 10

## Utilities

### `clamp_limit(limit)`

Normalizes pagination limits:
- 0 → DEFAULT_PAGE_LIMIT
- 1..=MAX_PAGE_LIMIT → unchanged
- > MAX_PAGE_LIMIT → MAX_PAGE_LIMIT

### `canonicalize_tags_checked(env, tags)`

Validates and canonicalizes tags with error handling.

### `RemitwiseEvents::emit(env, category, priority, action, data)`

Emits a standardized event.

## Running Tests

```bash
cargo test -p remitwise-common
```

