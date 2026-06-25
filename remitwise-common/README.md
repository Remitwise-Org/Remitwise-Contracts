# Remitwise Common Library

Shared types, constants, and utilities used across all Remitwise Soroban smart contracts.

## Features

- Shared enums: Category, FamilyRole, CoverageType
- Event taxonomy: EventCategory, EventPriority, RemitwiseEvents emitter
- Pagination utilities: clamp_limit
- Storage TTL constants
- Tag canonicalization and validation
- Encoding stability tests

## Quickstart

```rust
use remitwise_common::{
    Category, FamilyRole, EventCategory, EventPriority, RemitwiseEvents,
    canonicalize_tags_checked, TagError, clamp_limit
};

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

