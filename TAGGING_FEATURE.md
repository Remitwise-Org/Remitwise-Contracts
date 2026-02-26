# Tagging Feature Documentation

## Overview

The tagging feature allows users to organize and categorize their savings goals, bills, and insurance policies using custom string labels (tags). This enables better grouping, filtering, and analytics across all financial entities.

## Implementation

### Data Model Changes

Tags have been added to the following structs:

1. **Bill** (`bill_payments/src/lib.rs`)
   - Added `tags: Vec<String>` field
   - Tags are preserved when bills are archived
   - Tags are copied to recurring bills

2. **ArchivedBill** (`bill_payments/src/lib.rs`)
   - Added `tags: Vec<String>` field
   - Tags are preserved from the original bill

3. **SavingsGoal** (`savings_goals/src/lib.rs`)
   - Added `tags: Vec<String>` field

4. **InsurancePolicy** (`insurance/src/lib.rs`)
   - Added `tags: Vec<String>` field

### Tag Management Functions

Each contract provides owner-only functions to manage tags:

#### Bill Payments Contract

```rust
pub fn add_tags_to_bill(
    env: Env,
    caller: Address,
    bill_id: u32,
    tags: Vec<String>,
) -> Result<(), Error>

pub fn remove_tags_from_bill(
    env: Env,
    caller: Address,
    bill_id: u32,
    tags: Vec<String>,
) -> Result<(), Error>
```

#### Savings Goals Contract

```rust
pub fn add_tags_to_goal(
    env: Env,
    caller: Address,
    goal_id: u32,
    tags: Vec<String>,
)

pub fn remove_tags_from_goal(
    env: Env,
    caller: Address,
    goal_id: u32,
    tags: Vec<String>,
)
```

#### Insurance Contract

```rust
pub fn add_tags_to_policy(
    env: Env,
    caller: Address,
    policy_id: u32,
    tags: Vec<String>,
)

pub fn remove_tags_from_policy(
    env: Env,
    caller: Address,
    policy_id: u32,
    tags: Vec<String>,
)
```

### Tag Validation

All contracts enforce the following validation rules:

- Tags cannot be empty (at least one tag must be provided)
- Each tag must be between 1 and 32 characters in length
- Tags are case-sensitive
- Duplicate tags are allowed

### Events

Tag operations emit events for tracking and analytics:

#### Bill Payments
- `tags_add`: Emitted when tags are added to a bill
  - Data: `(bill_id, owner, tags)`
- `tags_rem`: Emitted when tags are removed from a bill
  - Data: `(bill_id, owner, tags)`

#### Savings Goals
- `tags_add`: Emitted when tags are added to a goal
  - Data: `(goal_id, owner, tags)`
- `tags_rem`: Emitted when tags are removed from a goal
  - Data: `(goal_id, owner, tags)`

#### Insurance
- `tags_add`: Emitted when tags are added to a policy
  - Data: `(policy_id, owner, tags)`
- `tags_rem`: Emitted when tags are removed from a policy
  - Data: `(policy_id, owner, tags)`

### Authorization

- Only the owner of an entity (bill, goal, or policy) can add or remove tags
- All tag operations require authentication via `caller.require_auth()`
- Unauthorized attempts will result in a panic or error

### Storage

- Tags are stored as part of the entity struct
- Tags are included in all query results (paginated and single-entity queries)
- Tags persist across entity lifecycle (e.g., when bills are archived)
- Tags are copied to recurring bills when a bill is paid

## Usage Examples

### Adding Tags to a Bill

```rust
let tags = vec![
    String::from_str(&env, "utilities"),
    String::from_str(&env, "monthly"),
    String::from_str(&env, "high-priority")
];

client.add_tags_to_bill(&owner, &bill_id, &tags);
```

### Removing Tags from a Savings Goal

```rust
let tags_to_remove = vec![
    String::from_str(&env, "old-tag")
];

client.remove_tags_from_goal(&owner, &goal_id, &tags_to_remove);
```

### Adding Tags to an Insurance Policy

```rust
let tags = vec![
    String::from_str(&env, "health"),
    String::from_str(&env, "family")
];

client.add_tags_to_policy(&owner, &policy_id, &tags);
```

## Integration with Existing Features

### Pagination
Tags are automatically included in all paginated query results:
- `get_unpaid_bills`
- `get_all_bills_for_owner`
- `get_archived_bills`
- `get_active_policies`
- `get_goals`

### Archiving
When bills are archived, their tags are preserved in the `ArchivedBill` struct.

### Recurring Bills
When a recurring bill is paid and a new bill is created, the tags are copied to the new bill.

## Error Handling

### Bill Payments Contract
- `Error::EmptyTags` (12): Returned when trying to add/remove an empty tag list
- `Error::InvalidTag` (13): Returned when a tag is invalid (empty or > 32 characters)
- `Error::Unauthorized` (5): Returned when a non-owner tries to modify tags
- `Error::BillNotFound` (1): Returned when the bill doesn't exist

### Savings Goals & Insurance Contracts
These contracts use panics for error handling:
- "Tags cannot be empty": When trying to add/remove an empty tag list
- "Tag must be between 1 and 32 characters": When a tag is invalid
- "Only the [entity] owner can [add/remove] tags": When unauthorized
- "[Entity] not found": When the entity doesn't exist

## Future Enhancements

Potential improvements for the tagging system:

1. **Tag-based Filtering**: Add query functions to filter entities by tags
2. **Tag Analytics**: Aggregate statistics by tag (e.g., total amount by tag)
3. **Tag Suggestions**: Auto-suggest tags based on entity names or patterns
4. **Tag Limits**: Enforce maximum number of tags per entity
5. **Tag Normalization**: Automatically normalize tags (lowercase, trim whitespace)
6. **Predefined Tags**: Support for system-defined tag categories
