# Family Wallet Contract

Soroban smart contract for managing family member accounts with spending limits and role-based access control.

## Overview

The Family Wallet contract implements financial control mechanisms for families:

- Add and manage family members
- Set individual spending limits
- Implement role-based access control
- Enforce spending compliance

## Key Features

### ✓ Member Management

- Add family members with Stellar addresses
- Assign roles (sender, recipient, admin)
- Set personalized spending limits
- Update member information

### ✓ Spending Controls

- Validate spending against limits
- Role-based permission system
- Real-time limit checking
- Flexible limit adjustment

### ✓ Role-Based Access

**Admin Role**:

- Manage all family members
- Set and modify spending limits
- Change member roles
- Full contract access

**Sender Role**:

- Spend up to assigned limit
- Cannot manage members
- Limited to their transactions

**Recipient Role**:

- Receive transfers
- Cannot initiate spending
- View-only access

## Data Structure

### FamilyMember

```rust
pub struct FamilyMember {
    pub address: Address,       // Stellar address
    pub name: String,           // Member name
    pub spending_limit: i128,   // Limit in stroops
    pub role: String,           // "sender", "recipient", "admin"
}
```

## API Reference

### add_member

Add a new family member.

```rust
pub fn add_member(
    env: Env,
    address: Address,
    name: String,
    spending_limit: i128,
    role: String,
) -> bool
```

**Parameters:**

- `address`: Stellar address
- `name`: Member's name
- `spending_limit`: Daily/monthly limit in stroops
- `role`: "sender", "recipient", or "admin"

**Returns:** `true` on success

**Example:**

```rust
FamilyWallet::add_member(
    env,
    child_address,
    String::from_small_str("Alice"),
    10_000_000,     // 1 USDC daily limit
    String::from_small_str("sender"),
);
```

### get_member

Retrieve a family member.

```rust
pub fn get_member(env: Env, address: Address) -> Option<FamilyMember>
```

**Returns:** `Some(FamilyMember)` if found

### get_all_members

Get all family members.

```rust
pub fn get_all_members(env: Env) -> Vec<FamilyMember>
```

**Returns:** Vector of all members

### update_spending_limit

Modify a member's spending limit.

```rust
pub fn update_spending_limit(
    env: Env,
    address: Address,
    new_limit: i128,
) -> bool
```

**Returns:** `true` if successful

**Example:**

```rust
// Increase child's allowance
FamilyWallet::update_spending_limit(env, child_address, 15_000_000);
```

### check_spending_limit

Validate spending against limit.

```rust
pub fn check_spending_limit(
    env: Env,
    address: Address,
    amount: i128,
) -> bool
```

**Returns:** `true` if within limit

**Example:**

```rust
let transaction_amount = 5_000_000;
if FamilyWallet::check_spending_limit(env, address, transaction_amount) {
    process_transaction(address, transaction_amount);
} else {
    reject_transaction("Over spending limit");
}
```

## Usage Examples

### Setup Family

```rust
// Add parent as admin
FamilyWallet::add_member(
    env,
    parent_address,
    String::from_small_str("Parent"),
    1_000_000_000,  // High limit
    String::from_small_str("admin"),
);

// Add child as sender
FamilyWallet::add_member(
    env,
    child_address,
    String::from_small_str("Alice"),
    10_000_000,     // 1 USDC daily
    String::from_small_str("sender"),
);

// Add grandparent as recipient
FamilyWallet::add_member(
    env,
    grandparent_address,
    String::from_small_str("Grandma"),
    0,              // Limit not used
    String::from_small_str("recipient"),
);
```

### Check Spending Limit

```rust
// Before processing transaction
let amount = 5_000_000;

if FamilyWallet::check_spending_limit(env, user_address, amount) {
    // Execute transfer
    transfer(user_address, recipient, amount);
    println!("Transfer successful");
} else {
    // Reject transfer
    println!("Amount exceeds spending limit");
}
```

### Update Allowance

```rust
// Parent increases child's weekly allowance
let current_member = FamilyWallet::get_member(env, child_address).unwrap();
let new_limit = current_member.spending_limit + 5_000_000;  // Add 0.5 USDC

FamilyWallet::update_spending_limit(env, child_address, new_limit);
```

### Family Financial Review

```rust
let all_members = FamilyWallet::get_all_members(env);

for member in all_members.iter() {
    println!("{}: {} ({} limit)",
        member.name,
        member.role,
        member.spending_limit / 100_000_000  // Convert to USDC
    );
}
```

## Integration Points

### With Remittance Split

- Controls how split amounts can be spent
- Enforces family spending policy
- Prevents overspending on allocated amounts

### With Bill Payments

- Coordinate bill payments with spending limits
- Ensure bills don't exceed allocations
- Family-wide bill management

### With Savings Goals

- Prevent spending of allocated savings
- Enforce savings discipline
- Coordinate with family goals

## Role Specifications

### Admin Role

Permissions:

- [ ] Add new members
- [x] Remove members (via update with zero limit)
- [x] Modify spending limits
- [x] Change member roles
- [x] View all members
- [x] Approve high-value transactions

### Sender Role

Permissions:

- [x] Make purchases
- [x] Check personal limit
- [x] View personal account
- [ ] Add/remove members
- [ ] Modify limits
- [ ] Access others' information

### Recipient Role

Permissions:

- [x] Receive transfers
- [x] View account balance
- [ ] Initiate spending
- [ ] Add/remove members
- [ ] Modify limits
- [ ] Access others' information

## Best Practices

### 1. Parent-Child Setup

```rust
// Parent account with admin role
FamilyWallet::add_member(
    env,
    parent_addr,
    String::from_small_str("Parent"),
    0,  // Admin, no limit
    String::from_small_str("admin"),
);

// Child account with sender role and limit
FamilyWallet::add_member(
    env,
    child_addr,
    String::from_small_str("Child"),
    50_000_000,  // 5 USDC weekly
    String::from_small_str("sender"),
);
```

### 2. Transaction Validation

```rust
// Check limit before every transaction
if !FamilyWallet::check_spending_limit(env, user_addr, amount) {
    return error("Insufficient allowance");
}

// Execute transaction
execute_transfer(user_addr, recipient, amount);
```

### 3. Dynamic Adjustments

```rust
// Monitor spending and adjust limits
let member = FamilyWallet::get_member(env, user_addr)?;
if member.spending_limit < 5_000_000 {
    // Consider increasing limit
    FamilyWallet::update_spending_limit(
        env,
        user_addr,
        10_000_000,  // Double the limit
    );
}
```

## Security Considerations

1. **Role Enforcement**: Only admins can modify members
2. **Limit Enforcement**: Spending always checked against limit
3. **Address Verification**: Stellar addresses verified by runtime
4. **No Privilege Escalation**: Roles can't be self-promoted
5. **Immutable History**: Member records are audit-able

## Testing

```rust
#[test]
fn test_add_member() {
    let env = Env::default();

    let success = FamilyWallet::add_member(
        env,
        address,
        String::from_small_str("Alice"),
        10_000_000,
        String::from_small_str("sender"),
    );
    assert!(success);
}

#[test]
fn test_spending_limit() {
    let env = Env::default();

    // Add member with 10M limit
    FamilyWallet::add_member(env, addr, name, 10_000_000, role);

    // Within limit: true
    assert!(FamilyWallet::check_spending_limit(env, addr, 5_000_000));

    // Over limit: false
    assert!(!FamilyWallet::check_spending_limit(env, addr, 15_000_000));
}
```

## Deployment

### Compile

```bash
cd family_wallet
cargo build --release --target wasm32-unknown-unknown
```

### Deploy

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/family_wallet.wasm \
  --network testnet \
  --identity deployer
```

## Contract Size

- **Unoptimized**: ~90KB
- **Optimized**: ~45KB

## Gas Costs (Estimate)

- Add member: ~4,000 stroops
- Update limit: ~3,000 stroops
- Check limit: ~1,000 stroops
- Query member: ~1,500 stroops

## Error Scenarios

### Add Duplicate Member

```rust
// Adding same address twice
FamilyWallet::add_member(env, addr, "Alice", limit, role);
FamilyWallet::add_member(env, addr, "Alice2", limit, role);
// Second call overwrites first member
```

### Check Limit for Nonexistent Member

```rust
let result = FamilyWallet::check_spending_limit(env, unknown_addr, amount);
// Returns: false (member not found)
```

### Update Nonexistent Member

```rust
let result = FamilyWallet::update_spending_limit(env, unknown_addr, new_limit);
// Returns: false (member not found)
```

## Future Enhancements

- [ ] Multi-signature approval for high-value transfers
- [ ] Transaction history per member
- [ ] Recurring spending allowances (weekly/monthly)
- [ ] Shared spending pools
- [ ] Spending analytics and reports
- [ ] Automatic limit reset on schedule
- [ ] Emergency override for admins

## References

- [Full API Reference](../docs/API_REFERENCE.md#family-wallet-contract)
- [Usage Examples](../docs/USAGE_EXAMPLES.md#family-wallet-examples)
- [Architecture Overview](../docs/ARCHITECTURE.md)
- [Deployment Guide](../docs/DEPLOYMENT_GUIDE.md)

## Support

For issues or questions:

- [Remitwise GitHub](https://github.com/Remitwise-Org/Remitwise-Contracts)
- [Stellar Documentation](https://developers.stellar.org/)
