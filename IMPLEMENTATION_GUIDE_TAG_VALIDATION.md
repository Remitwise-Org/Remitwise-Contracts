# Bill Payments Tag Validation Implementation Guide - Issue #485

## Overview
This document outlines the implementation of tag validation and canonicalization for the bill_payments contract, consistent with savings_goals.

## Changes Made

### 1. Shared Tag Validation Module (remitwise-common/src/lib.rs)

**Added:**
- `TAG_MIN_LENGTH` constant (1)
- `TAG_MAX_LENGTH` constant (32)
- `tags` module with the following functions:
  - `validate_tag(tag: &String)` - Validates single tag length (1-32 chars)
  - `validate_tags(tags: &Vec<String>)` - Validates tag batch
  - `canonicalize_tag(env: &Env, tag: &String) -> String` - Normalizes tags:
    - Trims leading/trailing whitespace
    - Converts to lowercase
    - Collapses multiple spaces to single space
  - `validate_and_canonicalize(env: &Env, tag: &String) -> String` - Combined validation + canonicalization
  - `validate_and_canonicalize_tags(env: &Env, tags: &Vec<String>) -> Vec<String>` - Batch operation

### 2. Required Changes to bill_payments/src/lib.rs

#### A. Import the tags module at the top of the file:
```rust
use remitwise_common::tags;
```

#### B. Add tag parameter to create_bill function (around line 559):
Current signature:
```rust
pub fn create_bill(
    env: Env,
    owner: Address,
    name: String,
    amount: i128,
    due_date: u64,
    recurring: bool,
    frequency_days: u32,
    external_ref: Option<String>,
    currency: String,
) -> Result<u32, Error>
```

New signature (add tags parameter):
```rust
pub fn create_bill(
    env: Env,
    owner: Address,
    name: String,
    amount: i128,
    due_date: u64,
    recurring: bool,
    frequency_days: u32,
    external_ref: Option<String>,
    currency: String,
    tags: Option<Vec<String>>,  // NEW PARAMETER
) -> Result<u32, Error>
```

In the function body, replace line 617:
```rust
// OLD:
tags: Vec::new(&env),

// NEW:
tags: if let Some(t) = tags {
    tags::validate_and_canonicalize_tags(&env, &t)
} else {
    Vec::new(&env)
},
```

#### C. Add Tag Management Functions

Insert these functions after the "Remaining operations" section (around line 1443):

```rust
// -----------------------------------------------------------------------
// Tag management
// -----------------------------------------------------------------------

/// Adds tags to a bill's metadata.
///
/// # Security
/// - `caller` must authorize the invocation
/// - Only the bill owner can add tags
///
/// # Arguments
/// * `caller` - Address of the caller (must be bill owner)
/// * `bill_id` - ID of the bill
/// * `tags` - Tag list to append (validated and canonicalized)
///
/// # Errors
/// * `BillNotFound` - If bill_id doesn't exist
/// * `Unauthorized` - If caller is not the bill owner
/// * `InvalidTag` - If any tag fails validation
/// * `EmptyTags` - If tags list is empty
///
/// # Panics
/// Panics if tags validation fails
pub fn add_tags_to_bill(env: Env, caller: Address, bill_id: u32, tags: Vec<String>) -> Result<(), Error> {
    caller.require_auth();
    Self::extend_instance_ttl(&env);

    // Validate and canonicalize tags
    let canonical_tags = tags::validate_and_canonicalize_tags(&env, &tags);

    let mut bills: Map<u32, Bill> = env
        .storage()
        .instance()
        .get(&symbol_short!("BILLS"))
        .unwrap_or_else(|| Map::new(&env));

    let mut bill = bills.get(bill_id).ok_or(Error::BillNotFound)?;

    if bill.owner != caller {
        panic!("Only the bill owner can add tags");
    }

    // Append tags
    for tag in canonical_tags.iter() {
        bill.tags.push_back(tag);
    }

    bills.set(bill_id, bill);
    env.storage()
        .instance()
        .set(&symbol_short!("BILLS"), &bills);

    // Emit event
    RemitwiseEvents::emit(
        &env,
        EventCategory::State,
        EventPriority::Medium,
        symbol_short!("tags_add"),
        (bill_id, caller.clone(), tags),
    );

    Ok(())
}

/// Removes tags from a bill's metadata.
///
/// # Security
/// - `caller` must authorize the invocation
/// - Only the bill owner can remove tags
///
/// # Arguments
/// * `caller` - Address of the caller (must be bill owner)
/// * `bill_id` - ID of the bill
/// * `tags` - Tag list to remove (validated and canonicalized)
///
/// # Errors
/// * `BillNotFound` - If bill_id doesn't exist
/// * `Unauthorized` - If caller is not the bill owner
/// * `InvalidTag` - If any tag fails validation
/// * `EmptyTags` - If tags list is empty
///
/// # Notes
/// - Removing a tag that is not present is a no-op
pub fn remove_tags_from_bill(env: Env, caller: Address, bill_id: u32, tags: Vec<String>) -> Result<(), Error> {
    caller.require_auth();
    Self::extend_instance_ttl(&env);

    // Validate and canonicalize tags
    let canonical_tags = tags::validate_and_canonicalize_tags(&env, &tags);

    let mut bills: Map<u32, Bill> = env
        .storage()
        .instance()
        .get(&symbol_short!("BILLS"))
        .unwrap_or_else(|| Map::new(&env));

    let mut bill = bills.get(bill_id).ok_or(Error::BillNotFound)?;

    if bill.owner != caller {
        panic!("Only the bill owner can remove tags");
    }

    // Remove matching tags
    let mut new_tags = Vec::new(&env);
    for existing_tag in bill.tags.iter() {
        let mut should_keep = true;
        for remove_tag in canonical_tags.iter() {
            if existing_tag == remove_tag {
                should_keep = false;
                break;
            }
        }
        if should_keep {
            new_tags.push_back(existing_tag);
        }
    }

    bill.tags = new_tags;
    bills.set(bill_id, bill);
    env.storage()
        .instance()
        .set(&symbol_short!("BILLS"), &bills);

    // Emit event
    RemitwiseEvents::emit(
        &env,
        EventCategory::State,
        EventPriority::Medium,
        symbol_short!("tags_rem"),
        (bill_id, caller.clone(), tags),
    );

    Ok(())
}
```

### 3. Test Requirements (bill_payments/src/test.rs or tests/)

Create comprehensive tests covering:

#### A. Tag Validation Tests
```rust
#[test]
fn test_create_bill_with_valid_tags() { ... }

#[test]
fn test_create_bill_with_empty_tags_list() { ... }

#[test]
#[should_panic(expected = "Tag must be at least 1 character")]
fn test_create_bill_with_empty_tag() { ... }

#[test]
#[should_panic(expected = "Tag must be at most 32 characters")]
fn test_create_bill_with_oversized_tag() { ... }

#[test]
fn test_create_bill_tag_canonicalization_lowercase() { ... }

#[test]
fn test_create_bill_tag_canonicalization_trim_whitespace() { ... }

#[test]
fn test_create_bill_tag_canonicalization_collapse_spaces() { ... }
```

#### B. Add Tags Tests
```rust
#[test]
fn test_add_tags_to_bill_success() { ... }

#[test]
fn test_add_tags_to_bill_unauthorized() { ... }

#[test]
fn test_add_tags_to_bill_not_found() { ... }

#[test]
fn test_add_tags_to_bill_empty_list() { ... }

#[test]
fn test_add_tags_to_bill_invalid_tag() { ... }

#[test]
fn test_add_tags_preserves_duplicates() { ... }
```

#### C. Remove Tags Tests
```rust
#[test]
fn test_remove_tags_from_bill_success() { ... }

#[test]
fn test_remove_tags_from_bill_unauthorized() { ... }

#[test]
fn test_remove_tags_from_bill_not_found() { ... }

#[test]
fn test_remove_tags_from_bill_empty_list() { ... }

#[test]
fn test_remove_tags_nonexistent_is_noop() { ... }

#[test]
fn test_remove_tags_removes_all_occurrences() { ... }
```

#### D. Edge Cases
```rust
#[test]
fn test_tag_with_only_whitespace_rejected() { ... }

#[test]
fn test_tag_exact_min_length() { ... }

#[test]
fn test_tag_exact_max_length() { ... }

#[test]
fn test_tag_special_characters_allowed() { ... }

#[test]
fn test_tags_case_sensitive_after_canonicalization() { ... }
```

### 4. Documentation Updates (bill_payments/README.md)

Add section after existing tag documentation:

```markdown
## Tag Validation and Canonicalization

### Validation Rules
- Tags must be between 1 and 32 characters (inclusive)
- Tags cannot be empty
- Empty tag lists are rejected for add/remove operations

### Canonicalization
All tags are automatically normalized before storage:
1. Leading and trailing whitespace is trimmed
2. Converted to lowercase for consistency
3. Multiple consecutive spaces are collapsed to single space

### Examples
- `"  Electricity  "` → `"electricity"`
- `"HIGH  PRIORITY"` → `"high priority"`
- `"Urgent"` → `"urgent"`

### Security Benefits
- Prevents malicious tag strings (e.g., extremely long tags)
- Ensures consistent indexing for off-chain search
- Eliminates case-sensitivity issues in tag matching
```

## Implementation Checklist

- [x] Add shared tag validation to remitwise-common
- [ ] Add import statement to bill_payments/src/lib.rs
- [ ] Update create_bill to accept optional tags parameter
- [ ] Implement add_tags_to_bill function
- [ ] Implement remove_tags_from_bill function  
- [ ] Create comprehensive test suite (95%+ coverage)
- [ ] Update README documentation
- [ ] Run cargo test -p bill_payments
- [ ] Verify test coverage meets 95% threshold
- [ ] Review and commit changes

## Time Estimate
- Implementation: 4-6 hours
- Testing: 3-4 hours
- Documentation: 1-2 hours
- Review: 1-2 hours
- **Total: 9-14 hours** (well within 96-hour timeframe)

## Notes
- The implementation follows the exact same pattern as savings_goals for consistency
- Tag validation uses panic for invalid inputs (matching existing pattern)
- Events are emitted for add/remove operations for audit trail
- Canonicalization ensures reliable off-chain indexing
